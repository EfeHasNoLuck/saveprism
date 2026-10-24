# HANDOFF

**State (2026-10-25, 00:20):** NOW.md todos 1-5 done (no release: see below); working on todo 6, wave 1 of full parity
with saveeditonline.com. Last commit `e2745ba`. Main is clean except git-ignored `fixtures/external/` and `.motor/`.

**Works (with evidence)**
- MCP server `saveprism-desktop.exe mcp` (`e553b88`, `d2a7cbe`): a real client works. `claude -p --mcp-config
  <json> --strict-mcp-config --allowedTools "mcp__saveprism__*" --model sonnet` opened a copy of a real MV save, set
  gold 200 → 4321 and an item 1 → 9, saved with a backup; values re-checked. Reviewed; findings fixed.
- Ask AI + Settings → AI tools (`f4fe817`, `304baf0`): checked in the running app (dark/light, EN/TR).
- Installer 0.1.0 (`910e293`): setup.exe + .msi built; the MSI unpacked with `msiexec /a` holds
  `saveprism-desktop.exe` (that is the installed exe name), which opens its window and answers `mcp`.
- REA analysis of saveeditonline.com: `docs/research/saveeditonline-behavior.md` (section 9 = ranked gaps; the site
  claims ~70 formats on `/formats`).
- Wave 1 merged: Ren'Py saves signed with the player's own key (`13505dc`; a reviewer verified our signatures with
  Ren'Py's own `ecsign.py`; the real Ren'Py save still round-trips exact); MessagePack/CBOR/BSON/NBT (`095ce2e`).
  `bash .claude/araclar/kontrol.sh rust` 3/3 and `web` 3/3 after both merges.

**Broken / not checked**
- EVREN hit its DAILY token limit (2026-10-24 ~23:45; still limited at 00:16, error "Günlük token limitiniz aşıldı").
  It probably resets at 00:00 UTC (03:00 here). While limited, `opencode run` hangs and retries: test with
  `timeout 90 opencode run -m evren/deepseek-v4.1-flash "say ok"`.
- Four wave 1 jobs stopped half done, uncommitted, in their worktrees: `../saveprism-wt/layers` (21 files),
  `text` (43), `ui` (22), `renpyfix` (11). Briefs: `.motor/w1-common.md` + `.motor/w1-<name>.md` (copies inside each
  worktree's `.motor/`).
- Ren'Py signing not tried in a real game. Installer not installed for real (no Windows Sandbox here).

**Next step:** when EVREN works again, in each of the four worktrees run in the background, a minute apart (five at
once locked opencode's database): `OPENCODE_CONFIG=<main>/opencode.json opencode run --continue -m
evren/deepseek-v4.1-flash --agent build "You stopped because of a token limit. Continue the task in .motor/w1-common.md
and .motor/w1-<name>.md from where you stopped: check git status, finish what is missing, run cargo fmt and kontrol.sh,
write the report. Same rules: repo root only, no /tmp, do not commit."`. Then per branch: `python -I .motor/merge_wt.py
<name> "<subject>"`, kontrol.sh rust + web (separate command: the hook refuses a Bash line that writes files and names
`.claude/araclar`), review renpyfix with a Claude subagent (keys), visual check of ui (Where is my save?, Compare),
commit, push. Then wave 2 (NOW.md).

**User decisions this session:** do not ask about publishing until the app is proven (memory
`no-premature-release-ask`); no value finder / memory scanner (Cheat Engine's job); automatic work stays invisible in
the UI (memory `ux-quiet-backend`); for game keys, look for the game folder ourselves and ask for it only if not found.
