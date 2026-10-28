# HANDOFF

**State (2026-10-28, ~17:00 TR):** MCP gaps, auto-update and the crash screen are done and merged on `main`; a fresh
signed MSI/NSIS waits for the maintainer's install test (Next 4). Earlier today: the polish list and its follow-ups,
checked in the running app over CDP. Everything is committed locally, NOT pushed (GitHub Actions minutes are
out until next month; last pushed `4214786`). Weekly Claude limit 11% at ~15:30 TR (resets 2026-11-04 11:00 UTC); keep
visual checks in a Claude subagent (model sonnet), text chores in EVREN.

## Done today, second half (local commits on main)
`ac70822` App color (Blue/Violet/Teal/Green/Amber/Rose, Windows accent, Monochrome, Custom; theme-file `appColor`;
neutral grey opening cover) · `f2d9cca` photo preview click/drop (drops in Settings never open a save) ·
`5ce3552` fixes from the visual check (dark borders now follow the app color, empty photo zone shows one drop label,
last Windows accent cached so no blue start, accent swatch shows the accent) · `09408b6`, `2c24efd`, `a623039`
frosted glass over a background photo (maintainer chose B from `.motor/vcheck-1028d/ab-*.png`): cards 80 % dark /
88 % light, header/bottom bar/Settings sidebar blur, photo fades toward `--background` (slider renamed "Fade"),
`.on-photo` full text color on headings, group labels, slogan, Reset to default · `e30c3c2` Card opacity slider
(Appearance → Photo, `glassOpacity` 0-100, default 85, theme-file `glass`); the maintainer found the wide halo
too much, so text on the photo now has a thin contour + short lower-right shadow in dark only; light has none
(`ddeb633`, the pale shadow looked like a stray highlight; light top Reset to default 3.8:1). Trade-off measured:
dark Settings labels over the photo's bright grey part are 3.1-4.0:1 (light theme passes).
Checks after each: web 3/3 (rust 3/3, cargo deny, ignored desktop tests after the merges). Visual reports and
screenshots: `.motor/vcheck-1028c/`, `-1028d/`, `-1028e/` (briefs `.motor/vcheck-1028c.md`, `d`, `e`).
Known leftovers: Settings Preview red "Health" row 4.0:1 in dark (old, not glass); light active sidebar tab 4.0:1 at
the photo's darkest spot; a dropped non-image shows "(SP-APP-029)" in its toast like other error toasts.
Worktree leftovers the maintainer may delete by hand: `../saveprism-wt/{satisfactory,w4-settings,w6-shortcuts,
w7-settings-fix,w7-screens-fix,w8-appcolor,w8-photo}` (branches `wt/w8-*` are merged).

## Maintainer decisions today
- About keeps the short tagline "A save editor that runs on your PC. No account, no upload." (TR too). The GitHub
  "About" description is a DIFFERENT, search-friendly line: "Save editor for PC games. Edit Unity, Unreal, RPG Maker,
  Ren'Py and Godot saves offline, with automatic backups." GitHub topics list: `docs/research/seo-2026-10-28.md`.
- App color changeable, customization left to the community (theme files). Glass look in Photo mode (B).
- "Ben başka bir şey göremiyorum, sonra release düşünürüz", then the README/GitHub showcase (below).

