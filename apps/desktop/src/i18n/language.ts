/** Display data of the languages the app ships with (no React, no Tauri). */
import type { Language } from "./detect";

/** The language's own name, never translated. */
export const LANGUAGE_NAMES: Record<Language, string> = {
  en: "English",
  tr: "Türkçe",
};

/** The short code on the header button. */
export const LANGUAGE_SHORT: Record<Language, string> = {
  en: "EN",
  tr: "TR",
};

/** Which flag belongs to a language (English uses the UK flag). */
export const LANGUAGE_FLAG: Record<Language, "gb" | "tr"> = {
  en: "gb",
  tr: "tr",
};

/** The short code for a language. */
export function languageShortCode(language: Language): string {
  return LANGUAGE_SHORT[language];
}

/** The language's own name. */
export function languageName(language: Language): string {
  return LANGUAGE_NAMES[language];
}
