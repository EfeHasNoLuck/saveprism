//! Palworld save container: `container.palworld-plz`.
//!
//! Layout (little endian): `uncompressed_size u32 · compressed_size u32 · "PlZ" · kind u8 · zlib data`.
//! Kind `0x31` is one zlib stream; kind `0x32` is a zlib stream whose output is another zlib stream.
//! `compressed_size` is the size of the first compression's output: the payload length for `0x31`,
//! the length of the inner zlib stream for `0x32`. The bytes inside are a GVAS file, opened by
//! `format.gvas`. The actual zlib work is done by [`ZlibLayer`], which keeps its own parameters.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Confidence, DecodeCtx, Error, ErrorCode, Layer, ParamValue, Params, Registry, Result, SniffCtx,
};

use crate::layers::ZlibLayer;

/// The file uses Oodle compression (`PlM`), which is not supported.
pub const PAL_OODLE: ErrorCode = ErrorCode::new("SP-PAL-001");
/// The Palworld container header or its size fields are not valid.
pub const PAL_INVALID: ErrorCode = ErrorCode::new("SP-PAL-002");
/// The inner bytes cannot be wrapped again (too large for the 32-bit size fields).
pub const PAL_ENCODE: ErrorCode = ErrorCode::new("SP-PAL-003");

const HEADER_LEN: usize = 12;
const KIND_SINGLE: u8 = 0x31;
const KIND_DOUBLE: u8 = 0x32;

/// `container.palworld-plz`.
///
/// Parameters: `plz_kind` (0x31 or 0x32), `inner` (zlib parameters of the stream next to the GVAS
/// bytes) and, for kind 0x32, `outer` (zlib parameters of the outer stream).
#[derive(Clone, Copy, Debug, Default)]
pub struct PalworldPlzLayer;

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PAL_INVALID, message).with_offset(len_u64(offset))
}

fn read_u32(input: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let bytes = <[u8; 4]>::try_from(input.get(at..end)?).ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn table<'a>(params: &'a Params, key: &str) -> Result<&'a Params> {
    match params.get(key) {
        Some(ParamValue::Table(inner)) => Ok(inner),
        _ => Err(Error::new(
            saveprism_core::codes::CORE_INVALID_PARAM,
            format!("parameter \"{key}\" must be a table"),
        )),
    }
}

fn size_u32(len: usize) -> Result<u32> {
    u32::try_from(len).map_err(|_| Error::new(PAL_ENCODE, "the data is too large for a PlZ file"))
}

impl Layer for PalworldPlzLayer {
    fn id(&self) -> &'static str {
        "container.palworld-plz"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        match input.get(8..11) {
            Some(b"PlZ") => Confidence::HIGH,
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
        let magic = input.get(8..11);
        if magic == Some(b"PlM".as_slice()) {
            return Err(
                Error::new(PAL_OODLE, "Oodle compression (PlM) is not supported").with_offset(8),
            );
        }
        if magic != Some(b"PlZ".as_slice()) {
            return Err(bad("the \"PlZ\" marker is missing at byte 8", 8));
        }
        let (Some(uncompressed), Some(compressed), Some(&kind)) =
            (read_u32(input, 0), read_u32(input, 4), input.get(11))
        else {
            return Err(bad("the Palworld header is cut short", 0));
        };
        let payload = input.get(HEADER_LEN..).unwrap_or_default();

        let mut params = Params::new();
        params.set("plz_kind", u32::from(kind));
        let (data, first_size) = match kind {
            KIND_SINGLE => {
                let (data, inner) = ZlibLayer.decode(payload, ctx)?;
                params.set("inner", inner);
                (data, payload.len())
            }
            KIND_DOUBLE => {
                let (mid, outer) = ZlibLayer.decode(payload, ctx)?;
                let (data, inner) = ZlibLayer.decode(&mid, ctx)?;
                params.set("outer", outer);
                params.set("inner", inner);
                (data, mid.len())
            }
            other => {
                return Err(bad(
                    format!("unknown PlZ kind 0x{other:02X} (expected 0x31 or 0x32)"),
                    11,
                ));
            }
        };

        if usize::try_from(compressed).ok() != Some(first_size) {
            return Err(bad(
                format!(
                    "the compressed size {compressed} does not match the data ({first_size} bytes)"
                ),
                4,
            ));
        }
        if usize::try_from(uncompressed).ok() != Some(data.len()) {
            return Err(bad(
                format!(
                    "the uncompressed size {uncompressed} does not match the data ({} bytes)",
                    data.len()
                ),
                0,
            ));
        }
        Ok((data, params))
    }

    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
        let kind = match u8::try_from(params.int("plz_kind")?) {
            Ok(kind @ (KIND_SINGLE | KIND_DOUBLE)) => kind,
            _ => {
                return Err(Error::new(
                    saveprism_core::codes::CORE_INVALID_PARAM,
                    "parameter \"plz_kind\" must be 49 (0x31) or 50 (0x32)",
                ));
            }
        };
        let first = ZlibLayer.encode(inner, table(params, "inner")?)?;
        let first_size = size_u32(first.len())?;
        let payload = if kind == KIND_DOUBLE {
            ZlibLayer.encode(&first, table(params, "outer")?)?
        } else {
            first
        };

        let mut out = Vec::with_capacity(HEADER_LEN.saturating_add(payload.len()));
        out.extend_from_slice(&size_u32(inner.len())?.to_le_bytes());
        out.extend_from_slice(&first_size.to_le_bytes());
        out.extend_from_slice(b"PlZ");
        out.push(kind);
        out.extend_from_slice(&payload);
        Ok(out)
    }
}

/// Registers the Palworld container layer.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(PalworldPlzLayer));
}
