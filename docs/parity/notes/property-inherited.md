# Inherited readonly property boundary — tsr-2zk.4.12

## Decision

No semantic cutover is published. The current split between inherited property
values and declaration metadata is reproduced, but exposing resolved base symbols
fails the mandatory no-loss gate. Integrator prerequisite: coordinate contextual
member reads and circular interface conformance before publishing that cutover.
The original unresolved readonly/member cutover prerequisite is
`tsr-2zk.4.12.1`; protected concrete derived receiver/value-alias access is
tracked separately as `tsr-2zk.4.14`.
No case-name logic, suppression, additional cache or compatibility shim was added.

## Pinned reproduction

Native: `vendor/typescript-go` commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, built with Go 1.26.8 using the
existing disposable offline bootstrap. TSR baseline is the initial Box checkout
of integration commit `5dd3bad84d12991e1ba169d2d5687321e1989740`.
The release TSR binary and unfiltered diagnostic/type verdict dumps were frozen
before edits. Actual dump population: 10,570 diagnostic source cases; 477,970
aligned type assertions. The type TSV also contains six non-row continuation
lines; these are not assertions. These are not strict full-message/order/length
or complete-input equivalence oracles.

Native operations read: `Checker.resolveObjectTypeMembers`,
`Checker.addInheritedMembers`, `Checker.getPropertyOfTypeEx`,
`Checker.isAssignmentToReadonlyEntity`, `Checker.checkPropertyAccessibilityAtLocation`,
`Checker.forEachProperty`, and `Checker.hasBaseType`
(`internal/checker/checker.go`). Inherited members retain their base declaration
identity, but their values use the instantiated base's mapper and the concrete
receiver's polymorphic `this`. A derived own value declaration shadows a base.

Ad hoc control (both CLIs: `--noEmit --strict --pretty false`):

```ts
class Base<T> { readonly value!: T; mutable!: T; }
class Derived extends Base<number> {
  constructor() { super(); this.value = 1; }
}
declare let d: Derived;
d.value = 1;
d.mutable = 1;
class Shadow extends Base<number> { value = 0; }
declare let s: Shadow;
s.value = 1;
class Plain { readonly value = 0; constructor() { this.value = 0; } }
class Nested extends Derived {}
declare let n: Nested;
n.value = 1;
class StaticBase<T> { static readonly value = 0; }
class StaticDerived extends StaticBase<number> {}
StaticDerived.value = 1;
```

Native reports TS2540 at (3,33), (6,3), (14,3), (17,15), each with complete
message `Cannot assign to 'value' because it is a read-only property.` Frozen TSR
reports only (17,15). The rejected candidate reports all four at identical
locations with identical CLI text and order. The own constructor, writable
member, and own shadow controls remain legal. This disproves a spelling-based
readonly solution: the base symbol and constructor declaration identity matter.

## Rejected cutover and unfiltered evidence

The experiment in `members.rs` consulted existing `get_base_types` when the
symbol-only heritage walk declined; inherited values read through the resolved
base and then the receiver mapper. It removed the syntax-only generic value
walk. It did not introduce another cache. It was reverted after the complete
unfiltered dumps finished.

| Population | Frozen | Rejected candidate |
| --- | ---: | ---: |
| RIGHT aligned type assertions | 469,765 / 477,970 | 469,834 / 477,970 |
| GAP assertions | 993 | 971 |
| WRONG assertions | 7,212 | 7,165 |
| RIGHT diagnostic cases | 4,221 / 10,570 | 4,221 / 10,570 |
| EMPTY_RIGHT diagnostic cases | 4,968 / 10,570 | 4,968 / 10,570 |

Aggregate totals hide regressions: **28 formerly RIGHT type lines and 3 formerly
RIGHT/EMPTY_RIGHT diagnostic cases lost**. No gains below are shipped.

The absence-aware receipt additionally walks **every protected before key**,
not merely the intersection produced by `join`: type keys are stable
`case:section:row`, diagnostics use the source case ID. Frozen/candidate maps
contain respectively 477,970/477,970 distinct type keys and 10,570/10,570
distinct diagnostic keys, with zero duplicate keys. Of 469,765 prior RIGHT type
keys, zero vanished and 28 changed verdict (28 total losses). Of 9,189 prior
RIGHT/EMPTY_RIGHT diagnostic keys, zero vanished and three changed verdict
(three total losses). Missing after keys are explicitly classified as losses.
The six type continuation lines are excluded from key parsing on both sides,
not counted as keys. These are receipts for the rejected experiment only;
the final committed tree has no semantic cutover.

