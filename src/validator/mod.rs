use std::{net::SocketAddr, time::Duration};

use blst::min_pk::SecretKey as BlsSecretKey;
use bytes::Bytes;
use futures::StreamExt;
use msg::{tcp::Tcp, PubError, RepSocket, Request as MsgRequest};
use tokio::time::sleep;
use tracing::{debug, error};

mod store;
pub use store::{DataStore, InMemoryStore};

use crate::{
    common::{Log, Message, Namespace, Record, Timestamp},
    primitives::{bls::sign_with_prefix, Request},
};

pub trait ValidatorSpec {
    fn write(&mut self, namespace: Namespace, message: Message) -> Record;
    fn read(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log;
}

pub struct Validator<DS: DataStore> {
    store: DS,
    secret_key: BlsSecretKey,
    conn: RepSocket<Tcp>,
    local_addr: Option<SocketAddr>,
}

impl Validator<InMemoryStore> {
    pub async fn new_in_memory(secret_key: BlsSecretKey, port: u16) -> Result<Self, PubError> {
        Self::new(InMemoryStore::with_capacity(4096), secret_key, port).await
    }
}

impl<DS: DataStore + Send + Sync> ValidatorSpec for Validator<DS> {
    fn write(&mut self, namespace: Namespace, message: Message) -> Record {
        let timestamp = Timestamp::now();

        let message_digest = message.digest(timestamp, &namespace);
        let signature = sign_with_prefix(&self.secret_key, message_digest);
        let record = Record { message, timestamp, signature };
        self.store.write_one(namespace, record.clone());

        record
    }

    fn read(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log {
        self.store.read_range(namespace, start, end)
    }
}

impl<DS: DataStore + Send + Sync> Validator<DS> {
    pub async fn new(store: DS, secret_key: BlsSecretKey, port: u16) -> Result<Self, PubError> {
        let mut conn = RepSocket::new(Tcp::default());
        conn.bind(("0.0.0.0", port)).await?;
        let local_addr = conn.local_addr();
        Ok(Self { store, secret_key, conn, local_addr })
    }

    pub async fn run(&mut self) {
        loop {
            while let Some(req) = self.conn.next().await {
                debug!("Received request");
                self.handle_request(req);
            }

            error!("Validator connection unexpectedly closed");
            sleep(Duration::from_millis(1000)).await;
        }
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    fn handle_request(&mut self, req: MsgRequest) {
        let request = match serde_json::from_slice::<Request>(req.msg()) {
            Ok(request) => request,
            Err(err) => {
                error!(?err, "Failed to parse request");
                return;
            }
        };

        match request {
            Request::Write { namespace, message } => {
                debug!(?namespace, "Received write request");
                let record = self.write(namespace, message);
                let Ok(response) = serde_json::to_vec(&record).map(Bytes::from) else {
                    error!("Failed to serialize record");
                    return;
                };

                if let Err(err) = req.respond(response) {
                    error!(?err, "Failed to respond to request");
                }
            }
            Request::Read { namespace, start, end } => {
                debug!(?namespace, "Received read request");
                let log = self.read(namespace, start, end);
                let Ok(response) = serde_json::to_vec(&log) else {
                    error!("Failed to serialize log");
                    return;
                };

                if let Err(err) = req.respond(Bytes::from(response)) {
                    error!(?err, "Failed to respond to request");
                }
            }
            Request::ReadMessage { namespace, message } => {
                // TODO
            }
        }
    }
}
