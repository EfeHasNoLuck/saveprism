// @vitest-environment node
import { describe, expect, it } from "vitest";
import { mcpSetupJson } from "./setup";

describe("mcpSetupJson", () => {
  it("wraps the executable path in the MCP servers shape", () => {
    expect(mcpSetupJson("C:\\Program Files\\SavePrism\\SavePrism.exe")).toBe(
      [
        "{",
        '  "mcpServers": {',
        '    "saveprism": {',
        '      "command": "C:\\\\Program Files\\\\SavePrism\\\\SavePrism.exe",',
        '      "args": [',
        '        "mcp"',
        "      ]",
        "    }",
        "  }",
        "}",
      ].join("\n"),
    );
  });

  it("round-trips to the same object", () => {
    const parsed = JSON.parse(mcpSetupJson("/usr/bin/saveprism")) as unknown;
    expect(parsed).toEqual({
      mcpServers: { saveprism: { command: "/usr/bin/saveprism", args: ["mcp"] } },
    });
  });
});