| Assigned exact source case | Frozen RIGHT type lines | Candidate RIGHT type lines | Candidate diagnostic verdict |
| --- | ---: | ---: | --- |
| compiler/declarationEmitExpressionInExtends | 8 / 10 | 10 / 10 | RIGHT |
| compiler/genericDefaults | 1191 / 1230 | 1194 / 1230 | WRONG |
| conformance/classExpression3 | 18 / 22 | 22 / 22 | EMPTY_RIGHT |
| conformance/classExpressionES63 | 18 / 22 | 22 / 22 | EMPTY_RIGHT |
| conformance/classExtendingClassLikeType | 63 / 75 | 75 / 75 | WRONG |
| conformance/contextualThisType | 9 / 13 | 9 / 13 | EMPTY_WRONG |
| conformance/importCallExpression3ES2020 | 17 / 20 | 20 / 20 | EMPTY_RIGHT |
| conformance/importCallExpressionInCJS4 | 17 / 20 | 20 / 20 | EMPTY_RIGHT |
| conformance/instancePropertiesInheritedIntoClassType | 76 / 83 | 83 / 83 | RIGHT, formerly WRONG |
| conformance/mixinAccessModifiers | 164 / 198 | 182 / 198 | WRONG |
| conformance/mixinClassesMembers | 182 / 194 | 194 / 194 | EMPTY_WRONG, formerly EMPTY_RIGHT |

## Complete rejected transition inventory

Types: WRONG→RIGHT 83, GAP→RIGHT 14, RIGHT→WRONG 28, GAP→WRONG 8.
New wrong GAP rows are `conformance/intersectionThisTypes:0:` rows
29,31,32,35,37,38,40,42: native `Thing5` or `() => Thing5` becomes
`Thing4` or `() => Thing4`. Six previously WRONG rows also change in that case
(34,39,41,44,45,46), previously `any`, now the same wrong base receiver.
Native `resolveObjectTypeMembers` / `getTypeWithThisArgument` must propagate
concrete `Thing5` through intersection constituents, not let the structural base
image retain `Thing4` presentation. This is an owned member-image prerequisite.
Two previously WRONG rows in `compiler/thislessFunctionsNotContextSensitive3`
(44,73) change from `{ editor: Editor; char?: string | undefined; }` to `any`.

Diagnostics: RIGHT→WRONG 2, EMPTY_RIGHT→EMPTY_WRONG 1, WRONG→RIGHT 2,
EMPTY_WRONG→EMPTY_RIGHT 1. Changed-but-still-wrong cases:
`compiler/genericDefaults` removes extraneous TS2415 (464,15) but still lacks
all eleven expected TS2744; `compiler/thislessFunctionsNotContextSensitive3`
loses expected TS2783 (62,9), (81,9) while retaining extraneous TS2352;
`conformance/contextualThisType` replaces extraneous TS2430 (5,11) with
extraneous TS2322 (9,5); `conformance/mixinAccessModifiers` adds six new
extraneous TS2445 (71,6), (72,6), (84,6), (85,6), (97,6), (98,6).
No improvements justify these new wrong results.

## Required coordinated work

- **Outside ownership: `contextual.rs`,
  `contextual_type_for_object_literal_named_element`** (the branch around the
  `get_property_of_type(contextual, name)` call). It reads an inherited root via
  `get_type_of_symbol` and subsequently applies only the receiver mapper. Read
  the semantic instantiated member through `get_type_of_property_of_type`,
  preserving the base mapper and concrete receiver. Pinned counterpart:
  `getTypeOfPropertyOfContextualTypeEx`. Candidate loses eight RIGHT lines in
  `compiler/temporal` (6171, 6172, 6178, 6179, 6286, 6287, 6312, 6313):
  `{ smallestUnit: "minute"; }` widens to `{ smallestUnit: string; }`, and adds
  four TS2769 diagnostics (1707,15; 1708,15; 1741,9; 1749,9).
