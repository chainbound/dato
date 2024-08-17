use alloy::primitives::B256;

use crate::{Log, Message, Namespace, ReadMessageResponse, Record, Timestamp};

pub trait ValidatorSpec {
    fn write(&mut self, namespace: Namespace, message: Message) -> Record;
    fn read(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log;
    fn read_message(&self, namespace: Namespace, msg_id: B256) -> ReadMessageResponse;
}
