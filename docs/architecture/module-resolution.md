# Module resolution

`tsr-module`, ported from `internal/module` at the pinned commit
(`5b1047d10d32e7d5b446be4de56b126ff42f82bb`). Written here rather than borrowed —
[ADR-0004](../adr/0004-oxc-inspiration-not-dependency.md) and
[ADR-0017](../adr/0017-program-before-tsconfig.md).

**Status (2026-08-04):** 95/95 judgeable `.trace.json` baselines, 100.00%.

## What it is

Resolution turns an import specifier into a file. TypeScript's version of that is
not Node's, which is the whole reason it is 2,400 lines:

- **Extension substitution.** `./foo.js` finds `foo.ts`; `./foo` finds
  `foo.ts`, `foo.tsx`, `foo.d.ts`, `foo.js`, `foo.jsx` in that order; `./foo.mjs`
  finds `foo.mts` then `foo.d.mts` then `foo.mjs`. The table is
  `ResolutionState::try_adding_extensions`, and each arm's order is baselined
  step by step.
- **Two passes through `node_modules`.** TypeScript and declarations first,
  JavaScript and JSON second — so an `@types` package high in the directory tree
  beats an untyped implementation lower down.
- **`paths`, `typesVersions`, `moduleSuffixes`, `rootDirs`.** Four separate
  mapping layers, three of which can each redirect a lookup before the file
  system is touched.
- **`exports`/`imports` maps**, with conditions matched in declaration order,
  pattern keys sorted by longest literal prefix, and `types@<semver-range>`
  conditions gated on the compiler's own version.
- **The `node10Result` fallback**: when a package resolves to JavaScript through
  its export map, resolution runs a *second* time with `exports` switched off, so
  the checker can say "this library needs a configuration update" instead of
  "no types".

## What is not here, and why

**`node10` and `classic` resolution are not ported — upstream has not ported
them either.** `internal/module/resolver.go` contains no mention of either
algorithm, and `core.CompilerOptions.GetModuleResolutionKind` falls through to
`Bundler` for `Unknown`, `Classic`, and `Node10` alike. typescript-go's own test
harness skips every case that asks for them.

This corrects the framing this slice started from, which listed "the
node16/nodenext/bundler/classic split" as scope. There are three kinds, not four.
A `node10`/`classic` *request* survives in the raw
`CompilerOptions::module_resolution` field, which is what decides whether a
conformance case is judgeable at all — but it never reaches the resolver.

**Corrected number.** An earlier version of `module_resolution_kind` in
`tsr-core` had an extra arm returning `Node10` when `moduleResolution` was
unspecified. That arm does not exist upstream. It made 54 baselined corpus cases
derive an unimplemented kind; the resolver panicked on the first conformance run.
Found by the trace oracle, fixed, and the corrected function carries the note.

Also deferred, with nothing depending on them yet:

| Deferred | Where it lives upstream | Blocked on |
|---|---|---|

| `GetEntrypointsFromPackageJsonInfo`, `loadEntrypointsFromExportMap` | `resolver.go` 2,143–2,368 | language-service auto-imports; no consumer |
| Project-reference redirects | `GetCompilerOptionsWithRedirect` | needs project references, which `tsr-tsoptions` does not read |
| The typings-location extra pass | `tryResolveFromTypingsLocation` | needs an installed-typings host |

## Layout

| File | Upstream | What |
|---|---|---|
| `resolver.rs` | `module/resolver.go`, `module/cache.go` | the state machine |
| `types.rs` | `module/types.go` | `Extensions`, `NodeResolutionFeatures`, `PackageId`, results, conditions |
| `util.rs` | `module/util.go`, `core/pattern.go` | patterns, package-name arithmetic |
| `package_json.rs` | `packagejson/` | the fields, and `typesVersions` selection |
| `json.rs` | `internal/json` | an ordered, duplicate-tolerant JSON reader |
| `semver.rs` | `internal/semver` | versions and npm-style ranges |
| `messages.rs` | the 69 messages `internal/module` writes | the trace text, with upstream codes |

