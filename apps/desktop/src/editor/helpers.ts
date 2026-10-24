// Pure helpers of the editor views (no React, no Tauri) so they can be tested alone.

export type FieldKindLike = "int" | "float" | "bool" | "string";

export type RangeProblem = "empty" | "notNumber" | "notWhole" | "belowMin" | "aboveMax";

const INT_RE = /^[+-]?\d+$/;
const FLOAT_RE = /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i;

/** Checks typed text against the kind and the limits. `null` means the text is acceptable. */
export function checkRange(
  kind: FieldKindLike,
  text: string,
  min: number | null,
  max: number | null,
): RangeProblem | null {
  if (kind === "bool" || kind === "string") return null;
  const t = text.trim();
  if (t === "") return "empty";
  if (kind === "int") {
    if (!INT_RE.test(t)) return FLOAT_RE.test(t) ? "notWhole" : "notNumber";
  } else if (!FLOAT_RE.test(t)) {
    return "notNumber";
  }
  const n = Number(t);
  if (!Number.isFinite(n)) return "notNumber";
  if (min !== null && n < min) return "belowMin";
  if (max !== null && n > max) return "aboveMax";
  return null;
}

/** Result of a stepper click as text, clamped to the limits. `null` when `current` is not a number. */
export function stepValue(
  current: string,
  delta: number,
  min: number | null,
  max: number | null,
): string | null {
  const t = current.trim();
  if (!FLOAT_RE.test(t)) return null;
  let n = Number(t) + delta;
  if (!Number.isFinite(n)) return null;
  if (min !== null && n < min) n = min;
  if (max !== null && n > max) n = max;
  return String(n);
}

/** `t` of react-i18next, narrowed to what the helpers need (a fake works in tests). */
export type Translate = (key: string, options?: Record<string, unknown>) => string;

/** Formats a number in the UI language ("1,234" in English, "1.234" in Turkish). */
export type FormatNumber = (value: number) => string;

/**
 * The limits worth showing. The host clamps a limit past +-(2^53-1) (a 64-bit type) to that
 * value; such a limit reads as "no limit" on its side.
 */
export function shownLimits(
  min: number | null,
  max: number | null,
): { min: number | null; max: number | null } {
  return {
    min: min !== null && min > -Number.MAX_SAFE_INTEGER ? min : null,
    max: max !== null && max < Number.MAX_SAFE_INTEGER ? max : null,
  };
}

/** The type and limits of a field in the UI language: "Whole number · 0 to 99,999,999". */
export function fieldHint(
  kind: FieldKindLike,
  min: number | null,
  max: number | null,
  t: Translate,
  format: FormatNumber,
): string {
  const type = t(`type.${kind}`);
  if (kind !== "int" && kind !== "float") return type;
  const limits = shownLimits(min, max);
  let range = "";
  if (limits.min !== null && limits.max !== null) {
    range = t("range.between", { min: format(limits.min), max: format(limits.max) });
  } else if (limits.min !== null) {
    range = t("range.atLeast", { min: format(limits.min) });
  } else if (limits.max !== null) {
    range = t("range.atMost", { max: format(limits.max) });
  }
  return range === "" ? type : `${type} · ${range}`;
}

/** A whole number as text in the UI language's grouping; any other text stays as it is. */
export function formatValue(text: string, format: FormatNumber): string {
  if (!INT_RE.test(text.trim())) return text;
  const n = Number(text);
  return Number.isSafeInteger(n) ? format(n) : text;
}

/** Longest text value in an "old → new" line, in characters. */
export const CHANGE_TEXT_CHARS = 24;

/**
 * A value as an "old → new" line shows it: whole numbers grouped in the UI language, text quoted
 * and cut to `maxChars`, anything else (true/false, decimals) as it is.
 */
export function changeValue(
  kind: FieldKindLike,
  text: string,
  format: FormatNumber,
  maxChars = CHANGE_TEXT_CHARS,
): string {
  if (kind !== "string") return formatValue(text, format);
  const chars = Array.from(text);
  const short = chars.length > maxChars ? `${chars.slice(0, maxChars - 1).join("")}…` : text;
  return `"${short}"`;
}

/** "2,500 → 2,601", "false → true", "\"Ann\" → \"Ada\"". */
export function changeText(
  kind: FieldKindLike,
  original: string,
  value: string,
  format: FormatNumber,
  maxChars = CHANGE_TEXT_CHARS,
): string {
  return `${changeValue(kind, original, format, maxChars)} → ${changeValue(kind, value, format, maxChars)}`;
}

