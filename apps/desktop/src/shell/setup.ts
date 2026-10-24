/** Pure helpers for the "AI tools" section (no React, no Tauri). */

/**
 * The MCP setup snippet an AI app reads: the running SavePrism executable, started with `mcp`.
 * Two-space indent, so it pastes into a settings box as it is.
 */
export function mcpSetupJson(exePath: string): string {
  return JSON.stringify(
    { mcpServers: { saveprism: { command: exePath, args: ["mcp"] } } },
    null,
    2,
  );
}
