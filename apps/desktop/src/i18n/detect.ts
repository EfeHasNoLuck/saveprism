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
}

/**
 * Chooses the active language: a valid stored value wins, otherwise English.
 * The OS language is not used, so the app always starts in English until the
 * user picks another language.
 */
export function detectLanguage({ stored }: DetectInput): Language {
  return isLanguage(stored) ? stored : "en";
}
