//! Resource limits applied to untrusted input (size, decompression ratio, depth, node count, layers).

use crate::error::{Error, Result, codes};

const MIB: u64 = 1024 * 1024;

/// Limits enforced while decoding. Every parser must respect them.
#[derive(Clone, Debug, PartialEq)]
pub struct Limits {
    /// Largest accepted input in bytes.
    pub max_input_bytes: u64,
    /// Largest accepted decoded output of a single step in bytes.
    pub max_decoded_bytes: u64,
    /// Largest accepted output/input ratio; only enforced above `ratio_floor_bytes`.
    pub max_ratio: u64,
    /// Outputs up to this size are always accepted regardless of the ratio.
    pub ratio_floor_bytes: u64,
    /// Deepest accepted nesting level.
    pub max_depth: usize,
    /// Largest accepted node count.
    pub max_nodes: u64,
    /// Largest accepted number of layers in a recipe (the codec step is not counted).
    pub max_layers: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 * MIB,
            max_decoded_bytes: 1024 * MIB,
            max_ratio: 100,
            ratio_floor_bytes: 16 * MIB,
            max_depth: 512,
            max_nodes: 50_000_000,
            max_layers: 8,
        }
    }
}

impl Limits {
    /// Fails with `CORE_LIMIT_INPUT` when the input is larger than `max_input_bytes`.
    pub fn check_input(&self, len: u64) -> Result<()> {
        if len > self.max_input_bytes {
            return Err(Error::new(
                codes::CORE_LIMIT_INPUT,
                format!(
                    "input is {len} bytes, the limit is {} bytes",
                    self.max_input_bytes
                ),
            ));
        }
        Ok(())
    }

    /// The largest output a step may produce for an input of `input_len` bytes:
    /// `min(max_decoded_bytes, max(ratio_floor_bytes, input_len * max_ratio))`.
    pub fn output_budget(&self, input_len: u64) -> u64 {
        let by_ratio = input_len.saturating_mul(self.max_ratio);
        self.max_decoded_bytes
            .min(self.ratio_floor_bytes.max(by_ratio))
    }

    /// Fails with `CORE_LIMIT_DECODED` when `output_len` exceeds [`Limits::output_budget`].
    pub fn check_output(&self, input_len: u64, output_len: u64) -> Result<()> {
        let budget = self.output_budget(input_len);
        if output_len > budget {
            return Err(Error::new(
                codes::CORE_LIMIT_DECODED,
                format!(
                    "decoded output is {output_len} bytes for a {input_len} byte input, the budget is {budget} bytes (possible decompression bomb)"
                ),
            ));
        }
        Ok(())
    }
}

/// Converts a slice length to `u64` (lossless on every supported platform).
pub fn len_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let limits = Limits::default();
        assert_eq!(limits.max_input_bytes, 256 * 1024 * 1024);
        assert_eq!(limits.max_decoded_bytes, 1024 * 1024 * 1024);
        assert_eq!(limits.max_ratio, 100);
        assert_eq!(limits.ratio_floor_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_depth, 512);
        assert_eq!(limits.max_nodes, 50_000_000);
        assert_eq!(limits.max_layers, 8);
    }

    #[test]
    fn check_input_boundary() {
        let limits = Limits {
            max_input_bytes: 10,
            ..Limits::default()
        };
        assert!(limits.check_input(10).is_ok());
        let error = limits.check_input(11).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_INPUT);
    }

    #[test]
    fn output_budget_follows_floor_ratio_and_cap() {
        let limits = Limits::default();
        assert_eq!(limits.output_budget(0), 16 * MIB);
        assert_eq!(limits.output_budget(1000), 16 * MIB);
        assert_eq!(limits.output_budget(MIB), 100 * MIB);
        assert_eq!(limits.output_budget(100 * MIB), 1024 * MIB);
        assert_eq!(limits.output_budget(u64::MAX), 1024 * MIB);
    }

    #[test]
    fn check_output_rejects_bombs() {
        let limits = Limits::default();
        assert!(limits.check_output(100, 16 * MIB).is_ok());
        let error = limits.check_output(100, 16 * MIB + 1).unwrap_err();
        assert_eq!(error.code(), codes::CORE_LIMIT_DECODED);
        assert!(limits.check_output(MIB, 100 * MIB).is_ok());
        assert!(limits.check_output(MIB, 100 * MIB + 1).is_err());
    }

    #[test]
    fn len_conversion() {
        assert_eq!(len_u64(5), 5);
    }
}
