# HANDOFF

**State (2026-10-27, ~13:40 TR):** todo 6 (parity) done for everything with a public layout; the proof steps passed
today (real MSI install, real saves in the installed app, Claude Code over MCP with the installed exe). Next: the
Satisfactory test the user asked for, then polish, then (only after polish) ask the user about publishing.
**Not pushed:** GitHub Actions minutes are out; the user said "commit tut, push atarız sonra". Before each commit:
kontrol rust + web + `cargo deny check`. Last pushed commit: `4214786`.

**Running (EVREN, started ~13:33 TR, detached `opencode.exe`, no session owns it)**
- `satisfactory` in `../saveprism-wt/satisfactory` (brief `.motor/w3-satisfactory.md`, log `.motor/w3-satisfactory.log`,
  report `../saveprism-wt/satisfactory/.motor/w3-satisfactory-report.md`). The user's own saves are copied (local
  only, git-ignored) to `fixtures/external/satisfactory/` (tamam-46, autosave_0, tamam-43; header v14, save v52,
  build 424353) and into the worktree. Watch with a background until-loop (report exists, or no `opencode.exe` in
  `tasklist`); if it stopped without a report, find its session id in `~/.local/share/opencode/log/opencode.log`
  by the worktree path and resume with `--session` naming any refused path.
- Merge: `python -I .motor/merge_wt.py satisfactory "feat(codecs): Satisfactory saves"`, `cargo fmt --all`, kontrol
  rust + web, `cargo deny check`, the ignored real-save tests (`cargo test -p saveprism-codecs -- --ignored`), then a
  CLI `inspect` of the 3 local saves, commit. Then show the user: copy a save to `.motor/realtest/` and let them open
  it in the app (rebuild/reinstall only if they want to test the installed app: `pnpm -C apps/desktop tauri build`,
  then `msiexec /i ... /l*v <scratch log>` opened for them to click; they approve installs).

**Works (with evidence), this session** — all on `main`, local only:
- ser, kvdb, real Gwent layout, vn3, Godot resources, Gwent card list, ES3 number/object keys, GVAS hint inference,
  Gwent view (layout A + deck tabs, `c0e711d`), header "Close file" with the discard dialog (`c0e711d`),
  visual-check fixes (`0453533`). Sweep of 88 local real saves: only a deliberately corrupted MZ file fails.
- Proof 2026-10-27: MSI from `a04acaa` installed to `C:\Program Files\SavePrism\`; user opened 7 real save copies
  (`.motor/realtest/`) and liked it; Claude Code sonnet via `--mcp-config` → installed exe edited a real MV save copy
  (gold 200 → 4321, item 1 → 9), backup = original bytes, re-checked with the CLI.

**Polish (after Satisfactory; from the 2026-10-27 visual check, screenshots `.motor/vcheck-1027/`)** — UI work for one
Claude subagent with `isolation: "worktree"` + screenshots, then show the user:
1. Light theme: read-only and editable fields differ only by slightly paler text (make read-only clearly read-only).
2. Long strings are cut in fixed-width inputs (AGS signature/guid, ES3 guid, Brickadia authorId...).
3. "Ask AI" is missing on cards whose values all sit inside groups (protobuf, SII, Godot, SQLite/LiteDB, GVAS,
   Adventure Creator).
4. Witcher 3 untyped values show as `raw:00008f42`.
5. Empty containers show as `{0}`/`[0]` in some views and "0 items" in others.
6. LiteDB gives no hint why `_id`/indexed fields are locked.

**Broken / not checked**
- `0453533` fixes checked by unit tests only, not in the running app.
- Stale folders the hook won't let me delete: `.claude/worktrees/agent-a5b61daf70f35e62e`,
  `agent-a945e990b19f4bff1`, `agent-a15308e8a9ffff604` (user can delete by hand).
- Not possible (no public layout): Smile Game Builder, LiveMaker, Visionaire, Construct Classic, Aoi, Easy Save 2;
  Oodle/Kraken (no permissive decoder).

**User decisions:** ask about publishing only after polish; no value finder; quiet backend UX; at most 2 EVREN jobs at
once; no pushes while CI minutes are out; never touch the user's original saves (copies only).