## Done: README and GitHub showcase (2026-10-28, `c23e720`, `9d776ec`)
Vision prompt: `.motor/readme-vision-tr.md`. Brainstorm: footer = GitHub profile only (no e-mail); visuals = "as many as
fit" (hero GIF, dark/light screenshots per feature, Mermaid). README: hero (logo, badges, paragraph, quick links,
`docs/images/demo-{dark,light}.gif`), Download, Quick start, build from source, AI and MCP first (no API keys; setup per
app; tool table; diagram), features with 8 screenshot pairs, save-safety diagram, formats (15 most asked + the rest in
`<details>`), where SavePrism keeps things, CLI, themes/languages, repo tree, contributing, license, footer. CONTRIBUTING:
add a language / share a theme / add a format / add an MCP tool. Forms: `add_language.yml`, Discussions
`community-themes.yml` (needs a "Community themes" category once Discussions is on), feature request with area + AI/MCP
field, config links to Discussions. Visuals recorded by a subagent: `.motor/gif-1028.md`, scripts `.motor/gif-1028/`.
Not seen on GitHub yet (nothing pushed). `C:\SavePrismDemo\` (made-up demo files) is left for the maintainer to delete.
Slime Rancher 2 (maintainer, 2026-10-28): own binary tagged-record format, not encrypted; their saves copied to
`fixtures/external/slime-rancher-2/`; a "Slime Rancher 2 save support" session card was proposed (not started).

## Next (in order)
1. Done 2026-10-28: MCP gaps. `save_locations`, `compare_saves`, `open_code` + `copy_code`, `open_save` takes
   `gameFolder`; the window shares backup/app folders and backup counts in `%LOCALAPPDATA%\SavePrism\settings.json`
   (`session/shared_settings.rs`), read by the MCP server. Checks pass; real Claude Code round trip passed and the
   running app wrote the file (NOW.md). Pre-existing test leak seen: `session/compare.rs`
   `an_rpgmaker_mv_pair_reports_the_edited_value` saves with the default backup root, so every `cargo test` drops a
   small synthetic backup into the real `%LOCALAPPDATA%\SavePrism\backups` (not fixed).
2. Done 2026-10-28: auto-update (`6e9176e`, merge `1de6afd`). Signing key outside the repo:
   `%USERPROFILE%\.tauri\saveprism-updater.key` (+ `.pub`, no password). The maintainer must back it up: without it,
   installed copies can never take another update. Release: `bash scripts/release/build.sh` (signed installers +
   `target/release/bundle/nsis/latest.json`); upload the setup exe, its `.sig`, the MSI and `latest.json` to the
   GitHub release `v<version>`. Without the key: `pnpm build --no-sign`. Not verified: a real download → install →
   restart (needs a public release).
3. Done 2026-10-28: crash screen (`7493570`), checked in the running app (`.motor/crash-1028/`).
4. Built 2026-10-28 from `1de6afd` (signed): `target/release/bundle/msi/SavePrism_0.1.0_x64_en-US.msi` and `_tr-TR.msi`
   (10.25 MiB), `target/release/bundle/nsis/SavePrism_0.1.0_x64-setup.exe` (6.56 MiB). Waiting for the maintainer to
   install and try (Explorer entry, second-start hand-off, opening logo, first-launch tour, Check for updates in
   About, real saves, a real AI client over MCP, Settings backup folder used by the MCP).
5. Only then ask about publishing (release / public repo). Before going public: the `bed2ed9` history cleanup (needs
   the maintainer's OK); push the local commits once Actions minutes are back. No release workflow exists
   (`release.yml` in `docs/MAP.md` was never written): build locally and upload by hand for v0.1.

## Gotchas
- Start an EVREN job: `bash .motor/start_job.sh <name>` (brief `.motor/<name>.md`; `VCHECK=vcheck-1028` copies those
  screenshots into the worktree as `.motor/vcheck/`). At most 2 at once, started seconds apart ("database is locked").
- The project hook refuses `git checkout -- <file>` (use `git show HEAD:<file> > <file>`), forced worktree removal,
  and any Bash command that mentions `.claude/araclar` together with file edits.
- The dev WebView's localStorage is the maintainer's own: visual checks snapshot and restore every `saveprism.*` key.
- The old session "SavePrism: Satisfactory merge, then polish" (`local_54de6911-…`) could not be archived (Remote
  Control client attached); the maintainer can archive it from the sidebar.
- Focus engines: Unreal, Unity, RPG Maker, Ren'Py. Never touch the maintainer's original saves.
- Visual checks that pick a background photo also snapshot and restore the app folder's `background\` folder (it did
  not exist on the dev PC; the checks deleted it again). Test photo: a copy of `C:\Windows\Web\Wallpaper\Windows\img0.jpg`.
