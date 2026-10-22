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

export interface HintInput {
  hint: string;
  /** Already translated, for example "was 5". */
  wasText: string;
  changed: boolean;
  focused: boolean;
  /** Shown even when not focused or changed (Gold). */
  always: boolean;
  /** Out of range, not a number, or rejected by the host. */
  invalid: boolean;
}

export interface HintLine {
  text: string;
  error: boolean;
}

/** The line under a field, or `null` when none should show. */
export function hintLine(i: HintInput): HintLine | null {
  if (i.invalid) return i.hint ? { text: i.hint, error: true } : null;
  if (!(i.always || i.focused || i.changed)) return null;
  const parts: string[] = [];
  if (i.hint) parts.push(i.hint);
  if (i.changed) parts.push(i.wasText);
  return parts.length > 0 ? { text: parts.join(" · "), error: false } : null;
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
