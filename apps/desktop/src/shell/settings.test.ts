// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  clampKeepBackups,
  clampKeepDays,
  DEFAULT_SETTINGS,
  loadSettings,
  parseSettings,
  parseTextScale,
  patternOpacity,
  photoBlurPx,
  saveSettings,
  SETTINGS_KEY,
  textScalePx,
} from "./settings";

describe("parseSettings", () => {
  it("gives the defaults for missing or broken text", () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("{not json")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("[1,2]")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("42")).toEqual(DEFAULT_SETTINGS);
  });

  it("keeps valid fields and replaces invalid ones one by one", () => {
    const parsed = parseSettings(
      JSON.stringify({
        saveDefault: "overwrite",
        showNotifications: false,
        keepBackups: 12,
      }),
    );
    expect(parsed).toMatchObject({
      saveDefault: "overwrite",
      showNotifications: false,
      keepBackups: 12,
      textScale: DEFAULT_SETTINGS.textScale,
      colors: DEFAULT_SETTINGS.colors,
    });
  });

  it("keeps 10 backups by default and clamps the number to 1..50", () => {
    expect(DEFAULT_SETTINGS.keepBackups).toBe(10);
    expect(parseSettings(JSON.stringify({ keepBackups: 0 })).keepBackups).toBe(1);
    expect(parseSettings(JSON.stringify({ keepBackups: 51 })).keepBackups).toBe(50);
    expect(parseSettings(JSON.stringify({ keepBackups: 12.6 })).keepBackups).toBe(13);
    expect(parseSettings(JSON.stringify({ keepBackups: "many" })).keepBackups).toBe(10);
    expect(clampKeepBackups(Number.NaN)).toBe(10);
  });

  it("keeps backups 30 days by default and clamps the days to 1..365", () => {
    expect(DEFAULT_SETTINGS.keepBackupDays).toBe(30);
    expect(parseSettings(JSON.stringify({ keepBackupDays: 0 })).keepBackupDays).toBe(1);
    expect(parseSettings(JSON.stringify({ keepBackupDays: 999 })).keepBackupDays).toBe(365);
    expect(parseSettings(JSON.stringify({ keepBackupDays: 12.6 })).keepBackupDays).toBe(13);
    expect(parseSettings(JSON.stringify({ keepBackupDays: "many" })).keepBackupDays).toBe(30);
    expect(clampKeepDays(Number.NaN)).toBe(30);
  });

  it("migrates the old Small/Normal/Large text size to percentages", () => {
    expect(parseSettings(JSON.stringify({ textSize: "small" })).textScale).toBe(90);
    expect(parseSettings(JSON.stringify({ textSize: "normal" })).textScale).toBe(100);
    expect(parseSettings(JSON.stringify({ textSize: "large" })).textScale).toBe(115);
    // Old files also carry the old fields; they must not break loading.
    expect(
      parseSettings(JSON.stringify({ textSize: "large", patternOn: true })).textScale,
    ).toBe(115);
  });

  it("snaps a text scale to 5 % steps and clamps it to 80..140", () => {
    expect(parseTextScale(102)).toBe(100);
    expect(parseTextScale(103)).toBe(105);
    expect(parseTextScale(10)).toBe(80);
    expect(parseTextScale(900)).toBe(140);
    expect(parseTextScale("huge")).toBe(100);
    expect(parseTextScale(Number.NaN)).toBe(100);
  });

  it("keeps valid colour choices and replaces unknown ones one by one", () => {
    const parsed = parseSettings(
      JSON.stringify({
        colors: { changed: "rose", accent: "#12ab34", error: "nope" },
      }),
    );
    expect(parsed.colors).toEqual({ changed: "rose", accent: "#12ab34", error: "red" });
    expect(
      parseSettings(JSON.stringify({ colors: { changed: "#012345" } })).colors,
    ).toEqual({ changed: "#012345", accent: "cyan", error: "red" });
    expect(parseSettings(JSON.stringify({ colors: "blue" })).colors).toEqual(
      DEFAULT_SETTINGS.colors,
    );
  });

  it("clamps the pattern strength and photo sliders to 0..100", () => {
    expect(parseSettings(JSON.stringify({ patternStrength: 900 })).patternStrength).toBe(100);
    expect(parseSettings(JSON.stringify({ patternStrength: -5 })).patternStrength).toBe(0);
    expect(parseSettings(JSON.stringify({ photoBlur: 900 })).photoBlur).toBe(100);
    expect(parseSettings(JSON.stringify({ photoDarken: -5 })).photoDarken).toBe(0);
  });

  it("turns the old off pattern into the faintest pattern", () => {
    expect(
      parseSettings(JSON.stringify({ patternOn: false, patternStrength: 50 })).patternStrength,
    ).toBe(0);
  });

  it("accepts the background modes, pattern kinds and photo fits", () => {
    expect(
      parseSettings(JSON.stringify({ backgroundMode: "photo" })).backgroundMode,
    ).toBe("photo");
    expect(parseSettings(JSON.stringify({ patternKind: "grid" })).patternKind).toBe("grid");
    expect(parseSettings(JSON.stringify({ patternKind: "dots" })).patternKind).toBe("dots");
    expect(parseSettings(JSON.stringify({ photoFit: "fit" })).photoFit).toBe("fit");
    expect(parseSettings(JSON.stringify({ patternKind: "star" })).patternKind).toBe("default");
    expect(parseSettings(JSON.stringify({ backgroundMode: "video" })).backgroundMode).toBe(
      "pattern",
    );
  });

  it("keeps folder lists and drops nonsense entries", () => {
    expect(
      parseSettings(JSON.stringify({ backupFolders: ["C:/a", "", "C:/a", 12] })).backupFolders,
    ).toEqual(["C:/a"]);
    expect(parseSettings(JSON.stringify({ backupFolders: "C:/a" })).backupFolders).toEqual([]);
    expect(parseSettings(JSON.stringify({ appFolder: "  " })).appFolder).toBeNull();
    expect(parseSettings(JSON.stringify({ appFolder: "C:/app" })).appFolder).toBe("C:/app");
  });

  it("rejects an unknown save default", () => {
    expect(parseSettings(JSON.stringify({ saveDefault: "delete" })).saveDefault).toBe("saveAs");
  });

  it("remembers a known AI app and falls back to Claude Desktop", () => {
    expect(parseSettings(null).mcpClient).toBe("claudeDesktop");
    expect(parseSettings(JSON.stringify({ mcpClient: "vscode" })).mcpClient).toBe("vscode");
    expect(parseSettings(JSON.stringify({ mcpClient: "unknown" })).mcpClient).toBe(
      "claudeDesktop",
    );
  });
});

