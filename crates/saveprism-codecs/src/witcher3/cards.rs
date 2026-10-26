//! Card definitions from the player's own `def_gwint_cards_final.xml` and
//! `def_gwint_battle_king_cards.xml` files.
//!
//! The files are XML documents from the game's `content0` (and `items_plus` for New Game+). Only
//! data is read: no game content is executed and nothing is ever written back.
//!
//! The two shapes this parser accepts are the ones the game ships:
//!
//! ```xml
//! <card index="12" title="gwint_name_dandelion" name="dandelion"
//!       faction_index="F_NEUTRAL">
//!   <type_flags><flag name="TYPE_CREATURE" /></type_flags>
//! </card>
//! ```
//!
//! `title` is a localization key, not the text the player sees. Without a permissively licensed
//! `.w3strings` reader SavePrism does not decode it; the display name comes from the `name`
//! attribute (the readable identifier the game's own XML carries) and, when that is missing, from
//! the `title` key with the `gwint_name_` prefix and underscores removed. Battle-king cards live in
//! their own file and are marked with a `TYPE_KING` flag; both make a card a leader.
//!
//! The files are UTF-16LE in the legacy game and UTF-8 in the remastered one. This module decodes
//! either to UTF-8 before handing the text to `quick-xml`.

use std::collections::BTreeMap;

use encoding_rs::{UTF_8, UTF_16BE, UTF_16LE};
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use saveprism_core::{Error, Result};

use super::{W3_BUNDLE, parse_error};

/// Most cards one file may contribute.
pub const MAX_CARDS: usize = 100_000;

/// One card definition from the game's own files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    /// The `index` the save's Gwent records refer to (`W3:gwent-card:<index>`).
    pub index: u32,
    /// A readable name derived from the XML `name` attribute (or from `title` when it is absent).
    pub name: String,
    /// The XML `title` localization key, kept so a future `.w3strings` reader can translate it.
    pub title: String,
    /// The XML `faction_index`, such as `F_NORTHERN_KINGDOM`.
    pub faction: String,
    /// True for a leader card (`def_gwint_battle_king_cards.xml` or a `TYPE_KING` flag).
    pub leader: bool,
}

/// A set of card definitions keyed by index, where a later definition replaces an earlier one.
#[derive(Clone, Debug, Default)]
pub struct CardCatalog {
    cards: BTreeMap<u32, Card>,
}

impl CardCatalog {
    /// Adds cards; a card with an index that is already there replaces it.
    pub fn insert_all(&mut self, cards: impl IntoIterator<Item = Card>) {
        for card in cards {
            self.cards.insert(card.index, card);
        }
    }

    /// How many distinct card indexes the catalog holds.
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// True when the catalog holds no cards.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// The cards in ascending index order.
    pub fn into_cards(self) -> Vec<Card> {
        self.cards.into_values().collect()
    }
}

