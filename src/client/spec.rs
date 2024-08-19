use alloy::primitives::B256;
use async_trait::async_trait;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    common::{CertifiedReadMessageResponse, ClientError},
    CertifiedLog, CertifiedRecord, Log, Message, Namespace, Record, Timestamp,
};

#[async_trait]
pub trait ClientSpec {
    /// Write a message to the log for the given namespace. Returns the certified record or a write
    /// error.
    async fn write(
        &self,
        namespace: Namespace,
        message: Message,
    ) -> Result<CertifiedRecord, ClientError>;

    /// Get the certified log for the given namespace and time range.
    async fn read_certified(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<CertifiedLog, ClientError>;

    /// Get the uncertified log for the given namespace and time range.
    async fn read(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Log, ClientError>;

    /// Attempt to read the message specified by the given namespace and message ID.
    async fn read_message(
        &self,
        namespace: Namespace,
        msg_id: B256,
    ) -> Result<CertifiedReadMessageResponse, ClientError>;

    /// Subscribe to all messages in the given namespace.
    async fn subscribe(&self, namespace: Namespace) -> Result<ReceiverStream<Record>, ClientError>;

    /// Subscribe to all certified records in the given namespace.
    async fn subscribe_certified(
        &self,
        namespace: Namespace,
    ) -> Result<ReceiverStream<CertifiedRecord>, ClientError>;
}
