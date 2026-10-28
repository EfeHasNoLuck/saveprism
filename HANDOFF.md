# HANDOFF

**State (2026-10-28, ~17:30 TR):** everything planned before the install test is done on local `main` (last work commit
`639b13b`, then this handoff). Nothing pushed since `4214786`: GitHub Actions minutes are out until next month. Weekly
Claude limit ~11% (resets 2026-11-04 11:00 UTC); keep visual checks in a Claude subagent (sonnet), text chores in EVREN.
The plan and history are in `NOW.md`; this page is only what the next session needs.

## Works (with evidence)
- README/GitHub showcase (`c23e720`, `9d776ec`): both Mermaid diagrams render with mermaid 11.4.1, every form YAML
  parses, all 18 images in `docs/images/` reviewed (synthetic save, generated photo, no real data, no user name).
- MCP gaps (`ea9f664`, `145db26`): 20 tools; rust 3/3, web 3/3, cargo deny ok; a real Claude Code round trip on a
  fixture copy; the running app writes `%LOCALAPPDATA%\SavePrism\settings.json` for the MCP server; a full Rust test
  run leaves the real backup folder untouched.
- Auto-update (`6e9176e`, `1de6afd`): checked over CDP with a local mock (toast, About "0.2.0 is available", discard
  guard, progress); the real endpoint stays silent while the repo is private (`.motor/update-1028/`).
- Crash screen (`7493570`): checked over CDP, dark/light, EN/TR; the copied report holds no path (`.motor/crash-1028/`).
- Slime Rancher 2 (`33c0857`, `86019d5`): the maintainer's 5 saves (`fixtures/external/slime-rancher-2/`) round-trip
  exactly in the CLI and backend; a money edit saved with a backup and reopened; the current debug CLI detects them as
  `format.slime-rancher-2`.
- Installers built from `1de6afd` and signed (reported by the MCP session): `target/release/bundle/msi/
  SavePrism_0.1.0_x64_{en-US,tr-TR}.msi`, `target/release/bundle/nsis/SavePrism_0.1.0_x64-setup.exe`.

## Broken / not checked
- The new installers are not installed or tried yet (the maintainer's step).
- Slime Rancher 2: the one-row-per-slot layout is not seen in the running app; `slimerancher.cfg`/`.prf` do not open;
  Slime Rancher 1 not checked (no sample); its fuzz target is not in `.github/workflows/fuzz.yml`; cargo-fuzz is not
  installed here, so no fuzz target ran locally.
- Auto-update: a real download → install → restart needs a public release.
- README and forms not seen on GitHub; the Discussions "Community themes" category does not exist yet.
- Known, not blocking: Settings Preview red "Health" row 4.0:1 in dark; light active sidebar tab 4.0:1 at the photo's
  darkest spot; crash screen buttons plain; an MCP edit is not seen by a window with the same file open (it refuses to
  save over it, SP-APP-023); no code signing, so SmartScreen warns.
- Folders the project hook stops agents from deleting; the maintainer deletes them in their own PowerShell:
  `C:\SavePrismDemo\` (made-up demo files), `.claude\worktrees\agent-*` (7) and
  `..\saveprism-wt\{satisfactory,w4-settings,w6-shortcuts,w7-screens-fix,w7-settings-fix}`. Use the `\\?\` prefix
  (long paths): `Remove-Item -LiteralPath "\\?\<full path>" -Recurse -Force`.

## Next step
1. The maintainer installs `SavePrism_0.1.0_x64-setup.exe` (or a MSI) and tries: Explorer "Decode with SavePrism",
   a second start handing the file over, the opening logo, the first-launch tour, About → Check for updates ("Could
   not check" is expected while the repo is private), real saves (also Slime Rancher 2 slot rows), a real AI app over
   MCP using the backup folder chosen in Settings. They also back up `%USERPROFILE%\.tauri\saveprism-updater.key`
   (without it, installed copies can never take another update).
2. Fix what that test finds, check it, commit.
3. Only after the test passes, ask about publishing. Before going public: the `bed2ed9` history cleanup (real saves
   were pushed once; needs the maintainer's OK); push when Actions minutes are back (run `cargo deny check` first,
   then check the runs); turn on Discussions and add "Community themes"; set the GitHub description and topics
   (`docs/research/seo-2026-10-28.md`); `bash scripts/release/build.sh`, then upload the setup exe, its `.sig`, the
   MSI and `latest.json` to release `v0.1.0`.

## Maintainer decisions (2026-10-28)
- About tagline "A save editor that runs on your PC. No account, no upload."; the GitHub description is a different,
  search-friendly line (in `docs/research/seo-2026-10-28.md`).
- README footer: GitHub profile only, no e-mail. Visuals: as many as fit.
- Auto-update is expected, with Check for updates in About.

## Gotchas
- Start an EVREN job: `bash .motor/start_job.sh <name>` (brief `.motor/<name>.md`). At most 2 at once, started seconds
  apart ("database is locked").
- The project hook refuses `git checkout -- <file>` (use `git show HEAD:<file> > <file>`), recursive deletes, forced
  worktree removal, and any Bash command that mentions `.claude/araclar` together with file edits.
- Visual checks snapshot and restore every `saveprism.*` localStorage key and the app folder's `background\` folder;
  open fixture copies only; never a Windows wallpaper in published images.
- Focus engines: Unreal, Unity, RPG Maker, Ren'Py. Never touch the maintainer's original saves.
- The old session "SavePrism: Satisfactory merge, then polish" (`local_54de6911-…`) could not be archived (Remote
  Control client attached); the maintainer can archive it from the sidebar.
