//! Byte layers: base64, gzip, zlib and LZ-String.
//!
//! Every layer records the parameters it needs to wrap the inner bytes again exactly as they were.
//! Layers that produce output read it through `DecodeCtx::read_bounded` (or an equivalent budget),
//! so a decompression bomb stops while it is being produced.

use std::sync::Arc;

use saveprism_core::codes;
use saveprism_core::{Error, ErrorCode, Params, Registry, Result};

mod base64;
mod deflate;
mod gzip;
mod lzstring;
mod zlib;

pub use base64::Base64Layer;
pub use gzip::GzipLayer;
pub use lzstring::LzStringLayer;
pub use zlib::ZlibLayer;

/// The input is not valid base64.
pub const LAY_BASE64_INVALID: ErrorCode = ErrorCode::new("SP-LAY-001");
/// The input is not a valid gzip stream.
pub const LAY_GZIP_INVALID: ErrorCode = ErrorCode::new("SP-LAY-002");
/// The input is not a valid zlib stream.
pub const LAY_ZLIB_INVALID: ErrorCode = ErrorCode::new("SP-LAY-003");
/// The input is not valid LZ-String data.
pub const LAY_LZSTRING_INVALID: ErrorCode = ErrorCode::new("SP-LAY-004");
/// The decompressed UTF-16 data contains an unpaired surrogate.
pub const LAY_INVALID_UTF16: ErrorCode = ErrorCode::new("SP-LAY-005");
/// A layer could not encode (wrap) the inner bytes.
pub const LAY_ENCODE_FAILED: ErrorCode = ErrorCode::new("SP-LAY-006");

/// Registers the four byte layers.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(Base64Layer));
    reg.add_layer(Arc::new(GzipLayer));
    reg.add_layer(Arc::new(ZlibLayer));
    reg.add_layer(Arc::new(LzStringLayer));
}

/// A `CORE_INVALID_PARAM` error with a message.
pub(crate) fn invalid_param(message: String) -> Error {
    Error::new(codes::CORE_INVALID_PARAM, message)
}

/// Optional integer parameter that must fit in `u8`.
pub(crate) fn param_u8(params: &Params, key: &str, default: u8) -> Result<u8> {
    match params.opt_int(key)? {
        None => Ok(default),
        Some(value) => u8::try_from(value)
            .map_err(|_| invalid_param(format!("parameter \"{key}\" must be between 0 and 255"))),
    }
}

/// Optional integer parameter that must fit in `u32`.
pub(crate) fn param_u32(params: &Params, key: &str, default: u32) -> Result<u32> {
    match params.opt_int(key)? {
        None => Ok(default),
        Some(value) => u32::try_from(value).map_err(|_| {
            invalid_param(format!(
                "parameter \"{key}\" must be between 0 and {}",
                u32::MAX
            ))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_adds_the_four_layers() {
        let mut reg = Registry::default();
        register(&mut reg);
        for id in [
            "encoding.base64",
            "compression.gzip",
            "compression.zlib",
            "compression.lz-string",
        ] {
            assert!(reg.is_layer(id), "{id} is not registered");
        }
        assert_eq!(reg.layers().count(), 4);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(LAY_BASE64_INVALID.as_str(), "SP-LAY-001");
        assert_eq!(LAY_GZIP_INVALID.as_str(), "SP-LAY-002");
        assert_eq!(LAY_ZLIB_INVALID.as_str(), "SP-LAY-003");
        assert_eq!(LAY_LZSTRING_INVALID.as_str(), "SP-LAY-004");
        assert_eq!(LAY_INVALID_UTF16.as_str(), "SP-LAY-005");
        assert_eq!(LAY_ENCODE_FAILED.as_str(), "SP-LAY-006");
    }

    #[test]
    fn integer_params_are_range_checked() {
        let mut params = Params::new();
        params.set("small", 300_i64);
        params.set("neg", -1_i64);
        assert_eq!(param_u8(&params, "absent", 7).unwrap(), 7);
        assert_eq!(
            param_u8(&params, "small", 0).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
        assert_eq!(param_u32(&params, "small", 0).unwrap(), 300);
        assert_eq!(
            param_u32(&params, "neg", 0).unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
    }
}
