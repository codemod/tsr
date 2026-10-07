# Lazy object-literal accessors: tsr-2zk.11.5

## Status and identities

Investigation at TSR `5dd3bad84d12991e1ba169d2d5687321e1989740`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. No checker implementation changed:
the faithful cutover crosses files outside this lane's ownership. No converted
cases, optimization claim, or release acceptance claim.

The fresh Box built TSR with the installed pinned Rust 1.96.0 and native with
Go 1.26.8; offline bootstrap was unnecessary. Frozen release binary SHA-256:
`866e1c6ebe89bf98af5cd20e04fb44b93bb4f73a4cd3d84566c6021c0893211b`.
Native binary SHA-256:
`7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.
Temporary binary, verdict and probe artifacts are under `/tmp/box`; they are
not durable repository artifacts and will disappear with the Box.

Before edits, both unfiltered verdict dumps completed:

| Dump | Measured population | Verdicts |
| --- | --- | --- |
| Type assertions | 477,970 aligned rows | 469,765 RIGHT; 7,212 WRONG; 993 GAP |
| Diagnostic cases | 10,570 rows | 4,221 RIGHT; 4,968 EMPTY_RIGHT; 1,281 WRONG; 100 EMPTY_WRONG |

The type output file has 477,976 physical lines, including six non-verdict
lines. Grouping assertion-bearing cases gives 8,093 entirely RIGHT cases out
of 9,535; this is **not** the suite's case denominator: zero-assertion cases do
not appear in that grouping. Neither dump verifies complete diagnostic
messages, lengths, order, variants, or native complete-input checking.

## Direct reproduction

This source was run through pinned native `--declaration --emitDeclarationOnly
--strict --pretty false` and the existing TSR `probefile` example (real corpus
pipeline with bundled libraries):

```ts
function make() {
    return { get self() { return make(); } };
}
const result = make();
const distinct = {
    get value(): string { return "x"; },
    set value(v: number) {}
};
const computed = { get ["value"]() { return 1; } };
const recursive = { get self() { return recursive; } };
const illegal = { get self() { return illegal.self; } };
```

Native declaration output for `make` is:

```ts
declare function make(): {
    readonly self: {
        readonly self: /*elided*/ any;
    };
};
```

TSR's type walker reports `make : () => any`, its object literal as `error`,
and `result : any`. This directly supports the eager-publication hypothesis;
it is not inferred from historical target counts.

Native reports exactly one diagnostic in this control:

```text
../../tmp/box/accessor-controls.ts(8,23): error TS7023: 'self' implicitly has return type 'any' because it does not have a return type annotation and is referenced directly or indirectly in one of its return expressions.
```

The actual scratch source wrote the divergent pair on one line; the diagnostic
above preserves its observed path and location, rather than the expanded
source's location. TSR CLI reported no diagnostic. TSR's
walker reports the illegal member's type as `error`; native emits `any`.

Distinct controls: both preserve `get value(): string; set value(v: number);`
for the divergent pair and `readonly value: number` for the computed getter.
The legal variable-self reference must remain distinct from the illegal
accessor-self reference.

The current corpus names `compiler/noCircularitySelfReferentialGetter3` and
`compiler/noCircularitySelfReferentialGetter4` are verified. Both are already
fully RIGHT (9/9 and 23/23 type lines), and both are EMPTY_RIGHT diagnostics.
Pinned native checks with `--noEmit --strict --target es2015 --pretty false`
exit 0 with empty output. They are regression controls, not conversions.
The other 41 assigned names are present in the frozen type dump and have
non-RIGHT lines; no causal attribution of those lines to lazy accessors has
been established.

## Native algorithm and publication boundary

Ported counterpart to implement: native `checkObjectLiteral`
(`internal/checker/checker.go`) retains the accessor's member symbol and calls
`checkNodeDeferred`; it does **not** obtain its read type while collecting the
literal. Native `getWidenedProperty` returns a non-Property symbol unchanged,
so accessor return types are not forced or widened again by that operation.
`getTypeOfAccessors` performs annotation precedence (getter, setter,
auto-accessor), then getter-body inference, inside the symbol resolution
frame. `isPropertySymbolTypeRelated` (`internal/checker/relater.go`) reads
member types when the relation requires them. Node-builder
`addPropertyToElementList` (`internal/checker/nodebuilderimpl.go`) obtains the
read/write types and only then decides whether to serialize separate accessor
signatures.

The existing `PropertySlot::Accessor(SymbolId)` belongs to one private TSR
Checker's anonymous member image; its SymbolId refers to that Checker's
borrowed Program binder store, not a printed name or a cross-Program id.
`symbol_types` owns the resolved TypeId for the Checker lifetime. Repeated
property reads must reuse that completion rather than copy a second answer.
Relevant options include `noImplicitAny`/strict and contextual check mode;
setter read/write types and concrete receiver instantiation must remain
separate. A merged computed accessor needs native member-symbol identity,
not just an equal computed-name string.

Publication states:

- Absent: retain symbol without forcing a signature, body, or read/write type.
- Active: resolution-stack entry is provisional, not completed success.
- Completed success: `getTypeOfAccessors` publishes its inferred/annotated type.
- Completed circular failure: native publishes `any` and emits the applicable
  circularity diagnostic, not an absent entry or TSR's current `error` result.
- Unsupported: a missing canonical computed member cannot be certified as
  an ordinary bound accessor; do not silently substitute an eager read.

Preserve declaration origin, literal member order, getter/setter pairing,
setter parameter spelling, diagnostic file/span, and instantiated receiver
context. No new semantic cache or reuse surface is proposed.

The expensive workers are `getTypeOfAccessors` and getter-body/signature
inference, reached by relation, index construction, and serialization reads.
Actual executions, active repeats, completed hits and copy bytes were not
instrumented. Integrator Beads follow-up request: record this bounded work
attribution before extending accessor reuse; use tsr-1yb.11/tsr-1yb.11.1 and
tsr-2zk.11.5 as the owning context. No performance ratio was measured.

## Required serialized integration changes

These are prerequisites, not patches applied by this owner.

1. **`printing.rs`: `certified_object_literal_text_at`,
   `deferred_accessor_text_at`, and the structural member plan.** Current lazy
   branches only replace `Member::Property.printed`; they cannot expand one
   retained symbol to native divergent get/set declarations at serialization.
   Move the read/write comparison and signature rendering out of literal
   construction and into node-builder-equivalent member serialization. Compare
   semantic types as native does, not printed equality. Preserve native
   member order and recursion visitation. `distinct` above proves a necessary
   distinct control. No Diagnostic API cutover is needed for this step.
2. **`symbols.rs`: `get_type_of_accessors` and
   `report_accessor_circularity`.** Native failed pop publishes `any`; TSR
   currently inserts `intrinsics.error`. Port TS7023 for the unannotated getter
   under `noImplicitAny`, full native message and accessor span; retain the
   annotated TS2502 precedence. The current comment explicitly defers TS7023
   because eager object-literal resolution forms false cycles. Apply together
   with unconditional laziness, not ahead of it. `illegal` is the positive
   diagnostic control; `recursive` and the two named corpus cases must remain
   diagnostic-free.
3. **`symbols.rs`: `accessor_write_parameter` and computed accessor resolution;
   canonical member access owner as required.**
   `capture_checked_object_member` currently calls `accessor_write_parameter`,
   which calls `get_signature_from_declaration(setter)` during construction.
   Retain declaration/symbol identity and defer that signature extraction too.
   The current computed accessor path uses declaration-local binder symbols
   and sibling reconstruction; provide a canonical merged symbol access path
   corresponding to native late-bound member preparation before admitting
   computed accessor slots. Do not add string-keyed speculative completion.
4. **Owned `objects.rs`, after those prerequisites:** remove resolution-stack
   and annotation conditions from accessor deferral; publish accessor symbols
   for all supported accessor names, without eager `get_type_of_symbol`,
   `get_signature_from_declaration`, `member_text_at`, or setter-signature
   extraction. The getter/setter constructor branches currently perform
   eager comparison for their baked display. Remove that obsolete producer
   work after the serializer handles it. `object_literal_indexes` may force
   a contributing index member where native does; laziness does not authorize
   skipping semantically required consumers.
5. **Owned `widening.rs`:** retain native non-Property accessor identity without
   forcing it; its existing `reads_on_demand` skip already supports a lazy
   slot. Recheck union sibling/context reads rather than asserting that all
   widening consumers are lazy.

Full coverage was not run: `coverage` unconditionally writes all snapshots,
which this owner's contract forbids touching. No workspace tests, lint/fmt,
after-verdict comparison, or performance acceptance run was performed for
this documentation-only investigation. The integrator must run the strict
full-population oracle and release gates after the atomic cutover. No
unverified acceptance flags or partial checker implementation were committed.

## Contextual binding parameter projection: tsr-2zk.16.63

Subsequent owned root: native `getContextualTypeForBindingElement` first asks
`getContextualTypeForVariableLikeDeclaration` for the holder's type. TSR
previously required a holder annotation even for a contextually typed parameter.
The owned change retains annotation precedence, recursively projects binding
holders, and asks the existing contextual-parameter supplier for a parameter
holder. No RHS/implied-default extension or new cache was shipped.

Native-supported positive control:

```ts
const handler: (value: { cb?: (n: number) => string }) => void =
    ({ cb = n => n.toFixed() }) => {};
