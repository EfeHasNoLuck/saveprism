# HANDOFF

**State (2026-10-28, ~12:00 TR):** the maintainer's polish list and their 4 follow-ups are all done and merged on
`main`, checked in the running app over CDP. Everything is committed locally, NOT pushed (GitHub Actions minutes are
out until next month; last pushed `4214786`). Weekly Claude limit was ~94% (resets 2026-10-28 11:00 UTC); keep
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
1. MCP gaps: `compare_saves`, `save_locations`, `open_code` tools (read-only, same Session code, documented in
   `docs/ai.md`); check `open_save` uses the automatic key finder; the MCP server should use the backup folder chosen in
   Settings (today it always uses `%LOCALAPPDATA%\SavePrism\backups`; the UI keeps folder choices in WebView
   localStorage, so this needs a small shared settings file the app writes and the MCP reads).
2. Auto-update (maintainer expects it, 2026-10-28; not built): `tauri-plugin-updater`, signing key outside the repo,
   `latest.json` on GitHub Releases (reachable only once the repo or a releases repo is public); a quiet "new version"
   notice with one Update button, a "Check for updates" button in About, What's new after the restart (already there).
3. Crash screen (old plan M3-13, never built): no React error boundary today, so a render error leaves a blank
   window. Add one with "Something went wrong" + Copy report (no paths) + Reload; pending edits are lost, say so.
4. A fresh MSI/NSIS build for the maintainer to install and try (Explorer entry, second-start hand-off, opening logo,
   first-launch tour, What's new after an update, real saves, a real AI client over MCP).
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
