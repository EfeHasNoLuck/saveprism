//! Byte layers: base64, hex, percent escapes, gzip, zlib, deflate, bzip2, zstd, xz, lzma, lz4,
//! brotli, LZ-String, LZF, single/repeating-key XOR, the KiriKiri text-stream wrapper, the
//! TyranoScript URL escape, the zip container and tar.
//! brotli, LZ-String, LZF, single/repeating-key XOR, the TyranoScript URL escape, the zip and tar
//! containers, PS1/PS2 memory cards, PS2 `.psu` exports, SharkPort GBA snapshots, FromSoftware
//! BND4 binders, Blizzard MPQ archives and Brickadia BRZ archives.
//!
//! Every layer records the parameters it needs to wrap the inner bytes again exactly as they were.
//! Compression layers also keep the original compressed bytes together with a digest of the
//! contents, so an unchanged file is written back byte for byte and an edited one is compressed
//! again with the recorded settings. Layers that produce output read it through
//! `DecodeCtx::read_bounded` (or an equivalent budget), so a decompression bomb stops while it is
//! being produced.

use std::sync::Arc;

use saveprism_core::codes;
use saveprism_core::{Confidence, Error, ErrorCode, Params, Registry, Result, SniffCtx};

mod base64;
mod bnd4;
mod brotli;
mod brz;
mod bzip2;
mod deflate;
mod erf;
mod gzip;
mod hex;
mod kirikiri;
mod lz4;
mod lzf;
mod lzip;
mod lzma;
mod lzstring;
mod mpq;
mod percent;
mod ps2;
mod psu;
mod psx;
mod sharkport;
mod tar;
mod url_escape;
mod xor;
mod xz;
mod zip;
mod zlib;
mod zstd;

pub use base64::Base64Layer;
pub use bnd4::{BND4_DAMAGED, BND4_NO_PAYLOAD, BND4_WRITE, Bnd4Layer};
pub use brotli::BrotliLayer;
pub use brz::{BRZ_DAMAGED, BRZ_NO_PAYLOAD, BRZ_WRITE, BrzLayer};
pub use bzip2::Bzip2Layer;
pub use deflate::DeflateLayer;
pub use erf::{ERF_DAMAGED, ERF_NO_PAYLOAD, ERF_WRITE, ErfLayer};
pub use gzip::GzipLayer;
pub use hex::HexLayer;
pub use kirikiri::KirikiriLayer;
pub use lz4::Lz4Layer;
pub use lzf::LzfLayer;
pub use lzip::LzipLayer;
pub use lzma::LzmaLayer;
pub use lzstring::LzStringLayer;
pub use mpq::{MPQ_DAMAGED, MPQ_NO_PAYLOAD, MPQ_UNSUPPORTED, MPQ_WRITE, MpqLayer};
pub use percent::PercentLayer;
pub use ps2::{PS2_DAMAGED, PS2_NO_PAYLOAD, PS2_WRITE, Ps2CardLayer};
pub use psu::{PSU_DAMAGED, PSU_NO_PAYLOAD, PSU_WRITE, PsuLayer};
pub use psx::{
    GME_DAMAGED, GME_WRITE, PSX_DAMAGED, PSX_NO_PAYLOAD, PSX_WRITE, PsxCardLayer, PsxGmeLayer,
};
pub use sharkport::{SPS_DAMAGED, SPS_UNSUPPORTED, SPS_WRITE, SharkPortLayer};
pub use tar::{TAR_DAMAGED, TAR_NO_PAYLOAD, TAR_WRITE, TarLayer};
pub use url_escape::UrlEscapeLayer;
pub use xor::XorLayer;
pub use xz::XzLayer;
pub use zip::{ZIP_DAMAGED, ZIP_NO_PAYLOAD, ZIP_UNSUPPORTED, ZIP_WRITE, ZipLayer};
pub use zlib::ZlibLayer;
pub use zstd::ZstdLayer;

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
/// The input has a malformed percent escape (used by `encoding.url-escape`).
pub const LAY_URL_ESCAPE_INVALID: ErrorCode = ErrorCode::new("SP-URL-001");
/// The percent escapes do not spell UTF-8 text (used by `encoding.url-escape`).
pub const LAY_URL_ESCAPE_UTF8: ErrorCode = ErrorCode::new("SP-URL-002");
/// The input is not a valid raw deflate stream.
pub const LAY_DEFLATE_INVALID: ErrorCode = ErrorCode::new("SP-LAY-007");
/// The input is not a valid bzip2 stream.
pub const LAY_BZIP2_INVALID: ErrorCode = ErrorCode::new("SP-LAY-008");
/// The input is not a valid Zstandard frame.
pub const LAY_ZSTD_INVALID: ErrorCode = ErrorCode::new("SP-LAY-009");
/// The input is not a valid xz stream.
pub const LAY_XZ_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00A");
/// The input is not a valid lzip member.
pub const LZIP_INVALID: ErrorCode = ErrorCode::new("SP-LAY-010");
/// The input is not a valid LZMA-alone stream.
pub const LAY_LZMA_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00B");
/// The input is not a valid LZ4 frame.
pub const LAY_LZ4_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00C");
/// The input is not valid Brotli data.
pub const LAY_BROTLI_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00D");
/// The input is not valid LZF data.
pub const LAY_LZF_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00E");
/// The input is not simple XOR obfuscation of a known format.
pub const LAY_XOR_INVALID: ErrorCode = ErrorCode::new("SP-LAY-00F");
/// The input is not valid hex text.
pub const LAY_HEX_INVALID: ErrorCode = ErrorCode::new("SP-HEX-001");
/// The input is not valid percent-encoded text.
pub const LAY_PERCENT_INVALID: ErrorCode = ErrorCode::new("SP-PCT-001");

