/**
 * Pure settings logic (no DOM, no React). Storage is passed in, so every function can be tested
 * with a fake or a broken storage object. Theme and language keep their own keys and modules.
 */

import { DEFAULT_COLORS, isColorPreset, type ColorChoices, type ColorGroup } from "./colors";

export type SaveDefault = "saveAs" | "overwrite";

export type TextSize = "small" | "normal" | "large";

/** Root font sizes of the three text sizes, in pixels. */
export const TEXT_SIZE_PX: Record<TextSize, number> = {
  small: 15,
  normal: 16,
  large: 18,
};

export interface Settings {
  /** Show the background lattice. */
  patternOn: boolean;
  /** 0 to 100; 50 is the default strength. */
  patternStrength: number;
  /** What the main Save button does. */
  saveDefault: SaveDefault;
  /** Show the notifications bell. */
  showNotifications: boolean;
  /** Backups kept per file; 1 to 50, 10 by default. */
  keepBackups: number;
  /** Root font size of the whole interface. */
  textSize: TextSize;
  /** Preset of the changed, accent and error colours. */
  colors: ColorChoices;
}

export const SETTINGS_KEY = "saveprism.settings";

export const DEFAULT_SETTINGS: Settings = {
  patternOn: true,
  patternStrength: 50,
  saveDefault: "saveAs",
  showNotifications: true,
  keepBackups: 10,
  textSize: "normal",
  colors: DEFAULT_COLORS,
};

/** The two methods of `Storage` that settings need. */
export interface ReadableStorage {
  getItem(key: string): string | null;
}
export interface WritableStorage {
  setItem(key: string, value: string): void;
}

function clampStrength(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS.patternStrength;
  return Math.min(100, Math.max(0, Math.round(value)));
}

/** Clamps the number of backups kept to 1..50; unusable values give the default 10. */
export function clampKeepBackups(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS.keepBackups;
  return Math.min(50, Math.max(1, Math.round(value)));
}

function parseTextSize(value: unknown): TextSize {
  return value === "small" || value === "normal" || value === "large"
    ? value
    : DEFAULT_SETTINGS.textSize;
}

function parseColor(colors: unknown, group: ColorGroup): string {
  const value =
    typeof colors === "object" && colors !== null && !Array.isArray(colors)
      ? (colors as Record<string, unknown>)[group]
      : undefined;
  return isColorPreset(group, value) ? value : DEFAULT_COLORS[group];
}

/** Parses stored text. Anything unusable falls back to the default of that field. */
export function parseSettings(raw: string | null): Settings {
  if (raw === null) return { ...DEFAULT_SETTINGS };
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
  if (typeof data !== "object" || data === null || Array.isArray(data)) {
    return { ...DEFAULT_SETTINGS };
  }
  const record = data as Record<string, unknown>;
  return {
    patternOn:
      typeof record.patternOn === "boolean"
        ? record.patternOn
        : DEFAULT_SETTINGS.patternOn,
    patternStrength:
      typeof record.patternStrength === "number"
        ? clampStrength(record.patternStrength)
        : DEFAULT_SETTINGS.patternStrength,
    saveDefault:
      record.saveDefault === "saveAs" || record.saveDefault === "overwrite"
        ? record.saveDefault
        : DEFAULT_SETTINGS.saveDefault,
    showNotifications:
      typeof record.showNotifications === "boolean"
        ? record.showNotifications
        : DEFAULT_SETTINGS.showNotifications,
    keepBackups:
      typeof record.keepBackups === "number"
        ? clampKeepBackups(record.keepBackups)
        : DEFAULT_SETTINGS.keepBackups,
    textSize: parseTextSize(record.textSize),
    colors: {
      changed: parseColor(record.colors, "changed"),
      accent: parseColor(record.colors, "accent"),
      error: parseColor(record.colors, "error"),
    },
  };
}

/** Reads the settings; a missing or broken storage gives the defaults. */
export function loadSettings(storage: ReadableStorage | null | undefined): Settings {
  try {
    return parseSettings(storage ? storage.getItem(SETTINGS_KEY) : null);
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

/** Writes the settings. Returns false when the storage refused (the choice lasts for this session). */
export function saveSettings(
  settings: Settings,
  storage: WritableStorage | null | undefined,
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    return true;
  } catch {
    return false;
  }
}

/**
 * Opacity of the blue background lattice: about 12 % in dark and 14 % in light at the default
 * strength (50); 0 gives nothing, 100 gives twice as much.
 */
export function patternOpacity(strength: number, dark: boolean): number {
  const base = dark ? 0.12 : 0.14;
  return base * (clampStrength(strength) / 50);
}

/** The browser's localStorage, or null when it cannot even be reached. */
export function browserStorage(): Storage | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}
