use std::{
    fmt,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use alloy::primitives::{Bytes, Keccak256, B256};
use blst::min_pk::{
    AggregateSignature, PublicKey as BlsPublicKey, SecretKey as BlsSecretKey,
    Signature as BlsSignature,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bls::sign_with_prefix;

/// A namespace for a log record.
pub type Namespace = Bytes;

/// A message to be written to the log.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message(pub Bytes);

impl Message {
    /// Returns the digest of the message and the namespace.
    pub fn digest(&self, namespace: &Namespace) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(&self.0);

        hasher.finalize()
    }

    pub fn record_digest(&self, namespace: &Namespace, timestamp: Timestamp) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(timestamp.0.to_le_bytes());
        hasher.update(&self.0);

        hasher.finalize()
    }
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Write error: {0:?}")]
    Write(#[from] WriteError),
    #[error("Read error: {0:?}")]
    Read(#[from] ReadError),
    #[error("Subscription error: {0:?}")]
    SubscriptionError(#[from] SubscriptionError),
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
    #[error("No quorum reached, available: {available}, unavailable: {unavailable}")]
    NoQuorum { available: usize, unavailable: usize },
}

#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("Timed out")]
    Timeout,
    #[error("Failed to connect to validator publisher socket")]
    FailedToConnect,
    #[error("Failed to subscribe to topic")]
    FailedToSubscribe,
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

    pub fn duration_since(&self, other: Instant) -> Duration {
        let since_the_epoch =
            SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards");
        let now = since_the_epoch.as_millis();
        Duration::from_millis(now as u64 - self.0 as u64) - other.elapsed()
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

impl From<u64> for Timestamp {
    fn from(value: u64) -> Self {
        Timestamp(value as u128)
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
/// the quorum signature for the message.
/// The signature is over the msg_id, message, and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertifiedRecord {
    /// An indexed array of timestamps. The index is the validator ID.
    pub timestamps: Vec<Timestamp>,
    pub message: Message,
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

    /// Returns the certified record from a list of records.
    /// This method DOES NOT check the hash of each individual record message.
    pub fn from_records_unchecked(records: &[Record]) -> Self {
        let timestamps = records.iter().map(|r| r.timestamp).collect::<Vec<_>>();
        let sigs = records.iter().map(|r| r.signature).collect::<Vec<_>>();
        let message = records[0].message.clone();

        // TODO: there's probably a better way to do this
        let mut quorum_signature = AggregateSignature::from_signature(&sigs[0]);
        for sig in sigs.iter().skip(1) {
            let _ = quorum_signature.add_signature(sig, false);
        }

        CertifiedRecord { timestamps, message, quorum_signature }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CertifiedLog {
    pub records: Vec<CertifiedRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum CertifiedReadMessageResponse {
    Available(CertifiedRecord),
    Unavailable(CertifiedUnavailableMessage),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ReadMessageResponse {
    Available(Record),
    Unavailable(UnavailableMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertifiedUnavailableMessage {
    pub timestamps: Vec<Timestamp>,
    pub msg_id: B256,
    #[serde(with = "serde_bls_aggregate")]
    pub quorum_signature: AggregateSignature,
}

impl CertifiedUnavailableMessage {
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

/// An unavailable message response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnavailableMessage {
    pub timestamp: Timestamp,
    pub msg_id: B256,
    #[serde(with = "serde_bls")]
    pub signature: BlsSignature,
}

impl UnavailableMessage {
    /// Create a signed certificate for an unavailable message, by signing over its
    /// message ID and timestamp with the given secret key.
    pub fn create_signed(msg_id: B256, secret_key: &BlsSecretKey) -> Self {
        let timestamp = Timestamp::now();
        let digest = {
            let mut hasher = Keccak256::new();
            hasher.update(msg_id);
            hasher.update(timestamp.0.to_le_bytes());
            hasher.finalize()
        };

        let signature = sign_with_prefix(secret_key, digest);

        UnavailableMessage { timestamp, msg_id, signature }
    }

    pub fn digest(&self) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(self.msg_id);
        hasher.update(self.timestamp.0.to_le_bytes());
        hasher.finalize()
    }
}

/// A record of a message at a particular time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub timestamp: Timestamp,
    pub message: Message,
    #[serde(with = "serde_bls")]
    pub signature: BlsSignature,
}

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

    pub fn message_digest(&self, namespace: &Namespace) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
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
    pub pubkey: BlsPublicKey,
}

impl ValidatorIdentity {
    pub fn new(index: usize, pubkey: BlsPublicKey) -> Self {
        ValidatorIdentity { index, pubkey }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResponse {
    pub port: u16,
    pub auth_token: Bytes,
}
