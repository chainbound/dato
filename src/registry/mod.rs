use std::net::SocketAddr;

use async_trait::async_trait;
use blst::min_pk::PublicKey as BlsPublicKey;

use crate::ValidatorIdentity;

pub mod contract;
pub mod filesystem;

/// An interface for querying the set of validators in the DATO network.
/// This is used by clients to discover the set of sockets to connect to.
#[async_trait]
pub trait Registry {
    /// Returns the number of validators in the network.
    async fn validator_count(&self) -> eyre::Result<u64>;

    /// Returns a list of all validators in the network.
    async fn all_validators(&self) -> eyre::Result<Vec<ValidatorInfo>>;
}

#[derive(Debug, Clone)]
pub struct ValidatorInfo {
    pub index: u64,
    pub bls_pub_key: BlsPublicKey,
    pub stake: u64,
    pub socket: String,
    pub exists: bool,
}

impl ValidatorInfo {
    pub fn identity(&self) -> ValidatorIdentity {
        ValidatorIdentity { index: self.index as usize, pubkey: self.bls_pub_key }
    }
}

#[async_trait]
impl Registry for contract::ValidatorRegistry {
    async fn validator_count(&self) -> eyre::Result<u64> {
        self.get_validator_count().await
    }

    async fn all_validators(&self) -> eyre::Result<Vec<ValidatorInfo>> {
        self.get_all_validators().await
    }
}

#[async_trait]
impl Registry for filesystem::ValidatorRegistry {
    async fn validator_count(&self) -> eyre::Result<u64> {
        Ok(self.validators.len() as u64)
    }

    async fn all_validators(&self) -> eyre::Result<Vec<ValidatorInfo>> {
        Ok(self.validators.clone())
    }
}
