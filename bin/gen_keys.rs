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
