/**
 * Internationalisation contract (filled in by M0-7). The exported names and
 * signatures below are stable: `main.tsx` calls `initI18n()` before first
 * render and `App.tsx` renders `LanguageSwitcher`.
 */
export const SUPPORTED_LANGUAGES = ["en", "tr"] as const;

export type Language = (typeof SUPPORTED_LANGUAGES)[number];

/** Initialises translations and the active language. Stub: does nothing. */
export function initI18n(): void {}

/** Control for switching the language. Stub: renders nothing. */
export function LanguageSwitcher() {
  return null;
}
