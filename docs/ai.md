# Using SavePrism with AI

SavePrism works with AI chats in two ways:

1. **Ask AI** copies the values of a list with a short note, so you can paste them into any AI chat and ask what to change.
2. **The MCP server** lets an AI app (Claude Desktop, Claude Code, Cursor, VS Code and others) open, read and edit saves
   through SavePrism itself, with the same checks and backups as the app.

Nothing is sent anywhere by SavePrism. The AI app you use may send the values it reads to its own online model; keep that
in mind for saves with personal data (player names, account ids).

## Ask AI

Every list card (items, variables, switches, a character, the Values list) has an **Ask AI** button in its header. It
copies the values the card shows, with its search applied, on every page (at most 300), together with the file name, the
format and each value's limits. Paste the text into the chat and ask your question, for example "give me enough potions
for the final boss". Then make the changes in SavePrism.

## MCP server

The SavePrism program is also an MCP server: started as `SavePrism.exe mcp`, it talks to the AI app over standard
input and output and opens no window. It does not listen on a network port.

### Setup

Open SavePrism, go to **Settings → AI tools** and press **Copy setup**. It copies this, with the real path of your
SavePrism program filled in:

```json
{
  "mcpServers": {
    "saveprism": {
      "command": "C:\\Users\\you\\AppData\\Local\\SavePrism\\SavePrism.exe",
      "args": ["mcp"]
    }
  }
}
```

- **Claude Desktop:** Settings → Developer → Edit Config. Paste the `saveprism` entry into `mcpServers` of
  `claude_desktop_config.json` and restart Claude Desktop.
- **Claude Code:** `claude mcp add saveprism -- "C:\path\to\SavePrism.exe" mcp`
- **Cursor:** Settings → MCP → Add new MCP server, or paste the entry into `%USERPROFILE%\.cursor\mcp.json`.
- **VS Code:** add it to `.vscode/mcp.json` under `servers` (VS Code calls the key `servers`, not `mcpServers`, and wants
  `"type": "stdio"` in the entry).

### Tools

| Tool | What it does | Writes files |
|---|---|---|
| `open_save` | Opens a save (password for encrypted Easy Save 3 files). Runs the same round-trip check as the app. | no |
| `game_view` | The app's view: RPG Maker sections with names (gold, characters, items, variables, switches), the story variables of a Twine save, or the Values list. Filter and page through long lists. | no |
| `list_values` | The children of one value in the full tree, a page at a time. | no |
| `search_values` | Finds values by path or value text. | no |
| `field_info` | Type and limits of one value. | no |
| `set_data_folder` | Reads names from a game data folder (RPG Maker). | no |
| `propose_edits` | Changes values in memory. Everything is checked first; on any error nothing changes. | no |
| `add_value`, `add_inventory`, `remove_value` | Adds or removes entries where the format allows it, in memory. | no |
| `pending_changes` | Lists every change since opening: old value → new value. | no |
| `undo`, `discard_changes` | Takes back the last change, or all of them. | no |
| `save_changes` | Writes the changes to the open file, or to a new file in the same folder. A backup is made first, the file is replaced in one step, and every changed value is read back and checked. | **yes** |
| `list_backups`, `restore_backup` | Lists the backups of the open save and puts one back (the current file is backed up first). | restore: **yes** |

Only `save_changes` and `restore_backup` write files; both are marked as destructive, so AI apps ask you before they run
them. Let the AI show you `pending_changes` before you allow a save.

### Good to know

- The AI works on its own copy of the save in memory. A SavePrism window that has the same file open does not see the
  AI's changes; open the file again to see them.
- Neither side overwrites the other: when the file changed on disk after it was opened, saving is refused
  (`SP-APP-023`). Use **Save as** to keep your edits in another file, or open the file again.
- Files that do not pass the round-trip check open read-only here too, and save content is never run as code.
- The AI cannot write anywhere else: `save_changes` only replaces the open file or creates a new file next to it.
- Long lists come in pages (100 rows by default). A result over 80,000 characters is refused with a hint to ask for
  a smaller page, a section or a filter.
- Backups live in SavePrism's own data folder (`%LOCALAPPDATA%\SavePrism\backups`), never next to the save; the AI can list
  them with `list_backups`.
