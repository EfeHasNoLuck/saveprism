//! `crypto.terraria-plr`: the AES-CBC encryption of a Terraria player file.
//!
//! Format: `AES-CBC(key = utf16le(key text), iv = utf16le(key text), NoPadding)(plaintext)`,
//! where the plaintext is zero-padded by the game to a whole 16 byte block. The key never
//! appears in the parameters this layer returns; `decode` reads it from the recipe hint
//! `password` and `encode` from the parameters it is given, exactly like `crypto.es3-aes`.

use ::aes::{Aes128, Aes192, Aes256};
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use saveprism_core::{Confidence, DecodeCtx, Error, Layer, Params, Result, SniffCtx, codes};

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

/// The UTF-16LE bytes of the key text, used as both AES key and IV.
fn key_bytes(password: &str) -> Vec<u8> {
    password.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn bad_data(message: &str) -> Error {
    Error::new(
        TERRARIA_BAD_DATA,
        format!(
            "{message}. A Terraria player save is whole 16 byte AES blocks; the file may be cut short."
        ),
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
    let key = key_bytes(password);
    let mut block = [0_u8; BLOCK];
    block.copy_from_slice(first);
    let decrypted = match key.len() {
        16 => decrypt_block::<Aes128>(&key, &mut block),
        24 => decrypt_block::<Aes192>(&key, &mut block),
        32 => decrypt_block::<Aes256>(&key, &mut block),
        _ => false,
    };
    decrypted && plausible_header(&block)
}

/// Decrypts one block in place, ignoring padding.
fn decrypt_block<C>(key: &[u8], block: &mut [u8; BLOCK]) -> bool
where
    C: cbc::cipher::BlockCipherDecrypt + cbc::cipher::KeyInit + cbc::cipher::BlockSizeUser,
{
    let decryptor = match cbc::Decryptor::<C>::new_from_slices(key, key) {
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
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".plr"));
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
        if input.is_empty() || !input.len().is_multiple_of(BLOCK) {
            return Err(bad_data(
                "the encrypted part is not a whole number of blocks",
            ));
        }
        let key = key_bytes(password);
        let mut buffer = input.to_vec();
        let plain = match key.len() {
            16 => decrypt::<Aes128>(&key, &mut buffer),
            24 => decrypt::<Aes192>(&key, &mut buffer),
            32 => decrypt::<Aes256>(&key, &mut buffer),
            _ => {
                return Err(Error::new(
                    TERRARIA_WRONG_PASSWORD,
                    "this key has the wrong length for a Terraria player save",
                ));
            }
        }?;
        if !plausible_header(&plain) {
            return Err(wrong_password());
        }
        Ok((plain, Params::new()))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let password = params.str("password")?;
        if inner.is_empty() || !inner.len().is_multiple_of(BLOCK) {
            return Err(bad_data(
                "the plaintext is not a whole number of 16 byte blocks",
            ));
        }
        let key = key_bytes(password);
        let mut buffer = inner.to_vec();
        match key.len() {
            16 => encrypt::<Aes128>(&key, &mut buffer),
            24 => encrypt::<Aes192>(&key, &mut buffer),
            32 => encrypt::<Aes256>(&key, &mut buffer),
            _ => Err(Error::new(
                codes::CORE_INVALID_PARAM,
                "the Terraria key must be 8, 12 or 16 characters (16, 24 or 32 UTF-16LE bytes)",
            )),
        }
    }
}

fn decrypt<C>(key: &[u8], buffer: &mut [u8]) -> Result<Vec<u8>>
where
    C: cbc::cipher::BlockCipherDecrypt + cbc::cipher::KeyInit + cbc::cipher::BlockSizeUser,
{
    let decryptor = cbc::Decryptor::<C>::new_from_slices(key, key)
        .map_err(|_| bad_data("the key has the wrong length"))?;
    let plain = decryptor
        .decrypt_padded::<NoPadding>(buffer)
        .map_err(|_| bad_data("the ciphertext is not whole blocks"))?;
    Ok(plain.to_vec())
}

fn encrypt<C>(key: &[u8], buffer: &mut [u8]) -> Result<Vec<u8>>
where
    C: cbc::cipher::BlockCipherEncrypt + cbc::cipher::KeyInit + cbc::cipher::BlockSizeUser,
{
    let encryptor = cbc::Encryptor::<C>::new_from_slices(key, key)
        .map_err(|_| bad_data("the key has the wrong length"))?;
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
