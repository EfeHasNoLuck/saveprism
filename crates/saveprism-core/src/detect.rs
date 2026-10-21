//! Automatic recipe detection: sniff, trial decode, validate (best-first search over layer chains).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::codec::{Confidence, DecodeCtx, SniffCtx, verify_decoded};
use crate::error::{Error, Result, codes};
use crate::limits::{Limits, len_u64};
use crate::params::Params;
use crate::recipe::{Recipe, Step};
use crate::registry::Registry;

/// Options of [`detect`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectOptions {
    /// Largest number of trial decodes before giving up (`DET_BUDGET`).
    pub max_attempts: usize,
    /// Largest number of alternative recipes returned next to the best one.
    pub max_alternatives: usize,
    /// Name of the file, when known (extension hints).
    pub file_name: Option<String>,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self {
            max_attempts: 64,
            max_alternatives: 3,
            file_name: None,
        }
    }
}

/// A recipe that decoded the input, with the detector's confidence in it.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// The recipe.
    pub recipe: Recipe,
    /// How sure the detector is.
    pub confidence: Confidence,
}

/// The result of [`detect`].
#[derive(Clone, Debug)]
pub struct Detection {
    /// The most likely recipe.
    pub best: Candidate,
    /// Other plausible recipes, best first.
    pub alternatives: Vec<Candidate>,
    /// Number of trial decodes that were made.
    pub attempts: usize,
}

/// Largest number of states kept in the search frontier.
const MAX_FRONTIER: usize = 16;

/// A partly peeled input: the bytes now, the layer steps taken so far and the score so far.
struct State {
    bytes: Vec<u8>,
    steps: Vec<Step>,
    score: f32,
    /// Sum of the sizes of all kept intermediate results (bounded by `max_decoded_bytes`).
    kept: u64,
}

/// Orders states: `Greater` means "search this one first" (higher score, then fewer steps).
fn rank(a: &State, b: &State) -> Ordering {
    a.score
        .total_cmp(&b.score)
        .then_with(|| b.steps.len().cmp(&a.steps.len()))
}

/// Removes and returns the best state of the frontier.
fn pop_best(frontier: &mut Vec<State>) -> Option<State> {
    let index = frontier
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| rank(a, b))
        .map(|(index, _)| index)?;
    Some(frontier.swap_remove(index))
}

/// Total number of bytes held by the states of the frontier.
fn frontier_bytes(frontier: &[State]) -> u64 {
    frontier.iter().fold(0_u64, |sum, state| {
        sum.saturating_add(len_u64(state.bytes.len()))
    })
}

/// Drops the worst states until at most [`MAX_FRONTIER`] are left and their bytes together stay
/// within `max_bytes`. The last remaining state is never dropped (if it alone is over the limit,
/// that is a limit error elsewhere).
fn trim_frontier(frontier: &mut Vec<State>, max_bytes: u64) {
    while frontier.len() > 1
        && (frontier.len() > MAX_FRONTIER || frontier_bytes(frontier) > max_bytes)
    {
        let Some(index) = frontier
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| rank(a, b))
            .map(|(index, _)| index)
        else {
            return;
        };
        frontier.swap_remove(index);
    }
}

/// Length plus a simple digest, used to skip bytes that were already queued.
fn fingerprint(bytes: &[u8]) -> (usize, u64) {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    (bytes.len(), hasher.finish())
}

