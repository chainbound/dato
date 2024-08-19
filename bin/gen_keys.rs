//! This binary generates a CSV file with lines containing the following fields:
//! - Index (the incremental validator index)
//! - Private BLS key hex-encoded
//! - Public BLS key hex-encoded
//! - Validator DNS name in the expected Docker network setup
//!
//! The goal of using a file-based registry is to quickly simulate a discovery process
//! to test DATO in a local Docker network.

use std::{fs::File, io::Write};

use alloy::hex::encode_prefixed;
use dato::bls::random_bls_secret;

fn main() -> eyre::Result<()> {
    let mut f = File::create("registry.txt").unwrap();

    for i in 0..1000 {
        let privkey = random_bls_secret();
        let pubkey = encode_prefixed(privkey.sk_to_pk().to_bytes());
        let privkey = encode_prefixed(privkey.to_bytes());

        let line = format!("{i},{privkey},{pubkey},dato-validator-{i}:8222\n");

        f.write_all(line.as_bytes())?;
    }

    f.sync_all()?;

    Ok(())
}
