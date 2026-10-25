# HANDOFF

**State (2026-10-25, ~14:25):** todo 6 (parity with saveeditonline.com): wave 1, wave 2 and the first wave 3 batch are
merged. Last pushed `4336ee0` (CI green: ci, cargo-deny, fuzz); `9ed41ca` + this handoff commit go with the next push.

**Works (with evidence)**
- Every merge today passed `bash .claude/araclar/kontrol.sh rust` 3/3 and `web` 3/3; `cargo deny check` all ok before
  the last push; GitHub runs for `1cabd55` and `4336ee0`: ci, cargo-deny, fuzz all success.
- Merged today: layers by content + XOR + LZF (`9d50e17`), Odin (`73ebf7a`), text encodings (`9ef817e`), ES3 key finder
  (`60a36d2`, reviewed twice, no critical/high), password dialog focus (`b04c4ac`), fuzz OOM fixes + dependency
  advisories (`1cabd55`), visual-check polish + translated errors (`9501734`), KiriKiri/GameMaker (`fabf118`), PS1/PS2
  cards, .psu, SharkPort (`033a67b`).
- Password dialog (Pick the game folder, "No password found in that folder.", wrong/right password, Esc) checked in the
  running app over CDP, dark/light, EN/TR.
- Release build of `b04c4ac`: `target\release\saveprism-desktop.exe` + `bundle\nsis\SavePrism_0.1.0_x64-setup.exe`
  (the user is taking screenshots with it for a logo; it predates the polish and new formats).

**Running**
- EVREN `terraria` in `../saveprism-wt/terraria` (brief `.motor/w3-terraria.md`, log `.motor/w3-terraria-2.log`,
  opencode session `ses_e8d94405bffevqyIRK3YVEU3ov`, active at 11:21 UTC). It touches keys (the `.plr` key comes from
  the key finder) → Claude review before merge. When its report `.motor/w3-terraria-report.md` exists: merge with
  `python -I .motor/merge_wt.py terraria "<subject>"`, check union hunks (layer/codec counts, duplicated `pub use`;
  `fixtures/README.md` is binary in git: rebuild from main + the job's new rows), kontrol rust + web (separate
  commands), `cargo deny check`, commit, push once, then `gh run list --limit 4` until green.

**Broken / not checked**
- Not re-checked in the app after their fixes: Open folder (`7df9b4b`), polish + translated toasts (`9501734`),
  password field focus (`b04c4ac`). Needs one CDP visual check (memory `visual-check-localstorage`).
- Ren'Py signing not tried in a real game; installer not installed for real; most formats only tested with synthetic
  files.

**Next step:** merge terraria; then start the next EVREN batch from NOW.md wave 2/3 (Adventure Creator + Pixel Crushers
— stop at read-only if no public layout; Witcher 3 Gwent view; AGS, Artemis, Smile Game Builder, NScripter, Emuera,
LiveMaker; BioWare ERF/GFF, BND4, MPQ; LevelDB/LiteDB; local "unsupported file" report), at most 4 jobs at once, briefs
in `.motor/` (copy `w2-common.md` + the job brief into the worktree), always `opencode run ... < /dev/null`.

**User decisions:** do not ask about publishing until install + real saves + real AI client pass; no value finder; quiet
backend UX; look for game folders ourselves. 2026-10-25: user handles the malware cleanup elsewhere (memory
`malware-2026-10-25`); failed CI runs email the user, so run cargo deny before pushing, check runs after, batch pushes
(memory `ci-mail-noise`); check context every user turn and hand off at ~70%.
