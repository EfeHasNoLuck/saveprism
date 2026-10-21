/**
 * Pure language detection logic. Touches no DOM and no storage APIs, so it can
 * be unit-tested in a node environment.
 */
export const SUPPORTED_LANGUAGES = ["en", "tr"] as const;

export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** localStorage key that holds the user's explicit language choice. */
export const STORAGE_KEY = "saveprism.lang";

/** Type guard: true only for exactly supported language codes. */
export function isLanguage(value: unknown): value is Language {
  return (
    typeof value === "string" &&
    (SUPPORTED_LANGUAGES as readonly string[]).includes(value)
  );
}

export interface DetectInput {
  /** Value previously saved under STORAGE_KEY, if any. */
  stored: string | null | undefined;
  /** Browser/OS language tag such as "tr-TR" or "en-US", if any. */
  navigatorLanguage: string | null | undefined;
}

/**
 * Chooses the active language.
 * 1. A valid stored value wins.
 * 2. Otherwise a Turkish navigator language ("tr", "tr-TR", ...) yields "tr".
 * 3. Everything else yields "en".
 */
export function detectLanguage({
  stored,
  navigatorLanguage,
}: DetectInput): Language {
  if (isLanguage(stored)) {
    return stored;
  }
  const primary = (navigatorLanguage ?? "").trim().split(/[-_]/)[0];
  return primary?.toLowerCase() === "tr" ? "tr" : "en";
}
