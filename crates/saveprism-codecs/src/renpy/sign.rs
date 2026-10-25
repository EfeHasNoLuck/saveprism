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
//! - `renpy.py`: `path_to_saves(gamedir, "tokens")` names the token folder (see the desktop
//!   session's `renpy` module for the lookup order).
//! - `renpy/savetoken.py`: `decode_line` splits a line with `split(None, 2)` and decodes the second
//!   and third field with `base64.b64decode`; a line with a field that does not decode is skipped.
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

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(<redacted>)")
    }
}

impl Key {
    /// The SPKI DER of this key's public half, the first field of a `signature` line.
    fn public_der(&self) -> Vec<u8> {
        spki_der(self.signing.verifying_key())
    }
}

/// The SPKI DER (91 bytes) of a Pascal P-256 public key, in the layout Ren'Py writes.
fn spki_der(key: &VerifyingKey) -> Vec<u8> {
    let point = key.to_sec1_point(false);
    let mut der = SPKI_PREFIX.to_vec();
    // `to_sec1_point(false)` starts with 0x04, which the prefix already holds.
    der.extend_from_slice(point.as_bytes().get(1..).unwrap_or_default());
    der
}

/// The value of one base64 character, `None` for anything outside the standard alphabet.
fn sextet(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// The low byte of `value`.
fn low_byte(value: u32) -> u8 {
    u8::try_from(value & 0xff).unwrap_or_default()
}

/// Decodes base64 the way Python's `base64.b64decode(text)` does, which is what Ren'Py's
/// `decode_line` calls: text with a non-ASCII character is refused; other characters outside the
/// alphabet (spaces, line breaks, stray symbols) are skipped; a `=` before the second character of
/// a group is skipped; the padding that completes a group ends the data (the rest is ignored);
/// leftover bits are dropped. `None` when the characters do not fill the last group and no padding
/// completes it (Python's "incorrect padding").
fn decode_lenient(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() {
        return None;
    }
    let mut out = Vec::new();
    let mut bits: u32 = 0;
    let mut count: u32 = 0;
    let mut pads: u32 = 0;
    for byte in text.bytes() {
        if byte == b'=' {
            if count < 2 {
                continue;
            }
            pads += 1;
            if count + pads >= 4 {
                match count {
                    2 => out.push(low_byte(bits >> 4)),
                    _ => {
                        out.push(low_byte(bits >> 10));
                        out.push(low_byte(bits >> 2));
                    }
                }
                return Some(out);
            }
            continue;
        }
        let Some(value) = sextet(byte) else {
            continue;
        };
        pads = 0;
        bits = (bits << 6) | value;
        count += 1;
        if count == 4 {
            out.push(low_byte(bits >> 16));
            out.push(low_byte(bits >> 8));
            out.push(low_byte(bits));
            bits = 0;
            count = 0;
        }
    }
    (count == 0).then_some(out)
}

/// Splits a key file line the way `decode_line` does: strip it, then `split(None, 2)`. Gives the
/// kind, the second field and, when there is one, the rest of the line as the third field. `None`
/// for a line that is empty, a comment, or has a single word.
fn split_fields(line: &str) -> Option<(&str, &str, Option<&str>)> {
    let line = line.trim();
    if line.starts_with('#') {
        return None;
    }
    let (kind, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    Some(match rest.split_once(char::is_whitespace) {
        Some((second, third)) => (kind, second, Some(third.trim_start())),
        None => (kind, rest, None),
    })
}

/// Reads the `signing-key` lines of a `security_keys.txt`; malformed and `verifying-key` lines are
/// skipped, as Ren'Py's `load_tokens` does. Each line is split and base64-decoded like
/// `savetoken.decode_line`: the third field is decoded too, and a line whose field does not decode
/// is skipped. Never fails: an unreadable line is simply not a key.
pub fn parse_key_file(text: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    // Python reads the file with universal newlines, so a lone carriage return ends a line too.
    for line in text.split(['\n', '\r']) {
        if keys.len() >= MAX_KEYS {
            break;
        }
        let Some((kind, second, third)) = split_fields(line) else {
            continue;
        };
        if kind != "signing-key" {
            continue;
        }
        let Some(der) = decode_lenient(second) else {
            continue;
        };
        if third.is_some_and(|third| decode_lenient(third).is_none()) {
            continue;
        }
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
        let point = signing.verifying_key().to_sec1_point(false);
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

    #[test]
    fn base64_is_decoded_as_leniently_as_python_does() {
        let ok = |text: &str| decode_lenient(text);
        assert_eq!(ok(""), Some(Vec::new()));
        assert_eq!(ok("QUJD"), Some(b"ABC".to_vec()));
        assert_eq!(ok("QUI="), Some(b"AB".to_vec()));
        assert_eq!(ok("QQ=="), Some(b"A".to_vec()));
        // Spaces, line breaks and stray symbols are skipped.
        assert_eq!(ok(" QU\tJD\r\n"), Some(b"ABC".to_vec()));
        assert_eq!(ok("Q*UJ-D"), Some(b"ABC".to_vec()));
        // A `=` before the second character of a group is skipped.
        assert_eq!(ok("=QQ=="), Some(b"A".to_vec()));
        // Padding that completes a group ends the data; leftover bits need not be zero.
        assert_eq!(ok("QQ==QUJD"), Some(b"A".to_vec()));
        assert_eq!(ok("QR=="), Some(b"A".to_vec()));
        // Missing padding, or one character too many, is Python's "incorrect padding".
        assert_eq!(ok("QQ"), None);
        assert_eq!(ok("QUI"), None);
        assert_eq!(ok("QUJDQ"), None);
        // `b64decode` refuses a str with a non-ASCII character.
        assert_eq!(ok("QUJD\u{e9}"), None);
    }

    #[test]
    fn key_lines_are_split_and_decoded_like_decode_line() {
        let signing = signing_key(5);
        let line = key_line(&signing);
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (der, spki) = (fields.get(1).unwrap(), fields.get(2).unwrap());
        let parse = |text: &str| parse_key_file(text).len();

        // A stray symbol inside the key is ignored, as `base64.b64decode` does.
        assert_eq!(parse(&format!("signing-key *{der} {spki}\n")), 1);
        // The third field is the rest of the line; its spaces are skipped like any other.
        let (left, right) = spki.split_at(8);
        assert_eq!(parse(&format!("signing-key {der} {left} {right}\n")), 1);
        // The third field is optional, but when it is there it must decode.
        assert_eq!(parse(&format!("signing-key {der}\n")), 1);
        assert_eq!(parse(&format!("signing-key {der} QUJDQ\n")), 0);
        assert_eq!(parse(&format!("signing-key {der} {spki}\u{e9}\n")), 0);
        // A key that does not decode, a one-word line and a comment are skipped.
        assert_eq!(parse(&format!("signing-key QUJDQ {spki}\n")), 0);
        assert_eq!(parse("signing-key\n"), 0);
        assert_eq!(parse(&format!("# signing-key {der} {spki}\n")), 0);
        // A lone carriage return ends a line, as it does when Python reads the file.
        let two = format!("{}\r{}", line.trim_end(), key_line(&signing_key(6)));
        assert_eq!(parse(&two), 2);
    }
}
