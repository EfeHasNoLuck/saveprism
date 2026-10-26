// Pure helpers of the Witcher 3 Gwent view (no React, no Tauri) so they can be tested alone.

import type { Field, GwentCard, GwentDeck, W3Card } from "@/api/commands";

/**
 * The Gwent layout on show: "a" card tiles under faction tabs, "b" a dense list grouped by faction
 * beside a deck summary. One of them is removed once the maintainer has picked.
 */
export const GWENT_LAYOUT: "a" | "b" = "a";

/** The factions the view groups by; `other` holds a faction the view does not know. */
export type FactionKey =
  | "northern"
  | "nilfgaard"
  | "scoiatael"
  | "monsters"
  | "skellige"
  | "neutral"
  | "other";

/** The order of the faction tabs and groups. */
export const FACTION_ORDER: readonly FactionKey[] = [
  "northern",
  "nilfgaard",
  "scoiatael",
  "monsters",
  "skellige",
  "neutral",
  "other",
];

/**
 * The faction of a card definition's `faction_index` (`F_NORTHERN_KINGDOM`) or of a save's
 * selected deck (`GwintFaction_NothernKingdom`, spelled that way by the game); null for no text.
 */
export function factionKey(id: string | null | undefined): FactionKey | null {
  if (id === null || id === undefined || id.trim() === "") return null;
  const text = id.toLowerCase().replace(/[^a-z]/g, "");
  if (text.includes("neutral")) return "neutral";
  if (text.includes("nomansland") || text.includes("monster")) return "monsters";
  if (text.includes("nilfgaard")) return "nilfgaard";
  if (text.includes("scoia")) return "scoiatael";
  if (text.includes("skellige")) return "skellige";
  if (text.includes("north") || text.includes("nothern")) return "northern";
  return "other";
}

/** One card of the view: owned (it has a copy count) or missing (only in the game's list). */
export interface CollectionRow {
  index: number;
  /** The name from the game's files, or null without them. */
  name: string | null;
  /** The faction from the game's files, or null without them. */
  faction: FactionKey | null;
  leader: boolean;
  /** The copy count when the save has the card, else null (missing). */
  copies: Field | null;
}

function factionRank(faction: FactionKey | null): number {
  return faction === null ? FACTION_ORDER.length : FACTION_ORDER.indexOf(faction);
}

/** Faction order, then leaders first, then the card index. */
export function compareRows(a: CollectionRow, b: CollectionRow): number {
  return (
    factionRank(a.faction) - factionRank(b.faction) ||
    Number(b.leader) - Number(a.leader) ||
    a.index - b.index
  );
}

/**
 * The owned cards of the save with their names, plus every card of the game's list the save does
 * not hold (missing). Without the game's list (`catalog` null) only the owned cards are listed.
 */
export function buildCollection(
  owned: readonly GwentCard[],
  catalog: readonly W3Card[] | null,
): CollectionRow[] {
  const known = new Map<number, W3Card>();
  for (const card of catalog ?? []) known.set(card.index, card);
  const ownedIndexes = new Set<number>();
  const rows: CollectionRow[] = owned.map((card) => {
    ownedIndexes.add(card.index);
    const definition = known.get(card.index);
    return {
      index: card.index,
      name: definition?.name ?? null,
      faction:
        definition !== undefined ? factionKey(definition.faction) : catalog === null ? null : "other",
      leader: card.leader || definition?.leader === true,
      copies: card.copies,
    };
  });
  for (const definition of catalog ?? []) {
    if (ownedIndexes.has(definition.index)) continue;
    rows.push({
      index: definition.index,
      name: definition.name,
      faction: factionKey(definition.faction),
      leader: definition.leader,
      copies: null,
    });
  }
  return rows.sort(compareRows);
}

export type ShowFilter = "all" | "owned" | "missing";

export interface CollectionFilter {
  /** One faction, or every faction. */
  faction: FactionKey | "all";
  show: ShowFilter;
  query: string;
}

/**
 * The rows of one faction that match the filter. The query matches the shown name (`nameOf`) or
 * the index ("108" or "#108"), case-insensitive.
 */
