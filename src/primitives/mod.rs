use alloy::primitives::{Bytes, B256};
use serde::{Deserialize, Serialize};

use crate::common::{Message, Namespace, Timestamp};

pub mod bls;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Request {
    Write { namespace: Namespace, message: Message }, // expects a `Record` response
    Read { namespace: Namespace, start: Timestamp, end: Timestamp }, // expects a `Log` response
    ReadMessage { namespace: Namespace, msg_id: B256 }, // expects a `Log` response
}

impl Request {
    pub fn serialize(&self) -> Bytes {
        serde_json::to_vec(&self).unwrap().into()
    }
}

impl From<Bytes> for Message {
    fn from(bytes: Bytes) -> Self {
        Message(bytes)
    }
}
