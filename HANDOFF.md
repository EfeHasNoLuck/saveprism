# HANDOFF

**State (2026-10-27, ~03:45 TR):** todo 6 (parity with saveeditonline.com `/formats`), wave 3 done except the user's
Gwent A/B pick. **Not pushed:** GitHub Actions minutes ran out on 2026-10-26; the user said (2026-10-26 night) "commit
tut, push atarız sonra". Before each commit: kontrol rust + web + `cargo deny check`. Last pushed commit: `4214786`.

**Works (with evidence)** — merged this session, each after kontrol rust 3/3, web 3/3, cargo deny ok:
- ser `5e5a3e5` (protobuf, Bitser, SCS `.sii`, LZip, Base64URL, Snes9x, RetroArch, `.srm`); kvdb `dfc4903` (LevelDB,
  RocksDB read-only; LiteDB in place with `_id`/indexed fields read-only); real Gwent layout `097915f`; vn3 `5a09adf`
  (AliceSoft, Artemis read-only, Construct labels); Godot `.tres`/`.res` `4a0ed89`; Gwent card list from the
  player's game bundles `78dfdc9`; ES3 bare number/bool keys `3a2bc9a` and object keys `3a40520`; GVAS hint inference
  `e91ee3d`; Gwent view A+B `6092cc4`; visual-check fixes `0453533`.
- Sweep of all 88 local real saves (`fixtures/external`, CLI `inspect`): only the deliberately corrupted RPG Maker MZ
  file fails; 4 ES3 + 1 GVAS real saves newly open; no regressions.
- Visual check over CDP 2026-10-27 of every new format (screenshots `.motor/vcheck-1027/`), Gwent view checked on a
  copy of a real save (screenshots `.motor/gwent-ab/`, sent to the user).

**Waiting on the user:** Gwent layout A (tiles, faction tabs) or B (dense list + decks card). Set `GWENT_LAYOUT` in
`apps/desktop/src/editor/gwent.ts` (now "a"), delete the other layout's code and its strings, kontrol web, commit.

**Broken / not checked**
- The `0453533` fixes are checked only by unit tests, not in the running app.
- Design leftovers from the visual check are in NOW.md "Known gaps" (light-theme read-only contrast, Ask AI missing
  for grouped values, cut long strings, `raw:` Witcher values).
- `.claude/worktrees/agent-a5b61daf70f35e62e` could not be deleted (path too long; the hook blocks `rmdir /s`); the
  user can delete it by hand. Old `.claude/worktrees/agent-a945e990b19f4bff1` is also stale.
- Every new format is tested on synthetic files only, except Witcher 3/Gwent, ES3, GVAS (real saves).
- Not possible (no public layout): Smile Game Builder, LiveMaker, Visionaire, Construct Classic, Aoi, Easy Save 2;
  Oodle/Kraken (no permissive decoder); "SSSoftware Serialize" unknown.

**Next step:** apply the user's Gwent pick. Then the proof steps from NOW.md todo 5/6: install the MSI for real (with the
user present: it changes the PC), open real saves in the installed app, a real AI client over MCP. Ask about
publishing only after those pass.

**How jobs ran this session:** EVREN (`opencode run -m evren/deepseek-v4.1-flash --agent build`, `< /dev/null | cat >
log`) in `../saveprism-wt/<name>` worktrees from `HEAD`, briefs `.motor/w3-<name>.md` + `w2-common.md` copied in,
real saves copied into the worktree's git-ignored `fixtures/external` when needed; merged with
`python -I .motor/merge_wt.py <name> "<subject>"`, then `cargo fmt --all` if module order changed. Watch out: `*.log`
is git-ignored (the LevelDB `.log` fixture needed `!fixtures/synthetic/**/*.log`); a run stops for good when it touches
a path outside its worktree (resume with `--session <id>` naming the bad path). UI work: Claude subagent with
`isolation: "worktree"`; visual checks: sonnet subagent over CDP (snapshot/restore `saveprism.*` localStorage).

**User decisions:** do not ask about publishing until install + real saves + real AI client pass; no value finder;
quiet backend UX; at most 2 EVREN jobs at once; no pushes while CI minutes are out.
