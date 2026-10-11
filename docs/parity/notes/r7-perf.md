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
covers both findings. Named types (interfaces, classes, aliases with
arguments), literals and primitives defer. `main.ts`'s `item: Model000` is
the case §14 measured.

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
