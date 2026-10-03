# Native path normalization: contract and measured allocation reduction

The loader profile on `c9d8a659` identified allocation work in
`get_normalized_absolute_path`. The native `simpleNormalizePath` fast path now
runs before segment collection in both absolute and non-absolute normalization.
Already-normal paths keep the single owned string required by TSR's public API;
they do not build a segment vector, joined string or formatted root/result.
The general dot/parent-segment fallback remains.

Validation also found existing root and joining differences, tracked in
`tsr-6.54`. The same package corrects those contracts before using the fast path.
`tsr-1yb.2.1.2` owns the measured optimization. There are no public type changes,
new dependencies, semantic caches or unsafe conversions.

## Native contract

The oracle is `typescript-go` commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, specifically
`internal/tspath/path.go`:

- `GetEncodedRootLength` / `GetRootLength` protect the UNC server prefix
  `//server/`, allowing `share/..` to climb back to it. A bare `c:` is a root;
  `c:x` is relative and joins the supplied current directory.
- URL roots remain distinct from rooted disk paths. Local `file` URLs include
  eligible drive volumes, including `%3a` and `%3A`; remote authorities do not.
  Untitled `^/` roots and UTF-8 byte offsets are preserved.
- `CombinePaths` replaces the accumulated path with an absolute component,
  including a URL, and preserves existing trailing separator spelling when
  appending a relative component.
- `simpleNormalizePath` returns an already-normal path immediately, or tries
  `/./` and leading `./` cleanup. Removing `./` cannot turn a relative path into
  an absolute path. Duplicate separators and unresolved parents take the
  general fallback.
- `NormalizePath` and `GetNormalizedAbsolutePath` deliberately differ on roots
  and trailing separators. For example, a bare `c:` stays `c:` in the former
  and becomes `c:/` in the latter.

The private helper represents native encoded root length as `(length, is_url)`.
This keeps its URL distinction without adding a signed public index API.
The fast-path scanner examines only ASCII separators and whole dot segments;
non-ASCII file names remain byte-for-byte intact.

## Correctness evidence

The committed tables exercise 52 root forms and 103 absolute-normalization
controls, including the pinned upstream fixtures. Additional controls cover
URL versus disk roots, relative drives, UNC parent traversal, separator spelling,
Unicode and the two normalization APIs.

A temporary oracle built from the pinned Go package compared **40,290**
path/current-directory pairs across `GetRootLength`, `IsRootedDiskPath`,
`GetNormalizedAbsolutePath`, `NormalizePath` and `CombinePaths`: **zero
output differences**. Inputs include the 98 upstream normalization cases,
48 upstream root cases across four current directories, and 40,000 generated
pairs with seed 572. Generation combines POSIX, UNC, DOS, local/remote URL and
untitled prefixes with empty/dot/parent/name/Unicode/mixed-separator segments.
Temporary native source was removed after building the immutable oracle.

Release validation passed:

- `cargo test --release -p tsr-path`: 22 tests.
- `cargo test --release -p tsr-vfs -p tsr-module -p tsr-compiler -p tsr-execute`.
- `cargo clippy --release -p tsr-path --all-targets -- -D warnings`.
- `coverage module_resolution file_loader`: 95/95 resolver transcripts and
  96/96 loader cases, with committed snapshots unchanged. Existing trace
  controls exercise nonempty successful and failed resolution transcripts.
- Unfiltered conformance: all **474,251** rows byte-identical, including
  **459,451 RIGHT**, 2,195 GAP and 12,605 WRONG. No previously RIGHT losses.
- Formatting and `git diff --check`.

Conformance SHA-256 before and after:
`f01585f8cafc7fd2fc198b22350c097d1baaa0a6037421369613895e34db55fb`.
TSV verification preserves continuation lines and Unicode line separators in
source text; it splits records on literal newlines.

## Profile and full-project measurement