/// Finds a recipe for `input` (magic bytes, entropy, trial decoding, validation).
///
/// Best-first search: a state is the bytes left after some layers. For every state each codec and
/// each layer whose `sniff` is above `Confidence::NONE` is tried. A codec that decodes gives a
/// candidate recipe with confidence `state score * sniff`; a layer that decodes gives a new state
/// with score `state score * sniff`. States with a higher score are searched first, ties go to the
/// shorter recipe. The search stops when `opts.max_attempts` trial decodes were made or nothing is
/// left to try. Errors of single trials (including limit errors) only end that branch; an input
/// above `Limits::max_input_bytes` fails at once.
pub fn detect(
    input: &[u8],
    reg: &Registry,
    limits: &Limits,
    opts: &DetectOptions,
) -> Result<Detection> {
    limits.check_input(len_u64(input.len()))?;

    let mut seen: HashSet<(usize, u64)> = HashSet::new();
    seen.insert(fingerprint(input));
    let mut frontier = vec![State {
        bytes: input.to_vec(),
        steps: Vec::new(),
        score: 1.0,
        kept: len_u64(input.len()),
    }];
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut attempts = 0_usize;
    let mut budget_hit = false;

    'search: while let Some(state) = pop_best(&mut frontier) {
        let depth = state.steps.len();
        let sniff_ctx = SniffCtx {
            file_name: opts.file_name.as_deref(),
            depth,
        };

        for codec in reg.codecs() {
            let sniffed = codec.sniff(&state.bytes, &sniff_ctx);
            if sniffed <= Confidence::NONE {
                continue;
            }
            if attempts >= opts.max_attempts {
                budget_hit = true;
                break 'search;
            }
            attempts += 1;
            tracing::debug!(
                kind = codec.id(),
                bytes = state.bytes.len(),
                "detect: trying codec"
            );
            let mut ctx = DecodeCtx::new(limits).with_step(depth, Params::new());
            let Ok(decoded) = codec.decode(&state.bytes, &mut ctx) else {
                continue;
            };
            if verify_decoded(&decoded.doc, limits).is_err() {
                continue;
            }
            let mut steps = state.steps.clone();
            steps.push(Step::new(codec.id(), decoded.params));
            candidates.push(Candidate {
                recipe: Recipe::new(steps),
                confidence: Confidence::new(state.score * sniffed.value()),
            });
        }

        if depth >= limits.max_layers {
            continue;
        }
        for layer in reg.layers() {
            let sniffed = layer.sniff(&state.bytes, &sniff_ctx);
            if sniffed <= Confidence::NONE {
                continue;
            }
            if attempts >= opts.max_attempts {
                budget_hit = true;
                break 'search;
            }
            attempts += 1;
            tracing::debug!(
                kind = layer.id(),
                bytes = state.bytes.len(),
                "detect: trying layer"
            );
            let mut ctx = DecodeCtx::new(limits).with_step(depth, Params::new());
            let Ok((inner, params)) = layer.decode(&state.bytes, &mut ctx) else {
                continue;
            };
            if limits
                .check_output(len_u64(state.bytes.len()), len_u64(inner.len()))
                .is_err()
            {
                continue;
            }
            let kept = state.kept.saturating_add(len_u64(inner.len()));
            if kept > limits.max_decoded_bytes {
                continue;
            }
            if !seen.insert(fingerprint(&inner)) {
                continue;
            }
            let mut steps = state.steps.clone();
            steps.push(Step::new(layer.id(), params));
            frontier.push(State {
                bytes: inner,
                steps,
                score: state.score * sniffed.value(),
                kept,
            });
            trim_frontier(&mut frontier, limits.max_decoded_bytes);
        }
    }

    // Best first; equal confidence: the shorter recipe.
    candidates.sort_by(|a, b| {
        b.confidence
            .value()
            .total_cmp(&a.confidence.value())
            .then_with(|| a.recipe.steps.len().cmp(&b.recipe.steps.len()))
    });
    candidates.truncate(opts.max_alternatives.saturating_add(1));

    let mut ranked = candidates.into_iter();
    match ranked.next() {
        Some(best) => Ok(Detection {
            best,
            alternatives: ranked.collect(),
            attempts,
        }),
        None if budget_hit => Err(Error::new(
            codes::DET_BUDGET,
            format!(
                "no recipe found within {} trial decodes (attempt budget used up)",
                opts.max_attempts
            ),
        )),
        None => Err(Error::new(
            codes::DET_NO_MATCH,
            format!("no layer or codec recognised the input ({attempts} trial decodes)"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::codec::{Codec, DecodeCtx, Decoded, Layer};
    use crate::doc::SaveDoc;

    /// Removes a leading `marker` byte; sniffs `conf` when the input starts with it.
    struct StripLayer {
        id: &'static str,
        marker: u8,
        conf: f32,
    }

    impl Layer for StripLayer {
        fn id(&self) -> &'static str {
            self.id
        }
        fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            if input.first() == Some(&self.marker) {
                Confidence::new(self.conf)
            } else {
                Confidence::NONE
            }
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.get(1..).unwrap_or_default().to_vec(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            let mut out = vec![self.marker];
            out.extend_from_slice(inner);
            Ok(out)
        }
    }

    /// Accepts input whose first byte is one of `accepts`, with confidence `conf`.
    struct FirstByteCodec {
        id: &'static str,
        accepts: &'static [u8],
        conf: f32,
    }

    impl Codec for FirstByteCodec {
        fn id(&self) -> &'static str {
            self.id
        }
        fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            match input.first() {
                Some(byte) if self.accepts.contains(byte) => Confidence::new(self.conf),
                _ => Confidence::NONE,
            }
        }
        fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            Ok(Decoded {
                doc: SaveDoc::new(),
                params: Params::new(),
            })
        }
        fn encode(&self, _doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    fn registry(codecs: Vec<FirstByteCodec>, layers: Vec<StripLayer>) -> Registry {
        let mut reg = Registry::default();
        for codec in codecs {
            reg.add_codec(Arc::new(codec));
        }
        for layer in layers {
            reg.add_layer(Arc::new(layer));
        }
        reg
    }

    fn run(input: &[u8], reg: &Registry, opts: &DetectOptions) -> Result<Detection> {
        detect(input, reg, &Limits::default(), opts)
    }

    fn state(len: usize, score: f32) -> State {
        State {
            bytes: vec![0; len],
            steps: Vec::new(),
            score,
            kept: 0,
        }
    }

    #[test]
    fn trim_frontier_keeps_total_bytes_within_the_limit() {
        let mut frontier = vec![state(40, 0.9), state(40, 0.5), state(40, 0.1)];
        trim_frontier(&mut frontier, 100);
        assert_eq!(frontier.len(), 2);
        assert!(frontier_bytes(&frontier) <= 100);
        // The lowest score went first.
        assert!(frontier.iter().all(|s| s.score > 0.2));

        // A single state over the limit is kept (mirrors the old behaviour).
        let mut single = vec![state(200, 0.5)];
        trim_frontier(&mut single, 100);
        assert_eq!(single.len(), 1);
    }

    #[test]
    fn a_branching_input_under_a_small_byte_limit_is_still_detected() {
        // Two layers match the same marker, so every step branches.
        let reg = registry(
            vec![FirstByteCodec {
                id: "c.hash",
                accepts: b"#",
                conf: 0.5,
            }],
            vec![
                StripLayer {
                    id: "l.a",
                    marker: b'x',
                    conf: 1.0,
                },
                StripLayer {
                    id: "l.b",
                    marker: b'x',
                    conf: 0.9,
                },
            ],
        );
        let limits = Limits {
            max_decoded_bytes: 64,
            ..Limits::default()
        };
        let found = detect(b"xxx#", &reg, &limits, &DetectOptions::default()).unwrap();
        assert_eq!(found.best.recipe.kinds(), ["l.a", "l.a", "l.a", "c.hash"]);
    }

    #[test]
    fn defaults_match_the_contract() {
        let opts = DetectOptions::default();
        assert_eq!(opts.max_attempts, 64);
        assert_eq!(opts.max_alternatives, 3);
        assert_eq!(opts.file_name, None);
    }

    #[test]
    fn the_most_confident_candidate_is_the_best() {
        let reg = registry(
            vec![
                FirstByteCodec {
                    id: "a.low",
                    accepts: b"#",
                    conf: 0.3,
                },
                FirstByteCodec {
                    id: "b.high",
                    accepts: b"#",
                    conf: 0.8,
                },
            ],
            Vec::new(),
        );
        let detection = run(b"#data", &reg, &DetectOptions::default()).unwrap();
        assert_eq!(detection.best.recipe.kinds(), ["b.high"]);
        assert_eq!(detection.alternatives.len(), 1);
        assert_eq!(detection.alternatives[0].recipe.kinds(), ["a.low"]);
        assert_eq!(detection.attempts, 2);
    }

    #[test]
    fn on_equal_confidence_the_shorter_recipe_wins() {
        // Direct: 1.0 * 0.5. Through the layer: 1.0 * 1.0 * 0.5.
        let reg = registry(
            vec![FirstByteCodec {
                id: "c.any",
                accepts: b"x#",
                conf: 0.5,
            }],
            vec![StripLayer {
                id: "l.x",
                marker: b'x',
                conf: 1.0,
            }],
        );
        let detection = run(b"x#", &reg, &DetectOptions::default()).unwrap();
        assert_eq!(detection.best.recipe.kinds(), ["c.any"]);
        assert_eq!(detection.alternatives.len(), 1);
        assert_eq!(detection.alternatives[0].recipe.kinds(), ["l.x", "c.any"]);
    }

    #[test]
    fn alternatives_are_limited() {
        let reg = registry(
            vec![
                FirstByteCodec {
                    id: "c.1",
                    accepts: b"#",
                    conf: 0.9,
                },
                FirstByteCodec {
                    id: "c.2",
                    accepts: b"#",
                    conf: 0.8,
                },
                FirstByteCodec {
                    id: "c.3",
                    accepts: b"#",
                    conf: 0.7,
                },
            ],
            Vec::new(),
        );
        let opts = DetectOptions {
            max_alternatives: 1,
            ..DetectOptions::default()
        };
        let detection = run(b"#", &reg, &opts).unwrap();
        assert_eq!(detection.best.recipe.kinds(), ["c.1"]);
        assert_eq!(detection.alternatives.len(), 1);
    }

    #[test]
    fn an_exhausted_budget_reports_det_budget() {
        // The layer matches ten times in a row and there is no codec.
        let reg = registry(
            Vec::new(),
            vec![StripLayer {
                id: "l.x",
                marker: b'x',
                conf: 1.0,
            }],
        );
        let limits = Limits {
            max_layers: 20,
            ..Limits::default()
        };
        let opts = DetectOptions {
            max_attempts: 3,
            ..DetectOptions::default()
        };
        let error = detect(b"xxxxxxxxxx", &reg, &limits, &opts).unwrap_err();
        assert_eq!(error.code(), codes::DET_BUDGET);
    }

    #[test]
    fn nothing_that_matches_reports_det_no_match() {
        let reg = registry(
            vec![FirstByteCodec {
                id: "c.hash",
                accepts: b"#",
                conf: 0.5,
            }],
            vec![StripLayer {
                id: "l.x",
                marker: b'x',
                conf: 1.0,
            }],
        );
        let error = run(b"zzz", &reg, &DetectOptions::default()).unwrap_err();
        assert_eq!(error.code(), codes::DET_NO_MATCH);

        // A layer chain that never reaches a codec is no match either (not a budget problem).
        let error = run(b"xxx", &reg, &DetectOptions::default()).unwrap_err();
        assert_eq!(error.code(), codes::DET_NO_MATCH);
    }

    #[test]
    fn the_layer_limit_stops_the_chain() {
        let reg = registry(
            vec![FirstByteCodec {
                id: "c.hash",
                accepts: b"#",
                conf: 0.5,
            }],
            vec![StripLayer {
                id: "l.x",
                marker: b'x',
                conf: 1.0,
            }],
        );
        let limits = Limits {
            max_layers: 2,
            ..Limits::default()
        };
        let opts = DetectOptions::default();
        let found = detect(b"xx#", &reg, &limits, &opts).unwrap();
        assert_eq!(found.best.recipe.kinds(), ["l.x", "l.x", "c.hash"]);
        let error = detect(b"xxx#", &reg, &limits, &opts).unwrap_err();
        assert_eq!(error.code(), codes::DET_NO_MATCH);
    }

    #[test]
    fn an_oversized_input_fails_at_once() {
        let reg = registry(Vec::new(), Vec::new());
        let limits = Limits {
            max_input_bytes: 2,
            ..Limits::default()
        };
        let error = detect(b"abc", &reg, &limits, &DetectOptions::default()).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
    }

    #[test]
    fn the_file_name_reaches_sniff() {
        struct ExtCodec;
        impl Codec for ExtCodec {
            fn id(&self) -> &'static str {
                "c.ext"
            }
            fn sniff(&self, _input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
                if ctx.file_name == Some("save.dat") {
                    Confidence::MEDIUM
                } else {
                    Confidence::NONE
                }
            }
            fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
                Ok(Decoded {
                    doc: SaveDoc::new(),
                    params: Params::new(),
                })
            }
            fn encode(&self, _doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
                Ok(Vec::new())
            }
        }
        let mut reg = Registry::default();
        reg.add_codec(Arc::new(ExtCodec));
        let named = DetectOptions {
            file_name: Some("save.dat".to_string()),
            ..DetectOptions::default()
        };
        assert!(run(b"?", &reg, &named).is_ok());
        let error = run(b"?", &reg, &DetectOptions::default()).unwrap_err();
        assert_eq!(error.code(), codes::DET_NO_MATCH);
    }
}
