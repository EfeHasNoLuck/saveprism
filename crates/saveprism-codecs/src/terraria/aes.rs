//! `crypto.terraria-plr`: the AES-CBC encryption of a Terraria player file.
//!
//! Format: `AES-128-CBC(key = utf16le(key text), iv = utf16le(key text), NoPadding)(plaintext)`;
//! the key text is exactly 8 UTF-16 code units, so the key and the IV are both 16 bytes. The
//! game's own writer pads with PKCS7 (the .NET default), so a file ends in 1 to 16 padding bytes;
//! its reader uses no padding and stops after the player data, so bytes behind the data are
//! ignored. This layer therefore decrypts and encrypts without padding and only needs whole
//! blocks: the zero padding the player codec adds after an edit is accepted by that reader. The
//! key never appears in the parameters this layer returns; `decode` reads it from the recipe
//! hint `password` and `encode` from the parameters it is given, exactly like `crypto.es3-aes`.

use ::aes::Aes128;
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx};

use super::{TERRARIA_BAD_DATA, TERRARIA_PASSWORD_REQUIRED, TERRARIA_WRONG_PASSWORD};

const BLOCK: usize = 16;
/// Highest player version this crate accepts as plausible (the current release is below 400).
const MAX_PLAUSIBLE_VERSION: i32 = 1000;
/// Lowest version with the Re-Logic header that the first-block check understands.
const MIN_HEADER_VERSION: i32 = 135;

/// `crypto.terraria-plr`.
///
/// Hint on decode: `password` (text). Parameter on encode: `password` (text).
#[derive(Clone, Copy, Debug, Default)]
pub struct TerrariaPlayerLayer;

/// The UTF-16LE bytes of the key text, used as both AES key and IV. `None` unless the text is
/// exactly 8 UTF-16 code units (16 bytes), the only length the game uses.
fn key_bytes(password: &str) -> Option<[u8; BLOCK]> {
    let bytes: Vec<u8> = password.encode_utf16().flat_map(u16::to_le_bytes).collect();
    <[u8; BLOCK]>::try_from(bytes.as_slice()).ok()
}

fn bad_data(message: &str) -> Error {
    Error::new(
        TERRARIA_BAD_DATA,
        format!(
            "{message}. A Terraria player save is whole 16 byte AES blocks; the file may be cut short."
        ),
    )
}

fn wrong_key_length() -> Error {
    Error::new(
        TERRARIA_WRONG_PASSWORD,
        "this key has the wrong length for a Terraria player save (it is exactly 8 characters). The file is encrypted with the key from the game's own Terraria.exe; pick the game folder and try again.",
    )
}

fn wrong_password() -> Error {
    Error::new(
        TERRARIA_WRONG_PASSWORD,
        "the key did not decrypt this Terraria player save. The file is encrypted with the key from the game's own Terraria.exe; pick the game folder and try again.",
    )
}

/// True when `plain` starts like a decrypted Terraria player save: a plausible version and, for
/// version 135 and later, the Re-Logic `relogic` header with file type 3.
fn plausible_header(plain: &[u8]) -> bool {
    let version = match plain.get(..4) {
        Some(bytes) => match <[u8; 4]>::try_from(bytes) {
            Ok(array) => i32::from_le_bytes(array),
            Err(_) => return false,
        },
        None => return false,
    };
    if !(MIN_HEADER_VERSION..=MAX_PLAUSIBLE_VERSION).contains(&version) {
        return false;
    }
    let header = match plain.get(4..12) {
        Some(bytes) => match <[u8; 8]>::try_from(bytes) {
            Ok(array) => u64::from_le_bytes(array),
            Err(_) => return false,
        },
        None => return false,
    };
    super::relogic_header(header, super::FILE_TYPE_PLAYER)
}

/// True when `password` decrypts the first block of `input` into a plausible player header. It is
/// a cheap pre-check before trying many candidate keys; `true` is not proof that the key is right.
pub fn player_password_unlocks_first_block(input: &[u8], password: &str) -> bool {
    let first = match input.get(..BLOCK) {
        Some(bytes) => bytes,
        None => return false,
    };
    let Some(key) = key_bytes(password) else {
        return false;
    };
    let mut block = [0_u8; BLOCK];
    block.copy_from_slice(first);
    decrypt_block(&key, &mut block) && plausible_header(&block)
}

