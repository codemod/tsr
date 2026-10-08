# Lane notes: r5-heritage2 (tsr-2zk.1026)

Heritage conformance: `checkClassLikeDeclaration`'s base and implements arms,
`issueMemberSpecificError`, `checkInterfaceDeclaration`'s
`checkInheritedPropertiesAreIdentical` and base loop
(`vendor/typescript-go/internal/checker/checker.go:4293`, `:4494`, `:4991`,
pinned `5b1047d`). Frozen baseline: `12f46e7`.

Commit 1 (§1, §2), against the frozen baseline, both dumps unfiltered:
diagnostics +3 (`clodulesDerivedClasses`, `tsxGenericAttributesType5`,
`tsxGenericAttributesType6`), types unchanged, both loss checks empty. The
heritage lines of `bluebirdStaticThis` and `interfaceClassMerging` also
convert; those cases stay WRONG on other codes. Median child CPU over the
frozen binary, 21 samples: domain-model 1.016, generic-imports 0.974. Cases are named by their
diagnostics-dump keys; "plain" and configured keys are both in the one dump.

Population at the baseline (cases whose wrong lines include TS2415, TS2416,
TS2417, TS2420, TS2430, TS2320 or TS2720), from `diagverdictdump`:

| Case | Heritage lines | Other wrong lines |
|---|---|---|
| `contextualThisType`, `thisTypeInFunctions3` | extra TS2430 / TS2415 | 0 |
| `privateNamesAndFields`, `privateNamesAndMethods` ×2 configs, `privateNamesConstructorChain-1`, `-2` | extra TS2416 | 0 |
| `implementsIncorrectlyNoAssertion`, `tsxGenericAttributesType5`, `-6` | missing TS2416 | 0 |
| `classExtendsNull2`, `clodulesDerivedClasses` | missing TS2417 | 0 |
| `importTag23` | missing TS2420 | 0 |
| `multipleBaseInterfaesWithIncompatibleProperties2(exactoptionalpropertytypes=true)` | missing TS2320 | 0 |
| `subtypingWithObjectMembers5` | extra TS2420 ×3 (native: TS2559) | 3 |
| `interfaceClassMerging`, `enumAssignmentCompat7`, `builtinIterator`, `destructuringParameterDeclaration2`, `interfaceExtendsObjectIntersectionErrors` | missing TS2416 | ≥1 |
| `bluebirdStaticThis` | missing TS2420 | 2 |
| `complexRecursiveCollections`, `mergedInheritedMembersSatisfyAbstractBase` | missing TS2320/TS2430 | ≥1 |
| `genericDefaults`, `indexSignatureAndMappedType`, `dynamicNames` | extra TS2415 / TS2430 / TS2720 | ≥3 |

## 1. The merged-declaration declines, narrowed to what they protect

Upstream has no merged-declaration decline in either arm. The port carried two:

- **Source.** `declarations.len() > 1` declined every merged class, including
  a class merged with a namespace (a clodule). A namespace adds exports to the
  static side and nothing to the instance type, so the instance relation is the
  class's own. The decline now keys on
  [`has_single_type_declaration`](../../../crates/tsr-checker/src/heritage_conformance.rs):
  more than one class or interface declaration still declines, for the reason
  r4-heritage recorded (`mergedInterfacesWithInheritedPrivates3`). This is the
  narrowing r4-heritage §4 measured as converting nothing. Now that r5-relater3
  decides more pairs, it gives `bluebirdStaticThis` its TS2420
  (`class Promise<R> implements Promise.Thenable<R>`, merged with
  `namespace Promise`). The case stays WRONG on its TS2694/TS2724 lines.
