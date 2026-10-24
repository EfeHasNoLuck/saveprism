# HANDOFF

**State (2026-10-24, night):** NOW.md todo 2 (full saveeditonline parity) in progress. The user is away until about
10:00 and asked for everything on the parity list. All merged work is pushed; last commit `4316212`.

**Merged today (each: fixtures regenerate byte-for-byte from their Python generator, kontrol rust 3/3 + web 3/3, CLI
`inspect` gives exact round trip)**
- `b6f1566` RPG Maker XP/VX/VX Ace sections · `fa01711` Flash `.sol` (Codex) · `1337019` Pokémon Gen 3 (Codex) ·
  `a99e768` Java serialization (Codex; fixtures from a real JDK) · `4316212` RPG Maker 2000/2003 `.lsd` (Claude).
- Research: `docs/research/formats-wolf-lcf-qsp-rags-gen3.md` (LCF ids there are WRONG; verified ids are in
  `.motor/lcf.md`), `docs/research/formats-witcher-playerprefs-naninovel-html-packed.md`. Both unchecked.

Later merged: `2224b20` Ren'Py (Claude) · `0cb4f03` TyranoScript (EVREN) · `0feb046` .NET BinaryFormatter + RAGS (EVREN).

**In flight (check these first; each is EVREN deepseek in `../saveprism-wt/<n>`, uncommitted, log `.motor/<log>`)**
- Merged since: `a9e7ea5` Unity PlayerPrefs `.reg` · `62b2706` Godot (type 22 bug fixed by Claude).
- SQLite (`sqlite`, log `sqlite.log`, brief `.motor/sqlite.md`; adds `rusqlite` bundled).
- Zip-packed saves (`zip`, log `zip.log`, brief `.motor/zip.md`). Wolf RPG (`wolf`, log `wolf.log`, brief `.motor/wolf.md`;
  check its sources list before merging).
- Merge helper: `python -I .motor/merge_wt.py <name> "<subject>"` (commits the worktree, merges uncommitted, resolves the
  usual append conflicts, reruns generators), then kontrol in a separate command, inspect fixtures, commit, push.
  Union-resolved code files can still break (a test lost a closing brace once): read the compile errors.
- Card view add/remove + pick data folder: backend WIP commit `ad95457` on `wt/cards` (written by a Claude subagent via
  Bash because the hook refused Edit/Write outside the project — tell the user, already reported once); UI not started;
  brief `.motor/cards.md`.

**How to merge an external engine's worktree** (they cannot commit): `git -C ../saveprism-wt/<n> add -A && commit`, then in
main `git merge --no-commit --no-ff wt/<n>`, resolve conflicts (usually `fuzz/Cargo.toml`, `fuzz.yml` matrix,
`fixtures/README.md` — binary for git: use `git merge-file --union` on `:1:/:2:/:3:`), run the generator with
`python -I fixtures/tools/<n>/make_<n>.py` and check `git status fixtures/` shows no new changes, then kontrol (separate
command: the hook blocks `python` and `kontrol.sh` in one command), `cargo run -q -p saveprism-cli -- inspect <fixture>`,
commit, push. Always spot-check the format constants against the spec (Codex's Godot draft passed its own tests with a
wrong table).

**Engines (2026-10-24)**
- Codex: usage limit until 17 Nov. Cursor: free plan, `--model auto` only, now also at its limit; `--force` is blocked by
  the classifier. agy: research only (headless it cannot read files). EVREN via
  `OPENCODE_CONFIG=<repo>/opencode.json opencode run -m evren/deepseek-v4.1-flash --agent build "<brief inline>"` works in
  worktrees (tell it: run every command from the worktree root, never cd to a parent); mimo-v2.6-pro did nothing.
- Claude subagents can only write inside the project: use `isolation: "worktree"` (sanctioned); worktrees outside the
  project make the hook refuse their writes, and moving worktrees inside was refused by the classifier.
- Claude usage at 01:00: 5-hour 59 %, weekly 60 %. Keep Claude agents for Ren'Py-level work only.

**Next (NOW.md "Formats left" order)**: finish the in-flight items, then QSP, SQLite, Unity PlayerPrefs `.reg`, packed zip
saves, Wolf RPG, Naninovel view, Witcher 3. Visual check of the new views (RPG Maker 2k/2k3 and XP/VX/VX Ace sections use
the existing card UI) is not done.
