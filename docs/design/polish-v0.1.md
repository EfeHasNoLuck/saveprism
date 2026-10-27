# Polish for v0.1 — the maintainer's list (2026-10-27)

The maintainer's own visual and UX wishes for v0.1, with the decisions taken in the brainstorm. Work through them in
order; show mockups (A/B screenshots) before building a new screen.

## Start screen and header
- Keep the big open/drop area in the middle. Below it: an FAQ section (useful questions with short how-to answers).
- Header on the start screen: remove "Paste code" / "Open file" (already in the middle) and the theme toggle.
  Keep language, notifications, settings.
- Replace "Files stay on this PC. A backup is made before every save." with a short slogan in the right places:
  "Your files stay on your PC. Your data is safe." (no backup wording: it makes players ask where backups go).
- First launch: a short tour that points out where things are.

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
     the leftmost position means "only the latest"), up to 50.
   - Keep backups for N days (default 30); age-based cleanup never deletes the newest backup of a save.
   - Backup folder: choose where backups go (never the save's own folder).
   - The app's own folder (backups, pasted codes, settings): open it in Explorer and change it.
4. AI connection: two paths explained step by step with a visual stepper: (a) copy for a chatbot (paste into
   ChatGPT/Claude), (b) connect an AI agent over MCP (per-client setup with copy buttons).
5. Keyboard shortcuts: every action rebindable. Click to start recording, press the combination, recording stops;
   at most 3 keys per shortcut, at most 2 shortcuts per action; conflict warning; "Reset to default". The Windows key
   and system combinations (Alt+F4, Ctrl+Alt+Del) are not offered: Windows handles them before the app sees them.
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
- The design/skill repos the maintainer found (open-design, react-particle-image, get-shit-done, taste-skill,
  impeccable, awesome-design-md, genjutsu, motion-design-skill, design-dna, gsap-skills, threejs-skills): analyze each
  and adapt what fits SavePrism into our own docs; do not install third-party skill packs as they are.

## Windows integration
- Explorer right-click entry "Open with SavePrism" with the logo, added by the installer (on Windows 11 it sits under
  "Show more options"; the modern top-level menu is not needed).

## Open: community
- The maintainer wants a place where people share themes/customization packs and how to open game saves.
  Discussed scope in chat; not decided yet.
