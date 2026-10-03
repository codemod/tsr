# Next.js CLI comparison — 2026-10-02

Investigation: tsr-6.43. The initial diagnosis below is retained as historical
evidence. Its implementation follow-up is recorded next.

## Implementation follow-up

The follow-up fixes tsr-6.3.1, tsr-6.44, tsr-6.45 and tsr-6.46.
Measured at **6b42bfb5** against pinned tsgo
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`:

- Assertions: **457,641/478,855 (95.57%)**; cases **7,021/9,538**.
- Aligned verdicts: **457,641 RIGHT / 2,538 GAP / 14,064 WRONG**.
- Compared with e6812e01: **48 RIGHT gains**, zero RIGHT losses,
  five GAP-to-WRONG imported-name rows and nine changed already-WRONG rows.
- Diagnostics corpus: **2,790/5,488**; binder **8,497/8,497**.
- Full app: **130 TSR diagnostics**, versus **zero pinned-native diagnostics**.
  All 13 prior subset false positives are cleared.

The exact no-argument cargo command below took **30.004 seconds including a
release rebuild**; cargo reported **21.37 seconds** for that build. A separate
rebuild-free binary run completed in **7.760 seconds**, with the same 130
diagnostics. Native completed in **3.325 seconds**. Pre-fix current-source checks were stopped after several
minutes without a completed diagnostic pass; those historical measurements
remain below. Builds and competing verification workloads must not be folded
into a checker-only performance claim.

```sh
cd /Users/mohebifar/dev/codemod/app/apps/nextjs
cargo run --release --manifest-path /Users/mohebifar/dev/tsr/Cargo.toml --bin tsr
```

The separate `/Users/mohebifar/dev/codemod/tsr` checkout was preserved for the
user's concurrent agents; pull main there before using its manifest.

| Remaining category at 6b42bfb5 | Codes | Count |
|---|---|---:|
| Assignment and argument comparison | TS2322, TS2345 | 58 |
| Nullable/indexed access | TS18048, TS18047, TS2531 | 30 |
| Imported/member lookup | TS2339 | 15 |
| Comparison overlap | TS2367 | 10 |
| Missing checker diagnostics under expect-error | TS2578 | 8 |
| Callback context | TS7006 | 4 |
| Export lookup and type-only use-site meaning | TS2305, TS1361 | 4 |
| Array assertion overlap | TS2352 | 1 |

These counts total the fresh full pass, not the older unversioned 196-error log.
The remaining diagnostics and missing directive-covered errors are tracked by
tsr-6.47. Release workspace tests, four CLI controls, clippy and formatting pass.
Upstream-reference and section checks pass; the issue-id gate retains the
historical unresolved-citation backlog (tsr-10).

The blocking constructor is `new McpServer` in
`packages/api/src/entries/hono/grep-mcp/server.ts`. Its `Implementation` parameter
uses the recursive `Flatten<z.infer<...>>` utility in the SDK declaration.
An unresolved qualified reference carries the port's ANY flag while remaining
an error type. Conditional evaluation treated it as genuine `any`, recursively
evaluating both branches through arrays, sets, maps and mapped objects.
Native `getConditionalType` checks errorType before its any expansion
(`checker.go:24319`). The evaluator now recognizes unresolved error types and
declines evaluation, preserving the existing written-reference fallback.
Genuine `any` still evaluates both branches. This addresses the demonstrated
blowup; it does not implement native instantiation caches or all recursive
conditional types.

The React Hook Form and BullMQ failures share a binder cause. An external
`.d.ts` file with an implicit export context exported imported aliases.
`export *` then selected private imports from the first barrel member, creating
alias cycles or leaking unrelated type-only provenance. The source-file arm
now follows native `declareModuleMember` (`binder.go:377`): imports stay local,
export specifiers and explicitly exported import-equals aliases remain exports.
The checker preserves an unresolved imported reference's written form when a
resolved file's declaration is unavailable; an actually missing module retains
native any recovery, and a resolved non-type target still rejects.

The corresponding rule inside ambient namespaces is deferred as tsr-6.44.1.
The broader candidate exposed unsupported deferred-alias recovery and namespace
printing. It is not part of the package fix. The recorded refused measurements
in STATUS describe why this extra scope was excluded.

JSON parsing now records `JSON_FILE`; binding creates the file module and its
`export=` property while preserving the SourceFile's own module symbol
(`binder.go:754`). Reading that property widens the JSON expression
(`checker.go:16589`), so imports retain property types instead of merely hiding
TS2306. A control assigning the numeric JSON member to string still reports.

The Tailwind TS7016 is valid and preceded by `@ts-expect-error`. The CLI now
uses the same program-level directive filter as conformance, moved into
`tsr-compiler`. It suppresses preceding directives and reports unused
expect-error directives, except in skipped declaration files. This fixes a
CLI integration gap; allowJs does not suppress missing-declaration errors.
The native implicit-any-module rule also skips side-effect imports
(`checker.go:15486`); imports that consume a value still require declarations
under strict mode. This removes the app's `server-only` import false positives.

Assertions now regularize the object source and compare the target against its
widened form before the forward comparison (`checker.go:12317`). Regularization
alone still rejects the empty-object example because regular object literals
retain their exact property set. Widening removes that exactness. Both `as` and
angle-bracket assertions pass the empty-object controls; incompatible member
types still report TS2352.

Regression controls cover implicit/explicit alias exports, barrel type and
constructor use, retained JSON member types, assertion overlap, real any versus
unresolved conditional checks, directive suppression and unused directives.
The shared directive filter retains its original tests. Concurrent checkout
work and local configuration were preserved.

## Versions and full-project result

The requested command was run inside
`/Users/mohebifar/dev/codemod/app/apps/nextjs`:

```sh
cargo run --release --manifest-path /Users/mohebifar/dev/codemod/tsr/Cargo.toml --bin tsr
```

That checkout was at **80ba5796**. The working checkout at
`/Users/mohebifar/dev/tsr` was at **12c84fd0**, with checker code from
e6812e01. Both release binaries were rebuilt and run against the same app.
Untracked files in both checkouts were preserved.

Pinned tsgo (vendor commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`) completed a fresh
`--noEmit --incremental false --pretty false` check in **3.514 seconds**,
exit 0, with no diagnostics. File listing included **13,098 files**, including
**1,343 app files outside node_modules**. TSR's listing included **13,560
files**, with the same 1,343 app files outside node_modules; dependency loading
is not identical and merits a separate comparison.

