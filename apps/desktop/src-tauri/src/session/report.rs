//! The "file report" for a file SavePrism does not know.
//!
//! A player who hits "SavePrism does not know this file yet" can copy a short report into a
//! GitHub issue. The report is built here from the file's extension, size, first bytes and a
//! SHA-256 of the whole file. It never holds the file name, the path or any part of the file past
//! the first 32 bytes, and nothing is sent anywhere: the UI only puts the text on the clipboard.
//!
//! The file is opened read-only, as a regular file only (a link, a folder or a device is refused).
//! The first [`SAMPLE_BYTES`] bytes feed the hex, the text preview and the entropy class; the hash
//! is read in chunks from the same handle and skipped for files above [`MAX_HASH_BYTES`].

use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{ApiError, app_codes, io_error};

/// Bytes of the file start shown as hex and text.
pub const HEAD_BYTES: usize = 32;
/// Bytes of the file start the entropy class is worked out from.
pub const SAMPLE_BYTES: usize = 4096;
/// Files above this size are not hashed (the hash would take too long to be worth it).
pub const MAX_HASH_BYTES: u64 = 256 * 1024 * 1024;
/// How much of the file one hashing read takes.
const HASH_CHUNK: usize = 64 * 1024;
/// Longest extension reported, in characters.
const MAX_EXTENSION: usize = 16;
/// Samples shorter than this are never called compressed: too little data to tell.
const MIN_PACKED_SAMPLE: usize = 1024;
/// Entropy, in bits per byte, from which a sample looks compressed or encrypted.
const PACKED_ENTROPY: f64 = 7.5;

/// A rough guess at what the first bytes of a file are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntropyClass {
    /// The file has no bytes.
    Empty,
    /// Readable text (UTF-8 or a UTF-16/UTF-8 byte order mark).
    Text,
    /// Structured binary data.
    Binary,
    /// Random-looking bytes: compressed or encrypted.
    Compressed,
}

/// What the "Copy file report" action works from. It holds no path and no file name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReport {
    /// Lower-case extension without the dot; empty when the file has none (or an odd one).
    pub extension: String,
    /// Size of the file in bytes.
    pub byte_size: u64,
    /// The first 32 bytes as lower-case hex pairs separated by spaces.
    pub head_hex: String,
    /// The same bytes as printable ASCII; every other byte (and the backtick) is a dot.
    pub head_ascii: String,
    /// Rough class of the first 4 KiB.
    pub entropy: EntropyClass,
    /// SHA-256 of the whole file as lower-case hex; `None` for a file above the hash limit or one
    /// that changed while it was read.
    pub sha256: Option<String>,
    /// The SavePrism version that made the report.
    pub version: String,
}

/// Builds the report of the file at `path`.
pub fn read_file_report(path: &Path) -> Result<FileReport, ApiError> {
    report_with_cap(path, MAX_HASH_BYTES)
}

/// [`read_file_report`] with the hash size limit given, so tests need no huge file.
fn report_with_cap(path: &Path, hash_cap: u64) -> Result<FileReport, ApiError> {
    let linked = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    if !linked.file_type().is_file() {
        return Err(not_regular());
    }
    let mut file = fs::File::open(path).map_err(|error| io_error(path, error))?;
    // The opened handle is checked again: the path may have been swapped for a link meanwhile.
    let meta = file.metadata().map_err(|error| io_error(path, error))?;
    if !meta.is_file() {
        return Err(not_regular());
    }
    let size = meta.len();

    let mut sample = Vec::new();
    (&mut file)
        .take(SAMPLE_BYTES as u64)
        .read_to_end(&mut sample)
        .map_err(|error| io_error(path, error))?;
    let sha256 = if size <= hash_cap {
        hash_rest(&mut file, &sample, size, hash_cap).map_err(|error| io_error(path, error))?
    } else {
        None
    };

    let head: Vec<u8> = sample.iter().copied().take(HEAD_BYTES).collect();
    Ok(FileReport {
        extension: safe_extension(path),
        byte_size: size,
        head_hex: hex(&head),
        head_ascii: ascii_preview(&head),
        entropy: classify(&sample),
        sha256,
        version: saveprism_core::VERSION.to_string(),
    })
}

fn not_regular() -> ApiError {
    ApiError::new(
        app_codes::FILE_REPORT,
        "a file report can only be made for a regular file",
        "Pick the save file itself, not a folder or a shortcut.",
    )
}

