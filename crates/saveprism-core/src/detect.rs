//! Automatic recipe detection (signature only; the body arrives in a later task).

use crate::codec::Confidence;
use crate::error::{Error, Result, codes};
use crate::limits::Limits;
use crate::recipe::Recipe;
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

/// Finds a recipe for `input` (magic bytes, entropy, trial decoding, validation).
pub fn detect(
    _input: &[u8],
    _reg: &Registry,
    _limits: &Limits,
    _opts: &DetectOptions,
) -> Result<Detection> {
    Err(Error::new(
        codes::CORE_NOT_IMPLEMENTED,
        "detect is not implemented yet",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let opts = DetectOptions::default();
        assert_eq!(opts.max_attempts, 64);
        assert_eq!(opts.max_alternatives, 3);
        assert_eq!(opts.file_name, None);
    }

    #[test]
    fn detect_is_not_implemented_yet() {
        let error = detect(
            b"{}",
            &Registry::default(),
            &Limits::default(),
            &DetectOptions::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_NOT_IMPLEMENTED);
    }
}
