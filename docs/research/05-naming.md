# 05 — Naming Research (ADR-011)

> **TL;DR:** Recommendation: **SaveStack**. Runner-up: **SaveLore**. Save-R is available everywhere but its brand search collides with "Save-Rite" pharmacies and is heard as "saver".
>
> **Next:** the maintainer picks the name; then M0-11 renames the codename in one pass.

Date: 2026-10-21 · Sources: GitHub search, npm, crates.io, DNS (NS records via dns.google), Google autocomplete, web search.

---

## 1. What the name does — and doesn't — do

- People don't search for brand names they haven't heard of. They search for **"save editor" phrases** (§2). Those are won by the repo description, GitHub topics and, later, website pages.
- The name's job is different: be **memorable**, be **spellable after hearing it once**, and **own its brand query** (typing the name finds us first, not a pharmacy).
- Availability matters for trust: matching GitHub repo, npm, crates.io and a short domain.

## 2. Demand signals (free proxies)

| Signal | Finding |
|---|---|
| Google autocomplete for "save editor" | Dominated by `<game> save editor` (Deltarune, Stardew Valley, Palworld, Elden Ring, RPG Maker…) |
| "universal save editor" | Exists as a query, plus "universal unity save editor", "universal ue4 save editor", "universal save editor download" |
| "save editor offline" | Exists; "renpy save editor offline", "rpg maker save editor offline", "unity save editor offline" |
| Frustration signal | "save editor online not working" |
| GitHub topics | `save-editor` 863 repos · `savegame` 350 · `savegame-editor` 115 · `game-save-editor` 21 |

Implication: the description must say **"universal offline save editor"**; the name must be distinctive.

Real search volumes need a paid data source. `every-app/open-seo` (MIT, 24K★) works with a DataForSEO API key (pay-as-you-go, the maintainer's own account). It's worth using when the website is built (M8).

## 3. Candidates

| Name | GitHub exact-name repos | npm | crates.io | .app / .dev / .gg | Google brand query | Verdict |
|---|---|---|---|---|---|---|
| **SaveStack** | 2 empty repos (0★) | free | free | free / free / free | only "savestack site" | ✅ Recommended |
| **SaveLore** | none | free | free | taken / free / free | no suggestions | ✅ Runner-up |
| Save-R | none | free | free | free / free / free | "save-rite pharmacy", "save-rx" | ⚠️ Viable, weak brand search |
| OmniSave | 2 save-sync tools (same domain) | free | free | taken / free / free | "omnisafe" | ❌ |
| Savecraft | — | free | free | savecraft.gg is an AI save assistant | "savecraft gg" | ❌ Taken in our domain |
| SaveForge | 2 repos (0★) | free | free | taken / free / free | corrected to "saberforge" | ❌ |
| Quicksave | 69★ and 67★ repos | free | taken | taken / taken / taken | Baldur's Gate 3 results | ❌ |
| Resave · Savepad · Savix | small repos | mixed | mixed | mostly taken | "resaver", "savepod", "savix twitch" | ❌ |

No exact product named SaveStack or SaveLore was found by web search.

## 4. Why SaveStack

1. **Owns its search:** nothing else answers to "savestack".
2. **Everything free:** repo, npm, crates (`savestack-core`), `savestack.app` / `.dev` / `.gg`.
3. **Tells our story:** every save is a stack of layers (container → crypto → compression → format); SaveStack peels it and repacks it byte-for-byte.
4. **Easy across languages:** two common English words, said the way they're written.
5. **Tagline fits the demand:** "SaveStack — universal offline save editor".

Risk: "stack" sounds developer-ish to some gamers. The tagline and the logo carry the gaming side.

SaveLore is the warmer, more "gamer" option (lore = the story kept in your save), but it hints less at editing.

## 5. Before final adoption (maintainer, ≈15 min)

1. Free trademark search for the chosen name: TÜRKPATENT, EUIPO (TMview), USPTO.
2. Say-it-out-loud test with 2–3 people: can they type it after hearing it once?
3. Register the domain early if wanted (e.g. `savestack.app`). Purchasing is the maintainer's action.
