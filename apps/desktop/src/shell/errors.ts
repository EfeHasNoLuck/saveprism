/** Pure helpers that turn a thrown value into text for a toast. */

export interface ErrorParts {
  /** What went wrong (the reason). */
  reason: string;
  /** What the user can do (the fix); may be empty. */
  fix: string;
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

/**
 * Splits whatever was thrown into reason and fix. The host sends `{ code, message, hint }`;
 * anything else is shown as plain text without a fix.
 */
export function describeError(error: unknown): ErrorParts {
  if (typeof error === "object" && error !== null) {
    const record = error as Record<string, unknown>;
    if (typeof record.message === "string" && typeof record.hint === "string") {
      const base = capitalize(record.message.trim()).replace(/[.!?…]+$/, "");
      const reason =
        typeof record.code === "string" && base !== ""
          ? `${base} (${record.code}).`
          : sentence(base);
      return { reason, fix: sentence(record.hint) };
    }
  }
  const text = error instanceof Error ? error.message : String(error);
  return { reason: sentence(capitalize(text)), fix: "" };
}