Two supporting crates arrived with it: `tsr-vfs` (the five file-system questions
resolution asks) and the extension half of `tsr-path`.

## Three places the port is deliberately not idiomatic

The trace oracle pins the *walk*, not the answer, so structure is behaviour.

1. **Search results are three-valued.** `Search = Option<Resolved>` distinguishes
   "keep looking" (`None`) from "stop, with nothing" (`Some(Resolved::default())`)
   from "stop, with this file". Upstream spells these `nil`, `&resolved{}`, and a
   populated pointer. Collapsing the first two is the obvious simplification and
   changes which fallbacks run — `"exports": null` depends on it.
2. **Loaders are an enum, not closures.** Upstream passes
   `resolutionKindSpecificLoader` function values that capture mutable resolution
   state and are then invoked with that same state. Rust will not allow it, so
   `Loader` names the four call sites and `run_loader` dispatches.
3. **Object key order is load-bearing everywhere.** `exports` conditions are
   matched in declaration order and each attempt is traced, so `CompilerOptions`
   grew an `OrderedMap` and the JSON reader preserves insertion order. A
   `HashMap` here would produce a different, equally plausible trace and fail
   every baseline.

## Two things that look like optimisations and are not

- **The `package.json` info cache.** Its hits *emit trace lines*
  (`File '{0}' exists according to earlier cached lookups.`), so a resolver
  without one produces different output, not merely slower output.
- **`PackageJson::get_version_paths` replays its traces on every call.** Upstream
  memoises the computation and replays the messages, so a package consulted twice
  logs its `typesVersions` lines twice. Deduplicating would silently shorten every
  trace that revisits a package.

## Per-project query caches

The resolver also has native query caches (`internal/module/cache.go`). Module
keys contain the exact directory spelling, specifier, and resolution mode. Type
reference keys additionally distinguish the inferred-types containing file.
Successes and failures are cached; config-file lookup has separate semantics and
does not use either query cache. Project-reference redirects are not supported;
when introduced they must add native's redirected-config identity or use a
separately configured resolver.

As in native `ResolveModuleName` and `ResolveTypeReferenceDirective`, tracing
bypasses query-cache reads, while the package-JSON cache still emits its observed
hit messages. A cache entry therefore does not suppress a traced resolution walk.
These caches belong to one resolver with immutable options and one project
snapshot. Rebuild the resolver when files/options change; watch or incremental
hosts must invalidate or replace them before reuse. No persistent build cache is
involved.

The CLI also creates a `CachedFileSystem` for each compiler host, following
native `NewCachedFSCompilerHost` and `internal/vfs/cachedvfs`. File existence,
directory existence, directory entries and real paths have separate caches
keyed by exact path spelling. Negative and empty results are retained. Reads of
file contents and case-sensitivity queries always delegate to the backing host.
This wrapper remains active during traced resolution: it reuses disk results
without suppressing resolver trace messages. Configuration discovery precedes
the compiler host and keeps its own filesystem behavior.

`clear_cache` discards metadata without disabling caching;
`disable_and_clear_cache` bypasses reads and writes until `enable` is called.
Refreshing metadata alone does not invalidate resolver/package-JSON caches:
future watch or incremental support must refresh all snapshot-dependent layers
together. The current driver creates all of them afresh for each compilation.
The wrapper is sequential; a parallel loader must synchronize its maps and use
a shareable backing filesystem, as native does.

## The oracle, and the denominator

The suite is `crates/tsr-conformance/src/module_suite.rs`. How it splits the
transcript between resolver and loader — and why — is
[ADR-0018](../adr/0018-splitting-the-resolution-oracle.md).

The arithmetic, all of it asserted by
`the_skip_rule_explains_every_missing_baseline` rather than assumed:

