// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  addRecent,
  loadRecent,
  parseRecent,
  RECENT_KEY,
  RECENT_MAX,
  removeRecent,
  saveRecent,
  type RecentFile,
} from "./recent";

function entry(n: number): RecentFile {
  return { path: `C:\\saves\\file${n}.sav`, fileName: `file${n}.sav`, openedAt: n };
}

describe("addRecent", () => {
  it("puts the newest file first", () => {
    const list = addRecent([entry(1)], entry(2));
    expect(list.map((item) => item.openedAt)).toEqual([2, 1]);
  });

  it("caps the list at 5 files and drops the oldest", () => {
    let list: RecentFile[] = [];
    for (let n = 1; n <= 8; n += 1) list = addRecent(list, entry(n));
    expect(RECENT_MAX).toBe(5);
    expect(list).toHaveLength(5);
    expect(list.map((item) => item.openedAt)).toEqual([8, 7, 6, 5, 4]);
  });

  it("moves an already listed path to the front instead of duplicating it", () => {
    const list = addRecent([entry(3), entry(2), entry(1)], { ...entry(1), openedAt: 99 });
    expect(list.map((item) => item.openedAt)).toEqual([99, 3, 2]);
  });
});

describe("removeRecent", () => {
  it("removes by path", () => {
    expect(removeRecent([entry(1), entry(2)], entry(1).path)).toEqual([entry(2)]);
  });
});

describe("parseRecent", () => {
  it("gives an empty list for garbage", () => {
    expect(parseRecent(null)).toEqual([]);
    expect(parseRecent("nope")).toEqual([]);
    expect(parseRecent("{}")).toEqual([]);
  });

  it("drops invalid entries and duplicates, and caps the list", () => {
    const raw = JSON.stringify([
      entry(1),
      { path: 5 },
      null,
      entry(1),
      entry(2),
      entry(3),
      entry(4),
      entry(5),
      entry(6),
    ]);
    const list = parseRecent(raw);
    expect(list).toHaveLength(5);
    expect(list.map((item) => item.openedAt)).toEqual([1, 2, 3, 4, 5]);
  });
});

describe("loadRecent and saveRecent", () => {
  it("round-trips and survives broken storage", () => {
    const data = new Map<string, string>();
    const storage = {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => void data.set(key, value),
    };
    expect(saveRecent([entry(1), entry(2)], storage)).toBe(true);
    expect(data.has(RECENT_KEY)).toBe(true);
    expect(loadRecent(storage)).toEqual([entry(1), entry(2)]);

    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    expect(loadRecent(broken)).toEqual([]);
    expect(saveRecent([entry(1)], broken)).toBe(false);
    expect(loadRecent(null)).toEqual([]);
  });
});
