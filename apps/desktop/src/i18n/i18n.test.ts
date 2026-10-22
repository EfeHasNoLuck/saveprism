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
  it("uses a valid stored value", () => {
    expect(detectLanguage({ stored: "tr" })).toBe("tr");
    expect(detectLanguage({ stored: "en" })).toBe("en");
  });

  it("starts in English without a valid stored value", () => {
    expect(detectLanguage({ stored: "de" })).toBe("en");
    expect(detectLanguage({ stored: "" })).toBe("en");
    expect(detectLanguage({ stored: null })).toBe("en");
    expect(detectLanguage({ stored: undefined })).toBe("en");
  });

  it("uses the saveprism.lang storage key", () => {
    expect(STORAGE_KEY).toBe("saveprism.lang");
  });
});
