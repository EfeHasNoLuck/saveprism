# Contributing to SavePrism

Thanks for helping. This guide gets you from clone to a first pull request. You do not need to code to help: bug
reports, game requests, translations and themes count just as much.

| I want to | Start here |
|---|---|
| Report something that does not work | [Bug report](https://github.com/huseyincancalti/saveprism/issues/new?template=bug_report.yml) |
| Ask for a game or save format | [Game or format support](https://github.com/huseyincancalti/saveprism/issues/new?template=game_support.yml) |
| Suggest a feature or an AI/MCP tool | [Feature request](https://github.com/huseyincancalti/saveprism/issues/new?template=feature_request.yml) |
| Translate SavePrism | [Add a language](#add-a-language), or the [Add a language](https://github.com/huseyincancalti/saveprism/issues/new?template=add_language.yml) form |
| Share a theme | [Share a theme](#share-a-theme) |
| Add a save format | [Add a format](#add-a-format) (open a game request first, so we agree on scope) |
| Report a security problem | [SECURITY.md](SECURITY.md), privately, never as a public issue |

## Setup

Install:

- Windows 10 or 11, 64-bit
- Node 22 or newer and pnpm 11 (`corepack enable` picks the version from `package.json`)
- Rust 1.99 or newer, stable, MSVC toolchain, via [rustup](https://rustup.rs)
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/): Visual Studio Build Tools with "Desktop
  development with C++", and WebView2 (already on Windows 10 and 11)

Then:

```bash
pnpm install
pnpm dev      # the app, with hot reload
pnpm build    # installers in target/release/bundle/
```

No `.env` file, API keys or accounts are needed.

## Checks before every push

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check              # licenses and advisories (install with: cargo install cargo-deny)
pnpm typecheck
pnpm lint
pnpm test
```

## Rules

- Commit messages and pull request titles follow Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`,
  `test:`. Work on a branch such as `feat/<topic>`; `main` stays releasable.
- Write the failing test first, then the fix.
- Parsers must not panic on any input: no `unwrap`, `expect`, `panic!` or unchecked indexing. Limit every size, length
  and depth, and return `Result`.
- Never write a save file without a backup and an atomic replace (temp file, then replace).
- Never run content from a save file (no unpickling, no Ruby eval, no BinaryFormatter deserialization). Parse to a tree
  only.
- Error codes are stable and look like `SP-<AREA>-<NNN>`. Never reuse or renumber one.
- Every new module gets a row in `docs/MAP.md`.
- Everything in the repository is English: code, comments, docs, UI text in `locales/en/`, commit messages.

## Add a language

English (`en`) is the source; Turkish (`tr`) is the second language. For a new language with the code `<code>`
(for example `de`, `pt-BR`):

1. Copy `apps/desktop/locales/en/` to `apps/desktop/locales/<code>/` and translate `common.json` and `editor.json`.
   Keep every key and every `{{placeholder}}` as it is. Translate `CHANGELOG.md` too (the What's new dialog shows it).
2. `apps/desktop/src/i18n/detect.ts`: add the code to `SUPPORTED_LANGUAGES`.
3. `apps/desktop/src/i18n/language.ts`: add the language's own name, its short code and its flag.
4. `apps/desktop/src/i18n/flags.tsx`: add the flag, if it is not there yet.
5. Import the new files where the other languages are imported: `src/i18n/index.tsx` (`common.json`),
   `src/editor/i18n.ts` (`editor.json`) and `src/shell/WhatsNewDialog.tsx` (`CHANGELOG.md`).
6. Run `pnpm test`: `src/i18n/keys.test.ts` fails on any key that is missing or extra.

Check the result in the app (`pnpm dev`, then the language menu in the header): long words in buttons and card
headers are the usual problem. Attach a screenshot to the pull request.

## Share a theme

A theme is data only: colors, the app color, text size and the background, in a `.saveprism-theme.json` file.

1. Set **Settings → Appearance** the way you like it and press **Export theme…**.
2. Edit the file if you want (name, colors); keep `format` and `version` as they are.
3. Post it in [Discussions → Community themes](https://github.com/huseyincancalti/saveprism/discussions/categories/community-themes)
   with a screenshot. Others press **Import theme…** and pick the file.

The file format and how the color variables work: [docs/themes.md](docs/themes.md). A theme that ships with the app
(for example a high-contrast one) is code: see "Adding a high-contrast theme" there and open a pull request.

## Add a format

1. Open a [game request](https://github.com/huseyincancalti/saveprism/issues/new?template=game_support.yml) first, so
   we agree on scope.
2. Write down how the format behaves (a short spec in `docs/research/`), from your own saves and public documentation.
3. Add a module in `crates/saveprism-codecs/src/<format>/` and register it in `register_all` in
   `crates/saveprism-codecs/src/lib.rs`. Decoding then encoding an unchanged save must give the same bytes.
4. Add synthetic fixtures in `fixtures/synthetic/<format>/` with their expectations (see `fixtures/README.md`), a fuzz
   target in `fuzz/fuzz_targets/`, and a row in the README "Supported formats" table.

## Add an MCP tool

The MCP server lives in `apps/desktop/src-tauri/src/mcp/`: `tools.rs` lists the tools (with their `readOnlyHint` and
`destructiveHint`) and runs them on the same session code as the app; `tests.rs` covers them.

- Prefer read-only tools. A tool that writes a file must make a backup first, write only the open file or a new file
  next to it, and be marked destructive so AI apps ask the player.
- Keep results under the result cap (page long lists).
- Document the tool in the table in [docs/ai.md](docs/ai.md) and in the README.

## Test saves

- Submit only saves you created yourself, with personal data removed (user names, Steam IDs, paths with your name).
- By submitting a save you allow it to be distributed under the project license (MIT).
- Keep files small. Synthetic saves made by a script are even better.

## Clean room

Do not copy code, regexes, tables or key lists from closed-source tools or websites. Write down what the format does,
then implement it from that description and public documentation. Code from projects under a permissive license (MIT,
Apache-2.0, BSD) may be used as a reference with credit; GPL-family code may not.

Encryption keys come only from game files the player owns, and are stored with a record of where they came from
(source file, method, date). Keys are never committed without that record.

## Licenses

No GPL, AGPL, LGPL or SSPL dependencies. `cargo deny` checks this. Apache-2.0 and MPL-2.0 are allowed with a NOTICE
entry. Name the license of any new dependency in your pull request.

## AI-assisted contributions

Welcome. The same rules apply, and you are responsible for every line you submit. Run the checks yourself.

## Conduct and security

Be respectful: see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Report vulnerabilities privately: see
[SECURITY.md](SECURITY.md).
