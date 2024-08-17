use std::collections::VecDeque;

use alloy::primitives::B256;
use hashbrown::HashMap;
use hashmore::FIFOMap;
use tracing::warn;

use crate::{
    common::{Log, Namespace, Record, Timestamp},
    Message,
};

pub trait DataStore {
    fn read_range(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log;
    fn read_message(&self, namespace: Namespace, msg_id: B256) -> Option<Record>;
    fn write_one(&mut self, namespace: Namespace, record: Record);
}

/// An in-memory backend for the data store.
pub struct InMemoryStore {
    cap: usize,
    /// A map from namespace to a FIFO map of records. The FIFO map is used to
    /// evict old records when the capacity is reached for each namespace.
    record_maps: HashMap<Namespace, FIFOMap<B256, Record>>,
}

impl InMemoryStore {
    pub fn with_capacity(cap: usize) -> Self {
        Self { cap, record_maps: HashMap::with_capacity(cap) }
    }
}

impl DataStore for InMemoryStore {
    fn read_range(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log {
        let Some(existing) = self.record_maps.get(&namespace) else {
            return Log { records: Vec::new() }
        };

        // PERF: how to avoid iterating over all records in the namespace?
        // we could have a "FIFO B-tree map" keyed by timestamp ?
        let records = existing
            .values()
            .filter(|record| record.timestamp >= start && record.timestamp <= end)
            .cloned()
            .collect();

        Log { records }
    }

    fn read_message(&self, namespace: Namespace, msg_id: B256) -> Option<Record> {
        let existing = self.record_maps.get(&namespace)?;

        existing.iter().find(|(digest, _)| *digest == &msg_id).map(|(_, record)| record.clone())
    }

    fn write_one(&mut self, namespace: Namespace, record: Record) {
        let record_digest = record.digest(&namespace);

        if let Some(records) = self.record_maps.get_mut(&namespace) {
            records.insert(record_digest, record);
        } else {
            let mut records = FIFOMap::with_capacity(self.cap);
            records.insert(record_digest, record);
            self.record_maps.insert(namespace, records);
        }
    }
}