/// Parameter: the original wrapped bytes (kept so an unchanged inner value is written back
/// byte for byte).
pub(crate) const SOURCE_PARAM: &str = "source";
/// Parameter: size of the inner bytes at decode time.
pub(crate) const INNER_LEN_PARAM: &str = "inner_len";
/// Parameter: FNV-1a digest of the inner bytes at decode time.
pub(crate) const INNER_HASH_PARAM: &str = "inner_hash";

/// Registers the byte layers.
pub fn register(reg: &mut Registry) {
    reg.add_layer(Arc::new(Base64Layer));
    reg.add_layer(Arc::new(HexLayer));
    reg.add_layer(Arc::new(KirikiriLayer));
    reg.add_layer(Arc::new(PercentLayer));
    reg.add_layer(Arc::new(GzipLayer));
    reg.add_layer(Arc::new(DeflateLayer));
    reg.add_layer(Arc::new(ZlibLayer));
    reg.add_layer(Arc::new(Bzip2Layer));
    reg.add_layer(Arc::new(ZstdLayer));
    reg.add_layer(Arc::new(XzLayer));
    reg.add_layer(Arc::new(LzmaLayer));
    reg.add_layer(Arc::new(LzipLayer));
    reg.add_layer(Arc::new(Lz4Layer));
    reg.add_layer(Arc::new(BrotliLayer));
    reg.add_layer(Arc::new(LzfLayer));
    reg.add_layer(Arc::new(XorLayer));
    reg.add_layer(Arc::new(LzStringLayer));
    reg.add_layer(Arc::new(UrlEscapeLayer));
    zip::register(reg);
    tar::register(reg);
    erf::register(reg);
    bnd4::register(reg);
    mpq::register(reg);
    brz::register(reg);
    psx::register(reg);
    ps2::register(reg);
    psu::register(reg);
    sharkport::register(reg);
}

