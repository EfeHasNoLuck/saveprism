//! The simple XOR obfuscation layer: a single-byte or repeating key of 1 to 32 bytes.
//!
//! There is no header and no checksum, so the key is recovered from the expected start of the
//! inner file (known plaintext: JSON (RFC 8259), XML, zip, gzip (RFC 1952), binary plist, SQLite
//! (`https://www.sqlite.org/fileformat.html`, `SQLite format 3\0`), GVAS, Ruby Marshal, pickle
//! and the common compression magics) and from the most frequent byte of each key column (many
//! games XOR text with a short key, so the modal ciphertext byte maps to the modal plaintext
//! byte). A candidate key is accepted only when the whole decoded file is recognised by a codec or
//! layer at medium confidence or better, and a file that is already a known format is never
//! claimed as XOR.
//!
//! The parameter `key` holds the key bytes; encoding applies the same XOR, so an unchanged file is
//! written back exactly.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, Layer, Limits, Params, Registry, Result, SniffCtx,
};

use super::{LAY_XOR_INVALID, inner_is_known};

/// Longest key that is tried.
pub(crate) const MAX_KEY_LEN: usize = 32;
/// Largest input for which the frequency search is attempted.
const MAX_FREQUENCY_BYTES: usize = 1024 * 1024;

/// `obfuscation.xor`.
///
/// Parameters: `key` (1 to 32 bytes).
#[derive(Clone, Copy, Debug, Default)]
pub struct XorLayer;

/// Known plaintext at the start of an inner format. Every entry is a fixed byte sequence of the
/// public format (JSON/XML markers, zip and gzip magics, `bplist00`, `SQLite format 3\0`, `GVAS`,
/// Ruby Marshal `\x04\x08`, pickle `\x80`, the bzip2/zstd/xz/LZ4 magics and the LZF `ZV` frame).
const KNOWN_STARTS: &[&[u8]] = &[
    b"{\"",
    b"{\n",
    b"{",
    b"[",
    b"<?xml",
    b"PK\x03\x04",
    b"\x1f\x8b\x08",
    b"\x1f\x8b\x08\x00",
    b"\x1f\x8b\x08\x08",
    b"\x1f\x8b\x08\x04",
    b"bplist00",
    b"SQLite format 3\0",
    b"GVAS",
    b"\x04\x08",
    b"\x80",
    b"BZh",
    b"\x28\xb5\x2f\xfd",
    b"\xfd7zXZ\0",
    b"\x04\x22\x4d\x18",
    b"ZV",
];

/// Plaintext bytes a modal ciphertext byte may stand for (space, quote, comma, colon, newline,
/// digit, braces and a zero for binary payloads).
const MODAL_PLAINTEXT: &[u8] = &[b' ', b'"', b',', b':', b'\n', b'0', b'{', b'[', 0];

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LAY_XOR_INVALID, message).with_offset(len_u64(offset))
}

/// XORs `data` with `key`, which repeats over the data.
fn xor(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    data.iter()
        .zip(key.iter().cycle())
        .map(|(byte, key_byte)| byte ^ key_byte)
        .collect()
}

/// True when `data` starts with one of the known plaintext markers.
fn starts_known(data: &[u8]) -> bool {
    KNOWN_STARTS.iter().any(|start| data.starts_with(start))
}

/// True when `data` is recognised by a codec or a layer at medium confidence or better and the
/// detector can actually decode it. A bare sniffer match is not enough: the JSON sniffer, for
/// example, also accepts text that only starts with `{` and ends with `}`, and the base64 sniffer
/// accepts any short run of its alphabet (so it is not evidence of a real inner format here).
fn recognized(data: &[u8], original: &[u8]) -> bool {
    if data.is_empty() || data == original || !starts_known(data) || !strongly_known(data) {
        return false;
    }
    decodable(data)
}

