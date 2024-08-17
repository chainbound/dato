use std::path::PathBuf;

use alloy::primitives::Address;
use clap::Parser;
use eyre::{bail, eyre};
use url::Url;

use dato::{contract, filesystem, Client, Registry};

#[derive(Debug, Parser)]
struct CliOpts {
    #[clap(
        long,
        env = "DATO_EL_URL",
        conflicts_with = "registry_path",
        requires = "registry_address"
    )]
    pub execution_client_url: Option<Url>,
    #[clap(
        long,
        env = "DATO_REGISTRY_ADDRESS",
        conflicts_with = "registry_path",
        requires = "execution_client_url"
    )]
    pub registry_address: Option<Address>,
    #[clap(long, env = "DATO_REGISTRY_PATH", conflicts_with = "registry_address")]
    pub registry_path: Option<PathBuf>,
    #[clap(long, env = "DATO_API_PORT", default_value = "12440")]
    pub api_port: u16,
}

impl CliOpts {
    #[allow(unused)]
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

    let registry: Box<dyn Registry> = if let Some(registry_path) = opts.registry_path {
        Box::new(filesystem::ValidatorRegistry::read_from_file(registry_path)?)
    } else if let Some(registry_addr) = opts.registry_address {
        let el_url = opts.execution_client_url.ok_or(eyre!("Missing Execution client URL"))?;
        Box::new(contract::ValidatorRegistry::new(el_url, registry_addr))
    } else {
        bail!("Either 'registry_path' or 'registry_address' must be provided as a CLI argument");
    };

    let mut client = Client::new();

    // Iterate over the validators and connect to each one
    for validator in registry.all_validators().await? {
        client.connect(validator.identity(), validator.socket).await?;
    }

    client.run_api(opts.api_port).await?;

    Ok(())
}
