//! `crypto.es3-aes`: the Easy Save 3 encryption layer.
//!
//! File layout: `iv[16] || AES-128-CBC(key, iv, PKCS7)(payload)` with
//! `key = PBKDF2-HMAC-SHA1(password as UTF-8, salt = iv, 100 rounds, 16 bytes)`.
//!
//! The password is never part of the parameters this layer returns. `decode` reads it from the
//! recipe hints (`password`); `encode` reads it from the parameters it is given, so the caller must
//! add `password` to the step before encoding. The returned parameters only hold `iv`, which makes
//! re-encoding byte-exact.

use ::aes::Aes128;
use cbc::cipher::block_padding::{NoPadding, Pkcs7};
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx, codes};

use super::{ES3_BAD_DATA, ES3_PASSWORD_REQUIRED, ES3_WRONG_PASSWORD};

const IV_LEN: usize = 16;
const BLOCK: usize = 16;
const ROUNDS: u32 = 100;
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
/// How many bytes of the payload the wrong-password check and the entropy check look at.
const CHECK_WINDOW: usize = 256;

/// `crypto.es3-aes`.
///
/// Hint on decode: `password` (text). Parameter on encode: `iv` (16 bytes) and `password` (text).
#[derive(Clone, Copy, Debug, Default)]
pub struct Es3AesLayer;

fn derive_key(password: &str, iv: &[u8]) -> [u8; 16] {
    let mut key = [0_u8; 16];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), iv, ROUNDS, &mut key);
    key
}

fn bad_data(message: &str) -> Error {
    Error::new(
        ES3_BAD_DATA,
        format!(
            "{message}. An Easy Save 3 encrypted file is a 16 byte IV followed by whole 16 byte blocks; the file may be cut short or not encrypted. Open it without the encryption step if it is plain text."
        ),
    )
}

fn wrong_password() -> Error {
    Error::new(
        ES3_WRONG_PASSWORD,
        "wrong password, or the file is not Easy Save 3 encrypted. Check the password in the game's Easy Save settings and try again.",
    )
}

/// True when `password` decrypts the first block of `input` into bytes that could start Easy Save 3
/// content. It is a cheap pre-check for trying many candidate passwords before a full decode; a
/// `true` here is not proof that the password is right.
pub fn password_unlocks_first_block(input: &[u8], password: &str) -> bool {
    let Some((iv, cipher_text)) = input.split_at_checked(IV_LEN) else {
        return false;
    };
    let Some(first) = cipher_text.get(..BLOCK) else {
        return false;
    };
    let key = derive_key(password, iv);
    let Ok(decryptor) = cbc::Decryptor::<Aes128>::new_from_slices(&key, iv) else {
        return false;
    };
    let mut block = [0_u8; BLOCK];
    block.copy_from_slice(first);
    decryptor
        .decrypt_padded::<NoPadding>(&mut block)
        .is_ok_and(plausible)
}

/// True when the decrypted payload looks like Easy Save 3 content: JSON text or a gzip stream.
fn plausible(plain: &[u8]) -> bool {
    if plain.starts_with(&[0x1F, 0x8B]) {
        return true;
    }
    let body = plain.strip_prefix(&BOM[..]).unwrap_or(plain);
    if !matches!(body.first(), Some(b'{' | b'[' | b'"')) {
        return false;
    }
    let window = body.get(..CHECK_WINDOW).unwrap_or(body);
    let text = match std::str::from_utf8(window) {
        Ok(text) => text,
        // A multi-byte character cut by the window end is fine.
        Err(error) if error.error_len().is_none() => {
            match std::str::from_utf8(window.get(..error.valid_up_to()).unwrap_or_default()) {
                Ok(text) => text,
                Err(_) => return false,
            }
        }
        Err(_) => return false,
    };
    let total = text.chars().count();
    let control = text
        .chars()
        .filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        .count();
    control.saturating_mul(50) <= total
}

/// Rough "looks random" test used by `sniff`: many distinct byte values in the first bytes.
fn high_entropy(input: &[u8]) -> bool {
    let window = input.get(..CHECK_WINDOW).unwrap_or(input);
    let mut seen = [false; 256];
    for &byte in window {
        if let Some(slot) = seen.get_mut(usize::from(byte)) {
            *slot = true;
        }
    }
    let distinct = seen.iter().filter(|&&flag| flag).count();
    distinct.saturating_mul(2) >= window.len()
}

impl Layer for Es3AesLayer {
    fn id(&self) -> &'static str {
        "crypto.es3-aes"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        let sized = input.len() >= 32 && input.len().saturating_sub(IV_LEN).is_multiple_of(BLOCK);
        // Cannot be proven without the password, so the confidence stays low.
        if sized && high_entropy(input) {
            Confidence::LOW
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let password = ctx.hints().opt_str("password")?.ok_or_else(|| {
            Error::new(
                ES3_PASSWORD_REQUIRED,
                "this file is Easy Save 3 encrypted and needs a password. Enter the password used by the game and open the file again.",
            )
        })?;
        let (iv, cipher_text) = input
            .split_at_checked(IV_LEN)
            .ok_or_else(|| bad_data("the file is shorter than the 16 byte IV"))?;
        if cipher_text.is_empty() || cipher_text.len() % BLOCK != 0 {
            return Err(bad_data(
                "the encrypted part is not a whole number of blocks",
            ));
        }
        let key = derive_key(password, iv);
        let decryptor = cbc::Decryptor::<Aes128>::new_from_slices(&key, iv)
            .map_err(|_| bad_data("the key or IV has the wrong length"))?;
        let mut buffer = cipher_text.to_vec();
        let plain = decryptor
            .decrypt_padded::<Pkcs7>(&mut buffer)
            .map_err(|_| wrong_password())?
            .to_vec();
        if !plausible(&plain) {
            return Err(wrong_password());
        }
        let mut params = Params::new();
        params.set("iv", iv.to_vec());
        Ok((plain, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let password = params.str("password")?;
        let iv = params.bytes("iv")?;
        if iv.len() != IV_LEN {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                "parameter \"iv\" must be 16 bytes",
            ));
        }
        let key = derive_key(password, iv);
        let encryptor = cbc::Encryptor::<Aes128>::new_from_slices(&key, iv)
            .map_err(|_| bad_data("the key or IV has the wrong length"))?;
        let padded_len = (inner.len() / BLOCK)
            .checked_add(1)
            .and_then(|blocks| blocks.checked_mul(BLOCK))
            .ok_or_else(|| bad_data("the data is too large to encrypt"))?;
        let mut buffer = inner.to_vec();
        buffer.resize(padded_len, 0);
        let cipher_text = encryptor
            .encrypt_padded::<Pkcs7>(&mut buffer, inner.len())
            .map_err(|_| bad_data("the data could not be padded"))?;
        let mut out = Vec::with_capacity(IV_LEN + cipher_text.len());
        out.extend_from_slice(iv);
        out.extend_from_slice(cipher_text);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cheap_check_tells_the_right_password_from_a_wrong_one() {
        let mut params = Params::new();
        params.set("password", "GameSecret42");
        params.set("iv", vec![7_u8; 16]);
        let plain = br#"{"slot":{"__type":"int","value":3}}"#;
        let encrypted = Es3AesLayer.encode(plain, &params).unwrap();
        assert!(password_unlocks_first_block(&encrypted, "GameSecret42"));
        assert!(!password_unlocks_first_block(&encrypted, "not-the-key"));
        // Too short to hold an IV and a block.
        assert!(!password_unlocks_first_block(
            &encrypted[..20],
            "GameSecret42"
        ));
    }
}