The requested TSR run was stopped after **3m38s**, at 100% CPU and roughly
1.29 GiB RSS. The newer run was stopped after **3m19s**, at 99.6% CPU and
roughly 1.21 GiB RSS. Neither emitted diagnostics before termination. Those
times exclude the release builds. Stopping the checks is not a successful
typecheck, and zero emitted diagnostics is not a clean result.

One-second macOS samples placed **all 759 requested-version samples and all
756 current-version samples** under this path:

```text
check_source_file → check_node → check_callee_is_callable
→ check_property_access_expression → get_type_of_symbol
→ check_new_expression → get_class_construct_signatures
→ get_signature_from_declaration → get_type_from_type_node
→ get_instantiated_type_reference → evaluate_conditional_alias
→ evaluate_conditional_node → mapped_type_info → further instantiation
```

This localizes the performance blocker to constructor signature resolution
through conditional/mapped types. The exact constructor and repeated type
identities have not yet been isolated. Do not call it infinite recursion on
sampling evidence alone. Follow-up is already tracked by tsr-6.3.

The Rust evaluator at `crates/tsr-checker/src/declared.rs:5662` builds binding
frames and recursively resolves syntax, with a depth guard. Native
`getConditionalTypeInstantiation` at
`vendor/typescript-go/internal/checker/checker.go:22485` caches by effective
outer type arguments, alias and constraint mode. Native
`instantiateTypeWithAlias` at line 22104 also uses active mapper caches and
both depth/count limits. These are concrete comparison targets; a cache or
guard change is not yet a verified fix for this project.

## Currently reproduced diagnostic gaps

Smaller configurations extend the app's actual tsconfig, override `include`
and `exclude` to empty arrays, and select absolute root file paths via `files`.
They set `noEmit: true`, `incremental: false`, `types: ["node"]`, and explicit
app/Next.js node_modules `@types` directories to avoid moving automatic type
discovery to `/tmp`. Every subset also includes the app's `globals.d.ts`.

| Category | Current TSR evidence | Pinned tsgo | Missing behavior / confidence |
|---|---|---|---|
| Barrel export type meaning | Eight TS2709 in `packages/ui/src/form.tsx` for `FieldValues`/`FieldPath` | Clean | Public-entry imports fail while imports directly from `dist/types/fields` and `dist/types/path/eager` succeed. Confirmed barrel/alias resolution gap; exact failure site still needs instrumentation. |
| Type-only alias provenance | One TS1361 in `packages/queue/src/index.ts:78` for `new QueueEvents` | Clean | The app imports `QueueEvents` normally and BullMQ declares an exported class. Normal value imports must not inherit an unrelated type-only restriction. Exact provenance failure remains to trace. |
| JSON modules | Two TS2306 in `packages/ui/src/shiki/theme-data.ts` | Clean | Theme JSON files resolve, but are treated as files without module exports despite `resolveJsonModule`. |
| JavaScript module typing/loading | One TS7016 in `tooling/tailwind/web.ts` for `tailwindcss-bg-patterns` | Clean | The app enables `allowJs`/`checkJs`; native accepts the import. Investigate JS source inclusion and CommonJS/module-symbol construction before changing the diagnostic gate. |
| Assertion comparability | One TS2352 in `packages/ui/src/form.tsx:67` for `{} as FormItemContextValue` | Clean | Independent minimal controls also reject `{} as { id: string }`. Assertion compatibility must allow the native reverse-comparison/overlap behavior. |

