# HANDOFF

**State (2026-10-24, late morning):** NOW.md todos 1-3 are done (todo 2 formats all merged, todo 3 customization finished).
Last code commit `ef5c335`. Nothing in flight; working tree clean except git-ignored `fixtures/external/` and `.motor/`.

**Works (with evidence)**
- Every screen merged overnight was checked in the running app over CDP (dark/light, EN/TR) by a Claude subagent; its
  findings were fixed and re-checked (`422a4bb`, `9f4ce9d`): big numbers no longer cut, read-only top-level rows show their
  value, Ren'Py alias rows have unique keys and a read-only tooltip, Add popover texts, TR header fits, dimmed locked toggles.
- Customization (`45d202f`, `a900b54`): Text size S/N/L (grids and cards scale), color presets (full tonal sets,
  `colors.test.ts` checks 4.5:1), backups to keep 1-50, own names for unnamed RPG Maker variables/switches per game,
  `docs/themes.md`. Checked in the running app. Review of the backup count found that a low count trimmed every game's
  backups: fixed in core (`keep_last` only for the written file's folder; the backup just made is never pruned; tests).
- Real saves (local only, `fixtures/external/SOURCES.md` lists them): TyranoScript 2, zip JSON 3, Gen 3 3, Witcher 3 3,
  Ren'Py 1, RPG Maker XP 1, Godot JSON 1, all `inspect` → exact, editable. Gen 3 and Witcher 3 needed fixes (`ef5c335`).
- `bash .claude/araclar/kontrol.sh rust` 3/3 and `web` 3/3 on `ef5c335`.

**Broken / not checked**
- Turkish UI: host error toast bodies stay English (NOW.md Known gaps).
- A zip with several JSON entries opens only the largest (Shapez 2, House Flipper 2 `header.json`).
- No real saves found for VX/VX Ace, 2000/2003, Wolf RPG, QSP, Naninovel, Flash `.sol`, SugarCube.
- The CLI `decode`/`encode` only handle JSON, RPG Maker JSON, ES3 and GVAS (not the newer codecs).
- Remote Control is blocked by the organization's policy (the user asked for it; only their admin can allow it).
- The visual-check subagent once reset the dev app's localStorage (`saveprism.settings`, theme, lang) in the WebView
  profile; restored to `{"saveDefault":"overwrite"}`, lang `tr`, system theme. Other old dev settings were lost. Told the user.

**Next step:** NOW.md todo 4 (AI): an MCP server over the same core (open, read, search, propose and apply edits; saving
still makes a backup), then "Ask AI" (copy the selected values with a short context), then docs. Start with a short design
note in the brief (which process hosts the server, stdio vs the desktop app, how edits reach the open session), then hand
the code to EVREN (`.motor/` brief, deepseek-v4.1-flash, "run every command from the repo root, no /tmp") and review it
with a Claude subagent (it writes files).

**Engines (2026-10-24):** EVREN deepseek-v4.1-flash did all chores today well (real-save search, customization drafts,
Gen 3 / Witcher 3 fixes). Codex is at its limit until 17 Nov; Cursor free plan at its limit. The visual-check subagent must
snapshot every `saveprism.*` localStorage key first and restore it exactly at the end. Hook note: a Bash command that
writes files (sed -i, heredoc into a script) must not also mention `.claude/araclar` or `LICENSE`, or the hook refuses it.
