use blst::{
    min_pk::{PublicKey as BlsPublicKey, SecretKey as BlsSecretKey, Signature as BlsSignature},
    BLST_ERROR,
};
use rand::{thread_rng, RngCore};

/// The BLS Domain Separator used in Ethereum 2.0.
pub const BLS_DST_PREFIX: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

/// Sign the given data with the given BLS secret key.
/// Returns the BLS signature.
#[inline]
pub(crate) fn sign_with_prefix(key: &BlsSecretKey, data: impl AsRef<[u8]>) -> BlsSignature {
    key.sign(data.as_ref(), BLS_DST_PREFIX, &[])
}

/// Verify the given BLS signature against the given message digest and the public key.
/// Returns `true` if the signature is valid, `false` otherwise.
#[inline]
pub(crate) fn verify_signature(
    signature: &BlsSignature,
    pubkey: &BlsPublicKey,
    digest: impl AsRef<[u8]>,
) -> bool {
    signature.verify(false, digest.as_ref(), BLS_DST_PREFIX, &[], pubkey, true) ==
        BLST_ERROR::BLST_SUCCESS
}

/// Generate a random BLS secret key.
pub fn random_bls_secret() -> BlsSecretKey {
    let mut rng = thread_rng();
    let mut ikm = [0u8; 32];
    rng.fill_bytes(&mut ikm);
    BlsSecretKey::key_gen(&ikm, &[]).unwrap()
}
