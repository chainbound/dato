use std::collections::VecDeque;

use hashbrown::HashMap;

use crate::common::{Log, Namespace, Record, Timestamp};

pub trait DataStore {
    fn read_range(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log;
    fn write_one(&mut self, namespace: Namespace, record: Record);
}

/// An in-memory backend for a log.
///
/// Each record is stored in a `VecDeque` in a `HashMap` keyed by namespace.
/// The `cap` field is the maximum number of records to store in each `VecDeque`.
///
/// The `cap` field is optional, if `None` then the `VecDeque` will grow indefinitely.
pub struct InMemoryStore {
    cap: Option<usize>,
    records: HashMap<Namespace, VecDeque<Record>>,
}

impl InMemoryStore {
    pub fn with_capacity(cap: usize) -> Self {
        Self { cap: Some(cap), records: HashMap::with_capacity(cap) }
    }
}

impl DataStore for InMemoryStore {
    fn read_range(&self, namespace: Namespace, start: Timestamp, end: Timestamp) -> Log {
        let Some(existing) = self.records.get(&namespace) else {
            return Log { records: Vec::new() }
        };

        let records = existing
            .iter()
            .filter(|record| record.timestamp >= start && record.timestamp <= end)
            .cloned()
            .collect();

        Log { records }
    }

    fn write_one(&mut self, namespace: Namespace, record: Record) {
        if let Some(records) = self.records.get_mut(&namespace) {
            // evict the oldest record if we have reached capacity
            if self.cap.is_some_and(|cap| records.len() >= cap) {
                records.pop_front();
            }
            records.push_back(record);
        } else {
            let mut records = VecDeque::new();
            records.push_back(record);
            self.records.insert(namespace, records);
        }
    }
}
