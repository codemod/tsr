#!/usr/bin/env bash
# Build a native tsgo from the pinned vendor/typescript-go submodule without
# proxy.golang.org / go.dev access: Go toolchain and every module come from
# GitHub via `git clone`. Touches nothing tracked in the tsr checkout; the
# submodule's go.mod/go.sum are copied and used via -modfile.
#
# Usage: build-tsgo.sh [TSR_ROOT] [WORK_DIR]
set -euo pipefail
TSR_ROOT=${1:-/home/user/tsr}
WORK=${2:-/tmp/claude-0/gobuild}
GO_TAG=${GO_TAG:-go1.26.8}           # any go1.26.x satisfies `go 1.26`
BOOTSTRAP=${GOROOT_BOOTSTRAP:-/usr/local/go}   # needs >= go1.24.6 to build 1.26
SRC="$TSR_ROOT/vendor/typescript-go"
MODS="$WORK/mods"
mkdir -p "$WORK" "$MODS"
export GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off GOFLAGS=-mod=mod

# 1. Go toolchain from source.
if [ ! -x "$WORK/go/bin/go" ]; then
  [ -d "$WORK/go" ] || git clone -q --depth 1 --branch "$GO_TAG" https://github.com/golang/go "$WORK/go"
  (cd "$WORK/go/src" && GOROOT_BOOTSTRAP="$BOOTSTRAP" nice -n 10 ./make.bash)
fi
export PATH="$WORK/go/bin:$PATH"
go version

# 2. Modules at the exact versions in the submodule's go.mod.
#    name|github repo|ref (tag, or full commit for pseudo-versions)|import path
MODLIST='
go-winio|microsoft/go-winio|v0.6.2|github.com/Microsoft/go-winio
go-json|go-json-experiment/json|01eb4420fa68cc49437c0f7b50647364cb2bae38|github.com/go-json-experiment/json
go-cmp|google/go-cmp|v0.7.0|github.com/google/go-cmp
go-osstat|mackerelio/go-osstat|v0.2.7|github.com/mackerelio/go-osstat
patience|peter-evans/patience|v0.3.0|github.com/peter-evans/patience
xxh3|zeebo/xxh3|v1.1.0|github.com/zeebo/xxh3
x-sync|golang/sync|v0.21.0|golang.org/x/sync
x-sys|golang/sys|v0.46.0|golang.org/x/sys
x-term|golang/term|v0.44.0|golang.org/x/term
x-text|golang/text|v0.38.0|golang.org/x/text
gotest.tools|gotestyourself/gotest.tools|v3.5.2|gotest.tools/v3
cpuid|klauspost/cpuid|v2.2.10|github.com/klauspost/cpuid/v2
moq|matryer/moq|v0.7.1|github.com/matryer/moq
x-mod|golang/mod|v0.37.0|golang.org/x/mod
x-tools|golang/tools|v0.47.0|golang.org/x/tools
'
cp "$SRC/go.mod" "$WORK/tsgo.mod"
cp "$SRC/go.sum" "$WORK/tsgo.sum"
printf '\nreplace (\n' >> "$WORK/tsgo.mod"
while IFS='|' read -r dir repo ref mod; do
  [ -n "$dir" ] || continue
  if [ ! -e "$MODS/$dir/go.mod" ]; then
    if [[ "$ref" =~ ^[0-9a-f]{40}$ ]]; then
      git init -q "$MODS/$dir"
      git -C "$MODS/$dir" fetch -q --depth 1 "https://github.com/$repo" "$ref"
      git -C "$MODS/$dir" -c advice.detachedHead=false checkout -q FETCH_HEAD
    else
      git -c advice.detachedHead=false clone -q --depth 1 --branch "$ref" "https://github.com/$repo" "$MODS/$dir"
    fi
  fi
  printf '\t%s => %s\n' "$mod" "$MODS/$dir" >> "$WORK/tsgo.mod"
done <<< "$MODLIST"
printf ')\n' >> "$WORK/tsgo.mod"

# 3. Build.
(cd "$SRC" && nice -n 10 go build -modfile="$WORK/tsgo.mod" -o "$WORK/tsgo" ./cmd/tsgo)
"$WORK/tsgo" --version
echo "built $WORK/tsgo"
