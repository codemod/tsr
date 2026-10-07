# Inherited readonly property boundary — tsr-2zk.4.12

## Decision

No semantic cutover is published. The current split between inherited property
values and declaration metadata is reproduced, but exposing resolved base symbols
fails the mandatory no-loss gate. Integrator prerequisite: coordinate contextual
member reads and circular interface conformance before publishing that cutover.
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
