// @vitest-environment node
import { describe, expect, it } from "vitest";
import { changelogFor, changelogSections } from "./changelog";

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
