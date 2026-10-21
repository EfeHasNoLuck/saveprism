//! Round-trip guard (signature only; the body arrives in a later task).
//!
//! Before editing is enabled, `decode -> encode` is compared with the original bytes,
//! step by step; the worst verdict decides.

use crate::error::{Error, Result, codes};
use crate::limits::Limits;
use crate::recipe::Recipe;
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

/// Decodes and re-encodes `input` with `recipe` and compares the result with the original.
pub fn check_round_trip(
    _input: &[u8],
    _recipe: &Recipe,
    _reg: &Registry,
    _limits: &Limits,
) -> Result<GuardReport> {
    Err(Error::new(
        codes::CORE_NOT_IMPLEMENTED,
        "check_round_trip is not implemented yet",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn check_round_trip_is_not_implemented_yet() {
        let error = check_round_trip(
            b"{}",
            &Recipe::new(Vec::new()),
            &Registry::default(),
            &Limits::default(),
        )
        .unwrap_err();
        assert_eq!(error.code(), codes::CORE_NOT_IMPLEMENTED);
    }
}
