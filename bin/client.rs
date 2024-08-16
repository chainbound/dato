use alloy::primitives::Address;
use blst::min_pk::PublicKey;
use clap::Parser;
use url::Url;

use dato::{Client, ValidatorIdentity, ValidatorRegistry};

#[derive(Debug, Parser)]
struct CliOpts {
    #[clap(short, long, env = "DATO_EL_URL")]
    pub execution_client_url: Url,
    #[clap(short, long, env = "DATO_REGISTRY_ADDRESS")]
    pub registry_address: Address,
    #[clap(short, long, env = "DATO_API_PORT", default_value = "12440")]
    pub api_port: u16,
}

impl CliOpts {
    #[allow(dead_code)]
    pub async fn test() -> eyre::Result<Self> {
        Ok(Self {
            execution_client_url: Url::parse("http://localhost:8545")?,
            registry_address: Address::default(),
            api_port: 0,
        })
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts = CliOpts::parse();

    let registry = ValidatorRegistry::new(opts.execution_client_url, opts.registry_address);
    let validators = registry.get_all_validators().await.unwrap();

    let mut client = Client::new();

    // Iterate over the validators and connect to each one
    for validator in validators {
        let validator_identity = ValidatorIdentity {
            index: validator.index.to(),
            pubkey: PublicKey::from_bytes(validator.blsPubKey.to_vec().as_slice()).unwrap(),
        };

        // Connect to the validator
        client.connect(validator_identity, validator.socket).await?;
    }

    Ok(())
}
