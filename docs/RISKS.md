# Risks, Mitigations and Non-Goals

> **TL;DR:** Every known risk has an owner strategy. Risks we control (data loss, security, scope surprises) are **prevented by design**. Risks we don't control (endless formats, game updates, legal pressure) are **detected early and contained** so they never reach the user as a broken save.
>
> **Next:** review the Non-Goals list (§3) — it is the contract that prevents "that's not what I wanted" later.

Strategy legend: **Prevent** (make it impossible) · **Detect** (notice before the user does) · **Contain** (limit the blast radius) · **Accept** (known, documented, cheap to live with)

---

## 1. Risk register

| ID | Risk | Impact | Strategy | Residual risk |
|---|---|---|---|---|
| R1 | A user's save gets corrupted | Critical | Prevent | Very low |
| R2 | Endless long tail of formats | High | Contain | Unavoidable, but graceful |
| R3 | Game updates break support | Medium | Detect + Contain | Low |
| R4 | Maintainer burnout | High | Contain | Medium → Low with community |
| R5 | Legal pressure (keys, crypto, ToS, trademarks) | High | Prevent | Low |
| R6 | Security (malicious save files, supply chain, updater) | High | Prevent | Low |
| R7 | Bugs written by agents | Medium | Detect | Low |
| R8 | Distribution friction (SmartScreen, antivirus, Gatekeeper) | Medium | Contain | Medium until code signing |
| R9 | Scope surprises ("that's not what I wanted") | Medium | Prevent | Very low |
| R10 | Dependence on other projects | Low | Accept | None |

## 2. Mitigations

### R1 — Corrupted save (the one risk we treat as zero-tolerance)
1. Backup ring in the app data dir before every write (never next to the save).
2. Atomic replace: temp file → fsync → replace.
3. Round-trip guard on open: decode → encode must equal the original bytes, otherwise read-only.
4. Post-write verification: re-decode the written file and compare edited values; auto-restore on mismatch.
5. "Restore backup" is one click in the UI.

### R2 — Format long tail
1. **Support layers, not games.** Base64, zlib, AES, JSON… cover thousands of games without game-specific code.
2. **Recipe builder** lets users decode unknown files by hand; the recipe becomes a profile.
3. **Profiles are data** (TOML): the community adds games by PR without Rust.
4. **Support tiers** shown in the UI: Tier 1 (codec + golden tests, round-trip guaranteed) · Tier 2 (community profile) · Tier 3 (read-only / hex).
5. **Codec admission rule:** a new Rust codec needs an engine-level format, a sample with consent, and ≥3 independent requests (or one very popular game). Everything else goes through profiles.
Unknown files never fail silently: hex view + Diagnostic Report + "request format" issue.

### R3 — Game updates
1. Engine codecs (GVAS, ES3, pickle, Marshal) change rarely; breakage lands in **profiles**, which are data.
2. Profiles record the game version and a header signature; a mismatch shows a warning before editing.
3. The round-trip guard turns any unexpected change into read-only, not corruption.
4. Profile fixes ship without an app release (later: opt-in profile sync from the repo).
5. Every reported break becomes a regression fixture.

### R4 — Burnout
1. The agent factory does the heavy lifting; you decide at KAPI-1/KAPI-2.
2. Automation: release-please, CI, issue forms, labels, Dependabot. A release costs zero manual steps.
3. GitHub Discussions for questions, Issues only for bugs and format requests.
4. Contribution ladder: profile → translation → codec → reviewer → co-maintainer.
5. Published Non-Goals (§3) make "no" a link, not an argument.

### R5 — Legal
1. No key lists in the repo; profiles carry the *method* to find a key (ADR-009).
2. Keys are derived only from game files the user owns, locally.
3. Single-player saves only; no online games, anti-cheat, DRM or license checks.
4. Clean-room rule for competitor analysis (ADR-007); fixture consent policy; no game assets in the repo.
5. Game names are used descriptively only; `DISCLAIMER.md` + a documented takedown process (remove first, discuss after).

### R6 — Security
1. Rust core: memory-safe parsing of untrusted bytes.
2. Size/depth/ratio limits, no panics in parsers (lint-enforced), fuzzing for every decoder.
3. Save content is never executed (no unpickling, no Marshal eval, no BinaryFormatter).
4. cargo-deny + osv-scanner + CodeQL in CI; Dependabot.
5. Signed updates (minisign) and, after v0.1, code signing via SignPath.

### R7 — Agent-written bugs
1. "A report is a claim, not evidence": `kontrol.sh` output and tests are the evidence.
2. Reviewer is a different model than the coder; Codex second opinion on parsers, crypto and safe-write.
3. Every bug gets a failing test before the fix (regression corpus).
4. Quality strategy: [architecture/QUALITY.md](architecture/QUALITY.md).
5. Code map so fixes start in the right file: [MAP.md](MAP.md).

### R8 — Distribution friction
1. README explains the SmartScreen warning until signing lands.
2. VirusTotal link in each release; reproducible CI builds from tagged commits.
3. SignPath Foundation application after the first public release; macOS signing per ADR-012.

### R9 — Scope surprises
1. Non-Goals (§3) are explicit and change only through an ADR.
2. Irreversible decisions are asked as **A/B with consequences**, recorded in `architecture/kararlar.md`.
3. KAPI-1 (approve the plan) before any code; KAPI-2 (try it) before anything is called done.
4. Every milestone ends in a demo you can run yourself.

### R10 — Dependence on other projects
SavePrism stands alone. Apache-2.0 code may be adapted with attribution (no permission needed). A peer-level note to RPGSaveEditor goes out after v0.1 ([outreach note](research/outreach-rpgsaveeditor.md)).

---

## 3. Non-Goals (the scope contract)

SavePrism will **not**:

1. Edit online or multiplayer games, bypass anti-cheat, DRM or license checks.
2. Edit game memory or act as a trainer.
3. Ship game assets, encryption keys or other people's saves.
4. Upload saves anywhere or collect telemetry by default.
5. Decrypt console saves that need console-bound keys (PS4/PS5/Switch) in v1.

Changing this list requires an ADR.
