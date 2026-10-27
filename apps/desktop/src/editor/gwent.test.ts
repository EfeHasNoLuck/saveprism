import { describe, expect, it } from "vitest";
import type { Field, GwentCard, GwentDeck, W3Card } from "@/api/commands";
import {
  buildCollection,
  buildDeck,
  collectionSummary,
  deckRows,
  factionCounts,
  factionKey,
  filterCollection,
  groupByFaction,
  tileColumns,
  type CollectionRow,
} from "./gwent";

function copies(path: string, value: string): Field {
  return {
    path,
    value,
    original: value,
    kind: "int",
    typeName: null,
    min: 0,
    max: 2147483647,
    hint: "",
    editable: true,
    readOnlyReason: null,
    added: false,
  };
}

function owned(index: number, value: string, leader = false): GwentCard {
  return { index, leader, copies: copies(`/c/${index}`, value) };
}

function card(index: number, name: string, faction: string, leader = false): W3Card {
  return { index, name, title: `t_${index}`, faction, leader };
}

const CATALOG: W3Card[] = [
  card(3, "Ash Archer", "F_NORTHERN_KINGDOM"),
  card(4, "Bitter Bard", "F_NORTHERN_KINGDOM"),
  card(5, "Copper Captain", "F_NILFGAARD"),
  card(9, "Dusk Druid", "F_NEUTRAL"),
  card(1001, "Lord of Ash", "F_NORTHERN_KINGDOM", true),
  card(2001, "Lady of Dusk", "F_NILFGAARD", true),
];

const SAVE: GwentCard[] = [owned(4, "2"), owned(3, "1"), owned(9, "3"), owned(1001, "1", true)];

const nameOf = (row: CollectionRow) => row.name ?? `Card ${row.index}`;

describe("factionKey", () => {
  it("reads the card files' faction ids and the save's selected faction", () => {
    expect(factionKey("F_NORTHERN_KINGDOM")).toBe("northern");
    expect(factionKey("GwintFaction_NothernKingdom")).toBe("northern");
    expect(factionKey("F_NO_MANS_LAND")).toBe("monsters");
    expect(factionKey("GwintFaction_NoMansLand")).toBe("monsters");
    expect(factionKey("F_SCOIATAEL")).toBe("scoiatael");
    expect(factionKey("F_NILFGAARD")).toBe("nilfgaard");
    expect(factionKey("F_SKELLIGE")).toBe("skellige");
    expect(factionKey("F_NEUTRAL")).toBe("neutral");
    expect(factionKey("F_SOMETHING_NEW")).toBe("other");
    expect(factionKey("")).toBeNull();
    expect(factionKey(null)).toBeNull();
  });
});

describe("buildCollection", () => {
  it("lists the owned cards and marks the game's other cards as missing", () => {
    const rows = buildCollection(SAVE, CATALOG);
    expect(rows.map((row) => [row.index, row.copies?.value ?? null])).toEqual([
      [1001, "1"],
      [3, "1"],
      [4, "2"],
      [2001, null],
      [5, null],
      [9, "3"],
    ]);
    expect(rows[0]).toMatchObject({ name: "Lord of Ash", faction: "northern", leader: true });
  });

  it("without the game's files lists only the owned cards, unnamed, leaders first", () => {
    const rows = buildCollection(SAVE, null);
    expect(rows.map((row) => row.index)).toEqual([1001, 3, 4, 9]);
    expect(rows.every((row) => row.name === null && row.faction === null)).toBe(true);
  });

  it("keeps an owned card the game's files do not list, under other", () => {
    const rows = buildCollection([owned(777, "1")], CATALOG);
    const unknown = rows.find((row) => row.index === 777);
    expect(unknown).toMatchObject({ name: null, faction: "other" });
    expect(rows[rows.length - 1]?.index).toBe(777);
  });
});

describe("filterCollection", () => {
  const rows = buildCollection(SAVE, CATALOG);
  const all = { faction: "all" as const, show: "all" as const, query: "" };

  it("filters by owned and missing", () => {
    expect(filterCollection(rows, { ...all, show: "owned" }, nameOf)).toHaveLength(4);
    expect(
      filterCollection(rows, { ...all, show: "missing" }, nameOf).map((row) => row.index),
    ).toEqual([2001, 5]);
  });

  it("filters by faction", () => {
    expect(
      filterCollection(rows, { ...all, faction: "nilfgaard" }, nameOf).map((row) => row.index),
    ).toEqual([2001, 5]);
  });

  it("searches the name and the index", () => {
    const search = (query: string) =>
      filterCollection(rows, { ...all, query }, nameOf).map((row) => row.index);
    expect(search("bard")).toEqual([4]);
    expect(search("#100")).toEqual([1001]);
    expect(search("  ASH ")).toEqual([1001, 3]);
    expect(search("nothing")).toEqual([]);
  });

  it("searches the fallback name without the game's files", () => {
    const bare = buildCollection(SAVE, null);
    expect(filterCollection(bare, { ...all, query: "card 9" }, nameOf)).toHaveLength(1);
  });
});

