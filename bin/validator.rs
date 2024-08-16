use blst::min_pk::SecretKey as BlsSecretKey;
use clap::Parser;

use dato::Validator;

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
    #[clap(long)]
    pub port: u16,
    #[clap(long)]
    pub secret_key: String,
}

#[derive(Debug, Parser)]
struct RegisterOpts {
    #[clap(long)]
    pub pubkey: String,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let opts = CliOpts::parse();

    match opts.cmd {
        SubCommand::Run(run_opts) => {
            let sk = BlsSecretKey::from_bytes(&hex::decode(run_opts.secret_key)?)
                .map_err(|e| eyre::eyre!("Invalid secret key: {:?}", e))?;

            Validator::new_in_memory(sk, run_opts.port).await?.run().await;
        }
        SubCommand::Register(register_opts) => {
            println!("Registering with pubkey: {}", register_opts.pubkey);
        }
    }

    Ok(())
}
