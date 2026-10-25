import { describe, expect, it } from "vitest";
import type { CompareRow } from "@/api/commands";
import { filterCompareRows, rowIsHere, valueCell } from "./compare";

const ROWS: CompareRow[] = [
  { path: "/level", here: "7", there: "8", kind: "different" },
  { path: "/items/1", here: null, there: "ether", kind: "onlyThere" },
  { path: "/old", here: "true", there: null, kind: "onlyHere" },
];

describe("filterCompareRows", () => {
  it("keeps every row for an empty or blank query", () => {
    expect(filterCompareRows(ROWS, "")).toEqual(ROWS);
    expect(filterCompareRows(ROWS, "   ")).toEqual(ROWS);
    expect(filterCompareRows(ROWS, "")).not.toBe(ROWS);
  });

  it("matches the path, either value, case-insensitively", () => {
    expect(filterCompareRows(ROWS, "LEVEL").map((row) => row.path)).toEqual(["/level"]);
    expect(filterCompareRows(ROWS, "ether").map((row) => row.path)).toEqual(["/items/1"]);
    expect(filterCompareRows(ROWS, "TRUE").map((row) => row.path)).toEqual(["/old"]);
  });

  it("gives no rows when nothing matches", () => {
    expect(filterCompareRows(ROWS, "missing")).toEqual([]);
  });
});

describe("row helpers", () => {
  it("knows which side has the value", () => {
    expect(rowIsHere(ROWS[0]!)).toBe(true);
    expect(rowIsHere(ROWS[1]!)).toBe(false);
    expect(rowIsHere(ROWS[2]!)).toBe(true);
  });

  it("shows an empty cell for the missing side", () => {
    expect(valueCell(null)).toBe("");
    expect(valueCell("8")).toBe("8");
  });
});