The UI/theme/Tailwind subset reports **12** current TSR diagnostics; the
OTel/queue subset reports **one**, all absent from their matching tsgo checks.
These **13 are a reproduced subset, not a current full-project total**.
The imported OTel Logger no longer reports the historical missing `emit` in
this subset. The validator regex loop and encryption return object are clean
in both checkers. The previous standalone optional-array/discriminated-union
contextual inference and assignment-loop controls are also clean in both.

## Historical backlog classification

The earlier `/tmp/tsr-nextjs-cli-run.log` contains 196 diagnostics from an
older run whose compiler SHA is not recorded in that log. Its distribution
helps select probes but must not be represented as the pulled version's result.

| Historical category | Codes | Count | Current interpretation |
|---|---|---:|---|
| Assignment and argument compatibility | TS2322, TS2345 | 76 | Contextual inference, literal retention, generic substitution and structural comparison are candidates. Encryption and the existing contextual control are now clean; the rest need fresh probes. |
| Nullable/indexed access | TS18047, TS18048, TS18049, TS2531, TS2532 | 62 | Flow/reference matching and indexed access are candidates. The regex assignment loop is now clean. |
| Modules, exported meaning and type-only imports | TS2709, TS1361, TS7016, TS2306, TS2305 | 35 | Barrel types, QueueEvents, JSON and Tailwind still reproduce; not every old diagnostic was retested. |
| Missing members | TS2339 | 16 | Alias/member lookup and package export resolution are candidates; Logger.emit is clean in the current subset. |
| Callback context | TS7006 | 4 | Recheck generic overload callback contexts. |
| Definite assignment | TS2454 | 3 | Recheck try/catch/finally and exceptional control-flow paths. |

Prioritize full-project completion first, then barrel/value alias meaning,
JSON/JS module construction, and assertion comparability. Re-run the full
project after the performance fix before estimating residual false positives.
Follow-ups: tsr-6.3 (performance), tsr-6.44 (barrel/value alias meaning),
tsr-6.45 (JSON/JS modules), and tsr-6.46 (assertion comparability).

## Reproduction artifacts

All machine-specific evidence is local under `/tmp`:

- `tsr-nextjs-requested-80ba5796.log` and corresponding `-sample.txt`.
- `tsr-nextjs-current-12c84fd0.log` and corresponding `-sample.txt`.
- `tsgo-nextjs-reference.log`; `tsgo-nextjs-files.log`;
  `tsr-nextjs-current-files.log`.
- `tsr-nextjs-{flow,modules,infrastructure,encryption}-subset.json` and
  corresponding TSR/tsgo `.log` files.
- `tsr-nextjs-barrel-control.{ts,json}` and TSR/tsgo `.log` files: the public
  barrel produces two TS2709; both direct leaf imports are accepted.
- `tsr-nextjs-assertion-control.ts` and TSR/tsgo `.log` files: named and inline
  non-empty object assertions from `{}` both produce TS2352 only in TSR.

Run a subset with the current release binary and the pinned native binary:

```sh
/Users/mohebifar/dev/tsr/target/release/tsr --project /tmp/tsr-nextjs-modules-subset.json --pretty false
/tmp/tsr-95-tsgo --project /tmp/tsr-nextjs-modules-subset.json --pretty false
```

## Debug summary

**Problem:** The real app checks cleanly in pinned tsgo; TSR's full check does
not complete within the observed window and smaller runs report false positives.

**Root cause evidence:** Sampling confirms expensive constructor/type
resolution; public-barrel versus direct-leaf imports isolate an export meaning
gap; actual project roots and independent assertion controls establish the
remaining diagnostic differences. Internal root causes are not fully isolated.

**Recommended tests:** Add Program-backed controls for package re-export
chains, value/type-only import provenance, JSON and untyped JS modules. Extend
the existing assertion diagnostic tests with named and inline `{}` assertions
and incompatible assertions that must continue to report. Minimize the profiled
constructor before testing caching/lazy completion.

**Fix:** Diagnosis only. **Prevention:** Follow-up issues and these controls
preserve the distinction between old output and verified current behavior.
**Confidence:** High in the reproduced differences and sampled hot path;
limited for the unisolated internal mechanisms and untested historical errors.
