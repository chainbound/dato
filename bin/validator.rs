use blst::min_pk::SecretKey as BlsSecretKey;
use clap::{Parser, ValueEnum};

use dato::Validator;
use tracing::info;

#[derive(Debug, Parser)]
struct CliOpts {
    #[clap(subcommand)]
    pub cmd: SubCommand,
}

#[derive(Debug, Parser)]
enum SubCommand {
    /// Run the validator binary main loop.
    Run(RunOpts),
    /// Register a new validator to the on-chain registry contract.
    /// This command should be run once per individual validator instance.
    Register(RegisterOpts),
}

#[derive(Debug, Parser)]
struct RunOpts {
    #[clap(long, env = "DATO_VAL_PORT", default_value = "12450")]
    pub port: u16,
    #[clap(long, env = "DATO_VAL_SECRET_KEY")]
    pub secret_key: String,
    #[clap(long, env = "DATO_VAL_BACKEND", default_value = "in-memory")]
    pub backend: BackendType,
}

#[derive(Debug, Clone, Parser, ValueEnum)]
pub enum BackendType {
    #[clap(name = "in-memory")]
    InMemory,
    #[clap(name = "filesystem")]
    Filesystem,
}

#[derive(Debug, Parser)]
struct RegisterOpts {
    #[clap(long)]
    pub pubkey: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let opts = CliOpts::parse();

    match opts.cmd {
        SubCommand::Run(run_opts) => {
            let sk = BlsSecretKey::from_bytes(&alloy::hex::decode(run_opts.secret_key)?)
                .map_err(|e| eyre::eyre!("Invalid secret key: {:?}", e))?;

            match run_opts.backend {
                BackendType::InMemory => {
                    info!("Running validator with in-memory backend on port {}", run_opts.port);
                    Validator::new_in_memory(sk, run_opts.port).await?.run().await;
                }
                BackendType::Filesystem => {
                    info!("Running validator with filesystem backend on port {}", run_opts.port);
                    todo!()
                }
            }
        }
        SubCommand::Register(register_opts) => {
            println!("Registering with pubkey: {}", register_opts.pubkey);

            // TODO: registration logic
        }
    }

    Ok(())
}
