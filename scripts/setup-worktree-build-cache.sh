#!/usr/bin/env bash
# Keep dependency artifacts in the primary checkout instead of rebuilding them
# in every worktree. Cargo locks this directory during concurrent builds.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
common="$(git rev-parse --path-format=absolute --git-common-dir)"
shared="$(dirname "$common")/target"
local_target="$root/target"

command -v sccache >/dev/null || {
  echo "Install sccache first (macOS: brew install sccache)." >&2
  exit 1
}

if [[ "$local_target" != "$shared" ]]; then
  if [[ -L "$local_target" && "$(readlink "$local_target")" == "$shared" ]]; then
    :
  elif [[ -e "$local_target" || -L "$local_target" ]]; then
    echo "Refusing to replace $local_target; preserve needed binaries and remove unused build outputs first." >&2
    exit 1
  else
    mkdir -p "$shared"
    ln -s "$shared" "$local_target"
  fi
fi

printf 'Shared Cargo target: %s\n' "$shared"
printf '%s\n' 'Use rustc-wrapper = "sccache" and incremental = false under [build] in ~/.cargo/config.toml.'
printf '%s\n' 'Explicit CARGO_TARGET_DIR or --target-dir overrides this shared directory.'
sccache --show-stats