The baseline own-child macOS sample used `--noCheck` for 2 seconds at 1 ms.
Of 1,550 bounded main-thread samples, 134 have path normalization as their
nearest TSR owner, including 81 allocator samples. Disjoint phase ownership is
resolver 716, parser 408, other 318 and binder 108. These are locating CPU
samples, not whole-CLI time shares. Inclusive resolver ancestry must not be
added to the path owner count. Remaining filesystem probes are unique, so this
profile does not justify another duplicate-probe cache.

Full Next.js CLI timing uses immutable baseline/candidate executables built
outside timing, warmed filesystem inputs, fresh processes and
`--noEmit --incremental false --composite false --pretty false
--extendedDiagnostics`. Each round has five alternating pairs; the second
round reverses its starting order. All samples, including slower candidate
samples, are retained. Our builds, profiling and CPU-heavy gates ran outside timing.

| Round | Median wall, baseline → candidate | Wall reduction | Median user CPU | Median peak process RSS |
| --- | --- | --- | --- | --- |
| 1 | 4.713380 → 4.576637 s | 2.90% | 3.641792 → 3.488667 s | 1.102 → 1.109 GB |
| 2 | 4.833827 → 4.624622 s | 4.33% | 3.760087 → 3.539673 s | 1.127 → 1.112 GB |

Both median improvements exceed the unchanged 20 ms keep threshold. RSS ranges
overlap; this establishes no memory reduction. Measurements attribute the
combined root/join/normalization package, not an independent gain for each
helper.

Every CLI sample retained 123 complete normalized diagnostics (fingerprint
`cdb777a1930fee9c86ab978511539234e820c77a69a4586d522409488e074074`),
13,097 loaded files, 1,341 actual checked files and 13,560 parsed files.
Loaded identities, content hashes, configuration and lockfile hashes remained
stable before and after both rounds.

An untimed temporary CLI probe logged each file after `check_source_file`
completed. Baseline and candidate checked exactly the same 1,341 unique
identities in the same order. Sorted checked-identity SHA-256:
`7f7c78069408fb904e9e28c72ca6ec4f9e0008a0f778e4bd253239455de16d69`.
The logging patch was archived, source restored exactly, and the rebuilt
production binary matched the timed candidate byte-for-byte. These probe
times are excluded from throughput evidence.

Loaded identity SHA-256:
`7ffb6eb6de272445848f5afd7442ed0f50fc9517a9a8ceb1017467fa108015d4`.
Input content SHA-256:
`395a32b083501c1cac01a17c6585385849f997fc10a3faab37b2f3d2d7bd9b16`.

These are isolated TSR improvements. Native/full-project comparability remains
unfinished; **the required TSR/tsgo median wall ratio ≤0.50 is not proved**.
Loader costs, checked-scope/effective-option alignment and bounded worker
ownership remain necessary for `tsr-1yb`.

## Local evidence provenance

Base revision: `c9d8a6593da574cc874763ff8b5774318ed3eadb`.

- Path source SHA-256: `858801389f78c106b43e5b83caf6ae4fa44e544f368b50374d15f7c24cf9fc98`.
- Baseline CLI SHA-256: `34e81bb8eb340a985ddd0deff9ddbb0774355ba5473ca05d7543d2d62a4c812e`.
- Candidate CLI SHA-256: `9dcf14e36f759bb324c897db7cfd655ede85b61986003a4459a4b6ef0faad5ff`.
- Native path oracle SHA-256: `00fb4e9096adc40f498813693d1a23c8b7b69aa0a2b2cbe448123228e9fbde40`.

Artifacts: `/tmp/tsr-path-normalization-{source,corpus,paired}.json`,
`/tmp/tsr-path-normalization-pairs.py`,
`/tmp/tsr-path-native-oracle-final-controls.json`,
`/tmp/tsr-path-oracle-inputs.jsonl`,
`/tmp/tsr-path-native-oracle-main.go`,
`/tmp/tsr-path-checked-scope.{json,py,patch}`, and
`/tmp/tsr-c9-loader-{profile,attribution}.json` plus the raw `.sample`.
The optimization ledger retains the comparison and rejected initial controls;
committed tests and this report retain the portable contract and result.
