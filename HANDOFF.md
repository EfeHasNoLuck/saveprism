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
too much, so text on the photo now has a thin contour + short lower-right shadow (their wish). Trade-off measured:
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

## NEXT: README and GitHub showcase (maintainer's request, 2026-10-28)
The maintainer's full vision prompt (Turkish, written with Gemini, kept verbatim): `.motor/readme-vision-tr.md`. They
said: not strict rules; analyze the codebase first, adapt it to our reality, use your own judgment; ready to brainstorm
inconsistencies. Their structure: centered hero (logo, tech badges, MIT badge, one-paragraph why/what, quick links to
Quick start · AI & MCP · Report a bug · Add a theme · Add a language, screenshot/GIF) → Download + developer setup →
AI & MCP first in the details (what, how, config JSON, Mermaid diagram) → features, repo tree (`<details>`), config
table, theme + language customization → contributing, tests, issue links → license + personal footer. Plus GitHub
files: LICENSE, issue forms (bug, feature/AI tool, add theme, add language, config.yml), CONTRIBUTING.md.

What the codebase already has / where the prompt does not fit (raise these in the brainstorm, one question at a time):
1. `LICENSE` exists (MIT, "Copyright (c) 2026 Hüseyin Can ÇALTI") and is a LOCKED path: keep it as is.
2. `.github/` exists: `ISSUE_TEMPLATE/{bug_report,feature_request,game_support,config}.yml`, `PULL_REQUEST_TEMPLATE.md`,
   `dependabot.yml`, `workflows/`; and `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` exist. So: update and add,
   not create. Add `add_language.yml`. Themes were decided to live in GitHub Discussions → "Community themes" (the app
   links there): suggest a Discussions category form (`.github/DISCUSSION_TEMPLATE/community-themes.yml`) instead of
   (or besides) an `add_theme.yml` issue. Keep `game_support.yml` (save-format requests matter most here).
3. No `.env`, no API keys, no models: SavePrism never calls an AI model. "Ask AI" copies text for any chatbot; the MCP
   server is the app itself (`saveprism-desktop.exe mcp`, stdio, started by the AI app; tools listed in `docs/ai.md`).
   So the "API keys / models" part becomes "connect your AI app" (per-client setup, same as Settings → AI connection).
   The "env/config table" becomes a short "Where SavePrism keeps things" table (settings in the app, app folder
   `%LOCALAPPDATA%\SavePrism` with `backups\`, `pasted\`, `background\`; both folders changeable in Settings).
4. Personal footer with an e-mail makes the address public: ask the maintainer (suggest the GitHub profile only).
5. Badges: Rust, Tauri 2, React 19, TypeScript, Tailwind, Vite, pnpm, MIT, Windows; a release badge only after v0.1;
   no CI badge while Actions minutes are out (it would look broken).
6. Download: no release yet → "Download" points to Releases once v0.1 ships; until then "build from source".
   Dev setup facts: Windows 10/11, Node 22, pnpm 11, Rust 1.99 (MSVC), WebView2; `pnpm install`,
   `pnpm -C apps/desktop tauri dev`, `pnpm -C apps/desktop tauri build`; tests `cargo test`, `pnpm -C apps/desktop test`
   (our `kontrol.sh` is an internal tool under the locked `.claude/`).
7. Adding a language: `apps/desktop/locales/<code>/{common,editor}.json` + `CHANGELOG.md`, the language list and flag in
   `apps/desktop/src/i18n/` (`language.ts`, `flags.tsx`); `src/i18n/keys.test.ts` fails on any missing key. Adding a
   theme: export from Settings → Appearance → Theme files; format in `docs/themes.md` (w8-appcolor adds `appColor`).
8. Screenshots: pick from the next visual check, show no real save data, put them in `docs/images/`.
9. Keep the README's existing "Supported formats" table, the clean-room statement and the "keep your own backups"
   honesty; English only in the repo (a `README.tr.md` could come later if the maintainer wants).
10. The repo is private. Making it public needs the maintainer's explicit OK, and real saves were pushed once in
    `bed2ed9` (history cleanup needs their OK too).
Plan: write the README/CONTRIBUTING/forms through one EVREN job with a clear brief (docs only), then Claude reviews
the facts against the code; or Claude writes it directly if the limit allows (it is text-heavy, ~1 file + 3 forms).

## After that (unchanged order)
1. MCP gaps: `compare_saves`, `save_locations`, `open_code` tools (read-only, same Session code, documented in
   `docs/ai.md`); check `open_save` uses the automatic key finder; the MCP server should use the backup folder chosen in
   Settings (today it always uses `%LOCALAPPDATA%\SavePrism\backups`; the UI keeps folder choices in WebView
   localStorage, so this needs a small shared settings file the app writes and the MCP reads).
2. A fresh MSI/NSIS build for the maintainer to install and try (Explorer entry, second-start hand-off, opening logo,
   first-launch tour, What's new after an update, real saves, a real AI client over MCP).
3. Only then ask about publishing (release / public repo).

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