```

Pinned native and candidate CLI both exit 0 with empty strict/noEmit diagnostic
output. The real corpus-pipeline type probe changes the default callback from
`error` to `(n: number) => string`, its `n` from `any` to `number`, and
`n.toFixed()` from `any` to `string`. Permanent regression:
`contextual_binding_native_parameter.rs`, observing the callback parameter's
consumer-visible symbol type. Its standalone fixture uses no standard-library
members and expects the same native numeric parameter projection.

Identity/ownership: the query is keyed by the binding element's NodeId in the
borrowed Program AST, and projects the actual holder TypeId from the existing
private Checker parameter supplier. Optional properties are not stripped by
this contextual query. Property spelling only selects a member within that
concrete holder; it does not identify or cache a type globally. Alias and
receiver semantics stay in existing property/element projection. No new
publication table: absence remains no supported contextual type; active and
completed parameter work retain their existing supplier contracts. The worker
is contextual signature/parameter resolution plus member/index projection;
no additional completed-answer cache or reuse domain was added. Counts of
active repeats/actual worker executions remain uninstrumented: integrator
Beads attribution request under tsr-2zk.16.63 before extending this reuse.

Verification after the retained change:

- Specific permanent regression and `cargo test --workspace --release` pass.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo fmt --all -- --check` passes without changing other owner files.
- Both unfiltered verdict dumps complete with identical verdict populations:
  zero previously RIGHT type-line losses, zero RIGHT/EMPTY_RIGHT diagnostic
  losses, and zero newly RIGHT corpus rows. No named corpus case converted.
