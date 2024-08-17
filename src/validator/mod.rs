use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use alloy::primitives::B256;
use blst::min_pk::{SecretKey as BlsSecretKey, Signature};
use bytes::Bytes;
use futures::{ready, StreamExt};
use hashbrown::{HashMap, HashSet};
use msg::{tcp::Tcp, PubError, PubSocket, RepSocket, Request as MsgRequest};
use tokio::{sync::mpsc, task::JoinHandle};
use tracing::{debug, error, info};

mod store;
pub use store::{DataStore, InMemoryStore};

mod spec;
pub use spec::ValidatorSpec;

use crate::{
    common::{
        Log, Message, Namespace, ReadMessageResponse, Record, SubscribeResponse, Timestamp,
        UnavailableMessage,
    },
    primitives::{bls::sign_with_prefix, Request},
};

/// A validator instance that writes log records to a data store and
/// communicates with clients over a TCP socket.
///
/// The validator can read and write log records, as well as manage
/// subscriptions to log records in a given namespace by clients.
pub struct Validator<DS: DataStore> {
    /// Underlying data store backend for the validator log records
    store: DS,
    /// Active TCP socket for the validator to receive requests from clients
    /// and send responses back to them
    conn: RepSocket<Tcp>,
    /// BLS secret key for the validator to sign log records with timestamps
    secret_key: BlsSecretKey,
    /// Local address of the validator TCP socket
    local_addr: Option<SocketAddr>,
    /// Set of namespaces that have active subscriptions from clients
    active_subscriptions: HashSet<Namespace>,
    /// Publisher socket for sending messages to all subscribers
    pub_socket: PubSocket<Tcp>,
}

impl Validator<InMemoryStore> {
    pub async fn new_in_memory(secret_key: BlsSecretKey, port: u16) -> Result<Self, PubError> {
        Self::new(InMemoryStore::with_capacity(4096), secret_key, port).await
    }
}

impl<DS: DataStore + 'static> ValidatorSpec for Validator<DS> {
    fn write(&mut self, namespace: Namespace, message: Message) -> Record {
        let timestamp = Timestamp::now();

        let record_digest = message.record_digest(&namespace, timestamp);

        let signature = sign_with_prefix(&self.secret_key, record_digest);
        let record = Record { message, timestamp, signature };
        self.store.write_one(namespace, record.clone());

        record
    }

    fn read_range(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log {
        self.store.read_range(namespace, start, end)
    }

    fn read_message(&self, namespace: Namespace, msg_id: B256) -> ReadMessageResponse {
        let record = self.store.read_message(namespace, msg_id);

        if let Some(record) = record {
            ReadMessageResponse::Available(record)
        } else {
            let unavailable = UnavailableMessage::create_signed(msg_id, &self.secret_key);
            ReadMessageResponse::Unavailable(unavailable)
        }
    }

    fn subscribe(&mut self, namespace: Namespace) {
        self.active_subscriptions.insert(namespace);
    }
}

impl<DS: DataStore + 'static> Validator<DS> {
    /// Creates a new validator instance with the given data store backend,
    /// BLS secret key, and TCP port for the validator to listen on for new requests.
    ///
    /// This method also tries to bind both the request and publisher sockets.
    pub async fn new(store: DS, secret_key: BlsSecretKey, port: u16) -> Result<Self, PubError> {
        let mut conn = RepSocket::new(Tcp::default());
        conn.bind(("0.0.0.0", port)).await?;

        // TODO: add configurable port for publisher socket as well
        let mut pub_socket = PubSocket::new(Tcp::default());
        pub_socket.bind(("0.0.0.0", port + 1)).await?;

        Ok(Self {
            store,
            secret_key,
            local_addr: conn.local_addr(),
            active_subscriptions: HashSet::new(),
            pub_socket,
            conn,
        })
    }

    /// Address of the TCP socket at which the validator is listening for incoming requests.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
}

impl<DS: DataStore + 'static> Future for Validator<DS> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.get_mut();

        let (publisher_queue_tx, mut publisher_queue_rx) = mpsc::channel(512);

        loop {
            // process incoming requests from clients
            if let Poll::Ready(Some(req)) = this.conn.poll_next_unpin(cx) {
                let request = match serde_json::from_slice::<Request>(req.msg()) {
                    Ok(request) => request,
                    Err(err) => {
                        error!(?err, "Failed to parse request");
                        continue;
                    }
                };

                match request {
                    Request::Write { namespace, message } => {
                        debug!(?namespace, "Received write request");
                        let record = this.write(namespace.clone(), message);
                        let Ok(response) = serde_json::to_vec(&record).map(Bytes::from) else {
                            error!("Failed to serialize record");
                            continue;
                        };

                        if let Err(err) = req.respond(response.clone()) {
                            error!(?err, "Failed to respond to write request");
                        }

                        // Send a request to publish the record to the active subscribers
                        if this.active_subscriptions.contains(&namespace) {
                            info!(?namespace, "Sending record to publish queue");
                            if let Err(err) = publisher_queue_tx.try_send((namespace, response)) {
                                error!(?err, "Failed to add record to the publish queue");
                            }
                        }
                    }
                    Request::ReadRange { namespace, start, end } => {
                        debug!(?namespace, "Received read request");
                        let log = this.read_range(namespace, start, end);
                        let Ok(response) = serde_json::to_vec(&log) else {
                            error!("Failed to serialize log");
                            continue;
                        };

                        if let Err(err) = req.respond(Bytes::from(response)) {
                            error!(?err, "Failed to respond to read_range request");
                        }
                    }
                    Request::ReadMessage { namespace, msg_id } => {
                        debug!(?namespace, "Received read message request");
                        let signature = this.read_message(namespace, msg_id);
                        let Ok(response) = serde_json::to_vec(&signature).map(Bytes::from) else {
                            error!("Failed to serialize signature");
                            continue;
                        };

                        if let Err(err) = req.respond(response) {
                            error!(?err, "Failed to respond to read_message request");
                        }
                    }
                    Request::Subscribe { namespace } => {
                        debug!(?namespace, "Received subscribe request");
                        this.subscribe(namespace);

                        let res = SubscribeResponse {
                            port: this.pub_socket.local_addr().expect("Publisher not bound").port(),
                            // TODO: impl auth
                            auth_token: Bytes::from("noop").into(),
                        };

                        let Ok(response) = serde_json::to_vec(&res).map(Bytes::from) else {
                            error!("Failed to serialize subscribe response");
                            continue;
                        };

                        if let Err(err) = req.respond(response) {
                            error!(?err, "Failed to respond to subscribe request");
                        }
                    }
                }

                continue;
            }

            // try to flush any pending messages to publish to active subscribers
            if let Poll::Ready(Some((namespace, serialized_record))) =
                publisher_queue_rx.poll_recv(cx)
            {
                info!(?namespace, "Publishing record to subscribers");
                let topic_string = String::from_utf8_lossy(&namespace).to_string();
                if let Err(err) = this.pub_socket.try_publish(topic_string, serialized_record) {
                    error!(?err, "Failed to publish serialized record to subscriber");
                }

                continue;
            }

            return Poll::Pending
        }
    }
}
