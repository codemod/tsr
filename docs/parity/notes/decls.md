# Parity lane `decls` — declaration, merge and heritage checks

Lane issue `tsr-2zk.3` (epic `tsr-2zk`). Case list:
`docs/parity/lanes/decls.txt`. Box protocol: `docs/parity/box-protocol.md`.
Pinned upstream: `vendor/typescript-go` @ `5b1047d`.

This file records the lane's judgment calls, numbered so code comments can cite
them (`docs/parity/notes/decls.md` §N).

## §1 TS2403: an unannotated, uninitialized `var` is a written `any`

`check_subsequent_declaration_type` (TS2403, `checkVariableLikeDeclaration`'s
secondary-declaration arm, `checker.go:5928`) declines a top-level `any` or
`unknown` operand unless the declaration *wrote* it, because in this port `any`
is often "no better answer" (§338 / §865 of `checker-notes-diag2.md`: admitting
`any` unconditionally measured −34 cases).

**Forcing constraint.** `var y = ""; var y;` is TS2403 upstream
(`duplicateVariablesWithAny`, `varBlock`): the second declaration's type is
`any` because `getTypeForVariableLikeDeclaration` has nothing else to read —
no annotation, no initializer. (Under `noImplicitAny` it answers `autoType`,
which the secondary arm turns back into `any` via `convertAutoToAny`,
`checker.go:5932`.) That `any` is as certain as a written one.

**Decision.** A `VariableDeclaration` with neither annotation nor initializer,
whose list belongs to a `VariableStatement`, counts as written `any`. A
`for (var x in …)` / `for (var x of …)` declaration also has neither but takes
its type from the loop, so it does not qualify.

**Measured.** Removing the trust gate entirely (an experiment, not shipped) at
`0d996e8` gained 3 cases and lost 3 (`variableDeclarationInStrictMode1`,
`constructorParameterProperties`, `objectLiteralGettersAndSetters`). Each loss
was a wrong `any` from another subsystem: an unmerged-merge symbol (fixed here,
§3), private-member access on an instantiated class, and setter-contextual
parameter typing. The narrow rule keeps the gains and none of the losses.

**Falsifier.** A corpus case where an unannotated, uninitialized
variable-statement `var` has a non-`any` type upstream would show up as a new
extra TS2403.

## §2 TS2403: a ported identity relation, structural only between annotations

`crate::identity` ports `isTypeIdenticalTo` (`relater.go:119`) as a
three-valued walk: flags equality and singletons (`isTypeRelatedTo`,
`isRelatedTo` under `identityRelation`), unions and intersections both ways,
same-target type references by type arguments (independent parameters skipped),
then `propertiesIdenticalTo` / `compareProperties`, `signaturesIdenticalTo` /
`compareSignaturesIdentical` for call and construct signatures, and
`indexSignaturesIdenticalTo`. It replaces the earlier "identity fragment",
which approximated the object arm by mutual assignability.

**Why a separate walk and not a `Relation::Identity` in `crate::relater`.**
The identity arm shares none of assignability's relaxations (apparent types,
optionality, excess properties, discriminants, the `any`-index rule); its
structural comparison is symmetric and exhaustive. Threading a fifth relation
through every arm of the relater is more code than the walk and touches a file
another lane owns. What would change this: a second consumer needing identity
inside relation recursion (e.g. `Unmeasurable` variance in
`typeArgumentsRelatedTo`, or `compareTypesIdentical` for overload checks),
where sharing the relater's cache would matter.

**Accepted limitation: structural identity only between written types.** The
first full-identity build at `0d996e8` gained 12 cases and lost 7, every loss a
negative between *inferred* initializer types that another subsystem builds
imprecisely:

| loss | operand built wrong | owner |
|---|---|---|
| `arrayLiteralWidened` | `[null, null]` not widened to `any[]` under `strict: false` | widening |
| `overloadBindingAcrossDeclarationBoundaries{,2}` | overload chosen `Opt3` vs `Opt1` | calls |
| `typeRelationships` | `[this, this.c]` not subtype-reduced to `C[]` | unions |
| `for-inStatements{,Invalid}` | flags-differ arm declined for conditional types (fixed by restoring the flag test) | — |
| `variableDeclarationInStrictMode1` | `var eval` merged into `lib`'s `eval` (§3) | binder |