- All 16 suites complete through the existing read-only `casequery <suite>
  --list` runner. Checker types: 8,042/9,538; diagnostics: 4,221/5,502.
  Empty diagnostic controls remain 4,968/5,068 in the unfiltered dump.
  This uses the same suite implementations without writing forbidden snapshots;
  it does not certify strict full message/length/order/variant parity.

Fresh-process interleaved performance harness, `domain-model` and
`generic-imports`, frozen baseline versus candidate, 21 pairs each:

| Project | Candidate/baseline median child CPU | Observed pair-median wall |
| --- | --- | --- |
| domain-model | 0.99356 | 1.01536 |
| generic-imports | 1.02824 | 1.01873 |

The small wall variation warranted 41 pairs for generic-imports: CPU 1.02063,
wall 1.00763. Diagnostics, options and loaded scopes match and inputs stayed
unchanged. No CPU regression above the protocol's 1.03 threshold; wall
observations do not prove absolute no-slowdown. Candidate binary SHA-256:
`144a19598f7ddf8577a95da1d4b7ed9f64ce4b58707131a6be2c2eb6cb1a7069`.
Harness source SHA labels the then-current HEAD `b9c48897`; the measured binary
includes the explicitly described uncommitted contextual projection and test,
with no other implementation changes. Binary hash, not that HEAD label,
identifies the measured candidate.

Pinned native comparisons (21 pairs): observed wall ratios 1.02008 for
domain-model and 0.90752 for generic-imports. **Not verified ratios:** every
harness report explicitly has `work_comparable: false`,
`complete_input_equivalence_verified: false`,
`actual_checked_work_verified: false`, `verified_wall_ratio: null`, and
`target_verified: false`. Loaded scope/options/diagnostics match, but query
inputs, bundled-library bytes and actual checked work are not fully observed.
The 0.50 release target is not met or certified.

### Remaining atomic prerequisites

The Box `.beads/issues.jsonl` is a stale passive export, not an issue-existence
oracle. The integration owner supplied authoritative issue tsr-2zk.16.63 and
its current 19-case list later in this session; the earlier inference that
missing export evidence meant no authoritative issue evidence is withdrawn.
No historical conversion count was substituted.

Two attempted broader native paths were rejected after actual smoke aborted
with stack overflow; they are not present in the committed implementation:

1. Initializer RHS fallback plus implied-pattern context. Native
   `getTypeFromBindingElement` checks a simple default under explicit
   `unknownType` (a nested pattern under its implied type) before checking the
   holder initializer. Existing TSR `binding_element_implied_type` checks only
   context-independent defaults and has no explicit-context expression API.
   Required external hunk: `expressions.rs`/Checker context owner must provide
   native `checkExpressionWithContextualType` and
   `checkDeclarationInitializer` semantics, including cached normal/rest
   modes and parameter padding. Then owned initializer dispatch can apply
   native declaration-result-before-implied-pattern priority without recursive
   RHS/default re-entry. Do not implement active-query nil suppression.
2. Direct parameter-initializer contextual dispatch. The current
   `get_contextually_typed_parameter_type` also performs initializer widening,
   unlike native's raw `getContextuallyTypedParameterType` supplier. Asking it
   for its own initializer's context recursively checks that initializer.
   Required coordinated hunk: separate native raw contextual lookup from
   `symbols.rs` parameter-widening consumer (`getWidenedTypeForVariableLikeDeclaration`),
   then route initializer context to the raw supplier. This caller migration
   is outside ownership; do not create a compatibility shim.

Distinct native negative control:

```ts
const { cb = (n) => n.toFixed() } = { cb: (n: number) => n.toFixed() };
const [first = (n) => n.toFixed()] = [(n: number) => n.toFixed()];
```

Pinned native emits TS7006 for both unannotated defaults because implied types
are context-independent. Existing TSR still omits these diagnostics; the
retained contextual-parameter projection does not claim to fix that separate
explicit-context/implicit-any path. Full 19-case initializer-root completion
remains blocked on those contracts and authoritative case evidence.

### Array-rest contextual projection, same binding cluster

Native `getContextualTypeForBindingElement` does not reject array rest when
an annotation/context already supplies the holder. It projects the element
at the binding index even if the rest initializer itself is invalid. Removed
TSR's unconditional array-rest admission restriction; no initializer fallback
or declaration side pass was added. Same identity/publication/work boundary
as the contextual holder projection above.

Direct pinned-native control:

```ts
declare const input: [(n: number) => number, ...((n: number) => number)[]];
let [...rest = n => n]: [(n: number) => number, ...((n: number) => number)[]] = input;
```

Native reports TS2322 at `(2,9)` and TS1186 at `(2,14)` for the source shown
without the scratch file's leading strict directive. Observed scratch output
has `(3,9)` and `(3,14)`. The complete observed messages are:

