use alloy::primitives::B256;

use crate::{Log, Message, Namespace, ReadMessageResponse, Record, Timestamp};

/// A validator backend specification.
pub trait ValidatorSpec {
    /// Writes a message to the log.
    fn write(&mut self, namespace: Namespace, message: Message) -> Record;

    /// Reads a range of log records from the store within the given timestamps.
    fn read(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log;

    /// Reads a single log record from the store by its message ID.
    fn read_message(&self, namespace: Namespace, msg_id: B256) -> ReadMessageResponse;
}