So `check_subsequent_declaration_type` uses the structural arm only when both
declarations carry a type annotation; otherwise it keeps the fragment's
necessary condition (mutual assignability) via
`is_type_identical_to_by_assignability`. This is a trust rule of the same kind
as §1, not a port of anything upstream: upstream has one relation. **It goes
away** when those three subsystems produce upstream's types; the falsifier is
running the structural arm unconditionally and seeing no losses.

**Also declined:** a flags difference involving an enum (one enum has two
representations here; even object-vs-enum declines because a qualified
`M3.Color` annotation can resolve to an object-flagged type —
`instantiatedModule` measured a loss when it was allowed — narrowed in round
5 to that misresolved enum image, `r5-constraints2.md` §3); distinct type
parameters that share a declaring symbol or were freshly minted by
`instantiate_signature_with_fresh_parameters`; type predicates
(`compareTypePredicatesIdentical` unported); index, template-literal and
string-mapping identity arms.

**Port boundary.** No cache or side table; an assumption stack plays
`maybeKeys` and a flat depth bound (40) declines. The only consumer runs per
secondary `var` declaration.

## §3 TS2403: a merge the excludes forbid leaves the declaration unmerged

Upstream's `mergeSymbol` reports `reportMergeSymbolError` and does **not**
merge (`checker.go:14199`), so `getSymbolOfDeclaration(var eval)` in a strict
script is the file's own symbol, whose only value declaration is itself. This
port's binder (`Binder::merge_globals`, `crates/tsr-binder/src/binder.rs:830`)
inserts `merged[source] = target` *before* testing the excludes, so the source
still resolves to `lib.d.ts`'s `function eval`, and TS2403 compared the `var`
against the function. The checker now treats a symbol recorded in
`merge_conflicts()` as unmerged for this rule.

**Needed outside this lane:** the binder should not record the `merged` edge
for a conflicting pair. That is the root cause; the check-side test is local to
TS2403 and every other consumer of `merged_symbol` still sees the bad edge.

## §4 TS2394: the overload/implementation relation, with a local parameter loop

`check_function_or_constructor_symbol` ported only the implementation-presence
arms of `checkFunctionOrConstructorSymbol`; TS2394 came from a separate
heuristic (`check_overload_implementation_return`) that compared *written
primitive return keywords* of same-arity function declarations. It is replaced
by the upstream block (`checker.go:3697-3707`): every body-less declaration's
signature, in order, against the body's, reporting the first for which
`isImplementationCompatibleWithOverload` (`checker.go:3716`) fails. Constructors
and methods now take part (`parserClassDeclaration12`,
`constructorsWithSpecializedSignatures`, `parserParameterList16/17`).

**The parameter loop is local.** `isSignatureAssignableTo(erasedImpl,
erasedOverload, ignoreReturnTypes)` is `compareSignaturesRelated`. The relater
has that loop (`one_signature_related_to`), but only behind a pair of
signature-bearing *types*, and `is_pure_signature_type` admits only
function-expression, function-type, method and signature declarations. A type
minted from a function declaration or constructor signature therefore answers
`Unknown` (relater reason row 3, "no members table" — measured with
`relater::reasons` on `functionOverloads18`). So
`signature_parameters_assignable` ports the arity test, `this` types and the
parameter loop with upstream's `strictVariance` rule. It declines (a) a
non-array rest on either side and (b) the callback arm: a callable parameter
pair is decided only when both directions agree.

**What would change this.** If the relater exposes signature comparison
(`compareSignaturesRelated` with a check mode) or admits `FunctionDeclaration`
and `Constructor` in `is_pure_signature_type`, the local loop should go and
the callback arm comes for free. That is a change in `crate::relater`, which
this lane does not own.

**Falsifier.** A new extra TS2394 on an overload whose parameters relate only
through the callback arm or a tuple rest.

## §5 TS2411: index constraints use the assignability gate, not the general one