export interface HintInput {
  hint: string;
  focused: boolean;
  /** Shown even when not focused or changed (Gold). */
  always: boolean;
  /** Out of range, not a number, or rejected by the host. */
  invalid: boolean;
  /** "old → new" of a changed value, or `null`. */
  change: string | null;
}

export interface HintLine {
  text: string;
  error: boolean;
  /** The line is the "old → new" of a changed value. */
  change: boolean;
}

/**
 * The line under a field, or `null` when none should show. An error wins; a focused field shows
 * its type and limits; a changed field shows "old → new"; Gold shows its type and limits always.
 */
export function hintLine(i: HintInput): HintLine | null {
  if (i.invalid) return i.hint ? { text: i.hint, error: true, change: false } : null;
  if (i.focused && i.hint) return { text: i.hint, error: false, change: false };
  if (i.change) return { text: i.change, error: false, change: true };
  if (i.always && i.hint) return { text: i.hint, error: false, change: false };
  return null;
}

export interface Searchable {
  id: number;
  name: string | null;
  field: { value: string };
}

/**
 * Keeps entries whose name, `#id`, value or own name (from `ownName`) contains the query
 * (case-insensitive).
 */
export function filterEntries<T extends Searchable>(
  entries: readonly T[],
  query: string,
  ownName?: (entry: T) => string | null,
): T[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...entries];
  return entries.filter(
    (e) =>
      (e.name !== null && e.name.toLowerCase().includes(q)) ||
      (ownName?.(e) ?? "").toLowerCase().includes(q) ||
      `#${e.id}`.includes(q) ||
      e.field.value.toLowerCase().includes(q),
  );
}

/** What a list's "Set all" writes: true or false (switches) or one number (the other lists). */
export type BulkKind = "bool" | "number";

export interface BulkField {
  path: string;
  value: string;
  kind: FieldKindLike;
  editable: boolean;
}

/**
 * The values a list's "Set all" may change: the editable ones of the fitting kind. A variable that
 * holds text is left out of a number.
 */
export function bulkTargets<T extends BulkField>(fields: readonly T[], kind: BulkKind): T[] {
  return fields.filter(
    (f) =>
      f.editable && (kind === "bool" ? f.kind === "bool" : f.kind === "int" || f.kind === "float"),
  );
}

/** True when `value` already is `text`: the same number ("42" and "42.0") or the same true/false. */
export function sameValue(value: string, text: string): boolean {
  const a = value.trim();
  const b = text.trim();
  if (FLOAT_RE.test(a) && FLOAT_RE.test(b)) return Number(a) === Number(b);
  return a.toLowerCase() === b.toLowerCase();
}

/** Paths of the `targets` that setting them all to `text` would change. */
export function bulkPaths(targets: readonly BulkField[], text: string): string[] {
  return targets.filter((f) => !sameValue(f.value, text)).map((f) => f.path);
}

export function pageCount(total: number, size: number): number {
  if (total <= 0 || size <= 0) return 1;
  return Math.ceil(total / size);
}

export function clampPage(page: number, total: number, size: number): number {
  return Math.min(Math.max(0, page), pageCount(total, size) - 1);
}

export function pageItems<T>(items: readonly T[], page: number, size: number): T[] {
  const p = clampPage(page, items.length, size);
  return items.slice(p * size, p * size + size);
}

/** "1 / 3" */
export function pagerLabel(page: number, total: number, size: number): string {
  return `${clampPage(page, total, size) + 1} / ${pageCount(total, size)}`;
}

/**
 * Narrowest column of the entry grids, in pixels. These sizes are pixels at the default 16px text
 * size; callers scale them with the Text size setting (`rootScale` in `grid.ts`).
 */
export const MIN_COLUMN = 232;
/** Space between the columns, in pixels (Tailwind `gap-x-3`). */
export const COLUMN_GAP = 12;
/** Rows on one page of a list; a page holds this many entries per column. */
export const ROWS_PER_PAGE = 20;
/** Padding and border of a card, left and right together, in pixels (`p-3` and a 1 px border). */
export const CARD_EDGES = 26;
/** Space between the cards of a row, in pixels (Tailwind `gap-2.5`). */
export const CARD_GAP = 10;
/** Width of a card with nothing to list, in pixels. */
export const EMPTY_CARD = 192;

