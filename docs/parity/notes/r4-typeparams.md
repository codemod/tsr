# Lane notes: r4-typeparams (`tsr-2zk.901`, `tsr-2zk.913`, `tsr-2zk.911`)

Round-4 cloud lane. Native source is `vendor/typescript-go` @ `5b1047d`
(`internal/checker/checker.go` unless noted). Baselines are this box's frozen
dumps at `7b4e6a2` (types 470004 RIGHT / 7020 WRONG / 955 GAP; diagnostics
4264 RIGHT / 1238 WRONG / 4970 EMPTY_RIGHT / 98 EMPTY_WRONG).

## 1. Merged local type parameters (`tsr-2zk.901`) and class `@template` (`.16.106`) — measured, held

**Native.** `getLocalTypeParametersOfClassOrInterfaceOrTypeAlias`
(`checker.go:23806`) walks **every** declaration of the symbol that is an
interface, class, class expression or type alias (`isTypeAlias`, so the
reparsed JSDoc typedef/callback too) and appends each declaration's
`TypeParameters()` with `appendTypeParameters` (`:23822`), which is
`core.AppendIfUnique` on the parameter's declared type. A class or interface
files its type parameters in its own members table, so merged declarations
share one symbol per parameter name: `class Component<P, S>` merged with
`interface Component<P = {}, S = {}, SS = any>` answers `P, S, SS`. For a JS
class without written parameters the reparser has already put
`gatherTypeParameters(jsDoc, false)` of the **last** comment on the node
(`parser/reparser.go:459-470`, hosted tags only on the last comment, `:54`),
and `gatherTypeParameters` concatenates every `@template` tag of that comment
(`:293`).

**TSR before.** `declared.rs` `local_type_parameters_of` read the first
declaration only and returned a borrowed `&'a [&'a TypeParameterDeclaration]`,
which cannot hold a concatenation.

**The change (held as `r4-typeparams-merged-parameters.diff`).** The
function returns `Cow<'a, [&'a TypeParameterDeclaration<'a>]>`: borrowed
whenever one declaration (or one `@template` tag) supplies the whole list,
owned only when a second contributor has to be appended. Dedup key is the
parameter's merged symbol (the declared type is one per merged symbol). The
same change ports the class-host `@template` arm r4-jsdoc delivered as
`r4-jsdoc-class-template.diff`, without its "several tags keep the old
answer" limitation, and makes the typedef arm concatenate every `@template`
tag as native does. Call sites compile unchanged except two in `declared.rs`
(a slice pattern and a `for` loop need `*`/`.iter()`).

**The witness was already RIGHT here.** `reactDefaultPropsInferenceSuccess:0:81`
is RIGHT at `7b4e6a2`: in `react16.d.ts` the interface (with the defaults) is
declared before the class, so first-declaration reading happens to take the
list with defaults. The issue's class-first shape still diverges.

**Measured** (unfiltered, against the frozen baseline):

| Set | types | diagnostics | losses |
|---|---|---|---|
| merged list alone | +126 RIGHT | +3 cases (privacyCheckExportAssignmentOnExportedGenericInterface1, nonPrimitiveInGeneric, nonPrimitiveStrictNull) | 1 diag case, 6 type lines |
| + `r4-typeparams-alias-declaration.diff` | +140 RIGHT | +3 cases | 6 type lines |

The class-template part alone converts r4-jsdoc's list exactly
(jsdocClassMissingTypeArguments 3, jsdocTemplateTag6 11, extendsTag1 1,
extendsTag5 1, jsdocTemplateTag7 1, overloadTag3 1, jsFileMethodOverloads 1).

**Every loss is a coincidence the change unmasks:**

