use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use alloy::primitives::{Bytes, B256};
use async_trait::async_trait;
use blst::min_pk::{AggregateSignature, PublicKey};
use futures::stream::{FuturesUnordered, StreamExt};
use msg::{tcp::Tcp, ReqError, ReqSocket, SubSocket};
use tokio::{
    net::ToSocketAddrs,
    sync::mpsc::{self, error::TrySendError},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, info, instrument, trace, warn};

use crate::{
    common::{
        CertifiedLog, CertifiedReadMessageResponse, CertifiedRecord, CertifiedUnavailableMessage,
        ClientError, Log, Message, ReadError, ReadMessageResponse, Record, SubscribeResponse,
        SubscriptionError, Timestamp, UnavailableMessage, ValidatorIdentity,
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
    /// Mapping from validator IDs to their sockets.
    validator_sockets: HashMap<usize, ReqSocket<Tcp>>,
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
        socket.connect(addr).await?;

        self.validators.insert(validator.index, validator.pubkey);
        self.validator_sockets.insert(validator.index, socket);

        Ok(())
    }

    /// Check if the quorum has been reached. A quorum is reached when the number of votes is
    /// greater than or equal to 2/3 of the total number of validators.
    fn quorum_reached(&self, votes: usize) -> bool {
        if self.validators.len() < 3 {
            // 1 of 1 or 2 of 2 validators == quorum
            return votes == self.validators.len();
        }

        votes >= 2 * self.validators.len() / 3
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

        for (index, socket) in &self.validator_sockets {
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

            if self.quorum_reached(votes) {
                break;
            }
        }

        if !self.quorum_reached(votes) {
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

    #[instrument(skip(self))]
    async fn read_certified(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<CertifiedLog, ClientError> {
        // let mut responses = FuturesUnordered::new();

        // let request = Request::Read { namespace: namespace.clone(), message: message.clone() };
        // let serialized_req = request.serialize();

        // for (index, socket) in &self.validator_sockets {
        //     let cloned_req = serialized_req.clone();
        //     responses.push(async {
        //         // Send the request to the validator with a timeout.
        //         match tokio::time::timeout(WRITE_TIMEOUT, socket.request(cloned_req)).await {
        //             Ok(Ok(response)) => Some((*index, response)),
        //             Ok(Err(e)) => {
        //                 warn!(error = %e, "Error writing to validator {}", *index);
        //                 None
        //             }
        //             Err(e) => {
        //                 warn!(error = %e, "Timed out writing to validator {}", *index);
        //                 None
        //             }
        //         }
        //     });
        // }

        todo!()
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

        for (index, socket) in &self.validator_sockets {
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

        for (index, socket) in &self.validator_sockets {
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

            if self.quorum_reached(available_votes) ||
                self.quorum_reached(unavailable_votes) ||
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

        if self.quorum_reached(available_votes) {
            let mut certified_record = CertifiedRecord {
                timestamps: available_timestamps,
                message,
                quorum_signature: available_quorum_signature.expect("Quorum passed"),
            };

            let timestamp: u128 = certified_record.certified_timestamp().into();

            debug!(elapsed = ?start_ts.elapsed(), median_timestamp = timestamp, "Quorum reached");

            Ok(CertifiedReadMessageResponse::Available(certified_record))
        } else if self.quorum_reached(unavailable_votes) {
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
        let start = Instant::now();
        let mut responses = FuturesUnordered::new();

        let request = Request::Subscribe { namespace: namespace.clone() };
        let serialized_req = request.serialize();

        // request subscription to the selected namespace from all validators
        for (index, socket) in &self.validator_sockets {
            let cloned_req = serialized_req.clone();
            responses.push(async {
                // Send the request to the validator with a timeout.
                match tokio::time::timeout(WRITE_TIMEOUT, socket.request(cloned_req.into())).await {
                    Ok(Ok(response)) => Some((*index, response)),
                    Ok(Err(e)) => {
                        warn!(error = %e, "Error subscribing to validator {}", *index);
                        None
                    }
                    Err(e) => {
                        warn!(error = %e, "Timed out writing to validator {}", *index);
                        None
                    }
                }
            });
        }

        let mut validator_publisher_sockets = HashMap::new();

        // collect all publisher socket addresses from validators
        while let Some(Some((index, bytes))) = responses.next().await {
            trace!("Received response from validator {index}: {bytes:?}");

            let sub_response = match serde_json::from_slice::<SubscribeResponse>(&bytes) {
                Ok(response) => response,
                Err(err) => {
                    warn!(error = ?err, "Error deserializing response from validator {index}");
                    continue;
                }
            };

            validator_publisher_sockets.insert(sub_response.addr, index);
        }

        // now handle new messages to stream to the API consumer
        let (record_sub_tx, record_sub_rx) = mpsc::channel(512);
        let mut sub_socket = SubSocket::new(Tcp::default());

        let topic_string = String::from_utf8_lossy(&namespace).to_string();
        for (pub_socket_addr, validator_index) in validator_publisher_sockets {
            // TODO: use index to keep track of which validator we're connected to

            if let Err(err) = sub_socket.connect(pub_socket_addr).await {
                warn!(error = %err, "Failed to connect to validator publisher");
                return Err(SubscriptionError::FailedToConnect.into());
            }
            if let Err(err) = sub_socket.subscribe(topic_string.clone()).await {
                warn!(error = %err, "Failed to subscribe to namespace");
                return Err(SubscriptionError::FailedToSubscribe.into());
            }
        }

        // handle each subscription in a background task
        let record_sub_tx = record_sub_tx.clone();
        tokio::spawn(async move {
            while let Some(pub_msg) = sub_socket.next().await {
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
}