export function filterCollection(
  rows: readonly CollectionRow[],
  filter: CollectionFilter,
  nameOf: (row: CollectionRow) => string,
): CollectionRow[] {
  const query = filter.query.trim().toLowerCase();
  const number = query.replace(/^#/, "");
  return rows.filter((row) => {
    if (filter.faction !== "all" && row.faction !== filter.faction) return false;
    if (filter.show === "owned" && row.copies === null) return false;
    if (filter.show === "missing" && row.copies !== null) return false;
    if (query === "") return true;
    return (
      nameOf(row).toLowerCase().includes(query) ||
      (number !== "" && String(row.index).includes(number))
    );
  });
}

export interface OwnedCount {
  owned: number;
  total: number;
}

/** Owned and listed cards, and the same for the leaders. */
export function collectionSummary(rows: readonly CollectionRow[]): {
  cards: OwnedCount;
  leaders: OwnedCount;
} {
  const cards = { owned: 0, total: 0 };
  const leaders = { owned: 0, total: 0 };
  for (const row of rows) {
    const count = row.leader ? leaders : cards;
    count.total += 1;
    if (row.copies !== null) count.owned += 1;
  }
  return { cards, leaders };
}

/** The factions that have cards, in tab order, with their owned and listed counts. */
export function factionCounts(
  rows: readonly CollectionRow[],
): { faction: FactionKey; owned: number; total: number }[] {
  const counts = new Map<FactionKey, OwnedCount>();
  for (const row of rows) {
    if (row.faction === null) continue;
    const count = counts.get(row.faction) ?? { owned: 0, total: 0 };
    count.total += 1;
    if (row.copies !== null) count.owned += 1;
    counts.set(row.faction, count);
  }
  return FACTION_ORDER.flatMap((faction) => {
    const count = counts.get(faction);
    return count === undefined ? [] : [{ faction, ...count }];
  });
}

/** Rows split into runs of one faction, in their order (for the grouped list). */
export function groupByFaction(
  rows: readonly CollectionRow[],
): { faction: FactionKey | null; rows: CollectionRow[] }[] {
  const groups: { faction: FactionKey | null; rows: CollectionRow[] }[] = [];
  for (const row of rows) {
    const last = groups[groups.length - 1];
    if (last !== undefined && last.faction === row.faction) last.rows.push(row);
    else groups.push({ faction: row.faction, rows: [row] });
  }
  return groups;
}

/** Narrowest card tile, in pixels at the default 16px text size. */
export const MIN_TILE = 224;
/** Space between the tiles, in pixels (Tailwind `gap-2`). */
export const TILE_GAP = 8;
/** Rows of tiles on one page. */
export const TILE_ROWS = 6;

/** Tile columns that fit in a box `width` pixels wide (at least one). */
export function tileColumns(width: number): number {
  return Math.max(1, Math.floor((width + TILE_GAP) / (MIN_TILE + TILE_GAP)));
}

/** One deck as the deck card shows it. */
export interface DeckRow {
  /** 1-based position in the save. */
  number: number;
  unlocked: boolean;
  cardCount: number;
  leader: number | null;
  /** The leader's name from the game's files, or null. */
  leaderName: string | null;
  /** The deck's faction (its leader's), or null without the game's files. */
  faction: FactionKey | null;
  /** The deck of the selected faction. */
  selected: boolean;
}

/** The decks with their leader names and factions, and which one is selected. */
export function deckRows(
  decks: readonly GwentDeck[],
  catalog: readonly W3Card[] | null,
  selected: string | null,
): DeckRow[] {
  const known = new Map<number, W3Card>();
  for (const card of catalog ?? []) known.set(card.index, card);
  const selectedFaction = factionKey(selected);
  return decks.map((deck, position) => {
    const leader = deck.leader === null ? undefined : known.get(deck.leader);
    const faction = leader === undefined ? null : factionKey(leader.faction);
    return {
      number: position + 1,
      unlocked: deck.unlocked,
      cardCount: deck.cardCount,
      leader: deck.leader,
      leaderName: leader?.name ?? null,
      faction,
      selected: faction !== null && faction === selectedFaction,
    };
  });
}