describe("loadSettings and saveSettings", () => {
  it("round-trips through a working storage", () => {
    const data = new Map<string, string>();
    const storage = {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => void data.set(key, value),
    };
    const settings = { ...DEFAULT_SETTINGS, patternStrength: 80 };
    expect(saveSettings(settings, storage)).toBe(true);
    expect(data.has(SETTINGS_KEY)).toBe(true);
    expect(loadSettings(storage)).toEqual(settings);
  });

  it("survives a storage that throws", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadSettings(broken)).toEqual(DEFAULT_SETTINGS);
    expect(saveSettings(DEFAULT_SETTINGS, broken)).toBe(false);
  });

  it("survives a missing storage", () => {
    expect(loadSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(saveSettings(DEFAULT_SETTINGS, undefined)).toBe(false);
  });
});

describe("scales", () => {
  it("maps the text percentage to the root font size", () => {
    expect(textScalePx(100)).toBe(16);
    expect(textScalePx(80)).toBeCloseTo(12.8);
    expect(textScalePx(115)).toBeCloseTo(18.4);
    expect(textScalePx(140)).toBeCloseTo(22.4);
  });

  it("is about 12 % in dark and 14 % in light at the default strength", () => {
    expect(patternOpacity(50, true)).toBeCloseTo(0.12);
    expect(patternOpacity(50, false)).toBeCloseTo(0.14);
    expect(patternOpacity(0, true)).toBe(0);
  });

  it("maps the photo blur percentage to a CSS radius", () => {
    expect(photoBlurPx(0)).toBe(0);
    expect(photoBlurPx(30)).toBeCloseTo(6);
    expect(photoBlurPx(100)).toBeCloseTo(20);
  });
});
