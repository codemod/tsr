# Lane notes: r4-templates (`tsr-2zk.909`, `tsr-2zk.16.118`)

Judgment calls made while porting template literal types, string mappings and
enum constant evaluation. Upstream anchors are `vendor/typescript-go` @
`5b1047d`. `tsr-6.11` and `tsr-6.18` are claimed elsewhere and not worked here.

## §1 Enum member values come from the checker's evaluator

**Forcing constraint.** `get_declared_type_of_enum` folded member initializers
with a nested, symbol-free `eval` that found references *by name* among the
prior members of the same enum, and only for `E.m` / `E["m"]` whose receiver
was spelled as the enum's own name. Upstream folds with `c.evaluate`
(`evaluator.NewEvaluator(c.evaluateEntity, OEKParentheses)`,
`checker.go:931`), whose `evaluateEntity` (`checker.go:24024`) resolves
identifiers and entity-name accesses with `resolveEntityName` and reads:

- the global `Infinity` / `NaN` (by symbol identity),
- any enum member — of this enum, another declaration of it, another enum,
  or a namespace-qualified path `A.B.C.E.V1` — through `evaluateEnumMember`
  (`checker.go:24077`), which answers `0` for a member declared after the
  usage and `nil` for a self-reference,
- a constant variable without a type annotation, evaluated with its own
  declaration as the location.

The old `eval` also lacked number-to-string `+` and template expressions.
Measured on the six `tsr-2zk.16.118` cases: `enumConstantMembers`
(`NaN`/`Infinity` members), `constEnums` (`A.B.C.E.V1`, `Enum1["W"]`,
`` Enum1[`V`] ``), `enumErrors`, `isolatedModulesGlobalNamespacesAndEnums`
(cross-file merged declarations, a cross-file `const`).

**What was ported.** `enum_initializer.rs` gains `evaluate`
(`evaluate_enum_constant`, arm for arm with `evaluator.go:24`, including
`evaluateTemplateExpression` and `jsnum`'s `Exponentiate`/shift semantics),
`evaluateEntity`, `evaluateEnumMember` (value only), `getEnumMemberValue`, an
expression form of `resolveEntityName`, and the slice of
`isBlockScopedNameDeclaredBeforeUse` the evaluator asks. The member loop in
`get_declared_type_of_enum` calls it with the member as `location`.

**Port record (`docs/conventions.md`).**

- *Native operation:* `computeEnumMemberValues` / `getEnumMemberValue`
  (`checker.go:23938`, `:23930`).
- *Key, owner:* upstream memoises `enumMemberLinks.value` per member
  declaration. No new table: the value's single owner here is the member's
  declared literal type (`declared_types[member]`, payload
  `TypeData::EnumLiteral`), published by the enum's member loop as soon as the
  member is folded; `enum_member_constant` reads it back.
- *Publication states:* upstream sets `NodeCheckFlagsEnumValuesComputed`
  before the loop, so a re-entrant read sees members folded so far and `nil`
  for the rest. Here the enum's `DeclaredType` resolution frame is held for
  the loop (`get_declared_type_of_enum`); `enum_member_constant` never forces
  an enum whose frame is on the stack, so the same reads answer `None`.
  The frame never closes a cycle (the only re-entrant road checks it first).
- *Receiver/alias context:* `resolveEntityName` follows aliases to a symbol
  carrying `Value` (`resolve_alias_fully`), and reads a namespace's exports
  after `resolveExternalModuleSymbol`.
- *Work boundary:* evaluation is syntactic over the initializer's spine plus
  one declared-type computation per other enum referenced; a constant
  variable chain is bounded by `EVALUATE_DEPTH_LIMIT` (64), since across files
  there is no before-use order to stop a cycle.

**Merged symbol.** `get_declared_type_of_enum` now answers a per-file symbol
of a merged enum with the merged enum's type. Upstream reaches
`getDeclaredTypeOfEnum` only through `getSymbolOfDeclaration`, which returns
the merged symbol; this port was building a second enum type from one file's
declarations when a caller asked with the per-file symbol, and in that type
`E = A` (A from the other file) minted its own `Enum.E` instead of reusing
`Enum.A`.

**Accepted residue.**

- Upstream computes values *per declaration*; this port computes a merged
  enum's declarations together, in declaration order. A reference from an
  earlier declaration to a member of a *later* declaration of the same enum
  (only possible across files) is `None` here where upstream would compute
  that declaration first.
- `IsSyntacticallyString`, `ResolvedOtherFiles` and `HasExternalReferences`
  are not carried; their consumers are the `isolatedModules` reports
  (TS18055/TS18056), which are not made from this computation.
- `isUsedInFunctionOrInstanceProperty`'s class-property arms are not ported;
  neither usage the evaluator asks with (an enum member, a variable
  declaration) sits under a class property without a function in between.
- `getExportsOfSymbol`'s `export *` re-exports are not read by the
  expression `resolveEntityName`; such a path answers `None`.

**Falsifier.** A case where an enum member's printed type differs from the
baseline because a reference folds here and not upstream (or the reverse);
the loss check over the corpus is the measurement.

## §2 `checkTemplateExpression` shares the evaluator

**Forcing constraint.** `checkTemplateExpression` (`checker.go:7992`) folds a
template expression with the *same* `c.evaluate` as enum members. The port
had a second, template-only copy (`evaluate_template_constant` over
`evaluate_constant_expression_with`) whose `evaluateEntity` slice read only
constant variables and the global `Infinity`/`NaN`, so
`` `${AnimalType.cat}` `` (a string enum member) stayed `string` where
upstream answers `"cat"` (`discriminatedUnionTypes4`: 9 lines, through the
narrowing that `case` clause feeds).

**What changed.** `evaluate_template_constant` now calls
`Checker::evaluate_constant` (§1) with the template as `location`; its
private `constant_entity_symbol` resolver is gone, replaced by §1's
expression `resolveEntityName`. The before-use test is §1's, which adds
upstream's deferred-use arm (a constant declared after a function that uses
it) the template copy did not have. The symbol-free
`evaluate_constant_expression` stays for its syntactic callers.