/// The SHA-256 of the file: `sample` is what was read from its start already, the rest is read
/// from `file` in chunks. `None` when the file turned out bigger than `cap` or its size is not
/// `size` (it changed while it was read), so the hash would not describe one version of it.
fn hash_rest(
    file: &mut fs::File,
    sample: &[u8],
    size: u64,
    cap: u64,
) -> io::Result<Option<String>> {
    let mut hasher = Sha256::new();
    hasher.update(sample);
    let mut total = sample.len() as u64;
    let mut chunk = vec![0u8; HASH_CHUNK];
    loop {
        let read = match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        total += read as u64;
        if total > cap {
            return Ok(None);
        }
        if let Some(part) = chunk.get(..read) {
            hasher.update(part);
        }
    }
    Ok((total == size).then(|| hex_bytes(&hasher.finalize(), "")))
}

/// The extension of `path`: lower-case ASCII letters, digits, `_` and `-`, at most 16 characters.
/// Anything else (a long or odd extension could hold a name) gives an empty text.
fn safe_extension(path: &Path) -> String {
    let Some(extension) = path.extension().and_then(|text| text.to_str()) else {
        return String::new();
    };
    let allowed = !extension.is_empty()
        && extension.chars().count() <= MAX_EXTENSION
        && extension
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if allowed {
        extension.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// Bytes as lower-case hex pairs separated by single spaces.
fn hex(bytes: &[u8]) -> String {
    hex_bytes(bytes, " ")
}

fn hex_bytes(bytes: &[u8], separator: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * (2 + separator.len()));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push_str(separator);
        }
        out.push(char::from(
            DIGITS.get(usize::from(byte >> 4)).copied().unwrap_or(b'0'),
        ));
        out.push(char::from(
            DIGITS
                .get(usize::from(byte & 0x0f))
                .copied()
                .unwrap_or(b'0'),
        ));
    }
    out
}

/// Printable ASCII as it is, every other byte and the backtick as a dot (so the text can sit in a
/// Markdown code span).
fn ascii_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if (0x20..=0x7e).contains(&byte) && byte != b'`' {
                char::from(byte)
            } else {
                '.'
            }
        })
        .collect()
}

/// Guesses what a file start is: text, structured binary, or random-looking (compressed or
/// encrypted). Rough by design: it only helps whoever reads the report.
fn classify(sample: &[u8]) -> EntropyClass {
    if sample.is_empty() {
        return EntropyClass::Empty;
    }
    if looks_like_text(sample) {
        return EntropyClass::Text;
    }
    if sample.len() >= MIN_PACKED_SAMPLE && entropy_bits(sample) >= PACKED_ENTROPY {
        return EntropyClass::Compressed;
    }
    EntropyClass::Binary
}

