# HANDOFF

**State (2026-10-25, afternoon):** wave 1 and the first wave 2 batch of todo 6 are merged and pushed (last: `60a36d2`
key finder). Every merge passed `kontrol.sh rust` 3/3 and `web` 3/3. Main is clean except git-ignored
`fixtures/external/` and `.motor/`.

**Running**
- EVREN (deepseek-v4.1-flash), one worktree each in `../saveprism-wt/<name>`, briefs `.motor/w2-common.md` +
  `.motor/<brief>.md`, logs `.motor/<brief>.log`, reports `../saveprism-wt/<name>/.motor/<brief>-report.md`:
  `polish` (w2-polish), `vn1` (w3-vn1), `console` (w3-console), `terraria` (w3-terraria; touches keys → Claude review).
- A Claude visual check of the password dialog's "Pick the game folder…" (result goes in NOW.md).

**How to merge a job:** report present → `python -I .motor/merge_wt.py <name> "<subject>"` → check union hunks
(`git diff --cached`; unions can duplicate `pub use` lines; `fixtures/README.md` is binary in git: rebuild it from main's
version plus the job's new rows) → `bash .claude/araclar/kontrol.sh rust` then `web` (separate commands) → commit, push →
`git worktree remove ../saveprism-wt/<name>` + `git branch -d wt/<name>`. A new crate: regenerate notices
(`bash scripts/third-party-notices/generate.sh`; new license ids go in `deny.toml` and
`scripts/third-party-notices/about.toml`). Resume a stopped EVREN job with `opencode run --session <ses_id>` (ids in
`~/.local/share/opencode/log/opencode.log`, `created id=ses_… directory=…\<name>`). Always `< /dev/null` for opencode.

**Broken / not checked:** Ren'Py signing not tried in a real game; installer not installed for real; Open folder fix
(`7df9b4b`) not clicked again in the app.

**Next:** merge the four jobs as they finish, then the rest of wave 3 in NOW.md (AGS, Artemis, Smile Game Builder,
NScripter, AliceSoft, LiveMaker, Emuera, Construct Classic, Visionaire; BioWare ERF/GFF, BND4, MPQ; emulator states;
LevelDB/RocksDB/LiteDB; local "unsupported file" report), then the REA pass on saveeditor.online.

**User decisions:** do not ask about publishing until the app is proven; no value finder / memory scanner; automatic work
stays invisible in the UI; for game keys, look for the game folder ourselves and ask only if not found. 2026-10-25: the
user handles the malware cleanup elsewhere (memory `malware-2026-10-25`); stay on SavePrism.
