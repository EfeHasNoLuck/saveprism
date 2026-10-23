# HANDOFF

**State (2026-10-23):** NOW.md todo 1 (polish) is done; todo 2 (parity with saveeditonline.com) is in progress.
Last code commit `0aaaeed`. All pushed to the private repo.

**Works (with evidence)**
- `bash .claude/araclar/kontrol.sh rust` 3/3 and `kontrol.sh web` 3/3 after `3cee490`.
- `4d1db11` Backups dialog (list + restore; current file backed up first; backup must open; bytes checked) and "Set all" per
  RPG Maker list. Codex review: no critical/high; symlink backup fixed. Checked over CDP (dark/light, EN/TR).
- `115286c` Twine SugarCube 2 story view (variables edited at their last writer in `state.delta`). Synthetic fixtures only.
- `3cee490` Paste code / Copy code. Codex review finding (link in the pasted folder) fixed. Checked over CDP.
- Format notes: `docs/research/formats-sugarcube-tyrano-renpy.md` (SugarCube part corrected from the source).
- `0aaaeed` Ruby Marshal codec (RPG Maker XP/VX/VX Ace): exact round trip on synthetic fixtures, scalar edits, opens in
  the generic Values list; Codex review 2 mediums fixed. Fuzz target only compile-checked (no cargo-fuzz here); no real
  XP/VX/VX Ace save tested (none with an open license found).

**Broken / not checked**
- `CLAUDE.md` is changed but not committed (new goal line + "chores go to external engines first"); the user commits it.
- No real SugarCube save tested; SugarCube 2.37 `state.history` saves (no deltas) open as the plain Values list.
- On the dev PC something outside SavePrism alters clipboard text over ~1,400 characters; a long copied code pasted back
  there fails. Not an app bug.
- Remote Control is disabled by the organization's policy (user tried to turn it on, 2026-10-23).

**Next step**
NOW.md todo 2, next: RPG Maker XP/VX/VX Ace sections on the Marshal tree (gold, items from the `@items` Map keys, actors,
variables, switches; names from a plain `Data/*.rvdata2` if present; `marshal::ruby_class(node)` gives class names), like
the MV/MZ view. After it: TyranoScript (URL-escaped JSON), Ren'Py (zip + pickle as data only; signature prompt can be accepted by the
player), Godot, Wolf RPG, add a value from the card view, pick the data folder by hand.
How chores went today: Claude subagents built the UI/codec pieces with a visual check over CDP; Codex `gpt-6-luna`
read-only reviews were useful; agy research needs the brief on stdin (see memory `external-engines`).
