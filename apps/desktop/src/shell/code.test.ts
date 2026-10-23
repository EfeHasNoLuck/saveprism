// @vitest-environment node
import { describe, expect, it } from "vitest";
import { copyCodePlace, PASTED_SAVE_NAME, pastedText, saveAsDefaultPath } from "./code";
import { engineLabel } from "./engine";

describe("save code helpers", () => {
  it("starts Save as at the open file, or at a plain name for a pasted code", () => {
    expect(saveAsDefaultPath({ path: "C:\\saves\\file1.rpgsave", pasted: false })).toBe(
      "C:\\saves\\file1.rpgsave",
    );
    expect(
      saveAsDefaultPath({ path: "C:\\AppData\\SavePrism\\pasted\\code-1.txt", pasted: true }),
    ).toBe(PASTED_SAVE_NAME);
  });

  it("offers Copy code as the main action, a menu item or not at all", () => {
    expect(copyCodePlace({ pasted: true, textCode: true })).toBe("main");
    expect(copyCodePlace({ pasted: false, textCode: true })).toBe("menu");
    expect(copyCodePlace({ pasted: false, textCode: false })).toBe("none");
  });

  it("takes the plain text of a paste unless it is blank", () => {
    const data = (text: string) => ({ getData: (kind: string) => (kind === "text/plain" ? text : "") });
    expect(pastedText(data("  N4Ig...==\n"))).toBe("  N4Ig...==\n");
    expect(pastedText(data(" \r\n\t"))).toBeNull();
    expect(pastedText(data(""))).toBeNull();
    expect(pastedText(null)).toBeNull();
  });

  it("names the engine of a pasted RPG Maker code", () => {
    expect(engineLabel("code-1.txt", "format.json", "rpgmaker-mv")).toBe("RPG Maker MV");
    expect(engineLabel("code-1.txt", "format.json", "rpgmaker-mz")).toBe("RPG Maker MZ");
    expect(engineLabel("code-1.txt", "format.json", null)).toBe("JSON");
  });
});
