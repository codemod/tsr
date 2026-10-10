#!/bin/bash
# Offline cargo bootstrap: crates.io / static.rust-lang.org are blocked by the
# environment network policy; GitHub git works. Rebuild the locked registry
# crates from their GitHub sources into a directory source. Submodules are
# cloned too: libmimalloc-sys vendors mimalloc's C sources as two of them
# (ADR-0055).
set -e
TSR_ROOT="${TSR_ROOT:-$(git rev-parse --show-toplevel)}"; export TSR_ROOT
HERE="$(cd "$(dirname "$0")" && pwd)"
V=/tmp/claude-0/vend; mkdir -p $V/src; cp "$HERE"/assemble.py "$HERE"/list.txt $V/
cd $V/src
while read c r t s; do d="$(echo $r|tr / _)@$t"; [ -d "$d" ] && continue
  if ! git clone -q --depth 1 --recurse-submodules --shallow-submodules --branch "$t" "https://github.com/$r" "$d" 2>/dev/null; then
    git clone -q "https://github.com/$r" "$d"; (cd "$d"; git checkout -q "$(git log --format=%H -G"^version = \"$t\"" -- Cargo.toml | tail -1)"); fi
done < $V/list.txt
(cd "$TSR_ROOT" && python3 $V/assemble.py)
mkdir -p ~/.cargo; cat > ~/.cargo/config.toml <<'CFG'
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "/tmp/claude-0/vend/vendor"
[net]
offline = true
CFG
echo "export RUSTUP_TOOLCHAIN=stable" >> ~/.bashrc
