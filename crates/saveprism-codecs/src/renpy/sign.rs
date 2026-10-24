//! Ren'Py's save signature: ECDSA over NIST P-256 (secp256r1) with SHA-1, from the player's own
//! key file.
//!
//! Public source read (Ren'Py is MIT-licensed):
//! - `renpy/savetoken.py`: `create_token` makes an NIST P-256 key and writes
//!   `signing-key <base64 SEC1 DER> <base64 SPKI DER>` to `security_keys.txt`; `load_tokens` reads
//!   the `signing-key` and `verifying-key` lines; `sign_data` signs the data with every signing key
//!   and writes `signature <base64 SPKI DER> <base64 raw r||s>` lines (through `encode_line`);
//!   `verify_data` checks those lines; `check_load` accepts a save whose `log` verifies and
//!   `check_persistent` does the same for the uncompressed persistent data; `upgrade_savefile`
//!   signs the `log` entry.
//! - `renpy/ecsign.py`: `Curve NIST256p`, `default_hashfunc = hashlib.sha1`, `SigningKey.to_der`
//!   (SEC1, 121 bytes), `VerifyingKey.to_der` (SPKI, 91 bytes) and the 64-byte `r || s` signature.
//! - `renpy/persistent.py`: `load` decompresses and calls `check_persistent(s, unused_data)`;
//!   `save_task` writes `zlib.compress(data) + sign_data(data)`.
//! - `renpy.py`: `path_to_saves` puts the token folder under `%APPDATA%/RenPy/tokens` on Windows,
//!   `~/.renpy/tokens` on Linux and `~/Library/RenPy/tokens` on macOS.
//!
//! The key material never leaves the process: [`parse_key_file`] takes the text of the player's
//! `security_keys.txt`, [`sign_data`] builds the `signatures` text in memory, and no function here
//! logs, formats or returns the private key.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use p256::ecdsa::signature::hazmat::{PrehashSigner, PrehashVerifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use sha1::{Digest, Sha1};

/// Prefix of a SEC1 `ECPrivateKey` DER for NIST P-256 (RFC 5915, as Ren'Py writes it).
const SEC1_PREFIX: [u8; 7] = [0x30, 0x77, 0x02, 0x01, 0x01, 0x04, 0x20];
/// The curve and public-key fields between the scalar and the point of a SEC1 DER.
const SEC1_MID: [u8; 18] = [
    0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0xa1, 0x44, 0x03, 0x42,
    0x00, 0x04,
];
/// Prefix of the `SubjectPublicKeyInfo` DER for NIST P-256 (RFC 5480, as Ren'Py writes it).
const SPKI_PREFIX: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];
/// Most signing keys read from one key file (Ren'Py signs with all of them; each line is small).
const MAX_KEYS: usize = 16;

/// One signing key from `security_keys.txt` and the public key's SPKI DER.
pub struct Key {
    signing: SigningKey,
}

impl Key {
    /// The SPKI DER of this key's public half, the first field of a `signature` line.
    fn public_der(&self) -> Vec<u8> {
        spki_der(self.signing.verifying_key())
    }
}

/// The SPKI DER (91 bytes) of a Pascal P-256 public key, in the layout Ren'Py writes.
fn spki_der(key: &VerifyingKey) -> Vec<u8> {
    let point = key.to_encoded_point(false);
    let mut der = SPKI_PREFIX.to_vec();
    // `to_encoded_point(false)` starts with 0x04, which the prefix already holds.
    der.extend_from_slice(point.as_bytes().get(1..).unwrap_or_default());
    der
}

/// Reads the `signing-key` lines of a `security_keys.txt`; malformed and `verifying-key` lines are
/// skipped, as Ren'Py's `load_tokens` does. Never fails: an unreadable line is simply not a key.
pub fn parse_key_file(text: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    for line in text.lines() {
        if keys.len() >= MAX_KEYS {
            break;
        }
        let mut parts = line.split_whitespace();
        if parts.next() != Some("signing-key") {
            continue;
        }
        let Some(encoded) = parts.next() else {
            continue;
        };
        let Ok(der) = STANDARD.decode(encoded) else {
            continue;
        };
        let Ok(secret) = p256::SecretKey::from_sec1_der(&der) else {
            continue;
        };
        keys.push(Key {
            signing: SigningKey::from(secret),
        });
    }
    keys
}

/// Signs `data` with every key and returns the text for the `signatures` entry, the format of
/// `savetoken.sign_data` (`signature <base64 SPKI DER> <base64 r||s>` and a newline per key).
/// An empty slice of keys gives an empty string.
pub fn sign_data(data: &[u8], keys: &[Key]) -> String {
    let digest = Sha1::digest(data);
    let mut text = String::new();
    for key in keys {
        let Ok(signature): Result<Signature, _> = key.signing.sign_prehash(&digest) else {
            continue;
        };
        text.push_str("signature ");
        text.push_str(&STANDARD.encode(key.public_der()));
        text.push(' ');
        text.push_str(&STANDARD.encode(signature.to_bytes()));
        text.push('\n');
    }
    text
}

