// @vitest-environment node
import { describe, expect, it } from "vitest";
import { ageParts, ageText, dateTimeText, sizeText } from "./backups";

const NOW = Date.UTC(2026, 9, 23, 14, 0, 0);
const MINUTE = 60_000;

describe("ageParts", () => {
  it("uses the largest whole unit", () => {
    expect(ageParts(NOW - 42_000, NOW)).toEqual({ value: 42, unit: "second" });
    expect(ageParts(NOW - 5 * MINUTE - 59_000, NOW)).toEqual({ value: 5, unit: "minute" });
    expect(ageParts(NOW - 3 * 60 * MINUTE, NOW)).toEqual({ value: 3, unit: "hour" });
    expect(ageParts(NOW - 2 * 24 * 60 * MINUTE, NOW)).toEqual({ value: 2, unit: "day" });
  });
  it("treats a time in the future as now", () => {
    expect(ageParts(NOW + 5000, NOW)).toEqual({ value: 0, unit: "second" });
  });
});

describe("ageText", () => {
  it("reads naturally in English and Turkish", () => {
    expect(ageText(NOW - 5 * MINUTE, NOW, "en")).toBe("5 minutes ago");
    expect(ageText(NOW - 24 * 60 * MINUTE, NOW, "en")).toBe("yesterday");
    expect(ageText(NOW, NOW, "en")).toBe("now");
    expect(ageText(NOW - 5 * MINUTE, NOW, "tr")).toBe("5 dakika önce");
  });
});

describe("dateTimeText", () => {
  it("has the date and the time with seconds", () => {
    const text = dateTimeText(NOW, "en");
    expect(text).toContain("2026");
    expect(text).toMatch(/\d{1,2}:\d{2}:\d{2}/);
  });
});

describe("sizeText", () => {
  it("picks bytes, KB or MB", () => {
    expect(sizeText(512, "en")).toBe("512 B");
    expect(sizeText(12_595, "en")).toBe("12.3 KB");
    expect(sizeText(1_258_291, "en")).toBe("1.2 MB");
  });
  it("uses the decimal mark of the language", () => {
    expect(sizeText(12_595, "tr")).toBe("12,3 KB");
  });
});