- **Base.** `has_single_type_declaration(base)` declined a base merged from a
  class and an interface (`interfaceClassMerging`) as well as the lib-merged
  bases it was written for. Dropping it outright (measured, full dump):
  `tsxGenericAttributesType5`/`-6` convert, and four cases regress:
  - `subclassUint8Array`, `classExtendingBuiltinType` and
    `classFieldSuperAccessible` (lib interfaces merged across `lib.*.d.ts`
    files);
  - `extendConstructSignatureInInterface`, an unresolved base symbol for a
    `var` base, which the `is_some_and` already declines.

  The base decline now keys on a declaration in the bundled default library
  (`has_default_library_declaration`). That is the population whose
  inherited signatures (`Uint8Array`'s construct-signature inheritance,
  `resolveObjectTypeMembers` concatenating base signatures) this port's
  declared member table does not carry.

  *Falsifier:* a user-declared base that merges interfaces across files and
  still regresses would show the decline was about merging after all.
  *What would let it go:* base signature inheritance in the declared member
  table (`declared.rs`, r5-typeparams2's file).

## 2. Clodule bases on the static side (TS2417)

`check_static_side_assignability` (`check.rs`, this lane's TS2417 arm)
declined any base whose symbol had more than one declaration. It now accepts
one class declaration plus any number of namespace declarations: the
namespace's exports are static members, and the walk already relates them
member by member. `clodulesDerivedClasses` converts. r5-relater3 §6 made
`typeof Path.Utils` vs `typeof Shape.Utils` decidable, and the base merge was
the remaining decline.

## 3. Held diff: inherited members read their owner's `this` as the receiver

[r5-heritage2-inherited-this-argument.diff](r5-heritage2-inherited-this-argument.diff)
(`members.rs`, `contextual.rs`; main's files).

**Root cause.** `resolveObjectTypeMembers` instantiates every base with the
receiver's this argument (`getTypeWithThisArgument(baseType, thisArgument)`),
so `interface Y extends X {}` reads X's `a: (p: this) => void` as
`(p: Y) => void` on a `Y`. The port's receiver substitution
(`instantiate_for_reference_with_this`) maps only the receiver's own minted
`this`. An inherited member therefore keeps `X.this`. `Y` is then not
assignable to `X`: the extra TS2430/TS2415 in `contextualThisType` and
`thisTypeInFunctions3`, and the extra TS2322 for `x = y`.

**Change.** After the receiver substitution, a property whose declaring owner
(`symbol.parent`) is a class or interface other than the receiver's target
maps that owner's polymorphic `this` to the this argument. It does so only
when that `this` has been minted and is mentioned. The contextual property
read (`contextual.rs`, `getTypeOfPropertyOfContextualType`'s concrete arm)
takes the same step. Without it, the literal in `contextualThisType` types
`p: this` against a `Y` that now reads `(p: Y) => Y`, and becomes a TS2322.

**Measured** against the frozen baseline, both dumps, unfiltered:

- diagnostics +3, no losses: `contextualThisType`, `thisTypeInFunctions`,
  `thisTypeInFunctions3`;
- types +9 RIGHT, **−3 RIGHT**: `inferenceErasedSignatures` 0:21–0:23
  (`SomeClassC`, `SomeClassM`, `SomeClassR`) become `error`.

**Why the loss.** `set<K extends keyof this>` is inherited from
`SomeBaseClass`. Before the diff both sides of the conditional's
`SomeClass extends SomeAbstractClass<infer C, …>` read the identical
unsubstituted signature. After it they read `keyof SomeClass` and
`keyof SomeAbstractClass<C, M, R>`, which are distinct. The relater
answers `Unknown` for that generic signature pair, and the conditional
gaps. Native relates them. The loss is the relater's (r5-relater4,
`relater.rs`), not the substitution's: the substitution is native's. The
diff is held until the relater decides the pair. It is not narrowed by
a heuristic.

## 4. Not converted here, with the owner of the root cause

- **Private names** (`privateNamesAndFields`, `privateNamesAndMethods`,
  `privateNamesConstructorChain-1`/`-2`, extra TS2416). The binder keys `#foo`
  by its text. Native keys it per declaring class,
  `GetSymbolNameForPrivateIdentifier` = `__#<classSymbolId>@#foo`. Skipping
  `#` names in `issue_member_specific_error` is not enough: the class-level
  relation still fails, because the derived `#foo` shadows the base's, and
  the broad TS2415 would replace the TS2416. Native's relation passes,
  because the derived type carries both keys. The fix is the binder key plus
  every checker lookup of a private name (`lookupSymbolForPrivateIdentifierDeclaration`).
  That is a shared-contract change across the binder, `expressions.rs` and
  `members.rs`, and it is not drafted here.
- **Implemented type aliases** (`implementsIncorrectlyNoAssertion`). Native
  implements `getReducedType(getTypeFromTypeNode(typeRefNode))` and checks
  `isValidBaseType`. `check_class_implemented_types` resolves only class and
  interface symbols. It needs `base_types.rs`' `is_valid_base_type` to be
  `pub(crate)` (r5-decls) and the type of an `ExpressionWithTypeArguments`
  that names an alias.
- **Weak implemented targets** (`subtypingWithObjectMembers5`, native TS2559
  ×3 where the port reports TS2420). The broad report is
  `checkTypeAssignableTo(typeWithThis, baseWithThis, name, broadDiag)`, and
  `isRelatedToEx`'s common-property check reports TS2559 with no head
  message. The port's `report_weak_type_failure` is private to
  `assignreport.rs` (r5-report2), so this ships as the §5 diff.
- **JSDoc `@implements` on an exported JS class** (`importTag23`). The tag's
  name resolves from inside the JSDoc comment, which keeps no parent edge
  (ADR-0003; the parser's `attach_jsdoc`), so `heritage_entity_symbol`
  (`members.rs`) never reaches the module's locals. A non-exported class
  in a script resolves only because the globals answer. The native reparser
  hosts the tag in the class's heritage clause.
- **Interfaces extending a mapped alias** (`multipleBaseInterfaesWithIncompatibleProperties2`
  under `exactOptionalPropertyTypes`). `get_property_names_of_type` answers
  `None` for `http.AgentOptions extends Partial<TcpSocketConnectOpts>`, so
  `checkInheritedPropertiesAreIdentical` declines (`members.rs`).
- **Distinct enums under bivariance** (`enumAssignmentCompat7`). The port
  does not report `a = b` for `(p: first.E) => void` and
  `(p: second.E) => void` either, so this is a relater gap, not a heritage
  one.
- **Generic mapped base** (`indexSignatureAndMappedType`, extra TS2430):
  `Record<T, string>` vs `{ [key: string]: string }` is the relater's
  generic-mapped arm (`tsr-2zk.977`).
- **`extends` a value whose construct signature returns an intersection**
  (`genericDefaults`, extra TS2415 at `Derived03 extends Base02`). The base
  is `Base02 & A` from `Base02Constructor`'s construct signature. The
  derived declared type does not inherit `a` from that intersection base
  (`declared.rs`/`base_types.rs`).
- **`extends null` merged with an interface** (`classExtendsNull2`, TS2417
  against `null`). Native's base list is non-empty only through the
  interface's `extends Base`. The static-side walk in `check.rs` is a
  property-by-property port that has no `null` target.

## 5. Held diff: the broad report's weak-target arm (TS2559)

[r5-heritage2-weak-broad-report.diff](r5-heritage2-weak-broad-report.diff)
(`assignreport.rs` visibility, r5-report2's file; and this lane's
`issue_member_specific_error`).

When no member reports, `issueMemberSpecificError` ends with
`checkTypeAssignableTo(typeWithThis, baseWithThis, name, broadDiag)`. The
relation's first failure for a weak target with no common property is
`isRelatedToEx`'s common-property check (`relater.go:2676`). It reports
TS2559/TS2560 through `reportError`, not `reportRelationError`, so the head
message is never applied and the whole diagnostic is TS2559 at the class
name. The diff makes `report_weak_type_failure` `pub(crate)` and calls it
before the broad diagnostic, as `report_relation_failure` already does for
assignments.

Measured on top of commit 1, both dumps unfiltered: diagnostics +1
(`subtypingWithObjectMembers5`, three TS2420 → TS2559), types unchanged, no
losses.

The rest of `reportRelationError` is not ported for this report. Native's
missing-property suppression (`relater.go:4816`) keeps the head for the
implements messages (`isConversionOrInterfaceImplementationMessage`), but
for TS2415 and TS2430 it gives way to the TS2741/TS2739 chain. No case in
the population needs that yet.