**Falsifier.** A template expression whose folded literal differs from the
baseline; the corpus loss check measures it.

## §3 `computeConstantEnumMemberValue`'s switch reads the evaluator

**Forcing constraint.** The switch (`checker.go:23997`–`:24020`) reports
TS2477/TS2478 on a const enum's non-finite value, TS2474 (const) or TS1066
(ambient) on a `nil` value, and TS18033 otherwise. Without a symbol-aware
evaluator the port decided "`nil`" three different syntactic ways:
`enum_member_name.rs` (§819/§821 of `checker-notes-diag2.md`: the symbol-free
evaluator plus a reference-identifier / call guard), `enum_initializer_may_evaluate`
(an over-approximation for TS18033), and `const_enum_numeric_value` (numeric
arms only, for TS2477/TS2478). Each declined wherever an enum member or
constant could be involved, and the guard over-reported TS2474 where a
reference *does* fold (`constEnums`: `W5 = Enum1[`V`]`,
`constEnumPropertyAccess3`) and under-reported where it does not
(`constEnumErrors`: `Y = E1.Z` naming a missing member, `F = E * E`
through prior members).

**What changed.** All three ask `Checker::evaluate_constant(initializer,
member)` (§1) — the same answer the member's declared type is built from.
The three approximations are deleted; §819 and §821 carry a superseded note.
The checks stay where their callers put them (`check.rs`'s member arm and
`check_enum_member_name`); evaluating up to three times per member is
syntactic work over one initializer, with any referenced enum's values
already published.

**Falsifier.** A TS1066/TS2474/TS18033/TS2477/TS2478 line that differs from
the baseline where the evaluator's answer is the cause.

## §4 TS2565 / TS2651 come from `evaluateEnumMember`

**Forcing constraint.** Upstream reports both from inside the evaluation of
a member's initializer (`evaluateEnumMember`, `checker.go:24077`): TS2565
when the reference resolves to the member being computed (`A = A`,
`B = E.B`, `C = E["C"]`, `D = 1 + D` — `enumPropertyAccessBeforeInitalisation`),
TS2651 when it is declared after the usage. The port had a separate walker
(`collect_enum_forward_references` with a binder-only `evaluated_enum_member`)
that reported TS2651 only, followed only a template's first span, resolved
only `E.m` on an identifier naming the enum, and was gated on the enum not
being ambient.

**What changed.** The evaluator takes an optional report sink.
`check_enum_member_forward_references` evaluates the member's initializer
with a sink and reports what `evaluateEnumMember` would; the declared-type
computation evaluates without one, so nothing is reported twice and a
reference into another enum (whose values are forced through its declared
type) is reported by that enum's own member check, as upstream reports it
from that enum's `computeEnumMemberValues`. The ambient gate is gone:
`isBlockScopedNameDeclaredBeforeUse`'s ambient-usage arm (in §1's helper)
already answers "before" there, and a self-reference is TS2565 in an ambient
enum too. The member name in TS2565 is the symbol's name, which is what the
baseline spells (`Property 'B'` for `B = E.B`).

**Accepted residue.** Reports are made in the member check's order (after
TS2452/TS1066/TS2474/TS18033/TS2477 for the same member) rather than
mid-switch. Only `const enum E { A = A }` puts two of them (TS2565, TS2474)
on one span, where upstream emits TS2565 first; diagnostics are compared
sorted by position, so the order is not observable there.