/// Like [`inner_is_known`], but without the base64 text guess, the loose text codecs and XOR
/// itself.
fn strongly_known(data: &[u8]) -> bool {
    thread_local! {
        static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    BUSY.with(|busy| {
        if busy.replace(true) {
            return false;
        }
        let result = strongly_known_inner(data);
        busy.set(false);
        result
    })
}

fn strongly_known_inner(data: &[u8]) -> bool {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let ctx = SniffCtx::default();
    reg.codecs()
        .filter(|codec| {
            !matches!(
                codec.id(),
                "format.csv" | "format.ini" | "format.properties" | "format.toml" | "format.yaml"
            )
        })
        .any(|codec| codec.sniff(data, &ctx) >= Confidence::MEDIUM)
        || reg
            .layers()
            .filter(|layer| {
                !matches!(
                    layer.id(),
                    "obfuscation.xor" | "encoding.base64" | "encoding.hex" | "encoding.percent"
                )
            })
            .any(|layer| layer.sniff(data, &ctx) >= Confidence::MEDIUM)
}

/// True when a codec or layer that recognises `data` at medium confidence or better can really
/// decode it. A sniff alone is not proof (the plist sniffer only checks `bplist00`, and repeated
/// ciphertext can carry that marker without being a plist).
fn decodable(data: &[u8]) -> bool {
    let mut reg = Registry::default();
    crate::register_all(&mut reg);
    let limits = Limits::default();
    let ctx = SniffCtx::default();
    let candidates = reg
        .codecs()
        .filter(|codec| {
            !matches!(
                codec.id(),
                "format.csv" | "format.ini" | "format.properties" | "format.toml" | "format.yaml"
            ) && codec.sniff(data, &ctx) >= Confidence::MEDIUM
        })
        .any(|codec| codec.decode(data, &mut DecodeCtx::new(&limits)).is_ok());
    if candidates {
        return true;
    }
    reg.layers()
        .filter(|layer| {
            !matches!(
                layer.id(),
                "obfuscation.xor" | "encoding.base64" | "encoding.hex" | "encoding.percent"
            ) && layer.sniff(data, &ctx) >= Confidence::MEDIUM
        })
        .any(|layer| layer.decode(data, &mut DecodeCtx::new(&limits)).is_ok())
}

/// Tries the known-plaintext keys for one key length. A known start at least as long as the key
/// supplies every key byte (SQLite's 16-byte header covers a 13-byte key); a start shorter than the
/// key cannot.
fn known_keys(input: &[u8], len: usize, out: &mut Vec<Vec<u8>>) {
    for start in KNOWN_STARTS {
        if start.len() < len || input.len() < len {
            continue;
        }
        let mut key = Vec::with_capacity(len);
        for index in 0..len {
            let (Some(&byte), Some(&plain)) = (input.get(index), start.get(index)) else {
                continue;
            };
            key.push(byte ^ plain);
        }
        if key.len() == len {
            out.push(key);
        }
    }
}

/// The most frequent byte in every key column.
fn modal_key(input: &[u8], len: usize) -> Vec<u8> {
    let mut key = Vec::with_capacity(len);
    for column in 0..len {
        let mut counts = [0_u32; 256];
        for &byte in input.iter().skip(column).step_by(len) {
            if let Some(count) = counts.get_mut(usize::from(byte)) {
                *count = count.saturating_add(1);
            }
        }
        let mut best = 0_u8;
        let mut best_count = 0_u32;
        for (value, &count) in counts.iter().enumerate() {
            if count > best_count {
                best_count = count;
                best = u8::try_from(value).unwrap_or(0);
            }
        }
        key.push(best);
    }
    key
}

/// Recovers a key of 1 to 32 bytes from `input`, or `None` when nothing verifies.
pub(crate) fn break_key(input: &[u8]) -> Option<Vec<u8>> {
    if input.len() < 3 || inner_is_known(input) {
        return None;
    }
    let mut candidates: Vec<Vec<u8>> = Vec::new();
    for len in 1..=MAX_KEY_LEN {
        if len > input.len() {
            break;
        }
        known_keys(input, len, &mut candidates);
    }
    for candidate in candidates {
        if candidate.iter().any(|&byte| byte != 0) && recognized(&xor(input, &candidate), input) {
            return Some(candidate);
        }
    }
    if input.len() > MAX_FREQUENCY_BYTES {
        return None;
    }
    for len in 1..=MAX_KEY_LEN.min(input.len()) {
        let modal = modal_key(input, len);
        for &plain in MODAL_PLAINTEXT {
            let candidate: Vec<u8> = modal.iter().map(|&byte| byte ^ plain).collect();
            if candidate.iter().any(|&byte| byte != 0) && recognized(&xor(input, &candidate), input)
            {
                return Some(candidate);
            }
        }
    }
    None
}

impl Layer for XorLayer {
    fn id(&self) -> &'static str {
        "obfuscation.xor"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        match break_key(input) {
            Some(_) => Confidence::new(0.45),
            None => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let key = match ctx.hints().opt_bytes("key")? {
            Some(key) if !key.is_empty() => key.to_vec(),
            _ => break_key(input)
                .ok_or_else(|| bad("no XOR key reproduces a format SavePrism knows", 0))?,
        };
        if key.len() > MAX_KEY_LEN {
            return Err(bad(
                format!("the XOR key is longer than {MAX_KEY_LEN} bytes"),
                0,
            ));
        }
        if key.iter().all(|&byte| byte == 0) {
            return Err(bad("an all-zero XOR key is the identity transform", 0));
        }
        let inner = xor(input, &key);
        if !inner_is_known(&inner) {
            return Err(bad(
                "the XOR key does not reproduce a format SavePrism knows",
                0,
            ));
        }
        ctx.limits()
            .check_output(len_u64(input.len()), len_u64(inner.len()))?;
        let mut params = Params::new();
        params.set("key", key);
        Ok((inner, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let key = params.bytes("key")?;
        if key.is_empty() {
            return Err(super::invalid_param(
                "parameter \"key\" must not be empty".to_string(),
            ));
        }
        if key.len() > MAX_KEY_LEN {
            return Err(super::invalid_param(format!(
                "parameter \"key\" is longer than {MAX_KEY_LEN} bytes"
            )));
        }
        if key.iter().all(|&byte| byte == 0) {
            return Err(super::invalid_param(
                "parameter \"key\" must not be all zero".to_string(),
            ));
        }
        Ok(xor(inner, key))
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::Limits;

    use super::*;

    const JSON: &[u8] = b"{\"player\":{\"name\":\"Ada\",\"level\":12,\"gold\":340},\"day\":3}";

    fn decode(input: &[u8]) -> Result<(Vec<u8>, Params)> {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        XorLayer.decode(input, &mut ctx)
    }

    #[test]
    fn single_byte_key_around_json_is_broken_and_round_trips() {
        let packed = xor(JSON, &[0x5A]);
        let (inner, params) = decode(&packed).unwrap();
        assert_eq!(inner, JSON);
        assert_eq!(params.bytes("key").unwrap(), [0x5A]);
        assert_eq!(XorLayer.encode(&inner, &params).unwrap(), packed);
    }

    #[test]
    fn repeating_key_is_broken_from_the_known_start() {
        let packed = xor(JSON, b"ke");
        let (inner, params) = decode(&packed).unwrap();
        assert_eq!(inner, JSON);
        assert_eq!(params.bytes("key").unwrap(), b"ke");
    }

    #[test]
    fn plain_json_is_never_claimed() {
        assert_eq!(XorLayer.sniff(JSON, &SniffCtx::default()), Confidence::NONE);
        assert_eq!(decode(JSON).unwrap_err().code(), LAY_XOR_INVALID);
    }

    #[test]
    fn all_zero_key_is_refused() {
        let mut params = Params::new();
        params.set("key", vec![0_u8, 0, 0]);
        assert_eq!(
            XorLayer.encode(JSON, &params).unwrap_err().code(),
            saveprism_core::codes::CORE_INVALID_PARAM
        );
        let zeroed = xor(JSON, &[0u8]);
        assert_eq!(decode(&zeroed).unwrap_err().code(), LAY_XOR_INVALID);
    }

    #[test]
    fn key_length_is_bounded() {
        let mut params = Params::new();
        params.set("key", vec![1_u8; MAX_KEY_LEN + 1]);
        assert_eq!(
            XorLayer.encode(JSON, &params).unwrap_err().code(),
            saveprism_core::codes::CORE_INVALID_PARAM
        );
    }

    #[test]
    fn random_bytes_are_not_claimed() {
        let mut state: u32 = 0x1234_5678;
        let mut input = vec![0xFF_u8];
        for _ in 0..512 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            input.push(state.to_le_bytes()[3]);
        }
        assert_eq!(
            XorLayer.sniff(&input, &SniffCtx::default()),
            Confidence::NONE
        );
    }
}