/// True when one `signature` line verifies `data`, the check of `savetoken.verify_data` without the
/// "key is in `verifying_keys`" part (a save carries the public key it was signed with).
pub fn verify_data(data: &[u8], signatures: &str) -> bool {
    let digest = Sha1::digest(data);
    signatures.lines().any(|line| {
        let mut parts = line.split_whitespace();
        if parts.next() != Some("signature") {
            return false;
        }
        let (Some(key), Some(signature)) = (parts.next(), parts.next()) else {
            return false;
        };
        let (Ok(key), Ok(signature)) = (STANDARD.decode(key), STANDARD.decode(signature)) else {
            return false;
        };
        let Ok(key) = VerifyingKey::from_public_key_der(&key) else {
            return false;
        };
        let Ok(signature) = Signature::from_slice(&signature) else {
            return false;
        };
        key.verify_prehash(&digest, &signature).is_ok()
    })
}

/// The SEC1 DER of a key from its 32-byte scalar and 65-byte uncompressed point, for building a
/// `security_keys.txt` in tests and for tools that write one. `None` when a value has the wrong
/// length.
pub fn sec1_der(scalar: &[u8], point: &[u8]) -> Option<Vec<u8>> {
    if scalar.len() != 32 || point.len() != 65 {
        return None;
    }
    let mut der = SEC1_PREFIX.to_vec();
    der.extend_from_slice(scalar);
    der.extend_from_slice(&SEC1_MID);
    // The 0x04 of the uncompressed point is already the last byte of `SEC1_MID`.
    der.extend_from_slice(point.get(1..).unwrap_or_default());
    Some(der)
}

/// The SPKI DER of a 65-byte uncompressed point, for a `signing-key` line. `None` when the point
/// has the wrong length.
pub fn spki_from_point(point: &[u8]) -> Option<Vec<u8>> {
    if point.len() != 65 {
        return None;
    }
    let mut der = SPKI_PREFIX.to_vec();
    der.extend_from_slice(point.get(1..).unwrap_or_default());
    Some(der)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signing_key(scalar: u8) -> SigningKey {
        let mut bytes = [0_u8; 32];
        bytes[31] = scalar;
        SigningKey::from_slice(&bytes).unwrap()
    }

    /// A `security_keys.txt` line as `savetoken.create_token` writes it.
    fn key_line(signing: &SigningKey) -> String {
        let point = signing.verifying_key().to_encoded_point(false);
        let der = sec1_der(&signing.to_bytes(), point.as_bytes()).unwrap();
        let spki = spki_from_point(point.as_bytes()).unwrap();
        format!(
            "signing-key {} {}\n",
            STANDARD.encode(der),
            STANDARD.encode(spki)
        )
    }

    #[test]
    fn a_key_file_round_trips_and_signs_the_renpy_way() {
        let signing = signing_key(7);
        let text = format!("# a comment\n{}", key_line(&signing));
        let keys = parse_key_file(&text);
        assert_eq!(keys.len(), 1);
        let data = b"a pickled log";
        let signatures = sign_data(data, &keys);
        let mut lines = signatures.lines();
        let line = lines.next().unwrap();
        assert!(lines.next().is_none());
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.first(), Some(&"signature"));
        assert_eq!(fields.len(), 3);
        let key = STANDARD.decode(fields.get(1).unwrap()).unwrap();
        assert_eq!(key, spki_der(signing.verifying_key()));
        let signature = STANDARD.decode(fields.get(2).unwrap()).unwrap();
        assert_eq!(signature.len(), 64);
        // The check `savetoken.verify_data` makes.
        assert!(verify_data(data, &signatures));
        assert!(!verify_data(b"another log", &signatures));
    }

    #[test]
    fn several_keys_give_several_lines_and_bad_lines_are_skipped() {
        let text = format!(
            "verifying-key {}\nnot a line\nsigning-key not-base64\n{}{}",
            STANDARD.encode(spki_from_point(&[4; 65]).unwrap()),
            key_line(&signing_key(3)),
            key_line(&signing_key(9))
        );
        let keys = parse_key_file(&text);
        assert_eq!(keys.len(), 2);
        let signatures = sign_data(b"log", &keys);
        assert_eq!(signatures.lines().count(), 2);
        assert!(verify_data(b"log", &signatures));
    }

    #[test]
    fn an_empty_file_has_no_keys() {
        assert!(parse_key_file("").is_empty());
        assert!(parse_key_file("# only comments\n").is_empty());
    }
}
