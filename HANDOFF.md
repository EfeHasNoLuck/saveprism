# HANDOFF

**State (2026-10-28, ~19:00 TR):** the maintainer's install test is done (what they did not try goes to the community).
The first public release is now **0.9.0 for Windows, Linux and macOS** (maintainer's decision): version bumped, a
`release` workflow builds all three into a draft release, README covers all three. Nothing pushed since `f96e98c`:
Actions minutes for the private repo are out, but Actions are free once the repo is public. Weekly Claude limit 21%
used (resets 2026-11-04 11:00 UTC).
The plan and history are in `NOW.md`; this page is only what the next session needs.

## Works (with evidence)
- README/GitHub showcase (`8d94e39`, `9eb850e`): both Mermaid diagrams render with mermaid 11.4.1, every form YAML
  parses, all 18 images in `docs/images/` reviewed (synthetic save, generated photo, no real data, no user name).
- MCP gaps (`3734bd6`, `82b9b2f`): 20 tools; rust 3/3, web 3/3, cargo deny ok; a real Claude Code round trip on a
  fixture copy; the running app writes `%LOCALAPPDATA%\SavePrism\settings.json` for the MCP server; a full Rust test
  run leaves the real backup folder untouched.
- Auto-update (`b4b1a3b`, `b6732c8`): checked over CDP with a local mock (toast, About "0.2.0 is available", discard
  guard, progress); the real endpoint stays silent while the repo is private (`.motor/update-1028/`).
- Crash screen (`5c8884e`): checked over CDP, dark/light, EN/TR; the copied report holds no path (`.motor/crash-1028/`).
- Slime Rancher 2 (`ad3987e`, `354e444`): the maintainer's 5 saves (`fixtures/external/slime-rancher-2/`) round-trip
  exactly in the CLI and backend; a money edit saved with a backup and reopened; the current debug CLI detects them as
  `format.slime-rancher-2`. The maintainer edited a real save in SavePrism; the game loaded it with the
  change (2026-10-28).
- Windows installer (`2d12d3e`, built as 0.1.0 before the version bump): setup exe only, no MSI (the updater only
  serves the exe). Publisher "Hüseyin Can ÇALTI" seen by the maintainer in the installed apps list; the setup exe shows
  the app logo. It installs per user into `%LOCALAPPDATA%\SavePrism\` next to the backups; Tauri's uninstaller removes
  only its own files there (non-recursive `RMDir`). The maintainer checked: "Decode with SavePrism" (Turkish label),
  the first-launch tour, a real Slime Rancher 2 edit loaded by the game.
- 0.9.0 prep (this commit): version 0.9.0 in Cargo, package.json, tauri.conf, both CHANGELOGs, notices, bug form;
  bundle targets `nsis, appimage, deb, app, dmg` (Tauri skips the other systems' targets); macOS ad-hoc signing
  (`signingIdentity: "-"`); CI rust job back on Windows, Ubuntu and macOS; `.github/workflows/release.yml`
  (tauri-action, manual run, draft release, `latest.json` for all systems); release text draft in
  `scripts/release/notes-0.9.0.md`. Rust 3/3 and web 3/3 passed on Windows.

## Broken / not checked
- Linux and macOS: never built or run. The CI rust job and the release workflow on Ubuntu/macOS have not run yet
  (first push after going public); expect small fixes. WSL Ubuntu here has no Rust and sudo needs a password.
- Left to the community (maintainer, 2026-10-28; public text must invite feedback warmly, never "you test it"):
  About → Check for updates against the real release, a real AI app over MCP, a second file handed to the open window.
- Explorer label language: the installer has no language picker; NSIS runs it in the Windows display language
  (checked in the NSIS docs), so Turkish Windows gets "SavePrism ile çöz", every other language "Decode with
  SavePrism". Someone who changes the Windows language later keeps the old label until the next install or update.
  The maintainer questioned this rule; explained, no change asked yet.
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
1. Release text `scripts/release/notes-0.9.0.md`: approved by the maintainer (2026-10-28).
2. Going public (asked 2026-10-28; needs the maintainer's yes): rewrite history once with git-filter-repo (not
   installed: `pip install git-filter-repo`) to drop `fixtures/external/` (added in `a973589`, removed in `4712fe9`:
   saves from other public repos) and to replace the author e-mail `hsyncalti2@gmail.com` (all 317 commits) with
   `116316749+huseyincancalti@users.noreply.github.com`; set that address as `user.email` here; `cargo deny check`;
   force-push; make the repo public. Actions are then free: fix what the Ubuntu/macOS CI runs show.
3. The secret `TAURI_SIGNING_PRIVATE_KEY` (text of `%USERPROFILE%\.tauri\saveprism-updater.key`): the maintainer adds
   it in GitHub → Settings → Secrets → Actions, or `gh secret set` with their OK (also make sure they keep a backup
   copy of that file). Then Actions → release → Run
   workflow; check the draft has the setup exe, dmg, AppImage, deb, their `.sig` files and `latest.json`; put the
   approved text on the draft; publish only with the maintainer's OK (not a pre-release: the updater reads
   `releases/latest`).
4. Turn on Discussions and add "Community themes"; set the GitHub description and topics
   (`docs/research/seo-2026-10-28.md`).

## Maintainer decisions (2026-10-28)
- Publisher "Hüseyin Can ÇALTI" (keep the surname in capitals). Windows ships as the setup exe only, no MSI.
- README: tell the maintainer when it is final; they review it before going public.
- First public release 0.9.0 with Linux and macOS; the community tests those; a real 1.0 after a quiet period.
- Release: show the maintainer only the release text (short bullets of what the player gets); fill tag and title
  yourself; publish after their OK. They saw the README and said it looks good.
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