/// UTF-8 (a character cut off at the end of the sample is fine) without NULs and with hardly any
/// control characters, or a UTF-8/UTF-16 byte order mark.
fn looks_like_text(sample: &[u8]) -> bool {
    if sample.starts_with(&[0xff, 0xfe]) || sample.starts_with(&[0xfe, 0xff]) {
        return true;
    }
    let valid = match std::str::from_utf8(sample) {
        Ok(_) => true,
        Err(error) => error.error_len().is_none() && error.valid_up_to() + 4 > sample.len(),
    };
    if !valid {
        return false;
    }
    let controls = sample
        .iter()
        .filter(|&&byte| (byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r')) || byte == 0x7f)
        .count();
    controls * 100 <= sample.len()
}

/// Shannon entropy of the bytes, in bits per byte (0 to 8).
fn entropy_bits(sample: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &byte in sample {
        if let Some(count) = counts.get_mut(usize::from(byte)) {
            *count += 1;
        }
    }
    let total = sample.len() as f64;
    counts
        .iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let probability = f64::from(count) / total;
            -probability * probability.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).expect("create");
        file.write_all(bytes).expect("write");
        (dir, path)
    }

    /// The SHA-256 of "abc" (FIPS 180-2 test vector).
    const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn reports_size_hex_text_and_hash() {
        let (_dir, path) = write_temp("Slot1.SAV", b"abc");
        let report = read_file_report(&path).expect("report");
        assert_eq!(report.extension, "sav");
        assert_eq!(report.byte_size, 3);
        assert_eq!(report.head_hex, "61 62 63");
        assert_eq!(report.head_ascii, "abc");
        assert_eq!(report.entropy, EntropyClass::Text);
        assert_eq!(report.sha256.as_deref(), Some(SHA256_ABC));
        assert_eq!(report.version, saveprism_core::VERSION);
    }

    #[test]
    fn keeps_only_the_first_32_bytes() {
        let bytes: Vec<u8> = (0u8..200).collect();
        let (_dir, path) = write_temp("data.bin", &bytes);
        let report = read_file_report(&path).expect("report");
        assert_eq!(report.byte_size, 200);
        assert_eq!(report.head_hex.split(' ').count(), HEAD_BYTES);
        assert!(report.head_hex.starts_with("00 01 02 03"));
        assert!(report.head_hex.ends_with("1e 1f"));
        assert_eq!(report.head_ascii.chars().count(), HEAD_BYTES);
        assert_eq!(report.entropy, EntropyClass::Binary);
    }

    #[test]
    fn output_holds_the_extension_but_no_path_or_name() {
        let (dir, path) = write_temp("John Smith secret.sav", b"SAVE\0\0\0\x01 hello");
        let report = read_file_report(&path).expect("report");
        let json = serde_json::to_string(&report).expect("json");
        let dir_text = dir.path().to_string_lossy().into_owned();
        assert!(!json.contains(&dir_text), "directory leaked: {json}");
        assert!(!json.to_lowercase().contains("john"), "name leaked: {json}");
        assert!(!json.contains("secret"), "name leaked: {json}");
        assert!(!json.contains("Smith"), "name leaked: {json}");
        assert_eq!(report.extension, "sav");
        assert_eq!(report.head_ascii, "SAVE.... hello");
    }

    #[test]
    fn odd_extensions_are_left_out() {
        assert_eq!(safe_extension(Path::new("a/b.RPGSAVE")), "rpgsave");
        assert_eq!(safe_extension(Path::new("a/persistent")), "");
        assert_eq!(safe_extension(Path::new("a/b.John Smith")), "");
        assert_eq!(safe_extension(Path::new("a/b.sav2")), "sav2");
        assert_eq!(safe_extension(Path::new("a/b.waytoolongextensionname")), "");
        assert_eq!(safe_extension(Path::new("a/b.")), "");
    }

    #[test]
    fn the_hash_is_skipped_above_the_cap() {
        let (_dir, path) = write_temp("big.dat", &[7u8; 100]);
        let skipped = report_with_cap(&path, 99).expect("report");
        assert_eq!(skipped.sha256, None);
        assert_eq!(skipped.byte_size, 100);
        let hashed = report_with_cap(&path, 100).expect("report");
        assert_eq!(hashed.sha256.map(|hash| hash.len()), Some(64));
    }

    #[test]
    fn a_file_longer_than_one_chunk_is_hashed_whole() {
        let bytes: Vec<u8> = (0..(HASH_CHUNK * 2 + 123))
            .map(|index| (index % 251) as u8)
            .collect();
        let (_dir, path) = write_temp("long.bin", &bytes);
        let report = read_file_report(&path).expect("report");
        let expected = hex_bytes(&Sha256::digest(&bytes), "");
        assert_eq!(report.sha256, Some(expected));
    }

    #[test]
    fn a_folder_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = read_file_report(dir.path()).expect_err("folder");
        assert_eq!(error.code, app_codes::FILE_REPORT);
    }

    #[test]
    fn a_missing_file_is_a_read_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = read_file_report(&dir.path().join("nope.sav")).expect_err("missing");
        assert_ne!(error.code, app_codes::FILE_REPORT);
    }

    #[test]
    fn a_symbolic_link_is_refused() {
        let (dir, target) = write_temp("real.sav", b"abc");
        let link = dir.path().join("link.sav");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        if let Err(error) = made {
            // Windows without the symlink privilege (no developer mode): nothing to test here.
            eprintln!("symlink test skipped: {error}");
            return;
        }
        let error = read_file_report(&link).expect_err("link");
        assert_eq!(error.code, app_codes::FILE_REPORT);
        assert!(read_file_report(&target).is_ok());
    }

    #[test]
    fn classifies_text_binary_packed_and_empty() {
        assert_eq!(classify(&[]), EntropyClass::Empty);
        assert_eq!(classify(b"{\"gold\": 100}\n"), EntropyClass::Text);
        assert_eq!(
            classify("Kay\u{131}t dosyas\u{131}".as_bytes()),
            EntropyClass::Text
        );
        assert_eq!(classify(&[0xff, 0xfe, b'a', 0]), EntropyClass::Text);
        assert_eq!(classify(&[0, 0, 0, 1, 0, 0, 0, 2]), EntropyClass::Binary);
        // A pseudo-random sample (xorshift) looks compressed or encrypted.
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let random: Vec<u8> = (0..SAMPLE_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        assert_eq!(classify(&random), EntropyClass::Compressed);
        // Too short to tell, so it stays "binary".
        assert_eq!(
            classify(random.get(..64).unwrap_or(&[])),
            EntropyClass::Binary
        );
    }

    #[test]
    fn the_preview_never_holds_a_backtick() {
        assert_eq!(ascii_preview(b"a`b\x00\x7f~ "), "a.b..~ ");
    }
}
