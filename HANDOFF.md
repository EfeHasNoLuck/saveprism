# HANDOFF

**State (2026-10-27, ~18:10 TR):** polish done. Today on `main` (local only, not pushed; GitHub Actions minutes are out,
user: "commit tut, push atarız sonra"; last pushed `4214786`): Satisfactory codec + inventory view, the visual-check
polish, and the fixes from a player audit of the four focus engines (details and commits in `NOW.md`, todo 6, and
`.motor/audit-1027/REPORT.md`). Every merge: kontrol rust + web 3/3, `cargo deny check`, ignored real-save tests.
**Next:** ask the user about publishing. Recommended first: build a fresh MSI from `main`
(`pnpm -C apps/desktop tauri build`), the user installs it (`msiexec /i ... /l*v <scratch log>`, they click) and opens a
few real save copies from `.motor/realtest/`; then publishing (public repo + release) only with their explicit OK.

**User decisions:** focus on Unreal, Unity, RPG Maker, Ren'Py; other engines only through issues (memory
`focus-engines`). No value finder; quiet backend UX; at most 2 EVREN jobs at once; never touch the user's original
saves (copies only). Weekly Claude limit was at 81% on 2026-10-27: prefer EVREN for code chores.

**Works (evidence today):** CLI opens the 3 real Satisfactory saves exactly; the user opened one in the app (raw tree →
view added since, not opened by the user yet). Audit fixes checked in the running app by a subagent (screenshots
`.motor/polish2-1027/`), except the last small-fixes commit `329e69d` (unit tests only).

**Not done / known:** audit leftovers listed in `NOW.md` Known gaps (Set all only for RPG Maker, ES3 trash icons, Ren'Py
internals, Palworld `PlayerUId` editable, Palworld `Level.sav` raw in debug builds only).
**Gotchas:** Claude worktree subagents start at origin/main (pushed) — brief them to `git merge --ff-only main` first.
Leftover folders the hook won't delete (Filename too long; user can delete by hand): `../saveprism-wt/satisfactory`,
`.claude/worktrees/agent-a0b6a9b8ab82d0764`, `agent-ab7b66a783b5d7813`, `agent-a5b61daf70f35e62e`,
`agent-a945e990b19f4bff1`, `agent-a15308e8a9ffff604`.
