use alloy::{
    hex::{decode_to_slice, encode_prefixed},
    primitives::{Bytes, Keccak256, B256},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use blst::min_pk::{AggregateSignature, Signature as BlsSignature};
use thiserror::Error;

/// A namespace for a log record.
pub type Namespace = Bytes;

/// A message to be written to the log.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message(pub Bytes);

impl Message {
    pub fn digest(&self, timestamp: Timestamp, namespace: &Namespace) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(timestamp.0.to_le_bytes());
        hasher.update(&self.0);

        hasher.finalize()
    }
}

#[derive(Debug, Error)]
pub enum WriteError {
    #[error("Timed out")]
    Timeout,
    #[error("Network error: {0:?}")]
    Network(#[from] msg::ReqError),
    #[error("No quorum reached, only {got} out of {needed} validators signed")]
    NoQuorum { got: usize, needed: usize },
}

#[derive(Debug, Error)]
pub enum ReadError {
    #[error("Timed out")]
    Timeout,
}

/// A type representing a UNIX millisecond timestamp
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(u128);

impl Timestamp {
    pub fn now() -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
        Timestamp(since_the_epoch.as_millis())
    }
}

impl From<u128> for Timestamp {
    fn from(value: u128) -> Self {
        Timestamp(value)
    }
}

impl From<Timestamp> for u128 {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add for Timestamp {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Timestamp(self.0 + other.0)
    }
}

impl std::ops::Div<u128> for Timestamp {
    type Output = Self;

    fn div(self, other: u128) -> Self {
        Timestamp(self.0 / other)
    }
}

/// A certified record of a message at a particular time. Contains
/// the quorum signature for the message. The message may be `None` if if does not exist.
/// The signature is over the msg_id, message, and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertifiedRecord {
    /// An indexed array of timestamps. The index is the validator ID.
    pub timestamps: Vec<Timestamp>,
    pub message: Option<Message>,
    #[serde(with = "serde_bls_aggregate")]
    pub quorum_signature: AggregateSignature,
}

impl CertifiedRecord {
    /// Returns the median of all the timestamps in the array.
    pub fn certified_timestamp(&mut self) -> Timestamp {
        self.timestamps.sort();
        if self.timestamps.len() % 2 == 0 {
            let mid = self.timestamps.len() / 2;
            (self.timestamps[mid - 1] + self.timestamps[mid]) / 2
        } else {
            self.timestamps[self.timestamps.len() / 2]
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertifiedLog {
    pub records: Vec<CertifiedRecord>,
}

/// A record of a message at a particular time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub timestamp: Timestamp,
    pub message: Message,
    #[serde(with = "serde_bls")]
    pub signature: BlsSignature,
}

// serde bytes
mod serde_bls_aggregate {
    use blst::min_pk::{AggregateSignature, Signature as BlsSignature};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(sig: &AggregateSignature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&alloy::hex::encode_prefixed(sig.to_signature().to_bytes()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<AggregateSignature, D::Error>
    where
        D: Deserializer<'de>,
    {
        alloy::hex::decode(String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
            .and_then(|bytes| {
                let sig = BlsSignature::from_bytes(&bytes).map_err(|e| {
                    serde::de::Error::custom(format!(
                        "failed to deserialize BLS signature: {:?}",
                        e
                    ))
                })?;

                Ok(AggregateSignature::from_signature(&sig))
            })
    }
}

// serde bytes
mod serde_bls {
    use blst::min_pk::Signature as BlsSignature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(sig: &BlsSignature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&alloy::hex::encode_prefixed(sig.to_bytes()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlsSignature, D::Error>
    where
        D: Deserializer<'de>,
    {
        alloy::hex::decode(String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
            .and_then(|bytes| {
                BlsSignature::from_bytes(&bytes).map_err(|e| {
                    serde::de::Error::custom(format!(
                        "failed to deserialize BLS signature: {:?}",
                        e
                    ))
                })
            })
    }
}

impl Record {
    pub fn digest(&self, namespace: &Namespace) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(self.timestamp.0.to_le_bytes());
        hasher.update(&self.message.0);

        hasher.finalize()
    }
}
/// An ordered list of records.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Log {
    pub records: Vec<Record>,
}

impl Log {
    pub fn extend(&mut self, other: Log) {
        self.records.extend(other.records);
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ValidatorIdentity {
    pub index: usize,
    pub pubkey: blst::min_pk::PublicKey,
}

impl ValidatorIdentity {
    pub fn new(index: usize, pubkey: blst::min_pk::PublicKey) -> Self {
        ValidatorIdentity { index, pubkey }
    }
}
