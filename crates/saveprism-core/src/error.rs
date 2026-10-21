//! Error type, stable error codes and context helpers.
//!
//! Every error carries a stable code (`SP-<AREA>-<NNN>`), a technical message,
//! an optional byte offset and a context chain ("while decoding step 2/3 ...").
//! The user-facing text lives in the UI locale files under the key returned by
//! [`ErrorCode::i18n_key`].

use std::fmt;

/// Result alias used across the workspace; the error type defaults to [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A stable error code such as `SP-CORE-001`. Codes never change meaning once released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Creates a code from its textual form, e.g. `ErrorCode::new("SP-JSON-001")`.
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    /// Returns the textual form of the code.
    pub fn as_str(&self) -> &'static str {
        self.0
    }

    /// Returns the i18n key of the user-facing text, e.g. `error.sp-core-001`.
    pub fn i18n_key(&self) -> String {
        format!("error.{}", self.0.to_ascii_lowercase())
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// The error type of every SavePrism library crate.
#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct Error {
    code: ErrorCode,
    message: String,
    offset: Option<u64>,
    context: Vec<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Error {
    /// Creates an error with a code and a technical message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            offset: None,
            context: Vec::new(),
            source: None,
        }
    }

    /// Records the byte offset (in the input of the failing step) where the problem was found.
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Records the underlying error.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Box::new(source));
        self
    }

    /// Appends a context line (outermost context last), e.g. `"while decoding step 2/3 (compression.gzip)"`.
    pub fn context(mut self, context: impl Into<String>) -> Self {
        self.context.push(context.into());
        self
    }

    /// The stable error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The technical message (English, not meant for end users).
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The byte offset, if known.
    pub fn offset(&self) -> Option<u64> {
        self.offset
    }

    /// The context lines, innermost first.
    pub fn context_chain(&self) -> &[String] {
        &self.context
    }

    /// A multi-line technical report: `SP-...: message (offset 0x1A0)` followed by one indented line per context.
    pub fn report(&self) -> String {
        let mut text = format!("{}: {}", self.code, self.message);
        if let Some(offset) = self.offset {
            text.push_str(&format!(" (offset 0x{offset:X})"));
        }
        for line in &self.context {
            text.push_str("\n  ");
            text.push_str(line);
        }
        text
    }
}

/// Adds context to the error of a [`Result`] lazily.
pub trait ResultExt<T> {
    /// On error, appends the context line produced by `f`.
    fn context(self, f: impl FnOnce() -> String) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, f: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|error| error.context(f()))
    }
}

/// Error codes owned by the core crate. Codec crates define their own codes next to their code.
pub mod codes {
    use super::ErrorCode;

