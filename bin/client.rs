use std::path::PathBuf;

use alloy::primitives::Address;
use clap::Parser;
use url::Url;

use dato::{contract, filesystem, run_api, Client, Registry};

#[derive(Debug, Parser)]
struct CliOpts {
    #[clap(
        short,
        long,
        env = "DATO_EL_URL",
        conflicts_with = "registry_path",
        requires = "registry_address"
    )]
    pub execution_client_url: Option<Url>,
    #[clap(
        short,
        long,
        env = "DATO_REGISTRY_ADDRESS",
        conflicts_with = "registry_path",
        requires = "execution_client_url"
    )]
    pub registry_address: Option<Address>,
    #[clap(short, long, env = "DATO_REGISTRY_PATH", conflicts_with = "registry_address")]
    pub registry_path: Option<PathBuf>,
    #[clap(short, long, env = "DATO_API_PORT", default_value = "12440")]
    pub api_port: u16,
}

impl CliOpts {
    #[allow(dead_code)]
    pub async fn test() -> eyre::Result<Self> {
        Ok(Self {
            execution_client_url: None,
            registry_address: None,
            api_port: 0,
            registry_path: Some("registry.txt".parse()?),
        })
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts = CliOpts::parse();

    let validators = if let Some(registry_path) = opts.registry_path {
        let registry = filesystem::ValidatorRegistry::read_from_file(registry_path)?;
        registry.all_validators().await?
    } else if let Some(registry_addr) = opts.registry_address {
        let el_url = opts
            .execution_client_url
            .ok_or_else(|| eyre::eyre!("Execution client URL must be provided"))?;
        let registry = contract::ValidatorRegistry::new(el_url, registry_addr);
        registry.all_validators().await?
    } else {
        eyre::bail!("Either registry_path or registry_address must be provided")
    };

    let mut client = Client::new();

    // Iterate over the validators and connect to each one
    for validator in validators {
        client.connect(validator.identity(), validator.socket).await?;
    }

    run_api(client, opts.api_port).await?;

    Ok(())
}
