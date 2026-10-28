#!/usr/bin/env bash
# Builds the signed Windows installers and writes latest.json for the in-app updater.
# The updater key stays outside the repo: %USERPROFILE%\.tauri\saveprism-updater.key (no password),
# or the file named by SAVEPRISM_UPDATER_KEY. Run from anywhere: bash scripts/release/build.sh
# Upload to the GitHub release tagged v<version>: the NSIS setup exe, its .sig and latest.json (no MSI: 2026-10-28).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
key="${SAVEPRISM_UPDATER_KEY:-${USERPROFILE:-$HOME}/.tauri/saveprism-updater.key}"

if [ ! -f "$key" ]; then
  echo "updater key not found: $key" >&2
  echo "set SAVEPRISM_UPDATER_KEY to the key file, or build without the update signature: pnpm build --no-sign" >&2
  exit 1
fi

# The key text goes to the Tauri CLI only through the environment; it is never printed.
TAURI_SIGNING_PRIVATE_KEY="$(cat "$key")"
export TAURI_SIGNING_PRIVATE_KEY
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

pnpm -C "$root" build
node "$here/latest-json.mjs" "$root"