- **Outside ownership: `inference.rs`, `infer_from_members` and contextual
  consumers of property roots.** Audit inherited semantic member types rather
  than root declaration types, retaining native inference fixing context.
  `compiler/thislessFunctionsNotContextSensitive3` loses 18 RIGHT lines
  (26,30,40,47,49,50,51,52,53,54,58,69,76,78,79,80,81,82).
  Example: `Extension<{ suggestion: SuggestionOptions; }>` becomes
  `Extension<Options>`. Attribution to the exact inference branch remains
  unproven; do not treat this audit request as a verified fix.
- **Outside ownership: `heritage_conformance.rs`,
  `check_interface_heritage_conformance`.** Preserve native failed/active base
  resolution policy before running structural conformance. Candidate adds
  TS2430 at (14,15) and (18,15) to
  `conformance/interfaceThatIndirectlyInheritsFromItself`, formerly RIGHT.
  Direct pinned CLI control reports six TS2310 diagnostics only (physical
  source lines differ by one from corpus directive-stripped lines), no TS2430.
- **Owned continuation, blocked on coordinated cutover:**
  `readonly_target.rs::property_accessibility_error` and `class_derives_from`
  still use syntax/symbol-only heritage. Native uses resolved type-based
  `hasBaseType`, including intersections and declaring classes underlying
  synthetic properties. Candidate adds TS2445 at (83,14), (92,14) to
  `conformance/mixinClassesMembers`; direct native CLI reports no diagnostics.
- **Owned continuation, same publication boundary:**
  `readonly_target.rs::is_assignment_to_readonly_property` must obtain mapped
  readonly metadata from the resolved member image, not only the inherited
  origin symbol. `compiler/readonlyAssignmentInSubclassOfClassExpression`
  loses RIGHT lines 8 and 10 (`any` becomes `number`) for a base cast to
  `new () => Readonly<{ attrib: number }>`. Root symbols alone do not carry
  the mapping's readonly modifier.

The completed candidate workspace test run stopped at
`members::property_name_tests::composite_this_member_keeps_the_whole_intersection_receiver`:
186 checker unit tests passed, one failed. The failure was an obsolete decline
assertion: candidate now correctly resolves the inherited `this` names rather
than `None`. It must be replaced with receiver behavior assertions on a real
cutover, not re-pinned to incidental implementation wording. Candidate clippy
(`--workspace --all-targets -- -D warnings`) and workspace format check were
clean. No candidate speed certification was attempted after the correctness
losses.

## Ownership, publication and expensive work contract

Native `resolveObjectTypeMembers` publishes provisional own members before base
traversal (`ObjectFlagsUnresolvedMembers`), then completed members; native base
resolution precedes this and resets partially resolved members. TSR's existing
`base_type_links` belongs to one private `Checker`, lives for that checker, and
uses merged binder `SymbolId` keys. Constructor types and base types are distinct
queries guarded by `Resolutions` / `ResolvedBaseConstructorType` and
`ResolvedBaseTypes`. Completed empty, error, active/provisional, and unsupported
producer results must not be equated. Existing constructor-signature gaps are
not proof of native empty members. No Program-global or cross-worker identities
are used.

Member declaration roots are binder symbols; instantiated value identity also
includes the ordered reference arguments, receiver `this`, static/instance side,
and mapper. Diagnostic permission additionally depends on the actual access
node, declaring constructor, written alias/origin, and read/write policy. A
shared root symbol does not certify shared access results or readonly flags.
The expensive workers are `get_base_constructor_type_of_class`,
`resolve_base_types_of_class`, constructor-return instantiation and member/type
substitution. Existing base links reuse base resolution; member requests can
still repeat traversal and substitution. No counts, copy-byte attribution or
reuse benefit were measured. **Beads follow-up request to integrator:** attach
an actual-worker/query/completed-hit/active-repeat/copy-byte audit to
`tsr-2zk.4.12` / `tsr-1yb.11` before extending member reuse. No speculative cache
is justified by this correctness experiment.

## Protected alias receiver root — tsr-2zk.4.14 (initial rejected partial port)

Direct native/TSR control:

```ts
class Base { protected p = 1; }
const Alias = Base;
class Derived extends Alias { read(b: Base) { return b.p; } }
class Middle extends Alias {}
class Deep extends Middle { read(b: Base) { return b.p; } }
```

