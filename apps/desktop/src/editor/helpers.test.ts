import { describe, expect, it } from "vitest";
import {
  allocateColumns,
  bulkPaths,
  bulkTargets,
  cardWidth,
  catalogId,
  changeText,
  changeValue,
  checkRange,
  clampPage,
  columnsIn,
  countChangeKinds,
  fieldHint,
  filterCatalog,
  filterEntries,
  formatDateTime,
  formatValue,
  hintLine,
  noteText,
  pageCount,
  pageItems,
  pagerLabel,
  sameValue,
  stepValue,
} from "./helpers";

const base = {
  hint: "Whole number · 0 to 99,999,999",
  focused: false,
  always: false,
  invalid: false,
  change: null,
};

describe("hintLine", () => {
  it("is hidden when idle", () => {
    expect(hintLine(base)).toBeNull();
  });
  it("shows the hint when focused", () => {
    expect(hintLine({ ...base, focused: true })).toEqual({
      text: base.hint,
      error: false,
      change: false,
    });
  });
  it("always shows for Gold", () => {
    expect(hintLine({ ...base, always: true })?.text).toBe(base.hint);
  });
  it("uses the error tone when invalid, even if idle", () => {
    expect(hintLine({ ...base, invalid: true })).toEqual({
      text: base.hint,
      error: true,
      change: false,
    });
  });
  it("shows old → new for a changed value, also in place of Gold's hint", () => {
    const change = "2,500 → 2,601";
    expect(hintLine({ ...base, change })).toEqual({ text: change, error: false, change: true });
    expect(hintLine({ ...base, always: true, change })?.text).toBe(change);
  });
  it("shows the hint on focus and the error over old → new", () => {
    const change = "2,500 → 2,601";
    expect(hintLine({ ...base, focused: true, change })?.text).toBe(base.hint);
    expect(hintLine({ ...base, invalid: true, change })).toEqual({
      text: base.hint,
      error: true,
      change: false,
    });
  });
});

describe("changeText", () => {
  const en = (n: number) => n.toLocaleString("en");
  const tr = (n: number) => n.toLocaleString("tr");
  it("groups whole numbers in the UI language", () => {
    expect(changeText("int", "2500", "2601", en)).toBe("2,500 → 2,601");
    expect(changeText("int", "10000", "-5", tr)).toBe("10.000 → -5");
  });
  it("keeps true/false and decimals as they are", () => {
    expect(changeText("bool", "false", "true", en)).toBe("false → true");
    expect(changeText("float", "100.5", "7.25", tr)).toBe("100.5 → 7.25");
  });
  it("quotes text and cuts long text", () => {
    expect(changeText("string", "Ann", "Ada", en)).toBe('"Ann" → "Ada"');
    expect(changeValue("string", "a".repeat(30), en)).toBe(`"${"a".repeat(23)}…"`);
    expect(changeValue("string", "a".repeat(30), en, Infinity)).toBe(`"${"a".repeat(30)}"`);
    expect(changeValue("string", "", en)).toBe('""');
  });
});

describe("fieldHint", () => {
  const t = (key: string, o?: Record<string, unknown>) =>
    o ? `${key}(${Object.values(o).join(",")})` : key;
  const format = (n: number) => n.toLocaleString("en");

  it("names the type and both limits", () => {
    expect(fieldHint("int", 0, 99999999, t, format)).toBe(
      "type.int · range.between(0,99,999,999)",
    );
  });
  it("shows one side when the other is open", () => {
    expect(fieldHint("int", 0, null, t, format)).toBe("type.int · range.atLeast(0)");
    expect(fieldHint("float", null, 99, t, format)).toBe("type.float · range.atMost(99)");
  });
  it("treats a clamped 64-bit limit as no limit", () => {
    const safe = Number.MAX_SAFE_INTEGER;
    expect(fieldHint("int", -safe, safe, t, format)).toBe("type.int");
    expect(fieldHint("int", 0, safe, t, format)).toBe("type.int · range.atLeast(0)");
  });
  it("has no range for true/false and text", () => {
    expect(fieldHint("bool", 0, 1, t, format)).toBe("type.bool");
    expect(fieldHint("string", null, null, t, format)).toBe("type.string");
  });
});

