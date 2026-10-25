// @vitest-environment node
import { describe, expect, it } from "vitest";
import { ErrorCodes, type FileReport } from "@/api/commands";
import en from "../../locales/en/common.json";
import tr from "../../locales/tr/common.json";
import { buildFileReport, GAME_LINE, REPORT_TITLE } from "./fileReport";

const SAMPLE: FileReport = {
  extension: "sav",
  byteSize: 1234567,
  headHex: "53 41 56 45 00 00 00 01 20 68 65 6c 6c 6f",
  headAscii: "SAVE.... hello",
  entropy: "binary",
  sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  version: "0.1.0",
};

/** A key such as `toast.copyReport` looked up in a nested locale bundle. */
function lookup(bundle: unknown, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>(
      (node, part) =>
        node !== null && typeof node === "object"
          ? (node as Record<string, unknown>)[part]
          : undefined,
      bundle,
    );
}

describe("buildFileReport", () => {
  it("writes a short Markdown-friendly report", () => {
    expect(buildFileReport(SAMPLE)).toBe(
      [
        "SavePrism file report",
        "- Game: (please write the game name here)",
        "- SavePrism version: 0.1.0",
        "- File extension: .sav",
        "- Size: 1,234,567 bytes",
        "- First bytes (hex): `53 41 56 45 00 00 00 01 20 68 65 6c 6c 6f`",
        "- First bytes (text): `SAVE.... hello`",
        "- Looks like: binary data",
        "- SHA-256: `ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`",
      ].join("\n"),
    );
  });

  it("starts with the title and asks for the game name", () => {
    const text = buildFileReport(SAMPLE);
    expect(text.startsWith("SavePrism file report\n")).toBe(true);
    expect(REPORT_TITLE).toBe("SavePrism file report");
    expect(text).toContain("Game: (please write the game name here)");
    expect(text.split("\n")).toContain(GAME_LINE);
  });

  it("holds no path or file name", () => {
    const text = buildFileReport(SAMPLE);
    expect(text).not.toMatch(/[A-Za-z]:\\/);
    expect(text).not.toMatch(/\\|\/Users\/|\/home\//);
    // A report has no field for a path, so the host's answer cannot carry one into the text.
    expect(Object.keys(SAMPLE).sort()).toEqual(
      ["byteSize", "entropy", "extension", "headAscii", "headHex", "sha256", "version"].sort(),
    );
  });

  it("says so when there is no extension or no hash", () => {
    const text = buildFileReport({ ...SAMPLE, extension: "", sha256: null, entropy: "compressed" });
    expect(text).toContain("- File extension: (none)");
    expect(text).toContain("- SHA-256: (not computed)");
    expect(text).toContain("- Looks like: compressed or encrypted data (looks random)");
  });

  it("handles an empty file", () => {
    const text = buildFileReport({
      ...SAMPLE,
      byteSize: 0,
      headHex: "",
      headAscii: "",
      entropy: "empty",
    });
    expect(text).toContain("- Size: 0 bytes");
    expect(text).toContain("- First bytes (hex): `(none)`");
    expect(text).toContain("- Looks like: an empty file");
  });
});

describe("file report strings", () => {
  const keys = [
    "toast.copyReport",
    "toast.reportCopied",
    "toast.reportCopiedBody",
    "toast.reportFailed",
    "errors.codes.SP-APP-027.reason",
    "errors.codes.SP-APP-027.fix",
  ];

  it("has every key in English and Turkish", () => {
    for (const key of keys) {
      for (const [name, bundle] of [["en", en], ["tr", tr]] as const) {
        const value = lookup(bundle, key);
        expect(typeof value, `${name} is missing ${key}`).toBe("string");
        expect((value as string).trim(), `${name} has an empty ${key}`).not.toBe("");
      }
      expect(lookup(tr, key), `${key} is not translated`).not.toBe(lookup(en, key));
    }
  });

  it("says what the success toast says", () => {
    expect(lookup(en, "toast.reportCopied")).toBe("Report copied");
    expect(lookup(en, "toast.reportCopiedBody")).toBe(
      "Paste it into a new issue with the game's name.",
    );
  });

  it("offers the report for the unknown-format code", () => {
    expect(ErrorCodes.unknownFormat).toBe("SP-DET-001");
    expect(typeof lookup(en, "errors.codes.SP-DET-001.reason")).toBe("string");
  });
});
