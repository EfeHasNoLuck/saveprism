# Contributing to SavePrism

This guide gets you from clone to first pull request in about 5 minutes.

## Quick setup

Install:

- Node 22 or newer
- pnpm 11
- Rust stable via [rustup](https://rustup.rs)
- Tauri prerequisites for your OS: https://v2.tauri.app/start/prerequisites/

Then:

```sh
pnpm install
pnpm dev
```

## Commands before every push

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
```

## Ways to contribute

- **Bug report:** use the "Bug report" issue form. Attach the Diagnostic Report if the app offers one.
- **Game or format report:** use the "Game or format support" form.
- **Test save:** add a save you created yourself (see "Test saves" below).
- **Translation:** edit files under `locales/<language>/`.
- **Codec:** add a decoder/encoder for a file format. Open an issue first so we agree on scope.
- **Game profile:** add a data-only profile that describes where a game keeps its saves and how to read them.

## Rules

- Commit messages follow Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`.
- Work on a branch named `feat/<topic>`. `main` stays releasable.
- Write the failing test first, then the fix.
- Parsers must not panic on any input. No `unwrap`, `expect`, `panic!` or unchecked indexing. Limit every size, length and depth. Return `Result`.
- Error codes are stable and use the form `SP-<AREA>-<NNN>`. Never reuse or renumber a code.
- Every new module gets a row in `docs/MAP.md`.
- Never write a save file without a backup and an atomic replace.
- Never execute content from a save file (no unpickling, no Ruby eval, no BinaryFormatter). Parse to a tree only.

## Test saves

- Submit only saves you created yourself.
- Remove personal data first: user names, Steam IDs, paths that contain your name.
- By submitting a save you allow it to be distributed under the project license (MIT).
- Keep files small. Put expected output under `fixtures/expected/`.

## Clean room

Do not copy code, regexes, tables or key lists from closed-source tools or websites. Write down what the format does (behavior), then implement from that description and public documentation.

Encryption keys come only from game files you own. Record the evidence: source file, method and date. Keys are never committed without that record.

## Licenses

No GPL, AGPL, LGPL or SSPL dependencies. `cargo-deny` checks this in CI. Apache-2.0 and MPL-2.0 are allowed with a NOTICE entry. State the license of any new dependency in your pull request.

## AI-assisted contributions

Allowed. The same rules apply, and you are responsible for every line you submit. Run the tests yourself.

## Conduct and security

Be respectful: see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Report vulnerabilities privately: see [SECURITY.md](SECURITY.md).