Frozen TSR reports TS2445 at (3,56), (5,54). Native reports TS2446 at those
locations, naming Derived/Deep and Base in the complete receiver restriction
message. An owned experiment replaced `class_derives_from`'s capped syntax
walk with existing type-based `has_base_type`, and used that same predicate for
the concrete containing receiver. Candidate/native control stdout matched
byte-for-byte. No cache was added; base links retain the owner/publication
contract above. The permanent experimental regression test also verified codes,
spans and class-name arguments, plus a direct-subclass legal control and outside
TS2445 control; it passed, then was removed when the experiment was rejected.

Complete unfiltered dumps: 477,970 type keys and 10,570 diagnostic keys, zero
vanished protected keys, zero changed prior RIGHT type or RIGHT/EMPTY_RIGHT
case verdicts, and zero changed type payloads. **One changed-but-still-WRONG
case blocks publication:** `conformance/mixinAccessModifiers` removes TS2445
at (76,6), (77,6), (89,6), (90,6). The (89,6) diagnostic is expected, not
extraneous. Direct pinned native CLI confirms TS2445 at physical source (92,6),
the directive-stripped corpus (89,6), for `C4.s` in C5's static method.

Native prerequisite: `isClassDerivedFromDeclaringClasses` runs `forEachProperty`
and requires inheritance from **every protected constituent's declaring class**.
TSR `property_accessibility_error` receives one winning constituent SymbolId;
changing ancestry alone treats the composite property's permission as that
single origin. This loses the Protected2 restriction on C4.s. Faithful completion
requires the owned member subsystem to expose the complete composite property
roots, static context and containing type together, then apply native
`forEachProperty` policy. It must not special-case C4 or reinstate syntax-based
ancestry just for synthetic properties. This experiment is reverted too.

All candidate workspace release tests completed, including doc tests; clippy
and workspace format checks passed. Full unfiltered coverage completed over
12,444 discovered sources: checker_types 8042/9538, diagnostics 4221/5502.
Snapshot writes alone were redirected; no diagnostic-output hooks were used,
but coverage stdout equivalence against an unintercepted run was not separately
proved. Correctness rejection does not rely on that coverage receipt: its
loss is present in the ordinary unmodified verdict and native CLI paths.
Initial 21-pair baseline wall ratios were noisy (domain-model 1.1164,
generic-imports 1.0460); 41-pair repetitions were 1.0025 and 1.0124.
Matching scope/options/diagnostics still does not prove complete actual work.
No semantic change from either experiment is shipped. The single-owner whole-file
member/static/alias contract remains unchanged pending a complete port.

### Canonical native ancestry identity and writer/publication audit

Pinned `hasBaseType(t, checkBase)` invokes `getTargetType`: reference targets
are canonical class/interface types, not printed class names, arbitrary
structurally equal objects, or reference argument lists. Native
`createTypeReferenceEx` writes each reference target and ordered arguments and
interns it in the target-owned instantiation map keyed by `getTypeListKey`.
`getTypeWithThisArgument` retains those ordered arguments and appends the
concrete this argument, recursively preserving intersection constituents.
Ancestry intentionally strips that reference context only for target comparison;
member values, mapped readonly flags, presentation and access diagnostics must
not reuse that stripped identity as their full semantic context.

Existing TSR `class_or_interface_target` reads `type_reference_targets` and
Named member-owner symbols, merges the binder symbol, and admits only actual
class/interface owners. Existing `has_base_type` reads `get_base_types`, whose
writers are `resolve_base_types_of_class` and `resolve_base_types_of_interface`.
No new traversal or cache remains in the shipped code. For a future full
`.4.14` port, preserve the existing private Checker lifetime, merged symbol
base-query ownership, active resolution stack/provisional base vector versus
`BaseTypes.resolved` completion, and constructor failure/unsupported gaps.
Do not promote an active recursive result to completed success or an unsupported
constructor-signature result to an empty member image. Static access must retain
the resolved constructor side and all inherited synthetic constituent roots;
written value aliases remain diagnostic context, not a substitute ancestry key.