```text
12,444  corpus cases
   155  set @traceResolution
    95  have a plain .trace.json baseline   →   95 judged, 95 passed, 100.00%
    60  do not, in exactly three buckets:
          41  upstream skips them by compiler option — node10/classic
              resolution, AMD/UMD/System modules, baseUrl, outFile, ES5
          14  have only configuration-varied baselines, e.g.
              `case(module=commonjs).trace.json` (bd tsr-bb4.1)
           5  ran and traced nothing; baseline.Run deletes the reference file
              when the content is NoContent
```

(146 `.trace.json` files exist: these 95, plus the configuration-varied ones,
which are several files per case.)

The `tsconfig-configured` bucket that used to sit here is gone. `tsr-tsoptions`
reads those configs ([tsconfig.md](tsconfig.md)), so their **options are visible
to upstream's own skip predicate** — which is why the skipped-by-option bucket
grew from 23 to 41 and the empty-trace bucket from 3 to 5. Twenty of them are now
judged, taking this suite from 75 to 95.

**An absent baseline is not evidence on its own.** This project has been caught by
that repeatedly, so `upstream_skip_reason` reimplements
`harnessutil.SkipUnsupportedCompilerOptions` rather than inferring the reason from
the absence, and the three buckets are pinned by count — and the smallest, the one
that could silently absorb a regression, by name. A case moving between buckets
fails the test.

### The sanitiser

The suite reproduces `harnessutil.TracerForBaselining.sanitizeTrace`, which is
part of the oracle rather than an incidental detail. It does two stateful
rewrites over the whole run:

- The compiler version becomes `FakeTSVersion`, so a version bump does not rewrite
  35 baselines.
- The `package.json` existence-cache messages are normalised against a *separate*
  cache the tracer keeps, so the first mention of a file always reads as a real
  probe and every later one as a cache hit — **regardless of which the resolver
  actually did**. Without this the committed output would depend on the order
  resolutions happened to run in, and the baselines would be unstable.

## The other half: the file loader

Landed. It has its own suite, `file_loader`, over the same baselines, and its own
document — [file-loader.md](file-loader.md) — with the decisions in
[ADR-0019](../adr/0019-the-loader-gate-discharges-the-mode-circularity.md).

**No loader can move the number above**: this suite replays requests taken from
the baseline. What changed is what it claims alongside. `file_loader` compares
the requests themselves — specifier, containing file, order, and **mode** — and
reaches 96/96. Together the two cover a `.trace.json` end to end, which neither
does alone.

The mode is the part worth noting. ADR-0018 recorded a circularity it could not
avoid: this suite reads each resolution's mode out of the baseline's own
`Resolving in {0} mode with conditions {1}.` line, so it could never catch a
wrong one. `file_loader` supplies nothing —
`getImpliedNodeFormatForFile`, the `package.json` `type` lookup, and
`getModeForUsageLocation` all run — and compares that line. That falsifier is
discharged.

The two denominators differ, by exactly one arithmetic step:

```text
    95  module_resolution
    +5  cases upstream traced nothing for; here that is an assertion, not a skip
    -4  cases needing libReplacement, which resolves the bundled lib files
        through the module resolver and which the loader cannot run (bd tsr-9or.5)
 =  96  file_loader   →   96 passed, 100.00%
```

Any other difference between the two denominators is a bug in one of the suites,
not a fact about the compiler. Everything *before* the judging — which cases
upstream runs, under what options, over what file system, with which root files —
is shared, in `crates/tsr-conformance/src/trace_case.rs`, so the two can only
drift where a suite says it is less capable.

## Growing the number

What is left of the 146 baselines, and what each needs:

1. **14 configuration-varied cases** (bd tsr-bb4.1). Their baselines live under
   `case(module=commonjs).trace.json`, so judging them means running each case
   once per configuration. A denominator change, and it affects the parser and
   binder suites far more than these (757 cases there).
2. **4 `libReplacement` cases** (bd tsr-9or.5), which need the bundled
   `lib.*.d.ts` on the host so the lib reference chain can be walked.
3. **41 cases upstream skips itself**, which will come back only if upstream
   ports `node10`/`classic` resolution or AMD/UMD/System modules.

Nothing here is blocked on this port any more.
