use alloy::primitives::{Bytes, B256};
use serde::{Deserialize, Serialize};

use crate::common::{Message, Namespace, Timestamp};

pub mod bls;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Request {
    /// Request to write a message to the log.
    /// Expects a [`crate::Record`] response
    Write { namespace: Namespace, message: Message },

    /// Request to read a range of messages from the log.
    /// Expects a [`crate::Log`] response
    ReadRange { namespace: Namespace, start: Timestamp, end: Timestamp },

    /// Request to read a single message from the log.
    /// Expects a [`crate::Log`] response
    ReadMessage { namespace: Namespace, msg_id: B256 },

    /// Request to subscribe to all messages in a namespace.
    /// Expects a response containing the socket address of the
    /// publisher and an authorization token to use for the subscription.
    Subscribe { namespace: Namespace },
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
