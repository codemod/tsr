# Lazy signature parameter demand — tsr-2zk.9.7

## Current reproduction

Investigated on parent `5dd3bad84d12991e1ba169d2d5687321e1989740`, against
`vendor/typescript-go` commit `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The native CLI was built directly with Go 1.26.8; Cargo used the installed
pinned Rust 1.96.0 toolchain. No offline bootstrap or tracked setup changes
were necessary.

Run both CLIs with `--ignoreConfig --strict --noEmit --pretty false` on:

```typescript
function getValue(argument: typeof value): number { return 1; }
const value = getValue(1);
function text(argument: typeof message): string { return 'ok'; }
const message = text('hello');
```

TSR emits TS7022 at `(2,7)` and `(4,7)`:

```text
'value' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.
'message' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.
```

Pinned native emits no diagnostics. This is a current annotated-parameter
reproduction, not evidence inferred from historical case names.

A distinct, genuinely circular control must retain native diagnostics:

```typescript
function circular(argument: typeof result) { return argument; }
const result = circular(1);
function getNumber(argument: number): number { return argument; }
const invalid = getNumber('text');
```

Pinned native emits, in order:

- `(1,10)` TS7023: `'circular' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.`
- `(1,19)` TS2502: `'argument' is referenced directly or indirectly in its own type annotation.`
- `(2,7)` TS7022: `'result' implicitly has type 'any' because it does not have a type annotation and is referenced directly or indirectly in its own initializer.`
- `(4,27)` TS2345: `Argument of type 'string' is not assignable to parameter of type 'number'.`

Current TSR emits only the latter two. Removing all circularity diagnostics
would be wrong; the return/parameter resolution ordering also matters.

## Native operation and existing boundary

`Checker.getSignatureFromDeclaration` in `internal/checker/checker.go` stores
parameter symbols and publishes `signatureLinks.resolvedSignature` without
reading their types. The signature initially has unresolved return and
predicate slots. `Checker.getTypeOfParameter` reads `getTypeOfSymbol`, then
adds declaration optionality. Fixed-position consumers enter through
`Checker.tryGetTypeAtPosition` in `internal/checker/relater.go`; rest-position
consumers read the rest symbol type separately. The node builder's
`symbolToParameterDeclaration` / `serializeTypeForDeclaration` performs
written-node reuse at serialization time, not at signature construction.

TSR already has `Parameter`'s private `Slot::{Resolved, Symbol}` in
`signatures.rs`. However, `parameter_of` uses `Symbol` only for an
unannotated parameter whose own symbol Type resolution frame is already
active and whose type is not published. All other identifier parameters
resolve annotations or symbol types eagerly. The annotated reproduction
above therefore still closes a cycle during construction that native does
not construct.

The existing `parameter_type` accessor can perform symbol-backed demand,
using the existing `symbol_types` cache and `resolutions` stack. It is not
necessary to invent a second semantic cache. Fully changing construction
also requires preserving written-annotation reuse and contextual/mapped
parameter images; merely broadening the unannotated special case does not
port this root cause.

### Ownership, publication, context, work

- **Key/value:** parameter identity is `SymbolId` in this checker's binder;
  resolved values are `TypeId` in its type store. An instantiated or
  contextually assigned parameter instead retains its resolved image.
- **Owner/lifetime/options:** the private Checker owns `symbol_types` and the
  Type-resolution stack for its checking lifetime. Optionality depends on
  `strict_null_checks` and the declaration's initializer/question token.
  Types computed under alias/mapped/contextual bindings must not be reused
  as an unrelated original declaration's type.
- **Publication:** an absent symbol entry is uncomputed; a Type frame on
  `resolutions` is active, not completed. The existing symbol worker
  publishes the result in `symbol_types` after computation. A symbol slot
  carries identity, not a provisional resolved answer. TSR's unsupported
  `error` type is not proof of native's completed error type. The current
  `parameter_of` error-annotation fallback must be addressed at lazy demand,
  not silently retained as an eager producer or replaced by suppression.
- **Consumer context:** `this`, rest versus fixed position, optionality,
  written aliases and annotation origin, original versus instantiated
  signature, and alias-evaluation mapper context remain distinct. Native
  semantic optionality and printer-written annotation are not the same
  slot representation.
- **Expensive work:** `get_type_of_symbol` / its variable-parameter-property
  worker and `get_type_from_type_node` are the current computation boundary.
  `symbol_types` already owns completed reuse; no extra cache was added.
  Worker executions, active repeats, completed hits, copies and whole-project
  benefit were not instrumented or measured in this investigation.
  **Integrator Beads follow-up request:** track those demand/publication
  counts and mapper-context equivalence under tsr-2zk.9.7 before extending
  reuse. This Box has no authorized Beads mutation scope.

## Required serialized cross-owner changes

No semantic implementation was applied: the faithful cutover crosses
explicitly forbidden whole-file boundaries. The integrator has selected a
single owner for the broad cutover once current node-reuse, checker, objects,
members and inference consumers release their files. The requirements below
are that future owner's atomic contract, not requests for piecemeal edits or
heuristic workarounds. `.9.7` remains open with no conversions.

1. **`crates/tsr-checker/src/node_reuse.rs`: `WrittenAnnotation`,
   `Checker::reuse_annotation`, its equivalence/serialization readers.**
   A written annotation currently stores an already-resolved `TypeId`;
   signature construction cannot populate that identity without resolving
   the parameter. Provide a real demand-time annotation path that preserves
   the original node and mapping context, and applies the existing
   `pseudoTypeEquivalentToType` rules only when serialization demands it.
   Do not store `error` as a fake unresolved equivalence identity.
2. **`crates/tsr-checker/src/checker.rs`:
   `Checker::signature_member_text_at`.** Replace its direct
   `parameter.written_text` read with the demand-time annotation accessor
   agreed with the calls owner, after semantic parameter demand. Preserve
   current reference-site and alias rendering. This file is not owned here.
3. **`crates/tsr-checker/src/objects.rs`: `signature_member_text`.** Replace
   the direct `parameter.written_text` read with the corresponding site-free
   demand-time annotation accessor. Preserve declaration syntax and
   predicates. This file is not owned here.
4. **Inference preparation contract:**
   `inference.rs::mentions_type_parameter_inner` currently follows
   `peek_parameter_type`, which omits unpublished symbol slots. A full lazy
   cutover must not turn an unpublished edge into a proven absence of type
   parameters. The calls owner owns this walk, but its `&self` contract is
   consumed outside ownership by `contextual.rs`, `declared.rs`,
   `index_access_reports.rs`, `mapped.rs` and `members.rs`. Agree on demand or
   prepared-completion semantics before changing those callers. Simply
   ignoring the edge can incorrectly skip instantiation/inference.
5. **Oracle coordination:** `parity-full-corpus` exclusively owns all
   `crates/tsr-conformance/src`, including `types_producer.rs`, and owns the
   migration of obsolete expected-driven producer API callers. Route any
   producer contract request through the integrator; this lane must not edit
   conformance sources or other owners' permanent caller tests. New
   `calls_lazy_*.rs` tests remain exclusive to this lane. The current coverage
   runner writes snapshots unconditionally; the oracle owner/integrator must
   provide or run the read-only full-population gate. The existing verdict
   tools are not that gate. Native root claims `tsr-2zk.16.27` and
   `tsr-2zk.16.61` are assigned; this lane does not claim their work.

Once those files are released to the single cutover owner, that owner must
convert original parameter construction, all parameter printers and
mapping/inference consumers together. Pattern parameters currently use eager declaration
computation and admission gates; a whole-parameter-symbol port must also
coordinate their binder identities rather than retaining an undocumented
identifier-only cutover.

## Frozen baseline and verification limits

Before any tracked edit, the unfiltered commands completed:

```text
cargo build --release -p tsr-conformance -p tsr
cargo run -q --release -p tsr-conformance --example diagverdictdump
cargo run -q --release -p tsr-conformance --example verdictdump
```

The frozen release binary and dumps were stored under `/tmp/box/base`.
Their SHA-256 identities:

- `tsr`: `866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`
- `diag.tsv`: `ff08a03bb448a639ac7e219d8e7e4b2921b6aa5cf95230c4744efba413da9482`
- `types.tsv`: `25ed9bd55f95956c4515044bf7f41334799bbcc6e2a186eaf6ad36824621660a`
- Native CLI: `7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`

Diagnostic dump: 10,570 rows; RIGHT 4,221, EMPTY_RIGHT 4,968, WRONG 1,281,
EMPTY_WRONG 100. Type dump: 477,970 verdict rows; RIGHT 469,765, GAP 993,
WRONG 7,212. The type file also contains six non-verdict continuation lines;
raw physical line count is 477,976, not the semantic denominator.

These are unfiltered *tool invocations*, not full-population proof:
`verdict.rs::verdict_rows` excludes varied types and known divergences, and
omits absent/unaligned producer assertions. Diagnostic verdicts do not prove
complete messages, lengths, chains and order. No candidate binary exists,
so no after-loss check, candidate performance claim or verified TSR/tsgo
ratio is asserted. No code tests were added or run for a nonexistent port.

Named targets checked against current code:

- `compiler/functionWithDefaultParameterWithNoStatements16`: diagnostics
  empty; focused type dump contains no WRONG/GAP rows. Not a new conversion.
- `compiler/contextualParamTypeVsNestedReturnTypeInference4`:
  EMPTY_RIGHT diagnostics. Not a new conversion.
- `compiler/reverseMappedTypeContextualTypeNotCircular`: WRONG diagnostics;
  TSR emits none. Pinned native directly reports TS2322 with
  `Target signature provides too few arguments. Expected 2 or more, but got 1.`
  This observed missing relation diagnostic is not evidence of current
  eager-parameter circularity in that case.

Converted cases: none. The release target and all remaining exclusive
cases remain unverified, not reduced to the controls above.

## INFER-NO-CANDIDATE-GUARD experiment — tsr-2zk.16.61

The integrator subsequently assigned this root to the calls owner. The prior
oracle-coordination paragraph describes its earlier ownership notification,
not a competing current claim. The Box's local `bd show` could not resolve
`.61`; that is a local database limitation, not evidence that the issue is
absent. The passive `.beads/issues.jsonl` export is stale and not authoritative.
The integration owner confirms `.61` exists and supplies these 20 current
failed targets (blocked scope, not promised conversions):

- `compiler/acceptSymbolAsWeakType`
- `compiler/arrayFlatMap`
- `compiler/computedPropertyBindingElementDeclarationNoCrash1`
- `compiler/contextualParamTypeVsNestedReturnTypeInference4`
- `compiler/contextualTypeFunctionObjectPropertyIntersection`
- `compiler/declarationEmitOverloadedPrivateInference`
- `compiler/dissallowSymbolAsWeakType`
- `compiler/doYouNeedToChangeYourTargetLibraryES2015`
- `compiler/doYouNeedToChangeYourTargetLibraryES2016Plus`
- `compiler/implicitIndexSignatures`
- `compiler/reverseMappedTypeContextualTypeNotCircular`
- `conformance/contextualTypeTupleEnd`
- `conformance/genericCallWithConstructorTypedArguments5`
- `conformance/genericCallWithFunctionTypedArguments5`
- `conformance/genericRestParameters1`
- `conformance/inferingFromAny`
- `conformance/objectLiteralContextualTyping`
- `conformance/partiallyAnnotatedFunctionInferenceWithTypeParameter`
- `conformance/restTupleElements1`
- `conformance/variadicTuples1`

Historical counts are not current completion evidence. Other prerequisites
may block these sources; already-RIGHT type IDs or EMPTY_RIGHT diagnostic
cases are controls, not conversions.

### Native-supported reproduction and experiment

Pinned `Checker.getInferredType` succeeds without candidates: use an
instantiated default if present, otherwise unknown (any with AnyDefault),
then apply the instantiated constraint. NoDefault instead uses silentNever.
`InferenceTypeMapper.Map` in `internal/checker/mapper.go` consumes
intra-expression sites before fixing, clears non-fixed cached inferences,
marks the selected inference fixed, and then demands its inferred type.
Candidate writers in `inferFromTypes` clear non-fixed inference results when
candidate sets or top-level widening change. `newBackreferenceMapper`
provides unknown for self/forward default references; earlier references
use the context's non-fixing mapper.

Current TSR already implements those fallback choices in
`resolve_inference_with_constraints`; the additional
`check_generic_call_worker::structural_source_supplied` veto prevents that
resolver from running. Removing that veto and its now-dead
`predicate_only_inference_has_no_source` exception was tested, then withdrawn
because it failed the zero-loss gate. No semantic change or regression test
from the withdrawn experiment is committed.

Native CLI declaration emit and the TSR corpus producer agreed after the
experimental cutover on these controls, previously TSR error:

| Declaration and call | Native/candidate result |
|---|---|
| `infer<T>(value: { p?: T }): T; infer({})` | `unknown` |
| `infer<T = string>(value: { p?: T }): T; infer({})` | `string` |
| `infer<T extends { tag: string }>(value: { p?: T }): T; infer({})` | `{ tag: string; }` |
| `infer<T = string, U = T>(value: { p?: U }): [T, U]; infer({})` | `[string, string]` |
| `infer<T extends string = 'fallback'>(value: { p?: T }): T; infer({})` | `"fallback"` |

Distinct supplied-member, contextual-return and fixing controls stayed
`number`: `{ p: 1 }`, `const result: number = infer({})`, and
`callbacks(() => 1, value => { const checked: number = value; })`.
Three new behavioral tests exercised absent optional members, dependent
non-fixing defaults, and supplied/contextual candidates; all passed in the
workspace release run. They were removed with the withdrawn implementation,
not committed as permanently failing tests.

### Publication and work boundary

No new cache, identity or mapper was introduced. Existing private-Checker
`InferenceInfo` domains are type-parameter TypeIds in the checker's store;
context includes original signature, ordered parameters, priority,
covariant/contravariant candidates, fixing state and JS AnyDefault options.
`resolve_inference_with_constraints` uses its call-local map for provisional
recursive default/constraint resolution, then completed ordered results.
`fixed_type` preserves a fixing read; non-fixing reads recompute from current
candidates. An absent candidate is not an unsupported producer type and is
not a completed failure. The experiment mistakenly exposed existing
collector/producer omissions as absent candidates; that distinction is the
remaining prerequisite, not justification for a new syntax heuristic.
Written alias context, concrete receiver and diagnostics remain owned by
existing signature/instantiation consumers. The removed veto performed
parameter graph walks solely to decline; no count of worker executions or
result copies was collected. Integrator Beads request: record collector
unsupported/completed-absence and circular-source boundaries under `.61`
before broadening reuse. No optimization claim follows from this experiment.

### Completed gates and rejection evidence

Frozen candidate SHA-256:
`2c077758bb6653573976dc664c0ff3a89108a00198eb4ebed59c6f56759e2332`.
Both unfiltered verdict invocations completed, with missing-key checks:

- Types: 477,970 IDs before and after; RIGHT 469,765 -> 469,859,
  GAP 993 -> 911, WRONG 7,212 -> 7,200. Transitions: 39 GAP->RIGHT,
  58 WRONG->RIGHT, 43 GAP->WRONG, **3 RIGHT->WRONG**. No missing type IDs.
- Diagnostics: 10,570 IDs before and after; RIGHT 4,221 -> 4,227,
  EMPTY_RIGHT 4,968 -> 4,965; **3 EMPTY_RIGHT losses**. No missing case IDs.
- A throwaway read-only runner invoked every existing coverage suite over
  all 12,444 discovered cases without touching snapshots. All suites
  completed. `checker_types`: 8,054/9,538 (2,906 skipped), line percentage
  98.12135197502376; `diagnostics`: 4,227/5,502 (6,942 skipped).
  These retain the legacy scorer's exclusions, not the strict full-population
  oracle promised by `parity-full-corpus`.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
  `cargo fmt --all -- --check` passed after formatting the owned changes.
- `cargo test --workspace --release` failed only when it reached
  `tsr-conformance/tests/signature_position_prerequisites.rs`: two tests pin
  native-supported calls to TSR's old `error` refusal. The remaining workspace
  tail was not run, so the workspace is not reported clean.

The experimental candidate would fully convert 12 legacy-scored type cases:
`compiler/couldNotSelectGenericOverload`,
`compiler/declarationEmitOverloadedPrivateInference`,
`compiler/doYouNeedToChangeYourTargetLibraryES2016Plus`,
`compiler/implicitIndexSignatures`,
`compiler/indexSignatureOfTypeUnknownStillRequiresIndexSignature`,
`compiler/jsxInExtendsClause`,
`compiler/reverseMappedTypeContextualTypeNotCircular`,
`compiler/tupleTypeInference2`,
`conformance/genericCallWithConstructorTypedArguments5`,
`conformance/indexSignatureTypeInference`, `conformance/inferingFromAny`,
`conformance/objectLiteralContextualTyping`. These are rejected experimental
conversions, not delivered gains or completion of the 20-case target list.

### Exact prerequisite failures

1. **Circular source/return completion:** all three formerly-RIGHT type
   losses are `compiler/circularReferenceInReturnType2` IDs `0:27`, `0:30`,
   `0:31`. Native directly emits TS7022 for A and TS7023 for fields;
   `fields` prints `() => any`. Candidate emits no CLI diagnostics and prints
   `() => { a: Field<unknown, string>; }`. Its field argument's `type: A`
   producer is `error`, and `inference.rs::infer_from_types_within` ignores
   error sources, allowing fallback to unknown. The initializer's source
   resolution must have native semantics before removing the veto.
   Integrator coordinate `symbols.rs::get_type_of_variable_or_parameter_or_property_worker`
   / `report_circularity_error` with the owned
   `signatures.rs::return_type_of` and `calls.rs` resolution consumers.
   The pinned operations are `getTypeOfVariableOrParameterOrPropertyWorker`,
   `reportCircularityError`, `getReturnTypeOfSignature`, and
   `getResolvedSignature`. Merely relabeling all TSR unsupported errors as
   native any would fabricate inference and is rejected.
2. **Missed reverse-mapped contravariant candidate:**
   `compiler/contravariantOnlyInferenceFromAnnotatedFunction` loses
   EMPTY_RIGHT; native directly emits no diagnostics and infers
   `[string, { bar: string; }]`; candidate infers
   `[unknown, { bar: string; }]` and reports TS2322. Owned
   `inference.rs::infer_to_mapped_constraint_worker` / `infer_from_members`
   require native `inferToMappedType` and reverse-mapped member inference,
   not an unconditional default. This is a collector prerequisite to queue
   under its own root; it is not permission to special-case empty buckets.
3. **Generic indexed callable source:**
   `compiler/voidReturnIndexUnionInference` loses EMPTY_RIGHT. Native directly
   emits no diagnostics; candidate reports two TS2345s for
   `P["onFoo"] | undefined` / `P["onBar"] | undefined` against callbacks
   returning unknown. The indexed source's constraint/signature exposure
   must match native before `inference.rs::infer_from_types_within` /
   `infer_from_signature_parameters` can collect R. Coordinate the actual
   indexed-type producer with its owner; no `types.rs`, `members.rs`, or
   symbol producer edits were authorized here.
4. **Symbol-indexed member source:** `conformance/symbolProperty61` loses
   EMPTY_RIGHT. Native directly emits no diagnostics; candidate reports
   TS2345 for `MyObservable<number>` against `InteropObservable<unknown>`.
   Its computed-symbol member/index signature image must support native
   `inferFromProperties` / `inferFromSignatures` before calling an empty T
   bucket a completed absence. Coordinate member-image production with its
   owner; `inference.rs::infer_from_members` is the owned consumer.
5. **Obsolete caller test expectations:** oracle owner must update
   `signature_position_prerequisites.rs::primitive_array_mismatch_does_not_skip_naked_or_unsupported_generics`
   (`unsupported` expects error) and
   `empty_global_arrays_do_not_prove_a_primitive_mismatch`
   (both empty-array winners expect error). Existing comments already say
   native accepts/selects these candidates. This lane did not edit the tests.

### Fresh-process performance, experimental candidate only

Harness ran interleaved fresh-process pairs versus frozen TSR and pinned
native. Initial 21-pair generic-imports baseline measurement was noisy
(wall 1.067749, CPU 1.046451); repeated with 41 pairs as required.

| Project | candidate/baseline wall | candidate/baseline median child CPU | candidate/native wall (21 pairs) |
|---|---:|---:|---:|
| domain-model | 0.960815 (21 pairs) | 0.998643 | 1.120973 |
| generic-imports | 0.999717 (41 pairs) | 0.998544 | 0.773407 |

Diagnostics match in all measured comparisons. Loaded-file scope matches;
`complete_input_equivalence_verified=false` and
`actual_checked_work_verified=false` in every report. These are observed
ratios only, neither verified comparable-work ratios nor the <=0.50 release
target. Initial samples ran alongside builds/checks; that noise is explicit.
The rejected candidate is not a released optimization.

Final disposition: restored the original owned inference implementation and
removed the experimental permanent tests. Only this evidence record is
committed. `.61` remains unimplemented; three type and three diagnostic
losses must be solved faithfully before the cutover can ship.

### Subsequent ownership clarification and inline collector inspection

The broad lazy-signature root is reserved for one future whole-file owner;
no piecemeal `node_reuse`/shared-printer edits are requested. Current owned
no-candidate work must preserve the distinction between an unpublished
parameter edge, unsupported source work and completed candidate absence.
`compiler/contextualParamTypeVsNestedReturnTypeInference4` was already
EMPTY_RIGHT for diagnostics; it is not a conversion. Frozen native outputs
remain reproduction evidence, not proof of an implemented port.

Inline inspection of `infer_to_mapped_constraint_worker` confirms the
homomorphic `keyof B` arm reverses B and returns immediately, matching native
`inferToMappedType`. Therefore simply adding an extra template walk in that
arm to infer A would be an unproven second convention. The annotated
contravariant repro must instead be traced through the existing deferred
argument recheck (`check_generic_call_worker`), contextual annotation
inferences (`infer_contextual_annotations`) and native fixing/intra-expression
consumers. No speculative extra walk, unknown-to-any substitution or
unpublished-edge skip was added. Any eventual code port retains all full
verification gates; investigation notes have no doc-only tests.

### Next owned cluster: generic arity-survivor overload recovery

After the integrator requested continued owned work, this control reproduced
against the pinned CLI and the current corpus producer:

```typescript
export declare function select<T>(value: { item: T }, extra: number): T;
export declare function select<T>(value: T[]): T[];
export const bad = select('wrong');
export declare function make<T>(values: T[]): T[];
export const tooMany = make(1, 'extra');
```

Native declaration emit prints `bad: unknown` and `tooMany: unknown[]`.
TSR prints `bad: unknown[]` and `tooMany: unknown[]`. The latter is a distinct
already-correct control, not a conversion. In
`calls.rs::choose_ordered_overload`, the sole generic arity survivor bypasses
`transcribed_generic_set_walk` unless an argument is context-sensitive.
Native `chooseOverload` still checks its instantiated applicability. When
that survivor fails, `getCandidateForOverloadFailure` calls
`pickLongestCandidateSignature`, whose `getLongestCandidateIndex` chooses
the first signature covering the argument count (the first declaration in
this control), not the rejected sole arity survivor.

Exact owned API seam: route all generic arity survivors through the existing
candidate applicability walk and perform recovery via
`inference.rs::check_generic_call_with_mode(..., overload_failure=true)`
when all candidates are definitely rejected. Preserve ordered candidates,
written type arguments, fresh failure inference context and previously
assigned contextual argument types. Current `.61` veto blocks this recovery
for the selected `{ item: T }` candidate with a string source; fixing only
selection replaces `unknown[]` with `error`, not native `unknown`. No partial
selection fix was applied. This cluster therefore depends on the measured
no-candidate cutover and its loss prerequisites, not an annotation peek or
an any-vs-unknown heuristic.

Native `contextuallyCheckFunctionExpressionOrObjectLiteralMethod` also
requires a completed contextual signature before
`inferFromAnnotatedParametersAndReturn`; its target parameter and return
reads are semantic demands (`getTypeAtPosition`, `getReturnTypeOfSignature`).
The current `contextual.rs::contextual_signature_result` invokes owned
`infer_contextual_annotations` while obtaining contextual signatures.
`.9.7.1` lazy parameter completion must preserve that re-entry order;
annotation syntax alone cannot certify an unpublished target edge. Route
that atomic signature-demand contract to the future single lazy-cutover
owner. No new cache or completion claim was introduced.

## CANDIDATE-FOR-OVERLOAD-FAILURE receipt — tsr-2zk.16.27

The integrator subsequently assigned this root with 18 current blocked
cases. That count is scope, not promised conversions; the original brief's
case list remains available, but no separate exact 18-case receipt was
supplied. Read pinned `resolveCall`, `getCandidateForOverloadFailure`,
`pickLongestCandidateSignature`, `getLongestCandidateIndex`,
`getTypeArgumentsFromNodes`, `inferSignatureInstantiationForOverloadFailure`,
and the `apparentArgumentCount` writer in `internal/checker/services.go`.

Native recovery is not a shallow longest-signature heuristic:

- `resolveCall` preserves reordered candidates and effective arguments,
  runs subtype then assignability selection, and recovers only after both
  fail. It publishes the recovered signature before reporting errors, so
  diagnostic re-entry observes that identity rather than rerunning recovery.
- `getCandidateForOverloadFailure` first calls `checkNodeDeferred`. It
  combines only a multi-candidate non-generic set without candidatesOutArray;
  otherwise it selects from the original ordered set.
- Selection uses apparentArgumentCount only in the services query scope;
  otherwise it uses effective argument length. First covering/effective-rest
  candidate wins; only if none covers does maximum parameter count win.
- Written arguments use `getTypeArgumentsFromNodes`: truncate surplus,
  append each raw declaration default/constraint/unknown. This is not
  successful-call `fillMissingTypeArguments` substitution.
- Inferred failure arguments use a fresh inference context with JS AnyDefault
  and SkipContextSensitive|SkipGenericFunctions. The original parameter
  identities, concrete receiver and assigned callback context remain distinct.

### Additional directly reproduced controls

```typescript
export declare function failed<T = string>(first: { value: T }, required: number): T;
export declare function failed<T = number>(first: T[]): T[];
export const chosen = failed('wrong');
export class Base<T extends Date> { constructor(value: T) {} }
export class Derived<T extends Date> extends Base<T> {}
export const missing = new Derived();
export declare function written<T = string, U = T>(value: T, other: U): [T, U];
export const writtenFailure = written<boolean>(true);
```

Pinned native CLI declaration output versus current TSR producer:

| Binding | Native | TSR |
|---|---|---|
| chosen | string | number[] |
| missing | Derived<Date> | Derived<Date> |
| writtenFailure | [boolean, T] | [boolean, boolean] |

The constructor control is already correct, not a conversion. A second
written-argument control distinguished valid versus failed ordinary calls:
`partial<boolean>(true,true)` prints `[boolean, boolean]` in both;
`partial<boolean>(true)` prints native `[boolean, T]`, TSR
`[boolean, boolean]`. Excessive `<boolean, number, Date>` prints
`[boolean, number]` in both. Therefore changing all defaults to raw references
would break valid calls; the recovery boundary is the required algorithm.

### Exact atomic implementation seams and blockers

Owned implementation would provide one semantic recovery operation accepting
call NodeId, already ordered candidate vector, actual effective arguments,
and recovery/query mode. It must return the complete instantiated Signature,
not only its return type. Its absent/active/completed/unsupported publication
must use existing resolved-call identities, not a second cache. Required
owned consumers are `choose_ordered_overload`,
`transcribed_generic_set_walk`, `check_call_expression_worker`, and the
written/inferred failure branches in `inference.rs`. Move the current
`written_type_argument_arity_failure` raw-tail filling into that operation;
do not broaden its arity-only trigger as a side-pass workaround.

Two full-root blockers remain:

1. `.61` is a real owned inference prerequisite. Correctly choosing the first
   recovery candidate from the controls above still hits
   `check_generic_call_worker::structural_source_supplied`. Its measured
   removal loses three formerly-RIGHT type IDs and three EMPTY_RIGHT cases.
   A fallback treating unsupported/unpublished sources as completed absence
   is not licensed. `.9.7.1` must supply actual lazy parameter demand, not
   annotation peeking. No safe single-root candidate has passed these gates.
2. Forbidden caller `expressions.rs::check_new_expression` selects a single
   candidate directly and calls `check_generic_call_with` (ordinary mode).
   Full recovery requires routing both single- and multi-candidate failures
   through the same complete recovery Signature API, then publishing before
   diagnostic re-entry. The owner must migrate this caller after the recovery
   API and prerequisite are complete. There is no existing shared
   `checkNodeDeferred` operation in the owned files; its checking-order
   prerequisite belongs to the checking owner, not a local suppression pass.

No partial recovery implementation, shim or permanently failing regression
test was committed. No new candidate gates or conversion matrix is asserted;
the previous `.61` experiment remains the last completed unfiltered evidence.
Further owned cluster work requires an authoritative next-root assignment,
or completion of the stated publication/collector prerequisites. `.16.27`
remains open and unimplemented.

## Semantic rest-pattern arity port — calls lane tsr-2zk.9

The integrator assigned native minimum/count/rest arity as the next owned
root and will file its specific Beads issue after identification. Root:
**rest binding-pattern length overrides the parameter's semantic type**.
Current TS2554 witnesses are `conformance/iterableArrayPattern17` and
`conformance/iterableArrayPattern26`; both mistakenly require two arguments
for an annotated array rest parameter. No explicit matching named cluster
was found in the existing type-triage tables; this is a directly reproduced
native arity root, not a speculative reassignment of a tuple-normalization
cluster. Integrator issue request: link this port under tsr-2zk.9.

### Native algorithm and implementation

Pinned `getMinArgumentCountEx`, `getParameterCount` and
`hasEffectiveRestParameter` (`internal/checker/relater.go`) read the rest
symbol type. A fixed tuple contributes required/fixed elements; an array
rest remains unbounded. A destructuring name does not change the annotation.
`tryGetTypeFromEffectiveTypeNode` (`internal/checker/checker.go`) uses the
implied binding-pattern type for an unannotated non-contextual parameter,
including a rest parameter.

`call_arity.rs::sole_signature_arity` now obtains the original declaration's
Signature and uses the existing canonical minimum/count/rest queries for
rest parameters. Removed the binding-pattern-length arity helper.
`signatures.rs::parameter_of` uses the existing implied-pattern builder for
unannotated rest binding parameters inside its existing non-contextual
admission branch. This companion is necessary: the current symbol worker
otherwise supplies implicit any[] and loses the required nested tuple
arity in `iterableArrayPattern25`. No symbol-worker edit, new cache, syntax
peek of an unpublished edge, or lazy-signature cutover was introduced.

**Boundary:** declaration NodeId and binder parameter SymbolId remain owned
by the private Checker for its checking lifetime. Signature slots retain
original versus instantiated/contextual identity; strict-null-check options
continue to govern void/optional semantic types. No new absent/active/success/
failure table was added. The existing signature constructor and parameter
accessor own forcing; unsupported signature construction remains unsupported,
not a guessed tuple. Alias/default/rest type computation stays with the
existing type store and binding-pattern builder. Expensive work is original
signature construction and rest type demand; there is no duplicate cache.
Worker/hit/re-entry/copy counts were not instrumented. Integrator bounded
Beads follow-up request: measure semantic-rest arity demand counts before
extending query reuse; timing below is not a verified speed claim.

### Before/after native controls

```typescript
function annotated(...[a, b]: number[]) {}
annotated();
annotated(1);
annotated(1, 2, 3);
function fixed(...[a, b]: [number, number]) {}
fixed(1);
fixed(1, 2, 3);
function defaults(a = 1, b: number) {}
defaults(1);
```

Run both CLIs with `--ignoreConfig --strict --noEmit --pretty false`.
Before TSR reports three extra TS2554s on lines 2, 3, 4. After TSR and pinned
native output match byte-for-byte, preserving ordered diagnostics:

- `(6,1)` TS2554: `Expected 2 arguments, but got 1.`
- `(7,13)` TS2554: `Expected 2 arguments, but got 3.`
- `(9,1)` TS2554: `Expected 2 arguments, but got 1.`

Unannotated nested rest control `iterableArrayPattern25` directly matches
native CLI TS2554 output byte-for-byte after the companion fix. Permanent
`calls_min_arity_native_rest.rs` tests exercise annotated array versus fixed
and implied tuples, and required-after-default minimum. Both pass.

### Final verification and limits

Candidate SHA-256:
`6b247ae91157fab06c109a742f6aba05c4c061643a109afd3589b2125771947c`.
Final completed unfiltered dump matrix, including missing keys:

- Types: 477,970 IDs before/after; RIGHT 469,765 -> 469,766;
  WRONG 7,212 -> 7,211; GAP 993 unchanged. **0 formerly-RIGHT losses,
  0 missing IDs**. Only gained ID:
  `compiler/restParameterWithBindingPattern1:0:0`, native/candidate
  `(...{ a, b }: { a: any; b: any; }) => void` versus old
  `(...{ a, b }: any[]) => void`. Other case IDs still gap; no case conversion.
- Diagnostics: 10,570 IDs before/after, all verdicts unchanged. RIGHT 4,221,
  EMPTY_RIGHT 4,968. **0 RIGHT/EMPTY_RIGHT losses, 0 missing case IDs**.
- Read-only full existing coverage suite completed over all 12,444 discovered
  cases: checker_types 8,042/9,538 (2,906 skipped), line percentage
  98.10193064706435; diagnostics 4,221/5,502 (6,942 skipped).
  Existing oracle exclusions and lack of strict message/length/order
  population proof remain explicit; not >=99.9% full-corpus evidence.
- `cargo test --workspace --release`: passed through all tests/doc-tests.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- First semantic-query experiment lost iterableArrayPattern25's RIGHT
  diagnostic; companion implied-pattern fix repaired it. Final matrix above
  is the corrected candidate, not the rejected experiment.

Fresh-process interleaved 21-pair final measurements, diagnostics and loaded
scope matching in every run:

| Project | candidate/baseline wall | median child CPU ratio | candidate/pinned-native observed wall |
|---|---:|---:|---:|
| domain-model | 0.953806 | 0.999043 | 1.036818 |
| generic-imports | 1.000381 | 0.998785 | 0.916880 |

Baseline no-slowdown evidence is within noise; complete_input_equivalence
and actual_checked_work verification flags remain false. No verified native
ratio or <=0.50 release claim. Build/setup is outside the timed child samples.

### Remaining shared prerequisite, not silently counted as converted

`iterableArrayPattern17/26` still fail corpus diagnostic verdicts: TSR now
omits erroneous syntax TS2554 but also omits native argument-type errors
(TS2741 / TS2345). Existing syntax argument checking reads the array
annotation without rest element expansion. Integration owner must route
these complete rest signatures through the shared semantic applicability/
diagnostic caller in `check.rs` and the owned calls applicability consumer;
do not add a second binding-name rule. No outside-owned files were edited.
Delivered: faithful rest-type arity boundary and one native type-line gain;
whole-case conversions: zero.

### Next native arity scope: untyped JS minimum publication

After the rest-pattern port, directly tested two further arity boundaries.
`only(1,2,3)` against a one-parameter function matches pinned native pretty
output including the full excess-argument underline; no span fix is needed
there. Overloaded trailing-void parameters also match native and are not
conversions.

Current `compiler/jsFileFunctionParametersAsOptional` does reproduce a
remaining root: JS `function f(a,b,c){}` called from TS with zero, one and two
arguments emits three TSR TS2554s; pinned native emits none. The CLI control
uses separate foo.js/bar.ts files with `--allowJs --strict false --noEmit`.
Native `getSignatureFromDeclaration` publishes
`SignatureFlagsIsUntypedSignatureInJSFile` only for non-IIFE JS function-like
signatures with all syntactically unannotated parameters and no contextual
type under signature context. `getMinArgumentCountEx` consumes that captured
flag to return zero; `SignatureFlagsPropagatingFlags` retains it through
instantiation. This is not a query about whether all resolved types are any.

Exact required atomic metadata API: add a Signature untyped-JS marker, set
it at original signature construction using the pinned conditions, initialize
synthetic signatures correctly and propagate through instantiated/composite
signatures; consume it in `signature_min_argument_count`. Current literal
constructors in forbidden `contextual.rs` and `decorators.rs` must migrate
with owned `signatures.rs`. Recomputing the context at every min-arity read
would replace publication state and can re-enter inference; no such heuristic
or duplicate side cache was added. This root therefore needs serialized
whole-file metadata ownership. Integrator issue request: untyped-JS signature
minimum publication, current witness jsFileFunctionParametersAsOptional.
The verified rest-pattern commit remains delivered; this next root is open.
