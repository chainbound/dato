use std::str::FromStr;

use alloy::{
    contract::{Error as ContractError, Result as ContractResult},
    primitives::{Address, Bytes},
    providers::{ProviderBuilder, RootProvider},
    sol,
    sol_types::{Error as SolError, SolInterface},
    transports::{http::Http, TransportError},
};
use reqwest::Client;
use url::Url;

use ValidatorRegistryContract::{
    getValidatorByIndexReturn, getValidatorCountReturn, Validator,
    ValidatorRegistryContractInstance,
};

#[derive(Debug, Clone)]
pub struct ValidatorRegistry(
    ValidatorRegistryContractInstance<Http<Client>, RootProvider<Http<Client>>>,
);

impl ValidatorRegistry {
    pub fn new<U: Into<Url>>(execution_client_url: U, registry_address: Address) -> Self {
        let provider = ProviderBuilder::new().on_http(execution_client_url.into());
        let registry = ValidatorRegistryContract::new(registry_address, provider);

        Self(registry)
    }

    /// Gets the validator count.
    pub async fn get_validator_count(&self) -> ContractResult<getValidatorCountReturn> {
        self.0.getValidatorCount().call().await.map_err(Into::into)
    }

    /// Gets a validator by index.
    pub async fn get_validator_by_index(
        &self,
        index: u64,
    ) -> ContractResult<getValidatorByIndexReturn> {
        self.0.getValidatorByIndex(index).call().await.map_err(Into::into)
    }

    /// Gets all validators.
    pub async fn get_all_validators(&self) -> ContractResult<Vec<Validator>> {
        let count = self.get_validator_count().await?;
        let mut validators = Vec::new();

        for index in 0..count._0.to() {
            if let Ok(validator) = self.get_validator_by_index(index).await {
                validators.push(validator._0);
            }
        }

        Ok(validators)
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
    use super::*;
    use alloy::primitives::U256;

    #[tokio::test]
    async fn test_get_all_validators() -> eyre::Result<()> {
        let registry = ValidatorRegistry::new(
            Url::parse("http://localhost:8545")?,
            Address::from_str("0xYourContractAddressHere")?,
        );

        let validators = registry.get_all_validators().await?;
        assert!(!validators.is_empty());

        for validator in validators {
            println!("Validator Index: {}", validator.index);
            println!("BLS Public Key: {:?}", validator.blsPubKey);
            println!("Stake: {}", validator.stake);
            println!("Socket: {}", validator.socket);
            println!("Exists: {}", validator.exists);
        }

        Ok(())
    }
}
