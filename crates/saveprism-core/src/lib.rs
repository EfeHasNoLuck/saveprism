//! SavePrism core.
//!
//! This crate will hold the format-independent save document model
//! (`SaveDoc`), the `Codec` trait implemented by every format, and the safe
//! writing module (backup + atomic replace). Those arrive in milestone M1;
//! for now the crate only exposes its version.

/// Version of the SavePrism core, taken from the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_not_empty() {
        assert!(!VERSION.is_empty());
    }
}