| Loss | Native | With change | Unported piece it exposes |
|---|---|---|---|
| interfaceExtendsObjectIntersection (diag, 11× TS2507) | none | TS2507 | `type Constructor<T> = …; declare function Constructor<T>()`: the binder (like native `bindEachFunctionsFirst`) lists the function first. ~30 alias readers take `declarations.first()` as the alias declaration; native uses `core.Find(symbol.Declarations, IsEitherTypeAliasDeclaration)` (`getDeclaredTypeOfTypeAlias`, `:23845`). Before, the first declaration (the function) gave `[]`, so the alias was "non-generic" and its reference silently became `error`. Fixed by `r4-typeparams-alias-declaration.diff` (new `type_alias_declaration_of` + the call sites in `declared.rs`, `mapped.rs`, `members.rs`, `signatures.rs`, `string_mapping.rs`, `templates.rs`, `constraints.rs`), which also makes `declare const c: Constructor<I1>; const v: number = new c()` report native's TS2322 (the baseline reports nothing). |
| genericDefaults:0:1052/1053/1057 | `A` | `any` | `interface i07 { a: A; } interface i07<A = number> { b: A; }`: the first declaration's `A` is the global interface (native `isTypeParameterSymbolDeclaredInContainer`, ported in the binder). Now that `i07` is generic, `instantiate_type`'s **name-based** `mentions_type_parameter` (`inference.rs`) sees the global `A` mention the parameter name `A` and instantiates it to error. Native substitutes by type identity. |
| complexRecursiveCollections:1:910/916/924 | `Collection.Indexed<Z_1>` | `Indexed<Z_1>` | `Immutable.Collection` is functions + namespace + interface; the functions come first, so it was "non-generic" before. The renamed (`Z_1`) clone of the overloads now re-prints its return through the qualified-generic mint (`qualified_type_reference`, NB-SYMBOL-CHAIN, `tsr-2zk.39`), which drops the namespace qualifier. |

**Performance — the second blocker.** `local_type_parameters_of` is called
far more often than native's once-per-symbol computation (625,729 calls with
two or more declarations on a one-line file: every `Array`/`Promise`-like lib
symbol is interface + var). Callgrind on
`generate_perf_project.py --modules 100`: `local_type_parameters_of`
inclusive 25.2M Ir → 148.7M Ir, total +3.0% Ir (4,220.6M → 4,352.2M; the
alias sweep is included in that binary). Native stores the list on the
declared type (`InterfaceType.localTypeParameters`); the port needs the same
memo — a `Checker` field keyed by `SymbolId`, published once per symbol,
which is a hub (`checker.rs`) edit this lane does not own. Median child CPU
of that binary against the baseline: domain-model 1.066 at 21 samples, 1.000
at 41; generic-imports 0.992 / 1.018. The bench projects pass the CPU gate
only within its noise band; the deterministic Ir is the number that blocks.

**Reopening condition.** Land the per-symbol memo, the alias-declaration
sweep, and identity-based mention in `instantiate_type`; then apply
`r4-typeparams-merged-parameters.diff` and expect +140 type lines, +3
diagnostics cases, with complexRecursiveCollections' three lines left for the
symbol-chain lane.

**`getDefaultFromTypeParameter`.** Native takes a parameter's default from
the first declaration of the parameter symbol that has one
(`getResolvedTypeParameterDefault`, `:22007`). TSR's readers
(`signatures.rs` `type_parameter_of`, `node_reuse.rs`, `declared.rs`
arity/default code, `type_argument_arity.rs`) read `default_type` off the
one node in the list, which after the merge is the parameter's first
declaration. For `class C<P>` declared before `interface C<P = {}>` that
misses the default. Not ported: every reader is outside this lane; the
faithful shape is one helper (first declaration with a default) that each
reader calls.

## 2. TS2313 for a mapped constraint over its own parameter (`tsr-2zk.913`) — committed

**Native.** `T extends { [P in T]: number }`. `getResolvedBaseConstraint(T)`
pushes `T`, and `getConstraintFromTypeParameter(T)` builds the constraint
node's type. `getTypeFromMappedTypeNode` (`:24255`) creates the mapped type
and **eagerly** calls `getConstraintTypeFromMappedType` (`:22678`), which is
`getConstraintOfTypeParameter(P)`, guarded by `hasNonCircularBaseConstraint`
(`:17066`): `getResolvedBaseConstraint(P)` pushes `P`, `P`'s constraint is
`T`, and `getResolvedBaseConstraint(T)` fails its push. Both frames then fail
their pop and both report TS2313 at their constraint declarations (2:24 and
2:32 in `incorrectRecursiveMappedTypeConstraint`); `T`'s resolved base is
`circularConstraintType`, so `getConstraintOfTypeParameter(T)` is nil.

**TSR before.** The resolution-stack machinery (`base_constraint_of_type`,
`report_circular_constraint`) was already native's, but TSR mints a mapped
type per evaluation with no first-creation event, so nothing resolved `P`'s
base constraint inside `T`'s frame and no cycle was seen.

