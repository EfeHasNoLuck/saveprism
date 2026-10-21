//! The two kinds of recipe steps: [`Layer`] (bytes to bytes) and [`Codec`] (bytes to [`SaveDoc`]).

use std::io::Read;

use crate::doc::SaveDoc;
use crate::edit::{Edit, apply_edits};
use crate::error::{Error, ErrorCode, Result, codes};
use crate::limits::{Limits, len_u64};
use crate::params::Params;

/// How sure a `sniff` is that it understands the input, from 0.0 to 1.0.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Confidence(f32);

impl Confidence {
    /// Not recognised.
    pub const NONE: Confidence = Confidence(0.0);
    /// Weak hint (for example only the file extension matches).
    pub const LOW: Confidence = Confidence(0.25);
    /// Plausible (for example a heuristic matched).
    pub const MEDIUM: Confidence = Confidence(0.5);
    /// Strong evidence (for example a magic number matched).
    pub const HIGH: Confidence = Confidence(0.9);
    /// Proven (for example a checksum matched).
    pub const CERTAIN: Confidence = Confidence(1.0);

    /// Creates a confidence; the value is clamped to 0.0..=1.0 and NaN becomes 0.0.
    pub fn new(value: f32) -> Self {
        if value.is_nan() {
            Self(0.0)
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    /// The value in 0.0..=1.0.
    pub fn value(self) -> f32 {
        self.0
    }
}

/// Context handed to `sniff`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SniffCtx<'a> {
    /// Name of the file, when known (extension hints).
    pub file_name: Option<&'a str>,
    /// Number of layers already peeled off.
    pub depth: usize,
}

/// Context handed to `decode`: limits, counters and the hints written in the recipe.
#[derive(Debug)]
pub struct DecodeCtx<'a> {
    limits: &'a Limits,
    depth: usize,
    nodes: u64,
    hints: Params,
}

impl<'a> DecodeCtx<'a> {
    /// Creates a context at step 0 with no hints.
    pub fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            depth: 0,
            nodes: 0,
            hints: Params::new(),
        }
    }

    /// The active limits.
    pub fn limits(&self) -> &Limits {
        self.limits
    }

    /// Index of the recipe step being decoded.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Nodes charged so far.
    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    /// Parameters written for this step in the recipe (for example a password key reference).
    pub fn hints(&self) -> &Params {
        &self.hints
    }

    /// Moves the context to the next step, keeping limits and the node counter.
    pub fn with_step(mut self, depth: usize, hints: Params) -> Self {
        self.depth = depth;
        self.hints = hints;
        self
    }

    /// Counts `count` more nodes; fails with `CORE_LIMIT_NODES` above `Limits::max_nodes`.
    pub fn charge_nodes(&mut self, count: u64) -> Result<()> {
        self.nodes = self.nodes.saturating_add(count);
        if self.nodes > self.limits.max_nodes {
            return Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!(
                    "document has more than {} nodes (limit)",
                    self.limits.max_nodes
                ),
            ));
        }
        Ok(())
    }

    /// Fails with `CORE_LIMIT_DEPTH` when `depth` is above `Limits::max_depth`.
    pub fn check_depth(&self, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!(
                    "nesting depth {depth} is above the limit of {}",
                    self.limits.max_depth
                ),
            ));
        }
        Ok(())
    }

    /// See [`Limits::output_budget`].
    pub fn output_budget(&self, input_len: u64) -> u64 {
        self.limits.output_budget(input_len)
    }

    /// See [`Limits::check_output`].
    pub fn check_output(&self, input_len: u64, output_len: u64) -> Result<()> {
        self.limits.check_output(input_len, output_len)
    }

    /// A new, empty document whose node limit is `Limits::max_nodes`. Codecs open their
    /// document with this, so `push` and `reserve` enforce the limit while parsing.
    pub fn new_doc(&self) -> SaveDoc {
        SaveDoc::with_node_limit(self.limits.max_nodes)
    }

    /// Reads `reader` to its end, but never more than [`DecodeCtx::output_budget`] for an input of
    /// `input_len` bytes: at most `budget + 1` bytes are read, and more than `budget` fails with
    /// `CORE_LIMIT_DECODED`. A read error fails with `io_code`. Decompression layers read their
    /// decoder through this, so a bomb stops while it is being produced, not after.
    pub fn read_bounded(
        &self,
        reader: impl Read,
        input_len: u64,
        io_code: ErrorCode,
    ) -> Result<Vec<u8>> {
        let budget = self.output_budget(input_len);
        let mut limited = reader.take(budget.saturating_add(1));
        let mut out = Vec::new();
        limited
            .read_to_end(&mut out)
            .map_err(|source| Error::new(io_code, "reading the data failed").with_source(source))?;
        if len_u64(out.len()) > budget {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "decoded output is larger than {budget} bytes for a {input_len} byte input (possible decompression bomb)"
                ),
            ));
        }
        Ok(out)
    }
}

