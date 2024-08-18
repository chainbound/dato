use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use alloy::primitives::B256;
use async_trait::async_trait;
use blst::min_pk::{AggregateSignature, PublicKey};
use futures::stream::{FuturesUnordered, StreamExt};
use hashmore::FIFOMap;
use msg::{tcp::Tcp, ReqError, ReqSocket, SubSocket};
use tokio::{
    net::{lookup_host, ToSocketAddrs},
    sync::mpsc::{self, error::TrySendError},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument, trace, warn};

use crate::{
    common::{
        CertifiedLog, CertifiedReadMessageResponse, CertifiedRecord, CertifiedUnavailableMessage,
        ClientError, Log, Message, ReadError, ReadMessageResponse, Record, SubscribeResponse,
        Timestamp, ValidatorIdentity,
    },
    primitives::{bls::verify_signature, Request},
    Namespace, WriteError,
};

use super::ClientSpec;

const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);

const READ_TIMEOUT: Duration = Duration::from_millis(1000);

/// A client that can write and read log records from validators.
#[derive(Default)]
pub struct Client {
    /// Mapping from validator public keys to their IDs.
    validators: HashMap<usize, PublicKey>,
    /// Mapping from validator IDs to their socket addresses and sockets.
    validator_sockets: HashMap<usize, (SocketAddr, ReqSocket<Tcp>)>,
}

impl Client {
    /// Create a new client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Connect to a certain validator at the given address.
    pub async fn connect_validator<A: ToSocketAddrs>(
        &mut self,
        validator: ValidatorIdentity,
        addr: A,
    ) -> Result<(), ReqError> {
        // TODO: add timeout
        let mut socket = ReqSocket::new(Tcp::default());

        let mut addrs = lookup_host(addr).await?;
        let endpoint = addrs.next().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "could not find any valid address",
            )
        })?;

        socket.connect(endpoint).await?;

        self.validators.insert(validator.index, validator.pubkey);
        self.validator_sockets.insert(validator.index, (endpoint, socket));

        Ok(())
    }
}

#[async_trait]
impl ClientSpec for Client {
    #[instrument(skip(self, message))]
    async fn write(
        &self,
        namespace: Namespace,
        message: Message,
    ) -> Result<CertifiedRecord, ClientError> {
        let start = Instant::now();
        let mut responses = FuturesUnordered::new();

        let request = Request::Write { namespace: namespace.clone(), message: message.clone() };
        let serialized_req = request.serialize();

        for (index, (_, socket)) in &self.validator_sockets {
            let cloned_req = serialized_req.clone();
            responses.push(async {
                // Send the request to the validator with a timeout.
                match tokio::time::timeout(WRITE_TIMEOUT, socket.request(cloned_req.into())).await {
                    Ok(Ok(response)) => Some((*index, response)),
                    Ok(Err(e)) => {
                        warn!(error = %e, "Error writing to validator {}", *index);
                        None
                    }
                    Err(e) => {
                        warn!(error = %e, "Timed out writing to validator {}", *index);
                        None
                    }
                }
            });
        }

        // Pre-allocate and set to all zeroes
        let mut timestamps = vec![Timestamp::default(); self.validators.len()];

        let mut quorum_signature: Option<AggregateSignature> = None;
        let mut votes = 0;

        // Iterate over the responses until we have a quorum of valid responses OR we run out of
        // valid responses.
        while let Some(Some((index, bytes))) = responses.next().await {
            trace!("Received response from validator {index}: {bytes:?}");

            let record = match serde_json::from_slice::<Record>(&bytes) {
                Ok(record) => record,
                Err(err) => {
                    warn!(error = ?err, "Error deserializing response from validator {index}");
                    continue;
                }
            };

            let pubkey = self.validators.get(&index).expect("Validator not found");

            if record.message != message {
                warn!("Message mismatch from validator {:?}", index);
                continue;
            }

            let digest = record.digest(&namespace);

            // Verify the BLS signature
            if !verify_signature(&record.signature, pubkey, digest) {
                warn!(?pubkey, "Invalid signature from validator {index}");
                continue;
            }

            trace!("Validated response from validator {index}");

            if let Some(q) = quorum_signature.as_mut() {
                q.add_signature(&record.signature, false).unwrap();
            } else {
                quorum_signature = Some(AggregateSignature::from_signature(&record.signature));
            }

            // Increase the number of votes, and store the timestamp
            votes += 1;
            timestamps[index] = record.timestamp;

            if has_reached_quorum(self.validators.len(), votes) {
                break;
            }
        }

        if !has_reached_quorum(self.validators.len(), votes) {
            return Err(WriteError::NoQuorum { got: votes, needed: self.validators.len() }.into());
        }

        let mut certified_record = CertifiedRecord {
            timestamps,
            message,
            quorum_signature: quorum_signature.expect("Quorum passed"),
        };

        let timestamp: u128 = certified_record.certified_timestamp().into();

        debug!(elapsed = ?start.elapsed(), median_timestamp = timestamp, "Quorum reached");

        Ok(certified_record)
    }

