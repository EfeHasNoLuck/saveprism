// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
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
    });
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
  it("is about 3 % in dark and 2 % in light at the default strength", () => {
    expect(patternOpacity(50, true)).toBeCloseTo(0.03);
    expect(patternOpacity(50, false)).toBeCloseTo(0.02);
  });

  it("is 0 at strength 0 and scales up to twice the default", () => {
    expect(patternOpacity(0, true)).toBe(0);
    expect(patternOpacity(100, true)).toBeCloseTo(0.06);
    expect(patternOpacity(1000, false)).toBeCloseTo(0.04);
    expect(patternOpacity(Number.NaN, true)).toBeCloseTo(0.03);
  });
});
