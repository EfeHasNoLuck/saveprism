---
name: devret
description: Handoff between sessions. Use at a feature end, at about 70% context, or when the user says "devret". Writes one HANDOFF.md, updates NOW.md, commits, pushes, then rolls over (desktop) or tells the user to /clear (Remote Control).
argument-hint: "[reason: context | feature done | quota]"
---

# Handoff

1. Update `NOW.md` (Done / Todo) so it is true right now.
2. Overwrite `HANDOFF.md` (one page, English):
   - **State:** one or two lines, last commit hash.
   - **Works (with evidence):** command + result, or what the user saw.
   - **Broken / not checked:** plain list.
   - **Next step:** the first todo, concrete.
   Drop any line you cannot back with evidence.
3. `git add NOW.md HANDOFF.md` (+ any verified work) → commit `docs: handoff` → push.
4. Roll over:
   - User typed at the desktop → `start_session` (fresh, same folder, prompt "Read HANDOFF.md and continue."), `detach_session`,
     then `archive_session self`. One open session at the end.
   - User on Remote Control (start_session/clear are refused) → tell the user in one line:
     "/clear yaz, sonra şunu gönder: HANDOFF.md'yi oku ve devam et."
