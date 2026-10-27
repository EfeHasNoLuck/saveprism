# Polish for v0.1 — the maintainer's list (2026-10-27)

The maintainer's own visual and UX wishes for v0.1, with the decisions taken in the brainstorm. Work through them in
order; show mockups (A/B screenshots) before building a new screen.

## Mockups (approved 2026-10-27)
Static HTML mockups in `.motor/mockups-1027/` (local, git-ignored; `start.html`, `settings-B.html`, `settings-A.html`,
`where.html` + shared CSS/JS; PNGs next to them). The maintainer picked:
- Settings layout **B**: a full-window page under the normal header, left sidebar with "Back (Esc)", "Find a setting"
  search, the tabs and a "Community themes" link; breadcrumb; a live Preview card on Appearance. The controls inside
  each tab are the ones drawn in `settings-A.html` (same in A and B).
- Start screen and "Where is my save?" as drawn ("Where is my save?" liked as is).
- About: say "MIT License"; do not advertise "free".
- Keyboard shortcuts: not the drawn "● Press keys…" chip (looks generic). Chosen (A, like VS Code): clicking a
  shortcut opens a small centered box "Press the new keys for <action>"; the keys show large while held; Enter saves,
  Esc cancels; a conflict is explained inside the box.
- Mockup content that was invented must be replaced with real data: the shortcut list (use the app's real shortcuts),
  the exe path (`saveprism-desktop.exe`; Copy setup fills the real path), notice versions, engine hints.

## Start screen and header
- Keep the big open/drop area in the middle. Below it: an FAQ section (useful questions with short how-to answers).
- Header on the start screen: remove "Paste code" / "Open file" (already in the middle) and the theme toggle.
  Keep language, notifications, settings.
- Replace "Files stay on this PC. A backup is made before every save." with a short slogan in the right places:
  "Your files stay on your PC. Your data is safe." (no backup wording: it makes players ask where backups go).
- First launch: a short tour that points out where things are.
- After each update: a "What's new" popup the player can close, with the release notes. The notes are built into the
  app from the release text at build time (no network call, works offline, keeps "nothing leaves the PC").

## Language
- Header language button and the Settings language list show round (sphere) flag icons plus the language's own name
  ("Türkçe", "English"; English uses the UK flag). Windows cannot draw flag emoji, so use small images (check the
  license of the flag set: MIT/permissive only).
- In Settings the language is a dropdown (more languages later).

## Settings window: tabs on the left, inside the same window (like Windows Settings)
1. General: language (dropdown), theme System / Light / Dark (default System, follows the OS).
2. Appearance:
   - Text size as a percentage slider (fine steps), replacing Small/Normal/Large.
   - Colors: presets plus a free color picker for each default color; no contrast warning; text color switches
     automatically to stay readable on the chosen background; "Reset to default".
   - Background: Pattern (Grid = plain squares, Default = our nested squares, Dots) or Photo (pick an image, copied
     into the app's folder; Fit / Fill; blur slider, default 30%; a darken slider so text stays readable).
3. Saving and backups:
   - Default save action (moved here).
   - Backups to keep: a slider, minimum 1 (the newest backup always stays, so a bad write can never lose the save;
     the leftmost position means "only the latest"), up to 50, default 10.
   - Keep backups for N days (default 30); age-based cleanup never deletes the newest backup of a save.
   - Say it under the controls: "The latest backup of each save is always kept, so you can always go back."
   - Backup folder: choose where backups go (never the save's own folder).
   - The app's own folder (backups, pasted codes, settings): open it in Explorer and change it.
4. AI connection: two paths explained step by step with a visual stepper: (a) copy for a chatbot (paste into
   ChatGPT/Claude), (b) connect an AI agent over MCP (per-client setup with copy buttons).
   MCP gaps to close AFTER the polish (maintainer, 2026-10-27; they do not depend on the UI): the app can compare
   two saves, list "Where is my save?" folders and open a pasted save code, but the MCP server cannot; add
   `compare_saves`, `save_locations` and `open_code` tools (same Session code, read-only, documented in `docs/ai.md`),
   and check that `open_save` uses the same automatic key finder as the app.
5. Keyboard shortcuts: every action rebindable. Click to start recording, press the combination, recording stops;
   at most 3 keys per shortcut, at most 2 shortcuts per action; conflict warning; "Reset to default". The Windows key
   and system combinations (Alt+F4, Ctrl+Alt+Del) are not offered: Windows handles them before the app sees them.
   A short note in the tab tells the player so. (Catching them would need a system-wide keyboard hook: antivirus
   tools treat that like a keylogger and it breaks Windows' own shortcuts, so no.)
6. About: like a phone's About page — logo, version, links. Third-party notices open in an in-app scrollable list
   (today the entry looks like a web link but goes nowhere).

## Other screens
- Custom scrollbar instead of the default Chromium one, in the app's style.
- "Where is my save?": a search box, grouped by engine / game type rather than by game name, and a
  "Your game is not listed? Tell us" entry (GitHub issue; works once the repo is public). Mockup first.
- Keep the UI easy for people used to ordinary apps: familiar layouts, clear labels.

## Motion and branding
- An animated logo when the app opens (for example the logo drawing itself). Installer screens and Windows
  notifications cannot animate: they get the static logo.
- Animation libraries must be permissive (CSS, Motion/MIT, lottie-web/MIT); GSAP's license is not, so not GSAP.
- The design/skill repos the maintainer found were analyzed (2026-10-27, report `.motor/design-repos-report.md`, local).
  Adopted as our own rules (no packages, no skill packs installed):
  - Opening logo: plain CSS/SVG stroke animation (no library, starts on first paint, follows theme colors), shown only
    while the app loads; it never delays or blocks input. (motion-design-skill, comparison in the report)
  - UI motion: 150-200 ms, ease-out entrances (e.g. scale 0.96 → 1), fast ease-in exits; never block input; follow
    Windows' "show animations" setting (`prefers-reduced-motion`). (genjutsu, motion-design-skill)
  - Dialogs and Settings: 4/8 px spacing rhythm, visible keyboard focus, full keyboard navigation, Esc closes, focus
    stays inside the dialog. (impeccable)
  - Start screen FAQ and About: tight type hierarchy, no generic three-card templates. (taste-skill)
  - Theme files for import/export: a typed JSON schema (colors, pattern, photo settings), validated on import, data
    only. (design-dna, open-design token ideas)
  - Not used: react-particle-image (abandoned, constant CPU), GSAP and gsap-skills (license), Three.js (size, GPU),
    get-shit-done (workflow tool; our NOW.md process covers it).

## Windows integration
- Explorer right-click entry with the logo, added by the installer: TR "SavePrism ile çöz", EN "Decode with
  SavePrism" (on Windows 11 it sits under "Show more options"; the modern top-level menu is not needed).

## Community
- v0.1: export/import a theme as a file in Settings; a "Community" link to GitHub Discussions, where people share
  themes and notes on how to open game saves (shared "recipes" are data only, never code). No server, no accounts.
- Later (out of scope now): if the community grows, a website, showing shared themes/recipes inside the app, donations.
