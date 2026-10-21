# Security Policy

This document says which versions get fixes, what counts as a vulnerability, and how to report one.

## Supported versions

Only the latest release is supported.

## How to report

Use GitHub's "Report a vulnerability" button on the repository's Security tab. This opens a private report. Do not open a public issue for a vulnerability.

Include the SavePrism version, your OS, and steps to reproduce. If a crafted save file triggers the problem, attach it to the private report.

## In scope

- A crafted save file that causes a crash, out-of-bounds memory access, or an infinite loop.
- A decompression bomb: a small file that expands to exhausting memory or disk.
- Path traversal in archive or save handling.
- A write that skips the backup or is not atomic.
- A bypass of update signature verification.

## Out of scope

- Cheating in games.
- DRM or anti-cheat circumvention.
- Online or multiplayer games.

## Response

We acknowledge a report within 7 days. There is no bug bounty.
