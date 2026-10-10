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