/// Parses one card XML file. `leaders` is true for `def_gwint_battle_king_cards.xml`.
///
/// A card without an `index` is ignored. A malformed document (not well-formed XML, an attribute
/// that cannot be escaped, or more than [`MAX_CARDS`] cards) is refused; the caller decides whether
/// to skip just this file and keep reading the other bundles.
pub fn parse_cards(xml: &[u8], leaders: bool) -> Result<Vec<Card>> {
    let text = decode_xml(xml)?;
    let body = strip_declaration(&text);
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(false);
    let mut cards = Vec::new();
    let mut current: Option<Card> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| parse_error(format!("a card XML file is not well-formed: {error}")))?;
        match event {
            Event::Start(element) => {
                if element.name().as_ref() == "card" {
                    if current.is_some() {
                        return Err(parse_error("a card XML file nests a card inside another"));
                    }
                    let Some(card) = read_card(&element, leaders)? else {
                        return Err(parse_error("a card has no index attribute"));
                    };
                    current = Some(card);
                } else if element.name().as_ref() == "flag"
                    && let Some(card) = current.as_mut()
                    && flag_is_king(&element)?
                {
                    card.leader = true;
                }
            }
            Event::Empty(element) => {
                if element.name().as_ref() == "card" {
                    // A `<card id="18"/>` inside `<summonFlags>` is a reference, not a definition.
                    if let Some(card) = read_card(&element, leaders)? {
                        push_card(&mut cards, card)?;
                    }
                } else if element.name().as_ref() == "flag"
                    && let Some(card) = current.as_mut()
                    && flag_is_king(&element)?
                {
                    card.leader = true;
                }
            }
            Event::End(element) => {
                if element.name().as_ref() == "card"
                    && let Some(card) = current.take()
                {
                    push_card(&mut cards, card)?;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(cards)
}

fn push_card(cards: &mut Vec<Card>, card: Card) -> Result<()> {
    if cards.len() >= MAX_CARDS {
        return Err(Error::new(
            W3_BUNDLE,
            format!("a card XML file holds more than {MAX_CARDS} cards"),
        ));
    }
    cards.push(card);
    Ok(())
}

/// Reads the attributes of one `<card>` start tag.
fn read_card(element: &quick_xml::events::BytesStart<'_>, leaders: bool) -> Result<Option<Card>> {
    let mut index = None;
    let mut title = String::new();
    let mut name = String::new();
    let mut faction = String::new();
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| parse_error(format!("a card XML attribute is malformed: {error}")))?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| {
                parse_error(format!("a card XML attribute is not valid UTF-8: {error}"))
            })?;
        match attribute.key.as_ref() {
            "index" => {
                index = Some(value.trim().parse::<u32>().map_err(|_| {
                    parse_error(format!(
                        "a card index is not a number: {:?}",
                        value.as_ref()
                    ))
                })?);
            }
            "title" => title = value.trim().to_string(),
            "name" => name = value.trim().to_string(),
            "faction_index" => faction = value.trim().to_string(),
            _ => {}
        }
    }
    let Some(index) = index else {
        return Ok(None);
    };
    Ok(Some(Card {
        index,
        name: readable_name(&name, &title),
        title,
        faction,
        leader: leaders,
    }))
}

fn flag_is_king(element: &quick_xml::events::BytesStart<'_>) -> Result<bool> {
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute
            .map_err(|error| parse_error(format!("a card flag attribute is malformed: {error}")))?;
        if attribute.key.as_ref() == "name" {
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| parse_error(format!("a card flag is not valid UTF-8: {error}")))?;
            return Ok(value.trim().eq_ignore_ascii_case("TYPE_KING"));
        }
    }
    Ok(false)
}

/// A display name: the XML `name` when it has one, else the `title` key made readable.
fn readable_name(name: &str, title: &str) -> String {
    let source = if name.is_empty() {
        title
            .strip_prefix("gwint_name_")
            .or_else(|| title.strip_prefix("GWINT_NAME_"))
            .unwrap_or(title)
    } else {
        name
    };
    let mut out = String::with_capacity(source.len());
    for word in source.split('_').filter(|word| !word.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    if out.is_empty() {
        title.to_string()
    } else {
        out
    }
}

/// Decodes a UTF-8 or UTF-16 XML document to text.
fn decode_xml(xml: &[u8]) -> Result<String> {
    let (detected, bom_len) = encoding_rs::Encoding::for_bom(xml).unwrap_or((UTF_8, 0));
    // A BOM is the reliable marker; without one the first bytes of `<?xml` still tell UTF-16 apart.
    let encoding = if bom_len == 0 && detected == UTF_8 {
        if xml.starts_with(&[0x3C, 0x00, 0x3F, 0x00]) {
            UTF_16LE
        } else if xml.starts_with(&[0x00, 0x3C, 0x00, 0x3F]) {
            UTF_16BE
        } else {
            UTF_8
        }
    } else {
        detected
    };
    let (text, had_errors) = if encoding == UTF_16LE {
        UTF_16LE.decode_with_bom_removal(xml)
    } else if encoding == UTF_16BE {
        UTF_16BE.decode_with_bom_removal(xml)
    } else {
        UTF_8.decode_with_bom_removal(xml)
    };
    if had_errors {
        return Err(parse_error("a card XML file is not valid UTF-8 or UTF-16"));
    }
    Ok(text.into_owned())
}

/// Drops a leading XML declaration, whose `encoding="UTF-16"` no longer matches the decoded text.
fn strip_declaration(text: &str) -> &str {
    let trimmed = text.trim_start();
    if !trimmed.starts_with("<?xml") {
        return text;
    }
    match trimmed.find("?>") {
        Some(end) => trimmed.get(end.saturating_add(2)..).unwrap_or_default(),
        None => text,
    }
}