describe("counts", () => {
  it("counts owned of listed cards and leaders", () => {
    expect(collectionSummary(buildCollection(SAVE, CATALOG))).toEqual({
      cards: { owned: 3, total: 4 },
      leaders: { owned: 1, total: 2 },
    });
  });

  it("counts each faction in tab order", () => {
    expect(factionCounts(buildCollection(SAVE, CATALOG))).toEqual([
      { faction: "northern", owned: 3, total: 3 },
      { faction: "nilfgaard", owned: 0, total: 2 },
      { faction: "neutral", owned: 1, total: 1 },
    ]);
    expect(factionCounts(buildCollection(SAVE, null))).toEqual([]);
  });

  it("groups rows by faction runs", () => {
    const groups = groupByFaction(buildCollection(SAVE, CATALOG));
    expect(groups.map((group) => [group.faction, group.rows.length])).toEqual([
      ["northern", 3],
      ["nilfgaard", 2],
      ["neutral", 1],
    ]);
  });
});

describe("tileColumns", () => {
  it("fits whole tiles and never less than one", () => {
    expect(tileColumns(0)).toBe(1);
    expect(tileColumns(224)).toBe(1);
    expect(tileColumns(456)).toBe(2);
    expect(tileColumns(455)).toBe(1);
    expect(tileColumns(1200)).toBe(5);
  });
});

describe("deckRows", () => {
  const decks: GwentDeck[] = [
    { unlocked: true, leader: 2001, cardCount: 25, cards: [] },
    { unlocked: true, leader: 1001, cardCount: 29, cards: [] },
    { unlocked: false, leader: 5002, cardCount: 0, cards: [] },
  ];

  it("takes the faction from the leader and marks the selected faction's deck", () => {
    const rows = deckRows(decks, CATALOG, "GwintFaction_NothernKingdom");
    expect(rows.map((row) => [row.number, row.faction, row.selected])).toEqual([
      [1, "nilfgaard", false],
      [2, "northern", true],
      [3, null, false],
    ]);
  });

  it("without the game's files knows no factions and selects nothing", () => {
    const rows = deckRows(decks, null, "GwintFaction_NothernKingdom");
    expect(rows.every((row) => row.faction === null && !row.selected)).toBe(true);
    expect(rows[2]).toMatchObject({ number: 3, unlocked: false, cardCount: 0 });
  });
});

describe("buildDeck", () => {
  const deck: GwentDeck = { unlocked: true, leader: 1001, cardCount: 5, cards: [4, 9, 4, 5, 3] };

  it("lists the leader first, then each card once with its copies in the deck", () => {
    const rows = buildDeck(deck, SAVE, CATALOG);
    expect(rows.map((row) => [row.index, row.inDeck, row.copies?.value ?? null])).toEqual([
      [1001, 1, "1"],
      [3, 1, "1"],
      [4, 2, "2"],
      [5, 1, null],
      [9, 1, "3"],
    ]);
    expect(rows[0]).toMatchObject({ leader: true, name: "Lord of Ash", faction: "northern" });
  });

  it("keeps the owned count editable and marks a card the save does not hold as missing", () => {
    const rows = buildDeck(deck, SAVE, CATALOG);
    expect(rows.find((row) => row.index === 4)?.copies?.path).toBe("/c/4");
    const all = { faction: "all" as const, show: "missing" as const, query: "" };
    expect(filterCollection(rows, all, nameOf).map((row) => row.index)).toEqual([5]);
    expect(
      filterCollection(rows, { ...all, show: "all", query: "bard" }, nameOf).map((row) => row.index),
    ).toEqual([4]);
  });

  it("works without the game's files and without a leader", () => {
    const rows = buildDeck({ ...deck, leader: null }, SAVE, null);
    expect(rows.map((row) => row.index)).toEqual([3, 4, 5, 9]);
    expect(rows.every((row) => row.name === null && row.faction === null)).toBe(true);
  });

  it("an empty deck holds only its leader", () => {
    const rows = buildDeck({ unlocked: false, leader: 2001, cardCount: 0, cards: [] }, SAVE, CATALOG);
    expect(rows.map((row) => [row.index, row.copies])).toEqual([[2001, null]]);
  });
});
