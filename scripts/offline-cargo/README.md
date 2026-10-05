# Offline cargo bootstrap

For cloud sessions whose network policy blocks `index.crates.io`,
`static.crates.io` and `static.rust-lang.org` but allows `git` over GitHub.

```bash
scripts/offline-cargo/bootstrap.sh          # from the repo root
git submodule update --init --recursive --depth 1
export RUSTUP_TOOLCHAIN=stable              # the pinned 1.96.0 cannot be installed
cargo build --release -p tsr-conformance
```

## What it does

`list.txt` names each registry crate in `Cargo.lock`, its GitHub repository, the
tag (or, where no tag exists, the version whose `Cargo.toml` bump is checked
out) and the crate's subdirectory. `bootstrap.sh` clones them; `assemble.py`
turns each checkout into a cargo *directory source* under
`/tmp/claude-0/vend/vendor`:

- resolves `workspace = true` inheritance from the enclosing workspace,
  because a directory-source crate has no workspace;
- drops `path` keys, dev-dependencies, test/bench/example targets, and the
  feature entries that named a dropped dependency;
- writes `.cargo-checksum.json` with the `Cargo.lock` package checksum and an
  empty file list, so the lockfile verifies unchanged.

`windows-sys` and `windows-link` are empty stubs: they only compile on Windows.

`~/.cargo/config.toml` then replaces `crates-io` with that directory and sets
`net.offline`.

## Limits, accepted

- The crate content is the git tree at the release, not the published `.crate`.
  Identical for compilation in every crate here; a crate whose build depends on
  generated files present only in the published package would fail loudly at
  build time, not silently.
- The toolchain is whatever `stable` the image carries (1.97.0 when written),
  not the pinned 1.96.0. Clippy verdicts can differ from CI's; CI stays the
  authority for the lint gate (see `rust-toolchain.toml`).
- Adding or bumping a dependency requires a matching `list.txt` row.
- This changes nothing that is built for CI or release; it is local tooling.

## Native tsgo for the perf comparison

`scripts/offline-cargo/build-tsgo.sh [TSR_ROOT] [WORK_DIR]` builds Go 1.26 from
its GitHub source (bootstrapped by the image's Go 1.24) and every module of the
pinned `vendor/typescript-go/go.mod` from GitHub clones, then builds
`./cmd/tsgo` with a copied `-modfile` plus `replace` block. Nothing tracked is
modified. The result (`$WORK_DIR/tsgo`) goes in the `--tsgo` slot of
`cargo run -p xtask -- perf-project`. Toolchain build takes a few minutes.
