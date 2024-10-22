use alloy::{
    primitives::Address,
    providers::{ProviderBuilder, RootProvider},
    sol,
    transports::http::Http,
};
use blst::min_pk::PublicKey as BlsPublicKey;
use reqwest::Client;
use url::Url;

use super::ValidatorInfo;

use ValidatorRegistryContract::{Validator, ValidatorRegistryContractInstance};

/// A smart-contract-based validator registry for the DATO network validators.
#[derive(Debug, Clone)]
pub struct SmartContractRegistry(
    ValidatorRegistryContractInstance<Http<Client>, RootProvider<Http<Client>>>,
);

impl SmartContractRegistry {
    /// Creates a new `ValidatorRegistry` instance with the given execution client URL and registry
    /// address to interact with the `ValidatorRegistry` contract on an Ethereum network.
    pub fn new<U: Into<Url>>(execution_client_url: U, registry_address: Address) -> Self {
        let provider = ProviderBuilder::new().on_http(execution_client_url.into());
        let registry = ValidatorRegistryContract::new(registry_address, provider);

        Self(registry)
    }

    /// Gets the validator count.
    pub async fn get_validator_count(&self) -> eyre::Result<u64> {
        self.0.getValidatorCount().call().await.map_err(Into::into).map(|count| count._0.to())
    }

    /// Gets a validator by index.
    pub async fn get_validator_by_index(&self, index: u64) -> eyre::Result<ValidatorInfo> {
        self.0
            .getValidatorByIndex(index)
            .call()
            .await
            .map_err(Into::into)
            .map(|val| ValidatorInfo::try_from(val._0))
            .and_then(|val| val)
    }

    /// Gets all validators.
    pub async fn get_all_validators(&self) -> eyre::Result<Vec<ValidatorInfo>> {
        let count = self.get_validator_count().await?;
        let mut validators = Vec::new();

        for index in 0..count {
            if let Ok(validator) = self.get_validator_by_index(index).await {
                validators.push(validator);
            }
        }

        Ok(validators)
    }
}

impl TryFrom<Validator> for ValidatorInfo {
    type Error = eyre::Report;

    fn try_from(validator: Validator) -> Result<Self, Self::Error> {
        let pubkey = BlsPublicKey::from_bytes(validator.blsPubKey.to_vec().as_slice())
            .map_err(|e| eyre::eyre!("Failed to parse BLS public key: {:?}", e))?;

        Ok(Self {
            index: validator.index.to(),
            bls_pub_key: pubkey,
            stake: validator.stake.to(),
            socket: validator.socket,
            exists: validator.exists,
        })
    }
}

sol! {
    #[sol(rpc)]
    interface ValidatorRegistryContract {
        struct Validator {
            uint256 index;
            bytes blsPubKey;
            uint256 stake;
            string socket;
            bool exists;
        }

        function getValidatorCount() external view returns (uint256);
        function getValidatorByIndex(uint64 _index) external view returns (Validator memory);
    }

    library Errors {
        error CountError(uint256 count);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tracing::warn;

    use super::*;

    #[tokio::test]
    async fn test_get_all_validators() -> eyre::Result<()> {
        let _ = tracing_subscriber::fmt::try_init();

        if reqwest::get("http://localhost:8545").await.is_err() {
            warn!("Skipping test_get_all_validators, as the Ethereum node is not running.");
            return Ok(());
        }

        let registry = SmartContractRegistry::new(
            Url::parse("http://localhost:8545")?,
            Address::from_str("0xYourContractAddressHere")?,
        );

        let validators = registry.get_all_validators().await?;
        assert!(!validators.is_empty());

        for validator in validators {
            println!("Validator Index: {}", validator.index);
            println!("BLS Public Key: {:?}", validator.bls_pub_key);
            println!("Stake: {}", validator.stake);
            println!("Socket: {}", validator.socket);
            println!("Exists: {}", validator.exists);
        }

        Ok(())
    }
}