**Bounded Beads follow-up request for integrator (`tsr-2zk.4.14`):** instrument
one protected value-alias fixture, one indirect alias fixture, and
`mixinAccessModifiers` to count ancestry requests, completed base-link hits,
active repeats, actual constructor/base-resolution executions, member traversal
executions and copied base-vector bytes. Counts are currently unmeasured; add no
reuse/cache surface until that bounded audit is recorded. Native
`isClassDerivedFromDeclaringClasses` / `forEachProperty` and concrete receiver
`hasBaseType` are the consumers. Next active owned work is exposing complete
static/instance synthetic constituent roots without changing shared type stores,
symbols, or any other checker files.

## Completed protected constituent port — tsr-2zk.4.14.1

The second `.4.14` implementation retains composite origins in
`members.rs::property_accessibility_roots`. It reads the existing composite
property image, recursively follows underlying roots, and preserves the base
constructor image on the static side. An own class member shadows inherited
origins; no binder or shared type-store symbol is synthesized. The returned
ordered root list is query-local and unpublished, with no new semantic cache.
`readonly_target.rs::property_accessibility_error` applies native synthetic
modifier precedence (private, public, protected), retains static context, and
requires ancestry from every protected declaring class via
`class_derives_from_protected_roots`. Concrete receiver restriction separately
uses the existing `has_base_type` predicate. This replaces the capped
syntax-only ancestry walk, which is removed.

Pinned operations: `forEachProperty`, `isClassDerivedFromDeclaringClasses`,
`checkPropertyAccessibilityAtLocation`, `hasBaseType`, and
`getDeclarationModifierFlagsFromSymbolEx`; native SHA and canonical target /
reference-writer contracts above apply. Root queries use receiver TypeId plus
property name, own/static side, and existing base/composite publication. Root
identity is binder SymbolId, not printed property or class text. Read/write
accessor declaration selection remains at the access consumer. Ordered reference
arguments and concrete this are not conflated with ancestry target identity.
The private Checker owns all existing base links; no Program/global publication
is introduced. Root absence is not a cached completed member failure.
The existing base-resolution guard owns active reentry versus completed bases.
Expensive work is the base constructor/base resolution plus inherited/composite
traversal; the bounded `.4.14` count follow-up above remains required before
extending reuse. No speed optimization claim or worker-count claim is made.

Native control:

```ts
class A { protected static s = 1; }
class B { protected static s = 1; }
declare function mix<X, Y>(x: X, y: Y): X & Y;
class Both extends mix(A, B) { static read() { Both.s; } }
class One extends A { static read() { Both.s; } }
Both.s;
class Public { static s = 1; }
class Mixed extends mix(A, Public) {}
Mixed.s;
class Base { protected p = 1; }
const Alias = Base;
class Derived extends Alias { read(b: Base) { return b.p; } }
```

Native and candidate stdout match byte-for-byte: TS2445 at (4,53), (5,44),
(6,6), all naming `typeof Both`; TS2446 at (12,56), naming Derived and Base.
`Mixed.s` remains legal by public-constituent precedence. Before: TS2445 at
(4,53), (6,6) names A, misses (5,44), wrongly rejects Mixed.s, and reports
TS2445 rather than TS2446 at (12,56).
Permanent `property_protected_constituent_native_static.rs` checks the exact
ordered access-error codes, spans and complete message arguments.

Full unfiltered dumps completed, compared with the completed constructor port:
477,970 type keys and 10,570 diagnostic keys; zero vanished or changed protected
keys, including all 469,765 prior RIGHT type lines and 9,189 RIGHT/EMPTY_RIGHT
diagnostic cases. **All type payloads unchanged.** The only diagnostic payload
change is `conformance/mixinAccessModifiers`, still WRONG: five extraneous
TS2445 removed at (54,4), (76,6), (77,6), (90,6), (103,6). Expected (89,6)
TS2445 remains. Multiset comparison of every changed case finds **zero expected
before diagnostics lost and zero new diagnostics**. No named full case converts;
remaining missing/extra mixin diagnostics require the still-open member cutover
and other previously identified producers, not suppression.

