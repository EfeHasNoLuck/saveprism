// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  clampKeepBackups,
  DEFAULT_SETTINGS,
  loadSettings,
  parseSettings,
  patternOpacity,
  saveSettings,
  SETTINGS_KEY,
} from "./settings";

describe("parseSettings", () => {
  it("gives the defaults for missing or broken text", () => {
    expect(parseSettings(null)).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("{not json")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("[1,2]")).toEqual(DEFAULT_SETTINGS);
    expect(parseSettings("42")).toEqual(DEFAULT_SETTINGS);
  });

  it("keeps recent files off by default", () => {
    expect(DEFAULT_SETTINGS.showRecent).toBe(false);
  });

  it("keeps valid fields and replaces invalid ones one by one", () => {
    const parsed = parseSettings(
      JSON.stringify({
        patternOn: false,
        patternStrength: "loud",
        saveDefault: "overwrite",
        showRecent: "yes",
        showNotifications: false,
      }),
    );
    expect(parsed).toEqual({
      patternOn: false,
      patternStrength: DEFAULT_SETTINGS.patternStrength,
      saveDefault: "overwrite",
      showRecent: DEFAULT_SETTINGS.showRecent,
      showNotifications: false,
      keepBackups: DEFAULT_SETTINGS.keepBackups,
      textSize: DEFAULT_SETTINGS.textSize,
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

  it("accepts the three text sizes and replaces anything else", () => {
    for (const textSize of ["small", "normal", "large"] as const) {
      expect(parseSettings(JSON.stringify({ textSize })).textSize).toBe(textSize);
    }
    expect(parseSettings(JSON.stringify({ textSize: "huge" })).textSize).toBe("normal");
  });

  it("keeps valid colour presets and replaces unknown ones one by one", () => {
    const parsed = parseSettings(
      JSON.stringify({
        colors: { changed: "rose", accent: "green", error: "nope" },
      }),
    );
    expect(parsed.colors).toEqual({ changed: "rose", accent: "green", error: "red" });
    expect(
      parseSettings(JSON.stringify({ colors: { changed: "nope" } })).colors,
    ).toEqual({ changed: "violet", accent: "cyan", error: "red" });
    expect(parseSettings(JSON.stringify({ colors: "blue" })).colors).toEqual(
      DEFAULT_SETTINGS.colors,
    );
  });

  it("clamps the pattern strength to 0..100", () => {
    expect(parseSettings(JSON.stringify({ patternStrength: 900 })).patternStrength).toBe(100);
    expect(parseSettings(JSON.stringify({ patternStrength: -5 })).patternStrength).toBe(0);
  });

  it("rejects an unknown save default", () => {
    expect(parseSettings(JSON.stringify({ saveDefault: "delete" })).saveDefault).toBe("saveAs");
  });
});

describe("loadSettings and saveSettings", () => {
  it("round-trips through a working storage", () => {
    const data = new Map<string, string>();
    const storage = {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => void data.set(key, value),
    };
    const settings = { ...DEFAULT_SETTINGS, showRecent: true, patternStrength: 80 };
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

describe("patternOpacity", () => {
  it("is about 12 % in dark and 14 % in light at the default strength", () => {
    expect(patternOpacity(50, true)).toBeCloseTo(0.12);
    expect(patternOpacity(50, false)).toBeCloseTo(0.14);
  });

  it("is 0 at strength 0 and scales up to twice the default", () => {
    expect(patternOpacity(0, true)).toBe(0);
    expect(patternOpacity(100, true)).toBeCloseTo(0.24);
    expect(patternOpacity(1000, false)).toBeCloseTo(0.28);
    expect(patternOpacity(Number.NaN, true)).toBeCloseTo(0.12);
  });
});
