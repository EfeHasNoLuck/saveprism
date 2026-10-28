# HANDOFF

**State (2026-10-28, ~20:15 TR):** the repo `huseyincancalti/saveprism` is public; the first release is **0.9.0 for
Windows, Linux and macOS** (Linux/macOS are tested by the community). Last work commit `dd3abb6` (patched uesave for
the macOS CI failures), then this handoff; its `ci` run was still going when this was written. History, plan and
decisions: `NOW.md`.

## Works (with evidence)
- Windows install test by the maintainer (setup exe only, no MSI): logo on the setup exe, publisher "Hüseyin Can
  ÇALTI" in the installed apps list, "SavePrism ile çöz" in Explorer, the first-launch tour, a real Slime Rancher 2
  edit that the game loaded. README reviewed by the maintainer ("güzel gözüküyor"); release text approved ("uygun").
- Going public: a history scan found no keys, tokens or passwords; git-filter-repo removed `fixtures/external/`
  from every commit; the maintainer force-pushed main and the 8 `dependabot/*` branches (every `refs/pull/*/head`
  now points at the clean history) and made the repo public. Checked with `gh repo view` (PUBLIC) and `git ls-remote`.
- Checked over the GitHub API: Discussions on with category `community-themes`; ruleset "Protect main" active
  (`deletion`, `non_fast_forward`); description and 14 topics set; secret `TAURI_SIGNING_PRIVATE_KEY` exists.
- CI: Windows and Ubuntu green from `8b07c46` on. macOS fix `dd3abb6`: a counting allocator showed uesave
  allocating 5.3 GB of zeroed buffers (largest 1.8 GB) while the GVAS hint search tried wrong guesses on a 379-byte
  save; with `vendor/uesave` (uesave 0.7.1, MIT, `COPYING`, two read fixes in `PATCHES.md`, wired through
  `[patch.crates-io]`) the same save makes no allocation over 1 MiB. Local `kontrol.sh rust` 3/3, `cargo deny` ok.

## Broken / not checked
- macOS CI after `dd3abb6`: not seen yet (`gh run list -R huseyincancalti/saveprism -b main`). Before the fix, 7
  GVAS tests failed on macOS only (hint search over its 3 s budget).
- The `release` workflow has never run; the Linux and macOS builds have never been built or opened.
- Auto-update end to end (download → install → restart) needs the published release.
- Left to the community (maintainer's choice; public text must invite feedback warmly, never "you test it"): the
  update check, a real AI app over MCP, a second file handed to the open window, Linux and macOS in general.
- Dependabot's 8 pull requests are stale (based on the old history); some fail CI. Close or let Dependabot rebase.
- Known, not blocking: Explorer label stays in the old language if Windows' language changes after install (until
  the next update); SmartScreen and macOS warn (no code signing).
- Leftovers the project hook stops agents from deleting (the maintainer deletes them in PowerShell with the `\\?\`
  prefix): `C:\SavePrismDemo\`, `.claude\worktrees\agent-*` (7), `..\saveprism-wt\{satisfactory,w4-settings,
  w6-shortcuts,w7-screens-fix,w7-settings-fix}`. Old MSI files in `target/release/bundle/msi/` are stale.

## Next step
1. Check the `ci` run of `dd3abb6` (or later): all three systems green. If macOS still fails, read the job log
   (`gh api repos/huseyincancalti/saveprism/actions/jobs/<id>/logs`).
2. `gh workflow run release.yml -R huseyincancalti/saveprism --ref main`; wait for all three jobs. Check the draft
   release `v0.9.0`: setup exe, `.dmg`, `.AppImage`, `.deb`, their `.sig` files and `latest.json` with Windows,
   Linux and macOS entries; the file names must match `scripts/release/notes-0.9.0.md` (fix the text if not).
3. `gh release edit v0.9.0 -R huseyincancalti/saveprism --notes-file scripts/release/notes-0.9.0.md`.
4. Ask the maintainer to publish (their explicit OK; not a pre-release, or the updater endpoint
   `releases/latest/download/latest.json` will not find it). If the auto-mode classifier refuses the publish, the
   maintainer presses Publish on the draft page.

## Maintainer decisions (2026-10-28)
- First public release 0.9.0 with Linux and macOS; a real 1.0 after a quiet period.
- Windows ships as the setup exe only. Publisher "Hüseyin Can ÇALTI" (surname in capitals).
- The author e-mail in commits stays as it is.
- Release: show the maintainer only the release text; tag and title are ours; publish after their OK.
- Explorer label follows the Windows display language at install time (Turkish → Turkish, else English); revisit
  when a new UI language is added. Ask before any choice about what players see.

## Gotchas
- The project hook stops agents from force-pushing, deleting recursively, writing any file named `LICENSE`, and
  any Bash command that names `.claude/araclar` together with a file edit. The auto-mode classifier refuses repo
  settings changes (pausing Actions, rulesets); the maintainer does those on the web.
- Failed CI runs e-mail the maintainer: run `kontrol.sh rust`, `kontrol.sh web` and `cargo deny check` before a push.
- In PowerShell `bash` is WSL; use `"C:\Program Files\Git\bin\bash.exe"` for scripts the maintainer runs.
- Start an EVREN job: `bash .motor/start_job.sh <name>` (brief `.motor/<name>.md`), at most 2 at once.
