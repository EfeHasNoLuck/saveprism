// @vitest-environment node
import { describe, expect, it } from "vitest";
import { searchSettings, SETTINGS_INDEX } from "./settingsSearch";

const EN: Record<string, string> = {
  "settings.language": "Language",
  "settings.theme": "Theme",
  "settings.textSize": "Text size",
  "settings.changedColor": "Changed values",
  "settings.accentColor": "Accent",
  "settings.errorColor": "Errors",
  "settings.background": "Background",
  "settings.patternStrength": "Pattern strength",
  "settings.photo": "Photo",
  "settings.photoFit": "Photo fit",
  "settings.blur": "Blur",
  "settings.darken": "Darken",
  "settings.defaultAction": "Default save action",
  "settings.keepBackups": "Backups to keep",
  "settings.keepDays": "Keep backups for",
  "settings.backupFolder": "Backup folder",
  "settings.appFolder": "App folder",
};
const TR: Record<string, string> = {
  ...EN,
  "settings.language": "Dil",
  "settings.textSize": "Yazı boyutu",
  "settings.keepBackups": "Tutulacak yedekler",
};

const en = (key: string) => EN[key] ?? key;
const tr = (key: string) => TR[key] ?? key;

describe("searchSettings", () => {
  it("filters rows by their label in the current language", () => {
    expect(searchSettings("text", en).map((entry) => entry.id)).toEqual([
      "appearance.textSize",
    ]);
    expect(searchSettings("YAZI", tr, "tr").map((entry) => entry.id)).toEqual([
      "appearance.textSize",
    ]);
  });

  it("finds every row whose label holds the query and names its tab", () => {
    const rows = searchSettings("backup", en);
    expect(rows.length).toBeGreaterThan(1);
    expect(rows.every((entry) => entry.tab === "backups")).toBe(true);
    expect(rows.map((entry) => entry.id)).toContain("backups.keep");
  });

  it("ignores case, surrounding spaces and empty text", () => {
    expect(searchSettings("  THEME ", en).map((entry) => entry.id)).toEqual(["general.theme"]);
    expect(searchSettings("   ", en)).toEqual([]);
    expect(searchSettings("nothing like this", en)).toEqual([]);
  });

  it("has every row pointing at a real tab", () => {
    const tabs = new Set(["general", "appearance", "backups", "ai", "shortcuts", "about"]);
    for (const entry of SETTINGS_INDEX) {
      expect(tabs.has(entry.tab)).toBe(true);
    }
  });
});