/// Decrypts one block in place, ignoring padding.
fn decrypt_block(key: &[u8; BLOCK], block: &mut [u8; BLOCK]) -> bool {
    let decryptor = match cbc::Decryptor::<Aes128>::new_from_slices(key, key) {
        Ok(decryptor) => decryptor,
        Err(_) => return false,
    };
    decryptor.decrypt_padded::<NoPadding>(block).is_ok()
}

/// Rough "looks random" test: at least half the byte values appear in the window.
fn high_entropy(input: &[u8]) -> bool {
    let window = input.get(..256).unwrap_or(input);
    let mut seen = [false; 256];
    for &byte in window {
        if let Some(slot) = seen.get_mut(usize::from(byte)) {
            *slot = true;
        }
    }
    let distinct = seen.iter().filter(|&&flag| flag).count();
    distinct.saturating_mul(2) >= window.len()
}

impl Layer for TerrariaPlayerLayer {
    fn id(&self) -> &'static str {
        "crypto.terraria-plr"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named = ctx.file_name.is_some_and(super::is_player_file_name);
        let sized = input.len() >= 2 * BLOCK && input.len().is_multiple_of(BLOCK);
        // It cannot be proven without the key, and it must never claim another format's file.
        if named && sized && high_entropy(input) {
            Confidence::LOW
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let password = ctx.hints().opt_str("password")?.ok_or_else(|| {
            Error::new(
                TERRARIA_PASSWORD_REQUIRED,
                "this Terraria player save is encrypted and needs the key from the game's own Terraria.exe. Pick the game folder and open the file again.",
            )
        })?;
        let key = key_bytes(password).ok_or_else(wrong_key_length)?;
        if input.is_empty() || !input.len().is_multiple_of(BLOCK) {
            return Err(bad_data(
                "the encrypted part is not a whole number of blocks",
            ));
        }
        let mut buffer = input.to_vec();
        let plain = decrypt(&key, &mut buffer)?;
        if !plausible_header(&plain) {
            return Err(wrong_password());
        }
        Ok((plain, Params::new()))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let key = key_bytes(params.str("password")?).ok_or_else(wrong_key_length)?;
        if inner.is_empty() || !inner.len().is_multiple_of(BLOCK) {
            return Err(bad_data(
                "the plaintext is not a whole number of 16 byte blocks",
            ));
        }
        let mut buffer = inner.to_vec();
        encrypt(&key, &mut buffer)
    }
}

fn decrypt(key: &[u8; BLOCK], buffer: &mut [u8]) -> Result<Vec<u8>> {
    let decryptor =
        cbc::Decryptor::<Aes128>::new_from_slices(key, key).map_err(|_| wrong_key_length())?;
    let plain = decryptor
        .decrypt_padded::<NoPadding>(buffer)
        .map_err(|_| bad_data("the ciphertext is not whole blocks"))?;
    Ok(plain.to_vec())
}

fn encrypt(key: &[u8; BLOCK], buffer: &mut [u8]) -> Result<Vec<u8>> {
    let encryptor =
        cbc::Encryptor::<Aes128>::new_from_slices(key, key).map_err(|_| wrong_key_length())?;
    let cipher_text = encryptor
        .encrypt_padded::<NoPadding>(buffer, buffer.len())
        .map_err(|_| bad_data("the plaintext could not be encrypted"))?;
    Ok(cipher_text.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cheap_check_tells_the_right_key_from_a_wrong_one() {
        let mut plain = vec![0_u8; BLOCK];
        let version = 317_i32.to_le_bytes();
        plain[..4].copy_from_slice(&version);
        let header =
            (u64::from(super::super::FILE_TYPE_PLAYER) << 56) | super::super::RELOGIC_MAGIC;
        plain[4..12].copy_from_slice(&header.to_le_bytes());
        let mut params = Params::new();
        params.set("password", "TestKey1");
        let encrypted = TerrariaPlayerLayer.encode(&plain, &params).unwrap();
        assert!(player_password_unlocks_first_block(&encrypted, "TestKey1"));
        assert!(!player_password_unlocks_first_block(&encrypted, "WrongKey"));
        assert!(!player_password_unlocks_first_block(
            &encrypted[..8],
            "TestKey1"
        ));
    }
}
