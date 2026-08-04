# Module resolution

`tsr-module`, ported from `internal/module` at the pinned commit
(`5b1047d10d32e7d5b446be4de56b126ff42f82bb`). Written here rather than borrowed —
[ADR-0004](../adr/0004-oxc-inspiration-not-dependency.md) and
[ADR-0017](../adr/0017-program-before-tsconfig.md).

**Status (2026-08-04):** 75/75 judgeable `.trace.json` baselines, 100.00%.

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
| The file loader — the walk that *requests* resolutions | `compiler/fileloader.go`, `compiler/filesparser.go` (1,358 lines) | needs `/// <reference />` pragma parsing, which `tsr-parser` does not do |
| `GetEntrypointsFromPackageJsonInfo`, `loadEntrypointsFromExportMap` | `resolver.go` 2,143–2,368 | language-service auto-imports; no consumer |
| Project-reference redirects | `GetCompilerOptionsWithRedirect` | needs `tsr-tsoptions` |
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

## The oracle, and the denominator

The suite is `crates/tsr-conformance/src/module_suite.rs`. How it splits the
transcript between resolver and loader — and why — is
[ADR-0018](../adr/0018-splitting-the-resolution-oracle.md).

The arithmetic, all of it asserted by
`the_skip_rule_explains_every_missing_baseline` rather than assumed:

```text
12,444  corpus cases
   155  set @traceResolution
   109  have a .trace.json baseline
    46  do not, in exactly three buckets:
          23  upstream skips them by compiler option — mostly node10/classic
              resolution, plus AMD/UMD/System modules, baseUrl, outFile, ES5
          20  configure themselves with a tsconfig.json unit, so their options
              are not in their directives (bd tsr-9or slice 3)
           3  ran and traced nothing; baseline.Run deletes the reference file
              when the content is NoContent
   109  baselined
   -20  need tsconfig parsing
   -14  have only configuration-varied baselines (bd tsr-bb4.1)
 =  75  judged   →   75 passed, 100.00%
```

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

## Growing the number

The next two moves, in order:

1. **The file loader** (bd tsr-9or). Gate: the `======== Resolving ... ========`
   headers, in order, from the same 75 baselines — plus the 3 empty-trace cases,
   which become real assertions rather than skips. Needs `/// <reference />`
   pragma parsing in `tsr-parser` first.
2. **`tsr-tsoptions`** (ADR-0017 slice 3). Gate: the 20 tsconfig-configured trace
   cases here, and the 757 configuration-varied cases the other suites skip.
