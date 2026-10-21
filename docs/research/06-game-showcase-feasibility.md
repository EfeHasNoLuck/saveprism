# 06 — Game Showcase and Catalog: Feasibility

> **TL;DR:** Feasible with no network and no server of our own. Ship a Wikidata (CC0) game-name snapshot in the app, keep a local "My games" list, and let the user publish a pick through a prefilled GitHub issue.
> An anonymous POST endpoint adds legal, abuse and trust cost for little launch gain. Keep it as an optional later phase behind an ADR.
> Blockers found: RAWG forbids redistribution, IGDB and Steam need secrets or restrict storage, adult titles need a hide-by-default rule.
>
> **Next:** the maintainer answers D1–D5 in §8. No code before M7.

Date: 2026-10-21. Method: official docs and ToS pages only (no keys, no dumps, no sign-ups). "Unverified" = not confirmed from a primary page in this run. This is research, not legal advice. Sources are numbered [S#] in the list at the end.

---

## 1. Catalog sources

| Source | License / ToS | Key or proxy | Engine field | Adult flags | Limits | Snapshot OK? |
|---|---|---|---|---|---|---|
| Wikidata | Structured data is CC0 [S4] | None. Send a User-Agent [S5] | P408 "software engine"; usage report shows 22,773 in its "M" column (legend not shown), partial [S2][S3] | No verified flag (unverified) | SPARQL: 60 s deadline, 60 s CPU per 60 s, 5 parallel queries per IP [S5] | **Yes** |
| VNDB | ODbL + DbCL. Images and descriptions may carry other licenses [S7]. Free for non-commercial [S6] | None for reads [S6] | `release.engine`, free-text string, nullable [S6] | `has_ero`, `uncensored`, `minage` 0–18, tag category `ero`, `image.sexual` [S6] | 200 req / 5 min [S6]. Daily dumps [S8] | **Yes, ODbL duties.** Visual novels only: 67,364 [S9] |
| IGDB (Twitch) | Twitch Developer Services Agreement. Free non-commercial; commercial needs a partnership (secondary source) [S11]. Official docs returned HTTP 403: unverified | Twitch client ID + secret. Secret must never reach users "even in an obscured form" [S10]. Build-time or proxy only | `game_engines` endpoint exists (Go wrapper lists the type) [S11]. Coverage unverified | `themes` and `age_ratings` exist [S11]. Erotic theme ID unverified | 4 req/s, 8 open (secondary) [S11] | **Unverified. Do not ship** |
| RAWG | Free = non-commercial, backlink on every page, "No data redistribution" [S1] | Key on every request [S1] | Not documented (unverified) | Not documented (unverified) | 20,000 req/month free [S1] | **No** |
| Steam | Web API ToU: 100,000 calls/day; data for users' "personal use via your Application"; delete all Steam Data on termination; key confidential [S12]. Subscriber Agreement §4.C bars bots and scripts on Steam services [S15] | Key for many methods [S13]. `appdetails` is undocumented: unverified | None | Content descriptors come from the Content Survey [S14]. API exposure unverified | 100,000/day [S12] | **No.** Use the App ID as an identifier only |
| MobyGames | Free key for not-for-profit; commercial must register; 360 req/hour (search summary; page returned 403) [S16] | Key | Unverified | Unverified | 360/hour [S16] | **No / unverified** |
| itch.io | Server-side API is account-scoped and needs a key; public browse via RSS only [S17]. ToS bars "harvesting information about others" [S17] | Key | Not in API docs | Tag pages exist (e.g. `tag-erotic`) | `robots.txt` disallows `/search` [S17] | **No** |

Findings:
1. **The catalog does not need an engine field.** SavePrism detects the format locally (M7-1). The catalog only has to answer "which game is this?". Engine data can rank suggestions, nothing more.
2. Engine coverage elsewhere is incomplete. A SteamDB-based detector reported Unity 29,226, Unreal 7,405, GameMaker 2,585, RPG Maker 1,942 titles in 2021 and warned of false negatives [S18]. SteamDB terms: unverified (page returned 403).
3. PCGamingWiki has engine data but its content license was reported as CC BY-NC-SA 3.0 (Wikipedia, ~2022; unverified today) [S19]. NC terms do not fit a shipped MIT app. Skip.
4. Best fit: **Wikidata as the base**. Optional enrichment from **VNDB** for Ren'Py/visual novels and adult flags, at build time only. If VNDB data enters the snapshot, publish that data file under ODbL with attribution. The MIT code stays separate (legal reading unverified; ask counsel or decide in an ADR).
5. Steam App IDs are in Wikidata (P1733: 129,147 in the "M" column, includes non-games, report date and legend not visible) [S3]. That lets us link to a store page without calling Valve.

## 2. Offline-first

Yes. No server of our own is needed.

1. A scheduled GitHub Action runs chunked SPARQL queries (each under 60 s, at most 5 at once) and writes `catalog.json.zst` plus a manifest (version, date, count, sha256) [S5][S22].
2. The snapshot is bundled in the app resources and loaded by Rust. Search is local. Runtime network calls: none.
3. Refresh path 1 (default): rebuild the snapshot into every app release. The updater signature then covers it. Tauri's updater already checks a static `latest.json` and enforces signatures [S23].
4. Refresh path 2 (later, optional): app fetches the manifest from GitHub Pages and verifies sha256 plus a minisign signature. This is a second network call, so keep it off by default.
5. Size, **estimate, not measured**: 100k titles at ~45 bytes (QID, label, year, Steam ID, flags) is ~4.5 MB JSON and ~1.2–1.5 MB compressed at 3–4x. An engine-filtered subset of ~20k titles is ~0.9 MB raw and ~0.25 MB compressed.
6. Pitfall: scheduled workflows in public repos are disabled after 60 days without repository activity, and runs can be delayed at peak [S22]. Add a heartbeat commit or a monthly manual check.
7. Pitfall: Wikidata lags for new or niche games. Add a "game missing" issue template, which is option A below.

## 3. Report collection

| | A. No network, GitHub issue | B. Opt-in anonymous POST | C. Both |
|---|---|---|---|
| Participation | Low (assumption): needs a GitHub account | Higher (assumption): one click | Highest |
| Cost | Zero | Free tier: 100,000 requests/day [S20]. KV allows only 1,000 writes/day, so use D1 (100,000 rows written/day, 5 GB) [S20] | As B |
| Privacy | Nothing leaves the app. The user's GitHub name becomes public by their choice | Edge sees the IP. Workers Logs are on by default for new Workers, 3-day free retention: disable them [S20] | As B |
| Abuse | GitHub accounts slow spam. Action can check that the game ID is in the catalog and count distinct accounts | No identity, so "independent" cannot be enforced. Rate-limit binding is per location and approximate; free-plan availability unverified [S20] | As B |
| Moderation | Maintainer triage, partly automated | Threshold plus first-appearance review | Both |
| Legal load | README note only | Privacy policy, KVKK/GDPR notice, ADR (§4, §6) | Highest |

Moderation rules that work for both:
1. Catalog-only picks. No free text. Unknown game: use the "game missing" issue template.
2. Show a game publicly after at least 3 reports (A: from distinct GitHub accounts; B: speed bump only) or one maintainer-verified fixture.
3. Two tiers on the showcase: "Verified by maintainer (fixture, round-trip tested)" and "Reported by users". A public "works with" list must not tempt users to edit unverified saves (risk R1).
4. B must never transmit adult-flagged or unknown-flag picks (§4, §5).
5. Prefilled issues use `title`, `body`, `labels`, `template`; a too-long URL returns 414 [S21]. Keep the payload short.

## 4. Legal and privacy (research, not advice)

**GDPR and KVKK**
1. An IP address is an online identifier [S26: Recital 30]. For a website operator a dynamic IP can be personal data when legal means exist to link it to a person (CJEU Breyer, C-582/14; read through a secondary source, primary text not read in this run) [S26].
2. In B the maintainer, an individual, is the controller (inference). Cloudflare acts as processor, with SCCs and the EU-US DPF in its DPA [S20].
3. KVKK: explicit consent must be "freely given, specific and informed" (Art. 3). Notice is due when data is collected (Art. 10). Transfers abroad need a basis (Art. 9). Controllers must register (Art. 16) unless exempt [S27].
4. VERBIS exemption: reported as fewer than 50 employees and under TRY 100 million balance sheet, not processing sensitive data as the main activity (CMS summary of a 2023 Board decision; primary text unverified) [S27]. Confirm with Turkish counsel before launching B.
5. Option A avoids all of this: nothing is sent by SavePrism, and GitHub is the controller of the user's account data.
6. Adult-game picks linked to a person could reveal sex life or preferences. Treat as potentially special-category data (GDPR Art. 9, KVKK Art. 6; legal reading unverified here). Design so such picks never leave the device.

**Prompt design (EDPB Guidelines 03/2022)**
1. Scope: the guidelines address social media interfaces [S24]. For SavePrism they are a reference by analogy (inference), not binding text.
2. *Emotional steering*: colours and wording that push a choice [S24]. A bright "Yes" next to a grey "No" is a nudge toward sharing.
3. *Hidden in plain sight*: small or low-contrast text next to an eye-catching element [S24]. A faint "don't show again" link falls here.
4. *Continuous prompting*: repeated asks wear users down [S24]. Every refusal must stick.
5. Withdrawal must be as easy as consent (Art. 7(3)) [S24][S25]. Provide a Settings switch, one click.
6. **Equal weight:** both buttons same size, same shape, same text size and contrast. The checkbox label has body-text contrast. Bottom-left is fine if it is visible and works on its own (ticking it and closing counts as "No").
7. Separate the local benefit from public sharing. Step 1: "Add this save to My games?" (local, no consent issue). Step 2, only for A/B: "Tell others it worked?" unchecked by default.

**Trademarks and art**
1. Game names are trademarks. Nominative use to say which game works with the tool is the standard defense. US test: product not readily identifiable otherwise, only as much of the mark as needed, nothing suggesting sponsorship (*New Kids on the Block v. News America*, 9th Cir. 1992) [S29, secondary]. EU: Art. 14(1)(c) and 14(2) EUTMR require use in accordance with honest practices [S29]. Turkish law: unverified.
2. Wording: "Reported to work with SavePrism" plus app version and date. Never "official", "certified", "supported by". Add: "Not affiliated with or endorsed by any publisher. Names belong to their owners." Add a takedown contact.
3. Steam's API terms bar implying a Valve relationship [S12]. No Valve or publisher logos.
4. **Cover art is publisher copyright. Ship text only.**

**Hosting policy**
1. GitHub bars "sexually themed or suggestive content that serves little or no purpose", including graphic text, and may put such content behind an opt-in [S30]. Pages ToS restricts obscene sexual content [S22]. A plain title is not graphic, but keep adult titles out of the README and the default site.
2. **Minors:** GDPR Art. 8 applies only to consent-based services offered directly to children; a service clearly limited to adults sits outside it [S26][S25]. KVKK sets no fixed age [S27]. We cannot verify age, so collect no personal data and hide adult content by default.

## 5. Adult content

Classify with the union rule: **adult if any source says adult; safe only with positive evidence; otherwise unknown.**

1. Flags: VNDB `has_ero`, `uncensored`, `minage` = 18, `ero` tag [S6]. IGDB themes and age ratings (unverified, secondary) [S11]. Steam content descriptors from the Content Survey (API exposure unverified) [S14]. Wikidata: no verified flag.
2. Display: names only, no images. Adult and unknown titles never appear in suggestions, "recent", or browse lists. They appear only when the user types a title (3+ characters), tagged "18+".
3. Public showcase: adult and unknown entries hidden by default. An opt-in "Show 18+ titles" toggle (self-declared, no age check) reveals text-only rows. If the toggle ships, put it on a separate subdomain with the `adult` rating meta tag, as Google advises when a significant share is explicit [S31].
4. False negative (adult shown by default): GitHub and SafeSearch exposure, reputation. Mitigation: unknown stays hidden, report button, fast removal.
5. False positive (safe game hidden): low harm. Accept.
6. A clean title can still hide explicit content (mods, patches). No source detects this. Say so in the policy.

## 6. Positioning against "offline, no telemetry"

1. RISKS §3 item 4 says SavePrism will not "upload saves anywhere or collect telemetry by default". A user-triggered, previewed, opt-in report is not "by default". The Non-Goals list still needs an ADR if B ships, because changing the list requires one.
2. The app already makes one network call, the update check [S23]. README wording: "Works fully offline. Saves never leave your device. The only automatic call is the update check."
3. Examples of acceptable opt-in stats:
   - Home Assistant: opt-in, aggregated public results, payloads stored in Cloudflare KV for up to 60 days, sent data written to the local log [S28].
   - Debian popcon: opt-in package list, anonymized and published daily [S28].
   - Contrast: Homebrew is opt-out with a notice and no IP field in the payload [S28]. Opt-out would break trust in an "offline" tool.
4. Trust rules: default off, show the exact payload before sending, no install ID, no save contents or file names, Settings switch, source of the endpoint public.
5. Only claim what is true. "We do not log your IP" needs Workers Logs off and a check. Cloudflare still handles connection metadata in transit; say so in the policy.
6. Copy: "Nothing is sent unless you press Send. This opens your browser with a pre-filled GitHub issue. You see everything before it is submitted."

## 7. SEO

1. Demand is real: autocomplete is dominated by "<game> save editor" (05-naming §2).
2. Per-game pages are allowed but dangerous when thin. Google's spam policies target "doorway abuse" (pages made to rank for similar queries) and "scaled content abuse" (large amounts of unoriginal content with little value) [S31].
3. Rule: generate a page only for a game with a **verified profile** (M7-6): save path, format, tested app version, known fields, caveats. A page that is only a name from the catalog is not generated.
4. Build as static pages by a custom Pages workflow. Pages limits: 1 GB site, 100 GB/month soft bandwidth; the 10 builds/hour cap does not apply to custom workflows [S22].
5. Adult titles: no pages, or noindex on a separate subdomain with the rating tag [S31].
6. Fits M8 item 3 (GitHub Pages site and live format table).

## 8. Recommendation

**Phase 0 (M5-3, ⏱10 min):** README wording on offline and the update check only.
**Phase 1 (M7, zero network, ⏱≈7 h, about 2 sessions):**
1. Catalog builder Action + schema + tests ⏱120 min.
2. Snapshot loader + local prefix search in Rust ⏱90 min.
3. "My games" list + two-step prompt (§4) inside M7-7 library screen ⏱90 min.
4. "Share on GitHub" button + issue templates (report, game missing) ⏱45 min.
5. Adult union rule + 18+ tag ⏱60 min. Docs ⏱30 min.

**Phase 2 (M8-3, ⏱≈2 h):** moderation Action (catalog check, distinct accounts, threshold), `showcase.json`, Pages table with the two tiers.
**Phase 3 (optional, ⏱≈3 h + counsel):** Worker + D1 + policy + ADR, only if Phase 2 shows too few reports.
**Phase 4 (⏱≈2 h):** per-game pages from verified profiles (§7).

Estimates are mine, not measured. Do not start before M7: the verified tier needs real fixtures and profiles.

**Decisions for the maintainer**

| # | Option A | Option B |
|---|---|---|
| D1 Report channel | GitHub issue only. No ADR. Low participation, zero privacy load | Add anonymous POST. ADR, privacy policy, KVKK check. Higher participation, ongoing abuse work |
| D2 Catalog | Wikidata only (CC0). Thin coverage of visual novels, no adult flags | Wikidata + VNDB at build time. Better Ren'Py coverage and flags. Data file under ODbL with attribution |
| D3 Adult titles | Never published. Simplest, safest | Text-only 18+ section on a separate subdomain. More reach, more policy risk |
| D4 "Verified" tier | Maintainer fixtures only. Slow, strong claim | Fixtures or 3 user reports. Faster, weaker claim |
| D5 Catalog refresh | Rebuilt with each release. No runtime call | Optional in-app refresh. Fresher, second network call |

My picks: D1-A, D2-A (add VNDB later if visual novels matter), D3-A, D4-B with two labelled tiers, D5-A.

**Maintainer input (2026-10-21)**
1. In-app 18+ switch in Settings, off by default. Turning it on asks "Are you 18 or older?" (self-declared). When on, the local
   catalog lists every game, adult titles included. This settles the in-app side of §5; the public showcase (D3) stays open.
2. Network only on user action; the app works fully offline otherwise. Matches D1-A and D5-A.
3. Wants game images ("like apps that pull movie data from an API"). Note: IMDb's data access is paid; free movie apps mostly use
   TMDb with attribution (from memory, verify). New decision **D6 Images**:
   - A: no images, names only (today's §5 rule).
   - B: the user pastes their own free key (SteamGridDB or IGDB) in Settings; images load only on demand, cached locally; adult
     titles get images only when the 18+ switch is on. No secret ships in the app.
   - C: Steam store header image by App ID, no key. Steam ToU for images unverified.
   Research before M7: SteamGridDB API terms, IGDB per-user keys, Steam image terms.

## Risk table

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Catalog data license violation (RAWG, Steam, IGDB) | Medium | High | CC0 base only; ODbL file for VNDB; ask IGDB in writing before use |
| Adult title shown by default | Medium | High | Union rule, unknown hidden, text only, removal path |
| Prompt judged manipulative | Medium | Medium | Equal-weight buttons, sticky refusal, Settings switch |
| Trust loss ("offline" promise) | Medium | High | Option A first, opt-in only, payload preview, honest wording |
| Fake or spam reports | High (B) | Low | Catalog IDs only, threshold, manual first review, ban list |
| Trademark complaint | Low | Medium | Names only, nominative wording, disclaimer, takedown contact |
| Thin SEO pages penalised | Medium | Medium | Pages only for verified profiles |
| KVKK/GDPR duty missed in B | Medium | High | Defer B; counsel review; no IP stored; logs off |
| Showcase pushes unverified edits (R1) | Medium | High | Two tiers; round-trip guard stays; "reported" label with version and date |
| Scheduled Action silently stops | Medium | Low | Heartbeat commit; version and date shown in the app |

## Sources (all accessed 2026-10-21)

- S1 RAWG API: https://rawg.io/apidocs
- S2 Wikidata P408: https://www.wikidata.org/wiki/Property:P408
- S3 Wikidata property usage: https://www.wikidata.org/wiki/Wikidata:Database_reports/List_of_properties/all
- S4 Wikidata licensing: https://www.wikidata.org/wiki/Wikidata:Licensing
- S5 WDQS manual: https://www.mediawiki.org/wiki/Wikidata_Query_Service/User_Manual
- S6 VNDB API: https://api.vndb.org/kana
- S7 VNDB data license: https://vndb.org/d17
- S8 VNDB dumps: https://vndb.org/d14
- S9 VNDB statistics: https://vndb.org/
- S10 Twitch app registration: https://dev.twitch.tv/docs/authentication/register-app/
- S11 IGDB (secondary; official docs 403): https://api-docs.igdb.com/ · https://discuss.dev.twitch.com/t/commercial-use-of-igdb-api/23567 · https://macmyths.com/apps/igdb/ · https://pkg.go.dev/github.com/Henry-Sarabia/igdb
- S12 Steam Web API ToU: https://steamcommunity.com/dev/apiterms
- S13 Steamworks Web API: https://partner.steamgames.com/doc/webapi_overview
- S14 Steam Content Survey: https://partner.steamgames.com/doc/gettingstarted/contentsurvey
- S15 Steam Subscriber Agreement: https://store.steampowered.com/subscriber_agreement/
- S16 MobyGames API (search summary): https://mobygames.com/info/api
- S17 itch.io: https://itch.io/docs/api/overview · https://itch.io/robots.txt · https://itch.io/docs/legal/terms
- S18 Steam engine detector (2021): https://wnhub.io/news/marketing/item-3819
- S19 PCGamingWiki license (secondary): https://en.wikipedia.org/wiki/PCGamingWiki
- S20 Cloudflare: https://developers.cloudflare.com/workers/platform/pricing/ · https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/ · https://developers.cloudflare.com/workers/observability/logs/workers-logs/ · https://www.cloudflare.com/cloudflare-customer-dpa/
- S21 GitHub issue URLs: https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-an-issue · https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-githubs-form-schema
- S22 GitHub Pages limits and Actions schedule: https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits · https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows
- S23 Tauri updater: https://v2.tauri.app/plugin/updater/
- S24 EDPB Guidelines 03/2022 v2.0: https://www.edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-032022-deceptive-design-patterns-social-media_en
- S25 EDPB Guidelines 05/2020: https://www.edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-052020-consent-under-regulation-2016679_en
- S26 GDPR: https://eur-lex.europa.eu/eli/reg/2016/679/oj/eng · Breyer (secondary): https://arxiv.org/pdf/2511.01751
- S27 KVKK: https://www.kvkk.gov.tr/Icerik/6649/Personal-Data-Protection-Law · https://cms.law/en/tur/legal-updates/expanded-verbis-exemptions-for-small-scale-data-controllers · https://childreninthedigitalworld.twobirds.com/home/turkey
- S28 Opt-in stats: https://www.home-assistant.io/integrations/analytics/ · https://popcon.debian.org/ · https://docs.brew.sh/Analytics
- S29 Trademarks: https://eur-lex.europa.eu/eli/reg/2017/1001/oj/eng · https://en.wikipedia.org/wiki/Nominative_use
- S30 GitHub AUP: https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies · https://docs.github.com/en/site-policy/acceptable-use-policies/github-sexually-obscene-content
- S31 Google: https://developers.google.com/search/docs/essentials/spam-policies · https://developers.google.com/search/docs/specialty/explicit/guidelines
- Internal: docs/research/05-naming.md §2, docs/RISKS.md §3, docs/ROADMAP.md (M5, M7, M8)
