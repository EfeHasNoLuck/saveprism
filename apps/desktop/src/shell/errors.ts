/** Pure helpers that turn a thrown value into text for a toast. */

import type { TFunction } from "i18next";

export interface ErrorParts {
  /** What went wrong (the reason). */
  reason: string;
  /** What the user can do (the fix); may be empty. */
  fix: string;
}

/** An error as the host sends it. */
interface HostError {
  code: string;
  message: string;
  hint: string;
}

/** Upper-cases the first letter ("no file is open" becomes "No file is open"). */
export function capitalize(text: string): string {
  return text === "" ? text : text.charAt(0).toUpperCase() + text.slice(1);
}

/** Adds a full stop when the text does not end with punctuation. */
function sentence(text: string): string {
  const trimmed = text.trim();
  if (trimmed === "") return "";
  return /[.!?…]$/.test(trimmed) ? trimmed : `${trimmed}.`;
}

/** The reason with its code, as the fallback and the tests show it. */
function withCode(text: string, code: string): string {
  const base = capitalize(text.trim()).replace(/[.!?…]+$/, "");
  return code === "" ? sentence(base) : `${base} (${code}).`;
}

/** The host-shaped parts of a thrown value, or null when it is not an `ApiError`. */
function hostError(error: unknown): HostError | null {
  if (typeof error !== "object" || error === null) return null;
  const record = error as Record<string, unknown>;
  if (typeof record.message !== "string" || typeof record.hint !== "string") return null;
  return {
    code: typeof record.code === "string" ? record.code : "",
    message: record.message,
    hint: record.hint,
  };
}

/** Codes whose host message carries a value the translated text repeats. */
const PARAM_CODES = new Set(["SP-APP-004", "SP-APP-005", "SP-APP-013", "SP-APP-020"]);

/**
 * The parameters of a translated error, read from the host's English message. `null` when the
 * message has another shape, so the plain (parameter-free) text is used instead.
 */
export function errorParams(code: string, message: string): Record<string, string> | null {
  const quoted = message.match(/"([^"]*)"/);
  if (code === "SP-APP-004" || code === "SP-APP-020") {
    return quoted === null ? null : { value: quoted[1] ?? "" };
  }
  if (code === "SP-APP-013") {
    return quoted === null ? null : { name: quoted[1] ?? "" };
  }
  if (code === "SP-APP-005") {
    const range = message.match(
      /^(-?\d+) is outside the range of this value \((-?\d+) to (-?\d+)\)$/,
    );
    if (range !== null) {
      return { value: range[1] ?? "", low: range[2] ?? "", high: range[3] ?? "" };
    }
    const count = message.match(/^(-?\d+) is outside the range of a count/);
    return count === null ? null : { value: count[1] ?? "" };
  }
  return null;
}

/** The translated reason and fix of a known code, or null for an unknown one. */
function translated(host: HostError, t: TFunction): ErrorParts | null {
  if (host.code === "") return null;
  const base = `errors.codes.${host.code}`;
  const params = PARAM_CODES.has(host.code) ? errorParams(host.code, host.message) : {};
  const reasonKey = params === null ? `${base}.reasonPlain` : `${base}.reason`;
  const reason = t(reasonKey, { ...(params ?? {}), defaultValue: "" });
  if (reason === "") return null;
  const fix = t(`${base}.fix`, { defaultValue: "" });
  return { reason: withCode(reason, host.code), fix: sentence(fix) };
}

/**
 * Splits whatever was thrown into reason and fix. The host sends `{ code, message, hint }`; a
 * known code is shown in the language of `t` and an unknown one as the host's English text.
 * Anything else is shown as plain text without a fix.
 */
export function describeError(error: unknown, t?: TFunction): ErrorParts {
  const host = hostError(error);
  if (host !== null) {
    if (t !== undefined) {
      const known = translated(host, t);
      if (known !== null) return known;
    }
    return { reason: withCode(host.message, host.code), fix: sentence(host.hint) };
  }
  const text = error instanceof Error ? error.message : String(error);
  return { reason: sentence(capitalize(text)), fix: "" };
}
