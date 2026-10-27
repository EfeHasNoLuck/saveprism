/**
 * The SavePrism theme file: a typed JSON object that carries the Appearance choices. It is data
 * only; nothing in it is executed. Export builds it from the current settings, import validates
 * every field before anything changes (unknown keys are ignored).
 */

import { COLOR_GROUPS, isColorChoice, type ColorChoices } from "./colors";
import {
  parseTextScale,
  type PatternKind,
  type PhotoFit,
  type Settings,
} from "./settings";

export const THEME_FORMAT = "saveprism-theme";
export const THEME_VERSION = 1;
export const THEME_SUFFIX = ".saveprism-theme.json";
export const MAX_THEME_BYTES = 64 * 1024;
/** Name used when the player has not typed one; also the export file name. */
export const DEFAULT_THEME_NAME = "SavePrism theme";
const MAX_NAME_CHARS = 64;

export type ThemeBackground =
  | { kind: "pattern"; pattern: PatternKind; strength: number }
  | { kind: "photo"; fit: PhotoFit; blur: number; darken: number };

export interface ThemeFile {
  format: typeof THEME_FORMAT;
  version: typeof THEME_VERSION;
  name: string;
  colors: ColorChoices;
  /** Text size in percent, 80 to 140. */
  textSize: number;
  background: ThemeBackground;
}

/** Why a theme file was refused; the UI translates it. */
export type ThemeFileError =
  | "tooBig"
  | "json"
  | "format"
  | "version"
  | "name"
  | "colors"
  | "textSize"
  | "background";

export type ThemeParseResult =
  | { ok: true; theme: ThemeFile }
  | { ok: false; reason: ThemeFileError };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isIntegerIn(value: unknown, min: number, max: number): value is number {
  return (
    typeof value === "number" &&
    Number.isInteger(value) &&
    value >= min &&
    value <= max
  );
}

function isPatternKind(value: unknown): value is PatternKind {
  return value === "grid" || value === "default" || value === "dots";
}

function isPhotoFit(value: unknown): value is PhotoFit {
  return value === "fit" || value === "fill";
}

/** Builds the exportable theme from the live settings. */
export function buildThemeFile(settings: Settings, name: string): ThemeFile {
  const background: ThemeBackground =
    settings.backgroundMode === "photo"
      ? {
          kind: "photo",
          fit: settings.photoFit,
          blur: settings.photoBlur,
          darken: settings.photoDarken,
        }
      : {
          kind: "pattern",
          pattern: settings.patternKind,
          strength: settings.patternStrength,
        };
  return {
    format: THEME_FORMAT,
    version: THEME_VERSION,
    name: name.trim() || DEFAULT_THEME_NAME,
    colors: { ...settings.colors },
    textSize: settings.textScale,
    background,
  };
}

/** The text written to disk (pretty JSON with a final newline). */
export function serializeThemeFile(theme: ThemeFile): string {
  return `${JSON.stringify(theme, null, 2)}\n`;
}

/** Parses and validates a theme file. */
export function parseThemeFile(text: string): ThemeParseResult {
  if (new TextEncoder().encode(text).length > MAX_THEME_BYTES) {
    return { ok: false, reason: "tooBig" };
  }
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    return { ok: false, reason: "json" };
  }
  if (!isRecord(data)) return { ok: false, reason: "json" };
  if (data.format !== THEME_FORMAT) return { ok: false, reason: "format" };
  if (data.version !== THEME_VERSION) return { ok: false, reason: "version" };

  const name = typeof data.name === "string" ? data.name.trim() : "";
  if (name === "" || name.length > MAX_NAME_CHARS) {
    return { ok: false, reason: "name" };
  }

  if (!isRecord(data.colors)) return { ok: false, reason: "colors" };
  const colors = {} as ColorChoices;
  for (const group of COLOR_GROUPS) {
    const value = data.colors[group];
    if (!isColorChoice(group, value)) return { ok: false, reason: "colors" };
    colors[group] = value;
  }

  if (!isIntegerIn(data.textSize, 80, 140)) {
    return { ok: false, reason: "textSize" };
  }

  const background = data.background;
  if (!isRecord(background)) return { ok: false, reason: "background" };
  let parsedBackground: ThemeBackground;
  if (background.kind === "pattern") {
    if (
      !isPatternKind(background.pattern) ||
      !isIntegerIn(background.strength, 0, 100)
    ) {
      return { ok: false, reason: "background" };
    }
    parsedBackground = {
      kind: "pattern",
      pattern: background.pattern,
      strength: background.strength,
    };
  } else if (background.kind === "photo") {
    if (
      !isPhotoFit(background.fit) ||
      !isIntegerIn(background.blur, 0, 100) ||
      !isIntegerIn(background.darken, 0, 100)
    ) {
      return { ok: false, reason: "background" };
    }
    parsedBackground = {
      kind: "photo",
      fit: background.fit,
      blur: background.blur,
      darken: background.darken,
    };
  } else {
    return { ok: false, reason: "background" };
  }

  return {
    ok: true,
    theme: {
      format: THEME_FORMAT,
      version: THEME_VERSION,
      name,
      colors,
      textSize: data.textSize,
      background: parsedBackground,
    },
  };
}

/**
 * The settings an imported theme changes. A photo theme keeps the photo already in the app folder;
 * with no photo it falls back to the Default pattern (the image itself is not in the file in v0.1).
 */
export function themeSettingsPatch(
  theme: ThemeFile,
  current: Settings,
): Partial<Settings> {
  const patch: Partial<Settings> = {
    colors: { ...theme.colors },
    textScale: parseTextScale(theme.textSize),
  };
  const background = theme.background;
  if (background.kind === "pattern") {
    patch.backgroundMode = "pattern";
    patch.patternKind = background.pattern;
    patch.patternStrength = background.strength;
  } else {
    patch.photoFit = background.fit;
    patch.photoBlur = background.blur;
    patch.photoDarken = background.darken;
    if (current.photoName) {
      patch.backgroundMode = "photo";
    } else {
      patch.backgroundMode = "pattern";
      patch.patternKind = "default";
    }
  }
  return patch;
}

/** A safe `<name>.saveprism-theme.json` file name for the export dialog. */
export function defaultThemeFileName(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${slug || "saveprism-theme"}${THEME_SUFFIX}`;
}

/** Adds the theme suffix when the player typed a name without it. */
export function ensureThemeFileName(path: string): string {
  return path.toLowerCase().endsWith(THEME_SUFFIX) ? path : `${path}${THEME_SUFFIX}`;
}
