use alloy::primitives::{Bytes, B256};
use async_trait::async_trait;

use crate::{
    common::CertifiedReadMessageResponse, CertifiedLog, CertifiedRecord, Log, Message, Namespace,
    ReadError, ReadMessageResponse, Timestamp, WriteError,
};

#[async_trait]
pub trait ClientSpec {
    /// Write a message to the log for the given namespace. Returns the certified record or a write
    /// error.
    async fn write(
        &self,
        namespace: Namespace,
        message: Message,
    ) -> Result<CertifiedRecord, WriteError>;

    /// Get the certified log for the given namespace and time range.
    async fn read_certified(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<CertifiedLog, ReadError>;

    /// Get the uncertified log for the given namespace and time range.
    async fn read(
        &self,
        namespace: Namespace,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<Log, ReadError>;

    /// Attempt to read the message specified by the given namespace and message ID.
    async fn read_message(
        &self,
        namespace: Namespace,
        msg_id: B256,
    ) -> Result<CertifiedReadMessageResponse, ReadError>;
}
