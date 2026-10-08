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
