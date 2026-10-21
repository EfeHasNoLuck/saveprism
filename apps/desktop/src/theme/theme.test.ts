import { describe, expect, it } from "vitest";
import { parseStoredPreference, resolveTheme, STORAGE_KEY } from "./theme";

describe("STORAGE_KEY", () => {
  it("is the saveprism.theme key", () => {
    expect(STORAGE_KEY).toBe("saveprism.theme");
  });
});

describe("parseStoredPreference", () => {
  it("accepts the three valid values", () => {
    expect(parseStoredPreference("light")).toBe("light");
    expect(parseStoredPreference("dark")).toBe("dark");
    expect(parseStoredPreference("system")).toBe("system");
  });

  it("falls back to system for null", () => {
    expect(parseStoredPreference(null)).toBe("system");
  });

  it("falls back to system for garbage text", () => {
    expect(parseStoredPreference("")).toBe("system");
    expect(parseStoredPreference("LIGHT")).toBe("system");
    expect(parseStoredPreference(" dark ")).toBe("system");
    expect(parseStoredPreference("{\"theme\":\"dark\"}")).toBe("system");
  });
});

describe("resolveTheme", () => {
  it("light preference is light regardless of OS", () => {
    expect(resolveTheme("light", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("dark preference is dark regardless of OS", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("dark", true)).toBe("dark");
  });

  it("system preference follows the OS setting", () => {
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
  });
});
