// @vitest-environment node
import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, type Settings } from "./settings";
import {
  buildThemeFile,
  DEFAULT_THEME_NAME,
  defaultThemeFileName,
  ensureThemeFileName,
  MAX_THEME_BYTES,
  parseThemeFile,
  serializeThemeFile,
  themeSettingsPatch,
  type ThemeFile,
} from "./themeFile";

const valid: ThemeFile = {
  format: "saveprism-theme",
  version: 1,
  name: "Blue night",
  colors: { changed: "violet", accent: "#123abc", error: "red" },
  textSize: 110,
  background: { kind: "pattern", pattern: "dots", strength: 70 },
};

function parse(value: unknown) {
  return parseThemeFile(JSON.stringify(value));
}

describe("parseThemeFile", () => {
  it("accepts a valid theme file", () => {
    const result = parse(valid);
    expect(result).toEqual({ ok: true, theme: valid });
  });

  it("trims the name", () => {
    const result = parse({ ...valid, name: "  Blue night  " });
    expect(result.ok && result.theme.name).toBe("Blue night");
  });

  it("ignores unknown keys everywhere", () => {
    const result = parse({
      ...valid,
      extra: true,
      colors: { ...valid.colors, extra: "#ffffff" },
      background: { ...valid.background, extra: 1 },
    });
    expect(result).toEqual({ ok: true, theme: valid });
  });

  it("refuses text that is not JSON or larger than 64 KB", () => {
    expect(parseThemeFile("{not json")).toEqual({ ok: false, reason: "json" });
    expect(parseThemeFile("[]")).toEqual({ ok: false, reason: "json" });
    expect(parseThemeFile("x".repeat(MAX_THEME_BYTES + 1))).toEqual({
      ok: false,
      reason: "tooBig",
    });
  });

  it("refuses a wrong format or version", () => {
    expect(parse({ ...valid, format: "other" })).toEqual({
      ok: false,
      reason: "format",
    });
    expect(parse({ ...valid, version: 2 })).toEqual({
      ok: false,
      reason: "version",
    });
  });

  it("refuses a missing, empty or too long name", () => {
    expect(parse({ ...valid, name: undefined })).toEqual({
      ok: false,
      reason: "name",
    });
    expect(parse({ ...valid, name: "   " })).toEqual({
      ok: false,
      reason: "name",
    });
    expect(parse({ ...valid, name: "x".repeat(65) })).toEqual({
      ok: false,
      reason: "name",
    });
  });

  it("refuses a colour that is neither a preset nor #rrggbb", () => {
    expect(parse({ ...valid, colors: { ...valid.colors, accent: "not-a-color" } })).toEqual({
      ok: false,
      reason: "colors",
    });
    expect(parse({ ...valid, colors: { ...valid.colors, accent: "#12345" } })).toEqual({
      ok: false,
      reason: "colors",
    });
    expect(parse({ ...valid, colors: { changed: "violet", accent: "cyan" } })).toEqual({
      ok: false,
      reason: "colors",
    });
  });

  it("refuses a text size outside 80..140 or not an integer", () => {
    expect(parse({ ...valid, textSize: 79 })).toEqual({ ok: false, reason: "textSize" });
    expect(parse({ ...valid, textSize: 141 })).toEqual({ ok: false, reason: "textSize" });
    expect(parse({ ...valid, textSize: "110" })).toEqual({ ok: false, reason: "textSize" });
    expect(parse({ ...valid, textSize: 110.5 })).toEqual({
      ok: false,
      reason: "textSize",
    });
  });

  it("refuses a broken background", () => {
    expect(parse({ ...valid, background: { kind: "pattern", pattern: "stars" } })).toEqual({
      ok: false,
      reason: "background",
    });
    expect(
      parse({ ...valid, background: { kind: "pattern", pattern: "dots", strength: 101 } }),
    ).toEqual({ ok: false, reason: "background" });
    expect(parse({ ...valid, background: { kind: "photo", fit: "stretch" } })).toEqual({
      ok: false,
      reason: "background",
    });
    expect(
      parse({ ...valid, background: { kind: "photo", fit: "fit", blur: 101, darken: 20 } }),
    ).toEqual({ ok: false, reason: "background" });
    expect(parse({ ...valid, background: "photo" })).toEqual({
      ok: false,
      reason: "background",
    });
  });
});