/// Checks a finished document against the limits: no more than `max_nodes` nodes and no reserved
/// node left unfinished. Runs after every `Codec::decode`, whatever the codec did itself.
pub(crate) fn verify_decoded(doc: &SaveDoc, limits: &Limits) -> Result<()> {
    if len_u64(doc.len()) > limits.max_nodes {
        return Err(Error::new(
            codes::CORE_LIMIT_NODES,
            format!(
                "the decoded document has {} nodes, the limit is {}",
                doc.len(),
                limits.max_nodes
            ),
        ));
    }
    if doc.unfinished_count() != 0 {
        return Err(Error::new(
            codes::CORE_DOC_BUILD,
            format!(
                "the decoded document has {} reserved nodes that were never finished",
                doc.unfinished_count()
            ),
        ));
    }
    Ok(())
}

/// Result of [`Codec::decode`]: the document plus the parameters needed to write it back.
#[derive(Clone, Debug)]
pub struct Decoded {
    /// The decoded tree.
    pub doc: SaveDoc,
    /// Parameters `encode` needs to reproduce the original bytes.
    pub params: Params,
}

/// A bytes-to-bytes step: encoding, compression or encryption (`"encoding.base64"`, `"compression.gzip"`).
pub trait Layer: Send + Sync {
    /// Stable identifier used in recipes.
    fn id(&self) -> &'static str;
    /// Cheap check whether `input` looks like this layer's output.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence;
    /// Unwraps `input`; returns the inner bytes and the parameters needed to wrap them again.
    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)>;
    /// Wraps `inner` again using the parameters `decode` returned.
    fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>>;
}

