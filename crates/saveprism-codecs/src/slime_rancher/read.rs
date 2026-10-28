//! Reads the envelope, name tables, summary, currency and ammo-slot records of a save.

use saveprism_core::{DecodeCtx, IntWidth, NodeId, Result, SaveDoc, Value};

use crate::slime_rancher::{BEGIN, END, LOOKUP_TAG, parse_error, push};

/// Exact header of a currency record: tag `SRCRNCY`, version 1, begin mark.
const CURRENCY_HEAD: &[u8] = b"\x07SRCRNCY\x01\x00\x00\x00\x00\x30";
/// Exact header of an ammo-slot record: tag `SRAD`, version 2, begin mark.
const SLOT_HEAD: &[u8] = b"\x04SRAD\x02\x00\x00\x00\x00\x30";
/// Exact header of the emotion record inside a slot: tag `SRSED`, version 1, begin mark.
const EMOTIONS_HEAD: &[u8] = b"\x05SRSED\x01\x00\x00\x00\x00\x30";
/// The name table that item indexes point into.
const ITEM_TABLE: &str = "IdentifiableType";
/// Entries accepted in one currency or emotion record; the samples have 1 and 4.
const MAX_SMALL_LIST: i32 = 64;
/// Slots accepted in one group; the vacpack has 7 in the samples.
const MAX_SLOTS: i32 = 1024;
/// Highest root-record version accepted (the samples are version 11).
const MAX_ROOT_VERSION: i32 = 1000;

/// A currency record that parsed completely.
struct Currency {
    start: usize,
    amount_at: usize,
    amount: i32,
}

/// One ammo slot.
struct Slot {
    item: i32,
    count: i32,
    count_at: usize,
}

/// Slots that follow one slot count.
struct SlotGroup {
    start: usize,
    slots: Vec<Slot>,
}

/// What the summary at the top of the root record holds.
struct Summary {
    save_name: String,
    game_version: String,
    currency: Currency,
}

/// True when `input` starts with an empty-tag root record whose body opens with `INDXTBLLKUP`.
pub(crate) fn has_header(input: &[u8]) -> bool {
    input.first() == Some(&0)
        && input.get(5..7) == Some(&BEGIN[..])
        && input.get(7..7 + LOOKUP_TAG.len()) == Some(LOOKUP_TAG)
        && input
            .get(1..5)
            .and_then(|bytes| bytes.try_into().ok())
            .map(i32::from_le_bytes)
            .is_some_and(|version| (0..=MAX_ROOT_VERSION).contains(&version))
}

/// Builds the document. Also returns the offset of the summary's money when it equals the player's
/// money, so the writer keeps the two in step.
pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Option<usize>)> {
    if !has_header(input) {
        return Err(parse_error("not a Slime Rancher 2 save", 0));
    }
    if input.get(input.len().saturating_sub(2)..) != Some(&END[..]) {
        return Err(parse_error(
            "the Slime Rancher 2 save does not end with its root end mark (cut short?)",
            input.len(),
        ));
    }
    let mut cursor = Cursor::new(input, 1);
    let root_version = cursor.i32()?;
    cursor.expect(&BEGIN, "the root begin mark")?;
    let tables = name_tables(&mut cursor)?;
    let items = tables
        .iter()
        .find(|(name, _)| name == ITEM_TABLE)
        .map(|(_, names)| names.as_slice())
        .unwrap_or_default();
    let summary = summary(&mut Cursor::new(input, cursor.pos)).ok();

    let currencies: Vec<Currency> = find_all(input, CURRENCY_HEAD)
        .filter_map(|at| currency(input, at).ok())
        .collect();
    let summary_at = summary.as_ref().map(|summary| summary.currency.start);
    let mut players = currencies
        .iter()
        .filter(|currency| Some(currency.start) != summary_at);
    let player = match (players.next(), players.next()) {
        (Some(player), None) => Some(player),
        _ => None,
    };
    let groups = slot_groups(input);
    let vacpack = player.and_then(|player| groups.iter().position(|g| g.start > player.start));

    let mut doc = ctx.new_doc();
    let mut entries = Vec::new();
    let version = push(&mut doc, int(root_version), "format-version", Some(1), true)?;
    entries.push(("format_version".to_owned(), version));
    if let Some(summary) = &summary {
        let name = push(
            &mut doc,
            Value::Str(summary.save_name.clone()),
            "save-name",
            None,
            true,
        )?;
        entries.push(("save_name".to_owned(), name));
        let game = push(
            &mut doc,
            Value::Str(summary.game_version.clone()),
            "game-version",
            None,
            true,
        )?;
        entries.push(("game_version".to_owned(), game));
    }
    if let Some(player) = player {
        let money = push(
            &mut doc,
            int(player.amount),
            "money",
            Some(player.amount_at),
            false,
        )?;
        entries.push(("money".to_owned(), money));
    }
    let mut storage = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let id = slots_node(&mut doc, group, items)?;
        if Some(index) == vacpack {
            entries.push(("vacpack".to_owned(), id));
        } else {
            storage.push(id);
        }
    }
    if !storage.is_empty() {
        let id = push(&mut doc, Value::Array(storage), "storage", None, true)?;
        entries.push(("storage".to_owned(), id));
    }
    let root = push(&mut doc, Value::Object(entries), "save", None, true)?;
    doc.set_root(root);

    let mirror = summary
        .filter(|summary| player.is_some_and(|player| player.amount == summary.currency.amount))
        .map(|summary| summary.currency.amount_at);
    Ok((doc, mirror))
}