describe("theme files from settings", () => {
  const settings: Settings = {
    ...DEFAULT_SETTINGS,
    textScale: 120,
    colors: { changed: "rose", accent: "#abcdef", error: "orange" },
    patternKind: "grid",
    patternStrength: 35,
  };

  it("builds the current pattern theme and round-trips it", () => {
    const theme = buildThemeFile(settings, "  My theme ");
    expect(theme).toMatchObject({
      format: "saveprism-theme",
      version: 1,
      name: "My theme",
      textSize: 120,
      colors: { changed: "rose", accent: "#abcdef", error: "orange" },
      background: { kind: "pattern", pattern: "grid", strength: 35 },
    });
    expect(parseThemeFile(serializeThemeFile(theme))).toEqual({ ok: true, theme });
  });

  it("uses the default name when there is none", () => {
    expect(buildThemeFile(settings, "   ").name).toBe(DEFAULT_THEME_NAME);
  });

  it("builds a photo theme from the photo settings", () => {
    const theme = buildThemeFile(
      { ...settings, backgroundMode: "photo", photoFit: "fit", photoBlur: 12, photoDarken: 65 },
      "Photo",
    );
    expect(theme.background).toEqual({ kind: "photo", fit: "fit", blur: 12, darken: 65 });
  });
});

describe("themeSettingsPatch", () => {
  it("applies colours, text size and a pattern", () => {
    const theme: ThemeFile = {
      ...valid,
      textSize: 115,
      background: { kind: "pattern" as const, pattern: "grid" as const, strength: 20 },
    };
    const patch = themeSettingsPatch(theme, DEFAULT_SETTINGS);
    expect(patch).toEqual({
      colors: valid.colors,
      textScale: 115,
      backgroundMode: "pattern",
      patternKind: "grid",
      patternStrength: 20,
    });
  });

  it("keeps the current photo for a photo theme", () => {
    const theme: ThemeFile = {
      ...valid,
      background: { kind: "photo" as const, fit: "fit" as const, blur: 5, darken: 70 },
    };
    const patch = themeSettingsPatch(theme, {
      ...DEFAULT_SETTINGS,
      photoName: "background.png",
    });
    expect(patch).toMatchObject({
      backgroundMode: "photo",
      photoFit: "fit",
      photoBlur: 5,
      photoDarken: 70,
    });
  });

  it("uses the Default pattern for a photo theme when there is no photo", () => {
    const theme: ThemeFile = {
      ...valid,
      background: { kind: "photo" as const, fit: "fill" as const, blur: 5, darken: 70 },
    };
    const patch = themeSettingsPatch(theme, { ...DEFAULT_SETTINGS, photoName: null });
    expect(patch).toMatchObject({
      backgroundMode: "pattern",
      patternKind: "default",
      photoFit: "fill",
      photoBlur: 5,
      photoDarken: 70,
    });
  });
});

describe("theme file names", () => {
  it("makes a safe export name", () => {
    expect(defaultThemeFileName("Blue Night!")).toBe(
      "blue-night.saveprism-theme.json",
    );
    expect(defaultThemeFileName("   ")).toBe("my-theme.saveprism-theme.json");
    expect(defaultThemeFileName(DEFAULT_THEME_NAME)).toBe("my-theme.saveprism-theme.json");
  });

  it("adds the suffix only when it is missing", () => {
    expect(ensureThemeFileName("C:\\themes\\blue")).toBe(
      "C:\\themes\\blue.saveprism-theme.json",
    );
    expect(ensureThemeFileName("blue.SavePrism-Theme.JSON")).toBe(
      "blue.SavePrism-Theme.JSON",
    );
  });
});
