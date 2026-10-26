//! The Gwent collection of a Witcher 3 save for the game view.
//!
//! The codec marks the Gwent records with type hints (see `saveprism_codecs::witcher3`):
//! `W3:gwent-manager` on the manager, `W3:gwent-collection` on the two collection blocks,
//! `W3:gwent-card:<index>` on every card record, `W3:gwent-decks` / `W3:gwent-deck` on the decks
//! and `W3:gwent-selected-deck` on the selected faction. This module walks those hints (never the
//! record names, which repeat) and returns the owned cards with their copy counts, the decks and
//! the selected faction. Only the copy counts are editable; card names come from the player's own
//! game files through `w3_cards`.

use saveprism_core::{NodeId, Value};
use serde::Serialize;

use super::game::{At, Ctx, Field, NON_NEGATIVE};

/// Most card records the view lists (the game has a few hundred).
const MAX_CARDS: usize = 5_000;
/// Most decks the view lists (the game has five).
const MAX_DECKS: usize = 64;
const HINT_MANAGER: &str = "W3:gwent-manager";
const HINT_COLLECTION: &str = "W3:gwent-collection";
const HINT_CARD: &str = "W3:gwent-card:";
const HINT_DECKS: &str = "W3:gwent-decks";
const HINT_DECK: &str = "W3:gwent-deck";
const HINT_SELECTED: &str = "W3:gwent-selected-deck";

/// One card of the player's collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GwentCard {
    /// The card index the game's card definitions use.
    pub index: u32,
    /// True for a leader (battle king) card.
    pub leader: bool,
    /// The number of copies owned (`numCopies`), the only editable Gwent value.
    pub copies: Field,
}

/// One deck (read-only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GwentDeck {
    /// The game lets the player use this deck.
    pub unlocked: bool,
    /// The card index of the deck's leader, when the save has one.
    pub leader: Option<u32>,
    /// The number of cards in the deck.
    pub card_count: u32,
}

/// The Gwent state of a Witcher 3 save.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GwentView {
    /// The owned cards: the collection first, then the leaders, each in save order.
    pub cards: Vec<GwentCard>,
    /// The decks in save order.
    pub decks: Vec<GwentDeck>,
    /// The selected faction as the save names it (`GwintFaction_...`), when present.
    pub selected: Option<String>,
}

impl Ctx<'_> {
    fn hint(&self, id: NodeId) -> Option<&str> {
        self.doc
            .get(id)?
            .type_hint
            .as_ref()
            .map(|hint| hint.as_str())
    }

    /// The object entries at `at` with their paths.
    fn children(&self, at: &At) -> Vec<(String, At)> {
        match self.doc.get(at.0).map(|node| &node.value) {
            Some(Value::Object(entries)) => entries
                .iter()
                .map(|(key, child)| {
                    let mut path = at.1.clone();
                    path.push(key.as_str());
                    (key.clone(), (*child, path))
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The child of `at` whose key is `key` (a repeated key's first entry).
    fn child_named(&self, at: &At, key: &str) -> Option<At> {
        self.children(at)
            .into_iter()
            .find(|(name, _)| name == key)
            .map(|(_, child)| child)
    }

    /// The card records of one collection block.
    fn gwent_cards(&self, block: &At, leader: bool, out: &mut Vec<GwentCard>) {
        for (_, card) in self.children(block) {
            if out.len() >= MAX_CARDS {
                return;
            }
            let Some(index) = self
                .hint(card.0)
                .and_then(|hint| hint.strip_prefix(HINT_CARD))
                .and_then(|text| text.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(copies) = self
                .child_named(&card, "numCopies")
                .and_then(|at| self.field(&at, NON_NEGATIVE))
            else {
                continue;
            };
            out.push(GwentCard {
                index,
                leader,
                copies,
            });
        }
    }

    fn gwent_deck(&self, deck: &At) -> GwentDeck {
        let unlocked = self
            .child_named(deck, "DeckUnlocked")
            .and_then(|at| match self.doc.get(at.0).map(|node| &node.value) {
                Some(Value::Bool(value)) => Some(*value),
                Some(Value::Int { v, .. }) => Some(*v != 0),
                _ => None,
            })
            .unwrap_or(false);
        let leader = self
            .child_named(deck, "LeaderIndex")
            .and_then(|at| self.int_at(&at))
            .and_then(|value| u32::try_from(value).ok());
        let card_count = self
            .child_named(deck, "CardCount")
            .and_then(|at| self.int_at(&at))
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);
        GwentDeck {
            unlocked,
            leader,
            card_count,
        }
    }

    /// The Gwent collection, or `None` when the save has no Gwent manager.
    pub(super) fn gwent_view(&self) -> Option<GwentView> {
        let root: At = (self.doc.root(), saveprism_core::Path::root());
        let variables = self.child_named(&root, "Variables")?;
        let (_, manager) = self
            .children(&variables)
            .into_iter()
            .find(|(_, at)| self.hint(at.0) == Some(HINT_MANAGER))?;
        let mut cards = Vec::new();
        let mut leaders = Vec::new();
        let mut decks = Vec::new();
        let mut selected = None;
        for (key, at) in self.children(&manager) {
            match self.hint(at.0) {
                Some(HINT_COLLECTION) if key.contains("Leader") => {
                    self.gwent_cards(&at, true, &mut leaders);
                }
                Some(HINT_COLLECTION) => self.gwent_cards(&at, false, &mut cards),
                Some(HINT_DECKS) => {
                    for (_, deck) in self.children(&at) {
                        if decks.len() < MAX_DECKS && self.hint(deck.0) == Some(HINT_DECK) {
                            decks.push(self.gwent_deck(&deck));
                        }
                    }
                }
                Some(HINT_SELECTED) => {
                    selected = self
                        .child_named(&at, "deckIndex")
                        .and_then(|at| self.str_at(&at).map(str::to_string));
                }
                _ => {}
            }
        }
        cards.append(&mut leaders);
        cards.truncate(MAX_CARDS);
        Some(GwentView {
            cards,
            decks,
            selected,
        })
    }
}
