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

    /// Returns the digest of the namespace, message and timestamp.
    pub fn record_digest(&self, namespace: &Namespace, timestamp: Timestamp) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(timestamp.0.to_le_bytes());
        hasher.update(&self.0);

        hasher.finalize()
    }
}

/// An error that can occur when interacting with the client.
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum ClientError {
    #[error("Write error: {0:?}")]
    Write(#[from] WriteError),
    #[error("Read error: {0:?}")]
    Read(#[from] ReadError),
    #[error("Subscription error: {0:?}")]
    SubscriptionError(#[from] SubscriptionError),
}

/// An error that can occur when writing to the log.
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum WriteError {
    #[error("Timed out")]
    Timeout,
    #[error("Network error: {0:?}")]
    Network(#[from] msg::ReqError),
    #[error("No quorum reached, only {got} out of {needed} validators signed")]
    NoQuorum { got: usize, needed: usize },
}

/// An error that can occur when reading from the log.
#[derive(Debug, Error)]
#[allow(missing_docs)]
pub enum ReadError {
    #[error("Timed out")]
    Timeout,
    #[error("No quorum reached, available: {available}, unavailable: {unavailable}")]
    NoQuorum { available: usize, unavailable: usize },
}

/// An error that can occur when subscribing to the log.
#[allow(missing_docs)]
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
    /// Returns the current timestamp.
    pub fn now() -> Self {
        let start = SystemTime::now();
        let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
        Timestamp(since_the_epoch.as_millis())
    }

    /// Returns the duration since the given timestamp.
    pub fn duration_since(&self, other: Instant) -> Duration {
        let since = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards");
        Duration::from_millis(since.as_millis() as u64 - self.0 as u64) - other.elapsed()
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    /// The message that was certified.
    pub message: Message,
    /// The aggregated signature for the message from all validators.
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

/// A log of certified records of seen messages.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CertifiedLog {
    /// The certified records in the log.
    pub records: Vec<CertifiedRecord>,
}

/// A response to a `read_message` request on the client API.
#[derive(Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum CertifiedReadMessageResponse {
    /// A certificate of availability for a given message
    Available(CertifiedRecord),
    /// A certificate of unavailability for a given message
    Unavailable(CertifiedUnavailableMessage),
}

/// A response to a `read_message` request on the validator API.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, Deserialize)]
pub enum ReadMessageResponse {
    /// A record of the availability of the message.
    Available(Record),
    /// A record of the unavailability of the message.
    Unavailable(UnavailableMessage),
}

/// A certified "non-existence" record for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertifiedUnavailableMessage {
    /// An indexed array of timestamps. The index is the validator ID.
    pub timestamps: Vec<Timestamp>,
    /// The message ID that is unavailable.
    pub msg_id: B256,
    /// The aggregated signature for the message from all validators.
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

/// A signed "non-existence" record for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnavailableMessage {
    /// The timestamp of the unavailability message.
    pub timestamp: Timestamp,
    /// The message ID that is unavailable.
    pub msg_id: B256,
    /// The signature for the message ID and timestamp.
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

    /// Returns the digest of the message ID and timestamp.
    pub fn digest(&self) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(self.msg_id);
        hasher.update(self.timestamp.0.to_le_bytes());
        hasher.finalize()
    }
}

/// A signed record of a message with an associated timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// The timestamp of the observed message.
    pub timestamp: Timestamp,
    /// The message that was observed.
    pub message: Message,
    /// The signature for the namepsace, message, and timestamp.
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
    /// Returns the digest of the namespace, message and timestamp.
    pub fn digest(&self, namespace: &Namespace) -> B256 {
        let mut hasher = Keccak256::new();
        hasher.update(namespace);
        hasher.update(self.timestamp.0.to_le_bytes());
        hasher.update(&self.message.0);

        hasher.finalize()
    }

    /// Returns the inner message digest for the record.
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
    /// The records in the log.
    pub records: Vec<Record>,
}

impl Log {
    /// Create a new log from a list of records.
    pub fn extend(&mut self, other: Log) {
        self.records.extend(other.records);
    }

    /// Returns the number of records in the log.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns true if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// A validator identity, consisting of an index and a public key.
#[derive(Debug, Clone)]
pub struct ValidatorIdentity {
    /// The validator incremental index
    pub index: usize,
    /// The validator public key used to sign messages
    pub pubkey: BlsPublicKey,
}

impl ValidatorIdentity {
    /// Create a new `ValidatorIdentity` from an index and public key.
    pub fn new(index: usize, pubkey: BlsPublicKey) -> Self {
        ValidatorIdentity { index, pubkey }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeResponse {
    pub port: u16,
    pub auth_token: Bytes,
}
