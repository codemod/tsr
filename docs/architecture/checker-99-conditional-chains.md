# Conditional alias chains and definite conditional outcomes

Pinned tsgo: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Baseline `9044e902`:
456,755/478,855 matching assertions, 6,971/9,538 complete cases. Part of the
conditional-type unit (`bd tsr-6.3`) and its partial-default sibling
(`bd tsr-6.23`).

## The forcing constraint

`conditionalTypes1` alone carried 79 non-RIGHT rows. Twenty-seven of them were
one shape: a generic alias whose body is a *reference* to a conditional alias.

```ts
type Extends<T, U> = T extends U ? true : false;
type IsString<T> = Extends<T, string>;
type Q1 = IsString<number>;   // tsgo: false; port printed IsString<number>
```

The port evaluates conditional aliases over syntax: `evaluate_conditional_alias`
binds the alias's parameters in `alias_evaluation_bindings` and walks the
`ConditionalTypeNode`. Any other body shape was refused, so the outer alias
stayed a named reference even when every argument was concrete.

Upstream has no such refusal because it has no such shape distinction.
`getTypeFromTypeAliasReference` (`internal/checker/checker.go:23580`) resolves
`IsString`'s body `Extends<T, string>` through `getTypeAliasInstantiation`
(`checker.go:23641`), so `IsString`'s *declared type* is a deferred conditional
whose root is `Extends`'s declaration. Instantiating `IsString<number>` then
instantiates that conditional (`getConditionalTypeInstantiation`,
`checker.go:22485`) and evaluates it.

## What is ported

- `evaluate_conditional_alias` follows a generic alias whose body is a
  `TypeReferenceNode` to another type alias. The reference's arguments resolve
  under the outer frame; a missing tail takes each declared default resolved
  under the preceding arguments (`fillMissingTypeArguments`,
  `checker.go:21954`); the inner alias evaluates with the *caller's* alias.
  That last point is upstream's: `getConditionalTypeInstantiation` receives the
  alias passed to `instantiateTypeWithAlias` unchanged, so in
  `type N3 = Not<boolean>` the distributed `true | false` is named `N3`, while
  `declare const n3: Not<boolean>` prints `boolean` (no new alias outside an
  alias body). Both are pinned.
- The alias-declared-position branch of `get_instantiated_type_reference`
  evaluates the chain with the alias of the enclosing declaration. A refusal
  there falls through to the named reference rather than to `error`: the
  direct-conditional branch's `error` is a measured decision about that shape
  and was not extended to a new one.
- `alias_declares_conditional` answers whether an alias's declared type is
  conditional through such a chain. The signature printer's written-annotation
  reuse (`serializeTypeForDeclaration`) now uses it, so `h(y: IsString<"a">)`
  keeps its written parameter while `y` itself is `true`.

## Definite outcomes for a generic extends type

Evaluating the chain exposed a pre-existing wrong answer. In
`conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable`,
`WithName<T>` (`DiscriminateUnion<BigUnion, 'name', T>`) evaluated to the first
union member. The direct form had the same defect:

```ts
type W5<T extends 'a' | 'b'> = { name: 'a' } extends { name: T } ? 1 : 0;
// tsgo: W5<T> (deferred); port printed 0
```

`getConditionalType` (`checker.go:24372-24429`) decides a non-deferred check in
two steps. FALSE requires the check to be unassignable to the extends type's
*permissive* instantiation (every type parameter mapped to the wildcard); TRUE
requires assignability to the *restrictive* instantiation (every type parameter
mapped to an unconstrained clone). Anything else stays deferred. The port
related the check to the extends type once, which is the restrictive half
alone, and read its `false` as definite.

`conditional_extends_instantiations` now builds both instantiations for an
extends type that mentions type parameters, and `definite_conditional_outcome`
applies upstream's order. Two deviations are stated rather than hidden:

- The port has no wildcard type distinct from `any`; the permissive image maps
  to `any`. Relating to the wildcard is relating to `any`, but the wildcard's
  instantiation propagation (an indexed access or conditional over the
  wildcard yields the wildcard) is not modelled. A failed instantiation defers.
- The restrictive clone is a fresh `TYPE_PARAMETER` not registered as a declared
  parameter, so no constraint is found for it — the observable half of
  `getRestrictiveTypeParameter`'s `noConstraintType`.

This alone still answered `W7<T>` (`Record<'name', T>` as the extends type) and
the large-union case wrongly: `{ name: 'a' }` related to `Record<"name", T'>`.
`get_property_names_of_type` enumerated the members table of the `Record` alias
symbol, which is not the type's member list, and answered no names, so the
relater's property walk demanded nothing. `mapped_alias_literal_key_names`
gives the names `resolveMappedTypeMembers` would: for a reference to a generic
alias whose body is `{ [P in K]: … }` over its own parameter `K`, the string
literal keys of `K`'s argument. Other key shapes keep the existing enumeration.

### Rejected: report such alias references as unenumerable

The first repair returned `None` (unknown) for every reference owned by an
alias symbol. It fixed the two losses but measured **22 RIGHT→WRONG and one
RIGHT→GAP** (indexingTypesWithNever 8, reverseMappedTypeInferenceWidening1 8,
genericConditionalConstrainedToUnknownNotAssignableToConcreteObject 2,
genericFunctionInference1 2): rows that depend on the relater deciding
alias-of-mapped references today. Reopening condition: general mapped-type
member resolution for alias references (not just literal keys), at which point
the empty enumeration disappears instead of being guarded.

## Measurement

At `9044e902` + this change: 456,789/478,855 (+34). Transitions:
34 WRONG→RIGHT (conditionalTypes1 27, mappedTypesArraysTuples 3,
deferredConditionalTypes 1, others), one WRONG→GAP
(genericCallInferenceInConditionalTypes1), **zero RIGHT losses and zero
GAP→WRONG**. The permissive/restrictive step and the literal-key names are
corpus-neutral by themselves; together they remove the two RIGHT→WRONG rows the
chain alone introduced in the large-union case.

## Controls

`crates/tsr-conformance/tests/conditional_alias_chains.rs` pins, against pinned
tsgo declaration output (`tsgo --declaration --strict`): chain evaluation
(`IsString<number>` → `false`), default fill through the chain, the alias
naming split (`N3` versus `boolean`), written-annotation reuse, the three
deferred generic extends shapes, the permissive FALSE outcome
(`string extends { name: T }` → `0`), and `Record` literal keys deciding a
concrete relation (`Z2<'b'>` → `0`, `Z2<'a'>` → `1`).

## Remaining in this family

- A non-generic conditional (`type Z = A extends B ? 1 : 0`, or a conditional
  written directly in an annotation) is still printed from its written text;
  upstream's `getTypeFromConditionalTypeNode` evaluates it. About 76 non-RIGHT
  rows print a conditional where the baseline wants its branch.
- Distributive constraint, infer-through-chain and recursive conditional
  relations remain in `bd tsr-6.3`.
