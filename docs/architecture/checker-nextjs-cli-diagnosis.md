# Next.js CLI comparison — 2026-10-02

Investigation: tsr-6.43. This is a diagnosis; no checker implementation changed.

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