`check_index_constraint_for_property` / `…_for_index_signature` asked
`pair_is_reportable`, which adds an *enum veto* on top of
`assignability_pair_is_reportable`. That veto exists for rules whose own enum
handling is unported (comparisons, operators); `checkIndexConstraintForProperty`
is a plain `isTypeAssignableTo(propType, info.valueType)`, and the relater's
enum arms are ported. Lifting it reports 13 of the 16 TS2411 lines in
`enumIsNotASubtypeOfAnythingButNumber` (the other 3 are an enum against an
array, an interface and `typeof f`, where the relater still answers
`Unknown`). No loss in the full run.

## §6 TS2564 on an error-typed property: declined, blocked on resolution

Upstream skips `checkPropertyInitialization` for a property whose type has
`TypeFlagsAnyOrUnknown`, and `errorType` carries `Any` — so `x: A` with `A`
generic and unargumented (TS2314), or `z: W` with `W` unresolved (TS2304), never
gets TS2564. This port reports there unless the name is computed (§324 of
`checker-notes-diag2.md`), producing extra TS2564 in lane cases such as
`genericReturnTypeFromGetter1`, `genericsWithoutTypeParameters1` and
`typeParameterUsedAsTypeParameterConstraint4`.

Following upstream (skip every error-typed property), measured on the tree of
this lane's second commit, converted eight cases —
`decoratorMetadataNoLibIsolatedModulesTypes`, `decoratorMetadataTypeOnlyImport`,
`genericReturnTypeFromGetter1`, `genericsWithoutTypeParameters1`,
`metadataImportType`,
`ClassAndModuleThatMergeWithModuleMemberThatUsesClassTypeParameter`,
`typeParameterUsedAsTypeParameterConstraint4`, `parserRealSource6` — and lost
`decoratorMetadataWithTypeOnlyImport2`: there `field: Services.Service` names a
class through `import type { Services }`, which this port fails to resolve, so
its "error type" is a resolution gap rather than an upstream error. **Not
shipped.** Unblocked by resolving namespace members through a type-only
import (names lane); then the `computed_name` condition goes.

## §7 TS2813/TS2814: a function merged with a non-ambient class

`check_function_or_constructor_symbol` declined every symbol with a class
declaration, because upstream's `hasNonAmbientClass` arm (`checker.go:3660`)
was unported and running the rest of the walk without it gave wrong lines in
the `ClassAndModuleThatMerge…` family (`checker-notes-diag2.md`). The arm is
now ported: when a non-ambient class declaration is among the declarations of
a `Function`-flagged symbol (and the symbol is not a constructor), every class
declaration gets TS2813 and every function declaration TS2814, at its name.
Class *expressions* still decline the symbol (they do not merge by name
upstream, so their presence means the binder merged something upstream did
not).

The "all declarations share one parent" bound now admits one more shape: every
declaration an `export`ed member of a block of the same merged namespace.
Upstream's binder merges exactly those (`declareModuleMember` → the namespace's
`exports`), so `namespace M { export function f() {} } namespace M { export
class f {} }` is one symbol upstream too (`duplicateIdentifiersAcrossContainerBoundaries`).
Locals of two blocks and a class body beside a namespace block stay declined.

**Measured** (full run): +14 cases, no loss — `augmentedTypesClass2a`,
`augmentedTypesFunction`, `callOverloads1`–`5`, `classOverloadForFunction{,2}`,
`funClodule`, `nameCollisions`, `staticClassMemberError`,
`multipleExportDefault5`, `duplicateIdentifiersAcrossContainerBoundaries`.

## §8 TS2717 and TS2687: the rest of `checkVariableLikeDeclaration`'s merge arms

The TS2403 rule (§1–§3) now runs for property declarations and property
signatures too, as upstream's `checkVariableLikeDeclaration` does
(`checker.go:5893-5935`): a secondary declaration whose widened type is not
identical to the symbol's reports TS2717 (`errorNextVariableOrPropertyDeclarationMustHaveSameType`
picks the property message), and the TS2687 arms — `areDeclarationFlagsIdentical`
(optionality plus private/protected/async/abstract/readonly/static) on the
primary against every other variable-like declaration and on each secondary
against the primary — are ported beside it.

