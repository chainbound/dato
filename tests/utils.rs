use std::net::SocketAddr;

use blst::min_pk::PublicKey as BlsPublicKey;
use dato::{bls::random_bls_secret, Validator};

pub async fn spin_up_validator() -> eyre::Result<(SocketAddr, BlsPublicKey)> {
    let dummy_sk = random_bls_secret();
    let pubkey = dummy_sk.sk_to_pk();
    let validator = Validator::new_in_memory(dummy_sk, 0).await?;
    let validator_addr = validator.local_addr().expect("Listening");
    validator.run_forever();

    Ok((validator_addr, pubkey))
}
