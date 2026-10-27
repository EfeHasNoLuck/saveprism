// @vitest-environment node
import { describe, expect, it } from "vitest";
import { SUPPORTED_LANGUAGES } from "./detect";
import { LANGUAGE_FLAG, languageName, languageShortCode } from "./language";

describe("language menu data", () => {
  it("has a short code, an own name and a flag for every shipped language", () => {
    for (const language of SUPPORTED_LANGUAGES) {
      expect(languageShortCode(language)).toMatch(/^[A-Z]{2}$/);
      expect(languageName(language).trim()).not.toBe("");
      expect(["gb", "tr"]).toContain(LANGUAGE_FLAG[language]);
    }
  });

  it("shows the language's own name, not a translation", () => {
    expect(languageName("en")).toBe("English");
    expect(languageName("tr")).toBe("Türkçe");
  });

  it("uses the UK flag for English and the Turkish flag for Turkish", () => {
    expect(LANGUAGE_FLAG).toEqual({ en: "gb", tr: "tr" });
  });
});
