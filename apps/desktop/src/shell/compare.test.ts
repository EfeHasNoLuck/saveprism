import { describe, expect, it } from "vitest";
import type { CompareRow } from "@/api/commands";
import {
  filterCompareRows,
  kindBadgeClasses,
  pathAncestors,
  pickRevealTarget,
  rowIsHere,
  valueCell,
} from "./compare";

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

  it("matches the name a row shows", () => {
    const labels = new Map([["/items/1", "Item #1 Potion"]]);
    expect(filterCompareRows(ROWS, "potion", labels).map((row) => row.path)).toEqual(["/items/1"]);
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

  it("gives each one-sided badge its own chip tones", () => {
    expect(kindBadgeClasses("different")).toBeNull();
    expect(kindBadgeClasses("onlyHere")).toContain("bg-surface-highest");
    expect(kindBadgeClasses("onlyThere")).toContain("bg-changed-tag");
    expect(kindBadgeClasses("onlyHere")).not.toBe(kindBadgeClasses("onlyThere"));
  });
});

describe("reveal helpers", () => {
  it("lists the shown ancestors nearest first", () => {
    expect(pathAncestors("/actors/0/name")).toEqual(["/actors/0", "/actors"]);
    expect(pathAncestors("/actors")).toEqual([]);
    expect(pathAncestors("/")).toEqual([]);
    expect(pathAncestors("")).toEqual([]);
  });

  it("keeps an exact row when it is shown", () => {
    expect(pickRevealTarget("/actors", ["/actors"])).toBe("/actors");
  });

  it("falls back to the nearest shown ancestor of a whole container", () => {
    expect(pickRevealTarget("/actors/0/name", ["/actors", "/actors/1"])).toBe("/actors");
    expect(pickRevealTarget("/actors/0/name", ["/actors/0", "/actors"])).toBe("/actors/0");
  });

  it("falls back to the first shown descendant when no ancestor is shown", () => {
    expect(pickRevealTarget("/actors", ["/actors/1/name", "/actors/0/name"])).toBe(
      "/actors/1/name",
    );
    expect(pickRevealTarget("/actors", ["/other", "/actors/2/level"])).toBe("/actors/2/level");
  });

  it("returns null when nothing on the way is shown yet", () => {
    expect(pickRevealTarget("/actors", ["/gold"])).toBeNull();
    expect(pickRevealTarget("/actor", ["/actors"])).toBeNull();
  });
});
