//! `format.slime-rancher-2`: Slime Rancher 2 save games (`<id>_<slot>.sav`).
//!
//! Written from the bytes of the maintainer's own saves (game 1.3.0, 2026-10-28); no public format
//! description was found and no game or tool code was used. The full behavioral spec is in
//! `docs/research/formats-slime-rancher.md`. Nothing in a save is executed.
//!
//! # Layout
//!
//! The file is one record. A record is a tag (a .NET `BinaryWriter` string: 7-bit length, then
//! UTF-8), an `int32` version, the begin mark `00 30`, the body and the end mark `00 40`. The root
//! record has an empty tag. Bodies mix plain little-endian values with nested records and have no
//! length fields, so a body can only be read with its schema. This codec knows a few schemas and
//! leaves every other byte as it is:
//!
//! - `INDXTBLLKUP` (first in the root): name tables. `int32` count, then per table its name and an
//!   `INDXTBL` record (`int32` count, then that many strings). Other records store an index into a
//!   table instead of a name; `IdentifiableType` names the items.
//! - The summary that follows (save name, one more string, an `int32`, then an empty-tag record
//!   with the game version, an `int32` and a currency record). The load screen shows it.
//! - `SRCRNCY` version 1: `int32` count, then per entry the `int32` amount and an `int32` that is
//!   250 in every sample (meaning unknown). There are two: the summary copy and the player's.
//! - `SRAD` version 2 (an ammo slot): `int32` item index into `IdentifiableType` (`-1` = empty),
//!   `int32` count, `int32` (0 in every sample), an `SRSED` version 1 record (`int32` count, then
//!   `int32` key and `float32` per entry), one byte (0 in every sample). Slots come in groups: an
//!   `int32` slot count right before the slots. The player's vacpack is the first group after the
//!   player's currency record. (`SRAD` version 4 is a different record, a world actor.)
//!
//! Currency and slot records are found by their exact header bytes and accepted only when the whole
//! record parses and ends with `00 40`; anything else is not shown.
//!
//! # Tree
//!
//! The root is a read-only `Object` with the hint `SR2:save`:
//!
//! | Key | Node | Editable |
//! |---|---|---|
//! | `format_version` | `Int` (I32), the root record version | no |
//! | `save_name`, `game_version` | `Str` from the summary | no |
//! | `money` | `Int` (I32), the player's newbucks | yes |
//! | `vacpack` | `Object` `SR2:slots`, one count per slot | counts of filled slots |
//! | `storage` | `Object` of slot groups (`Storage 1`, ...: plots and the like) | counts of filled slots |
//!
//! A slot is one `Int` (I32) count under the key `Slot <n>: <item>`; the item is the
//! `IdentifiableType` name made readable (`SlimeDefinition.Tabby` → `Tabby Slime`,
//! `IdentifiableType.PhosphorPlort` → `Phosphor Plort`) or `empty`. An empty slot's count is
//! read-only.
//!
//! # Exact writing
//!
//! The decoded bytes are kept as a parameter. Writing copies them and puts every editable value
//! back at its byte offset (same width), so an unchanged document gives the original bytes. When
//! the summary copy of the money equalled the player's money, it is written along with it, so the
//! load screen shows the new amount. Negative money or counts are refused.

mod read;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, NodeId,
    ParamValue, Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The file is not a Slime Rancher 2 save, or its header or name tables are cut short.
pub const SR2_PARSE: ErrorCode = ErrorCode::new("SP-SR2-001");
/// A value cannot be written back (negative, out of place, or the original bytes are missing).
pub const SR2_WRITE: ErrorCode = ErrorCode::new("SP-SR2-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "SR2:";

/// Begin mark after a record's tag and version.
pub(crate) const BEGIN: [u8; 2] = [0x00, 0x30];
/// End mark after a record's body.
pub(crate) const END: [u8; 2] = [0x00, 0x40];
/// The name-table record that opens the root record (with its 7-bit length byte).
pub(crate) const LOOKUP_TAG: &[u8] = b"\x0bINDXTBLLKUP";

const ORIGINAL: &str = "original";
const SUMMARY_MONEY_AT: &str = "summary_money_at";
const HINT_MONEY: &str = "SR2:money";
const HINT_COUNT: &str = "SR2:count";

/// Registers the Slime Rancher 2 codec as `format.slime-rancher-2`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(SlimeRancher2Codec));
}

/// The `format.slime-rancher-2` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct SlimeRancher2Codec;

impl Codec for SlimeRancher2Codec {
    fn id(&self) -> &'static str {
        "format.slime-rancher-2"
    }

    /// An empty-tag root record whose body starts with the `INDXTBLLKUP` name tables.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if read::has_header(input) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, summary_money_at) = read::read_all(input, shared)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        if let Some(at) = summary_money_at {
            params.set(SUMMARY_MONEY_AT, i64::try_from(at).unwrap_or(i64::MAX));
        }
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get(ORIGINAL) else {
            return Err(write_error("the original Slime Rancher 2 save is missing"));
        };
        let mut out = original.clone();
        let mut money = None;
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(write_error("the Slime Rancher 2 tree has a cycle"));
            }
            let Some(node) = doc.get(id) else {
                continue;
            };
            stack.extend(doc.children(id));
            if node.flags.contains(NodeFlags::READ_ONLY) {
                continue;
            }
            let hint = node.type_hint.as_ref().map(TypeHint::as_str);
            if hint != Some(HINT_MONEY) && hint != Some(HINT_COUNT) {
                continue;
            }
            let (Some(span), Value::Int { v, .. }) = (node.span, &node.value) else {
                continue;
            };
            let value = non_negative(*v)?;
            write_i32(&mut out, span.start, span.end, value)?;
            if hint == Some(HINT_MONEY) {
                money = Some(value);
            }
        }
        if let (Some(value), Some(at)) = (money, params.opt_int(SUMMARY_MONEY_AT)?) {
            let at = u64::try_from(at).map_err(|_| write_error("invalid summary offset"))?;
            write_i32(&mut out, at, at.saturating_add(4), value)?;
        }
        Ok(out)
    }
}

/// Edited values must be a non-negative `int32`.
fn non_negative(value: i128) -> Result<i32> {
    i32::try_from(value)
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| write_error(format!("{value} is not a valid amount (0 to 2147483647)")))
}

fn write_i32(out: &mut [u8], start: u64, end: u64, value: i32) -> Result<()> {
    let slot = usize::try_from(start)
        .ok()
        .zip(usize::try_from(end).ok())
        .filter(|(start, end)| end.checked_sub(*start) == Some(4))
        .and_then(|(start, end)| out.get_mut(start..end));
    let Some(slot) = slot else {
        return Err(write_error("a value is outside the original save"));
    };
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

pub(crate) fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(SR2_PARSE, message).with_offset(len_u64(at))
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(SR2_WRITE, message)
}

/// Pushes a node with a hint, an optional byte span and the read-only flag when asked.
pub(crate) fn push(
    doc: &mut SaveDoc,
    value: Value,
    kind: &str,
    span: Option<usize>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(format!("{HINT_PREFIX}{kind}")));
    node.span = span.map(|start| ByteSpan {
        start: len_u64(start),
        end: len_u64(start).saturating_add(4),
    });
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}
