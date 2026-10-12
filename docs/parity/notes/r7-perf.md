# Parity lane `r7-perf` (round 7)

Lane: `tsr-2zk.1277`, the dedicated performance-prerequisite lane of round 7
of epic `tsr-2zk`. It succeeds [`r6-checkperf.md`](r6-checkperf.md), whose
§7 is the starting point here. Box protocol: `docs/parity/box-protocol.md`;
lane row: `docs/parity/round7.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Release target (CLAUDE.md): verified TSR/tsgo median wall ratio
<= 0.50 on equivalent complete work.

This lane owns `crates/tsr` and `crates/tsr-execute` (the driver; the
integrator confirmed that `tsr-execute` belongs to the driver on
2026-10-10), `crates/tsr-compiler` and `perf_links.rs`. Sections are
numbered so code and reports can cite them (`r7-perf.md` §N).

## §1 Method and base

- **Base**: `origin/main` @ `9020aa67`, frozen before any edit with
  `scripts/parity_gate.sh freeze /tmp/box/base`. That gives 12,238
  diagnostics rows and 556,357 type rows (right 551,176).
- **Container**: Amp orb, 4 vCPU, 7 GiB, pinned Rust 1.96.0, glibc
  `malloc`. Native oracle: `target/tsgo-pinned` (rev `5b1047d`).
- **Ratios**: `scripts/whole_project_perf.py` (default multi-threaded mode,
  `--noEmit --incremental false --composite false --pretty false`). TSR and
  the other binary alternate in fresh processes. The tables show median wall
  and median child CPU (`user + system` from `wait4`). There are 21 samples
  on the bench projects and 5 on jsTyping.
- **jsTyping**: the r4/r5 scratch config
  `vendor/typescript-go/_submodules/TypeScript/src/jsTyping/tsconfig.perf.json`
  (`{"extends": "./tsconfig.json", "compilerOptions": {"types": [],
  "pretty": false, "noEmit": true}}`). It is untracked in the submodule and
  not committed.

**Base ratios** (TSR/tsgo):

| project | wall | CPU | TSR wall | tsgo wall | `diagnostics_match` |
|---|---:|---:|---:|---:|---|
| domain-model | 0.801 | 0.517 | 0.164 s | 0.205 s | true |
| domain-model-large | 0.774 | 0.561 | 0.531 s | 0.686 s | true |
| generic-imports | 0.878 | 0.427 | 0.060 s | 0.069 s | true |
| jsTyping | 4.416 | 4.334 | 8.458 s | 1.915 s | **false** (423 vs 86) |

jsTyping is not equivalent work: TSR reports 339 diagnostics that tsgo does
not, and misses 2 (§4). Its ratio cannot verify the target until that gap
closes.

**STATUS.md §1's jsTyping causes are stale.** STATUS.md names two causes:
`resolving_signature_calls` not being parked, and the `members.rs:3233`
composite-name decline answering 137k of 138k asks. Both are already
handled. r6-checkperf §2 landed the main park, and r6-checkperf §3 measured
the composite enumeration at 431 declines and 61 M Ir (0.09% of the check
phase), so it is not a lever. This lane does not touch `calls.rs` or
`members.rs` for either.

## §2 JSDoc deferral in non-JavaScript files (`tsr-2zk.17.1`, ADR-0053)

This is r6-checkperf §4's held diff, landed with the integrator's approval
(2026-10-10). It needed parser hunks in r7-parser's files and one-line
`ParseOptions` literals in the binder and checker benches. The ADR is
renumbered from 0051, which the lone-surrogate record took, to
[ADR-0053](../../adr/0053-jsdoc-deferred-in-checked-ts-files.md). The ADR
holds the forcing measurement, the native anchors (`withJSDoc`,
`internal/parser/jsdoc.go:56`; `jsdocScannerInfoHasSeeOrLink`) and the
falsifiers. It is not a cache, so the checker-port cache record does not
apply.

**Re-measured on this base:**

- Both unfiltered dumps are byte-identical to the base after the
  `ms=`/`mib=` columns are cut. `parity_gate.sh compare` reports 0 gained,
  0 lost and 0 missing.
- The summed diagnostics-dump case time fell from 612.9 s to 411.6 s. The
  harness parses through the same gated loader.
- CLI output (`--pretty false`) is `cmp`-identical to the base binary on
  domain-model, domain-model-large, generic-imports and jsTyping.
- Coverage bin rows are unchanged: `checker_types` 8,677/9,538,
  `checker_types_configured` 1,752/1,928, `diagnostics` 4,786/5,502,
  `diagnostics_configured` 955/1,091.
- `cargo test --workspace --release`: 3,752 passed, 0 failed, 17 ignored.

Perf against the frozen base binary (new/base, 21 samples):

| project | wall | CPU |
|---|---:|---:|
| domain-model | 0.888 | **0.957** |
| generic-imports | 0.728 | **0.735** |

TSR/tsgo after the change (21 samples, same session):

| project | base wall | **new wall** | new CPU |
|---|---:|---:|---:|
| domain-model | 0.801 | **0.706** | 0.509 |
| domain-model-large | 0.774 | 0.781 | 0.569 |
| generic-imports | 0.878 | **0.648** | 0.317 |

domain-model-large's wall moved within its run-to-run spread. It parses the
same lib set, so its absolute saving matches the others (about 118 M Ir,
r6-checkperf §4), but that saving is 2.7% of its Ir.

## §3 `tsr-2zk.1258`: the multi-threaded CLI diagnostic drop (reproduced)

**Experiment.** A scratch materializer (in `/tmp/box/mt`, not committed)
wrote every multi-file conformance case to disk: 3,065 cases with at least
two `@filename` units and no embedded `tsconfig.json` or symlinks. Each
case's `@` options became `compilerOptions`. The materializer then ran the
base binary twice per case, with `--singleThreaded` and with
`--checkers 4`, and compared stdout and exit code.

**Result.** 2 of 3,065 cases differ. In both, the multi-checker run drops
diagnostics:

| case | dropped under `--checkers 4` |
|---|---|
| `compiler/mergedClassWithNamespacePrototype.ts` | `b.ts(2,15)` TS2300 `Duplicate identifier 'prototype'.` |
| `compiler/duplicateIdentifiersAcrossFileBoundaries.ts` | `file2.ts(3,10)` TS2814, `file2.ts(4,7)` TS2813 |

tsgo-pinned, in its default 4-checker mode, reports all three.

**Root cause: not the driver.** `checker_pool.rs` mirrors
`checkerpool.go`. Each checker publishes only diagnostics on files it owns,
as native's `GetDiagnostics(file)` asks the file's own checker. The drop
happens because TSR issues these two diagnostics from the wrong trigger. A
checker that does not own the target file issues them, and the owning
checker never does:

1. **TS2300 `prototype`** comes from `check.rs`
   `check_merged_namespace_prototype`, which runs when the *class* is
   checked (`a.d.ts`) and reports onto the namespace in `b.ts`. Native
   reports it while merging (`mergeSymbolTable` → `reportMergeSymbolError`,
   reached from `initializeChecker`), so every checker has it before any
   file is checked. The faithful home is the program-level merge report
   that every checker runs (`merge_conflicts.rs`
   `report_merge_conflicts`, fed by the binder's recorded pairs). That
   needs the binder (r7-shared) to record the class-`prototype` collision,
   or `check.rs` to move the check into the per-checker merge report.
2. **TS2813/TS2814** come from `class_function_merge.rs`
   `check_class_function_merge`, which runs only from the symbol's
   *first* class or function declaration. Native's
   `checkFunctionOrConstructorSymbolWorker` runs once per symbol per
   checker, from whichever declaration that checker reaches first
   (`links.functionOrConstructorChecked`, `checker.go:3463`). The checker
   that owns `file2.ts` therefore reaches it from `file2.ts`'s declaration.
   The fix is a per-checker once-per-symbol link in place of the
   first-declaration test.

The conformance harness checks with one checker, so both cases already
score RIGHT there. The drop is visible only in the multi-threaded CLI.
Neither fix is in this lane's files; both are routed in the report.

## §4 jsTyping's diagnostic delta, classified

The base binary run through the harness's flags gives TSR 423 diagnostics
and tsgo 86. TSR has 339 extra and misses 2 (TS7006 and TS7031 at
`sys.ts(1696)`).

| code | extra |
|---|---:|
| TS2769 | 164 |
| TS2345 | 74 |
| TS2339 | 32 |
| TS18048 | 16 |
| TS2678 | 15 |
| TS2322 | 14 |
| TS2540 | 8 |
| other | 16 |

Clusters with cut-down repros. Each repro prints nothing on tsgo-pinned
and prints the diagnostic shown on TSR. The `lib`/`target` settings are
es2020 with `strict`.

1. **Intersection-target inference priority**: `NonNullable<TIn>`, i.e.
   `TIn & {}`. This covers 157 of the 164 TS2769, all on
   `nodeVisitor(...)`, `visitNode(...)` and `visitNodes(...)` calls in
   `visitorPublic.ts` and `transformers/*`, and likely some TS2345s.
   ```ts
   declare function v3<TIn extends Node | undefined>(node: TIn, visitor: (n: NonNullable<TIn>) => void): TIn;
   export function f(visitor: (n: Node) => void, t: Node | undefined) { v3(t, visitor); }
   // TSR: TS2345 'Node | undefined' is not assignable to parameter of type 'Node'.
   ```
   Native, in `inferFromTypes` (`inference.go`), infers to the naked type
   variable of an intersection target through `inferToMultipleTypes` at
   `InferencePriorityNakedTypeVariable`. The contravariant candidate from
   `NonNullable<TIn>` therefore loses to the direct candidate from the
   first argument. TSR picks `Node`. Owner: `inference.rs` (r7-contextual).
2. **Inference from an interface extending `ReadonlyArray<T>` instantiated
   with a union type alias**: about 25 TS2345, plus follow-on TS2339/TS2322
   on `T`. Examples are `forEach`/`findIndex`/`first`/`lastOrUndefined` on
   `NodeArray<CaseOrDefaultClause>`, `NodeArray<ModifierLike>` and
   `NodeArray<ArrayBindingElement>`, all of whose arguments are aliases of
   unions.
   ```ts
   interface NodeArray<T> extends ReadonlyArray<T> { readonly hasTrailingComma: boolean }
   interface C1 { kind: 1 } interface C2 { kind: 2 }
   type CC = C1 | C2;
   declare const xs: NodeArray<CC>;
   declare function f3<T>(array: readonly T[] | undefined): T;
   export const a = f3(xs);
   // TSR: TS2345 Argument of type 'NodeArray<CC>' is not assignable to parameter of type 'readonly T[]'.
   ```
   The same code with `NodeArray<C1 | C2>`, written without the alias,
   passes. Owner: `inference.rs` (r7-contextual). The alias-carrying
   instantiation may be r7-declared/r7-shared's (`instantiate`).
3. **Type-predicate narrowing of a union that has a non-matching member**:
   18 TS2345 on `string | (void & { __escapedIdentifier: void; })` against
   `__String` in `checker.ts` and `utilities.ts`.
   ```ts
   const enum ISN { Call = "__call", New = "__new" }
   type __String = (string & { __e: void; }) | (void & { __e: void; }) | ISN;
   interface Ident { escapedText: __String }
   declare function use(n: __String): void;
   declare function isString(text: unknown): text is string;
   export function f(nameArg: __String | Ident) {
       const name = isString(nameArg) ? nameArg : (nameArg as Ident).escapedText;
       use(name);  // TSR: TS2345 'string | (void & { __e: void; })' ...
   }
   ```
   Native's `getNarrowedType` (`flow.go:846`) keeps each declared member
   that is a strict subtype of `string`, giving `(string & {__e}) | ISN`.
   TSR answers `string`. The same narrowing of `__String` alone works.
   Owner: `flow.rs` (r7-flow).
4. **`getFlowTypeOfDestructuring` is unported**: TS18048 on destructured
   names (`tsbuildPublic.ts` 7 lines, `program.ts` 3 lines, `builder.ts`).
   ```ts
   interface State { readonly cache?: { orig(): void } }
   export function d1(state: State) {
       if (!state.cache) return;
       const { cache } = state;
       cache.orig();  // TSR: TS18048 'cache' is possibly 'undefined'.
   }
   ```
   Native's `getTypeForBindingElement` (`checker.go:17743`/`:17771`) calls
   `getFlowTypeOfDestructuring` (`:17849`), which narrows a synthetic
   `state.cache` reference. `destructuring_assignment.rs:32` records it as
   not ported. Owner: `binding_patterns.rs`/`destructure.rs`
   (r7-contextual).
5. **`new` with more than one construct signature, where the argument is a
   generic call that needs return-type contextual inference**: 5 of the
   remaining TS2769 (`new Map(xs.map((x, i) => [x, i]))` in `checker.ts`
   and `sourcemap.ts`, and `utilities.ts:1211`), plus the `(string |
   CommentDirective)` TS2339/TS2322/TS2345 follow-ons.
   ```ts
   interface C2 { new (): { any: true }; new <K, V>(entries?: readonly (readonly [K, V])[] | null): { k: K; v: V } }
   declare const B: C2;
   declare function id2<U>(f: U): U[];
   declare const s: string;
   export const b = new B(id2([s, 1]));
   // TSR: TS2345 '(string | number)[][]' is not assignable to 'readonly (readonly [string | number, string | number])[]'.
   ```
   The same overload set as a *call* signature (`G(id2([s, 1]))`) passes,
   and a single construct signature passes. So the inner call loses its
   contextual tuple type only on the construct-overload path. Owner:
   `calls.rs` `check_new_expression_*` / `expressions.rs`
   `check_new_expression` (r7-calls).
6. **Homomorphic mapped type over a union keeps `readonly`**: 8 TS2540
   (`binder.ts` `Mutable<ModuleDeclaration | SourceFile>`, `parser.ts`,
   `nodeFactory.ts`).
   ```ts
   type Mutable<T extends object> = { -readonly [K in keyof T]: T[K] };
   interface A { readonly flags: number; readonly a: string }
   interface B { readonly flags: number; readonly b: string }
   export function f(node: Mutable<A | B>) { node.flags |= 1; }
   // TSR: TS2540 Cannot assign to 'flags' because it is a read-only property.
   ```
   Native distributes a homomorphic mapped type over a union
   (`instantiateMappedType` → `mapTypeWithAlias`), so each member drops its
   modifiers. Owner: `mapped.rs` (r7-declared).
7. **Not reduced yet. A narrowed `input` loses members in
   `transformers/declarations.ts` `visitDeclarationSubtree`**: 15 TS2678 and
   14 TS2339 on `never`. After `isProcessedComponent(input)`, TSR's union
   lacks `GetAccessor`/`SetAccessor`/`TypeParameter` (native keeps them).
   `utilities.ts`' `isNamedEvaluation` shows the same thing after
   `isNamedEvaluationSource(node)`: `Parameter`/`BindingElement` are lost.
   Those members are intersections with `readonly dotDotDotToken:
   undefined`. Two hand-written reductions (in `/tmp/box/r/r4`) did *not*
   reproduce, so the trigger depends on more of `types.ts`. Probable owner:
   `flow.rs` (r7-flow, `narrowTypeByTypePredicate`).
8. **Not reduced yet. Predicate narrowing of a property-access reference to
   a tuple element**, e.g. `declaration.arguments[2].properties` after
   `isBindableObjectDefinePropertyCall(declaration)`, and
   `node.argumentExpression.text` after `isStringOrNumericLiteralLike(...)`.
   This covers about 10 TS2339/TS2740. The simple property-path form passes
   on TSR (`/tmp/box/r/r6`, `d2`), so the trigger is the element access
   into a `readonly [...]` tuple property. Probable owner: `flow.rs`.

The remaining ~20 are singletons (TS2454, TS2352, TS7053, arithmetic
operators on `string | number`) that are probably downstream of 1–8. They
need re-measuring once those land.

## §5 The lane's numbers

Ratios are TSR/tsgo, wall · CPU, with 21 samples (jsTyping 5). This
container's runs differ from one another by several percent; within a row
the binaries ran in one session. From §6 on, every row carries both
profiles (ADR-0054).

| state | profile | dm | dml | gi | jsTyping wall |
|---|---|---:|---:|---:|---:|
| base `9020aa67` | release | 0.801 · 0.517 | 0.774 · 0.561 | 0.878 · 0.427 | 4.416 (not equivalent) |
| §2 JSDoc deferral | release | 0.706 · 0.509 | 0.781 · 0.569 | 0.648 · 0.317 | — |
| §2, §6's session | release | 0.662 · 0.523 | 0.749 · 0.569 | 0.743 · 0.356 | — |
| §2, §6's session | **dist** | 0.692 · **0.460** | 0.744 · **0.540** | 0.738 · 0.366 | 4.159 (not equivalent) |

In §6's session the gi and dm walls are noisy against the
`dist`/`release` self-comparison (§6); the CPU column is the stable one.
The 0.50 target is not met on any project.

## §6 The `dist` profile is fat LTO, and the release ratio is measured on it (ADR-0054)

The integrator approved this as a build decision (2026-10-10). This lane
owns `Cargo.toml`'s `[profile.dist]` for this one commit.
`[profile.dist]` goes from thin to fat LTO with `codegen-units = 1`.
[ADR-0054](../../adr/0054-the-release-ratio-is-measured-on-the-dist-profile.md)
records the measured-and-shipped rule, and
`docs/architecture/whole-project-performance.md`'s reproduce block now
builds `target/dist/tsr`. Gates stay on `release`.

`dist`/`release` on the same tree (§2's commit), with the `release`
binary in the harness's `--tsgo` slot:

| project | wall | CPU | samples |
|---|---:|---:|---:|
| domain-model | 0.894 | **0.905** | 21 |
| generic-imports | 0.950 | **0.959** | 21 |
| domain-model-large | 0.922 | **0.900** | 21 |
| jsTyping | 0.921 | **0.942** | 5 |

An earlier probe used `CARGO_PROFILE_RELEASE_LTO=fat`, the same settings
through the environment. It read 0.934, 0.967 and 0.943 CPU on the first
three projects. CLI output is `cmp`-identical to the `release` binary on
dm, dml and gi, and `diagnostics_match` holds against the base on jsTyping.
A from-scratch `cargo build --profile dist -p tsr` takes 3 min 11 s on this
container.

No workflow builds a shipped `tsr`. `perf.yml` builds only the parser
example and bench. A whole-project job building `target/dist/tsr` and
reporting both profiles is proposed to the integrator, who owns
`.github/`.

## §7 The front end, `tsr-2zk.17.3`, the allocator and `tsr-2zk.1261`

**Front end after §2** (`--extendedDiagnostics`, three runs each):

| project | parse | bind | check |
|---|---:|---:|---:|
| generic-imports | 23–31 ms | 11 ms | 1 ms |
| domain-model | 35 ms | 15 ms | 66–74 ms (4 checkers) |

On generic-imports the critical path is `lib.dom.d.ts`, about 83% of the
parsed bytes and 80% of the bound nodes. It is parsed straight into the
shared tables, then bound serially (r5-loader §3's gate: largest/total > 0.5).
r5-loader §6.2 measured moving its parse to a worker: the AST publication
copy (0.31 of the parse) then lands on the coordinator, with nothing to
overlap it on this project. The same holds for a private bind of
`lib.dom` (publication about 0.45 of the bind). Either change wins only
once publication stops copying. That is `tsr-2zk.17.4` (rebasable node
ids: about 900 `node_id` read sites in generated `tsr-ast` code) and
`tsr-2zk.17.5` (per-file binding with id-offset merge). `tsr-2zk.17.3`
records itself as blocked on both. Neither is in this lane's files
(`tsr-ast`, `tsr-parser`, `tsr-binder`), and each needs its own design
record, because it changes ADR-0003/ADR-0034's node-table ownership. They
are not attempted here.

The one front-end piece inside this lane's files is a pipelined bind. Here
the coordinator would bind `lib.dom` directly into the canonical tables
while workers privately bind the files after it. On domain-model that
publication is about 78 K nodes at about 0.45 of the bind, so the estimate
is ≤2 ms out of a 15 ms bind. It was not built.

**The allocator** is about 11% self of domain-model's single-threaded
profile (`malloc`, `_int_free`, `cfree`, `_int_malloc`,
`malloc_consolidate`, `realloc`). On generic-imports, kernel page-fault
handling is about 12% (`do_user_addr_fault`, `clear_page_erms`,
memcg charge). Two glibc tunings were measured through `GLIBC_TUNABLES`,
with no code change: `glibc.malloc.hugetlb=1` (THP for malloc, wall
0.993 / CPU 0.994) and a 256 MiB `top_pad` + `mmap_threshold` (0.991 /
0.988), 21 samples on generic-imports. Both are noise, so no
`mallopt`/`unsafe` hook was proposed (ADR-0011). The first-touch faults
are the memory the parse and bind write. A different global allocator
would need a new dependency, and boxes cannot change `Cargo.lock`. So the
allocator share stays with `tsr-2zk.1092` (Signature copies, r5-checkperf2
§2): fewer allocations, not a faster allocator.

**`tsr-2zk.1261` (ramdaToolsNoInfinite2): not reproducible on main.**
r6-declared3 §1 measured the per-call alias-frame map with r6-declared2's
*held binder diff* applied (12.9 G Ir, 1.4 s). On `a51bc525` the case
takes 182 ms in the diagnostics dump and 0.35 s wall in a filtered
`verdictdump` (`TOTAL 493 right 474`). The cost appears only on the held
diff's path, so `.1261` should be re-measured when that diff lands, and not
before. The corpus's slowest cases on this base are
`performanceComparisonOfStructurallyIdenticalInterfacesWithGenericSignatures`
(3.9 s), `relationComplexityError` (1.7 s) and `ramdaToolsNoInfinite`
(1.7 s).

## §8 The CLI process does not tear down what it is about to exit from

**Forcing measurement.** After the report is written, `run_compilation`
still dropped the `Program`: node table, node map, `BindResult` hash maps,
per-file vectors and the arena. Each checker worker also dropped its
`Checker`'s type, symbol-link and relation tables before `std::thread::scope`
joined it. That work is on the critical path. The pool waits for the
slowest worker's drop, and the process cannot exit until `main` returns.
Native `tsc` (tsgo) does none of it: the process exits without a final
collection, and the operating system reclaims the heap.

**Change** (`tsr-execute`, the driver):

- `System::exits_after_command()` defaults to `false`. `OsSystem`, the
  `tsr` binary's host, answers `true`.
- When it is `true`, `run_compilation` `mem::forget`s the `Program` and the
  arena once the exit status is known. `check_program_files` gets the
  answer and `mem::forget`s each checker after its diagnostics are
  collected.
- The baseline runner and the test hosts keep the default, so a
  long-running process that runs many commands drops everything as before.

This is not a cache, side table or traversal, so the checker-port record
does not apply. No answer depends on it. The forgotten values are only
memory: no file handle, lock or thread is held by a `Program` or `Checker`.

**Measured** (base: this branch's previous commit, `9242e12f`'s `release`
binary; `/tmp/box/ab.py`, interleaved fresh processes, 21 samples):

| project | wall | CPU |
|---|---:|---:|
| domain-model | 0.959 | 0.972 |
| generic-imports | 0.965 | 0.960 |

The box-protocol harness (`whole_project_perf.py`, the frozen `9242e12f`
binary in the `--tsgo` slot, 21 samples) gives domain-model 0.979 wall ·
**0.945** CPU and generic-imports 0.978 · **0.980**. The output is
`cmp`-identical on dm, dml and gi. Both dumps are unchanged (`compare`
against the `9242e12f` freeze: 0 lost, 0 missing; the conformance harness
does not use `OsSystem`). `cargo test --workspace --release`: 3,753 passed,
0 failed.

**How we would know it is wrong.** A host that answers `true` and runs a
second command in the same process: memory would grow by one program per
command. `OsSystem` is constructed once per `main` and runs exactly one
`command_line`.

## §9 `GetProgramDiagnostics` reaches the CLI and the diagnostics suite

**Forcing case.** `compiler/pathsValidation5` run as a CLI project: tsgo
prints three TS5090 (`Non-relative paths are not allowed`) on
`tsconfig.json`, and TSR printed one TS2882 on `src/main.ts`.
`verify_options.rs` already ports `verifyCompilerOptions`
(`program.go:751`), but only the full oracle (`full_oracle.rs:506`) called
it. Neither the CLI nor the legacy `diagnostics` suite ever saw
`programDiagnostics`. The integrator granted the
`diagnostics_suite.rs` wiring to this lane (2026-10-10).

**Native.** `Program.GetProgramDiagnostics` (`program.go:698`) is
`SortAndDeduplicate(programDiagnostics ++ includeProcessor global)`. Its
two consumers treat it differently:

- `GetDiagnosticsOfAnyProgram` (`program.go:1782`, the CLI) appends it only
  when there are no syntactic diagnostics. When it, or
  `GetGlobalDiagnostics`, is non-empty, that function never asks for the
  semantic set, so no checker runs. `listFilesOnly` still gets program
  diagnostics.
- The test runner's `compileFilesWithHost` (`harnessutil.go:633`)
  concatenates config, program, syntactic, semantic, global and declaration
  diagnostics **without** that gate.

**Change.**

- `program_diagnostics.rs`: `program_level_diagnostics` (verify plus
  `global_program_diagnostics`) and `semantic_diagnostics_are_asked`.
  `diagnostics_of_any_program` now takes the program-level set, applies the
  native gate, and returns `(file name, diagnostic)`. The CLI is its only
  caller.
- `compile.rs`: the config's `ConfigSyntax` is kept from the parse. The
  program-level set is computed before the pool, and the pool skips
  `check_source_file` when `semantic_diagnostics_are_asked` is false.
  `js_syntax` is still collected, because it is syntactic. A JavaScript
  file's `js_syntax` can only close the gate after checking, which is the
  one remaining case of checking work that is dropped.
- `diagnostics_suite.rs`: `program_level_diagnostics` runs against the
  case's tsconfig unit (the same input as `full_oracle.rs`), positioned
  through the unit text with no gate. Config parse diagnostics and these now
  share one positioning helper (`located_in_units`).

This is not a cache, side table or traversal. `verifyCompilerOptions` runs
once per program, as upstream's does at program creation.

**Measured** (on main `660718af`, against its own freeze):

- Diagnostics, WRONG → RIGHT: `pathsValidation5` and
  `pathMappingBasedModuleResolution1_node` (TS5090),
  `commonSourceDirectory_dts` (TS5011),
  `declarationEmitToDeclarationDirWithoutCompositeAndDeclarationOptions`
  (TS5069), and `bundlerOptionsCompat`. No other case's actual list moved.
  Types are unchanged. `compare`: 0 lost, 0 missing.
- Coverage bin on the pre-rebase tree (`1b466dc8` plus this branch):
  `checker_types` 8,682/9,538, `checker_types_configured` 1,753/1,928,
  `diagnostics` 4,821/5,502, `diagnostics_configured` 958/1,091. The bin
  was not run on that base alone, so the rows carry no measured delta;
  the dump comparison above is the delta.
- The CLI now prints tsgo's three TS5090 on `pathsValidation5` as a
  project. Output on dm, dml, gi and jsTyping is identical to the previous
  binary.
- Two `tsr-execute` tests passed `--target es5` only to name `lib.d.ts`.
  That is TS5108 (`Option 'target=ES5' has been removed`), which now
  closes the semantic gate, so they use `--target es2015` and
  `lib.es6.d.ts`. Native would not check those programs either.
- `cargo test --workspace --release --no-fail-fast`: 3,764 passed,
  0 failed. Strict `cargo clippy --workspace --all-targets -- -D warnings`
  is clean.
- Perf, the three r7-perf commits on `660718af` against its frozen binary
  (21 samples, child CPU): domain-model 0.988, generic-imports 0.976.

## §10 The `tsr` binary allocates with mimalloc (ADR-0055)

The integrator approved this on 2026-10-10, conditionally. It needed
interleaved A/B at 41 samples ≤ 1.00 child CPU on all four projects; the
options set in `main.rs` rather than the environment; and an ADR recording
the `unsafe` exception and the refused alternatives. This lane owns
`Cargo.toml`/`Cargo.lock` for the commit.
[ADR-0055](../../adr/0055-the-tsr-binary-allocates-with-mimalloc.md) has
the decision, the variant table (v3, v3 + `no_thp`, v2, v2 + purge off) and
the ADR-0011 exception: one `mi_option_set(purge_delay, -1)` at the start
of `main`.

**Measured** (the committed binary against its glibc parent `c36a7706`,
`/tmp/box/ab.py`, interleaved, 41 samples):

| project | wall | child CPU |
|---|---:|---:|
| generic-imports | 0.933 | **0.949** |
| domain-model | 0.820 | **0.833** |
| domain-model-large | 0.819 | **0.826** |
| jsTyping | 0.831 | **0.775** |

CLI output is `cmp`-identical to the glibc binary on all four projects. The
gates are unaffected by construction: only `crates/tsr` links the
allocator, and the conformance harness, tests and examples keep glibc.

## §11 Measured and refused: pre-sizing the node tables for the lib set

**Hypothesis.** Each file's parse grows the shared `NodeTable`/`NodeMap`
by `len / 10` rows. The growth steps copy rows already written, which
glibc's `mremap` avoids for large blocks and mimalloc does not. That could
explain generic-imports' small mimalloc deficit. **Change tried**
(`FileLoader::reserve_for_bundled_libs`, not committed): reserve once for
the whole bundled lib set's text (3.79 MB / 10 rows) before the walk.

**Measured** (41 samples, interleaved, generic-imports): glibc 1.003 wall /
1.000 CPU; mimalloc + reserve 1.002 / 1.024 against mimalloc 0.985 / 1.008.
It is noise at best, so it was refused. purge off (§10) is what closed
generic-imports' gap.

## §4 correction: the jsTyping classFields TS2345s were transient

Report 2 (and this note's earlier revisions of §4) said batch 2 added 6
TS2345 at `classFields.ts:2794`–`2922`. They are real on the binary built
from `1c204cd1`, which is batch-2 main `1b466dc8` plus this lane's
teardown commit (`/tmp/box/after4/tsr`): 429 diagnostics, 18 in
`classFields.ts`, 6 in lines 2700–2999. They are absent from `9020aa67`
(423 / 12 / 0) and from `c36a7706` on batch-5 main (259 / 1 / 0), using
the same command (`tsconfig.perf.json --noEmit --incremental false
--composite false --pretty false`). r7-calls tested `660718af` and later
builds, where they had already gone. So a batch-2 regression was fixed by
batch 3 or 4, and there is nothing left to route. jsTyping now reports 259
against tsgo's 86 on `c36a7706`.

## §12 The design record for `tsr-2zk.17.4`/`.17.5` (ADR-0056, proposed)

The integrator asked for it on 2026-10-10.
[ADR-0056](../../adr/0056-rebasable-node-ids-and-per-file-bind-for-a-zero-copy-front-end.md)
proposes rebasable node ids: an `AtomicU32`-backed `node_id`, a rebase
pass at publication, and per-file arenas kept in a program-owned pool. It
pairs them with the existing per-file bind and id-offset merge. The
measured input comes from `crates/tsr-compiler/examples/front_end_ceiling.rs`
(new; release, minimum of 7 rounds per file, 4 workers). Today's path to
zero-copy saves generic-imports 2.8–5.1 ms, domain-model 12.3–15.1 ms
and domain-model-large 22–30 ms per process. The floor on every bench is
`lib.dom.d.ts`'s single-threaded parse and bind, about 22 ms. Not built;
funding is the integrator's decision.

## §13 Where the ratios stand after §10

TSR/tsgo, `whole_project_perf.py`, 21 samples, one session, the binary at
`4c765057` (mimalloc). This session's absolute times ran 10–25% above the
§10 A/B session's; the ratios are within one run.

| project | release wall · CPU | **dist wall · CPU** |
|---|---:|---:|
| domain-model | 0.616 · 0.438 | **0.540** · 0.416 |
| domain-model-large | 0.745 · 0.526 | **0.716** · 0.479 |
| generic-imports | 0.665 · 0.330 | **0.598** · 0.303 |

jsTyping is still not equivalent work (259 diagnostics against 86 on
`c36a7706`). The 0.50 target is not met. domain-model-large is furthest
off, and its remainder is the check phase.

## §14 domain-model-large's remaining wall: checker 0 prints types on the success path

**Pool balance** (a temporary per-checker timer in `checker_pool.rs`, not
committed; three runs of the release binary, mimalloc). Checker 0 finishes
in 469–660 ms. Checkers 1–3 finish in 248–364 ms. Each checker has 50 or 51
files. The extra is one file: `src/main.ts` (program index 264, so
264 % 4 = checker 0, the same assignment as native `checkerpool.go:115`)
took **395 ms**. Single-threaded, after every model file was already
checked, it takes about 155 ms, and its cost is linear in its 200
`run()` blocks (K = 25 / 50 / 100 / 200 blocks: 21 / 35 / 68 / 155 ms).

**Against tsgo, single-threaded** (`--singleThreaded`, check time, three
runs; `/tmp/box/dmlk<K>` copies with `main.ts` cut to K blocks):

| K blocks | TSR check | tsgo check |
|---:|---:|---:|
| 25 | 0.93–1.04 s | 0.67–0.77 s |
| 200 | 1.09–1.26 s | 0.74–0.86 s |

TSR's single-threaded checker is about 1.4× tsgo's on this project. The
multi-threaded CPU ratio (0.48–0.53) flatters it, because tsgo's
goroutines spend CPU that does not shorten its wall. Per `main.ts` block
TSR pays about 1.1 ms against tsgo's about 0.3 ms.

**Where a block's time goes** (`perf`, dwarf call graphs, inclusive sample
delta between K = 200 and K = 25, single-threaded): 249 of the 339-sample
increase under `check_single_candidate_arguments` is
`objects.rs` `check_object_literal_members` → `member_text_at` →
`type_to_string_at` → `qualified_name_at` → `symbol_chain` →
`symbol_accessibility` (`try_symbol_table`,
`alternative_containing_modules`, `same_reference`). Every object literal
mints its type with its members' text rendered **at the literal's site**
(§735's render-at-reference). In `main.ts`, each
`describeModelNNNEvent({ kind: "created", item: createdNNN.value })`
renders `item`'s interface name by searching the accessible symbol chain
through a file with 600 imported names. No error is ever printed. Native
prints on error paths only. `get_type_at_flow_condition` /
`get_type_at_flow_branch_label` (175 samples, mostly the narrowing of
`createdNNN.ok`/`.value` inside the long function) come next.

Share of all CPU (4 checkers): `member_text_at` is **6.2%** on dml (17.3%
of checker 0, the critical path) and 2.1% on dm (7.0% of checker 0).
Making the literal's member text lazy, read through the
`PrintedSlot::on_demand` road that `AnonymousProperty` already has, or
through an ADR-0052 print-time plan, would take about 17% off checker 0
on dml. The type's display text currently enters `store.new_named` at
mint (`objects.rs:2318`), so laziness needs the store to accept deferred
text. That is the printer lane's (`printing.rs`, `symbol_accessibility.rs`)
and the contextual lane's (`objects.rs`) to build. It is routed through
the integrator, not built here.

## §15 Object-literal member text is rendered at its site only when a composer asks

The integrator granted this on 2026-10-10:
`check_object_literal_members`' mint site in `objects.rs`, the type
store's deferred display text, and `PrintedSlot` plumbing. r7-contextual,
r7-printer and r7-shared keep off those spots. §14 is the forcing
measurement: `member_text_at` was 6.2% of dml's CPU and 17.3% of checker 0.

**Pinned native operation.** `checkObjectLiteral` (`checker.go:13225`)
builds the literal's members and never prints them. A member's display
exists only when a node builder serializes it: `addPropertyToElementList`
→ `serializeTypeForDeclaration` (`nodebuilderimpl.go:2486`, `:2181`) at a
print site, which happens on an error path or in a `.types` writer.
§735's render-at-reference (`member_text_at`) asked
`type_to_string_at(member, literal)` for every plain property at the mint,
the work native never does.

**Probe first** (`/tmp/box/exp1`, not committed): with `member_text_at`
answering the site-free print for every member, the unfiltered dumps
moved by 4 type rows and no diagnostics. One was a loss,
`jsxFunctionTypeChildren:0:29`, where a spread composed the batch literal's
member text and printed `Element` for `JSX.Element`. So the site render is
read almost only by composers that build another type's text from the
member's own: spreads, const/inference images, declared instantiations.
Each reaches it through the canonical reader
`Checker::property_printed_type`, which is `&mut` and can therefore render
on demand.

**Change.**

- `PrintedSlot::at_site(member_type, literal)` (`objects.rs`) is a slot
  whose text is the member's render at the literal's node, asked by
  `property_printed_type` through `Checker::site_display_text` (the same
  `type_to_string_at`, with the same site-free fallback, as
  `member_text_at`).
- `check_object_literal_members`' plain property path gives its
  `AnonymousProperty` that slot, except when the pseudochecker's
  single-quoted reuse applies or `member_text_at`'s pending-return guard
  holds; both keep today's eager text. The literal's own display
  (`render_object_type`, `object_literal_members`) takes the member's
  site-free print, and `literal_property_members` builds the sorted member
  list without asking any slot.
- Methods, accessors and the other `member_text_at` callers are unchanged.

**Convention record.**

- *Key identity and owner:* `(member TypeId, literal NodeId)` in
  `TypeStore::site_display_texts`, owned by the literal's checker.
- *Publication states:* absent (nobody has asked), then published once and
  immutable (`publish_site_display_text` keeps the first text).
- *Receiver, alias and site context:* exactly the literal node, captured at
  the mint, as `member_text_at` used. The render's scope chain, aliases and
  type-parameter names are those of `type_to_string_at` from that node.
- *Expensive-work boundary:* `symbol_chain`/`symbol_accessibility`, about
  0.25 ms per named member in a file with 600 imports. It now runs at most
  once per `(member, literal)`, and only for members whose text is
  composed. It is display metadata only: it enters no intern key, and the
  relater, inference and narrowing read type ids.

**Measured, first cut (superseded by §15.1)** (base `92fe8f05` plus this branch's docs commits, frozen as
`/tmp/box/base9`):

- Diagnostics dump byte-identical. Types dump: 3 rows change, all
  `conformance/typeParametersAvailableInNestedScope3` WRONG → RIGHT
  (`:0:0`, `:0:17`, `:0:20`). The nested literal's `a` now prints
  native's `<T_2>` instead of the mint-time `<T_1>`, because its
  composer asks at its own render depth. `compare`: 3 gained, 0 lost, 0
  missing.
- A/B, interleaved, 21 samples, new/base: domain-model-large wall **0.825**
  · CPU 0.952; domain-model wall **0.933** · CPU 0.954.

### §15.1 The landed cut: anonymous members keep the eager render

*This corrects the first cut above.* Two audits narrowed it before it
became landable.

**CLI-message audit** (the integrator asked for it on 2026-10-10:
display text that becomes site-free could change TS2322 elaboration text,
which the diagnostics dump does not compare). A scratch example (not
committed) printed the full text of every diagnostic, with the flattened
message chain plus related information, for every case the diagnostics
dump judges, under the dump's own `case_guard`. That is 36,246
diagnostics, 1,010 of which embed an object type, in 385 cases. It ran on
`cd4afbcb` and on the branch. The first cut changed one message:
`aliasUsageInOrExpression` printed `{ x: typeof
/.src/aliasUsageInOrExpression_moduleA; } | null` where the base printed
`typeof moduleA` (native prints `typeof
import("aliasUsageInOrExpression_moduleA")`; both are wrong). That is a
union composing the literal's own display, with a module object's baked
file-path placeholder in place of its site render.

**jsTyping gate leg** (against `target/tsgo-pinned`): the first cut added
**15 lines tsgo does not report** (new_false = 15, lost_true = 0). Among
them are `tsbuildPublic.ts(783..789)` TS18048 `'cache' is possibly
'undefined'`, `checker.ts(8637)`/`(8643)` and `program.ts(784)`. None of
these moves a display. Rendering a callable member at the mint completes
its signatures' lazy returns in this port's order, and later answers in
jsTyping depend on that order. A hand reduction (`d1` in §4's cluster 4)
did not reproduce it, so the dependency needs the whole program. That is a
real resolution-order sensitivity in the checker, recorded here for the
lanes that own lazy returns. This cut does not change it.

**Landed rule.** A plain property member defers its site render only when
its type holds **no anonymous object type**, itself or as a
union/intersection constituent (`holds_anonymous_object`). Anonymous object
types are functions, methods, class, module and namespace values, which
covers both findings. **Nor may it hold an instantiable type** (a type
parameter, indexed access, conditional or substitution type). Rebased onto
batch-15 main `4bdd575d`, the anonymous-only rule added one jsTyping
new_false: `debug.ts(189,13)` TS2322 on `assertionCache[name] = { level,
assertion: Debug[name] }`. The deferred render of `Debug[name]` no longer
asked for its constraint at the mint, and the relation's verdict changed
with it. `804c649f` (anonymous-only) had already landed in batch 17. On
`9acf69d7` jsTyping no longer reports that line under either rule, because
r7-flow's `52706f75`/`02fb65bb` fixed the evaluation the render order
exposed. The instantiable exclusion lands as a fidelity follow-up on top of
it: native renders nothing at the mint, and the member whose render reads
constraints keeps today's order. Named types
(interfaces, classes, aliases with arguments), literals and primitives
defer. `main.ts`'s `item: Model000` is the case §14 measured.

**Measured** (against main `cd4afbcb`, frozen as `/tmp/box/base10`):

- Both unfiltered dumps are **byte-identical** (`compare`: 0 gained, 0 lost,
  0 missing). The first cut's three `typeParametersAvailableInNestedScope3`
  gains came from function-typed members and are not in this cut.
- CLI-message audit: **0 of 36,246** diagnostics differ.
- jsTyping gate leg: base 127 / new 127 unique lines, new_false = 0,
  lost_true = 0.
- Callgrind Ir, `--singleThreaded` (the integrator's deterministic guard),
  base → new:
  - generic-imports: 223,400,828 → 223,301,915 (−0.04%)
  - domain-model: 944,935,150 → 907,756,871 (**−3.9%**)
  - domain-model-large: 4,475,766,740 → 3,860,034,591 (**−13.8%**)
- Interleaved A/B (21 samples, noisy host): domain-model-large wall
  0.748 · CPU 0.901; domain-model 0.950 · 0.923.
## §16 generic-imports' CPU creep across batches 10–12: no instruction regression

**Question** (integrator, 2026-10-10): main at batch 12 (`cd4afbcb`)
read 1.035 wall / 1.075 child CPU against the batch-9 binary (`92fe8f05`)
at 41 samples, although each batch passed against its predecessor.

**Instruction counts** (callgrind, release binaries built from each batch
merge; deterministic across runs):

| binary | gi `--singleThreaded` Ir | gi default-mode Ir | dm single Ir | dml single Ir |
|---|---:|---:|---:|---:|
| batch 9 `92fe8f05` | 223,596,128 | 224,873,307 | 944,778,143 | 4,474,034,191 |
| batch 10 `0f7a1165` | 223,702,543 | — | — | — |
| batch 11 `86d9406e` | 223,703,391 | — | — | — |
| batch 12 `cd4afbcb` | 223,423,953 | 224,964,732 | 944,980,303 | 4,475,624,882 |

Every column moves by at most 0.08% (dml +0.04%, gi default +0.04%, gi
single −0.08%). Minor page faults (404–428 per run, `perf stat -r 15`) and
peak RSS (55.6–57.7 MB) are flat too.

**CPU on this orb.** Interleaved, 61 samples, all four binaries in one run.
At p10 the batch-9 → 12 ratios are 1.000 / 1.012 / 0.994 / 0.994, and at
the minimum 1.000 / 1.021 / 0.960 / 0.968. The median moves 13% between
binaries with identical Ir, because the orb's run-to-run noise is large
(medians 76–87 ms against p10s of 44–45 ms). The box-protocol harness at
41 samples gives batch 12 / batch 9 = 0.995 wall · 0.989 CPU. A second
`ab.py` run gives 0.876 · 0.879 the other way.

**Conclusion.** No commit in batches 10–12 adds work on generic-imports,
domain-model or domain-model-large. The 1.075 reading is measurement
spread: on a shared 4-vCPU host the median child CPU of identical work
drifts by more than the 3% gate. Two things would separate the series
from noise: callgrind Ir as the hot-path guard on the bench projects, and
p10 rather than median CPU when two binaries' Ir agree. Nothing to fix or
route.

## §17 Refused: keeping the eager render for instantiable literal members

The follow-up to `804c649f` that §15.1's first rebase called for:
`holds_anonymous_object` also answering true for a
`TypeFlags::INSTANTIABLE` member, so that such a member keeps the mint-time
site render. It was held on `box/r7-perf-held` (`42a83a49`). It was measured
on main `9acf69d7` and refused, because the jsTyping leg reported
**new_false = 1**: `checker.ts(51501,41)` TS2345 `Argument of type
'IndexSignatureDeclaration' is not assignable to parameter of type
'never'`. tsgo does not report that line, and main does not either. It was
one of §4's original false positives, gone since the deferral.

That is the other direction of the same sensitivity. On batch-15 main,
*deferring* `Debug[name]`'s render added a false TS2322 at
`debug.ts(189,13)`. On batch-17 main, after r7-flow's
`52706f75`/`02fb65bb`, *rendering* instantiable members eagerly adds a
false TS2345. A mint-time render resolves constraints and lazy types in an
order native never uses, because native renders nothing at the mint. So the
anonymous-only rule on main (`804c649f`) stays as landed, and the remaining
order dependence belongs to the lanes whose lazy evaluations answer
differently by order (lazy returns, indexed-access constraints). Its other
gates passed: CLI-message audit 0 of 36,308, and the corpus compare was
not reached.

## §36 Function, method and class literal members defer their site render (proposed)

This section is on `box/r7-perf-proposed-anon`, one commit on main
`cac2f4c8` in r7-contextual's `objects.rs`. It is proposed, not landed
from this lane. It is placed after §17 because it revisits §15.1's rule.

**Forcing measurement.** In jsTyping's single-threaded profile, 4.2% of
samples are in `type_to_string_at_worker`. 1.9% of that comes from
`check_object_literal_members` rendering function-typed members at the
literal's site through `signature_to_string_at`, for example the big
method-valued object literals in `checker.ts`. §15.1 kept every
anonymous object type (function, method, class, module, namespace) on
the eager site render for two reasons:

1. A module or namespace object's site-free print is its baked
   placeholder, the stripped file path. In the literal's own display it
   changed one CLI message (`aliasUsageInOrExpression`).
2. Deferring a callable member changed 15 jsTyping lines, through the
   order in which lazy returns completed.

**Re-measured on main `cac2f4c8`.** Reason 2 no longer holds. With
function, method and class values deferred, jsTyping's output is
identical to the base in both checker modes. Reason 1 is about value
objects only, so the rule now keeps module, namespace and enum value
objects eager (`holds_value_object`), along with instantiable types
(§17).

**Gates** against a freeze of `cac2f4c8`:

- Both unfiltered dumps are byte-identical once the `ms=`/`mib=`
  columns are cut. `compare` reports 0 gained, 0 lost and 0 missing.
- jsTyping leg against tsgo-pinned: default mode 111 against 111 lines,
  `--singleThreaded` 109 against 109, both identical, so new_false 0
  and lost_true 0. CLI output is `cmp`-identical on dm, dml and gi.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,609 → 10,609
  exact, 0 lost, 0 missing.
- Callgrind Ir (measured on the §31 chain with an environment switch):
  generic-imports +0.01%, domain-model −0.001%.
- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,848 passed, 0 failed.

**Speed.** On top of the §31/§33 chain, jsTyping `--singleThreaded`
user CPU (3 interleaved runs each, environment switch in one build)
went from 7.92 / 9.01 / 9.47 s to 7.31 / 8.20 / 8.56 s, a median of
**−9%**. On main alone (37–40 s, dominated by §19's re-check loops) the
difference is within the run-to-run spread: 36.8 / 37.6 / 39.2 s
against 37.2 / 37.7 / 40.1 s. So the change is worth having once the
chain lands, not before.

**How we would know it was wrong.** A CLI message that composes a
function-typed member's display from a site-free print where native
qualifies it differently. The exact oracle compares full message text
on its exact cases, so a change there would show as a loss. Or an
order dependence that comes back on a later main, which the jsTyping
leg would show as moved lines.

## §18 The in-memory file system's realpath keeps the real entry's spelling

**Routed by the integrator** from r7-printer on 2026-10-11: under
`useCaseSensitiveFileNames: false`, a module reached through a symlink got
a lowercased file name (`/.src/folder/…` for `Folder`). r7-printer
compensated in `module_specifiers.rs` with native's case-folding
`HasPrefix`. The corpus case is
`compiler/symbolLinkDeclarationEmitModuleNamesImportRef`.

**Root cause.** It is not the loader.
`tsr_vfs::InMemoryFileSystem::realpath`, the test host's file system, which
the conformance harness uses, answered the *canonical key* of the entry it
resolved through a link. That key is lowercased on a case-insensitive host.
Native vfstest's `MapFS.Realpath` (`internal/vfs/vfstest/vfstest.go:470`)
answers `file.Sys.(*sys).realpath`, the spelling the entry was created with.
It case-folds only the lookup key (`getCanonicalPath`). The resolver's
`realPath` (`resolutionState.realPath`) and the loader stored that answer
as the module's file name.

**Change** (`crates/tsr-vfs/src/lib.rs`):

- `InMemoryFileSystem` records `spellings`, the canonical path → creation
  spelling of every file and of each ancestor directory, first creator
  wins, as vfstest's `mkdirAll` does.
- `realpath` through a link answers the real entry's spelling. A path
  reached without a link keeps answering the caller's normalized spelling,
  as before. The resolver's `getOriginalAndResolvedFileName` compares the
  two case-insensitively and prefers the original.
- The CLI's `OsFileSystem` is unaffected.

This is not a cache of computed answers. The table is fixture data fixed
at construction. A new test (`realpath_through_a_link_keeps_the_real_spelling`)
pins the case. r7-printer's compensating `HasPrefix` fold can now be
re-measured for removal; it is native's own fold
(`tspath.HasPrefix(..., !useCaseSensitiveFileNames)`), so keeping it is
also faithful.

## §19 Ratios after batch 19, and jsTyping's batch-6 CPU regression

**Dist ratios** (TSR/tsgo, `whole_project_perf.py`, 21 samples, one session,
on `41f3c70b` plus this branch; release in parentheses):

| project | wall | CPU |
|---|---:|---:|
| domain-model | **0.605** (0.577) | 0.408 (0.411) |
| domain-model-large | **0.584** (0.620) | 0.449 (0.482) |
| generic-imports | **0.599** (0.631) | 0.298 (0.320) |

domain-model-large moved from 0.716 to 0.584, mostly from §15's deferred
literal text. The 0.50 target is not met.

**jsTyping regressed 46% in CPU at batch 6.** Dist jsTyping read 7.4× tsgo
(14.3 s against 1.93 s), against 4.16× at batch 5. Default-mode user CPU
on jsTyping, with the gate leg's command, at each batch merge:

| binary | user CPU | wall |
|---|---:|---:|
| batch 5 `20501307` | 28.2 s | 10.4 s |
| + r7-calls (`56f5a49b`, batch 6's first merge) | **41.1 s** | 18.9 s |
| batch 6 `5f2713fe` / 7 `2c34a50e` / 8 `0aa00136` | 39.9 / 39.9 / 40.5 s | ~18.9 s |
| batch 9 `92fe8f05` (mimalloc) | 33.0 s | 16.8 s |

The merge holds one commit, `23550f9d` ("the overload walk checks a nested
call argument under its first candidate"), the fix for §4's cluster 5. It
removed 7 false jsTyping lines (259 → 252). Single-threaded perf samples go
from 8 K to 12 K, spread evenly with no new hot symbol. That points to a
nested call argument being re-checked per candidate or per pass where
native resolves it once (`getResolvedSignature`'s
`links.resolvedSignature`). It was routed to r7-calls on 2026-10-11.
jsTyping is not in the CPU gate (domain-model and generic-imports only),
which is why the regression went unseen for 13 batches.

**Oracle tooling.** With the gate's 60 s TSR deadline,
`compiler/recursiveConditionalCrash3.ts` grows to 3.4 GB in 25 s on base
and candidate alike. Within the orb's 6 GiB workload cgroup, one such
worker beside the others is an OOM kill of the whole service
(`memory.oom.group`). This lane's exact-candidate run therefore used
`--deadline 20 --workers 3` for both base and candidate. The outcome set is
unchanged (TSR_TIMEOUT 2, TSR_FAILED 1, exact 9,582).

## §20 domain-model-large after the literal print: what checker 0 still pays

Pool probe (temporary timer, not committed; two runs, release, main
`41f3c70b` plus this branch). On domain-model-large, checker 0 finishes in
292–381 ms and checkers 1–3 in 195–308 ms: `src/main.ts` still makes
checker 0 the critical path, at about 1.45× the others, where §14 measured
about 2×. domain-model's four checkers are within 41–65 ms of each other.

Per `main.ts` block (perf with dwarf call graphs, inclusive sample delta
K = 200 − K = 25 blocks, single-threaded): the object-literal print is gone.
What remains:

- flow narrowing: `get_type_at_flow_condition` 171,
  `get_type_at_flow_branch_label` 155, `narrow_type` 64. These narrow
  `createdNNN.ok` / `createdNNN.value` and walk the long `run()` body's
  flow graph.
- `get_effects_signature` 62 and `get_type_at_flow_call` 59: each flow
  call node a walk crosses asks for the call's effects signature again.
  Native reads the call's `links.resolvedSignature` (`getEffectsSignature`
  → `getResolvedSignature`).
- `report_signature_applicability` 93, mostly the arguments' first check.
- hashbrown probing (`_mm_movemask_epi8` 44 self) from side-table reads.

All of these are r7-flow's (`flow.rs`) and r7-calls' (`calls.rs`,
`call_reports.rs`) files. They are routed through the integrator.

## §32 Main at batch 51 (`bacf0918`): ratios, and jsTyping's delta by owner

This section is placed before §21 so that it does not touch the lines
that the proposed branches `box/r7-perf-proposed-rcc` (an append) and
`box/r7-perf-proposed` (a block before §25) change.

**Ratios** (TSR/tsgo-pinned, `whole_project_perf.py`, one session,
21 samples, jsTyping 5; median wall · median child CPU):

| project | dist | release |
|---|---|---|
| domain-model | **0.489** · 0.393 | 0.557 · 0.436 |
| domain-model-large | **0.527** · 0.417 | 0.573 · 0.467 |
| generic-imports | **0.565** · 0.287 | 0.663 · 0.330 |
| jsTyping | 7.42 · 5.85 | 8.31 · 6.40 |

A later quiet session read main's dist ratios at dm 0.524, dml 0.510 and
gi 0.604, in the session that measured the proposed chain on
`box/r7-perf-proposed`. So dm straddles 0.50 between sessions on main alone.
`diagnostics_match` is true on the three bench projects. jsTyping still
carries §19's batch-6 regression. The fixes that remove it are on the
proposed chain (`box/r7-perf-proposed`), which is not landed.

**jsTyping's delta** (TSR 122 unique lines against tsgo's 86): 38 extra
and 2 missing. These are §21's clusters, re-counted line by line against
the source:

| cluster | lines | code | sites | owner |
|---|---:|---|---|---|
| `Pick<…>` against a string-index target | 8 | TS2345 | `utilities.ts` 9231–9280 | r7-reports (`relater.rs`, `isObjectTypeWithInferableIndex`) |
| destructuring-assignment narrowing | 5 | TS2322, TS2365 | `semver.ts` 63/73, `program.ts` 1220 ×2, `esDecorators.ts` 1309 | r7-flow (`flow.rs`) |
| generic-signature assignability | 5 | TS2322, TS2345 | `nodeFactory.ts` 543/579/7158, `moduleNameResolver.ts` 1087, `tsbuildPublic.ts` 1669 | r7-reports / inference (`relater.rs`, `inference.rs`) |
| tuple context through overloaded `append`/`concatenate` | 4 | TS2322 | `builder.ts` 1304/1432/1494, `declarations.ts` 544 | r7-contextual (`contextual.rs`) |
| aliased-condition inlining | 3 | TS2345, TS18048 | `checker.ts` 38800/45144/48616 | r7-flow |
| equality and `asserts` narrowing | 3 | TS18048, TS2345 | `expressionToTypeNode.ts` 596 ×2, `visitorPublic.ts` 362 | r7-flow |
| closure narrowing of an outer `let` | 3 | TS18048 | `program.ts` 2508/2520/2570 | r7-flow |
| `BindableStatic*` overloads | 3 | TS2769, TS2322, TS2345 | `utilities.ts` 4187/4258, `binder.ts` 3594 | r7-calls (`calls.rs`) |
| `??=` RHS return-type inference | 2 | TS7053 | `commandLineParser.ts` 3295/3325 | r7-contextual |
| `Readonly<array intersection>` | 2 | TS2345 | `resolutionCache.ts` 460/463 | r7-declared (`mapped.rs`) |
| missing implicit-any reports | 2 missing | TS7006, TS7031 | `sys.ts` 1696 | r7-reports (`implicit_any.rs`) |

Compared with §21, equality/`asserts` narrowing has one TS18048 fewer.
Every other cluster has the same lines.

Three fresh reductions still reproduce on `bacf0918`. Each prints the
diagnostic on TSR and nothing on tsgo-pinned (`--strict --target
es2020`):

```ts
declare function parse(s: string): { major: number };
export function d1(major: string | number): number {
    if (typeof major === "string") ({ major } = parse(major));
    return major;   // TSR: TS2322 'string | number' is not assignable to 'number'
}
interface CO { strict?: boolean; noImplicitAny?: boolean; [option: string]: unknown }
declare function gsv(o: CO, k: string): boolean;
export function p1(o: Pick<CO, "strict" | "noImplicitAny">) {
    return gsv(o, "noImplicitAny");   // TSR: TS2345 'Pick<CO, ...>' is not assignable to 'CO'
}
declare function assign<T extends object>(t: T, ...a: T[]): T;
export function q1(options: Record<string, number>, name: string, value: number) {
    let result: Record<string, number> | undefined;
    (result ??= assign({}, options))[name] = value;   // TSR: TS7053 on '{}'
    return result;
}
```

Hand reductions of the aliased-condition, closure and equality clusters
were silent on both compilers, so those triggers need more of the
program.

## §34 Refused: lib.dom.d.ts parsed in place on the walk thread

This section is placed before §21, after §32, for the same reason as §32.

**Question.** Since §25, `lib.dom.d.ts` is parsed privately on a worker
and then copied into the shared tables at its visit. That copy is about
0.31 of the parse (r5-loader §6.2). On domain-model, the loader is 33 ms
of a 42–47 ms `Program time`. Would the walk thread do better parsing
`lib.dom.d.ts` in place, with no copy?

**Variants** (scratch builds of `FileLoader::prepare_root_parses` on main
`bacf0918`, not committed; dist; interleaved fresh processes, 25 pairs
each; ratio new/§25):

| variant | domain-model wall | domain-model-large wall | `Program time` |
|---|---:|---:|---|
| not prepared; parsed in place at its visit (last) | 1.015 | 1.027 | dm 42 → 44 ms, dml 70 → 81 ms |
| parsed in place **before** the walk, while workers prepare the roots | 0.996 | 1.019 | dm 42 → 41 ms, dml 77 → 86 ms |

Neither wins. In the first variant the walk thread publishes every root
before it starts a 15 ms serial parse that §25's worker had already
finished. In the second the walk thread spends that 15 ms before
publishing the first root, and the roots' copies then queue behind it.
dml has 202 roots, so it loses most. The second variant also changes
the node-id order (lib.dom before the roots), which would have needed
its own output audit. §25's arrangement stays. Without ADR-0056's
zero-copy publication, there is no faster arrangement of these jobs on
domain-model or domain-model-large.

## §35 A call whose callee declares no const type parameter is not resolved again for a literal's const context

This section is placed before §21, after §34, for the same reason as §32.
The change is in this lane's `perf_links.rs` and lands on `box/r7-perf`.

**Forcing measurement.** A single-threaded perf profile of jsTyping, on
the chain of §31 and §33 (profiling build, frame pointers), puts 4.7% of
all samples under `call_resolves_const_type_parameter`. That function is
reached from `literal_in_const_type_variable_context` through
`check_object_literal_members`, inside `choose_construct_overload`'s
argument walk. The pattern is `getScriptTargetFeatures`'
`new Map(Object.entries({ … }))`. For every member of the inner literal,
the port asked whether `Object.entries(…)`'s resolved signature
declares a const type parameter. Inside the outer overload walk an
inference context is active, so the per-call answer
(`perf_links.call_const_type_parameters`) is not published, and the
call was resolved again for each member.

**Native.** `isConstContext`'s type-variable arm
(`checker.go:13618`) reads the literal's contextual type, which comes
from the call's resolved signature. That signature is one of the
callee's call signatures, possibly instantiated, and instantiation keeps
each type parameter's `const` modifier. When no candidate declares one,
the answer cannot be true.

**Change.** `call_resolves_const_type_parameter` reads the callee's
call signatures (`signatures_of_type_kind`, which applies the apparent
type as resolution does). When there is at least one and none declares
a const type parameter, it answers `false` without resolving the call.
An empty or undecided signature list keeps the old road. This is not a
new cache: the existing per-call answer keeps its publication rules.

**Measured** (base: main `cac2f4c8`, batch 56):

- Both unfiltered dumps are byte-identical to a freeze of `cac2f4c8`
  once the `ms=`/`mib=` columns are cut. `compare` reports 0 gained,
  0 lost and 0 missing.
- jsTyping leg against tsgo-pinned (86 lines). Default mode: base 111,
  candidate 111, identical. `--singleThreaded`: 109 and 109, identical.
  new_false 0, lost_true 0. CLI output is `cmp`-identical on dm, dml
  and gi.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,609 → 10,609
  exact, 0 lost, 0 missing.
- Callgrind Ir, `--singleThreaded`: generic-imports 191,608,893 →
  191,610,958; domain-model 858,068,852 → 858,138,769 (+0.008%). The
  bench projects have no such call.
- jsTyping `--singleThreaded`, interleaved, 3 runs each, on a box shared
  with a gate run: user CPU 33.4–33.8 s → 30.4–31.2 s (**−7%**). On the
  §31 chain, which already removes §24's re-check loops, single-threaded
  wall went 8.97 → 7.80 s (−13%).
- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,848 passed, 0 failed.

**Side finding: jsTyping's output depends on the checker mode.** On main,
`--singleThreaded` reports 120 unique lines and the default
4-checker mode reports 122. The two extra lines are the `BindableStatic*`
cluster of §32: `utilities.ts(4187,92)` TS2769 and
`utilities.ts(4258,13)` TS2322. tsgo prints neither. The two modes
differ only in what the checker has resolved before it reaches
`utilities.ts`. So the cluster's answer depends on evaluation order:
it is an order bug in the overload or alias road, not a missing rule
(r7-calls). This change leaves both modes' outputs
unchanged.

**How we would know it was wrong.** A call whose resolved signature
declares a const type parameter that none of `signatures_of_type_kind`'s
answers declares, for example a signature synthesised for a union callee.
A literal in that call would lose its const context and widen, which a
types row would show.

## §37 Routed: a generic alias's mapped branch is resolved at its declaration

This section is placed before §21, after §35, for the same reason as §32.
It records a measurement and a routing. Nothing is built here.

**Forcing measurement.** On domain-model-large with the chain (§31),
`check_type_alias_circularity` costs 161 M Ir (4.6%) over 200
`FrozenModelNNN = DeepReadonly<ModelNNN>` aliases, about 800 K Ir per
alias. A generated file isolates it (`/tmp/box/micro`, not committed).
It has 200 pairs of interfaces shaped like the bench's `ModelNNN` and
`ModelNNNLine`, and the bench's `DeepReadonly`. Check time is the
minimum of 5 runs, `--singleThreaded --skipLibCheck`, release:

| file | TSR check | tsgo check |
|---|---:|---:|
| interfaces only | 7 ms | 4 ms |
| + 200 `type FNNN = DeepReadonly<MNNN>` | 48 ms | 6 ms |
| + 200 `const fNNN: FNNN = mNNN` | 56 ms | 34 ms |

Declaring the aliases costs TSR 41 ms and tsgo 2 ms. Using each alias
once in an assignment costs tsgo 28 ms, because that is where its lazy
members resolve, and TSR 8 ms more on top of its 41 ms. In total TSR
pays 49 ms against tsgo's 30 ms.

**Where TSR's cost is.** A callgrind of the declaration-only file
(profiling build) puts 194 M of
`evaluate_conditional_node`'s 206 M Ir under `get_type_from_type_node`
of the branches. The true branch
`{ readonly [K in keyof T]: DeepReadonly<T[K]> }` is resolved member by
member at the declaration, and each member's `DeepReadonly<T[K]>` again,
down to the leaves. `infer_conditional_parameters` (the
`T extends (infer U)[]` test) is the next largest, at 42 M Ir over
200 aliases. Native's `getConditionalType` picks the branch
and `instantiateType`s it, and a mapped type's instantiation is a new
deferred object type whose members `resolveStructuredTypeMembers`
computes only when a relation or property access reads them.

**Routing.** Lazy members for an instantiated mapped type belong to
r7-declared (`declared.rs`, `mapped.rs`). This lane does not build them.
The table bounds the gain on this pattern: the 41 ms at declaration
would become use-time work only for the members a relation reads, and
tsgo needs 30 ms for declaration and use together.

## §38 generic-imports outside lib.dom.d.ts, and the BindableStatic order pair

This section is placed before §21, after §37, for the same reason as §32.

### §38.1 generic-imports: where the non-dom time goes

The integrator asked on 2026-10-12 for generic-imports' costs other than
lib.dom.d.ts. Measured at `box/r7-perf` `36eca74e`: callgrind
`--singleThreaded` with a profiling build, and median wall of 31
interleaved fresh processes (release; tsgo-pinned beside it).

| run | TSR Ir | TSR wall | tsgo wall |
|---|---:|---:|---:|
| `--version` (process start and exit) | — | 2.9 ms | 7.9 ms |
| empty file, `--lib es2022` (no dom libs) | 37.7 M | 15.0 ms | 25.9 ms |
| empty file, default libs for es2022 | 192.0 M | 44.6 ms | 76.3 ms |
| generic-imports | 197.0 M | 43.3 ms | 73.5 ms |

- **The project's own three files cost nothing measurable.** generic-imports
  runs the same as an empty file with the same libs. Its check is 4.6 M Ir
  (2.3%).
- **The dom libs are 80% of the Ir.** That is 153 M of 192 M, about 30 ms
  of TSR's wall against about 50 ms of tsgo's: 0.60 on their own.
- **Everything else** is process start (2.9 ms against tsgo's 7.9 ms)
  plus the serial parse and bind of the other ~60 bundled libs: 15.0 ms
  against 25.9 ms, 0.58.

**Refused: preparing the non-dom libs on workers.** §25's lib
preparation runs only above its root gate (≥128 KiB, ≥8 roots), so
generic-imports parses every lib in place on the walk thread. A scratch
variant (not committed) prepared the predicted libs except the largest
on the dependency pool when the roots were under the gate, and left
lib.dom.d.ts in place at its visit. Node-id order and output were
unchanged. Interleaved over 41 pairs (dist) it measured **wall 1.045 and
CPU 1.176** against main, with `Program time` going from 31.0 to 33.0 ms.
The publication copy of the prepared libs (0.31–0.45 of their parse,
r5-loader §6.2) and the pool's start cost more than the overlap saves.
That is the same arithmetic as §34. Without ADR-0056's zero-copy
publication, generic-imports has no remaining front-end lever in this
lane's files, and the ADR stays unfunded (integrator, 2026-10-12).

### §38.2 jsTyping's `BindableStatic*` pair is an alias-resolution order bug

§35 found that `compiler/utilities.ts(4187,92)` TS2769 and
`(4258,13)` TS2322 appear with the default 4 checkers but not with
`--singleThreaded`. With a scratch hook in `checker_pool.rs` (not
committed) that checks a chosen list of file indices on one checker:

- `utilities.ts` alone reports both lines.
- `--checkers` 2, 3, 4, 6 and 8 all report them; `--checkers 1` does not.
- Checking either `types.ts` or `utilitiesPublic.ts` first removes
  both. No other earlier file does.
- In a scratch copy of the sources, one line at the top of `utilities.ts`,
  `type __a = BindableStaticNameExpression;`, removes both. The same line
  naming `BindableStaticAccessExpression`,
  `BindableStaticElementAccessExpression`,
  `LiteralLikeElementAccessExpression` or `EntityNameExpression` does
  not.

The reduction has no checker-mode dependence. TSR's answer depends only
on which alias of a cycle is resolved first:

```ts
interface Expression { _expressionBrand: any }
interface Identifier extends Expression { _identifierBrand: any }
interface ElementAccessExpression extends Expression { readonly expression: Expression }
type Element = ElementAccessExpression & { readonly expression: Name };
type Name = Identifier | Element;
export declare const n: Exclude<Name, Identifier>;
export const k: number = n;
// TSR:  TS2322 Type 'Name' is not assignable to type 'number'.
// tsgo: TS2322 Type 'Element' is not assignable to type 'number'.
// TSR with the two aliases swapped (Name first): 'Element', as tsgo.
```

When `Element` is resolved first, its type literal's `expression: Name`
is resolved eagerly while `Element` is still in progress. The `Name`
it publishes then fails `Exclude`'s distribution, so it is printed and
related unfiltered. Native defers a type literal's member types, so the
cycle never closes during resolution. This is r7-declared's alias road,
the same eager-member theme as §37 and §30. It is routed through the
integrator; nothing is built here.

## §39 Ratios at main `2d1b3a4c` (batch 64, the chain landed): not yet verified ≤0.50

This section is placed before §21, after §38, for the same reason as §32.

Batch 64 landed `box/r7-perf` `36eca74e`, which carries the §31/§33
chain, §30's re-entry guard and §36's deferred member print. TSR/tsgo-pinned,
`whole_project_perf.py`, median wall · median child CPU. Release and dist
were built from `2d1b3a4c`. `diagnostics_match` is true on dm, dml and gi
in every run. jsTyping is still not equivalent work.

| project | session 1, dist (21) | session 2, dist (31) | session 1, release (21) | session 2, release (31) |
|---|---|---|---|---|
| domain-model | 0.518 · 0.393 | 0.516 · 0.386 | 0.531 · 0.419 | 0.515 · 0.387 |
| domain-model-large | 0.524 · 0.411 | 0.506 · 0.393 | 0.548 · 0.436 | 0.549 · 0.429 |
| generic-imports | 0.575 · 0.298 | 0.602 · 0.304 | 0.601 · 0.305 | 0.588 · 0.303 |
| jsTyping (5) | 1.54 · 1.18 | 1.75 · 1.32 | 1.63 · 1.30 | 1.79 · 1.44 |

**A third, interleaved session** put tsgo, the pre-landing
`box/r7-perf` dist binary (`359a20af`) and main's dist binary side by
side over 31 rounds:

| project | pre-landing | main `2d1b3a4c` |
|---|---:|---:|
| domain-model wall · CPU | 0.508 · 0.372 | **0.496** · 0.361 |
| domain-model-large wall · CPU | 0.492 · 0.390 | 0.520 · 0.419 |

The same pre-landing binary measured dm 0.489 and dml 0.483 in its own
earlier session. Callgrind (`--singleThreaded`, release) puts main at
dm +0.44%, dml +0.51% and gi +0.02% Ir over the pre-landing binary,
which is batch 64's other merges. That is far smaller than the 0.02–0.03
swings between sessions. **Reading:** dm and dml now sit at 0.49–0.52 in
dist, depending on the session. The target is reached in some sessions
and missed in others, so it is not verified with margin. generic-imports
stays at 0.55–0.60 (§38.1). The levers left on dm/dml are other lanes'
eager evaluations (§37, §38.2).

## §21 Main at batch 25: ratios, jsTyping's remaining delta, the effects signature

**Ratios** (`6fe05d64`, TSR/tsgo wall · CPU, 21 samples, jsTyping 5):

| project | dist | release |
|---|---|---|
| domain-model | **0.537** · 0.380 | 0.630 · 0.442 |
| domain-model-large | **0.566** · 0.460 | 0.619 · 0.485 |
| generic-imports | **0.640** · 0.326 | 0.658 · 0.338 |
| jsTyping | **7.17** · 5.68 | 7.53 · 5.88 |

jsTyping still carries §19's batch-6 regression.

**jsTyping delta** (123 TSR lines against tsgo's 86): 39 extra (TS2345 15,
TS2322 13, TS18048 7, TS7053 2, TS2769 1, TS2365 1) and 2 missing
(TS7006, TS7031 at `sys.ts(1696)`). Clusters, all routed on 2026-10-11:

- **r7-flow, aliased-condition inlining** (3 lines). Reduction: `const node
  = d.attributes; if (node) d.attributes.token` gives a false TS18048.
- **r7-flow, destructuring-assignment narrowing** (5 lines). Reduction:
  `if (typeof major === "string") ({ major } = parse(major)); return major`
  gives a false TS2322.
- **r7-reports, `Pick<…>` against a string-index target** (8 lines;
  `isObjectTypeWithInferableIndex` for a mapped type literal). It has a
  reduction.
- **r7-contextual, `??=` RHS return-type inference** (2 lines, plus a
  duplicate TS7053 report at one span). It has a reduction.
- Not reduced:
  - r7-flow: closure narrowing of an outer `let` (3 lines), plus equality
    and `asserts`-overload narrowing (4 lines);
  - r7-reports/inference: generic-signature assignability (5 lines);
  - r7-contextual: tuple context through overloaded `append` (4 lines; two
    reductions passed);
  - r7-declared: `Readonly<array intersection>` (2 lines);
  - BindableStatic* overloads (3 lines);
  - r7-reports: missing implicit-any reports (2 lines).

**`getEffectsSignature`'s nil-dotted-name negative** (a scratch measurement
in a separate worktree; the code is in r7-flow's `flow.rs` and was not
committed here). `get_effects_signature` runs about 300 K times on dml, 2.3%
of Ir. 102,709 of those calls re-run `get_type_of_dotted_name` for an
expression-statement call whose callee has no explicit type, for example
`serviceNNN.move(...)` in `main.ts`. Each answers nil again. Native
publishes `unknownSignature` in `signatureLinks.effectsSignature` for
exactly that answer (`flow.go:2060`, `:2079-2082`). This port publishes
negatives only for written non-predicate returns. Publishing the nil
answer, under `effects_completion_context_is_original`, measured
**dml −1.7% and dm −0.3% Ir**. CLI output was identical on dm, dml and gi;
the jsTyping leg gave new_false 0 / lost_true 0. Separately, publishing
negatives for unannotated signatures whose lazy return is no longer
pending moved Ir by 0.00%. That is not where the walk's cost is, so it is
not proposed.

## §22 Three faithful hot-path fixes in other lanes' files (branch `box/r7-perf-proposed`)

Each is one commit on `box/r7-perf-proposed`, gated on main `127660ee`.
Gates for every commit: both unfiltered dumps byte-identical, exact
oracle 9,639 → 9,639 with 0 lost (`--deadline 20 --workers 3` for base and
candidate, §19), CLI output identical on dm, dml and gi, and jsTyping leg
new_false 0 / lost_true 0. They are proposed to the integrator for routing
or a grant; they are not landable from this lane's branch.

1. **`withJSDoc`'s scanner flag** (`tsr-parser/src/jsdoc.rs`
   `leading_jsdoc_marker`, r7-parser). In a non-JavaScript file the marker
   is `None` unless the token carries `PRECEDING_JSDOC_WITH_SEE_OR_LINK`.
   That is `jsdocScannerInfoHasSeeOrLink`, which native reads
   (`jsdoc.go:64`; ADR-0053). The scanner sets it with `mentions_tag`, the
   same predicate and terminator set as `jsdoc_has_see_or_link`. A debug
   build compared the flag with the old re-scan's verdict on every marker
   across jsTyping and found no disagreement. `parse_jsdoc_at` previously
   re-scanned the trivia (`jsdoc_ranges_in`) to recompute the flag. Ir:
   **gi −13.3%**, dm −3.3%, dml −0.8%.
2. **`check_grammar_modifiers`' memo** (`grammar.rs`, r7-grammar). A node
   without modifiers no longer enters `modifier_chain_checked`, which
   native does not have at all. The JS reparsed-host kinds keep the memo.
   That was 234 K inserts plus rehashing on dml. Ir: dm −0.6%, dml −0.9%.
3. **`getEffectsSignature`'s nil-dotted-name negative** (`flow.rs`,
   r7-flow; §21). Ir: dm −0.3%, dml −1.7%.

Together, by Ir: gi about −13%, dm about −4%, dml about −3.4%.

## §23 domain-model-large by file and by operation count: two more fixes

**Where checker 0's time goes.** A scratch build timed each
`check_source_file` (release, 4 checkers, main `127660ee`). Checker 0 owns
`main.ts` (index 264, so `264 % 4 == 0`). It finished at 310 ms; the
other three finished at about 220 ms. `main.ts` alone took 126 ms of it,
because checker 0 resolves lazily the 150 models its own files never
touched. The file association is native's `i % checkerCount`
(`checkerpool.go:98`), so the imbalance is native's too and is not a
lever.

**TSR against tsgo, single-threaded, by subtraction.** Dropping `main.ts`
from the project (`include: [core.ts, model*.ts]`) and taking the minimum
check time of 5 runs:

| | all files | without `main.ts` | `main.ts` alone |
|---|---:|---:|---:|
| TSR (dist) | 867 ms | 809 ms | 58 ms |
| tsgo | 624 ms | 513 ms | 111 ms |

TSR is already about 2× faster than tsgo on `main.ts`'s long flow walks,
and 1.58× slower on the 201 model files. Parallel checking hides this:
TSR scales 3× on 4 checkers and tsgo 1.2×. The per-file checker cost of
the model files is therefore the larger lever.

**Native operation counts.** A copy of the pinned tree
(`/tmp/box/tsgo-count`, not committed) counts calls with a
`checker.PrintDebugCounts()` at exit. The comparison was against TSR's
callgrind call counts on the model files:

| native | count | TSR | count |
|---|---:|---:|---:|
| `NameResolver.Resolve` | 36,769 | `BindResult::resolve_name` | 142,801 |
| `instantiateSignature` | 7,660 | `instantiate_signature` | 26,342 |
| `createTypeReference` | 18,171 | `create_type_reference` | 41,136 |
| `getPropertyOfType` | 159,560 | `get_property_of_type_ex` | 314,032 |
| `recursiveTypeRelatedTo` | 47,000 | `recursive_type_related_to` | 21,388 |
| `getTypeOfSymbol` | 231,997 | `get_type_of_symbol` | 144,653 |

The four rows where TSR does more work are the candidates. The other
two show that a TSR excess is not across the board.

**Fix 4: `check_used_before_its_declaration` resolves only what can
report** (`check.rs`, the dispatch lane's file). The rule runs for every
identifier node, and 51,415 of the 70,876 memo misses on the model files
came from it. They were property-signature, parameter, variable,
type-alias and import-specifier names, and type-reference names. Native
runs `checkResolvedBlockScopedVariable` inside `resolveName`, for
value-meaning lookups only. Both of the port's arms report only an
identifier that `is_value_reference` admits or that
`is_in_extends_clause` finds, so any other identifier now returns before
the resolution. A property access's name keeps the old path, because its
`qualified_member_of_namespace` fallback calls `resolve_alias`, which has
memo side effects. The memo misses fell from 70,876 to 33,470, with no
other rule picking up the skipped ones. Ir: dml −1.3%, dm −1.2%,
gi −0.1%.

**Fix 5: `binding_type_alias_body` stops at a non-alias reference**
(`destructure.rs`, r7-declared). `narrow_type` calls it for every
condition step of every flow walk: 201,600 calls on `main.ts` alone,
which is 8.1% of that file's check Ir. Each step with an
`Array<string>` declared type cloned the argument vector and built a
`(symbol, Vec)` key, only for `evaluate_alias_body` to answer `None`.
Both of that function's arms test `TYPE_ALIAS` first. The walk now
breaks before the clone. The one effect skipped is the
`(symbol, args) → error` insert into `alias_body_evaluations`, and that
table's only non-test reader (`index_signatures.rs`, the
`type_reference_targets` arm) requires `TYPE_ALIAS` too. Ir: dml −0.7%,
dm −0.1%.

Fixes 4 and 5 were measured on main `127660ee` plus §22's three
commits. The branch was then rebased onto main `59ceea97` (batch 34).
§22's item 1 had landed independently there as r7-parser's `a83e1dfc5`
(the same `PRECEDING_JSDOC_WITH_SEE_OR_LINK` test), so it was dropped
from the branch.

**Wall** (interleaved, 30 runs, dml, 4 checkers): main 400.7 ms,
`box/r7-perf-proposed` before these two fixes 395.7 ms, after them
387.1 ms, tsgo 712.7 ms. The ratio went from 0.562 to **0.543**.

**Not yet attacked.** `instantiate_signature` runs 3.4× native's count,
`create_type_reference` 2.3× and `get_property_of_type_ex` 2×.
`FrozenModelNNN = DeepReadonly<ModelNNN>` costs 160 µs per alias in
`check_type_alias_circularity` (32 ms single-threaded over 200 aliases).
Native defers the mapped type's members until something reads them; the
port's `evaluate_conditional_node` resolves them on construction.

**How we would know these were wrong.** For fix 4: an EXACT loss, or a
corpus case that reports TS2448/TS2449/TS2450 on a declaration name, a
type reference, or something else that is not a value reference. For
fix 5: any dump row that differs, since the walk's answer is unchanged
by construction.

## §24 jsTyping: two re-check loops that native does not have

**Where the time goes.** jsTyping's single-threaded check took 16.4 s
on TSR against tsgo's 1.6 s (main `59ceea97`). Per-file times from a
scratch build:

| file | check |
|---|---:|
| `compiler/utilities.ts` | 13.4 s |
| `compiler/transformers/jsx.ts` | 8.1 s |
| `compiler/checker.ts` | 3.6 s |
| every other file | under 0.5 s each |

With 4 checkers, `utilities.ts` alone is the critical path. Native call
counts (§23's counting copy) against TSR's scratch counters on the whole
project:

| native | count | TSR | count |
|---|---:|---:|---:|
| `getPropertyOfType` | 497 K | `get_property_of_type_ex` | 40.0 M |
| `getContextualType` | 122 K | `get_contextual_type` | 3.86 M |
| `isRelatedToEx` | 689 K | `is_related_to_with_flags` | 15.7 M |
| `instantiateType` | 238 K | `instantiate_type` | 3.55 M |
| `getTypeOfSymbol` | 1.34 M | `get_type_of_symbol` | 12.0 M |

**Fix A: `annotation_member_context` does not re-resolve a call that is
resolving** (`symbols.rs`). getScriptTargetFeatures nests generic calls
three deep: `new Map(Object.entries({ Array: new Map(Object.entries({…}))
… }))`. tsgo's `checkObjectLiteral` runs at most 4 times on any literal in
`utilities.ts`. TSR checked each inner literal 1,776 to 3,441 times, and
the count grew by 333 per member of the literal.

The loop: the §56 road reads each member's contextual type through the
literal's holder. At a `CallExpression` holder with no
`resolved_call_signatures` entry, it called `resolve_call_signature`.
That re-checked every argument, so every nested literal's members ran
the road again. The `narrow_value_stack` guard stops only the
re-entrant cycle, not the repetition.

Native does not resolve a call whose `resolvedSignature` is the
`resolvingSignature` sentinel (`getContextualTypeForArgumentAtIndex`,
`checker.go:29783`). The argument already has the contextual type that
the running resolution pushed. The port's equivalent of the sentinel is
`perf_links.main_resolving_calls`. While it holds the holder, the road
now answers `None`. The caller then takes
`check_expression_for_mutable_location`, the general contextual road,
which is native's only one.

`utilities.ts`: 12.7 s → **0.92 s**. jsTyping's error lines were
identical (124 against 124).

**Fix B: `discriminateContextualTypeByObjectMembers` is memoised as
native's is** (`contextual.rs` call sites; the memo is in
`perf_links.rs`). The port's own rustdoc said it recomputed per member
read. jsx.ts's `entities` literal (about 250 members, inside
`new Map(Object.entries({…}))`) paid the discrimination once per member
per pass. The memo key is native's `(nodeId, typeId)`. The rules are
those in the convention: decided answers only, `publishable_since`, and
no alias, mapped-template or identity frame open. `jsx.ts`: 7.75 s →
**3.26 s**.

**Fix C: `type_only_alias_declaration` is memoised per alias**
(`check.rs`, with the memo in `perf_links.rs`). Native reads
`links.typeOnlyDeclaration`, which is set once per alias
(`getTypeOnlyAliasDeclarationEx`, `checker.go:2144`). The port walked the
alias chain for every value identifier, `export *` barrels included,
through `specifier_type_only_export_star`. In checker.ts every imported
name crosses `_namespaces/ts.ts`'s barrel, and the walk was 7.2% of that
file's check Ir. The walk reads only the binder, the module host and
completed alias targets. An alias still resolving answers `None` early,
so the result is published only when no alias resolution is open.
jsTyping wall, interleaved with A and B already applied: 5.42 → 5.09 s.

**Combined wall** (interleaved, 3 runs, loaded box): main 16.79 s, A and
B 5.42 s, A, B and C 5.09 s, tsgo 2.39 s. jsTyping goes from 7.0× tsgo
to 2.1×.

**How we would know these were wrong.** For A: a corpus or EXACT row
where the §56 retained-literal road answered during the holder's own
resolution and the general road answers differently. That would show
up as a widened literal member (`"a"` printed as `string`). For B: any
row that differs, since the memo returns what the computation returned.
The same holds for C, and also an alias whose type-only status a later
alias resolution would change.

## §26 `propertiesRelatedTo` returns at the first failing property on every road

**Forcing measurement.** In dml, `DeepReadonly<ModelNNN>` relates
`ModelNNN` to `(infer U)[]`. Each such relation cost about 95 K Ir in
`evaluate_conditional_node`: 1,205 relations for 114.7 M Ir. Native's
`propertiesRelatedTo` (`relater.go:4240`) returns at the first target
property the source fails, here `length`. The port stopped early only on
the discriminated road, the one caller that passes exclusions. On every
other road it related all of `Array`'s remaining members before
combining.

r5-relater4 §1 kept that full walk. Applying the early return everywhere
had changed two `arrayDestructuringInSwitch1` type lines, through the
pairs the full walk published to the results store.

**Measured at main `6faf95a3`, early return on every road:**
- Both corpus dumps are identical to the base: 557,343 type rows, and
  the diagnostics rows.
- jsTyping CLI output is identical.
- Ir: dm −2.9%, dml −3.1%, gi 0.
- jsTyping wall −4% and CPU −7.5%, measured on main without §24's fixes.

The publication-order dependence r5-relater4 recorded no longer
reproduces.

**How we would know it was wrong.** An exact-oracle or corpus row that
moves, or a relation answer that depends on a pair the early return
leaves unpublished.

## §27 `resolve_name_with_export_alias` is memoised per identifier

Native resolves an identifier once and keeps the answer in
`links.resolvedSymbol` (`getResolvedSymbol`). Several of this port's
per-identifier rules resolve the same identifier again with the
export-alias walk: `check_value_identifier`, `check_type_reference_name`
and others. On dml's model files that walk was 124 M Ir inclusive (5.0%)
at main `6faf95a3` plus §22–§26.

The answer is memoised per `(node, meaning)` when the node is an
identifier resolved under its own text, as `resolve_identifier_memo`
already does for the plain walk. The walk's only checker callback is
`resolve_alias`. An alias still resolving answers `None`, so the result
is published only when `alias_resolving` is zero at both entry and exit.

Ir on top of §22–§26 and the §25 loader: dm −1.1%, dml −1.2%, gi
−0.1%. CLI output is identical on dm, dml, gi and jsTyping.

## §28 Ratios at main `27c17a4f` (batch 44), with and without this branch

TSR/tsgo-pinned wall · CPU, medians of interleaved runs on a quiet box:
25 runs each for dm, dml and gi, 3 for jsTyping. "Branch" is this chain
rebased on `27c17a4f`. The §25 loader is already on main.

| project | tsgo wall / CPU | main release | main dist | branch release | branch dist |
|---|---|---|---|---|---|
| domain-model | 209.4 / 663.5 ms | 0.530 · 0.396 | **0.492** · 0.378 | 0.499 · 0.371 | **0.488** · 0.362 |
| domain-model-large | 701.2 / 2,523 ms | 0.552 · 0.427 | **0.511** · 0.391 | 0.499 · 0.395 | **0.496** · 0.384 |
| generic-imports | 68.3 / 151.2 ms | 0.558 · 0.275 | **0.545** · 0.263 | 0.567 · 0.277 | **0.539** · 0.265 |
| jsTyping | 1.99 / 5.21 s | 8.22 · 6.28 | **7.43** · 5.71 | 1.95 · 1.53 | **1.92** · 1.50 |

The 0.50 target is met in dist on domain-model at main. It is also met on
domain-model-large with this branch. generic-imports is front-end bound:
parse 22 ms plus bind 12.5 ms of about 37 ms wall, nearly all of it
`lib.dom.d.ts`. jsTyping is checker-bound on `compiler/checker.ts`
(§24's remaining table).

## §29 The flow walk's identifier resolutions go through `resolve_identifier_memo`

Native's `isMatchingReference` and the other flow readers compare
`getResolvedSymbol(node)`, which is resolved once per identifier
(`links.resolvedSymbol`). This port's flow readers called
`binder.resolve_name` directly on every flow step:
`is_matching_reference`, `references_match`, `is_constant_reference`,
`contains_matching_reference`, `get_effects_signature`,
`get_discriminant_property_access` and narrowing.

On jsTyping's `compiler/checker.ts`, with §22–§27 applied, that was
1.45 G of the file's 14.8 G check Ir. For example, `is_matching_reference`
made 420,586 calls at about 1,360 Ir each.

Every such call now goes through `resolve_identifier_memo`. That calls
the same binder function with the same arguments, memoised per
`(node, meaning)` when the node is an identifier resolved under its own
text, and calls it directly otherwise. The answer is therefore identical
by construction. One `&self` reader (`nullable_literal_type`) keeps the
direct call.

Measured on top of §22–§28:
- jsTyping wall −6.2% and CPU −7.8% (interleaved, 3 runs); output
  identical.
- Ir: dm −0.4%, dml −0.5%, gi 0.

The same substitution in the other checker files is mechanical but
meets about 20 `&self` readers. It is not proposed here.
## §31 The chain rebased onto main `bacf0918` (batch 51): gates and ratios

This section is placed before §25 so that the chain's text and §30's
append do not touch the same lines. §22's item 1 is not in the chain,
because it landed as r7-parser's `a83e1dfc5` (§23). The other ten code
commits, §22 items 2–3 through §29, cherry-pick onto `bacf0918` with no
code conflict.

**Gates** against a freeze of `bacf0918`:

- Both unfiltered dumps are byte-identical once the `ms=`/`mib=`
  columns are cut. `compare` reports 0 gained, 0 lost and 0 missing.
- jsTyping leg: 122 against 122 lines, identical, so new_false 0 and
  lost_true 0. CLI output is `cmp`-identical on dm, dml and gi.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,458 → 10,458
  exact, 0 lost, 0 missing.
- Callgrind Ir, `--singleThreaded`, release:

  | project | main | chain | change |
  |---|---:|---:|---:|
  | generic-imports | 191,627,273 | 191,226,759 | −0.2% |
  | domain-model | 857,621,235 | 797,721,649 | **−7.0%** |
  | domain-model-large | 3,736,321,970 | 3,371,628,608 | **−9.8%** |

- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,841 passed, 0 failed.

**Ratios** (TSR/tsgo-pinned, `whole_project_perf.py`, one quiet
session, 21 samples, jsTyping 5; median wall · median child CPU):

| project | main dist | **chain dist** | chain release |
|---|---|---|---|
| domain-model | 0.524 · 0.399 | **0.496** · 0.363 | 0.528 · 0.387 |
| domain-model-large | 0.510 · 0.408 | **0.471** · 0.380 | 0.515 · 0.411 |
| generic-imports | 0.604 · 0.303 | **0.556** · 0.280 | 0.571 · 0.286 |
| jsTyping | 7.42 · 5.85 (earlier session) | **1.81** · 1.42 | 1.90 · 1.53 |

An earlier session on main alone read dm 0.489, dml 0.527, gi 0.565 in
dist. So dm's main-only reading straddles 0.50 between sessions. With
the chain, dm and dml are under 0.50 in dist, and `diagnostics_match`
is true on both. generic-imports is front-end bound (§12, ADR-0056).
jsTyping is still not equivalent work: 122 lines against 86.

## §33 Two more success-path fixes on the chain: inference's property order and the relation-first report

This section is placed before §25, after §31, for the same reason as §31.
Both fixes are in other lanes' files (`inference.rs`, `assignreport.rs`)
and are proposed on `box/r7-perf-proposed` on top of §31.

**Forcing measurement.** Callgrind on dml with the chain (main
`bacf0918` plus §31, `--singleThreaded`, profiling build, 3.50 G Ir)
shows a flat profile: no checker function above 1.7% self. Two
inclusive costs are on paths native takes only after a failure, or not
at all:

1. **`infer_from_members`** cost 116 M Ir inclusive over 4,376 calls.
   200 of those calls come from `DeepReadonly<ModelNNN>`'s
   `T extends (infer U)[]`, which infers from an interface to `Array<U>`
   at about 270 K Ir each. For every one of `Array`'s members the port
   asked `get_type_of_property_of_type(target, name)` before
   `get_type_of_property_of_type(source, name)`. That instantiates the
   target member's type even when the source has no such property.
   Native's `inferFromProperties` (`inference.go:828`) looks up the
   source property first and reads the target's type only when the
   source has one.
2. **`report_relation_failure`** (reached from `check_return_expression`
   and `check_variable_like_declaration`, 5,032 calls on dml) ran
   `elaborate_error`, the union-literal and excess-property arms and
   `report_weak_type_failure` before it related the pair. That is about
   76 M Ir inclusive, almost all of it on assignable pairs. Native's
   `checkTypeRelatedToAndOptionallyElaborate` (`relater.go:430`) returns
   at a successful `isTypeRelatedTo` before any elaboration.

**Changes.**

1. `infer_from_members` reads the source member's type first and skips
   the name when the source has none. This is native's order.
2. `report_relation_failure` computes the reporting relation
   (`relate_with_signature_diagnostic(…, true)`) first and returns at
   `Related`. The answer is kept and used where the road used to compute
   it, so no relation runs twice. A source whose node is an object literal,
   or which is a fresh object literal type, keeps the old order, because
   its excess-property arms run in this function rather than inside the
   relation.

Neither is a cache, side table or traversal, so the checker-port record
does not apply.

**Refused first cut of change 2.** A separate silent relation
(`report_errors = false`) in front of the old road changed
`compiler/relationComplexityError` from TS2859 to TS2322. The silent
relation overflowed first and published `ComplexityOverflow`, and the
reporting relation that followed read that entry and answered
`NotRelated`. Computing the reporting relation once, as landed, keeps
that case byte-identical.

**Measured** against the freeze of `bacf0918`, as cumulative Ir on top of
§31 (`--singleThreaded`, release):

| project | §31 chain | + change 1 | + change 2 |
|---|---:|---:|---:|
| generic-imports | 191,226,759 | 191,194,704 | 191,232,721 |
| domain-model | 797,721,649 | 793,080,069 (−0.6%) | 789,713,136 (−1.0%) |
| domain-model-large | 3,371,628,608 | 3,357,742,886 (−0.4%) | 3,341,605,125 (−0.9%) |

- `compare`: 0 gained, 0 lost and 0 missing. The diagnostics dump differs
  from the base in one row, a WRONG case:
  `compiler/styledComponentsInstantiaionLimitNotReached` loses its two
  false reports (TS2577 at 171:8 and TS2456 at 182:13). Native expects two
  TS2344, so the case stays WRONG. Change 1 alone causes this (checked
  with change 1 built without change 2). The only thing it removes is the
  instantiation of target members the source lacks, so the case depends
  on that order. It was not reduced. The types dump is byte-identical.
- jsTyping leg: 122 against 122 lines, identical. CLI output is
  `cmp`-identical on dm, dml and gi.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,458 → 10,458
  exact, 0 lost, 0 missing.
- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,841 passed, 0 failed.

**How we would know these were wrong.** For change 1: an inference
that depended on instantiating a target member the source lacks, which
would show as a changed type row. For change 2: a TS2322, TS2353 or
TS2559 that `elaborate_error` or the weak-type arm reported on a pair the
relation calls `Related`. Native never reports one, so a row losing such
a report would be a fidelity gain to verify, not a loss to restore.

### §33.1 The chain re-gated on main `ad4f76096` (batch 60)

The fourteen commits of §31 and §33 rebase onto `ad4f76096` with no
conflict. Gates against a freeze of `ad4f76096`:

- `compare`: 0 gained, 0 lost and 0 missing. The types dump is
  byte-identical. The diagnostics dump moves only §33's
  `styledComponentsInstantiaionLimitNotReached` row (WRONG, two false
  reports fewer).
- jsTyping leg against tsgo-pinned (86 lines): default mode 110 against
  110, `--singleThreaded` 108 against 108, both identical, so new_false 0
  and lost_true 0. CLI output is `cmp`-identical on dm, dml and gi.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,653 → 10,653
  exact, 0 lost, 0 missing.
- Callgrind Ir, `--singleThreaded`, release: generic-imports
  191,383,183 → 190,971,967 (−0.2%); domain-model 859,130,754 →
  791,042,078 (**−7.9%**); domain-model-large 3,746,920,151 →
  3,350,716,320 (**−10.6%**).
- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,854 passed, 0 failed.

## §25 The loader prepares the libs beside the roots, and the walk publishes from tickets

This section is numbered after §22–§24, which are on the
`box/r7-perf-proposed` chain.

**Forcing measurement.** A scratch timeline on dml (main `59ceea97`,
4 checkers, release, a loaded box):

| ms | event |
|---:|---|
| 1.4 | root prefetch starts |
| 26.9 | `prepare_root_parses` barrier ends: 202 roots parsed privately with `ordered`, `Lookahead::One` |
| 27 → 51 | the walk publishes the roots |
| 51.8 | `lib.es5.d.ts` parsed serially (3.0 ms) |
| 69.1 | `lib.dom.d.ts` parsed serially (15.5 ms) |
| 70.8 → 93.2 | bind |

The libs were never prepared, for two reasons. The lib tasks are added
after the prefetch (`add_lib_file_tasks`). `prepare_dependencies` excludes
static texts (r5-loader §2). Native queues the lib tasks with the roots
before `filesParser.start` (`fileloader.go:157`), so `lib.dom.d.ts`
parses beside them.

**Change** (`FileLoader::prepare_root_parses`):

1. When root preparation runs at all (the existing ≥128 KiB, ≥8-root
   barrier), it also prepares the bundled libs the walk will load.
   Those are the default libs plus, transitively, the
   `/// <reference lib>` directives in their texts, read with
   `parse_file_references` and resolved through `get_lib_file_name`, the
   same functions the walk uses. Under `libReplacement` a lib path is a
   traced module resolution, so prediction is skipped.
2. The jobs go to the dependency pool in walk order, except the largest
   file. That is `lib.dom.d.ts` under the default libs. It is visited last
   but takes longer to parse than the walk spends publishing everything
   before it, so it starts first.
3. There is no barrier. The walk waits on a file's ticket at its visit,
   so it publishes root *k* while later roots still parse. Dependencies
   queue behind on the same leased workers. A separate pool would have
   held the lease through the walk, and every `node_modules` dependency
   would then have parsed serially.

**Output.** Identical by construction. A prepared parse is used only
when its text equals the visited text, as before, and publication is the
path roots already took. CLI output and `--listFiles` are identical on
dm, dml, gi and jsTyping. `tsr-compiler` tests pass.

**Measured** on a quiet box at main `6faf95a3`, interleaved release runs
against main. Wall and CPU from 25–30 runs; `--extendedDiagnostics` phases
from 16 rounds.

| project | wall main → change | CPU | `Program time` |
|---|---|---|---|
| domain-model | 131.0 → **124.6 ms** (−4.9%) | 303.5 → 300.0 ms | 53.5 → 47.0 ms |
| domain-model-large | 405.5 → **383.0 ms** (−5.5%); repeat pair 401.2 → 385.6 ms | 1083 → 1081 ms | 92.5 → 71.0 ms |
| generic-imports | 43.5 → 44.3 ms (noise) | | unchanged: its roots are under the barrier, so nothing is prepared |

Check time did not move. A first 25-run session on the same binaries
read dml +1%, against −5% in every later interleaved session. That is
the VM's run-to-run noise at this size, so only the repeated pairs above
count.

**Rejected alternatives:**
- *The barrier kept, with libs added and an unordered longest-first
  pool.* The barrier still serialises the whole parse ahead of all
  publication.
- *Longest-first submission without a barrier.* The first root waited
  24 ms behind every lib.
- *Plain walk order.* dm's walk reached `lib.dom.d.ts` 23 ms before its
  parse finished.

r5-loader §6.2 refused a lib.dom side pool at 71.7 → 80.6 ms on gi.
That was measured with lib.dom's private parse at 48.7 ms, and gi is
not affected here.

**How we would know it is wrong.** One sign would be a program whose
predicted libs are not visited (a root with
`/// <reference no-default-lib="true"/>`) and that pays the unused
parses on wall. The pool drains queued jobs when the walk ends. Another
would be a dependency-heavy root set (≥128 KiB of roots plus many
`node_modules` declarations) where the queued root jobs delay the first
dependency visits by more than the barrier used to cost.

## §30 `recursiveConditionalCrash3`: a deferred conditional's eager branches regress without bound

**Symptom.** At main `d7346aa22` (batch 47) the oracle worker for
`compiler/recursiveConditionalCrash3.ts` grows by about 220 MB/s. It
reached 4.5 GB in 20 s, past the gate's cap. Native finishes the case in
0.32 s, at 80 MB.

**Attribution.** This is not a batch-47 change. With the same driver
(`full_oracle_actual`, per-process, case 5834), the batch-46 head
`57e45ae70` grows on the same curve: 2.26 GB at 10 s against 2.40 GB, and
4.43 GB at 20 s against 4.52 GB. That is about 5% faster at batch 47 and
otherwise the same. §19 recorded the case at 3.4 GB in 25 s at batch 19.
The flip from TSR_TIMEOUT to an OOM is the growth rate crossing the
worker's memory before the 60 s deadline. No commit introduced it.

**Native's limits are not what stops native.** On this case the counting
tsgo (§23) makes 237 `getConditionalType` calls and 43,531
`instantiateType` calls, and reports no TS2589. Native never approaches
`instantiationDepth` 100, `instantiationCount` 5,000,000 or the
tail-recursion limit of 1000. This port already applies the depth-100 and
5M checks in `evaluate_conditional_alias` and `instantiate_type`. Its
TS2589 reporter is held, because it would add false reports
(`current_node.rs`, r5-spans §2.3). Porting the reporter would add a
TS2589 native does not print, and would not bound memory. At this case's
rate the 5M budget is about 23 minutes and tens of GB away.

**Cause.** A scratch trace showed `Expand__` re-entered 96 levels deep,
always with the same `Depth` tuple and a growing `O`
(`X[K][P extends Keys ? 'value' : 'default'][…]`). `O` is generic, so
`O extends CanBeExpanded` is deferred (`is_deferred_type` is true). But
the deferred conditional's mint inside a mapped template
(`get_type_from_type_node_worker`, the `mapped_template_depth > 0` arm)
evaluates both branch types immediately, for
`mapped_conditional_branches`. Its true branch is
`Expand__<O[…], Keys, P, N, Depth>`. Under the alias frame that binds
`N = 4` and `Depth = [1, 1]`, that decides `N extends Depth['length']`
false and reaches the same deferred conditional again, with a longer `O`.
Native keeps a deferred conditional's branches lazy
(`getTrueTypeFromConditionalType`) and stops at the first deferral.

**Fix (`declared.rs`, with a field in `checker.rs`).** The mint records its
conditional node in `eager_branch_conditionals` while it evaluates the
branches. A mint of the same node re-entered from inside that evaluation
keeps its root without branches, which the arm already handles ("the root
and mapper remain useful when a branch is not yet computable"). The case
now completes in 0.23 s at 3 MB. The types it prints are still not
native's; native prints 1.25 MB of expanded types.

**How we would know it was wrong.** A corpus row whose inner deferred
conditional relied on branches computed by a re-entrant mint of the same
node. It would show as a mapped conditional that no longer re-evaluates
after substitution.

**Re-measured on main `bacf0918` (batch 51).** The CLI reproduces it
without the oracle: `tsr --target es2015 --noEmit` on the case file
reaches 4.1 GB in 14.7 s and aborts on an allocation failure. tsgo-pinned
finishes in 0.35 s at 89 MB and prints nothing. With the fix, TSR finishes
in 0.23 s at 82 MB and prints nothing. Gates against a freeze of
`bacf0918`:

- Both unfiltered dumps are byte-identical once the `ms=`/`mib=` columns
  are cut. `compare` reports 0 gained, 0 lost and 0 missing.
- jsTyping leg: 122 against 122 lines, identical, so new_false 0 and
  lost_true 0.
- Exact oracle (`--workers 1`, `ulimit -v 4500000`): 10,458 → 10,458
  exact, 0 lost, 0 missing. Case 5834 goes from TSR_FAILED (allocation
  failure at 22.9 s) to WRONG `types:type-text` at 58 ms.
- Callgrind Ir, `--singleThreaded`: generic-imports 191,627,273 →
  191,621,985; domain-model 857,621,235 → 857,975,134 (+0.04%).
- Strict clippy and fmt are clean. `cargo test --workspace --release`:
  3,841 passed, 0 failed.

**The other oracle timeout is the same family, and this guard does not
cover it.** `conformance/types/keyof/keyofAndIndexedAccess2.ts` is
TSR_TIMEOUT at 60 s (native 156 ms). Its trigger is four lines
(#30920):

```ts
type StrictExtract<T, U> = T extends U ? U extends T ? T : never : never;
type StrictExclude<T, U> = T extends StrictExtract<T, U> ? never : T;
type A<T> = { [Q in { [P in keyof T]: P; }[keyof T]]: T[Q]; };
type B<T, V> = A<{ [Q in keyof T]: StrictExclude<B<T[Q], V>, {}>; }>;
```

TSR does not finish in 10 s, and its memory grows slowly (430 MB). tsgo
prints nothing. The profile is `evaluate_conditional_alias` →
`evaluate_mapped_type_node` → `extends_instantiation` →
`definite_conditional_outcome`, re-entering `B<T[Q], V>` with a longer
`T`. That is the same eager evaluation of a deferred alias, reached
through the conditional-alias road rather than the mint. It is not attempted in this commit; it is
routed to the integrator for an issue and an owner.
