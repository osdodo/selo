#!/usr/bin/env bash
# Usage: scripts/build-linux.sh [cargo args...]   (no args = cargo build --release -p selo)
set -euo pipefail

BOX="${SELO_BOX:-fedora-toolbox-44}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! toolbox run -c "$BOX" true >/dev/null 2>&1; then
  echo "Toolbx container '$BOX' not available (create it with: toolbox create --assumeyes)." >&2
  exit 1
fi

[ "$#" -eq 0 ] && set -- build --release -p selo

exec toolbox run -c "$BOX" bash -lc \
  'source ~/.cargo/env && cd "$1" && shift && cargo "$@"' _ "$ROOT" "$@"
