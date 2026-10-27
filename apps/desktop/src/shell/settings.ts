/**
 * Pure settings logic (no DOM, no React). Storage is passed in, so every function can be tested
 * with a fake or a broken storage object. Theme and language keep their own keys and modules.
 */

import { DEFAULT_COLORS, isColorChoice, type ColorChoices, type ColorGroup } from "./colors";
import { DEFAULT_MCP_CLIENT, isMcpClient, type McpClient } from "./setup";

export type SaveDefault = "saveAs" | "overwrite";

/** How the background is drawn. */
export type BackgroundMode = "pattern" | "photo";
/** Which pattern the background shows. */
export type PatternKind = "grid" | "default" | "dots";
/** How the photo is fitted into the window. */
export type PhotoFit = "fit" | "fill";

/** Lowest text size, in percent. */
export const TEXT_SCALE_MIN = 80;
/** Highest text size, in percent. */
export const TEXT_SCALE_MAX = 140;
/** Step of the text size slider, in percent. */
export const TEXT_SCALE_STEP = 5;
/** Default text size, in percent. */
export const DEFAULT_TEXT_SCALE = 100;

export interface Settings {
  /** What the main Save button does. */
  saveDefault: SaveDefault;
  /** Show the notifications bell. */
  showNotifications: boolean;
  /** Backups kept per file; 1 to 50, 10 by default. */
  keepBackups: number;
  /** Backups older than this many days are removed; 1 to 365, 30 by default. */
  keepBackupDays: number;
  /** Root font size in percent of the default 16 px; 80 to 140. */
  textScale: number;
  /** Preset id or custom `#rrggbb` for the changed, accent and error colours. */
  colors: ColorChoices;
  /** 0 to 100; 50 is the default strength. */
  patternStrength: number;
  /** Grid (plain squares), Default (the logo's nested squares) or Dots. */
  patternKind: PatternKind;
  /** A pattern or a photo behind the app. */
  backgroundMode: BackgroundMode;
  /** Fit (whole image) or Fill (cover the window). */
  photoFit: PhotoFit;
  /** 0 to 100; the CSS blur radius follows the window. 30 by default. */
  photoBlur: number;
  /** 0 to 100; how dark the photo is made so text stays readable. 40 by default. */
  photoDarken: number;
  /** The file name of the copied photo, so a new choice reloads it. */
  photoName: string | null;
  /** Backup folders the player used, newest first (empty: the default folder). */
  backupFolders: string[];
  /** The app folder the player picked, or null for the default. */
  appFolder: string | null;
  /** The AI app the MCP setup is prepared for. */
  mcpClient: McpClient;
}

export const SETTINGS_KEY = "saveprism.settings";

export const DEFAULT_SETTINGS: Settings = {
  saveDefault: "saveAs",
  showNotifications: true,
  keepBackups: 10,
  keepBackupDays: 30,
  textScale: DEFAULT_TEXT_SCALE,
  colors: DEFAULT_COLORS,
  patternStrength: 50,
  patternKind: "default",
  backgroundMode: "pattern",
  photoFit: "fill",
  photoBlur: 30,
  photoDarken: 40,
  photoName: null,
  backupFolders: [],
  appFolder: null,
  mcpClient: DEFAULT_MCP_CLIENT,
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

/** Clamps the number of days backups are kept to 1..365; unusable values give 30. */
export function clampKeepDays(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS.keepBackupDays;
  return Math.min(365, Math.max(1, Math.round(value)));
}

/**
 * The text size in percent: a number snapped to the slider's 5 % steps and clamped to 80..140.
 * The three values of the old Small/Normal/Large setting migrate to 90/100/115.
 */
export function parseTextScale(value: unknown): number {
  const migrated =
    value === "small" ? 90 : value === "normal" ? 100 : value === "large" ? 115 : value;
  if (typeof migrated !== "number" || !Number.isFinite(migrated)) return DEFAULT_TEXT_SCALE;
  const snapped = Math.round(migrated / TEXT_SCALE_STEP) * TEXT_SCALE_STEP;
  return Math.min(TEXT_SCALE_MAX, Math.max(TEXT_SCALE_MIN, snapped));
}

/** The root font size in pixels for a text scale in percent. */
export function textScalePx(scale: number): number {
  return (16 * parseTextScale(scale)) / 100;
}

function parseColor(colors: unknown, group: ColorGroup): string {
  const value =
    typeof colors === "object" && colors !== null && !Array.isArray(colors)
      ? (colors as Record<string, unknown>)[group]
      : undefined;
  return isColorChoice(group, value) ? value : DEFAULT_COLORS[group];
}

function parsePatternKind(value: unknown): PatternKind {
  return value === "grid" || value === "default" || value === "dots"
    ? value
    : DEFAULT_SETTINGS.patternKind;
}

function parsePhotoFit(value: unknown): PhotoFit {
  return value === "fit" || value === "fill" ? value : DEFAULT_SETTINGS.photoFit;
}

function parseFolders(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const folders = value.filter(
    (item): item is string => typeof item === "string" && item.trim() !== "",
  );
  return [...new Set(folders)];
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
  // The old setting switched the lattice off completely; the new page has no off switch, so it
  // becomes the faintest pattern instead.
  const legacyOff = record.patternOn === false;
  const backgroundMode: BackgroundMode =
    record.backgroundMode === "photo" || record.backgroundMode === "pattern"
      ? record.backgroundMode
      : DEFAULT_SETTINGS.backgroundMode;
  return {
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
    keepBackupDays:
      typeof record.keepBackupDays === "number"
        ? clampKeepDays(record.keepBackupDays)
        : DEFAULT_SETTINGS.keepBackupDays,
    textScale: parseTextScale(record.textScale ?? record.textSize),
    colors: {
      changed: parseColor(record.colors, "changed"),
      accent: parseColor(record.colors, "accent"),
      error: parseColor(record.colors, "error"),
    },
    patternStrength: legacyOff
      ? 0
      : typeof record.patternStrength === "number"
        ? clampStrength(record.patternStrength)
        : DEFAULT_SETTINGS.patternStrength,
    patternKind: parsePatternKind(record.patternKind),
    backgroundMode,
    photoFit: parsePhotoFit(record.photoFit),
    photoBlur:
      typeof record.photoBlur === "number"
        ? clampStrength(record.photoBlur)
        : DEFAULT_SETTINGS.photoBlur,
    photoDarken:
      typeof record.photoDarken === "number"
        ? clampStrength(record.photoDarken)
        : DEFAULT_SETTINGS.photoDarken,
    photoName: typeof record.photoName === "string" ? record.photoName : null,
    backupFolders: parseFolders(record.backupFolders),
    appFolder:
      typeof record.appFolder === "string" && record.appFolder.trim() !== ""
        ? record.appFolder
        : null,
    mcpClient: isMcpClient(record.mcpClient) ? record.mcpClient : DEFAULT_MCP_CLIENT,
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

/** The CSS blur radius in pixels for the photo's blur setting (30 % is 6 px). */
export function photoBlurPx(blur: number): number {
  return (clampStrength(blur) / 100) * 20;
}

/** The browser's localStorage, or null when it cannot even be reached. */
export function browserStorage(): Storage | null {
  try {
    return typeof window !== "undefined" ? window.localStorage : null;
  } catch {
    return null;
  }
}
