# HANDOFF

**State (2026-10-28, ~09:45 TR):** the maintainer's polish list (`docs/design/polish-v0.1.md`, todo 7 in `NOW.md`)
is built, checked in the running app over CDP, fixed and re-checked. The maintainer then asked for 4 more changes; 2 are
done, 2 run in EVREN right now (below). Everything is committed locally on `main`, NOT pushed (GitHub Actions minutes
are out until next month; last pushed `4214786`). The weekly Claude limit was ~90%: EVREN builds, Claude merges,
checks and does visual checks through a Claude subagent (model sonnet).

## Done today (local commits on main)
`ce44aca` start screen + FAQ + header + Where is my save + scrollbar · `c649106` Settings page (General, Appearance,
Saving & backups; app-folder move fixed to move only SavePrism's own files) · `f04f7d7` AI connection tab ·
`5a5db1d` About + in-app notices + What's new (`CHANGELOG.md`) + first-launch tour · `7f89bad` rebindable shortcuts ·
`01027c7` opening logo, Explorer "Decode with SavePrism" (NSIS + MSI) + single instance, theme files · `9c0ea31`,
`dff0665`, `e93cac9` fixes from the visual check (11 problems, re-checked OK) · `1af2100` one "Where is my save?" on
the start screen (FAQ item removed, the drop-card link stays), About shows only the version under the name ·
`d4c5c5f` + the handoff commit: search research `docs/research/seo-2026-10-28.md`.
Screenshots the maintainer saw: `.motor/vcheck-1028b/m-*.png` (local).

## Maintainer decisions today
- About keeps the short tagline "A save editor that runs on your PC. No account, no upload." (TR too). The GitHub
  "About" description is a DIFFERENT, search-friendly line: "Save editor for PC games. Edit Unity, Unreal, RPG Maker,
  Ren'Py and Godot saves offline, with automatic backups." GitHub topics list: `docs/research/seo-2026-10-28.md`.
- The app's own color must be changeable (blue everywhere can bore people), with dynamic / monochrome / pick-a-color,
  and customization left to the community (theme files) → job `w8-appcolor`.
- The photo preview in Appearance must be clickable and accept a dropped image → job `w8-photo`.
- After these: "Ben başka bir şey göremiyorum, sonra release düşünürüz", and the README/GitHub showcase (below).

## EVREN jobs: BOTH FINISHED (~09:50 TR), not merged yet. Each wrote its report and says rust 3/3 + web 3/3 in its
own worktree; no `opencode.exe` is left. First step of the next session: merge them (below).
| job | worktree | brief | report (inside the worktree) | opencode session |
|---|---|---|---|---|
| w8-appcolor | `../saveprism-wt/w8-appcolor` | `.motor/w8-appcolor.md` | `.motor/w8-appcolor-report.md` | `ses_e7ed91988ffeUmOKEMXYhwmKTj` |
| w8-photo | `../saveprism-wt/w8-photo` | `.motor/w8-photo.md` | `.motor/w8-photo-report.md` | `ses_e7ed8a90fffeTmvuV2I33uAte5` |
- Done when the report file exists AND no `opencode.exe` is left in `tasklist` (a job may keep working a while after
  writing its report). `opencode.exe` survives the Claude session; the logs `.motor/w8-*.log` stay empty while running.
- Stopped without a report (EVREN daily limit resets 00:00 UTC = 03:00 TR, or a refused path): resume from the worktree
  root: `OPENCODE_CONFIG=<main>/opencode.json opencode run --session <id> -m evren/deepseek-v4.1-flash --agent build
  "Continue the task in .motor/<name>.md ... write .motor/<name>-report.md" < /dev/null | cat > <main>/.motor/<name>-resume.log`
  (pattern in `.motor/resume_job.sh`, which still assumes `w3-` names).
- Merge each: `python -I .motor/merge_wt.py <name> "<subject>"`. Both touch `SettingsPage.tsx` (Appearance: App color
  group vs Photo card), the locales and maybe `App.tsx`. CAREFUL: merge_wt resolves EVERY conflict by joining both
  sides; for `.tsx` that is often wrong. Rebuild the hunks with `git merge-file -p ours base theirs` (see how it was done
  for SettingsPage on 2026-10-28) and resolve by hand. Then `cargo fmt --all`, `bash .claude/araclar/kontrol.sh web`
  and `rust` (run kontrol in its own Bash call: the hook refuses a command that also edits files), `cargo deny check`,
  `cargo test -p saveprism-desktop -- --ignored`, commit (Conventional Commit + Co-Authored-By trailer).
- What to check in the results: Blue writes NO overrides (today's look exact); every preset / Windows accent / Monochrome
  / Custom has readable text on primary in dark and light; the theme file `appColor` field validates; `boot.js` does
  not flash blue for a non-blue color; Windows accent re-read on window focus. Photo: click and Enter open the picker;
  a drop on the zone imports the photo; any drop in Settings never opens a save; drops on the start screen and with a
  save open still open files.
- Then a visual re-check through a Claude subagent: copy `.motor/vcheck-1028b.md` (setup and localStorage rule are in
  `.motor/vcheck-1028.md`; CDP scripts in `.motor/vcheck-1028/`), list the new items (app color choices in both themes,
  photo click/drop, 5-item FAQ, About version-only), and a fresh screenshot set; show the maintainer.
- Worktree leftovers that `git worktree remove` could not delete (long paths), the maintainer may delete by hand:
  `../saveprism-wt/{satisfactory,w4-settings,w6-shortcuts,w7-settings-fix,w7-screens-fix}`.

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