/// The last step of a recipe: bytes to [`SaveDoc`] and back (`"format.json"`, `"format.gvas"`).
pub trait Codec: Send + Sync {
    /// Stable identifier used in recipes.
    fn id(&self) -> &'static str;
    /// Cheap check whether `input` is in this format.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence;
    /// Parses `input` into a document plus the parameters needed to write it back.
    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded>;
    /// Writes the whole document again.
    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>>;
    /// Applies edits to `original` and returns the new bytes. The default checks the input size,
    /// decodes and applies the edits under `limits`, and encodes again with `params`;
    /// lossless codecs override it.
    fn patch(
        &self,
        original: &[u8],
        edits: &[Edit],
        params: &Params,
        limits: &Limits,
    ) -> Result<Vec<u8>> {
        limits.check_input(len_u64(original.len()))?;
        let mut ctx = DecodeCtx::new(limits);
        let mut decoded = self.decode(original, &mut ctx)?;
        verify_decoded(&decoded.doc, limits)?;
        apply_edits(&mut decoded.doc, edits, limits)?;
        self.encode(&decoded.doc, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_clamped_and_ordered() {
        assert_eq!(Confidence::new(2.0).value(), 1.0);
        assert_eq!(Confidence::new(-1.0).value(), 0.0);
        assert_eq!(Confidence::new(f32::NAN).value(), 0.0);
        assert_eq!(Confidence::new(0.5), Confidence::MEDIUM);
        assert!(Confidence::NONE < Confidence::LOW);
        assert!(Confidence::LOW < Confidence::MEDIUM);
        assert!(Confidence::MEDIUM < Confidence::HIGH);
        assert!(Confidence::HIGH < Confidence::CERTAIN);
    }

    #[test]
    fn charge_nodes_enforces_the_limit() {
        let limits = Limits {
            max_nodes: 10,
            ..Limits::default()
        };
        let mut ctx = DecodeCtx::new(&limits);
        ctx.charge_nodes(6).unwrap();
        ctx.charge_nodes(4).unwrap();
        assert_eq!(ctx.nodes(), 10);
        let error = ctx.charge_nodes(1).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_NODES);
        assert_eq!(
            ctx.charge_nodes(u64::MAX).unwrap_err().code(),
            codes::CORE_LIMIT_NODES
        );
    }

    #[test]
    fn check_depth_enforces_the_limit() {
        let limits = Limits {
            max_depth: 3,
            ..Limits::default()
        };
        let ctx = DecodeCtx::new(&limits);
        assert!(ctx.check_depth(3).is_ok());
        assert_eq!(
            ctx.check_depth(4).unwrap_err().code(),
            codes::CORE_LIMIT_DEPTH
        );
    }

    #[test]
    fn with_step_replaces_depth_and_hints_but_keeps_the_node_counter() {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        ctx.charge_nodes(5).unwrap();
        let mut hints = Params::new();
        hints.set("key", vec![1_u8]);
        let ctx = ctx.with_step(2, hints);
        assert_eq!(ctx.depth(), 2);
        assert_eq!(ctx.nodes(), 5);
        assert!(ctx.hints().contains("key"));
        assert_eq!(ctx.limits(), &limits);
    }

    #[test]
    fn new_doc_uses_the_context_node_limit() {
        let limits = Limits {
            max_nodes: 2,
            ..Limits::default()
        };
        let ctx = DecodeCtx::new(&limits);
        let mut doc = ctx.new_doc();
        assert_eq!(doc.node_limit(), 2);
        // The root takes one node; one more fits, the next does not.
        doc.push(crate::doc::Node::new(crate::doc::Value::Null))
            .unwrap();
        assert_eq!(
            doc.push(crate::doc::Node::new(crate::doc::Value::Null))
                .unwrap_err()
                .code(),
            codes::CORE_LIMIT_NODES
        );
    }

    const TEST_IO: ErrorCode = ErrorCode::new("SP-TEST-001");

    /// A tiny budget: 100 bytes for a 10 byte input.
    fn small_budget() -> Limits {
        Limits {
            ratio_floor_bytes: 0,
            max_ratio: 10,
            ..Limits::default()
        }
    }

    #[test]
    fn read_bounded_returns_data_within_the_budget() {
        let limits = small_budget();
        let ctx = DecodeCtx::new(&limits);
        let data = vec![7_u8; 100];
        let out = ctx.read_bounded(data.as_slice(), 10, TEST_IO).unwrap();
        assert_eq!(out, data);
        assert!(
            ctx.read_bounded(std::io::empty(), 10, TEST_IO)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn read_bounded_rejects_data_over_the_budget() {
        let limits = small_budget();
        let ctx = DecodeCtx::new(&limits);
        let data = vec![7_u8; 101];
        let error = ctx.read_bounded(data.as_slice(), 10, TEST_IO).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn read_bounded_stops_an_endless_reader() {
        let limits = small_budget();
        let ctx = DecodeCtx::new(&limits);
        let error = ctx
            .read_bounded(std::io::repeat(0), 10, TEST_IO)
            .unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
    }

    #[test]
    fn read_bounded_reports_read_errors_with_the_given_code() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("broken pipe"))
            }
        }
        let limits = Limits::default();
        let ctx = DecodeCtx::new(&limits);
        let error = ctx.read_bounded(Broken, 10, TEST_IO).unwrap_err();
        assert_eq!(error.code(), TEST_IO);
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn verify_decoded_checks_node_count_and_unfinished_nodes() {
        let limits = Limits {
            max_nodes: 3,
            ..Limits::default()
        };
        assert!(verify_decoded(&SaveDoc::new(), &limits).is_ok());

        let mut big = SaveDoc::new();
        for _ in 0..3 {
            big.push(crate::doc::Node::new(crate::doc::Value::Null))
                .unwrap();
        }
        assert_eq!(
            verify_decoded(&big, &limits).unwrap_err().code(),
            codes::CORE_LIMIT_NODES
        );

        let mut open = SaveDoc::new();
        open.reserve().unwrap();
        assert_eq!(
            verify_decoded(&open, &limits).unwrap_err().code(),
            codes::CORE_DOC_BUILD
        );
    }

    #[test]
    fn output_checks_delegate_to_limits() {
        let limits = Limits::default();
        let ctx = DecodeCtx::new(&limits);
        assert_eq!(ctx.output_budget(0), limits.ratio_floor_bytes);
        assert!(ctx.check_output(0, limits.ratio_floor_bytes).is_ok());
        assert_eq!(
            ctx.check_output(0, limits.ratio_floor_bytes + 1)
                .unwrap_err()
                .code(),
            codes::CORE_LIMIT_DECODED
        );
    }
}