/// 64-bit FNV-1a digest of `data`.
fn fnv1a(data: &[u8]) -> [u8; 8] {
    let mut hash = 0xCBF2_9CE4_8422_2325_u64;
    for &byte in data {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash.to_be_bytes()
}

/// Records `source` (the wrapped bytes) and a digest of `inner`, so [`source_if_unchanged`] can
/// return the original bytes when the content was not edited.
pub(crate) fn keep_source(params: &mut Params, source: &[u8], inner: &[u8]) {
    params.set(SOURCE_PARAM, source.to_vec());
    params.set(
        INNER_LEN_PARAM,
        i64::try_from(inner.len()).unwrap_or(i64::MAX),
    );
    params.set(INNER_HASH_PARAM, fnv1a(inner).to_vec());
}

/// The original wrapped bytes when `inner` is still exactly what `decode` returned.
pub(crate) fn source_if_unchanged(inner: &[u8], params: &Params) -> Result<Option<Vec<u8>>> {
    let Some(source) = params.opt_bytes(SOURCE_PARAM)? else {
        return Ok(None);
    };
    let Some(stored_len) = params.opt_int(INNER_LEN_PARAM)? else {
        return Ok(None);
    };
    let Ok(stored_len) = usize::try_from(stored_len) else {
        return Ok(None);
    };
    let Some(stored_hash) = params.opt_bytes(INNER_HASH_PARAM)? else {
        return Ok(None);
    };
    if stored_len == inner.len() && stored_hash == fnv1a(inner) {
        Ok(Some(source.to_vec()))
    } else {
        Ok(None)
    }
}

/// True when a registered codec or a non-text layer recognises `data`. Used by the container and
/// text-encoding layers, which only unwrap bytes that lead somewhere.
pub(crate) fn inner_is_known(data: &[u8]) -> bool {
    thread_local! {
        static IN_KNOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    IN_KNOWN.with(|busy| {
        if busy.replace(true) {
            return false;
        }
        let mut reg = Registry::default();
        crate::register_all(&mut reg);
        let ctx = SniffCtx::default();
        // Loose text formats accept almost any short ASCII, which a trial decode of random bytes
        // can produce, so they do not prove that a layer without a magic is really there.
        let known = reg
            .codecs()
            .filter(|codec| {
                !matches!(
                    codec.id(),
                    "format.csv"
                        | "format.ini"
                        | "format.properties"
                        | "format.toml"
                        | "format.yaml"
                )
            })
            .any(|codec| codec.sniff(data, &ctx) >= Confidence::MEDIUM)
            || reg.layers().any(|layer| {
                !matches!(layer.id(), "encoding.hex" | "encoding.percent")
                    && layer.sniff(data, &ctx) >= Confidence::MEDIUM
            });
        busy.set(false);
        known
    })
}

/// A writer that stops a decompressor at the output budget.
pub(crate) struct BoundedSink<'a> {
    out: &'a mut Vec<u8>,
    budget: u64,
    over: bool,
}

impl<'a> BoundedSink<'a> {
    pub(crate) fn new(out: &'a mut Vec<u8>, budget: u64) -> Self {
        Self {
            out,
            budget,
            over: false,
        }
    }

    pub(crate) fn over_budget(&self) -> bool {
        self.over
    }
}

impl std::io::Write for BoundedSink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let next = u64::try_from(self.out.len())
            .unwrap_or(u64::MAX)
            .saturating_add(u64::try_from(buf.len()).unwrap_or(u64::MAX));
        if next > self.budget {
            self.over = true;
            return Err(std::io::Error::other(
                "the decoded output exceeds the size budget",
            ));
        }
        self.out.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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
    fn register_adds_every_layer() {
        let mut reg = Registry::default();
        register(&mut reg);
        for id in [
            "encoding.base64",
            "encoding.hex",
            "encoding.percent",
            "compression.gzip",
            "compression.deflate",
            "compression.zlib",
            "compression.bzip2",
            "compression.zstd",
            "compression.xz",
            "compression.lzma",
            "compression.lzip",
            "compression.lz4",
            "compression.brotli",
            "compression.lzf",
            "obfuscation.xor",
            "obfuscation.kirikiri",
            "compression.lz-string",
            "encoding.url-escape",
            "container.zip",
            "container.tar",
            "container.erf",
            "container.bnd4",
            "container.mpq",
            "container.brz",
        ] {
            assert!(reg.is_layer(id), "{id} is not registered");
        }
        assert_eq!(reg.layers().count(), 29);
    }

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(LAY_BASE64_INVALID.as_str(), "SP-LAY-001");
        assert_eq!(LAY_GZIP_INVALID.as_str(), "SP-LAY-002");
        assert_eq!(LAY_ZLIB_INVALID.as_str(), "SP-LAY-003");
        assert_eq!(LAY_LZSTRING_INVALID.as_str(), "SP-LAY-004");
        assert_eq!(LAY_INVALID_UTF16.as_str(), "SP-LAY-005");
        assert_eq!(LAY_ENCODE_FAILED.as_str(), "SP-LAY-006");
        assert_eq!(LAY_DEFLATE_INVALID.as_str(), "SP-LAY-007");
        assert_eq!(LAY_BZIP2_INVALID.as_str(), "SP-LAY-008");
        assert_eq!(LAY_ZSTD_INVALID.as_str(), "SP-LAY-009");
        assert_eq!(LAY_XZ_INVALID.as_str(), "SP-LAY-00A");
        assert_eq!(LAY_LZMA_INVALID.as_str(), "SP-LAY-00B");
        assert_eq!(LZIP_INVALID.as_str(), "SP-LAY-010");
        assert_eq!(LAY_LZ4_INVALID.as_str(), "SP-LAY-00C");
        assert_eq!(LAY_BROTLI_INVALID.as_str(), "SP-LAY-00D");
        assert_eq!(LAY_LZF_INVALID.as_str(), "SP-LAY-00E");
        assert_eq!(LAY_XOR_INVALID.as_str(), "SP-LAY-00F");
        assert_eq!(LAY_HEX_INVALID.as_str(), "SP-HEX-001");
        assert_eq!(LAY_PERCENT_INVALID.as_str(), "SP-PCT-001");
        assert_eq!(LAY_URL_ESCAPE_INVALID.as_str(), "SP-URL-001");
        assert_eq!(LAY_URL_ESCAPE_UTF8.as_str(), "SP-URL-002");
        assert_eq!(ZIP_DAMAGED.as_str(), "SP-ZIP-001");
        assert_eq!(ZIP_UNSUPPORTED.as_str(), "SP-ZIP-002");
        assert_eq!(ZIP_WRITE.as_str(), "SP-ZIP-003");
        assert_eq!(ZIP_NO_PAYLOAD.as_str(), "SP-ZIP-004");
        assert_eq!(TAR_DAMAGED.as_str(), "SP-TAR-001");
        assert_eq!(TAR_NO_PAYLOAD.as_str(), "SP-TAR-002");
        assert_eq!(TAR_WRITE.as_str(), "SP-TAR-003");
        assert_eq!(ERF_DAMAGED.as_str(), "SP-ERF-001");
        assert_eq!(ERF_NO_PAYLOAD.as_str(), "SP-ERF-002");
        assert_eq!(ERF_WRITE.as_str(), "SP-ERF-003");
        assert_eq!(PSX_DAMAGED.as_str(), "SP-PSX-001");
        assert_eq!(PSX_WRITE.as_str(), "SP-PSX-002");
        assert_eq!(PSX_NO_PAYLOAD.as_str(), "SP-PSX-003");
        assert_eq!(GME_DAMAGED.as_str(), "SP-GME-001");
        assert_eq!(GME_WRITE.as_str(), "SP-GME-002");
        assert_eq!(PS2_DAMAGED.as_str(), "SP-PS2-001");
        assert_eq!(PS2_WRITE.as_str(), "SP-PS2-002");
        assert_eq!(PS2_NO_PAYLOAD.as_str(), "SP-PS2-003");
        assert_eq!(PSU_DAMAGED.as_str(), "SP-PSU-001");
        assert_eq!(PSU_NO_PAYLOAD.as_str(), "SP-PSU-002");
        assert_eq!(PSU_WRITE.as_str(), "SP-PSU-003");
        assert_eq!(SPS_DAMAGED.as_str(), "SP-SPS-001");
        assert_eq!(SPS_UNSUPPORTED.as_str(), "SP-SPS-002");
        assert_eq!(SPS_WRITE.as_str(), "SP-SPS-003");
        assert_eq!(BND4_DAMAGED.as_str(), "SP-BND-001");
        assert_eq!(BND4_NO_PAYLOAD.as_str(), "SP-BND-002");
        assert_eq!(BND4_WRITE.as_str(), "SP-BND-003");
        assert_eq!(MPQ_DAMAGED.as_str(), "SP-MPQ-001");
        assert_eq!(MPQ_NO_PAYLOAD.as_str(), "SP-MPQ-002");
        assert_eq!(MPQ_WRITE.as_str(), "SP-MPQ-003");
        assert_eq!(MPQ_UNSUPPORTED.as_str(), "SP-MPQ-004");
        assert_eq!(BRZ_DAMAGED.as_str(), "SP-BRZ-001");
        assert_eq!(BRZ_NO_PAYLOAD.as_str(), "SP-BRZ-002");
        assert_eq!(BRZ_WRITE.as_str(), "SP-BRZ-003");
    }

    #[test]
    fn new_layers_do_not_claim_the_other_fixtures() {
        use saveprism_core::{Layer, SniffCtx};
        use std::fmt::Write as _;

        fn files(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(root) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => files(&path, out),
                    Ok(kind) if kind.is_file() => out.push(path),
                    _ => {}
                }
            }
        }

        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
        let mut paths = Vec::new();
        files(&root, &mut paths);
        let new_layers: [&dyn Layer; 18] = [
            &Bzip2Layer,
            &ZstdLayer,
            &XzLayer,
            &LzmaLayer,
            &Lz4Layer,
            &BrotliLayer,
            &HexLayer,
            &PercentLayer,
            &TarLayer,
            &PsxCardLayer,
            &PsxGmeLayer,
            &Ps2CardLayer,
            &PsuLayer,
            &SharkPortLayer,
            &ErfLayer,
            &Bnd4Layer,
            &MpqLayer,
            &BrzLayer,
        ];
        let ctx = SniffCtx::default();
        let mut failures = Vec::new();
        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if relative.starts_with("layers/")
                || relative.starts_with("console/")
                || relative.starts_with("bioware/")
                || relative.starts_with("archives/")
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            for layer in new_layers {
                if layer.sniff(&bytes, &ctx) > Confidence::NONE {
                    failures.push(format!("{} claims {relative}", layer.id()));
                }
            }
        }
        if !failures.is_empty() {
            let mut message = String::from(
                "new layers claimed existing fixtures:
",
            );
            for failure in failures {
                writeln!(message, "  {failure}").ok();
            }
            panic!("{message}");
        }
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