```text
../../tmp/box/binding-rest.ts(3,9): error TS2322: Type '(n: number) => number' is not assignable to type '[(n: number) => number, ...((n: number) => number)[]]'.
../../tmp/box/binding-rest.ts(3,14): error TS1186: A rest element cannot have an initializer.
```

Actual candidate corpus-pipeline smoke now prints `n => n : (n: number) =>
number` and both `n` references as `number`; before, the callback was `error`
and both references were `any`. This is contextual typing parity, not a claim
that this port owns those diagnostics. Added a rest-default regression beside
the parameter-holder regression. Removed the obsolete unit assertion that
pinned the deliberately unsupported rest-context implementation instead of
native behavior.

Post-change verification: workspace release tests, clippy, fmt check and all
16 read-only parity suites complete. Both unfiltered dumps complete; explicit
missing-key-aware comparison yields zero missing formerly RIGHT keys, zero
RIGHT type-line losses and zero RIGHT/EMPTY_RIGHT diagnostic losses. No corpus
conversions; coverage counts unchanged.

The initial rest performance invocation accidentally used the previous CLI
binary (its hash remained `144a1959...` despite rebuilding examples); those
samples are **not** rest-port performance evidence. Rebuilt the actual binary
with `cargo build --release -p tsr --bin tsr`, SHA-256
`d734b75f89d062fdcd6805276da78db86ad851bdf5c056cf7027c1ae9c84f784`,
and reran:

| Project | Candidate/baseline CPU, 41 pairs | Observed baseline wall | Observed native wall, 21 pairs |
| --- | --- | --- | --- |
| domain-model | 0.97904 | 0.99414 | 1.04770 |
| generic-imports | 1.00620 | 1.00088 | 0.91934 |

Diagnostics/options/loaded scope match and captured inputs are stable. No CPU
regression above 1.03. No verified native ratio: complete-input equivalence
and actual checker work remain false, verified ratio null, target false.
Candidate was measured on the isolated Box with only this owned rest
projection/test change beyond `c4b69044`; the binary hash identifies it.

Exact default/circular-parameter integration prerequisite: current
`symbols.rs::get_type_for_variable_like_declaration` calls
`get_contextually_typed_parameter_type` in its parameter branch (line 6169
at the measured tree); it must consume raw native
`getContextuallyTypedParameterType`, then apply optionality as native
`getTypeForVariableLikeDeclaration` does. Current owned supplier also widens
initializers. Native does that in `assignContextualParameterTypes`, requiring
coordination with the signatures consumer owner. The symbol entry worker is
currently named `get_type_of_variable_or_parameter_or_property_worker`, not
`get_type_of_variable_parameter_property_worker`. Do not patch around that
consumer using a second declaration traversal. Initializer/implied-pattern
completion remains dependent on the explicit-context API and authoritative
19-case evidence described below. Atomic accessor `.11.5.1` and lazy signature
`.9.7.1` remain queued for their single owner; `.11.5` is still open.

### Parenthesized initializer context: verified target conversion

Authoritative integration target list for tsr-2zk.16.63:

- compiler/classExpressionNames
- compiler/declarationEmitDestructuring2
- compiler/declarationEmitDestructuring3
- compiler/declarationEmitDestructuringArrayPattern2
- compiler/declarationEmitDestructuringArrayPattern4
- compiler/intraBindingPatternReferences
- compiler/objectBindingPatternContextuallyTypesArgument
- conformance/contextuallyTypedIife
- conformance/contextuallyTypedIifeStrict
- conformance/declarationsAndAssignments
- conformance/destructuringArrayBindingPatternAndAssignment1ES5
- conformance/destructuringArrayBindingPatternAndAssignment1ES5iterable
- conformance/destructuringArrayBindingPatternAndAssignment1ES6
- conformance/destructuringVariableDeclaration1ES5
- conformance/destructuringVariableDeclaration1ES6
- conformance/destructuringVariableDeclaration2
- conformance/literalTypes2
- conformance/literalTypesAndTypeAssertions
- conformance/restElementMustBeLast

Blocked scope is not a conversion promise. Current dump inspection identifies
one self-contained initializer-context producer defect: the owned
`objects.rs::contextual_binding_pattern` stopped at a parenthesized expression.
Native `getContextualType` transparently follows parentheses before
`getContextualTypeForInitializerExpression` applies declaration context and
then implied-pattern context. The producer now follows that same transparent
parent edge; annotation-first and parameter-context admission stay unchanged.

Actual before smoke:

```ts
const { B = class {} } = ({ B: undefined });
const { x = 1 } = (({ x: 2 }));
const { y = 1 }: { y: number } = ({ y: 2 });
```

Before: both B initializer lines printed `{ B: undefined; }` and the nested
parenthesized x initializer printed `{ x: number; }`. After: `{ B?: undefined;
}` and `{ x?: number; }`; the annotated y initializer remains `{ y: number;
}`. Pinned native `classExpressionNames.types` prints exactly the optional B
forms. Native checks the actual corpus source with `--noEmit --target es2015
--noImplicitAny --pretty false`, exit 0 and no diagnostics. Target checker
suite now passes **30/30 lines**, previously 28/30. The empty diagnostic
verdict remains EMPTY_RIGHT. Added permanent nested-parentheses and annotation
precedence controls in `contextual_binding_native_parentheses.rs`.

