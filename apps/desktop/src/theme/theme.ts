/**
 * Pure theme logic. No DOM, no React, no storage access here, so it can be
 * unit-tested in a node environment.
 */

export type ThemePreference = "light" | "dark" | "system";

export type ResolvedTheme = "light" | "dark";

/** localStorage key that holds the user's theme preference. */
export const STORAGE_KEY = "saveprism.theme";

/**
 * Parses a raw stored value. Anything that is not exactly one of the three
 * known preferences (including `null` and garbage text) falls back to "system".
 */
export function parseStoredPreference(raw: string | null): ThemePreference {
  if (raw === "light" || raw === "dark" || raw === "system") {
    return raw;
  }
  return "system";
}

/**
 * Resolves a preference to the concrete theme that must be shown.
 * "system" follows the OS setting; "light" and "dark" are used as-is.
 */
export function resolveTheme(
  pref: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (pref === "system") {
    return systemPrefersDark ? "dark" : "light";
  }
  return pref;
}
