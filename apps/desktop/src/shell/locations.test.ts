// @vitest-environment node
import { describe, expect, it } from "vitest";
import { translatePathPlaceholders } from "./locations";

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
