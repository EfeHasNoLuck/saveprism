// @vitest-environment node
import { describe, expect, it } from "vitest";
import { changelogFor, changelogForLanguage, changelogSections } from "./changelog";

const SAMPLE = `# Changelog

## 0.2.0

- Newer note

## 0.1.0

### Open and edit
- First note
- Second note
`;

describe("changelog", () => {
  it("lists every version section in file order", () => {
    expect(changelogSections(SAMPLE).map((section) => section.version)).toEqual(["0.2.0", "0.1.0"]);
  });

  it("picks the section of the running version", () => {
    expect(changelogFor(SAMPLE, "0.1.0")).toEqual([
      "",
      "### Open and edit",
      "- First note",
      "- Second note",
    ]);
  });

  it("accepts a heading with a short suffix", () => {
    expect(changelogFor("## 0.1.0 (beta)\n- Note", "0.1.0")).toEqual(["- Note"]);
  });

  it("gives null for an unknown or empty version", () => {
    expect(changelogFor(SAMPLE, "9.9.9")).toBeNull();
    expect(changelogFor(SAMPLE, "")).toBeNull();
  });
});

describe("changelogForLanguage", () => {
  const TR = "## 0.1.0\n\n- Türkçe not\n";
  const SOURCES = { en: SAMPLE, tr: TR };

  it("picks the notes of the UI language", () => {
    expect(changelogForLanguage(SOURCES, "tr", "0.1.0")).toEqual(["", "- Türkçe not"]);
    expect(changelogForLanguage(SOURCES, "en", "0.1.0")).toEqual([
      "",
      "### Open and edit",
      "- First note",
      "- Second note",
    ]);
  });

  it("uses the base language of a regional tag", () => {
    expect(changelogForLanguage(SOURCES, "tr-TR", "0.1.0")).toEqual(["", "- Türkçe not"]);
  });

  it("falls back to English without a file for the language", () => {
    expect(changelogForLanguage(SOURCES, "de", "0.1.0")).toContain("- First note");
    expect(changelogForLanguage(SOURCES, "", "0.1.0")).toContain("- First note");
  });

  it("falls back to English when the language file lacks the version", () => {
    expect(changelogForLanguage(SOURCES, "tr", "0.2.0")).toEqual(["", "- Newer note"]);
  });

  it("gives null when neither file has the version", () => {
    expect(changelogForLanguage(SOURCES, "tr", "9.9.9")).toBeNull();
    expect(changelogForLanguage({ tr: "## 0.0.1\n- Eski not" }, "tr", "0.1.0")).toBeNull();
  });
});
