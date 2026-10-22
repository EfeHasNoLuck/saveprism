import { describe, expect, it } from "vitest";
import {
  checkRange,
  clampPage,
  filterEntries,
  hintLine,
  pageCount,
  pageItems,
  pagerLabel,
  stepValue,
} from "./helpers";

const base = {
  hint: "Whole number · 0 to 99,999,999",
  wasText: "was 5",
  changed: false,
  focused: false,
  always: false,
  invalid: false,
};

describe("hintLine", () => {
  it("is hidden when idle", () => {
    expect(hintLine(base)).toBeNull();
  });
  it("shows the hint when focused", () => {
    expect(hintLine({ ...base, focused: true })).toEqual({ text: base.hint, error: false });
  });
  it("adds the original when changed", () => {
    expect(hintLine({ ...base, changed: true })).toEqual({
      text: `${base.hint} · was 5`,
      error: false,
    });
  });
  it("always shows for Gold", () => {
    expect(hintLine({ ...base, always: true })?.text).toBe(base.hint);
  });
  it("uses the error tone when invalid, even if idle", () => {
    expect(hintLine({ ...base, invalid: true })).toEqual({ text: base.hint, error: true });
  });
});

describe("checkRange", () => {
  it("accepts a value inside the limits", () => {
    expect(checkRange("int", "100", 0, 99999999)).toBeNull();
  });
  it("rejects above max and below min", () => {
    expect(checkRange("int", "100000000", 0, 99999999)).toBe("aboveMax");
    expect(checkRange("int", "-1", 0, 99999999)).toBe("belowMin");
  });
  it("rejects non-whole input for int", () => {
    expect(checkRange("int", "1.5", null, null)).toBe("notWhole");
    expect(checkRange("int", "1e3", null, null)).toBe("notWhole");
  });
  it("rejects text and empty input", () => {
    expect(checkRange("int", "abc", null, null)).toBe("notNumber");
    expect(checkRange("int", "0x10", null, null)).toBe("notNumber");
    expect(checkRange("float", "  ", null, null)).toBe("empty");
  });
  it("accepts decimals for float", () => {
    expect(checkRange("float", "1.5", 0, 2)).toBeNull();
    expect(checkRange("float", "2.5", 0, 2)).toBe("aboveMax");
  });
  it("does not check bool or string", () => {
    expect(checkRange("string", "anything", 0, 1)).toBeNull();
    expect(checkRange("bool", "true", null, null)).toBeNull();
  });
});

describe("stepValue", () => {
  it("steps and clamps", () => {
    expect(stepValue("5", 1, 0, 10)).toBe("6");
    expect(stepValue("10", 1, 0, 10)).toBe("10");
    expect(stepValue("0", -1, 0, 10)).toBe("0");
  });
  it("returns null for non-numbers", () => {
    expect(stepValue("abc", 1, null, null)).toBeNull();
  });
});

describe("filterEntries", () => {
  const entries = [
    { id: 1, name: "Potion", field: { value: "5" } },
    { id: 12, name: null, field: { value: "99" } },
    { id: 3, name: "Elixir", field: { value: "0" } },
  ];
  it("returns all for an empty query", () => {
    expect(filterEntries(entries, " ")).toHaveLength(3);
  });
  it("matches by name, case-insensitive", () => {
    expect(filterEntries(entries, "poti").map((e) => e.id)).toEqual([1]);
  });
  it("matches by #id", () => {
    expect(filterEntries(entries, "#12").map((e) => e.id)).toEqual([12]);
  });
  it("matches by value", () => {
    expect(filterEntries(entries, "99").map((e) => e.id)).toEqual([12]);
  });
});

describe("pager", () => {
  it("counts pages", () => {
    expect(pageCount(0, 50)).toBe(1);
    expect(pageCount(50, 50)).toBe(1);
    expect(pageCount(101, 50)).toBe(3);
  });
  it("clamps the page", () => {
    expect(clampPage(9, 101, 50)).toBe(2);
    expect(clampPage(-1, 101, 50)).toBe(0);
  });
  it("slices a page", () => {
    const items = Array.from({ length: 120 }, (_, i) => i);
    expect(pageItems(items, 2, 50)).toHaveLength(20);
    expect(pageItems(items, 0, 50)[0]).toBe(0);
  });
  it("labels", () => {
    expect(pagerLabel(0, 101, 50)).toBe("1 / 3");
  });
});