Workspace release tests completed through doc tests; clippy warnings-denied and
workspace format check passed. Full unfiltered coverage completed over 12,444
sources: checker_types 8042/9538, diagnostics 4221/5502. Scratch snapshot writes
alone were redirected, with the previously stated independent-stdout-proof limit.
21-pair observed baseline wall ratios: 1.0047/1.0254 (domain-model/generic-imports).
Additional 41-pair runs: **1.0042/1.0010**, within the stated measurement noise
band, no measured material slowdown. Pinned native observed ratios:
0.9982/0.9264. Loaded scope/options/diagnostics match, but complete captured
inputs and actual performed work remain unverified; all reports retain
`work_comparable=false`, `target_verified=false`. No <=0.50 certification.
Original readonly/member cutover remains open under `.4.12.1`; broad `.4.14`
also remains open. The bounded composite-origin/static-receiver slice is
`.4.14.1`; `bfca6e08` implements that slice, not broad issue completion.

## Constructor access-kind root — dedicated parent-.4 issue, integration queued

Delivery identifiers: code/test `5f26bb1f` (Box auto-commit), verification
record `60f065be`, scope record `e10f62b0`. Integrator reports a dedicated
single-root issue under parent `.4` created and this split delivery queued;
the numeric leaf ID was not supplied. It is separate from broad `.4.12` and
`.4.14`; no broader inherited-readonly completion is claimed.

Pinned `isAssignmentToReadonlyEntity` tests `ast.IsAccessExpression`, not only
property access. `assignment_is_inside_the_declaring_constructor` previously
rejected `ElementAccessExpression`, incorrectly reporting TS2540 for the
constructor's own `this['x']` assignment. It now selects the receiver from either
access kind and retains the exact declaration-parent identity check. A missing
control-flow container is not constructor permission (native reports readonly
there). No metadata cache, traversal, mapper or member image is added. This is not a
readonly-name or class-name pattern fix. The missing-flow-container branch is
not permission, matching native's error direction.

Native-supported control:

```ts
class C { readonly x: number; constructor() { this['x'] = 1; } }
class D extends C { constructor() { super(); this['x'] = 2; } }
declare let c: C;
c['x'] = 3;
```

Before: TS2540 at (1,52), (2,51), (4,3). Native and candidate: only (2,51),
(4,3), identical complete CLI messages/spans/order; stdout compared byte-for-byte.
Permanent `property_inherited_native_constructor.rs` verifies the remaining
readonly diagnostic spans and arguments, distinguishing own versus inherited
constructor writes. Workspace release tests, clippy with warnings denied, and
workspace format check all pass.

Complete unfiltered type and diagnostic dumps finished: 477,970 type keys and
10,570 diagnostic case keys. Every 469,765 prior RIGHT type key and 9,189 prior
RIGHT/EMPTY_RIGHT diagnostic key is present with unchanged verdict. **Zero
vanished, zero verdict losses, zero changed type or diagnostic payloads**, including
WRONG rows/cases. This ad hoc control fixes a real consumer bug but converts no
named corpus case. Full coverage completed: checker_types 8042/9538,
diagnostics 4221/5502 over 12,444 discovered source cases, unchanged scores.
Scratch snapshot-path redirection was used with the previously stated stdout
verification limit, never diagnostic-output rewriting.

21 fresh-process interleaved baseline/candidate wall pairs: domain-model
0.9949, generic-imports 0.9871 (no observed slowdown). Pinned native comparisons:
domain-model 1.0193, generic-imports 0.9298. Scope/options/diagnostics match in
these harness runs, but complete input capture and actual checked-worker work
remain unverified: `work_comparable=false`, `target_verified=false`. These are
observed ratios, not a verified performance target or optimization claim.

## INDEX-SYMBOL-UNION-KEY-INFOS — tsr-2zk.16.475 (one blocked case)

Current `index_infos_of_declaration` already enumerates `TypeData::Union`
constituents, filters with `is_valid_index_key_type`, and callers deduplicate
by semantic key TypeId across declarations. It does not treat printed union
text as a single key. Native `getIndexInfosOfIndexSymbol` uses `forEachType`,
`isValidIndexKeyType`, and pointer identity `findIndexInfo`: string, number,
ES symbol, pattern literal, or nongeneric intersection with a valid constituent.
Existing TSR predicate uses primitive flags, existing pattern metadata and
existing generic-signature metadata; no alternative key heuristic was added.

One actual native discrepancy reproduced by ordinary CLI:

```ts
interface Missing { [key: string | symbol]; }
```

