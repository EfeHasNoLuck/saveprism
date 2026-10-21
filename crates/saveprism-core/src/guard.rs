//! Round-trip guard.
//!
//! Before editing is enabled, `decode -> encode` is compared with the original bytes,
//! step by step; the worst verdict decides.

use crate::codec::DecodeCtx;
use crate::error::{Error, Result, codes};
use crate::limits::Limits;
use crate::pipeline;
use crate::recipe::{Recipe, Step};
use crate::registry::Registry;

/// How faithfully a step reproduces the original bytes. Ordered: the worst verdict is the largest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// Byte-identical.
    Exact,
    /// Same content, different bytes (formatting, compression).
    Semantic,
    /// The content differs.
    Different,
}

/// The verdict of one recipe step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepReport {
    /// Index of the step in the recipe.
    pub index: usize,
    /// Kind of the step.
    pub kind: String,
    /// Verdict for this step.
    pub verdict: Verdict,
    /// i18n key explaining the verdict.
    pub reason_key: &'static str,
    /// Technical detail (English).
    pub detail: String,
}

/// The result of [`check_round_trip`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardReport {
    /// The worst verdict of all steps.
    pub overall: Verdict,
    /// Per-step verdicts.
    pub steps: Vec<StepReport>,
    /// Whether editing may be enabled.
    pub editable: bool,
}

/// i18n key: the step reproduces the original bytes.
const KEY_EXACT: &str = "guard.exact";
/// i18n key: a layer rewrites the bytes but keeps the content.
const KEY_SEMANTIC_LAYER: &str = "guard.semantic.layer";
/// i18n key: the codec rewrites the bytes but keeps the document.
const KEY_SEMANTIC_CODEC: &str = "guard.semantic.codec";
/// i18n key: a layer changes the content.
const KEY_DIFFERENT_LAYER: &str = "guard.different.layer";
/// i18n key: the codec changes the document.
const KEY_DIFFERENT_CODEC: &str = "guard.different.codec";
/// i18n key: encoding the step failed.
const KEY_ENCODE_FAILED: &str = "guard.encode_failed";

/// Offset of the first byte where `left` and `right` differ; a shorter slice differs at its end.
fn first_diff(left: &[u8], right: &[u8]) -> Option<usize> {
    if let Some(position) = left.iter().zip(right.iter()).position(|(a, b)| a != b) {
        return Some(position);
    }
    (left.len() != right.len()).then(|| left.len().min(right.len()))
}

/// Technical detail: both sizes and the first differing offset (no file content).
fn diff_detail(original: &[u8], rebuilt: &[u8]) -> String {
    match first_diff(original, rebuilt) {
        None => format!("{} bytes, identical", original.len()),
        Some(offset) => format!(
            "original {} bytes, re-encoded {} bytes, first difference at offset {offset:#x}",
            original.len(),
            rebuilt.len()
        ),
    }
}

fn unknown_step(kind: &str) -> Error {
    Error::new(
        codes::CORE_UNKNOWN_STEP,
        format!("no layer or codec is registered as \"{kind}\""),
    )
}

fn encode_failed(index: usize, step: &Step, error: &Error) -> StepReport {
    StepReport {
        index,
        kind: step.kind.clone(),
        verdict: Verdict::Different,
        reason_key: KEY_ENCODE_FAILED,
        detail: format!("encoding failed with {}", error.code()),
    }
}