    // TODO: this implementation can be sped up by using a single request to read all messages
    // in the range and then filtering out the certified messages in a single pass.
    #[instrument(skip(self))]
    async fn read_certified(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<CertifiedLog, ClientError> {
        // start by reading all messages in a range
        let log = self.read(namespace.clone(), start, end).await?;

        // for each message in the log, attempt to read the certified message
        let mut certified_log = CertifiedLog::default();
        for record in log.records {
            let msg_id = record.message_digest(&namespace);
            match self.read_message(namespace.clone(), msg_id).await {
                Ok(CertifiedReadMessageResponse::Available(certified_record)) => {
                    certified_log.records.push(certified_record);
                }
                Ok(CertifiedReadMessageResponse::Unavailable(_)) => {
                    // skip unavailable messages
                }
                Err(e) => {
                    warn!(error = %e, "Error reading certified message");
                }
            }
        }

        Ok(certified_log)
    }

    #[instrument(skip(self))]
    async fn read(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Log, ClientError> {
        let start_ts = Instant::now();
        let mut responses = FuturesUnordered::new();

        let request = Request::ReadRange { namespace: namespace.clone(), start, end };
        let serialized_req = request.serialize();

        for (index, (_, socket)) in &self.validator_sockets {
            let cloned_req = serialized_req.clone();
            responses.push(async {
                // Send the request to the validator with a timeout.
                match tokio::time::timeout(READ_TIMEOUT, socket.request(cloned_req.into())).await {
                    Ok(Ok(response)) => Some((*index, response)),
                    Ok(Err(e)) => {
                        warn!(error = %e, "Error reading from validator {}", *index);
                        None
                    }
                    Err(e) => {
                        warn!(error = %e, "Timed out reading from validator {}", *index);
                        None
                    }
                }
            });
        }

        let mut verify_tasks = JoinSet::new();

        while let Some(Some((index, bytes))) = responses.next().await {
            trace!("Received response from validator {index}: {bytes:?}");

            let log = match serde_json::from_slice::<Log>(&bytes) {
                Ok(log) => log,
                Err(err) => {
                    warn!(error = ?err, "Error deserializing response from validator {index}");
                    continue;
                }
            };

            debug!(len = log.len(), "Got log from validator {index}");
            let pubkey = self.validators.get(&index).cloned().expect("Validator not found");
            let namespace = namespace.clone();

            // Verify the BLS signatures
            verify_tasks.spawn(async move {
                let start = Instant::now();

                for record in &log.records {
                    let digest = record.digest(&namespace);

                    if !verify_signature(&record.signature, &pubkey, digest) {
                        warn!(?pubkey, "Invalid signature from validator {index}");
                        return None;
                    }
                }

                debug!(elapsed = ?start.elapsed(), len = log.len(), "Signatures verified for validator {index}");
                Some(log)
            });
        }

        let mut final_log: Option<Log> = None;
        while let Some(Ok(Some(log))) = verify_tasks.join_next().await {
            if let Some(ref mut first) = final_log {
                first.records.extend(log.records);
            } else {
                final_log = Some(log);
            }
        }

        let mut final_log = final_log.unwrap_or_default();
        final_log.records.sort_by_key(|r| r.timestamp);
        debug!(elapsed = ?start_ts.elapsed(), records = final_log.len(), "Read completed");

        Ok(final_log)
    }

    async fn read_message(
        &self,
        namespace: Namespace,
        msg_id: B256,
    ) -> Result<CertifiedReadMessageResponse, ClientError> {
        let start_ts = Instant::now();
        let mut responses = FuturesUnordered::new();

        let request = Request::ReadMessage { namespace: namespace.clone(), msg_id };
        let serialized_req = request.serialize();

        for (index, (_, socket)) in &self.validator_sockets {
            let cloned_req = serialized_req.clone();
            responses.push(async {
                // Send the request to the validator with a timeout.
                match tokio::time::timeout(READ_TIMEOUT, socket.request(cloned_req.into())).await {
                    Ok(Ok(response)) => Some((*index, response)),
                    Ok(Err(e)) => {
                        warn!(error = %e, "Error reading from validator {}", *index);
                        None
                    }
                    Err(e) => {
                        warn!(error = %e, "Timed out reading from validator {}", *index);
                        None
                    }
                }
            });
        }

        // IMPORTANT: Pre-allocate and set to all zeroes
        let mut available_timestamps = vec![Timestamp::default(); self.validators.len()];
        let mut unavailable_timestamps = vec![Timestamp::default(); self.validators.len()];

        let mut available_quorum_signature: Option<AggregateSignature> = None;
        let mut unavailable_quorum_signature: Option<AggregateSignature> = None;

        let mut available_votes = 0;
        let mut unavailable_votes = 0;

        let mut message: Message = Default::default();

        // Iterate over the responses until we have a quorum of valid responses OR we run out of
        // valid responses.
        while let Some(Some((index, bytes))) = responses.next().await {
            trace!("Received response from validator {index}: {bytes:?}");

            let response = match serde_json::from_slice::<ReadMessageResponse>(&bytes) {
                Ok(response) => response,
                Err(err) => {
                    warn!(error = ?err, "Error deserializing response from validator {index}");
                    continue;
                }
            };

            match response {
                ReadMessageResponse::Available(record) => {
                    // Verify message integrity
                    if record.message.digest(&namespace) != msg_id {
                        warn!("Message mismatch from validator {:?}", index);
                        continue;
                    }

                    message = record.message.clone();
                    let pubkey = self.validators.get(&index).expect("Validator not found");

                    let digest = record.digest(&namespace);

                    if !verify_signature(&record.signature, pubkey, digest) {
                        warn!(?pubkey, "Invalid signature from validator {index}");
                        continue;
                    }

                    trace!("Validated response from validator {index}");

                    if let Some(q) = available_quorum_signature.as_mut() {
                        q.add_signature(&record.signature, false).unwrap();
                    } else {
                        available_quorum_signature =
                            Some(AggregateSignature::from_signature(&record.signature));
                    }

                    available_votes += 1;
                    available_timestamps[index] = record.timestamp;
                }
                ReadMessageResponse::Unavailable(unavailable) => {
                    let pubkey = self.validators.get(&index).expect("Validator not found");
                    let digest = unavailable.digest();

                    if !verify_signature(&unavailable.signature, pubkey, digest) {
                        warn!(?pubkey, "Invalid signature from validator {index}");
                        continue;
                    }

                    trace!("Validated unavailable response from validator {index}");

                    if let Some(q) = unavailable_quorum_signature.as_mut() {
                        q.add_signature(&unavailable.signature, false).unwrap();
                    } else {
                        unavailable_quorum_signature =
                            Some(AggregateSignature::from_signature(&unavailable.signature));
                    }

                    unavailable_votes += 1;
                    unavailable_timestamps[index] = unavailable.timestamp;
                }
            }

            if has_reached_quorum(self.validators.len(), available_votes) ||
                has_reached_quorum(self.validators.len(), unavailable_votes) ||
                available_votes + unavailable_votes >= self.validators.len()
            {
                break;
            }
        }

        trace!(
            available_votes,
            unavailable_votes,
            validators = self.validators.len(),
            "Quorum check"
        );

        if has_reached_quorum(self.validators.len(), available_votes) {
            let mut certified_record = CertifiedRecord {
                timestamps: available_timestamps,
                message,
                quorum_signature: available_quorum_signature.expect("Quorum passed"),
            };

            let timestamp: u128 = certified_record.certified_timestamp().into();

            debug!(elapsed = ?start_ts.elapsed(), median_timestamp = timestamp, "Quorum reached");

            Ok(CertifiedReadMessageResponse::Available(certified_record))
        } else if has_reached_quorum(self.validators.len(), unavailable_votes) {
            let mut certified_unavailable_message = CertifiedUnavailableMessage {
                timestamps: unavailable_timestamps,
                msg_id,
                quorum_signature: unavailable_quorum_signature.expect("Quorum passed"),
            };

            let timestamp: u128 = certified_unavailable_message.certified_timestamp().into();

            debug!(elapsed = ?start_ts.elapsed(), median_timestamp = timestamp, "Quorum reached");

            Ok(CertifiedReadMessageResponse::Unavailable(certified_unavailable_message))
        } else {
            Err(ReadError::NoQuorum { available: available_votes, unavailable: unavailable_votes }
                .into())
        }
    }

    #[instrument(skip(self))]
    async fn subscribe(&self, namespace: Namespace) -> Result<ReceiverStream<Record>, ClientError> {
        let mut responses = FuturesUnordered::new();

        let request = Request::Subscribe { namespace: namespace.clone() };
        let serialized_req = request.serialize();

        // request subscription to the selected namespace from all validators
        for (index, (remote_socket_addr, socket)) in &self.validator_sockets {
            let cloned_req = serialized_req.clone();
            responses.push(async {
                // Send the request to the validator with a timeout.
                match tokio::time::timeout(WRITE_TIMEOUT, socket.request(cloned_req.into())).await {
                    Ok(Ok(response)) => Some((*index, *remote_socket_addr, response)),
                    Ok(Err(e)) => {
                        warn!(error = %e, "Error subscribing to validator {}", *index);
                        None
                    }
                    Err(e) => {
                        warn!(error = %e, "Timed out subscribing to validator {}", *index);
                        None
                    }
                }
            });
        }

        let mut validator_publisher_sockets = HashMap::new();

        // collect all publisher socket addresses from validators
        while let Some(Some((index, remote_addr, bytes))) = responses.next().await {
            trace!("Received response from validator {index}: {bytes:?}");

            let sub_response = match serde_json::from_slice::<SubscribeResponse>(&bytes) {
                Ok(response) => response,
                Err(err) => {
                    warn!(error = ?err, "Error deserializing response from validator {index}");
                    continue;
                }
            };

            validator_publisher_sockets.insert((remote_addr.ip(), sub_response.port), index);
        }

        let (record_sub_tx, record_sub_rx) = mpsc::channel(512);

        tokio::spawn(async move {
            let mut sub_socket = SubSocket::new(Tcp::default());

            let topic_string = String::from_utf8_lossy(&namespace).to_string();
            for (pub_socket_addr, _validator_index) in validator_publisher_sockets {
                // TODO: use index to keep track of which validator we're connected to

                if let Err(err) = sub_socket.connect(pub_socket_addr).await {
                    warn!(error = %err, "Failed to connect to validator publisher");
                    return;
                };
                debug!(?pub_socket_addr, "Connected to publisher");

                if let Err(err) = sub_socket.subscribe(topic_string.clone()).await {
                    warn!(error = %err, "Failed to subscribe to namespace");
                    return;
                }

                info!(?pub_socket_addr, "Subscribed to publisher topic");
            }

            while let Some(pub_msg) = sub_socket.next().await {
                trace!(?pub_msg, "Received message from publisher");

                if let Ok(record) = serde_json::from_slice::<Record>(&pub_msg.into_payload()) {
                    // TODO: use map of connected pub sockets to index and index to pubkey
                    // to verify the signature of each incoming message

                    if let Err(err) = record_sub_tx.try_send(record) {
                        match err {
                            TrySendError::Closed(_) => {
                                warn!("API consumer closed subscription, stopping background task");
                                return;
                            }
                            TrySendError::Full(_) => {
                                warn!("API consumer subscription buffer full, dropping message");
                                continue;
                            }
                        }
                    }
                }
            }
        });

        Ok(ReceiverStream::new(record_sub_rx))
    }

    async fn subscribe_certified(
        &self,
        namespace: Namespace,
    ) -> Result<ReceiverStream<CertifiedRecord>, ClientError> {
        // perform a regular subscription to get all records
        let mut record_stream = self.subscribe(namespace.clone()).await?;

        let (certified_record_tx, certified_record_rx) = mpsc::channel(512);
        let validators_count = self.validators.len();

        // spawn a background task to aggregate records into certified records and
        // send them to the consumer stream
        tokio::spawn(async move {
            let mut records_by_id = FIFOMap::<B256, Vec<Record>>::with_capacity(1024);

            while let Some(record) = record_stream.next().await {
                let id = record.message_digest(&namespace);

                // TODO: clean this up with FIFOMap::entry API when available
                let records = if let Some(records) = records_by_id.get_mut(&id) {
                    records.push(record);
                    records
                } else {
                    records_by_id.insert(id, vec![record]);
                    records_by_id.get_mut(&id).unwrap()
                };

                if has_reached_quorum(validators_count, records.len()) {
                    let certified_record = CertifiedRecord::from_records_unchecked(records);
                    if let Err(err) = certified_record_tx.send(certified_record).await {
                        warn!(error = %err, "Failed to send certified record");
                    }
                }
            }
        });

        Ok(ReceiverStream::new(certified_record_rx))
    }
}

/// Function to compute if the quorum has been reached. A quorum is reached when the number of votes
/// is greater than or equal to 2/3 of the total number of validators.
fn has_reached_quorum(total_validators: usize, votes: usize) -> bool {
    if total_validators < 3 {
        // 1 of 1 or 2 of 2 validators == quorum
        return votes == total_validators;
    }

    votes >= 2 * total_validators / 3
}