**Port.** `compute_base_constraint`'s type-parameter arm first runs
`resolve_constraint_mapped_type_parameters`: for a declared (not
instantiated) parameter it finds `getConstraintDeclaration`'s node (first
declaration with a constraint, `:29132`) and walks the constituents
`getTypeFromTypeNode` resolves eagerly — parentheses, type operators, arrays,
unions, intersections, indexed accesses, type-reference arguments — calling
`base_constraint_of_type` on each mapped type's parameter. It does not enter
type-literal or signature members or a mapped template, which native resolves
lazily.

**Convention record.** No new cache or table: the step reuses
`base_constraint_cache` and the `ResolvedBaseConstraint` resolution frames.
Pinned operation: the eager `getConstraintTypeFromMappedType` call in
`getTypeFromMappedTypeNode`. Work boundary: once per computed base constraint
of a declared type parameter (the base-constraint cache key), one declaration
scan and one syntactic walk of the constraint node. Measured: callgrind on the
100-module project 4,220,558,549 → 4,221,207,292 Ir (+0.015%).

**Accepted divergences.** Native runs the eager step only on the first
evaluation of each mapped node; TSR runs it whenever a declared parameter's
base constraint is computed. Both find the cycle in either order (whichever
of `T` or `P` is entered first, the other is reached inside its frame), so
the reported set is the same. A mapped type reached only through a
conditional type's check/extends operands inside a constraint is not walked.
The related "Circularity originates in type at this location" information is
not carried (it was not before either).

**Converted.** Diagnostics: incorrectRecursiveMappedTypeConstraint now has
both TS2313 (it stays WRONG on the missing TS2365, §3). Alone the commit
changes no verdict and has zero losses.

## 3. r4-operators' held `+` patches re-measured on top of §2

`r4-operators-plus-type.diff` + `r4-operators-zero-literals.diff` (branch
`claude/beautiful-shannon-ar5gh0-r4-operators`) applied on §2: **+24 type
lines, 8 cases fully RIGHT** (additionOperatorWithConstrainedTypeParameter 6,
operatorsAndIntersectionTypes 6, typeParameterExtendsPrimitive 1,
plainJSRedeclare3 2, spellingUncheckedJS 2, tsNoCheckForTypescript 2,
tsNoCheckForTypescriptComments1 2, tsNoCheckForTypescriptComments2 2;
keyofAndIndexedAccessErrors +1 line), **zero losses** in either check. The
reopening condition r4-operators recorded is met. One gap remains in the
witness: the patched `check_addition` (`binary.rs`) answers `any` where
native reports TS2365 and answers `any`, but does not call
`report_operator_error`, so incorrectRecursiveMappedTypeConstraint's
diagnostics case stays WRONG on that one TS2365. That wiring is the operators
lane's.

## 4. r4-jsdoc's patch set re-measured with §1 (`tsr-2zk.911`, stretch)

Measured on the merged head `b2beae4` (integration `0cd6c43` + §2), against
that head's own dumps (types 470069 RIGHT; diagnostics 4287 RIGHT / 4976
EMPTY_RIGHT). Applied: `r4-jsdoc-scope-hop.diff`,
`r4-jsdoc-property-type.diff`, and §1's two diffs in place of
`r4-jsdoc-class-template.diff` (which §1 subsumes).

**Result:** +187 type lines RIGHT (470248), diagnostics +8 cases RIGHT
(checkJsdocTypeTagOnExportAssignment1/4/6, checkJsdocSatisfiesTag9,
jsDeclarationsInheritedTypes, and §1's three), 2 cases EMPTY_RIGHT →
EMPTY_WRONG, 8 type lines RIGHT → WRONG.

**All four r4-jsdoc losses remain**, each for the reason r4-jsdoc recorded;
none involves type parameters, so §1 cannot clear them:

| Loss | Still needs |
|---|---|
| jsdocImportType:0:8 | `getTypeFromJSDocValueReference` for a `@type` naming a `require` value |
| importTag24:1:18 | node reuse of a `@returns {Foo}` annotation in the signature printer (`node_reuse.rs`) |
| checkJsdocTypeTagOnExportAssignment8 (diag) | `getContextualType`'s `KindExportAssignment` arm for a hosted `@type` (`contextual.rs`) |
| expandoFunctionContextualTypesJs (diag) | contextual type of an expando `F.p = {...}` assignment (`contextual.rs`) |

The other six type losses are §1's own (genericDefaults ×3,
complexRecursiveCollections ×3).
