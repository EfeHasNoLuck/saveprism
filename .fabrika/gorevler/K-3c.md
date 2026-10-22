# K-3c — Real-game test saves

- **Dalga:** D9 · **Motor:** claude/general-purpose·sonnet (needs web + Bash) · **Effort:** medium · **Deneme:** 0/3
- **Amaç:** the user wants tests with real game data, not only our synthetic files: "gerçek oyundan verilerle test edelim. gidip bir
  yerden o test verilerini bul ve çek." Find real save files on the public web, download them, run SavePrism on them, report.
- **No git:** do not commit, push, or change branches. The CEO commits.

## Targets (at least 2 real files each where they exist)
RPG Maker MZ `.rmmzsave` · RPG Maker MV `.rpgsave` (plus, if you find one, a game's `data/Items.json` + `System.json` next to its save) ·
Unity Easy Save 3 `.es3` · Unreal GVAS `.sav` (other games than Deep Rock Galactic, which we already have) · Palworld `Level.sav`/player `.sav`.

## Where
Public GitHub repositories and issue attachments of save-editor / parser projects (e.g. test fixture folders), and other public
download pages that clearly offer the file. Use `gh search code`/`gh api` and web search. Prefer files whose repository license is
permissive (MIT, Apache-2.0, BSD, CC0, CC-BY).

## Safety rules
- Download only save/data files, each ≤ 20 MB, each into its own new folder `fixtures/external/<engine>/<short-source-name>/`.
- Never run, open with another program, or extract an archive's scripts. Archives (.zip): list contents first; extract only save/data files.
- Untrusted data: run our parser only through `cargo run -q -p saveprism-cli -- inspect <file>` (it is fuzzed and size-limited).
- Do not use any login, account, or paid source. Do not touch `fixtures/samples/**` (locked).

## Sahip (yalnız bunlara yaz)
`fixtures/external/**` · `.gitignore` (only to ignore files whose license is not permissive or unknown) · `.fabrika/raporlar/K-3c.md`

## Çıktı
`fixtures/external/SOURCES.md`: one row per file: path · game/engine · source URL · license (or "unknown") · size · sha256 ·
`inspect` result (format, guard exact/semantic/failed, error code if any). Files with unknown/non-permissive license stay local:
add them to `.gitignore` under a `# fixtures/external (local only)` block.

## Kabul
≥ 6 real files across ≥ 3 engines, all in SOURCES.md with URL and sha256; every `inspect` result recorded verbatim (one line each).
Rapor ≤8 satır (`.fabrika/raporlar/K-3c.md`): counts per engine, how many exact/semantic/failed, the failures' error codes.