/** Entry columns that fit in a box `width` pixels wide (at least one). */
export function columnsIn(width: number): number {
  return Math.max(1, Math.floor((width + COLUMN_GAP) / (MIN_COLUMN + COLUMN_GAP)));
}

/**
 * Columns for each list card of a row `width` pixels wide, given the number of entries of each
 * card. Every card with entries gets one column; each further column that fits goes to the card
 * that is tallest at that moment, so the cards end at similar heights. An empty card gets 0.
 */
export function allocateColumns(width: number, counts: readonly number[]): number[] {
  const columns: number[] = counts.map((n) => (n > 0 ? 1 : 0));
  const filled = columns.filter((c) => c > 0).length;
  if (filled === 0) return columns;
  const empty = counts.length - filled;
  const room =
    width - (counts.length - 1) * CARD_GAP - empty * EMPTY_CARD - filled * CARD_EDGES;
  // c columns take c * MIN_COLUMN + (c - 1) * COLUMN_GAP.
  let spare = Math.floor((room + filled * COLUMN_GAP) / (MIN_COLUMN + COLUMN_GAP)) - filled;
  while (spare > 0) {
    let tallest = -1;
    let rows = 1;
    counts.forEach((n, i) => {
      const c = columns[i] ?? 0;
      if (c === 0 || n <= c) return;
      const r = Math.ceil(n / c);
      if (r > rows) {
        rows = r;
        tallest = i;
      }
    });
    if (tallest < 0) break;
    columns[tallest] = (columns[tallest] ?? 0) + 1;
    spare -= 1;
  }
  return columns;
}

/** Width a card needs for `columns` entry columns, in pixels. */
export function cardWidth(columns: number): number {
  if (columns <= 0) return EMPTY_CARD;
  return columns * MIN_COLUMN + (columns - 1) * COLUMN_GAP + CARD_EDGES;
}

/** What a read-only row of the story view holds (mirrors `RowNote` of the host). */
export interface NoteLike {
  kind: "changed" | "map" | "set" | "date" | "code" | "empty";
  count: number | null;
  text: string | null;
}

/** A date and time in the UI language ("Oct 9, 2025, 12:00 PM"); `null` for an invalid time. */
export function formatDateTime(ms: number, language: string): string | null {
  if (!Number.isFinite(ms)) return null;
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return null;
  return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/**
 * The short text a read-only story value shows instead of an editor ("Map · 3 entries", "changed
 * during play"), and the longer reason for its tooltip.
 */
export function noteText(
  note: NoteLike,
  t: Translate,
  format: FormatNumber,
  language: string,
): { text: string; tip: string } {
  const count = note.count ?? 0;
  switch (note.kind) {
    case "map":
      return { text: t("note.map", { count, shown: format(count) }), tip: t("note.specialTip") };
    case "set":
      return { text: t("note.set", { count, shown: format(count) }), tip: t("note.specialTip") };
    case "date": {
      const raw = note.text ?? "";
      const shown = formatDateTime(Date.parse(raw), language) ?? raw;
      return { text: t("note.date", { date: shown }), tip: t("note.specialTip") };
    }
    case "code":
      return { text: t("note.code"), tip: t("note.specialTip") };
    case "empty":
      return { text: t("note.empty"), tip: t("note.emptyTip") };
    default:
      return { text: t("note.changed"), tip: t("note.changedTip") };
  }
}

/** A game database entry for the "Add" picker (mirrors `NamedId` of the host). */
export interface CatalogEntryLike {
  id: number;
  name: string;
}

/**
 * Keeps the database entries whose name or `#id` contains the query (case-insensitive). A query
 * that is only a number also matches that id exactly.
 */
export function filterCatalog<T extends CatalogEntryLike>(
  entries: readonly T[],
  query: string,
): T[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...entries];
  const id = catalogId(q);
  return entries.filter(
    (entry) =>
      entry.name.toLowerCase().includes(q) ||
      `#${entry.id}`.includes(q) ||
      (id !== null && entry.id === id),
  );
}

/** The database id typed as "12", "#12" or " #12 ", or `null`. */
export function catalogId(text: string): number | null {
  const match = /^#?(\d{1,5})$/.exec(text.trim());
  if (match === null) return null;
  const id = Number(match[1]);
  return id >= 1 && id <= 99_999 ? id : null;
}

/** How many pending changes added or removed entries. */
export function countChangeKinds(
  changes: readonly { kind?: string }[],
): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const change of changes) {
    if (change.kind === "added") added += 1;
    else if (change.kind === "removed") removed += 1;
  }
  return { added, removed };
}