Native reports TS1021 `An index signature must have a type annotation.` at
(1,21); TSR reports none. Exact external prerequisite:
`check.rs::check_index_signature_key_type` (around 2704) currently nests the
TS1021 guard under primitive `KeywordTypeNode` key recognition. Move native
`checkGrammarIndexSignature`'s missing-return-annotation requirement out of that
semantic key-shape branch, preserving grammar prerequisite ordering/span. Union
keys must not bypass it. `check.rs` is unowned; no parallel diagnostic collector
or suppression was introduced.

Native also retains valid key IndexInfos when the return annotation is absent
(default any) or resolves to error. An owned experiment removed the collector's
value-error refusal and adopted native default any. Complete unfiltered dumps
finished: 477,970 type keys and 10,570 diagnostic keys, zero vanished/changed
protected keys and zero changed payloads of any verdict. Full coverage finished
unchanged (8042/9538 types, 4221/5502 diagnostics). Clippy passed after a doc
markup correction; workspace test run failed an experimental assertion that
unresolved index access returns the intrinsic error TypeId: actual accesses
return alias-owned TypeIds instead. That assertion was not converted into a
representation/default test. The experiment and test were removed; no change
to the unowned alias/indexed representation is justified by this observation.
No named case or real index-value consumer conversion was proved, no candidate
performance claim or verified root completion is made.

Publication/work boundary for a future port: declaration collection is a private
Checker query, ordered by declaration then union constituent, producing each
valid key with value/readonly/source-declaration provenance. Existing caches own
resolved type metadata; this query-local vector adds no cache. Key identity is
store-local semantic TypeId, not alias spelling; declaration owner and static
side remain separate. A missing/error value is not absent key publication.
Expensive work is type-node resolution plus valid-key filtering and caller
identity dedup; actual worker executions/hits/copy bytes remain unmeasured.
Bounded Beads follow-up request: count those operations on primitive, union,
pattern/intersection, duplicate-key and unresolved-value controls before
extending reuse. Authoritative target is `conformance/indexSignatures1`, one blocked case.
Its current type verdict shows union-index type literals collapsing to `any`
(e.g. rows 258,293,296,299,302,305), while interface union key collection already
works. Exact blocking producer: `declared.rs::build_type_literal`
(the index-member branch around 2540) calls singular `index_signature_member`,
which declines UNION keys, before it ever calls `index_infos_of_declaration`.
The owned collector therefore cannot reach that literal. Required external hunk:
replace singular index rendering with members from the existing
`index_infos_of_declaration` followed by existing `index_info_members` for each
valid-key IndexInfo; no new plural API is needed. Deduplicate each semantic key
TypeId across declarations
in declaration/constituent order, and retain key-specific value/readonly/source
provenance. Do not render printed union text as one key. This requires a serialized unowned `declared.rs` caller
cutover onto those existing owned APIs; no shim or shared-file edit was made.

Missing duplicate TS2374 in this target belongs to
`index_constraint.rs::check_duplicate_index_signatures`, also unowned, not the
IndexInfo collector's dedup: native duplicate-diagnostic validation walks source
signatures even though resolved infos deduplicate key identity. Target also has
unrelated mapped/alias/inference/diagnostic gaps; no one-case passing promise is
made. This exact target and producer supersede the earlier unassigned-case note.

## Release-target limits

Observed 21-pair fresh-process interleaved comparisons after reverting the
candidate (therefore not candidate performance evidence): baseline TSR wall
ratio 1.0122 on domain-model, 0.9554 on generic-imports. Pinned native
comparisons: observed TSR/tsgo wall ratio 1.1063 on domain-model and 0.9318
on generic-imports.
These runs report matching loaded scope/options and diagnostics, but
`work_comparable=false`, `target_verified=false`: complete cross-tool query
inputs and actual performed checker work/worker budgets are unverified.
No verified <=0.50 claim is made. The restored full unfiltered coverage command
completed over all 12,444 discovered source cases: checker_types 8042/9538
(98.10% line rate), diagnostics 4221/5502. Snapshot writes were redirected into
`/tmp/box/snapshots` with a throwaway preload interceptor, leaving owned-file
boundaries intact; an attempted mount namespace was denied by the Box runtime.
Full message/chain/length/order parity and complete corpus source coverage remain
integration-oracle prerequisites. Restored `cargo test --offline --release
--workspace` completed successfully through workspace doc tests. Thus the final
code is the unchanged baseline, not the rejected candidate; the shipped change
is this evidence and prerequisite record only.
