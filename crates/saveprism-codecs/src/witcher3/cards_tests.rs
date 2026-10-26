//! Tests of the card XML parser.

use super::W3_PARSE;
use super::cards::{CardCatalog, parse_cards};

const REGULAR: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<redxml>
  <definitions>
    <gwint_card_definitions_final>
      <card index="1" title="gwint_name_alpha_test" name="alpha_test"
            faction_index="F_NEUTRAL">
        <type_flags>
          <flag name="TYPE_CREATURE" />
        </type_flags>
      </card>
      <card index="2" title="gwint_name_beta_test" faction_index="F_NILFGAARD">
        <type_flags>
          <flag name="TYPE_KING" />
        </type_flags>
      </card>
      <card index="3" name="gamma_test" faction_index="F_SCOIATAEL" />
    </gwint_card_definitions_final>
  </definitions>
</redxml>
"#;

#[test]
fn parses_names_titles_factions_and_leaders() {
    let cards = parse_cards(REGULAR.as_bytes(), false).expect("the invented XML parses");
    assert_eq!(cards.len(), 3);
    let first = cards.first().expect("a first card");
    assert_eq!(first.index, 1);
    assert_eq!(first.name, "Alpha Test");
    assert_eq!(first.title, "gwint_name_alpha_test");
    assert_eq!(first.faction, "F_NEUTRAL");
    assert!(!first.leader);

    let second = cards.get(1).expect("a second card");
    assert_eq!(second.name, "Beta Test");
    assert!(second.leader, "a TYPE_KING flag makes the card a leader");

    let third = cards.get(2).expect("a third card");
    assert_eq!(third.name, "Gamma Test");
    assert!(third.title.is_empty());
}

#[test]
fn a_title_is_made_readable_when_the_name_attribute_is_missing() {
    let cards = parse_cards(REGULAR.as_bytes(), false).expect("the invented XML parses");
    let second = cards.get(1).expect("a second card");
    assert_eq!(second.name, "Beta Test");
    assert_eq!(second.title, "gwint_name_beta_test");
}

#[test]
fn utf16le_with_a_bom_is_decoded() {
    let text = REGULAR.replace("UTF-8", "UTF-16");
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let cards = parse_cards(&bytes, false).expect("UTF-16 XML parses");
    assert_eq!(cards.len(), 3);
    assert_eq!(cards.first().expect("a first card").name, "Alpha Test");
}

#[test]
fn the_leader_file_marks_every_card_as_a_leader() {
    let cards = parse_cards(REGULAR.as_bytes(), true).expect("the invented XML parses");
    assert!(cards.iter().all(|card| card.leader));
}

#[test]
fn a_card_without_an_index_is_refused() {
    let xml = r#"<redxml><card title="gwint_name_no_index"></card></redxml>"#;
    let error = parse_cards(xml.as_bytes(), false).expect_err("a card needs an index");
    assert_eq!(error.code(), W3_PARSE);
    assert!(error.message().contains("index"));
}

#[test]
fn a_summon_reference_inside_summon_flags_is_not_a_definition() {
    let xml = br#"<redxml>
        <card index="18" name="mrmirror" faction_index="F_NEUTRAL">
          <summonFlags><card id="19"/><card id="20"/></summonFlags>
        </card>
    </redxml>"#;
    let cards = parse_cards(xml, false).expect("summon references are skipped");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards.first().expect("a card").index, 18);
}

#[test]
fn a_malformed_document_is_refused() {
    let xml = b"<redxml><card index=\"1\"><type_flags></redxml>";
    let error = parse_cards(xml, false).expect_err("an unclosed element is refused");
    assert_eq!(error.code(), W3_PARSE);
}

#[test]
fn a_catalog_lets_a_later_file_replace_a_card_by_index() {
    let base = parse_cards(REGULAR.as_bytes(), false).expect("the invented XML parses");
    let patch = parse_cards(
        br#"<redxml><card index="2" name="beta_patched" faction_index="F_NILFGAARD" /></redxml>"#,
        false,
    )
    .expect("the invented patch parses");
    let mut catalog = CardCatalog::default();
    catalog.insert_all(base);
    assert_eq!(catalog.len(), 3);
    catalog.insert_all(patch);
    assert_eq!(catalog.len(), 3, "the same index replaces rather than adds");
    let cards = catalog.into_cards();
    let patched = cards.iter().find(|card| card.index == 2).expect("card 2");
    assert_eq!(patched.name, "Beta Patched");
}

#[test]
fn an_attribute_cannot_reach_the_tree_as_markup() {
    let xml =
        br#"<redxml><card index="1" name="&lt;script&gt;" faction_index="F_NEUTRAL" /></redxml>"#;
    let cards = parse_cards(xml, false).expect("an escaped attribute parses");
    let card = cards.first().expect("a card");
    assert_eq!(card.name, "<script>");
}