Boundary: this is an AST parent traversal in the existing private Checker
producer, keyed by actual NodeId/parent identity, not syntax text. It borrows
the same Program-owned BindingPattern and changes no TypeId interning or
symbol publication. Absence remains no supported pattern context; the
traversal does not publish active work as completion, check defaults, or
force types. Alias/receiver/member diagnostic behavior stays in the existing
literal/property consumers. Each transparent parent adds one existing AST
lookup; no duplicate cache, member image or reuse domain is introduced.
Actual query counts remain uninstrumented; attribution follow-up request
under tsr-2zk.16.63 before broadening producer reuse.

Completed verification:

- Both unfiltered verdict dumps complete: **469,767/477,970 RIGHT type lines**
  (gain 2); zero missing formerly RIGHT keys and zero formerly RIGHT losses.
  Diagnostic verdicts unchanged, including all formerly RIGHT/EMPTY_RIGHT.
- All 16 read-only parity suites complete: checker types **8043/9538**, up one;
  diagnostics **4221/5502** and empty controls **4968/5068**, unchanged.
- Workspace release tests, clippy all-targets `-D warnings`, fmt check pass.
- Actual corpus-pipeline smoke, native corpus check and target 30/30 exercised.

Fresh candidate CLI SHA-256:
`6fb828c4bc42c3d4d8e7c10411bd9a0dc05427fc9f332283f289213f949d2a0a`.
Frozen-baseline interleaved fresh-process performance, 41 pairs; native, 21:

| Project | Baseline CPU ratio | Baseline observed wall | Native observed wall |
| --- | --- | --- | --- |
| domain-model | 0.95291 | 0.99720 | 1.02123 |
| generic-imports | 1.00309 | 0.99706 | 0.93470 |

No measured CPU slowdown beyond 1.03. Diagnostics/options/loaded scope match
and observed inputs stay stable. Native complete-input and actual-work
verification remain false, verified ratio null, target false. These are
observations, not a verified release ratio. Harness source label is
`2f3038f3`; the measured binary includes only this isolated owned producer
change beyond that commit and is identified by its hash.

Remaining 18 named targets still have failed lines. Inspected examples include
nested/default array contexts, pattern boolean/literal widening, contextual
IIFE defaults, and intra-pattern function defaults. They are not attributed
to this parentheses fix. The explicit-context and raw parameter supplier
prerequisites above remain necessary for the broader initializer cutover;
no declaration side pass, heuristic guard, or already-RIGHT conversion claim.

## LITERAL-CONTEXTUAL-INSTANTIABLE: native reproduction and prerequisite

Investigation after `70f524fc`; no implementation change or target conversion.
Authoritative issue **tsr-2zk.16.425**, targets
`compiler/jsxExcessPropsAndAssignability` and
`compiler/typePredicateFreshLiteralWidening`, supplied by integration after
the initial ad hoc investigation. The literal-constraint hypothesis was
tested directly, not inferred from old triage counts.

```ts
declare function constrained<T extends string>(value: { item: T }): T;
declare function nested<T extends string>(value: { inner: { item: T } }): T;
declare function numeric<T extends number>(value: { item: T }): T;
declare function unconstrained<T>(value: { item: T }): T;
const a = constrained({ item: "value" });
const b = nested({ inner: { item: "value" } });
const c = numeric({ item: 42 });
const d = unconstrained({ item: "value" });
```

Pinned native strict declaration-only emit exits 0 and emits `a: "value"`,
`b: "value"`, `c: 42`, `d: string`. Current TSR real corpus-pipeline probe
agrees on a/c/d but prints `b: string`; the nested inner object is
`{ item: string; }`. A second direct control uses nested
`T extends string ? T : never` context and nested numeric T: native emits
`"value"` and `42`, TSR emits `string` and `number`. A flat boolean-constrained
T already agrees (`true`). Plain instantiated `string` context preserves a
literal in the initializer's contextual check but the call return remains
string, as native does; that distinction must not become a primitive-string
heuristic.

Native `isLiteralOfContextualType` (`internal/checker/checker.go`) already has
an equivalent implemented arm in **unowned**
`signatures.rs::is_literal_of_contextual_type`: union/intersection recursion,
instantiable base constraint, matching string/number/bigint/symbol primitive
constraint, and recursive literal context. No missing arm was established.
Owned `objects.rs::check_expression_for_mutable_location` bypasses that query
for nested object literals inside call arguments because contextual signature
resolution is not re-entry-safe; the file records historical RIGHT losses
from indiscriminately removing that exclusion. This investigation did not
remove it or reproduce historical counts as current gains/losses.

Pinned native `inferTypeArguments` supplies each argument's actual paramType
and active inference context through `checkExpressionWithContextualType`
before checking the literal. The existing private Checker
`active_inference_contexts[call].signature` is the raw, non-reentrant identity
supplier; its writer is unowned
`inference.rs::check_generic_call_with_mode`, around
`check_generic_call_worker`. Owned
`contextual.rs::contextual_type_for_argument` consults that snapshot for raw
queries, but falls back to stateless signature resolution when no active
snapshot exists. A stateless raw retry can still re-enter the call and return
provisional any, so it is not a faithful universal replacement.