**One upstream special case is mirrored as a decline.**
`widenTypeForVariableLikeDeclaration` (`checker.go:18246`) turns a
`symbol`-typed member of the global `SymbolConstructor` into the member's
`unique symbol` (typescript-go#1212), so `readonly observer: symbol` merged with
`readonly observer: unique symbol` is identical upstream. This port's widening
(`crate::symbols`, not this lane's) lacks it, so the identity check declines
a symbol-typed pair under the global `SymbolConstructor`; the first measurement
without that decline lost `symbolObserverMismatchingPolyfillsWorkTogether`.
The proper fix is the special case in the widening.

## §9 TS2699: static members that collide with `Function`'s own properties

Ported both upstream sites, skipped in an ambient context as
`checkClassLikeDeclaration` does (`checker.go:4308`):
`checkObjectTypeForDuplicateDeclarations`' `prototype` arm (any static member
named `prototype`, `checker.go:3184`) and
`checkClassForStaticPropertyNameConflicts` (`name`/`length`/`caller`/`arguments`
unless `useDefineForClassFields`, read from `standard_class_fields`).

`check_merged_namespace_prototype` used to report TS2300 for a class's own
`static prototype`, because this port keeps static members in the class
symbol's `exports`, where that rule looks for a namespace's exported
`prototype`. Upstream binds static members after `bindClassLikeDeclaration`'s
check, so a static member never meets it; the rule now skips a declaration
whose parent is a class. The binder-side collision that *does* exist upstream —
a static method or accessor named `prototype` against the minted `prototype`
property (`MethodExcludes`/accessor excludes include `Property`) — is reported
beside TS2699, since this port's binder never mints that symbol.

Measured: `propertyNamedPrototype`, `staticPropertyNameConflictsInAmbientContext`
converted, no loss.

## §10 TS2423/TS2425/TS2426: methods in `checkKindsOfPropertyMemberOverrides`

`check_override_kind` ported only the property/accessor pair (TS2610/TS2611)
of `checkKindsOfPropertyMemberOverrides` (`checker.go:4626`), reading
declaration kinds syntactically along the base chain. The method arms are now
in the same walk: a base method overridden by an accessor (TS2423), a base
accessor or property overridden by a method (TS2426, TS2425); a method
overridden by a property stays the one legal mixed override. Methods now also
count as the nearest base member, which is what `getPropertiesOfType(baseType)`
answers — previously a method in an intermediate class was skipped and the
search continued to its ancestors.

Measured: +6 cases (`inheritance`, `inheritanceMemberFuncOverridingAccessor`,
`inheritanceMemberFuncOverridingProperty`, `multipleInheritance`,
`accessorsOverrideMethod`, `derivedClassFunctionOverridesBaseClassAccessor`),
no loss. The arguments keep the existing rule's simplification of printing the
class *names* (`TypeToString` of a generic class would print its type
parameters).

## §11 TS2320: inherited-property identity uses the identity relation

`checkInheritedPropertiesAreIdentical` compares same-named members of two
bases with `compareProperties(…, compareTypesIdentical)`. The port's
`is_property_identical_to` approximated the type comparison by mutual
assignability, which cannot separate `f(x: any): any` from `f<T>(x: T): T`
(each assignable to the other). It now asks `is_type_identical_to` (§2): base
member types are declarations' written types, the kind §2 trusts
structurally. Measured: `genericAndNonGenericInheritedSignature1` and `2`
converted, no loss.

## §7a TS2813/TS2814 now come from `crate::class_function_merge` (integration)

At the merge with `main` (2026-10-05), two ports of
`checkFunctionOrConstructorSymbolWorker`'s class-merge arm met: this lane's,
inside `check_function_or_constructor_symbol`, and main's standalone
`class_function_merge.rs`. Both reporting doubled every TS2813/TS2814. This
lane's arm sat behind the worker's single-file bound and was reached only from
function-like declarations, so it missed a class and a function merged across
files (`duplicateIdentifiersAcrossFileBoundaries`); main's runs from both
declaration kinds with a per-declaration ambient test. The integration kept
main's module as the only port and removed this lane's arm; every case either
side had RIGHT (`callOverloads1`–`5`, `classOverloadForFunction(2)`,
`funClodule`, `augmentedTypes*`, `nameCollisions`,
`duplicateIdentifiersAcross*Boundaries`, `staticClassMemberError`) is RIGHT.

## §12 TS2300 on duplicate members: `checkObjectTypeForDuplicateDeclarations` on binder symbols

Duplicate property members were reported by two name-comparison rules
(`checker-notes-diag2.md` §904/§906/§912/§914): one for type literals and
interfaces that matched identifier and string names only, one for classes that
re-normalised numeric spellings itself. Both were gated on the file having no
parse errors, and neither saw parameter properties or class expressions.

Upstream needs no name comparison: `PropertyExcludes` does not contain
`Property`, so `declareSymbol` merges two same-named properties into one
symbol with two declarations, and `checkObjectTypeForDuplicateDeclarations`
(`checker.go:3142`) walks one declaration's members asking `len(symbol.Declarations) > 1`
with its property/accessor state machine, then `reportDuplicateMemberErrors`
reports every member (and parameter property) of that declaration carrying
the symbol. The port now does exactly that, for classes, class expressions,
interfaces and type literals (this port's binder does give a type literal its
`__type` symbol and members table; the old comment saying otherwise was
stale). The binder already canonicalises numeric names (`1` and `1.0` are
both `"1"`), so that normalisation is no longer re-derived in the checker.

**No parse-error gate.** Upstream reports these in files with syntax errors
(`numericNamedPropertyDuplicates` has a TS1005 and four TS2300 pairs).

**Not ported:** the private-name arm (`Duplicate_identifier_0_Static_and_instance_elements_cannot_share_the_same_private_name`).
Late-bound duplicates (`[Symbol.isConcatSpreadable]` twice, `symbolProperty37`/`44`)
are `lateBindMember`'s report, not this walk's.

Measured: +6 (`parameterPropertyInConstructor2`, `staticModifierAlreadySeen`,
`numericNamedPropertyDuplicates`, `objectTypeWithDuplicateNumericProperty`,
`parser0_004152`, `stringNamedPropertyDuplicates`), no loss.

**Falsifier.** An extra TS2300 on a member whose symbol the binder merged
where upstream would not (a members-table merge across a conflict) — the
report trusts the binder's declaration lists.

## §13 TS2411: `checkTypeLiteral`'s index-constraint call

`check_index_constraints` ran for classes and interfaces only; upstream's
fourth call site is `checkTypeLiteral` (`checker.go:3134`), which checks the
literal's own type against its index signatures with the `__type` symbol as
owner. The port's binder gives a type literal that symbol (§12), and
`declared_in_owner` already reads a member's parent symbol, so the error node
resolves to the literal's member exactly as for an interface. No new cache:
the literal's type is `get_type_from_type_node`'s, which is already cached by
node.

Measured: +4 (`propertiesAndIndexers`, `stringIndexerConstrainsPropertyDeclarations2`,
`genericCallWithObjectTypeArgsAndIndexersErrors`, `recursiveTypesWithTypeof`),
no loss, no new extra TS2411.

## §14 Binder: TS2528 is chosen by the declaration, not by the name `default`

`declareSymbolEx` (`binder.go:224-244`) picks
`A_module_cannot_have_multiple_default_exports` when the conflicting
declaration `isDefaultExport` (a `default` modifier, or an export specifier
named `default`) or is a non-`export =` export assignment. The port tested
`name == "default"` instead, arguing nothing else could be filed under that
name. Parser recovery can: `import { default } from "m"` and
`import { yield as default }` bind locals named `default`, which collide with
each other and upstream reports TS2300 on them. `declare` now records whether
the declaration it binds is a default export in upstream's sense, and the
conflict branch reads that.

Measured: +1 (`es6ImportNamedImportIdentifiersParsing`), no loss.

## §15 Binder: the local half of an exported member is tested with the declaration's excludes

`declareModuleMember` (`binder.go:406`) declares an exported member twice:
a local carrying only `ExportValue` (or nothing, for a type), and the export.
Both calls pass the declaration's `symbolExcludes`. The port derived the
local's excludes from its flags — `ExportValue` or empty — so the local half
collided with nothing, and `class Box {}` followed by `export type Box;`
(a type alias, `TypeAliasExcludes = Type`) merged silently instead of
reporting TS2300/TS2567 on both. The local is now declared with
`flags.excludes()`, the same mask the export half uses.

Measured: +1 (`exportDeclaration_missingBraces`), no loss. Four extra TS2300
lines appear in `ambientModuleDeclarationWithReservedIdentifierInDottedPath`
and `…2`, both already WRONG: the parser fails on `namespace chrome.debugger`
(an extra TS1359), so the namespace's `declare var tabId` lands at file scope
and does collide with `export const tabId`. Upstream parses the dotted name;
the conflict disappears with the parser fix (parser lane).

**Falsifier.** A new extra TS2300/TS2451 between an exported declaration and
a same-named local in a correctly parsed file.

## §16 Binder: the `merged` edge on a conflicting merge stays (measured, refused)

Round 1 asked the binder not to record `merged[source] = target` when the
excludes forbid a merge (§3), as upstream's `mergeSymbol` never reaches
`recordMergedSymbol` on that arm. Built and measured at `188e64f`: no verdict
change, but two line regressions in WRONG cases — `recursiveComplicatedClasses`
lost its TS2507 at `extends Symbol` (17,31) and
`controlFlowFunctionLikeCircular1` gained an extra TS2448.

**Why.** Upstream's `resolveName` skips a script's `SourceFile` locals (they
were merged into `globals`), so `extends Symbol` in a script declaring
`class Symbol` still reaches `lib.d.ts`'s `var Symbol: SymbolConstructor`.
This port's resolver reads a script's file locals and relies on the `merged`
redirect to land on the global; without the edge the reference resolves to
the conflicting class itself. **Not shipped.** The edge can go once name
resolution stops consulting script-file locals (reported to the integrator);
until then §3's check-side test stays.

## §17 TS2564 on an error-typed property: shipped, bounded to a failed reference (supersedes §6)

§6 declined upstream's skip of an `errorType` property in
`checkPropertyInitialization` because a type-only import failed to resolve
(`decoratorMetadataWithTypeOnlyImport2`). The names lane has since resolved
type-only import clause names, and following upstream no longer loses it.

Dropping §324's "computed name only" bound outright, though, lost eight TS2564
lines in `missingTypeArguments1` and `returnTypeTypeArguments`: `p3: X3[]`
and `p4: I<X4>` (with `X3`/`X4` missing type arguments) are `Array<errorType>`
and `I<errorType>` upstream — objects, so TS2564 is reported — but this port
carries a nested `error` up as a gap (`get_type_from_array_type_node`: "a gap
in the element is a gap in the array"), so the property's type reads as
`error` too. The skip therefore applies only when the annotation **is** the
failed reference — a type reference, `import()` type or `typeof` query whose
own type arguments all resolved — which is exactly where
`getTypeFromTypeReference` / `getTypeFromImportTypeNode` /
`getTypeFromTypeQueryNode` answer `errorType`. A union or other composite
annotation that contains an error keeps reporting (upstream's union with
`errorType` is `errorType`, so that is a remaining decline, not a port).

Measured: +10 (`decoratorMetadataNoLibIsolatedModulesTypes`,
`decoratorMetadataTypeOnlyImport`, `genericReturnTypeFromGetter1`,
`genericsWithoutTypeParameters1`, `metadataImportType`, `missingTypeArguments1`,
`returnTypeTypeArguments`,
`ClassAndModuleThatMergeWithModuleMemberThatUsesClassTypeParameter`,
`parserRealSource6`, `typeParameterUsedAsTypeParameterConstraint4`), no loss;
TS2564 extra lines 15 → 1 (`circularIndexedAccessErrors`, a circular
indexed-access annotation).

**What would remove the bound.** The gap producers answering upstream's types
(`Array<errorType>`) instead of `error` (§3a, `tsr-2zk.31`); then the test is
`is_error(declared)` alone.
