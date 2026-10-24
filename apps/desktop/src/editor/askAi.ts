// Builds the note "Ask AI" copies (no React, no Tauri) so it can be tested alone.

import type { FieldKindLike, Translate } from "./helpers";

/** Most values the note lists; the rest are counted in its heading. */
export const MAX_ASK_AI_VALUES = 300;

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
  /** How many values the card shows in all (may be more than `rows`). */
  total: number;
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
    total > MAX_ASK_AI_VALUES
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
