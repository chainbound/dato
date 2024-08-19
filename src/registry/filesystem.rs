use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use blst::min_pk::PublicKey as BlsPublicKey;

use super::ValidatorInfo;

/// A validator registry that reads from the filesystem and caches the results.
#[derive(Debug, Clone)]
pub struct FilesystemRegistry {
    /// The path to the file containing the validator information.
    pub path: PathBuf,
    /// The list of validators loaded from the file.
    pub validators: Vec<ValidatorInfo>,
}

impl FilesystemRegistry {
    /// Create a new `ValidatorRegistry` that reads from the given path.
    ///
    /// The file should be a CSV with the following columns:
    /// `index, private_key, pubkey, stake, socket`
    pub fn read_from_file(path: PathBuf) -> eyre::Result<Self> {
        let file = BufReader::new(File::open(&path)?);

        let mut validators = Vec::new();
        for line in file.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split(',').collect();

            let index = parts[0].parse()?;
            let pubkey_str: String = parts[2].parse()?;
            let bls_pub_key = BlsPublicKey::from_bytes(&alloy::hex::decode(pubkey_str)?)
                .map_err(|e| eyre::eyre!("Invalid BLS public key: {:?}", e))?;

            // for now, we don't care about the stake
            let stake = 0;
            let socket = parts[3].parse()?;

            let val = ValidatorInfo { index, bls_pub_key, stake, socket, exists: true };
            validators.push(val);
        }

        Ok(Self { path, validators })
    }
}
