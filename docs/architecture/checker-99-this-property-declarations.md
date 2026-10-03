# This-property assignment declarations and JS constructor flow

Baseline `6b2203cc`: 456,949/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb. Units: bd tsr-6.26 (assignment
declaration value types) and STATUS §4's "JS constructors" item.

## The forcing constraint

A JavaScript class declares instance members by assignment: `this.x = 1` in a
constructor or method. The binder already filed those declarations on the
class, but `getWidenedTypeForAssignmentDeclaration` in this port returned the
error gap for every `this.x` declaration. Each read of such a member therefore
printed `error`/`any`, and every expression built on it (`this.data.find(...)`
in `thisInFunctionCallJs` alone: 32 rows) inherited the gap.

Upstream (`checker.go` `getWidenedTypeForAssignmentDeclaration`,
`isConstructorDeclaredThisProperty`) classifies a symbol whose declarations are
all this-property assignments into three kinds before the ordinary union:

1. **Typed** — a declaration carries a type (the reparser moves a statement's
   JSDoc `@type` onto `BinaryExpression.Type`); use it.
2. **Constructor** — some declaration's this-container is a class constructor;
   the type is the flow type of `this.x` at the constructor's `ReturnFlowNode`
   (`getFlowTypeInConstructor`, `flow.go:2466`), starting from the base class's
   property type or `undefined`. All-nullable flow types fall back to the union.
3. **Method** — otherwise; the base class property (`getTypeOfPropertyInBaseClass`)
   wins, else the declaration union plus `undefined` under `strictNullChecks`.

Inside the declaring constructor, `isThisPropertyAccessInConstructor`
(`checker.go:27328`) makes every access read `autoType`: a definite assignment
target prints `any`, a read follows its own flow from `undefined`.

## Decisions

**Reference identity for the constructor flow query.** Upstream synthesizes a
fresh `this.x` node parented to the constructor. This tree is immutable after
parsing (ADR-0012), and `check.rs`'s TS2564 arm records the same wall. The
query's only uses of the node are structural matching (`isMatchingReference`
compares `this` and the accessed name) and its container. So
`get_flow_type_of_property` takes an existing `this.x` declaration target from
the same constructor as the reference and the constructor's return flow as the
start. The alternative — a synthetic-reference variant of `FlowState` carrying
only a name — touches every matcher arm for no observable difference. It would
win if a query ever needed a reference in a constructor that writes no `this.x`
of that name; `getFlowTypeInConstructor` is only reached through a declaration
in that constructor, so none exists today.

**`autoType` spelling.** Auto-typed variables here already spell `autoType` as
the `any` intrinsic with an `is_auto` flag; property flow reuses that flag rather
than adding an intrinsic.

**Binder prerequisites, all from `declareSymbolEx` / `SetValueDeclaration`.**
The pinned controls exposed three binder divergences in this same mechanism:

- A prototype member conflicting with a `ReplaceableByMethod` this-property
  replaces it in the table without a diagnostic (`binder.go:203`). The port
  reported TS2300 and kept the property, so `this.foo = () => {}` beside
  `foo() {}` typed `foo` from the arrow.
- A non-assignment value declaration takes value-declaration precedence over an
  assignment declaration. `declare prop: string` after `this.prop = {}` now
  types `prop` as `string`. The effective-module half of `SetValueDeclaration`
  is not ported; the ambient guard is approximated by declaration-file/ambient
  module context in TypeScript files.
- A static this-container (static block or static member) files the property in
  the class exports (`getThisClassAndSymbolTable`, `binder.go:1137`), so
  `static { this.x = 1 }` is reachable as `C.x`. The old comment claiming
  `static x = 1` lived in `members` was stale.

**Braceless `@type`.** `parseTypeTag` parses with `mayOmitBraces`
(`parser/jsdoc.go:894`); `@type object` was dropped here, which left Typed
declarations on the Method road (`argumentsReferenceIn*_Js`).

**Definite property targets are not flow-narrowed** (`getFlowTypeOfAccessExpression`,
`checker.go:11396`); the element-access road already did this. Under
`exactOptionalPropertyTypes` the port keeps the narrowed answer: removing it
lost two `strictOptionalProperties1` rows because a `Partial<…>` member's
optionality is spelled as plain `undefined`, so `removeMissingType` cannot strip
it. That exception is the falsifier: once mapped optional members carry
`missingType`, deleting the guard must not lose those rows.

**JS errorType printing for nullish receivers.** A property read on a purely
nullish receiver answers `errorType`; JS baselines print it verbatim
(`jsFileClassSelfReferencedProperty`'s `this.testStackOverflow.bind : error`),
as the existing too-large-flow arm already does.

## Measured result

Candidate rebased on `6b2203cc`, full verdict dump against the same commit's
dump from an isolated checkout: **457,204/478,855 (95.48%)**, +255 matching
assertions; 195 WRONG→RIGHT, 60 GAP→RIGHT, **zero RIGHT losses**; 3
GAP→WRONG and 10 changed WRONG rows. The 3 GAP→WRONG are
`javascriptThisAssignmentInStaticBlock`'s `this.isArray` rows: the property
type is now the assigned arrow, whose inferred return is wrong because
`super` in a static block of `class extends Array` already reads `any` there
(row 14, pre-existing). Changed WRONG rows: `expandoFunctionContextualTypesJs`
now prints the `@type`'d `defaultProps` (only the `(): any` signature remains
wrong), `thisInFunctionCallJs` prints the function but lacks the `@this`
parameter, and `checkJsdocSatisfiesTag15` now sees its braceless `@type` but
prints the alias rather than the tuple.

Three pinned controls in `crates/tsr-conformance/tests/this_property_declarations.rs`
compare whole `.types` sequences with tsgo's own compiler-test baselines
(strict, non-strict, constructor/method/inherited/static/precedence cases).

Reverified on the current wave's exact base `cec7cef5` before integration:
457,641 → 457,896 aligned RIGHT, 2,538 → 2,475 GAP, and 14,064 → 13,872
WRONG. The same 195 WRONG→RIGHT, 60 GAP→RIGHT and three GAP→WRONG
transitions remain, with zero RIGHT losses. Complete checker cases improve
7,021 → 7,049 and diagnostic cases 2,790 → 2,793. These are this unit's
isolated results, not the combined wave totals recorded in STATUS.

## Not ported

- Constructor functions (`function C() { this.x = 1 }`): upstream's binder
  leaves these unbound (`!!! constructor functions`), and so does this port.
- Private names (`this.#x = …`) are excluded by the binder here; upstream binds
  them as `__#…@#x` members (`typeFromPrivatePropertyAssignmentJs`).
- `isAutoTypedProperty`'s TypeScript half (an unannotated property declaration
  under `noImplicitAny` reading its constructor flow) and the TS7008 implicit-any
  member diagnostic.
- Late-bound `this[k] = …` declarations.
