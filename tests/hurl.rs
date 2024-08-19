//! These tests work together with the `hurl` CLI tool to test the API.
//! Visit <https://hurl.dev/> to install it if you haven't already.

use std::time::Duration;

use tokio::time::sleep;
use tracing::info;

use dato::{bls::random_bls_secret, Client, Validator, ValidatorIdentity};

#[tokio::test]
async fn test_api_write_request() -> eyre::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    let dummy_sk = random_bls_secret();
    let pubkey = dummy_sk.sk_to_pk();
    let validator = Validator::new_in_memory(dummy_sk, 0).await?;
    let validator_addr = validator.local_addr().expect("Listening");
    tokio::spawn(validator);
    info!("Validator listening on: {}", validator_addr);

    let mut client = Client::new();
    let identity = ValidatorIdentity::new(0, pubkey);
    client.connect_validator(identity, validator_addr).await?;
    info!("Client connected to validator");

    client.run_api(8090).await?;

    sleep(Duration::from_secs(10)).await;

    // TODO: start the hurl command to test the API

    Ok(())
}
