// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  claudeCodeCommand,
  mcpCursorJson,
  mcpSetupJson,
  mcpSnippet,
  mcpVsCodeJson,
} from "./setup";

const WINDOWS_EXE = "C:\\Program Files\\SavePrism\\saveprism-desktop.exe";

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

describe("client formats", () => {
  it("keeps the path readable (one backslash per separator) in JSON", () => {
    const parsed = JSON.parse(mcpSetupJson(WINDOWS_EXE)) as {
      mcpServers: { saveprism: { command: string } };
    };
    expect(parsed.mcpServers.saveprism.command).toBe(WINDOWS_EXE);

    const cursor = JSON.parse(mcpCursorJson(WINDOWS_EXE)) as {
      mcpServers: { saveprism: { command: string; args: string[] } };
    };
    expect(cursor.mcpServers.saveprism).toEqual({ command: WINDOWS_EXE, args: ["mcp"] });
  });

  it("gives VS Code its servers key, stdio type and path", () => {
    const parsed = JSON.parse(mcpVsCodeJson(WINDOWS_EXE)) as {
      servers: { saveprism: { type: string; command: string; args: string[] } };
    };
    expect(parsed.servers.saveprism).toEqual({
      type: "stdio",
      command: WINDOWS_EXE,
      args: ["mcp"],
    });
  });

  it("quotes the executable path in the Claude Code line", () => {
    expect(claudeCodeCommand(WINDOWS_EXE)).toBe(
      `claude mcp add saveprism -- "${WINDOWS_EXE}" mcp`,
    );
    expect(claudeCodeCommand("C:\\Tools\\Save Prism\\saveprism-desktop.exe")).toBe(
      'claude mcp add saveprism -- "C:\\Tools\\Save Prism\\saveprism-desktop.exe" mcp',
    );
  });

  it("routes each client to its own snippet", () => {
    expect(mcpSnippet("claudeDesktop", WINDOWS_EXE)).toBe(mcpSetupJson(WINDOWS_EXE));
    expect(mcpSnippet("other", WINDOWS_EXE)).toBe(mcpSetupJson(WINDOWS_EXE));
    expect(mcpSnippet("cursor", WINDOWS_EXE)).toBe(mcpCursorJson(WINDOWS_EXE));
    expect(mcpSnippet("vscode", WINDOWS_EXE)).toBe(mcpVsCodeJson(WINDOWS_EXE));
    expect(mcpSnippet("claudeCode", WINDOWS_EXE)).toBe(claudeCodeCommand(WINDOWS_EXE));
  });
});