    /// A planned function that has no implementation yet.
    pub const CORE_NOT_IMPLEMENTED: ErrorCode = ErrorCode::new("SP-CORE-001");
    /// A recipe step names a layer or codec that is not registered.
    pub const CORE_UNKNOWN_STEP: ErrorCode = ErrorCode::new("SP-CORE-002");
    /// The recipe is structurally invalid (empty, wrong step order, bad version).
    pub const CORE_INVALID_RECIPE: ErrorCode = ErrorCode::new("SP-CORE-003");
    /// The recipe TOML could not be read or written.
    pub const CORE_RECIPE_FORMAT: ErrorCode = ErrorCode::new("SP-CORE-004");
    /// A step parameter is missing or has the wrong type or value.
    pub const CORE_INVALID_PARAM: ErrorCode = ErrorCode::new("SP-CORE-005");
    /// The input is larger than the configured limit.
    pub const CORE_LIMIT_INPUT: ErrorCode = ErrorCode::new("SP-CORE-006");
    /// The decoded output exceeds the size or ratio limit (decompression bomb guard).
    pub const CORE_LIMIT_DECODED: ErrorCode = ErrorCode::new("SP-CORE-007");
    /// The nesting depth exceeds the configured limit.
    pub const CORE_LIMIT_DEPTH: ErrorCode = ErrorCode::new("SP-CORE-008");
    /// The node count exceeds the configured limit.
    pub const CORE_LIMIT_NODES: ErrorCode = ErrorCode::new("SP-CORE-009");
    /// The recipe has more layers than the configured limit.
    pub const CORE_LIMIT_LAYERS: ErrorCode = ErrorCode::new("SP-CORE-010");
    /// A JSON Pointer path is malformed or does not exist.
    pub const CORE_INVALID_PATH: ErrorCode = ErrorCode::new("SP-CORE-011");
    /// An edit could not be applied.
    pub const CORE_EDIT_FAILED: ErrorCode = ErrorCode::new("SP-CORE-012");
    /// An edit targets a read-only node.
    pub const CORE_READ_ONLY: ErrorCode = ErrorCode::new("SP-CORE-013");
    /// A document was built wrongly: bad `finish` call or a reserved node that was never filled.
    pub const CORE_DOC_BUILD: ErrorCode = ErrorCode::new("SP-CORE-014");
    /// No layer/codec combination recognised the input.
    pub const DET_NO_MATCH: ErrorCode = ErrorCode::new("SP-DET-001");
    /// Detection stopped because the attempt budget ran out.
    pub const DET_BUDGET: ErrorCode = ErrorCode::new("SP-DET-002");
    /// The backup could not be created.
    pub const IO_BACKUP: ErrorCode = ErrorCode::new("SP-IO-001");
    /// The temporary file could not be written.
    pub const IO_TEMP_WRITE: ErrorCode = ErrorCode::new("SP-IO-002");
    /// The target could not be replaced (file in use or permission denied).
    pub const IO_REPLACE: ErrorCode = ErrorCode::new("SP-IO-003");
    /// Verification of the written file failed and the backup was restored.
    pub const IO_VERIFY_RESTORED: ErrorCode = ErrorCode::new("SP-IO-004");
    /// Verification failed and restoring the backup failed too.
    pub const IO_RESTORE_FAILED: ErrorCode = ErrorCode::new("SP-IO-005");
    /// No backup directory could be determined.
    pub const IO_NO_BACKUP_ROOT: ErrorCode = ErrorCode::new("SP-IO-006");
    /// A file could not be read.
    pub const IO_READ: ErrorCode = ErrorCode::new("SP-IO-007");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn i18n_key_is_lowercase_with_prefix() {
        assert_eq!(codes::CORE_NOT_IMPLEMENTED.i18n_key(), "error.sp-core-001");
        assert_eq!(ErrorCode::new("SP-IO-007").i18n_key(), "error.sp-io-007");
    }

    #[test]
    fn code_display_is_the_code() {
        assert_eq!(codes::DET_NO_MATCH.to_string(), "SP-DET-001");
        assert_eq!(codes::DET_NO_MATCH.as_str(), "SP-DET-001");
    }

    #[test]
    fn display_is_code_and_message() {
        let error = Error::new(codes::CORE_INVALID_PARAM, "bad key");
        assert_eq!(error.to_string(), "SP-CORE-005: bad key");
    }

    #[test]
    fn report_has_offset_and_context_lines() {
        let error = Error::new(codes::CORE_NOT_IMPLEMENTED, "bad magic")
            .with_offset(0x1A0)
            .context("while decoding layer 2/4")
            .context("while opening the file");
        assert_eq!(
            error.report(),
            "SP-CORE-001: bad magic (offset 0x1A0)\n  while decoding layer 2/4\n  while opening the file"
        );
        assert_eq!(error.offset(), Some(0x1A0));
        assert_eq!(error.context_chain().len(), 2);
        assert_eq!(error.message(), "bad magic");
    }

    #[test]
    fn report_without_offset_or_context_is_one_line() {
        let error = Error::new(codes::CORE_EDIT_FAILED, "nope");
        assert_eq!(error.report(), "SP-CORE-012: nope");
    }

    #[test]
    fn result_ext_adds_context_only_on_error() {
        let ok: Result<u8> = Ok(1);
        assert_eq!(ResultExt::context(ok, || "unused".to_string()).unwrap(), 1);

        let failed: Result<u8> = Err(Error::new(codes::CORE_EDIT_FAILED, "x"));
        let error = ResultExt::context(failed, || "while testing".to_string()).unwrap_err();
        assert_eq!(error.context_chain(), ["while testing".to_string()]);
    }

    #[test]
    fn source_is_exposed() {
        let io = std::io::Error::other("disk on fire");
        let error = Error::new(codes::IO_READ, "read failed").with_source(io);
        assert!(StdError::source(&error).is_some());
    }
}
