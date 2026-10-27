/** Pure setup snippets for the "AI connection" tab (no React, no Tauri). */

/** The AI apps the tab can prepare a setup for. */
export type McpClient = "claudeDesktop" | "claudeCode" | "cursor" | "vscode" | "other";

/** The choices, in the order the app pills show them. */
export const MCP_CLIENTS: McpClient[] = ["claudeDesktop", "claudeCode", "cursor", "vscode", "other"];

/** The default picked app. */
export const DEFAULT_MCP_CLIENT: McpClient = "claudeDesktop";

/** True when `value` names an app the tab knows. */
export function isMcpClient(value: unknown): value is McpClient {
  return typeof value === "string" && (MCP_CLIENTS as string[]).includes(value);
}

/**
 * The MCP setup snippet Claude Desktop and other clients read: the running SavePrism
 * executable, started with `mcp`. Two-space indent, so it pastes into a settings box as it is.
 */
export function mcpSetupJson(exePath: string): string {
  return JSON.stringify(
    { mcpServers: { saveprism: { command: exePath, args: ["mcp"] } } },
    null,
    2,
  );
}

/** Cursor reads the same `mcpServers` shape as Claude Desktop. */
export function mcpCursorJson(exePath: string): string {
  return mcpSetupJson(exePath);
}

/** VS Code uses `servers` and wants the transport named `stdio`. */
export function mcpVsCodeJson(exePath: string): string {
  return JSON.stringify(
    { servers: { saveprism: { type: "stdio", command: exePath, args: ["mcp"] } } },
    null,
    2,
  );
}

/**
 * The Claude Code command line. The executable path is double-quoted, so a path with spaces
 * stays one argument.
 */
export function claudeCodeCommand(exePath: string): string {
  return `claude mcp add saveprism -- "${exePath.replace(/"/g, '\\"')}" mcp`;
}

/** The exact text "Copy setup" copies for `client`. */
export function mcpSnippet(client: McpClient, exePath: string): string {
  switch (client) {
    case "claudeCode":
      return claudeCodeCommand(exePath);
    case "cursor":
      return mcpCursorJson(exePath);
    case "vscode":
      return mcpVsCodeJson(exePath);
    case "claudeDesktop":
    case "other":
      return mcpSetupJson(exePath);
  }
}