/// Decodes and re-encodes `input` with `recipe` and compares the result with the original.
///
/// A decode failure is an `Err` (the file cannot be opened at all). Everything else is a verdict:
/// an encode failure of a step is `Different`, never a panic.
pub fn check_round_trip(
    input: &[u8],
    recipe: &Recipe,
    reg: &Registry,
    limits: &Limits,
) -> Result<GuardReport> {
    let out = pipeline::decode(input, recipe, reg, limits)?;
    let (codec_step, layer_steps) = out
        .recipe
        .steps
        .split_last()
        .ok_or_else(|| Error::new(codes::CORE_INVALID_RECIPE, "the recipe has no steps"))?;
    let mut steps: Vec<StepReport> = Vec::with_capacity(out.recipe.steps.len());

    // Layers, outermost first: frames[i] is the layer's wrapped form, frames[i + 1] its content.
    for (index, step) in layer_steps.iter().enumerate() {
        let (Some(wrapped), Some(content)) = (out.frames.get(index), out.frames.get(index + 1))
        else {
            return Err(Error::new(
                codes::CORE_INVALID_RECIPE,
                "the decoded frames do not match the recipe",
            ));
        };
        let layer = reg
            .layer(&step.kind)
            .ok_or_else(|| unknown_step(&step.kind))?;
        let rebuilt = match layer.encode(content, &step.params) {
            Ok(bytes) => bytes,
            Err(error) => {
                steps.push(encode_failed(index, step, &error));
                continue;
            }
        };
        let detail = diff_detail(wrapped, &rebuilt);
        let (verdict, reason_key) = if rebuilt == *wrapped {
            (Verdict::Exact, KEY_EXACT)
        } else {
            // Different bytes: fine if unwrapping them again gives the same content.
            let mut ctx = DecodeCtx::new(limits).with_step(index, step.params.clone());
            let same_content = match layer.decode(&rebuilt, &mut ctx) {
                Ok((again, _)) => again == *content,
                Err(_) => false,
            };
            if same_content {
                (Verdict::Semantic, KEY_SEMANTIC_LAYER)
            } else {
                (Verdict::Different, KEY_DIFFERENT_LAYER)
            }
        };
        steps.push(StepReport {
            index,
            kind: step.kind.clone(),
            verdict,
            reason_key,
            detail,
        });
    }

    // Codec: the document written back, compared with the codec's input frame.
    let index = layer_steps.len();
    let codec_input = out
        .frames
        .last()
        .ok_or_else(|| Error::new(codes::CORE_INVALID_RECIPE, "no decoded frames"))?;
    let codec = reg
        .codec(&codec_step.kind)
        .ok_or_else(|| unknown_step(&codec_step.kind))?;
    match codec.encode(&out.doc, &codec_step.params) {
        Err(error) => steps.push(encode_failed(index, codec_step, &error)),
        Ok(rebuilt) => {
            let detail = diff_detail(codec_input, &rebuilt);
            let (verdict, reason_key) = if rebuilt == *codec_input {
                (Verdict::Exact, KEY_EXACT)
            } else {
                let mut ctx = DecodeCtx::new(limits).with_step(index, codec_step.params.clone());
                let same_doc = match codec.decode(&rebuilt, &mut ctx) {
                    Ok(decoded) => out.doc.tree_eq(&decoded.doc),
                    Err(_) => false,
                };
                if same_doc {
                    (Verdict::Semantic, KEY_SEMANTIC_CODEC)
                } else {
                    (Verdict::Different, KEY_DIFFERENT_CODEC)
                }
            };
            steps.push(StepReport {
                index,
                kind: codec_step.kind.clone(),
                verdict,
                reason_key,
                detail,
            });
        }
    }

    let overall = steps
        .iter()
        .map(|report| report.verdict)
        .max()
        .unwrap_or(Verdict::Exact);
    Ok(GuardReport {
        overall,
        steps,
        editable: overall != Verdict::Different,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::codec::{Codec, Confidence, Decoded, Layer, SniffCtx};
    use crate::doc::{Node, SaveDoc, Value};
    use crate::params::Params;

    #[test]
    fn worst_verdict_is_the_largest() {
        assert!(Verdict::Exact < Verdict::Semantic);
        assert!(Verdict::Semantic < Verdict::Different);
        assert_eq!(
            [Verdict::Exact, Verdict::Different, Verdict::Semantic]
                .into_iter()
                .max(),
            Some(Verdict::Different)
        );
    }

    /// XOR with the `key` parameter: exact in both directions.
    struct XorLayer;

    fn xor(data: &[u8], key: &[u8]) -> Vec<u8> {
        data.iter()
            .zip(key.iter().cycle())
            .map(|(byte, key_byte)| byte ^ key_byte)
            .collect()
    }

    impl Layer for XorLayer {
        fn id(&self) -> &'static str {
            "test.xor"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            let key = ctx.hints().bytes("key")?.to_vec();
            let mut params = Params::new();
            params.set("key", key.clone());
            Ok((xor(input, &key), params))
        }
        fn encode(&self, inner: &[u8], params: &Params) -> Result<Vec<u8>> {
            Ok(xor(inner, params.bytes("key")?))
        }
    }

    /// Decode drops trailing spaces, encode does not add them back.
    struct TrimLayer;

    impl Layer for TrimLayer {
        fn id(&self) -> &'static str {
            "test.trim"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            let end = input.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
            Ok((input[..end].to_vec(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.to_vec())
        }
    }

    /// Decode lowercases (not reversible) and encode reverses the bytes (does not undo decode).
    struct LossyLayer;

    impl Layer for LossyLayer {
        fn id(&self) -> &'static str {
            "test.lossy"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.to_ascii_lowercase(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.iter().rev().copied().collect())
        }
    }

    /// Decodes fine, always fails to encode.
    struct FailEncodeLayer;

    impl Layer for FailEncodeLayer {
        fn id(&self) -> &'static str {
            "test.failenc"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.to_vec(), Params::new()))
        }
        fn encode(&self, _inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Err(Error::new(codes::CORE_INVALID_PARAM, "cannot encode"))
        }
    }

    /// Builds a root array of strings from `parts`.
    fn string_doc(ctx: &mut DecodeCtx<'_>, parts: &[&str]) -> Result<SaveDoc> {
        let mut doc = ctx.new_doc();
        let mut ids = Vec::new();
        for part in parts {
            ctx.charge_nodes(1)?;
            ids.push(doc.push(Node::new(Value::Str((*part).to_string())))?);
        }
        ctx.charge_nodes(1)?;
        let root = doc.push(Node::new(Value::Array(ids)))?;
        doc.set_root(root);
        Ok(doc)
    }

    /// The strings of a root array.
    fn strings(doc: &SaveDoc) -> Result<Vec<&str>> {
        let Some(Value::Array(ids)) = doc.get(doc.root()).map(|n| &n.value) else {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                "root is not an array",
            ));
        };
        let mut parts = Vec::new();
        for id in ids {
            match doc.get(*id).map(|n| &n.value) {
                Some(Value::Str(text)) => parts.push(text.as_str()),
                _ => return Err(Error::new(codes::CORE_INVALID_PARAM, "not a string")),
            }
        }
        Ok(parts)
    }

    fn utf8(input: &[u8]) -> Result<&str> {
        std::str::from_utf8(input)
            .map_err(|e| Error::new(codes::CORE_INVALID_PARAM, "not UTF-8").with_source(e))
    }

    /// Lines split on `\n`; joining restores the bytes exactly.
    struct LinesCodec;

    impl Codec for LinesCodec {
        fn id(&self) -> &'static str {
            "test.lines"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            let parts: Vec<&str> = utf8(input)?.split('\n').collect();
            Ok(Decoded {
                doc: string_doc(ctx, &parts)?,
                params: Params::new(),
            })
        }
        fn encode(&self, doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(strings(doc)?.join("\n").into_bytes())
        }
    }

    /// Splits on any whitespace run and joins with one space: normalises spacing.
    struct WordsCodec;

    impl Codec for WordsCodec {
        fn id(&self) -> &'static str {
            "test.words"
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            let parts: Vec<&str> = utf8(input)?.split_whitespace().collect();
            Ok(Decoded {
                doc: string_doc(ctx, &parts)?,
                params: Params::new(),
            })
        }
        fn encode(&self, doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(strings(doc)?.join(" ").into_bytes())
        }
    }

    fn registry() -> Registry {
        let mut reg = Registry::default();
        reg.add_layer(Arc::new(XorLayer));
        reg.add_layer(Arc::new(TrimLayer));
        reg.add_layer(Arc::new(LossyLayer));
        reg.add_layer(Arc::new(FailEncodeLayer));
        reg.add_codec(Arc::new(LinesCodec));
        reg.add_codec(Arc::new(WordsCodec));
        reg
    }

    const KEY: [u8; 3] = [0x5A, 0x13, 0xC4];

    fn xor_step() -> Step {
        let mut params = Params::new();
        params.set("key", KEY.to_vec());
        Step::new("test.xor", params)
    }

    fn check(input: &[u8], steps: Vec<Step>) -> GuardReport {
        check_round_trip(input, &Recipe::new(steps), &registry(), &Limits::default()).unwrap()
    }

    #[test]
    fn exact_round_trip_is_editable() {
        let input = xor(b"hello\nworld\n", &KEY);
        let report = check(&input, vec![xor_step(), Step::bare("test.lines")]);
        assert_eq!(report.steps.len(), 2);
        assert!(report.steps.iter().all(|s| s.verdict == Verdict::Exact));
        assert!(report.steps.iter().all(|s| s.reason_key == "guard.exact"));
        assert_eq!(report.steps[0].index, 0);
        assert_eq!(report.steps[1].kind, "test.lines");
        assert_eq!(report.overall, Verdict::Exact);
        assert!(report.editable);
    }

    #[test]
    fn semantic_layer_is_editable() {
        let report = check(
            b"hello  ",
            vec![Step::bare("test.trim"), Step::bare("test.lines")],
        );
        assert_eq!(report.steps[0].verdict, Verdict::Semantic);
        assert_eq!(report.steps[0].reason_key, "guard.semantic.layer");
        assert_eq!(report.steps[1].verdict, Verdict::Exact);
        assert_eq!(report.overall, Verdict::Semantic);
        assert!(report.editable);
    }

    #[test]
    fn irreversible_layer_is_different_and_not_editable() {
        let report = check(
            b"ABC\nxyz",
            vec![Step::bare("test.lossy"), Step::bare("test.lines")],
        );
        assert_eq!(report.steps[0].verdict, Verdict::Different);
        assert_eq!(report.steps[0].reason_key, "guard.different.layer");
        assert_eq!(report.overall, Verdict::Different);
        assert!(!report.editable);
    }

    #[test]
    fn normalising_codec_is_semantic() {
        let report = check(b"a  b", vec![Step::bare("test.words")]);
        assert_eq!(report.steps.len(), 1);
        assert_eq!(report.steps[0].verdict, Verdict::Semantic);
        assert_eq!(report.steps[0].reason_key, "guard.semantic.codec");
        assert_eq!(report.overall, Verdict::Semantic);
        assert!(report.editable);
    }

    #[test]
    fn encode_failure_is_different_not_a_panic() {
        let report = check(
            b"abc",
            vec![Step::bare("test.failenc"), Step::bare("test.lines")],
        );
        assert_eq!(report.steps[0].verdict, Verdict::Different);
        assert_eq!(report.steps[0].reason_key, "guard.encode_failed");
        assert!(
            report.steps[0]
                .detail
                .contains(codes::CORE_INVALID_PARAM.as_str())
        );
        assert!(!report.editable);
    }

    #[test]
    fn detail_has_the_first_difference_and_overall_is_the_worst() {
        let report = check(
            b"ab  ",
            vec![Step::bare("test.trim"), Step::bare("test.lines")],
        );
        let detail = &report.steps[0].detail;
        assert!(detail.contains("0x2"), "{detail}");
        assert!(detail.contains('4'), "{detail}");

        let mixed = check(
            b"ABC\nxyz",
            vec![Step::bare("test.lossy"), Step::bare("test.lines")],
        );
        let worst = mixed.steps.iter().map(|s| s.verdict).max().unwrap();
        assert_eq!(mixed.overall, worst);
    }

    #[test]
    fn decode_failure_is_an_error() {
        let error = check_round_trip(
            &[0xFF, 0xFE],
            &Recipe::new(vec![Step::bare("test.lines")]),
            &registry(),
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
    }

    #[test]
    fn first_diff_handles_lengths() {
        assert_eq!(first_diff(b"abc", b"abc"), None);
        assert_eq!(first_diff(b"abc", b"abd"), Some(2));
        assert_eq!(first_diff(b"abc", b"ab"), Some(2));
        assert_eq!(first_diff(b"", b"a"), Some(0));
    }
}