describe("formatValue", () => {
  const format = (n: number) => n.toLocaleString("tr");
  it("groups whole numbers in the UI language", () => {
    expect(formatValue("10000", format)).toBe("10.000");
    expect(formatValue("-2147483648", format)).toBe("-2.147.483.648");
  });
  it("leaves other text alone", () => {
    expect(formatValue("1.5", format)).toBe("1.5");
    expect(formatValue("abc", format)).toBe("abc");
    expect(formatValue("99999999999999999999", format)).toBe("99999999999999999999");
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

describe("set all", () => {
  const field = (path: string, kind: "int" | "float" | "bool" | "string", value: string, editable = true) => ({
    path,
    kind,
    value,
    editable,
  });
  const fields = [
    field("/a", "int", "5"),
    field("/b", "float", "1.5"),
    field("/c", "string", "Ann"),
    field("/d", "bool", "true"),
    field("/e", "int", "7", false),
  ];
  it("takes the editable numbers for a number and skips text", () => {
    expect(bulkTargets(fields, "number").map((f) => f.path)).toEqual(["/a", "/b"]);
  });
  it("takes the editable true/false values for switches", () => {
    expect(bulkTargets(fields, "bool").map((f) => f.path)).toEqual(["/d"]);
  });
  it("compares numbers by value and true/false without case", () => {
    expect(sameValue("42", "42.0")).toBe(true);
    expect(sameValue("42", " 42 ")).toBe(true);
    expect(sameValue("42", "43")).toBe(false);
    expect(sameValue("True", "true")).toBe(true);
    expect(sameValue("false", "true")).toBe(false);
  });
  it("sends only the values that would change", () => {
    const targets = [field("/a", "int", "42"), field("/b", "int", "3"), field("/c", "float", "42.0")];
    expect(bulkPaths(targets, "42")).toEqual(["/b"]);
    expect(bulkPaths(targets, "0")).toEqual(["/a", "/b", "/c"]);
    expect(bulkPaths([], "1")).toEqual([]);
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

describe("entry columns", () => {
  it("fits whole columns, at least one", () => {
    expect(columnsIn(0)).toBe(1);
    expect(columnsIn(475)).toBe(1);
    expect(columnsIn(476)).toBe(2);
  });
  it("gives the spare columns of a row to the longest lists", () => {
    // 990 px row: Items with 25 entries, Weapons and Armors empty.
    expect(allocateColumns(990, [25, 0, 0])).toEqual([2, 0, 0]);
    // Variables 56, Switches 70: each gets one, then the tallest takes the next.
    expect(allocateColumns(990, [56, 70])).toEqual([1, 2]);
    expect(allocateColumns(1246, [56, 70])).toEqual([2, 2]);
  });
  it("never gives a list more columns than entries", () => {
    expect(allocateColumns(3000, [3, 0])).toEqual([3, 0]);
  });
  it("keeps one column per list when the row is too narrow", () => {
    expect(allocateColumns(0, [5, 5])).toEqual([1, 1]);
    expect(allocateColumns(800, [0, 0])).toEqual([0, 0]);
  });
  it("sizes a card for its columns", () => {
    expect(cardWidth(0)).toBe(192);
    expect(cardWidth(2)).toBe(2 * 232 + 12 + 26);
  });
});

describe("story notes", () => {
  const t = (key: string, options?: Record<string, unknown>) =>
    options ? `${key} ${JSON.stringify(options)}` : key;
  const format = (n: number) => n.toLocaleString("en");

  it("counts the entries of a Map and a Set", () => {
    expect(noteText({ kind: "map", count: 1200, text: null }, t, format, "en")).toEqual({
      text: 'note.map {"count":1200,"shown":"1,200"}',
      tip: "note.specialTip",
    });
    expect(noteText({ kind: "set", count: null, text: null }, t, format, "en").text).toBe(
      'note.set {"count":0,"shown":"0"}',
    );
  });

  it("shows a Date in the UI language and keeps text it cannot read", () => {
    const iso = "2025-10-09T12:00:00.000Z";
    const shown = formatDateTime(Date.parse(iso), "en");
    expect(shown).toMatch(/2025/);
    expect(noteText({ kind: "date", count: null, text: iso }, t, format, "en").text).toBe(
      `note.date ${JSON.stringify({ date: shown })}`,
    );
    expect(noteText({ kind: "date", count: null, text: "soon" }, t, format, "en").text).toBe(
      'note.date {"date":"soon"}',
    );
    expect(formatDateTime(Number.NaN, "en")).toBeNull();
    expect(formatDateTime(1e20, "en")).toBeNull();
  });

  it("explains values changed during play, code values and empty values", () => {
    expect(noteText({ kind: "changed", count: null, text: null }, t, format, "en")).toEqual({
      text: "note.changed",
      tip: "note.changedTip",
    });
    expect(noteText({ kind: "code", count: null, text: null }, t, format, "en").tip).toBe("note.specialTip");
    expect(noteText({ kind: "empty", count: null, text: null }, t, format, "en").tip).toBe("note.emptyTip");
  });
});

describe("catalog", () => {
  const entries = [
    { id: 1, name: "Potion" },
    { id: 3, name: "Hi-Potion" },
    { id: 12, name: "Ether" },
  ];
  it("matches names and #ids", () => {
    expect(filterCatalog(entries, "poti").map((e) => e.id)).toEqual([1, 3]);
    expect(filterCatalog(entries, "#12").map((e) => e.id)).toEqual([12]);
    expect(filterCatalog(entries, "12").map((e) => e.id)).toEqual([12]);
    expect(filterCatalog(entries, " ")).toHaveLength(3);
  });
  it("parses typed ids", () => {
    expect(catalogId("12")).toBe(12);
    expect(catalogId("#12")).toBe(12);
    expect(catalogId("0")).toBeNull();
    expect(catalogId("100000")).toBeNull();
    expect(catalogId("Potion")).toBeNull();
  });
});

describe("change counts", () => {
  it("counts added and removed entries only", () => {
    expect(
      countChangeKinds([
        { kind: "set" },
        { kind: "added" },
        { kind: "removed" },
        { kind: "added" },
      ]),
    ).toEqual({ added: 2, removed: 1 });
    expect(countChangeKinds([])).toEqual({ added: 0, removed: 0 });
  });
});
