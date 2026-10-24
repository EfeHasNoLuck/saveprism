// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  cleanName,
  countNames,
  entryName,
  emptyNames,
  gameFolder,
  loadNames,
  MAX_NAME_CHARS,
  MAX_NAMES,
  nameFor,
  namesKey,
  parseNames,
  saveNames,
  setOwnName,
  type OwnNames,
} from "./names";

/** A simple in-memory storage. */
function fakeStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => {
      data.set(key, value);
    },
    data,
  };
}

const brokenRead = {
  getItem: () => {
    throw new Error("storage is blocked");
  },
};

const brokenWrite = {
  setItem: () => {
    throw new Error("storage is full");
  },
};

describe("namesKey and gameFolder", () => {
  it("keys names by the game folder", () => {
    expect(namesKey("C:\\Games\\Example")).toBe("saveprism.names.C:\\Games\\Example");
  });

  it("takes the folder of a Windows or Unix save path", () => {
    expect(gameFolder("C:\\Games\\Example\\save1.rpgsave")).toBe("C:\\Games\\Example");
    expect(gameFolder("/home/player/game/save1.rpgsave")).toBe("/home/player/game");
    expect(gameFolder("save1.rpgsave")).toBe("save1.rpgsave");
  });
});

describe("cleanName", () => {
  it("trims and cuts to the character limit", () => {
    expect(cleanName("  Ada  ")).toBe("Ada");
    expect(cleanName("x".repeat(MAX_NAME_CHARS + 20))).toHaveLength(MAX_NAME_CHARS);
    expect(cleanName("   ")).toBe("");
  });
});

describe("parseNames", () => {
  it("reads valid names of both lists", () => {
    const raw = JSON.stringify({
      variables: { "12": "Gold left", "3": "Steps" },
      switches: { "7": "Door open" },
    });
    expect(parseNames(raw)).toEqual({
      variables: { "12": "Gold left", "3": "Steps" },
      switches: { "7": "Door open" },
    });
  });

  it("ignores malformed JSON and shapes", () => {
    expect(parseNames("{not json")).toEqual(emptyNames());
    expect(parseNames("[]")).toEqual(emptyNames());
    expect(parseNames('"text"')).toEqual(emptyNames());
    expect(parseNames(JSON.stringify({ variables: { abc: "x", "1": 2, "2": " " } }))).toEqual(
      emptyNames(),
    );
  });

  it("keeps at most the limit", () => {
    const variables: Record<string, string> = {};
    for (let i = 1; i <= MAX_NAMES + 10; i += 1) variables[String(i)] = `n${i}`;
    const names = parseNames(JSON.stringify({ variables }));
    expect(countNames(names)).toBe(MAX_NAMES);
  });
});

describe("loadNames and saveNames", () => {
  it("round-trips through the same storage", () => {
    const storage = fakeStorage();
    const names = setOwnName(emptyNames(), "variables", 12, "Gold left");
    expect(saveNames(names, storage, "C:\\Games\\Example")).toBe(true);
    expect(storage.data.get(namesKey("C:\\Games\\Example"))).toContain("Gold left");
    expect(loadNames(storage, "C:\\Games\\Example")).toEqual(names);
  });

  it("never throws on broken storage", () => {
    expect(loadNames(brokenRead, "game")).toEqual(emptyNames());
    expect(saveNames(emptyNames(), brokenWrite, "game")).toBe(false);
    expect(loadNames(null, "game")).toEqual(emptyNames());
    expect(saveNames(emptyNames(), null, "game")).toBe(false);
  });

  it("gives no names without a game key", () => {
    const storage = fakeStorage();
    expect(loadNames(storage, null)).toEqual(emptyNames());
    expect(saveNames(emptyNames(), storage, null)).toBe(false);
  });
});

describe("setOwnName", () => {
  it("adds, replaces and removes a name", () => {
    let names = setOwnName(emptyNames(), "switches", 7, "Door open");
    expect(nameFor(names, "switches", 7)).toBe("Door open");
    names = setOwnName(names, "switches", 7, "Door closed");
    expect(nameFor(names, "switches", 7)).toBe("Door closed");
    names = setOwnName(names, "switches", 7, "   ");
    expect(nameFor(names, "switches", 7)).toBeNull();
  });

  it("does not change the input object", () => {
    const names = emptyNames();
    setOwnName(names, "variables", 1, "One");
    expect(names.variables).toEqual({});
  });

  it("refuses bad ids and a new name past the limit", () => {
    expect(setOwnName(emptyNames(), "variables", 0, "Zero")).toEqual(emptyNames());
    const names: OwnNames = { variables: {}, switches: {} };
    for (let i = 1; i <= MAX_NAMES; i += 1) names.variables[String(i)] = `n${i}`;
    expect(setOwnName(names, "variables", MAX_NAMES + 1, "One more")).toBe(names);
    expect(setOwnName(names, "variables", 1, "Changed").variables["1"]).toBe("Changed");
    expect(setOwnName(names, "variables", 1, "").variables["1"]).toBeUndefined();
  });
});

describe("entryName", () => {
  const fallback = (id: number) => `Variable #${id}`;

  it("keeps a name from the game", () => {
    expect(entryName({ id: 2, name: "Gold" }, "Own", fallback)).toBe("Gold");
  });

  it("shows the own name over the fallback", () => {
    expect(entryName({ id: 2, name: null }, "Gold left", fallback)).toBe("Gold left");
  });

  it("falls back to the numbered name", () => {
    expect(entryName({ id: 2, name: null }, null, fallback)).toBe("Variable #2");
  });
});