Required single-owner inference/signature integration prerequisite: preserve
the selected signature and actual native contextual checking mode/context
through the nested argument check, with a non-fixing contextual mapper and
safe deferred context-sensitive member work. Then remove the obsolete nested
literal exclusion and raw retry in the owned mutable-location consumer as
part of that atomic cutover. No duplicate snapshot table, readonly/member
partial metadata, primitive constraint shortcut, declaration side pass, or
fallback signature resolution is proposed.

Identity/publication: call NodeId, selected signature/type-parameter identity,
ordered inference slots, check mode and fixing/non-fixing mapper belong to the
same private Checker. The active snapshot is provisional context, not a
completed inferred signature. Completion remains in the inference/call owner;
unsupported context must not be treated as successful any. Nested receiver,
alias origin and diagnostic policy remain those of the actual parameter
projection. Expensive work is contextual signature resolution plus argument
checking/inference; no new reuse introduced. Counts are unmeasured: bounded
Beads attribution follow-up request before extending raw snapshot reuse.

Verification for this investigation consists only of the two pinned-native
ad hoc declaration emits and TSR corpus-pipeline type probes. No changed
implementation means no new after-verdict/performance acceptance claim. The
last verified implementation remains `70f524fc` and its receipt above.

### Authoritative .16.425 target evidence

Both current target source names were verified in the corpus. No files outside
ownership were edited; a temporary `/.lib/react16.d.ts` mount copied the
existing corpus library solely for the native JSX invocation.

`typePredicateFreshLiteralWidening` uses a conditional plus recursive
homomorphic mapped alias `Narrow<A>`, applied by a curried generic function.
Pinned native checks the actual source with `--noEmit --target es2015
--strictNullChecks --pretty false`, exit 0 and no diagnostics. Declaration-only
emit independently confirms:

```ts
declare const item1: { value: "1"; };
declare const item2: { value: "2"; };
declare const item3: { value: null; };
declare const values1: ("1" | "2" | null)[];
declare const filteredValues1: ("1" | "2")[];
```

Frozen/current aligned TSR verdicts instead include
`Narrow<{ value: string; }>` for item1/item2, `{ value: string; }` for their
argument literals, and mapped alias values in the downstream map/filter
results. Current real corpus-pipeline ad hoc type probe reproduces those
widened/alias results. Diagnostic case remains EMPTY_RIGHT.

Exact unowned producer boundaries: `mapped.rs::generic_mapped_contextual_property_type`
and its template instantiation must supply the actual conditional/indexed
member type for the key, preserving the inference context; alias/type image
construction and inference must then evaluate the returned mapped image
rather than leave `Narrow<...>` presentation as a semantic substitute.
`signatures.rs::is_literal_of_contextual_type` already consumes the resulting
base constraint; `constraints.rs::base_constraint_of_type` owns that
resolution's binding-qualified completion. No evidence justifies a second
literal-constraint evaluator in owned contextual/objects/widening files.
Route the canonical mapped/constraint/inference producer under a single owner
before asking this lane to extend mutable-location preservation.

`jsxExcessPropsAndAssignability` native source check with `--noEmit --module
commonjs --target es2015 --jsx react --strict --pretty false`, using the exact
react16 library, reports:

```text
jsxExcessPropsAndAssignability.tsx(17,27): error TS2698: Spread types may only be created from object types.
jsxExcessPropsAndAssignability.tsx(18,6): error TS2322: Type 'ComposedComponentProps & { myProp: number; }' is not assignable to type 'IntrinsicAttributes & IntrinsicClassAttributes<Component<WrapperComponentProps, any, any>> & Readonly<...> & Readonly<...>'.
  Type 'ComposedComponentProps & { myProp: number; }' is not assignable to type 'Readonly<WrapperComponentProps>'.
jsxExcessPropsAndAssignability.tsx(18,27): error TS2698: Spread types may only be created from object types.
```

Locations above are actual unsplit corpus-source lines; the directive-split
baseline uses lines 13/14. Observed native output prefixes the full repository
relative path. TSR aligned case emits no diagnostics. Its type verdicts also
retain `extends any` instead of native `extends unknown` and omit the default
`any` argument in `React.ComponentClass`. Those are constraint/reference
producer prerequisites, not owned literal-preservation patches.

The aligned corpus literal line is WRONG (`"1000000"` expected, `string`
actual). However, the current ad hoc corpus-pipeline `probefile` walker prints
`'1000000' : "1000000"` already. Do not claim that isolated probe reproduces
this aligned failure, or fix it by a string heuristic. Exact unowned JSX
mutable-location consumer: `jsx_intrinsic.rs::jsx_inference_expression`
(calls `check_expression_for_mutable_location`) and JSX attributes/component
relation diagnostics. Pinned native counterparts are `checkJsxAttribute`
and `getContextualTypeForJsxExpression` in `internal/checker/jsx.go`. The JSX
owner must reproduce aligned query order with its actual instantiable props
context before any owned mutable-location cutover.

No target conversions, no checker change, no post-change loss or performance
claim for .16.425. The two cases are blocked scope, not a promise. Prior
`70f524fc` parity/performance receipts remain the last implemented root at
that investigation's publication.

