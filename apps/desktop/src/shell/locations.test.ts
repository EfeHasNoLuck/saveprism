// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  engineGroup,
  engineInitials,
  filterLocations,
  groupRank,
  hasPathPlaceholder,
  highlightText,
  isRegistryPath,
  orderLocations,
  tokenizePath,
  translatePathPlaceholders,
} from "./locations";

const TR = { gameFolder: "Oyun klasörü", game: "Oyun", company: "Şirket" };
const EN = { gameFolder: "game folder", game: "game", company: "company" };

describe("translatePathPlaceholders", () => {
  it("translates every placeholder and keeps the angle brackets", () => {
    expect(
      translatePathPlaceholders("%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>", TR),
    ).toBe("%USERPROFILE%\\AppData\\LocalLow\\<Şirket>\\<Oyun>");
    expect(translatePathPlaceholders("<game folder>\\www\\save", TR)).toBe(
      "<Oyun klasörü>\\www\\save",
    );
  });

  it("uses the longer placeholder first so <game folder> is not split at <game>", () => {
    expect(translatePathPlaceholders("<game folder>\\save", EN)).toBe("<game folder>\\save");
  });

  it("keeps a path without placeholders and an empty label as it is", () => {
    expect(translatePathPlaceholders("%APPDATA%\\RenPy\\<game>", TR)).toBe(
      "%APPDATA%\\RenPy\\<Oyun>",
    );
    expect(translatePathPlaceholders("C:\\saves", TR)).toBe("C:\\saves");
    expect(
      translatePathPlaceholders("<game>", { gameFolder: "", game: " ", company: "" }),
    ).toBe("<game>");
  });
});

describe("hasPathPlaceholder", () => {
  it("recognises every placeholder the host sends", () => {
    expect(hasPathPlaceholder("<game folder>\\save")).toBe(true);
    expect(hasPathPlaceholder("%APPDATA%\\RenPy\\<game>")).toBe(true);
    expect(hasPathPlaceholder("%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>")).toBe(true);
  });

  it("is false for a real folder path", () => {
    expect(hasPathPlaceholder("%APPDATA%\\RenPy\\saves")).toBe(false);
    expect(hasPathPlaceholder("C:\\saves")).toBe(false);
  });
});

describe("filterLocations", () => {
  const ITEMS = [
    {
      engine: "unity",
      name: "Unity",
      hint: "UnityPlayer.dll and a <game>_Data folder.",
      paths: [
        "%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>",
        "HKEY_CURRENT_USER\\Software\\<company>\\<game>",
      ],
    },
    {
      engine: "godot",
      name: "Godot",
      hint: "One .exe with a .pck file.",
      paths: ["%APPDATA%\\Godot\\app_userdata\\<game>"],
    },
    {
      engine: "witcher3",
      name: "The Witcher 3",
      hint: "Saves end in .sav in Documents.",
      paths: ["%USERPROFILE%\\Documents\\The Witcher 3\\gamesaves"],
    },
  ];

  it("matches the engine name, the hint and the path", () => {
    expect(filterLocations(ITEMS, "unity", "all").map((item) => item.engine)).toEqual(["unity"]);
    expect(filterLocations(ITEMS, "pck", "all").map((item) => item.engine)).toEqual(["godot"]);
    expect(filterLocations(ITEMS, "gamesaves", "all").map((item) => item.engine)).toEqual([
      "witcher3",
    ]);
  });

  it("is case-insensitive and ignores surrounding spaces", () => {
    expect(filterLocations(ITEMS, "  WITCHER ", "all").map((item) => item.engine)).toEqual([
      "witcher3",
    ]);
  });

  it("keeps only the chosen chip's engines", () => {
    expect(filterLocations(ITEMS, "", "godot").map((item) => item.engine)).toEqual(["godot"]);
    expect(filterLocations(ITEMS, "", "other").map((item) => item.engine)).toEqual(["witcher3"]);
  });

  it("returns nothing when no engine matches", () => {
    expect(filterLocations(ITEMS, "renpy", "all")).toEqual([]);
  });
});

