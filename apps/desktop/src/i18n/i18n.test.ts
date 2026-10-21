// @vitest-environment node
import { describe, expect, it } from "vitest";
import en from "../../locales/en/common.json";
import tr from "../../locales/tr/common.json";
import { detectLanguage, STORAGE_KEY } from "./detect";

function flatten(obj: Record<string, unknown>, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(obj)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object") {
      Object.assign(out, flatten(value as Record<string, unknown>, full));
    } else {
      out[full] = String(value);
    }
  }
  return out;
}

const enFlat = flatten(en as Record<string, unknown>);
const trFlat = flatten(tr as Record<string, unknown>);

const REQUIRED_KEYS = [
  "empty.title",
  "empty.subtitle",
  "footer.core",
  "footer.coreUnavailable",
  "language.label",
  "theme.toggle",
  "theme.light",
  "theme.dark",
  "theme.system",
];

describe("locale resources", () => {
  it("en and tr have the same flattened key set", () => {
    expect(Object.keys(trFlat).sort()).toEqual(Object.keys(enFlat).sort());
  });

  it("no value is empty in either locale", () => {
    for (const flat of [enFlat, trFlat]) {
      for (const [key, value] of Object.entries(flat)) {
        expect(value.trim(), `empty value for ${key}`).not.toBe("");
      }
    }
  });

  it("contains every key the UI relies on", () => {
    for (const key of REQUIRED_KEYS) {
      expect(enFlat, `en missing ${key}`).toHaveProperty(key);
      expect(trFlat, `tr missing ${key}`).toHaveProperty(key);
    }
  });

  it("keeps the interpolation placeholder in footer.core", () => {
    expect(enFlat["footer.core"]).toContain("{{version}}");
    expect(trFlat["footer.core"]).toContain("{{version}}");
  });
});

describe("detectLanguage", () => {
  it("uses a valid stored value first", () => {
    expect(detectLanguage({ stored: "tr", navigatorLanguage: "en-US" })).toBe("tr");
    expect(detectLanguage({ stored: "en", navigatorLanguage: "tr-TR" })).toBe("en");
  });

  it("ignores an invalid stored value and falls back to navigator", () => {
    expect(detectLanguage({ stored: "de", navigatorLanguage: "tr-TR" })).toBe("tr");
    expect(detectLanguage({ stored: "xx", navigatorLanguage: "en-US" })).toBe("en");
  });

  it("treats a missing or empty stored value as absent", () => {
    expect(detectLanguage({ stored: null, navigatorLanguage: "tr-TR" })).toBe("tr");
    expect(detectLanguage({ stored: undefined, navigatorLanguage: "tr" })).toBe("tr");
    expect(detectLanguage({ stored: "", navigatorLanguage: "tr-TR" })).toBe("tr");
  });

  it("maps Turkish navigator tags to tr", () => {
    expect(detectLanguage({ stored: null, navigatorLanguage: "tr-TR" })).toBe("tr");
    expect(detectLanguage({ stored: null, navigatorLanguage: "tr" })).toBe("tr");
  });

  it("maps every other navigator language to en", () => {
    expect(detectLanguage({ stored: null, navigatorLanguage: "en-US" })).toBe("en");
    expect(detectLanguage({ stored: null, navigatorLanguage: "de" })).toBe("en");
    expect(detectLanguage({ stored: null, navigatorLanguage: "" })).toBe("en");
    expect(detectLanguage({ stored: null, navigatorLanguage: undefined })).toBe("en");
    expect(detectLanguage({ stored: null, navigatorLanguage: null })).toBe("en");
  });

  it("uses the saveprism.lang storage key", () => {
    expect(STORAGE_KEY).toBe("saveprism.lang");
  });
});