## GETTER-RETURN-ANNOTATION-CONTEXT

Owned port: `contextual.rs::get_contextual_return_type` now consumes native
`getReturnTypeFromAnnotation`'s getter annotation, falling back to the bound
paired setter's first parameter annotation. The getter's own annotation
wins. No accessor type/signature/body query is added. Authoritative issue **tsr-2zk.16.417**, targets
`conformance/contextualTypeFromJSDoc` and
`conformance/objectLiteralGettersAndSetters`, supplied after the initial
annotation port. The measured case below is now a confirmed target.

Pinned native direct strict declaration-only emit exits 0 for:

```ts
const annotated = { get value(): { tag: "a" | "b" } { return { tag: "a" }; } };
const paired = { get value() { return { tag: "a" }; }, set value(v: { tag: "a" | "b" }) {} };
const divergent = { get value(): { tag: "a" } { return { tag: "a" }; }, set value(v: { tag: "b" }) {} };
const unannotated = { get value() { return { tag: "a" }; } };
class Holder { get value() { return { tag: "a" }; } set value(v: { tag: "a" | "b" }) {} }
```

TSR before: every getter's returned object and tag widen to `{ tag: string;
}`/string. After: annotated, paired, divergent and class-paired returns retain
`{ tag: "a"; }`/`"a"`; unannotated getter remains string. Native corpus
baseline `objectLiteralGettersAndSetters.types` also provides setter-first and
getter-first callback returns whose parameter is string, not any. The port
converts six aligned type lines in those controls. Permanent tests
`contextual_getter_native_annotation.rs` cover getter-before-divergent-setter,
class paired-setter fallback, and no annotation. No syntactic test-name branch
or literal heuristic exists.

Identity/owner/publication: function NodeId and its bound Program SymbolId
select the actual shared accessor declarations. The borrowed declaration type
node is resolved by the existing private Checker type-node supplier; no new
cache or accessor answer is published. Absence of annotations remains absent
context, not a body inference request. Existing active/completed type-node
publication remains its supplier's responsibility. Setter fallback does not
mint a member image or collapse getter/read and setter/write types. Distinct
static/instance symbols, declaration origin and annotation order remain those
of the bound symbol. Canonical private/late-bound pairing remains with the
accessor owner; no string-keyed sibling pairing added here.

Expensive work: at most the existing accessor declaration scan and annotation
resolution, then existing contextual return/member checking. No eager getter
body or signature work introduced. Actual scan/type-node worker counts remain
unmeasured; request bounded Beads attribution before expanding this reuse.

Verification: workspace release tests, clippy all-targets -D warnings, fmt
check, actual type-probe smoke, and all 16 read-only parity suites complete.
Against the originally frozen unfiltered dump: **469773/477970 RIGHT lines**
(gain 8 total: prior parentheses 2 plus getter 6), zero missing prior-RIGHT
keys and zero RIGHT type-line losses. Diagnostic RIGHT/EMPTY_RIGHT case
verdicts unchanged, with no missing keys. Case coverage unchanged at
**8043/9538**, diagnostics **4221/5502**, clean **4968/5068**.
`conformance/objectLiteralGettersAndSetters` is **165/179 lines**, not passing;
its duplicate/accessor construction prerequisites are not this context port.
No fully converted getter corpus case claimed.

Candidate CLI SHA-256:
`8e086daf05d1cc994cb5559bd03970c026f0c392aac2b1547d1a7b8261eac5b0`.
Fresh-process interleaved frozen-baseline performance, 41 pairs; native, 21:

| Project | Baseline CPU | Baseline observed wall | Native observed wall |
| --- | --- | --- | --- |
| domain-model | 0.98489 | 0.98908 | 1.02558 |
| generic-imports | 1.01056 | 1.00785 | 0.90638 |

No CPU regression above 1.03; wall observations do not prove absolute
no-slowdown. Diagnostics/options/loaded scopes match, captured inputs stable.
Complete-input and actual checked-work verification remain false; verified
ratio null and target false. No verified release ratio claimed. Harness
source label is `968b02d7`; actual measured implementation is the isolated
owned getter-context change beyond that commit, identified by binary hash.
Target-case acceptance remained pending the queue at this measurement;
see the authoritative JSDoc continuation below. Literal instantiable
`.16.425` remains open/investigation-only.

### .16.417 JSDoc annotation continuation

Native reparsing exposes JSDoc return and setter parameter annotations through
`getReturnTypeFromAnnotation`/`getAnnotatedAccessorType`. TSR stores those
nodes separately. Owned contextual return lookup now reuses existing raw
`jsdoc_return_annotation` and `jsdoc_parameter_annotation`, preserving the
written getter annotation and paired setter parameter precedence. It never
calls `getTypeOfAccessors`, signature construction, or body checking to obtain
this context. Existing private Checker JSDoc host metadata owns the annotation
node; no new cache or member image. Same annotation/publication/receiver/work
boundary as the getter port above, plus the existing JSDoc host lookup.

Pinned-native JS strictness-independent control, allowJs/checkJs declaration
emit, exits 0:

```js
/** @return {{ tag: 'a' | 'b' }} */
function f() { return { tag: 'a' }; }
class C {
  /** @param {{ tag: 'a' | 'b' }} value */
  set value(value) {}
  get value() { return { tag: 'a' }; }
}
```

Both raw annotations contextualize returned tag literals; permanent tests
register actual parser/binder JSDoc metadata and distinguish unannotated and
divergent-annotation controls. Five annotation tests pass. The actual
`contextualTypeFromJSDoc` corpus-pipeline smoke now preserves its returned
nested tuple contexts, gaining **6 lines**, **31/39 -> 37/39**. The previous
getter port gained 6 lines in `objectLiteralGettersAndSetters`, now 165/179.
Neither target is fully converted. Two further lines improve in
`instantiateTemplateTagTypeParameterOnVariableStatement`.

Exact remaining unowned prerequisite for the JSDoc target:
`symbols.rs::get_type_of_accessors_worker` obtains setter annotations through
`accessor_annotation`, which does not supply this setter's JSDoc @param type.
Native `getAnnotatedAccessorTypeNode` uses
`getEffectiveSetAccessorTypeAnnotationNode`. Port that raw JSDoc annotation
source in the canonical accessor producer; do not infer the setter contract
from a correctly contextually checked getter body. The two remaining x symbol
lines must have `[string, { x?: number; y?: number; }][]`, not the union of
getter-body tuple shapes. This lane does not edit symbols.rs. Remaining
object-literal target duplicate/accessor-image construction failures stay with
the queued atomic accessor owner, not a context side pass.

Verification after the JSDoc continuation: both unfiltered dumps, all 16
read-only parity suites, workspace release tests, clippy all-targets -D
warnings and fmt check complete. Zero missing formerly RIGHT keys, zero RIGHT
type-line losses, zero RIGHT/EMPTY_RIGHT diagnostic losses. RIGHT lines are
**469781/477970** (2 parentheses + 6 getter + 8 JSDoc gains over frozen base).
Case coverage remains **8043/9538**, diagnostics **4221/5502**, clean
**4968/5068**. No fully converted .16.417 target claimed.

Candidate CLI SHA-256:
`0330f4ff16411865a9881761f267fb6ab65eef60f66232b478b08bba772a5970`.
Fresh-process interleaved baseline 41 pairs and native 21 pairs:

| Project | Baseline CPU | Baseline observed wall | Native observed wall |
| --- | --- | --- | --- |
| domain-model | 0.99248 | 0.99790 | 1.04127 |
| generic-imports | 1.00282 | 0.99860 | 0.92881 |

No measured CPU regression above 1.03. Scope/options/diagnostics match and
captured inputs stable. Complete-input and actual-work proof remain false,
verified ratio null, target false; no verified 0.50 claim. Harness HEAD label
is `5c801fff`; isolated owned JSDoc contextual changes are identified by the
candidate binary hash. .16.417 remains open pending canonical accessor
producer and target completion.

## Generator contextual-return undefined controls

Investigation after `8126dbbd`; no generator implementation change. Read
pinned `getContextualReturnType`, `getContextualIterationType`,
`getContextualTypeForYieldOperand` and owned return/yield projection before
probing. Native accepts these actual esnext/strict controls with no errors:

```ts
function* plain(): Generator<undefined, undefined, unknown> {
    yield undefined; return undefined;
}
function* literals(): Generator<{ tag: "a" | "b" }, undefined, unknown> {
    yield { tag: "a" }; return undefined;
}
async function* asyncLiterals(): AsyncGenerator<{ tag: "a" | "b" }, undefined, unknown> {
    yield { tag: "a" }; return undefined;
}
function* delegated(): Generator<undefined, undefined, unknown> {
    return yield* plain();
}
```

Current TSR real corpus-pipeline probe agrees on all observed slots:
undefined operands/return, literal `{ tag: "a"; }` in synchronous and async
yields, unknown next slots, and undefined yield-star result. This is positive
control evidence, not a conversion. No owned context root is established.

Negative control (with two scratch directive lines preceding it):

```ts
function* invalid(): Generator<number, undefined, unknown> { yield undefined; return 1; }
function* absent(): Generator<number, undefined, unknown> { yield; }
async function* asyncInvalid(): AsyncGenerator<number, undefined, unknown> { yield undefined; return 1; }
```

Pinned native emits five TS2322 diagnostics, in order:

- (3,68) Type 'undefined' is not assignable to type 'number'.
- (3,79) Type '1' is not assignable to type 'undefined'.
- (4,61) Type 'undefined' is not assignable to type 'number'.
- (5,84) Type 'undefined' is not assignable to type 'number'.
- (5,95) Type '1' is not assignable to type 'undefined'.

TSR CLI emits only the synchronous yield diagnostics at (3,68), (4,61).
Exact consumer handoff: unowned
`assignreport.rs::check_yield_expression_assignability` and its
`check.rs` dispatch must check async yield operands against the actual
annotation yield slot; return assignability consumer in `check.rs` must
check generator return expressions against the annotation return iteration
slot. Native counterparts `checkYieldExpression` and `checkReturnStatement`
use these actual slots; do not change correct contextual projection or insert
a second pass. Body-return construction stays with its signature owner.

No new cache/traversal/member image, no wrapper function, no test or
performance claim for this investigation. `.16.417` remains open at its
canonical JSDoc/accessor producer boundary; `.16.425` remains open. Last
implementation and full-gate receipt is `8126dbbd` above.
