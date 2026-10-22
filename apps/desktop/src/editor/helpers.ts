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

/** Keeps entries whose name, `#id` or value contains the query (case-insensitive). */
export function filterEntries<T extends Searchable>(entries: readonly T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...entries];
  return entries.filter(
    (e) =>
      (e.name !== null && e.name.toLowerCase().includes(q)) ||
      `#${e.id}`.includes(q) ||
      e.field.value.toLowerCase().includes(q),
  );
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