fn int(v: i32) -> Value {
    Value::Int {
        v: i128::from(v),
        width: IntWidth::I32,
    }
}

fn slots_node(doc: &mut SaveDoc, group: &SlotGroup, items: &[String]) -> Result<NodeId> {
    let mut slots = Vec::with_capacity(group.slots.len());
    for slot in &group.slots {
        let filled = slot.item >= 0;
        let item = match usize::try_from(slot.item).ok() {
            Some(index) => Value::Str(
                items
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("#{index}")),
            ),
            None => Value::Null,
        };
        let item = push(doc, item, "item", None, true)?;
        let count = push(doc, int(slot.count), "count", Some(slot.count_at), !filled)?;
        let object = Value::Object(vec![("item".to_owned(), item), ("count".to_owned(), count)]);
        slots.push(push(doc, object, "slot", None, true)?);
    }
    push(doc, Value::Array(slots), "slots", None, true)
}

/// The first `INDXTBLLKUP`: table names and their entries.
fn name_tables(cursor: &mut Cursor<'_>) -> Result<Vec<(String, Vec<String>)>> {
    cursor.header("INDXTBLLKUP")?;
    let count = cursor.count(1)?;
    let mut tables = Vec::with_capacity(count);
    for _ in 0..count {
        let name = cursor.string()?;
        cursor.header("INDXTBL")?;
        let entries = cursor.count(1)?;
        let mut names = Vec::with_capacity(entries);
        for _ in 0..entries {
            names.push(cursor.string()?);
        }
        cursor.expect(&END, "the end of a name table")?;
        tables.push((name, names));
    }
    cursor.expect(&END, "the end of the name tables")?;
    Ok(tables)
}

fn summary(cursor: &mut Cursor<'_>) -> Result<Summary> {
    let save_name = cursor.string()?;
    let _slot = cursor.string()?;
    let _unknown = cursor.i32()?;
    cursor.header("")?;
    let game_version = cursor.string()?;
    let _unknown = cursor.i32()?;
    let currency = currency(cursor.data, cursor.pos)?;
    Ok(Summary {
        save_name,
        game_version,
        currency,
    })
}

/// A complete `SRCRNCY` version 1 record at `start`.
fn currency(input: &[u8], start: usize) -> Result<Currency> {
    let mut cursor = Cursor::new(input, start);
    cursor.expect(CURRENCY_HEAD, "a currency record")?;
    let count = cursor.i32()?;
    if !(1..=MAX_SMALL_LIST).contains(&count) {
        return Err(parse_error("unexpected currency entry count", cursor.pos));
    }
    let amount_at = cursor.pos;
    let amount = cursor.i32()?;
    if amount < 0 {
        return Err(parse_error("implausible currency amount", amount_at));
    }
    cursor.take(4)?;
    for _ in 1..count {
        cursor.take(8)?;
    }
    cursor.expect(&END, "the end of a currency record")?;
    Ok(Currency {
        start,
        amount_at,
        amount,
    })
}

