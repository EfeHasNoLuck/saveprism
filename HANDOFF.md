# HANDOFF

**State (2026-10-25, ~04:00):** wave 1 of todo 6 (full parity with saveeditonline.com) is fully merged and pushed
(`fe4f891` layers, `12dfc08` Ren'Py follow-ups, `752ae91` Where is my save? + Compare, `bede97c` structured text). Every
merge passed `kontrol.sh rust` 3/3 and `web` 3/3. Wave 2 runs on EVREN overnight. Main is clean except git-ignored
`fixtures/external/` and `.motor/`.

**Running when this was written**
- EVREN (deepseek-v4.1-flash), one worktree each, briefs `.motor/w2-common.md` + `.motor/w2-<name>.md`, logs
  `.motor/w2-<name>.log`, reports `../saveprism-wt/<name>/.motor/w2-<name>-report.md`:
  `unlock` (layers by content, XOR auto-break, LZF), `keys` (ES3 password from the game's own files; touches keys →
  Claude review before merge), `unity` (Odin, Easy Save 2), `enc` (Shift-JIS/GB18030/CP949/UTF-16/32). Started
  00:44-00:47 UTC; opencode sessions: unlock `ses_e8f7c068effed7iUZ9ENzfl63U`, keys `ses_e8f7b1a6dffeh13gHFaNaon4Vh`,
  unity `ses_e8f7a3221ffeW6dUlWm52vBmKq`, enc `ses_e8f79482affeJ7oo3yToujoMKt` (resume one with `--session <id>`).
  They were left running when Claude stopped at 99% weekly (they cost no Claude quota and never touch main).
  Already merged: unlock (`9d50e17`), unity (`73ebf7a`). Left: keys, enc.
- Visual check of Where is my save? + Compare done (NOW.md known gaps); its three real bugs fixed in `7df9b4b`, but
  Open folder was not clicked again in the app.

**How to merge a wave 2 job:** report present → `python -I .motor/merge_wt.py <name> "<subject>"` → check union hunks
(`git diff --cached`; the union can duplicate `pub use` lines) → `bash .claude/araclar/kontrol.sh rust` then `web` (separate
commands) → commit, push → `git worktree remove ../saveprism-wt/<name>` + `git branch -d wt/<name>`. A job that adds a crate:
regenerate notices (`bash scripts/third-party-notices/generate.sh`; new license ids go in both `deny.toml` and
`scripts/third-party-notices/about.toml`). If a job stopped half done: `opencode run --session <its ses_ id>` (find it in
`~/.local/share/opencode/log/opencode.log`, line `created id=ses_… directory=…saveprism-wt\<name>`); `--continue` picks the
newest session of the whole project, not of the worktree.

**EVREN gotcha found tonight:** `opencode run` hangs forever at startup unless stdin is closed: always add `< /dev/null`
(pipe output through `| cat > log`). The daily token limit resets at 00:00 UTC (03:00 here).

**Broken / not checked:** Ren'Py signing not tried in a real game; installer not installed for real (no Windows Sandbox).
A Claude worktree under `.claude/worktrees/` may be left as a folder (deleting it hit Windows long paths); it is not
registered and git-ignored.

**Next:** merge wave 2 jobs as they finish (review `keys`; visual check of the password dialog's "Pick the game
folder…"), then the rest of wave 2 in NOW.md, then wave 3.

**User decisions:** do not ask about publishing until the app is proven; no value finder / memory scanner; automatic work
stays invisible in the UI; for game keys, look for the game folder ourselves and ask only if not found. 2026-10-25 (one
time): use the second account's Claude quota overnight and stop safely at 99% weekly.
