//! SavePrism core.
//!
//! The format-independent heart of SavePrism:
//!
//! - [`SaveDoc`]: the universal document tree (arena of typed nodes) and [`Path`] addressing.
//! - [`Edit`] / [`apply_edits`]: atomic, path-addressed changes.
//! - [`Recipe`]: the ordered steps (layers, then one codec) that turn a file into a document,
//!   with [`Params`] holding the values needed to write it back byte for byte.
//! - [`Layer`] / [`Codec`] / [`Registry`]: the contracts that `saveprism-codecs` implements.
//! - [`pipeline`]: runs a recipe under [`Limits`] with error context on every step.
//! - [`detect`], [`guard`], [`io`]: automatic detection, the round-trip guard and safe writing
//!   (signatures only until their own tasks fill them in).
//! - [`error`]: [`Error`], stable [`ErrorCode`]s and the [`codes`] list.

pub mod codec;
pub mod detect;
pub mod doc;
pub mod edit;
pub mod error;
pub mod guard;
pub mod io;
pub mod limits;
pub mod params;
pub mod path;
pub mod pipeline;
pub mod recipe;
pub mod registry;

pub use codec::{Codec, Confidence, DecodeCtx, Decoded, Layer, SniffCtx};
pub use doc::{ByteSpan, FloatWidth, IntWidth, Node, NodeFlags, NodeId, SaveDoc, TypeHint, Value};
pub use edit::{
    Detached, DetachedKey, Edit, OwnedValue, Position, apply_edits, detach, entry_at, reattach,
};
pub use error::{Error, ErrorCode, Result, ResultExt, codes};
pub use limits::Limits;
pub use params::{ParamValue, Params};
pub use path::Path;
pub use pipeline::DecodeOutput;
pub use recipe::{Recipe, Step};
pub use registry::Registry;

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
