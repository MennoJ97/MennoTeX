//! OpenPGP verification of the TeX Live package database.
//!
//! `texlive.tlpdb.sha512.asc` is a detached signature over
//! `texlive.tlpdb.sha512`, which holds the SHA-512 of the *uncompressed*
//! `texlive.tlpdb`. Every package archive is then pinned by the SHA-512 in
//! the database, so this one signature anchors the whole chain.
//!
//! We pin the *primary* key fingerprint. TeX Live extends the expiry of its
//! signing subkey every year (see `tlpkg/gpg/tl-key-extension.txt`), so
//! newer copies of the same key, e.g. from an installed `texlive.infra`
//! (`tlpkg/gpg/pubring.gpg`), are accepted when they carry the same primary
//! fingerprint and valid bindings.

use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use pgp::composed::{Deserializable, DetachedSignature, SignedPublicKey};
use pgp::types::KeyDetails;

/// TeX Live Distribution <tex-live@tug.org>, as published at
/// <https://tug.org/texlive/files/texlive.asc>.
const TEXLIVE_KEY: &str = include_str!("../keys/texlive.asc");
pub const TEXLIVE_PRIMARY_FINGERPRINT: &str = "c78b82d8c79512f79cc0d7c80d5e5d9106bab6bc";

pub struct Verifier {
    keys: Vec<SignedPublicKey>,
}

/// Outcome of a successful verification, for logging.
#[derive(Debug)]
pub struct Verified {
    pub signing_key: String,
    /// Set when the signing subkey had expired at signing time according to
    /// the newest key material we have; TeX Live renews it yearly.
    pub expired_key_warning: bool,
}

fn fingerprint_hex(k: &impl KeyDetails) -> String {
    format!("{:x}", k.fingerprint())
}

impl Verifier {
    /// The pinned TeX Live key, plus newer copies of it from `extra_keyring`
    /// (a binary keyring such as `tlpkg/gpg/pubring.gpg`) if present.
    pub fn new(extra_keyring: Option<&Path>) -> Result<Verifier> {
        let (key, _) = SignedPublicKey::from_string(TEXLIVE_KEY).context("parsing embedded TeX Live key")?;
        let mut keys = vec![key];
        if let Some(path) = extra_keyring.filter(|p| p.exists()) {
            let file = std::fs::File::open(path)?;
            for k in SignedPublicKey::from_bytes_many(BufReader::new(file))? {
                // A malformed key in the keyring must not break verification.
                let Ok(k) = k else { continue };
                if fingerprint_hex(&k) == TEXLIVE_PRIMARY_FINGERPRINT {
                    keys.push(k);
                }
            }
        }
        // Only the subkey bindings matter here (checked per signature below).
        // `verify_bindings()` is too strict for this key: it also treats
        // third-party certifications on the user id as self-signatures.
        keys.retain(|k| fingerprint_hex(k) == TEXLIVE_PRIMARY_FINGERPRINT);
        if keys.is_empty() {
            bail!("no valid TeX Live signing key available");
        }
        Ok(Verifier { keys })
    }

    /// Trust only `armored` (whose primary fingerprint must be
    /// `fingerprint`): the test repository in `testdata/tlnet`.
    #[cfg(test)]
    pub fn with_key(armored: &str, fingerprint: &str) -> Result<Verifier> {
        let (key, _) = SignedPublicKey::from_string(armored)?;
        if fingerprint_hex(&key) != fingerprint.trim() {
            bail!("test key fingerprint mismatch");
        }
        Ok(Verifier { keys: vec![key] })
    }

    /// Verify an ASCII-armored detached signature over `data`.
    pub fn verify_detached(&self, data: &[u8], armored_sig: &str) -> Result<Verified> {
        let (sig, _) = DetachedSignature::from_string(armored_sig).context("parsing signature")?;
        let issuer_fprs: Vec<String> =
            sig.signature.issuer_fingerprint().iter().map(|f| format!("{f:x}")).collect();
        let sig_time = sig.signature.created().map(|t| t.as_secs());

        let mut last_err = None;
        for key in &self.keys {
            for sub in &key.public_subkeys {
                let fpr = fingerprint_hex(sub);
                if !issuer_fprs.is_empty() && !issuer_fprs.contains(&fpr) {
                    continue;
                }
                // The subkey must be bound to the pinned primary key. Use the
                // newest binding that verifies; older ones may use SHA-1.
                let binding = sub
                    .signatures
                    .iter()
                    .filter(|s| s.verify_subkey_binding(&key.primary_key, &sub.key).is_ok())
                    .max_by_key(|s| s.created().map(|t| t.as_secs()));
                let Some(binding) = binding else {
                    last_err = Some(anyhow!("subkey {fpr} has no valid binding to the TeX Live key"));
                    continue;
                };
                match sig.verify(sub, data) {
                    Ok(()) => {
                        // Expiry is relative to the subkey's creation time.
                        let expires = binding
                            .key_expiration_time()
                            .filter(|d| d.as_secs() != 0)
                            .map(|d| sub.key.created_at().as_secs().saturating_add(d.as_secs()));
                        let expired = matches!((expires, sig_time), (Some(e), Some(t)) if t > e);
                        return Ok(Verified { signing_key: fpr, expired_key_warning: expired });
                    }
                    Err(e) => last_err = Some(anyhow!("bad signature: {e}")),
                }
            }
        }
        Err(last_err
            .unwrap_or_else(|| anyhow!("signature was not made by the TeX Live key (issuer {issuer_fprs:?})")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real signature from tlnet (2026-10-05) over the matching `.sha512`.
    const SHA512: &[u8] = include_bytes!("../testdata/texlive.tlpdb.sha512");
    const SIG: &str = include_str!("../testdata/texlive.tlpdb.sha512.asc");

    #[test]
    fn verifies_real_tlnet_signature() {
        let v = Verifier::new(None).unwrap();
        let ok = v.verify_detached(SHA512, SIG).unwrap();
        assert_eq!(ok.signing_key, "d8f2f86057a857e42a88106a4ce1877e19438c70");
        assert!(!ok.expired_key_warning);
    }

    #[test]
    fn rejects_tampered_data() {
        let v = Verifier::new(None).unwrap();
        let mut bad = SHA512.to_vec();
        bad[0] ^= 1;
        assert!(v.verify_detached(&bad, SIG).is_err());
    }
}
