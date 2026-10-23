// @vitest-environment node
import { describe, expect, it } from "vitest";
import { baseName, dirName, engineLabel, exportFileName } from "./engine";
import { capitalize, describeError } from "./errors";

describe("describeError", () => {
  it("splits a host error into reason and fix", () => {
    const parts = describeError({
      code: "SP-APP-002",
      message: "no file is open",
      hint: "Open a save file first.",
    });
    expect(parts.reason).toBe("No file is open (SP-APP-002).");
    expect(parts.fix).toBe("Open a save file first.");
  });

  it("shows other values as plain text", () => {
    expect(describeError(new Error("boom")).reason).toBe("Boom.");
    expect(describeError("plain").fix).toBe("");
    expect(describeError(undefined).reason).toBe("Undefined.");
  });

  it("capitalizes only the first letter", () => {
    expect(capitalize("abc def")).toBe("Abc def");
    expect(capitalize("")).toBe("");
  });
});

describe("engine helpers", () => {
  it("names the engine from the extension first, then the codec", () => {
    expect(engineLabel("file1.rmmzsave", "format.rpgmaker-json")).toBe("RPG Maker MZ");
    expect(engineLabel("file1.rpgsave", "format.rpgmaker-json")).toBe("RPG Maker MV");
    expect(engineLabel("slot.sav", "format.gvas")).toBe("Unreal");
    expect(engineLabel("Save1.rxdata", "format.ruby-marshal")).toBe("RPG Maker XP");
    expect(engineLabel("Save1.rvdata", "format.ruby-marshal")).toBe("RPG Maker VX");
    expect(engineLabel("Save01.rvdata2", "format.ruby-marshal")).toBe("RPG Maker VX Ace");
    expect(engineLabel("data.bin", "format.ruby-marshal")).toBe("Ruby Marshal");
    expect(engineLabel("data.bin", "format.json")).toBe("JSON");
    expect(engineLabel("data.bin", "format.unknown")).toBeNull();
    expect(engineLabel("slot1.save", "format.json", "sugarcube")).toBe("SugarCube");
    expect(engineLabel("file1.rpgsave", "format.json", null)).toBe("RPG Maker MV");
  });

  it("splits paths with either separator", () => {
    expect(baseName("C:\\saves\\a.sav")).toBe("a.sav");
    expect(baseName("/home/u/a.sav")).toBe("a.sav");
    expect(dirName("C:\\saves\\a.sav")).toBe("C:\\saves");
    expect(dirName("a.sav")).toBe("");
  });

  it("names the .txt export after the save's stem", () => {
    expect(exportFileName("file1.rmmzsave")).toBe("file1-export.json");
    expect(exportFileName("slot.v2.es3")).toBe("slot.v2-export.json");
    expect(exportFileName("SaveGame")).toBe("SaveGame-export.json");
    expect(exportFileName(".hidden")).toBe(".hidden-export.json");
    expect(exportFileName("compact.json")).toBe("compact-export.json");
  });
});