describe("orderLocations", () => {
  const ITEMS = [
    { engine: "construct" },
    { engine: "unity" },
    { engine: "witcher3" },
    { engine: "godot" },
    { engine: "unreal" },
    { engine: "rpgmaker-mv" },
    { engine: "rpgmaker-classic" },
    { engine: "renpy" },
    { engine: "gamemaker" },
    { engine: "twine" },
  ];

  it("follows the chip order and keeps the host order inside a group", () => {
    expect(orderLocations(ITEMS).map((item) => item.engine)).toEqual([
      "unity",
      "unreal",
      "rpgmaker-mv",
      "rpgmaker-classic",
      "renpy",
      "godot",
      "gamemaker",
      "construct",
      "witcher3",
      "twine",
    ]);
  });

  it("ranks groups by the chip row, with other last", () => {
    expect(engineGroup("unity")).toBe("unity");
    expect(engineGroup("witcher3")).toBe("other");
    expect(groupRank("unity")).toBeLessThan(groupRank("gamemaker"));
    expect(groupRank("other")).toBe(groupRank("witcher3"));
  });

  it("keeps search results in the same grouped order", () => {
    const results = filterLocations(
      [
        { engine: "witcher3", name: "The Witcher 3", hint: "saves", paths: [] },
        { engine: "godot", name: "Godot", hint: "saves", paths: [] },
        { engine: "unity", name: "Unity", hint: "saves", paths: [] },
      ],
      "saves",
      "all",
    );
    expect(results.map((item) => item.engine)).toEqual(["unity", "godot", "witcher3"]);
  });
});

describe("engineInitials", () => {
  it("uses the short tiles of the dialog and derives the others", () => {
    expect(engineInitials("unity")).toBe("U");
    expect(engineInitials("unreal")).toBe("UE");
    expect(engineInitials("rpgmaker-mv")).toBe("MV");
    expect(engineInitials("rpgmaker-classic")).toBe("XP");
    expect(engineInitials("renpy")).toBe("R'");
    expect(engineInitials("gamemaker")).toBe("GM");
    expect(engineInitials("witcher3")).toBe("W");
    expect(engineInitials("neverwinter-nights")).toBe("NN");
  });
});

describe("highlightText", () => {
  it("splits the matches out and keeps their original case", () => {
    expect(highlightText("RPG Maker MV", "rpg")).toEqual([
      { text: "RPG", hit: true },
      { text: " Maker MV", hit: false },
    ]);
  });

  it("returns the whole text when the query is empty", () => {
    expect(highlightText("Unity", "  ")).toEqual([{ text: "Unity", hit: false }]);
  });

  it("treats special characters as plain text", () => {
    expect(highlightText("a .sav file", ".sav")).toEqual([
      { text: "a ", hit: false },
      { text: ".sav", hit: true },
      { text: " file", hit: false },
    ]);
    expect(highlightText("a (b) c", "(")).toEqual([
      { text: "a ", hit: false },
      { text: "(", hit: true },
      { text: "b) c", hit: false },
    ]);
  });
});

describe("tokenizePath", () => {
  it("marks environment variables and placeholders", () => {
    expect(tokenizePath("%USERPROFILE%\\AppData\\LocalLow\\<company>\\<game>")).toEqual([
      { kind: "env", text: "%USERPROFILE%" },
      { kind: "text", text: "\\AppData\\LocalLow\\" },
      { kind: "placeholder", text: "<company>" },
      { kind: "text", text: "\\" },
      { kind: "placeholder", text: "<game>" },
    ]);
  });

  it("marks a registry root as an environment-like key", () => {
    expect(tokenizePath("HKEY_CURRENT_USER\\Software\\<company>")).toEqual([
      { kind: "env", text: "HKEY_CURRENT_USER" },
      { kind: "text", text: "\\Software\\" },
      { kind: "placeholder", text: "<company>" },
    ]);
  });

  it("leaves a plain path as one text token", () => {
    expect(tokenizePath("C:\\saves")).toEqual([{ kind: "text", text: "C:\\saves" }]);
  });
});

describe("isRegistryPath", () => {
  it("recognises registry keys only", () => {
    expect(isRegistryPath("HKEY_CURRENT_USER\\Software")).toBe(true);
    expect(isRegistryPath("%APPDATA%\\EldenRing")).toBe(false);
    expect(isRegistryPath("<game folder>\\save")).toBe(false);
  });
});