/// A complete `SRAD` version 2 record at `start`; returns the slot and the offset after it.
fn slot(input: &[u8], start: usize) -> Result<(Slot, usize)> {
    let mut cursor = Cursor::new(input, start);
    cursor.expect(SLOT_HEAD, "an ammo slot")?;
    let item = cursor.i32()?;
    let count_at = cursor.pos;
    let count = cursor.i32()?;
    cursor.take(4)?;
    cursor.expect(EMOTIONS_HEAD, "slot emotions")?;
    let emotions = cursor.i32()?;
    if !(0..=MAX_SMALL_LIST).contains(&emotions) {
        return Err(parse_error("unexpected emotion count", cursor.pos));
    }
    for _ in 0..emotions {
        cursor.take(8)?;
    }
    cursor.expect(&END, "the end of slot emotions")?;
    cursor.byte()?;
    cursor.expect(&END, "the end of an ammo slot")?;
    if item < -1 || count < 0 || (item == -1 && count != 0) {
        return Err(parse_error("implausible ammo slot", start));
    }
    Ok((
        Slot {
            item,
            count,
            count_at,
        },
        cursor.pos,
    ))
}

/// Runs of back-to-back slots right after an `int32` that equals the run length.
fn slot_groups(input: &[u8]) -> Vec<SlotGroup> {
    let mut groups = Vec::new();
    let mut next_free = 0usize;
    for start in find_all(input, SLOT_HEAD) {
        if start < next_free {
            continue;
        }
        let mut slots = Vec::new();
        let mut pos = start;
        while let Ok((slot, end)) = slot(input, pos) {
            slots.push(slot);
            pos = end;
        }
        next_free = pos.max(start.saturating_add(1));
        let announced = start
            .checked_sub(4)
            .and_then(|at| input.get(at..start))
            .and_then(|bytes| bytes.try_into().ok())
            .map(i32::from_le_bytes);
        let fits = announced
            .and_then(|count| usize::try_from(count).ok())
            .is_some_and(|count| count == slots.len());
        if fits && announced.is_some_and(|count| count <= MAX_SLOTS) {
            groups.push(SlotGroup { start, slots });
        }
    }
    groups
}

/// Every offset where `needle` starts.
fn find_all<'a>(haystack: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(move |(_, window)| *window == needle)
        .map(|(at, _)| at)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let bytes = self
            .pos
            .checked_add(len)
            .and_then(|end| self.data.get(self.pos..end))
            .ok_or_else(|| parse_error("the Slime Rancher 2 save is cut short", self.pos))?;
        self.pos = self.pos.saturating_add(len);
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8> {
        let at = self.pos;
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| parse_error("the Slime Rancher 2 save is cut short", at))
    }

    fn i32(&mut self) -> Result<i32> {
        let bytes = self.take(4)?;
        let array: [u8; 4] = bytes
            .try_into()
            .map_err(|_| parse_error("the Slime Rancher 2 save is cut short", self.pos))?;
        Ok(i32::from_le_bytes(array))
    }

    /// An `int32` count of items that take at least `min_item` bytes each.
    fn count(&mut self, min_item: usize) -> Result<usize> {
        let at = self.pos;
        let count =
            usize::try_from(self.i32()?).map_err(|_| parse_error("a negative count", at))?;
        let remaining = self.data.len().saturating_sub(self.pos);
        if count.saturating_mul(min_item) > remaining {
            return Err(parse_error("a count is larger than the file", at));
        }
        Ok(count)
    }

    fn expect(&mut self, bytes: &[u8], what: &str) -> Result<()> {
        let at = self.pos;
        if self.take(bytes.len())? != bytes {
            return Err(parse_error(format!("expected {what}"), at));
        }
        Ok(())
    }

    /// A .NET `BinaryWriter` string: 7-bit encoded length (at most 5 bytes), then UTF-8 bytes.
    fn string(&mut self) -> Result<String> {
        let at = self.pos;
        let mut len = 0u32;
        let mut shift = 0u32;
        loop {
            let byte = self.byte()?;
            if shift == 28 && byte > 0x07 {
                return Err(parse_error("a string length is too large", at));
            }
            len |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        let len = usize::try_from(len).map_err(|_| parse_error("a string is too long", at))?;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| parse_error("a string is not UTF-8", at))
    }

    /// A record header with the given tag: tag, `int32` version, begin mark. Returns the version.
    fn header(&mut self, tag: &str) -> Result<i32> {
        let at = self.pos;
        if self.string()? != tag {
            return Err(parse_error(format!("expected the record `{tag}`"), at));
        }
        let version = self.i32()?;
        self.expect(&BEGIN, "a record begin mark")?;
        Ok(version)
    }
}
