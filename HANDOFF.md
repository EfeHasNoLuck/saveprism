# HANDOFF

**State (2026-10-26, ~22:45 TR):** todo 6 (parity with saveeditonline.com `/formats`), wave 3. Last commit is this
handoff, on top of `3b15435`. **Not pushed:** GitHub Actions minutes ran out on 2026-10-26 (jobs fail in ~5 s with
"recent account payments have failed or your spending limit needs to be increased"). Option B (memory
`ci-minutes-fallback`): commit locally, run kontrol rust + web + `cargo deny check` before each commit, push next month.
Last pushed commit: `4214786`.

**Works (with evidence)**
- Merged since the last handoff, each after `kontrol.sh rust` 3/3, `kontrol.sh web` 3/3, `cargo deny check` all ok:
  Terraria `.plr`/`.wld` (`e770de5`) + review fixes (`2d08633`: world name/id/size read-only, 8-char key candidates,
  item id/difficulty checks, `.plr.bak`, weak detector guess no longer beats the key path); NRBF fuzz OOM fix
  (`58ccdd3`, fuzz CI green after it); "Copy file report" on the unknown-format toast (`585479e`); new S mark in the
  header/About (`6e26f1a`); BioWare GFF/GFF4/ERF (`29f6677`); zstd 0.14.3 for RUSTSEC-2026-0391/0398/0403
  (`dcfa9c1`); Adventure Creator + Pixel Crushers (`4214786`); AGS, ONScripter, Emuera (`ccd64b1`); BND4, MPQ,
  Brickadia BRZ/BRS + `blake3` + regenerated notices (`31a1e50`).
- Logo pack in `docs/brand/`, app icons regenerated from `SavePrism-icon-navy.svg` (`60439a5`, CI green then).
- Visual check over CDP 2026-10-25 (dark/light, EN/TR): Open folder, translated error toasts, Compare polish,
  password focus all pass.

**Running (EVREN, at most 2 at once; logs stay empty until the end, progress in
`~/.local/share/opencode/log/opencode.log` by cwd)**
- `kvdb` (LevelDB, RocksDB, LiteDB) in `../saveprism-wt/kvdb`, session `ses_e86715b09ffeOB45I3YvblrKqF`, log
  `.motor/w3-kvdb-resume.log`, report `.motor/w3-kvdb-report.md` (in the worktree).
- `ser` (protobuf, Bitser, SCS .sii, LZip, Base64URL, Snes9x/RetroArch states) in `../saveprism-wt/ser`, session
  `ses_e86532ac0ffeSlSRfw1oWFh4HC`, log `.motor/w3-ser-resume.log`.
- If `tasklist | grep opencode` shows nothing and there is no report: resume with
  `bash .motor/resume_job.sh <kvdb|ser> <session> now` (start runs seconds apart: same-second starts fail with
  "database is locked"). A run stops for good when it touches a path outside its worktree; the prompt must name it.

**Merging a job:** `python -I .motor/merge_wt.py <name> "<subject>"` (keep it out of any command that mentions
`.claude/araclar`: the hook refuses the mix), then check union hunks: locale JSON (script now adds the comma; still
look for duplicate keys), `locations.rs` rows and test functions in `tests/detect.rs` can be cut in half; a job may
edit NOW.md (keep ours). Then kontrol rust, kontrol web, `cargo deny check` as separate commands; regenerate notices
(`bash scripts/third-party-notices/generate.sh`) when a crate was added; commit locally.

**Broken / not checked**
- Every new format since wave 2 is tested only with synthetic files; no real Terraria/BioWare/AGS/etc. save.
- "Copy file report" not clicked in the running app; the new header S mark seen only in the vite preview.
- Any unknown file of 16·n bytes opens the ES3 password dialog; wrong password empties the field.

**Next step:** merge `kvdb` and `ser` when their reports appear; then write and run the last wave 3 briefs: vn3
(Artemis, Smile Game Builder/YUKAR `.sgs`, LiveMaker, AliceSoft, Visionaire, Construct Classic, Aoi/SofthouseChara)
and the Witcher 3 Gwent collection view (UI: design-heavy, Claude subagent + A/B screenshots to the user). After that
one CDP visual check of everything new, then the proof steps (install for real, real saves, real AI client).

**User decisions:** do not ask about publishing until install + real saves + real AI client pass; no value finder;
quiet backend UX; at most 2 EVREN jobs at once (quota ran out at 14:51 TR with 4); no pushes while CI minutes are out.
