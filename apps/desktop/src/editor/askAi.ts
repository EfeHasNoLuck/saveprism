// Builds the note "Ask AI" copies (no React, no Tauri) so it can be tested alone.

import type { FieldKindLike, Translate } from "./helpers";

/** Most values the note lists; the rest are counted in its heading. */
export const MAX_ASK_AI_VALUES = 300;
/** Most children read while collecting the values inside a card's groups. */
export const MAX_ASK_AI_VISITS = 2000;
/** Deepest group level opened while collecting. */
export const MAX_ASK_AI_DEPTH = 16;

/** One value as the note writes it. */
export interface AskAiRow {
  /** The name the card shows, or null when it has none. */
  name: string | null;
  /** The number after the name ("#7"), when the card shows one. */
  id?: number;
  path: string;
  value: string;
  kind: FieldKindLike;
  min: number | null;
  max: number | null;
}

export interface AskAiInput {
  t: Translate;
  fileName: string;
  /** The format as the header names it ("RPG Maker MV"), or null when unknown. */
  format: string | null;
  /** The card's title ("Items", "Character #1 Harold"). */
  list: string;
  query: string;
  /** The values the card shows, on every page. */
  rows: readonly AskAiRow[];
  /** How many values the card shows in all (may be more than `rows`); null when not known. */
  total: number | null;
}

/** "1234" as "1,234". */
function groupDigits(value: number): string {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** A limit worth telling: not missing and not the edge of what any number can hold (plain JSON). */
function telling(limit: number | null): string | null {
  return limit === null || Math.abs(limit) >= Number.MAX_SAFE_INTEGER ? null : groupDigits(limit);
}

/** The limits of a value, or "" when it has none worth telling. */
function limitText(row: AskAiRow, t: Translate): string {
  const min = telling(row.min);
  const max = telling(row.max);
  if (min !== null && max !== null) return t("range.between", { min, max });
  if (min !== null) return t("range.atLeast", { min });
  if (max !== null) return t("range.atMost", { max });
  return "";
}

/**
 * The type and limits in parentheses. A number without limits needs no type: its value says what
 * it is. On/off and text always say their type.
 */
function detailText(row: AskAiRow, t: Translate): string {
  const range = limitText(row, t);
  const kind = t(`type.${row.kind}`);
  if (range === "") {
    return row.kind === "bool" || row.kind === "string" ? ` (${kind})` : "";
  }
  return ` (${kind}, ${range})`;
}

/** "Potion #1", "#7", or "this value" when the card shows neither a name nor a number. */
function subjectText(row: AskAiRow, t: Translate): string {
  if (row.name !== null) {
    return row.id === undefined
      ? row.name
      : t("askAi.subjectName", { name: row.name, id: row.id });
  }
  if (row.id !== undefined) return t("askAi.subjectId", { id: row.id });
  return t("askAi.subjectValue");
}

/**
 * The text "Ask AI" copies: the file, the card and its search, then at most `MAX_ASK_AI_VALUES`
 * values with their type, limits and path, then a short request. Every sentence comes from `t`;
 * names, values and paths are written as they are.
 */
export function buildAskAiText(input: AskAiInput): string {
  const { t, fileName, format, list, query, rows, total } = input;
  const copied = rows.slice(0, MAX_ASK_AI_VALUES);
  const q = query.trim();

  const lines = [t("askAi.intro")];
  lines.push(
    format === null || format.trim() === ""
      ? t("askAi.filePlain", { name: fileName })
      : t("askAi.file", { name: fileName, format }),
  );
  lines.push(q === "" ? t("askAi.list", { list }) : t("askAi.listSearch", { list, query: q }));
  lines.push(
    total === null
      ? t("askAi.valuesSome", { shown: groupDigits(copied.length) })
      : total > MAX_ASK_AI_VALUES
        ? t("askAi.valuesFirst", {
            shown: groupDigits(MAX_ASK_AI_VALUES),
            total: groupDigits(total),
          })
        : t("askAi.values", { count: groupDigits(total) }),
  );
  for (const row of copied) {
    lines.push(
      t("askAi.row", {
        subject: subjectText(row, t),
        value: row.value,
        detail: detailText(row, t),
        path: row.path,
      }),
    );
  }
  lines.push(t("askAi.ask"));
  return lines.join("\n");
}

/** How many values the note holds for `rows` (the toast counts these). */
export function askAiValueCount(rows: readonly AskAiRow[]): number {
  return Math.min(rows.length, MAX_ASK_AI_VALUES);
}

/** A child of a group as the host lists it (only what collecting needs). */
export interface AskAiChild {
  key: string;
  path: string;
  /** `object` and `array` are groups; `string`, `int`, `float` and `bool` are values. */
  kind: string;
}

/** Where the values inside groups come from (the host, or a test). */
export interface AskAiSource {
  /** One page of the children of the group at `path`, or null when the host refused. */
  children: (
    path: string,
    offset: number,
  ) => Promise<{ items: readonly AskAiChild[]; total: number } | null>;
  /** The value at `path`, or null when it has none. */
  field: (path: string) => Promise<Omit<AskAiRow, "name" | "id"> | null>;
}

const SCALARS = new Set(["string", "int", "float", "bool"]);

/**
 * The values "Ask AI" copies from a card that has groups: `rows` (the card's own values) first,
 * then each group's values in the order the card shows them (its values, then its groups), keeping
 * those whose name or value holds the search. It stops once it has one value more than the note
 * lists, after `MAX_ASK_AI_VISITS` children or below `MAX_ASK_AI_DEPTH` levels; `total` is then
 * null, since the file holds more than were read.
 */
export async function collectAskAiRows(
  rows: readonly AskAiRow[],
  groups: readonly string[],
  query: string,
  source: AskAiSource,
): Promise<{ rows: AskAiRow[]; total: number | null }> {
  const q = query.trim().toLowerCase();
  const out: AskAiRow[] = [...rows];
  let visits = 0;
  let stopped = false;
  const full = () => out.length > MAX_ASK_AI_VALUES;

  async function walk(path: string, depth: number): Promise<void> {
    if (depth > MAX_ASK_AI_DEPTH) {
      stopped = true;
      return;
    }
    const inner: string[] = [];
    let offset = 0;
    for (;;) {
      const page = await source.children(path, offset);
      if (page === null) return;
      for (const child of page.items) {
        if (full() || visits >= MAX_ASK_AI_VISITS) {
          stopped = true;
          return;
        }
        visits += 1;
        if (child.kind === "object" || child.kind === "array") {
          inner.push(child.path);
          continue;
        }
        if (!SCALARS.has(child.kind)) continue;
        const field = await source.field(child.path);
        if (field === null) continue;
        if (q === "" || child.key.toLowerCase().includes(q) || field.value.toLowerCase().includes(q)) {
          out.push({ name: child.key, ...field });
        }
      }
      offset += page.items.length;
      if (page.items.length === 0 || offset >= page.total) break;
    }
    for (const next of inner) {
      if (stopped) return;
      await walk(next, depth + 1);
    }
  }

  for (const group of groups) {
    if (stopped || full()) break;
    await walk(group, 1);
  }
  return { rows: out, total: stopped || full() ? null : out.length };
}
