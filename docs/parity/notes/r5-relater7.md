# r5-relater7 — relater arms, unique symbols, held work (`tsr-2zk.1088`, `.1055`, `.1065`, `.1068`)

Lane under epic `tsr-2zk`, successor to r5-relater6 ([`r5-relater6.md`](r5-relater6.md)).
Owns `crates/tsr-checker/src/relater.rs`, `index_access_reports.rs`, the
binder's declare-module import binding (item 5 only), the tests for its items
(`crates/tsr-checker/tests/relater7_arms.rs`) and this file. Native anchors are
`vendor/typescript-go` @ `5b1047d`.

## 0. Frozen base

`0fb3e6c`: the integration head `d57fffe` (batch AD) with r5-relater6's branch
tip `dc54b2c` merged in. r5-relater6's commits land in batch AF, which was not
on `claude/beautiful-shannon-ar5gh0` when this lane started, so the lane
builds on them directly.

- `diagverdictdump`: RIGHT 5405, EMPTY_RIGHT 5585, WRONG 1186, EMPTY_WRONG 62.
- `verdictdump`: RIGHT 545046, WRONG 6579, GAP 908.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 342,986,360; domain-model 1,200,177,830.

Every `Ir` below uses that command; the complete CLI output of each run is
compared with the base's (`cmp`).

## 1. Item 1 (`tsr-2zk.1088`): triage of the relater-attributed sole-TS2322 cases

r5-ts2322's `map.tsv` attributes 36 cases to the relater. On the base, four of
them are already RIGHT (r5-relater6: `stringMappingDeferralInConditionalTypes`,
`templateLiteralTypes5`, `errorInfoForRelatedIndexTypesNoConstraintElaboration`,
`mappedTypeUnionConstrainTupleTreatedAsArrayLike`). `r5census` under
`TSR_ASSIGN_PROBE` names every missing line of the rest `DECLINED` (the
relation answered `Unknown`), except `invariantGenericErrorElaboration` 4:19
(`NEVER`). Each was then traced through the relater (a local, uncommitted
trace of `is_related_to_with_flags`). Findings that move a case out of this
lane:

- **`enumAssignmentCompat3` (12 lines), `enumLiteralAssignableToEnumInsideUnion`
  (2): not a relater arm.** A namespace-rooted qualified enum reference
  (`First.E`) is minted as an OBJECT-flagged `Named` type (`declared.rs`,
  QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE's enum arm is scoped to
  alias-rooted names). `Abc.Nope.a -> First.E` then never reaches
  `isEnumTypeRelatedTo`; even `z = "x"` with `z: First.E` is silent. Owner:
  `declared.rs` (r5-declared3).
- **`assignFromBooleanInterface2`, `assignFromNumberInterface2`,
  `invalidBooleanAssignments` (extras): not a relater arm.** The relation
  answers Related (`boolean -> NotBoolean` relates `true`/`false` through the
  augmented `Boolean` apparent type), yet TS2322 is reported: the report is
  produced in `report_relation_failure` before the relation is asked
  (`missing_required_property`, `assignreport.rs`, r5-ts2322's lane).
- **`typeParameterHasSelfAsConstraint`: not a relater arm.** No `T -> number`
  relation is asked for `return x` under `T extends T`; the check site
  declines first.

## 2. A `unique symbol` against a decidable type

**Forcing constraint.** `isRelatedToEx` (relater.go:2605) answers a pair with
neither side structured or instantiable by `isSimpleTypeRelatedTo` alone. That
function's only arms for a `unique symbol` are `ESSymbolLike -> ESSymbol`
(relater.go:236) and identity. The port kept `UNIQUE_ES_SYMBOL` out of
`FLAG_DECIDABLE`, so every such pair answered `Unknown`:
`"" in Symbol.toPrimitive` (`typeof Symbol.toPrimitive -> object`,
`symbolType2`) and `const s: string = Symbol()` (`uniqueSymbolsErrors` 87).

**Ported.** A unique symbol against any flag-decidable type, or a decidable
type against a unique symbol, is NotRelated, after the simple arms (so
`unique symbol -> symbol`, `any`, `unknown` still relate). It mirrors the
existing object-against-unique-symbol arm.

**Alternatives.** Adding `UNIQUE_ES_SYMBOL` to `FLAG_DECIDABLE` would also
decide two *distinct* unique-symbol types as NotRelated. Rejected: a
predicate's or type-parameter constraint's `unique symbol` is still minted per
written node (`unique_symbols.rs`, `unique_symbol_awaits_printer_reuse`), so
two TypeIds may be one native type. That pair stays `Unknown`; it would win
once every slot reads `uniqueESSymbolTypes`.

**Measured** against §0, both dumps unfiltered, both loss checks empty:
- diagnostics: `symbolType2` and `uniqueSymbolsErrors` WRONG → RIGHT;
- types: `extractInferenceImprovement:0:41` and `:0:43` WRONG → RIGHT;
- `Ir`: generic-imports 342,986,360 → 342,958,772 (−0.008%); domain-model
  1,200,177,830 → 1,200,379,140 (+0.017%). CLI output identical.

**Falsifier.** A unique symbol that native relates to a non-symbol primitive
or to `object` (none exists in `isSimpleTypeRelatedTo`).

Test: `tests/relater7_arms.rs` `a_unique_symbol_against_a_decidable_target_is_decided`.

## 3. A private identifier declared in another class is an absent property

**Forcing constraint.** Native names a private identifier's symbol per
declaring class (`GetSymbolNameForPrivateIdentifier`, binder.go:369:
`__#<class symbol id>@#foo`). So `B`'s `#foo` is not `A`'s: against `interface
A2 extends A {}`, `getUnmatchedProperty` (relater.go:4233) finds `A`'s `#foo`
missing from `B` and the relation is False (`privateNamesUnique-5` 12:7). The
port keys members by their text, found `B`'s `#foo`, and (a `#` name carries
no `private` modifier, so the privacy arm did not fire) compared `number` to
`number`: Related, then `Unknown` from the surrounding walk.

**Ported** (`properties_related_to_excluding`): a `#` target member whose
source counterpart has another value declaration is treated as native's
absent member: NotRelated, or Related when the target member is optional
outside the subtype relations (getUnmatchedProperty's `requireOptionalProperties`).
An inherited `#foo` shares the base's declaration, so a subclass still relates.

**Alternative.** Escaping private names in the binder as native does. Rejected
for this lane: it changes every member lookup and printer that reads the name
(`members.rs`, `printing.rs`), none of them this lane's. The declaration test
is equivalent wherever a name lookup succeeds, which is the only place the
two keyings differ.

**Measured** against §2's commit, both loss checks (vs §0) empty:
- diagnostics: `privateNamesUnique-5` WRONG → RIGHT;
- types unchanged;
- `Ir`: generic-imports 342,958,772 → 342,977,973 (+0.006%); domain-model
  1,200,379,140 → 1,200,333,445 (−0.004%). CLI output identical.

**Falsifier.** A `#` member reached through a merged declaration whose value
declaration differs from native's single symbol (classes cannot redeclare a
private identifier, so none is known).

Test: `tests/relater7_arms.rs` `a_private_identifier_of_another_class_is_an_absent_property`.

## 4. Only the target class's privacy is nominal

**Forcing constraint.** The gate's class shortcut (`nominal_class_pair_verdict`,
`unions.rs`, §17 of `checker-notes-assign.md`) answers NotRelated for two
heritage-free classes when *either* declares a private or protected property.
Native has no such rule: `propertiesRelatedTo` walks the **target's**
properties (relater.go:4253). A source's own private members against a target
that declares none are extra properties, so `class C { #x } -> class D {}`
relates (`privateNameDeclarationMerging` 8:22, an extra TS2322). Only a target
private/protected (or `#`) member is one an unrelated class cannot supply: a
private member needs the same declaration (relater.go:4270, binder.go:369), a
protected one a valid override.

**Ported.** The shortcut's NotRelated is kept only when the target class
declares such a property (`class_declares_own_privacy`, the same syntax test
the shortcut makes, applied to the target alone). Other pairs take the
structural walk, whose privacy arm compares declarations.

**Alternatives.** Dropping the shortcut entirely: the structural walk decides
the same pairs, but `nominal_class_pair_verdict` would become dead code in a
file this lane does not own (`unions.rs`, r5-unionorder). The narrowed form is
native's answer on every pair the shortcut sees: an unrelated heritage-free
source never has the target's private declaration, and never validly
overrides its protected member. One edge stays: an *optional* target private
member natively admits a source lacking it; the shortcut still says
NotRelated there.

**Measured** against §3's commit, both loss checks (vs §0) empty:
- diagnostics: `privateNameDeclarationMerging` WRONG → RIGHT;
- types unchanged;
- `Ir`: generic-imports 342,977,973 → 342,991,541 (+0.004%); domain-model
  1,200,333,445 → 1,200,094,867 (−0.020%). CLI output identical.

**Falsifier.** A heritage-free class pair natively related while the target
declares a required private, protected or `#` property.

Test: `tests/relater7_arms.rs` `only_the_target_class_privacy_is_nominal`.

## 5. Variadic tuple elements, and `getNormalizedTupleType`

**Forcing constraint.** `propertiesRelatedTo`'s tuple arm (relater.go:4105-4227)
reads each element's flags. A spread of an array is `Rest`, a spread of a
generic type is `Variadic`, and both are `Variable`. Per source position: a
variadic target element accepts only a variadic source element, a variadic
source element needs a variable target element, and a required target element
needs a required source element. `sourceRest` is `combinedFlags & Rest` (a
variadic element alone does not open the source), and the fixed ends count
`NonRest` elements, variadic ones included. The port declined (`Unknown`) any
tuple pair with a generic spread on either side, so `any[] -> [...T, ...P]`
and `[any, any] -> [...T, ...P]` were silent (`variadicTuples3` 5, 10, 15).

**Ported** (`tuples_related_to`): the element flags and the three position
rules above. A variadic source against a rest target relates to
`createArrayType` of the rest's element type (relater.go:4199).

**The loss this exposed, and its fix.** Deciding those pairs first lost
`genericTupleWithSimplifiableElements` (EMPTY_RIGHT → EMPTY_WRONG, 11:13 and
13:13): `[1] -> [...args: { [S in SS]: [a: number] }[SS]]` became NotRelated,
the required source element meeting a variadic target. Native never sees that
target: `getNormalizedType` applies `getNormalizedTupleType` (checker.go:28073)
to a generic tuple, simplifying each simplifiable element (writing on the
target side) and renormalizing with `createNormalizedTupleType`, so the target
relates as `[a: number]`. Ported as `Relater::normalized_tuple`, in the gate
right after the indexed-access simplification: an indexed-access element is
simplified with `simplified_indexed_access`, and a changed list goes through
the port's `normalize_variadic_tuple` (TupleNormalizer). A conditional element
is not simplified (the port's `simplified_conditional` is reached only from
the conditional arms); stated divergence, no case asks for it.

**Measured** against §4's commit, both loss checks (vs §0) empty:
- diagnostics: `variadicTuples3` WRONG → RIGHT; nothing else moves;
- types unchanged;
- `Ir`: generic-imports 342,991,541 → 342,969,719 (−0.006%); domain-model
  1,200,094,867 → 1,200,168,204 (+0.006%). CLI output identical. The first
  version tested `is_generic_tuple_type` (which clones the element list) for
  every tuple pair: domain-model +0.063%. The borrowed indexed-access test
  runs first now.

Cross-checked against a native tsgo built from the pinned submodule on twelve
variadic shapes (`tests/relater7_arms.rs`
`variadic_tuple_elements_relate_by_their_flags`): identical reports.

**Falsifier.** A generic tuple pair where native's normalized element list
differs from `normalize_variadic_tuple`'s (a union spread distributing into a
union of tuples is the likeliest).

## 6. A string-mapping source against a template-literal target

**Forcing constraint.** For `Capitalize<string> -> `A${string}``, native's
template target arm (relater.go:3572) asks `isTypeMatchedByTemplateLiteralType`,
whose `inferTypesFromTemplateLiteralType` (relater.go:2345) answers only for a
string-literal or template source, so it fails. The source switch's
string-mapping case (relater.go:3782) then relates the base constraint:
`computeBaseConstraint` of `Capitalize<string>` is `string`
(checker.go:27544, the inner constraint equals the inner type), and `string ->
`A${string}`` is False. The port's gate routed only literal, template and
`string` sources against a template target to the worker, so the pair fell to
`Unknown` (`stringMappingOverPatternLiterals` 129-131, 147-149).

**Ported.** The gate routes a string-mapping source too; in the worker, after
the template target arm, a string-mapping source against a template target
answers `string_like_source_constraint` (r5-relater6 §2.2(2)'s helper), with no
later arm, since none relates a non-object target.

**Measured** against §5's commit, both loss checks (vs §0) empty:
- diagnostics: `stringMappingOverPatternLiterals` WRONG → RIGHT;
- types unchanged;
- `Ir`: generic-imports 342,969,719 → 342,974,161 (+0.001%); domain-model
  1,200,168,204 → 1,200,297,638 (+0.011%). CLI output identical.

**Falsifier.** A string mapping whose port base constraint is not native's
(`string_like_source_constraint` declines a gap or self constraint, leaving
`Unknown`).

Test: `tests/relater7_arms.rs` `a_string_mapping_meets_a_template_through_its_base_constraint`.

## 7. Item 2 (`tsr-2zk.1055`): `typeof E` and a fundule's `typeof Point`

Both witnesses are TS2403, whose test is `isTypeIdenticalTo`. The port asks it
through `is_type_identical_to_by_assignability` (`identity.rs`): two object
types are non-identical when either direction of assignability is NotRelated.
Both directions answered `Unknown`, so no TS2403. The trace shows why: the
relater's `has_members` admitted an anonymous type only when it carried
signatures or was a plain namespace object (r5-relater3 §6). `typeof E` (an
enum object) and `typeof Point` (a function merged with a namespace) fell to
row 3 (`NoMembersTable`).

**Ported (fundule).** `has_members` admits an anonymous type whose symbol is a
function merged with a value module (not a class, not an enum):
`is_function_namespace_object`. Native's `typeof Point` is
`createObjectType(ObjectFlagsAnonymous, symbol)` with the namespace exports as
members beside the function's call signatures (`resolveAnonymousTypeMembers`),
which the structural arm relates as it does a namespace object. `() => { x;
y } -> typeof Point` is now NotRelated (no `Origin`), so TS2403 is reported
(`FunctionAndModuleWithSameNameAndCommonRoot` test.ts 2:5, simple.ts 13:5).

**Measured** against §6's commit, both loss checks (vs §0) empty:
- diagnostics: `FunctionAndModuleWithSameNameAndCommonRoot`,
  `enumIsNotASubtypeOfAnythingButNumber` and
  `unionSubtypeIfEveryConstituentTypeIsSubtype` WRONG → RIGHT;
- types: `enumAssignabilityInInheritance:0:168` WRONG → RIGHT;
- `Ir`: generic-imports 342,974,161 → 342,990,563 (+0.005%); domain-model
  1,200,297,638 → 1,200,282,532 (−0.001%). CLI output identical.

**Held (enum object):** [`r5-relater7-enum-object.diff`](r5-relater7-enum-object.diff)
admits `typeof E` as well. It converts `typeOfEnumAndVarRedeclarations` (both
TS2403) and four `useObjectValuesAndEntries1` type lines (GAP → RIGHT), and
loses `mappedToToIndexSignatureInference` (EMPTY_RIGHT → EMPTY_WRONG, an extra
TS2345 at 11:25). There, `enumValues<K extends string, V extends string>(e:
Record<K, V>)` called with `E` infers `V = never` (native: `E`), and `typeof E
-> Record<"A" | "B", never>` is now a correct NotRelated where it used to be
`Unknown`. The relation is right and the inference is wrong: inferring to a
mapped target from an enum object (`inferToMappedType`/`inferFromObjectTypes`
reading `typeof E`'s members) belongs to `inference.rs` (main's). The diff
lands once that inference yields `E`; re-measure to zero losses then.

Test: `tests/relater7_arms.rs` `a_function_merged_with_a_namespace_relates_over_its_exports`.

## 8. Native's union fast paths, and the pure-signature test's order

Found while profiling item 3's `relationComplexityError`; each is a native step
the port skipped, so each pair walked the long way.

1. **`unionOrIntersectionRelatedTo`'s origin fast paths** (relater.go, before
   `eachTypeRelatedToType`). A source union normalized from an intersection
   whose origin contains the (aliased) target relates, since `A & B` relates
   to `A`; a source alias listed in a target union's origin relates. For
   `f1`'s `x = y` (`T1 & T2 -> T1`, a 4,096-literal `T1` distributed over `{ a }
   | { b }`) native answers at once; the port walked 8,192 intersections
   against 4,097 constituents (33 M pairs). Ported as
   `union_target_is_origin_member`, over the port's `union_origin` record
   (`intersections.rs` records a distributed intersection as the union's single
   origin entry). Native's alias test is a union printing as a symbol here.
2. **`typeRelatedToSomeType`'s fast paths** (relater.go:2974-3005):
   `containsType`, then a string/boolean/bigint literal (a number literal too
   under the subtype relations), not an enum literal, against a primitive
   union (`ObjectFlagsPrimitiveUnion`, getUnionType's `includes &
   TypeFlagsNotPrimitiveUnion == 0`, checker.go:25730) outside the comparable
   relation: related exactly when the union holds its base primitive or its
   other fresh/regular form, else False. Ported as `literal_in_union_shortcut`.
3. **`is_pure_signature_type`** (a port gate, no native counterpart) enumerated
   the member names and index infos of both sides of every pair before
   looking at the signature list it needs. It now tests the signature list
   first; the boolean is unchanged. It was 53% of the `relationComplexityError`
   profile, and is most of the domain-model saving below.

**Measured** against §7's commit, both loss checks (vs §0) empty; no verdict
or line moves (diagnostics and types identical to §7):
- `Ir`: generic-imports 342,990,563 → 342,947,034 (−0.013%); domain-model
  1,200,282,532 → 1,179,272,821 (−1.75%). CLI output identical;
- `relationComplexityError`: 67 s (base) → 24 s (CLI, `--strict`), still
  above the 10 s slowcases bar; §9's budget takes it the rest of the way.

## 9. Item 3 (`tsr-2zk.1065`): the relation-complexity budget

**Forcing constraint.** `checkTypeRelatedToEx` (relater.go:369) gives every
top-level check `relationCount = (16,000,000 − relation.size()) / 8`.
`recursiveTypeRelatedTo` spends one unit per published result (`:3163`,
`:3174`), and when the budget is gone sets `overflow` (`:3086`), after which
every structured comparison of that check is False (`:3062`). The check then
records the top pair as `Failed | ComplexityOverflow` and reports TS2859
instead of the relation's error. The port had none of it, so
`relationComplexityError`'s `f2` (`T1 & T2 -> T1 | null`, 8,192 intersections
against 4,098 constituents) walked to completion: 46-67 s, and no TS2859.

**Ported** (`Relater::relation_count`, `Relater::overflow`):
- the budget is computed in `Relater::new` from the persistent store's size
  (`RelationResults::len`, no longer test-only), so a nested top-level check
  (a variance measurement, say) gets its own, as native's `getRelater` pool
  does;
- `publish_result` spends one unit per result, in the persistent store or a
  framed walk's local one;
- `recursive_type_related_to` answers False once overflowed, and overflows
  when the budget is gone, in native's order (after the cache, before the
  maybe-key test);
- a new `CachedRelation::ComplexityOverflow` (native's
  `RelationComparisonResultComplexityOverflow`) records the top pair, so a
  later check reads it instead of walking again; a nested read is False, as
  native's.

**Stated divergence: the answer and the report.** Native's overflowing check
answers False and reports TS2859, at `errorNode` or, for a check with none,
at `c.currentNode`. The port has no `currentNode`, and its reporting site
(`report_relation_failure`) is `assignreport.rs` (r5-ts2322). Until that site
issues TS2859, an overflowing check answers `Unknown`, so the reporter
declines rather than printing a TS2322 native never prints, and a silent
caller reads `false` as native's. The reporter half is
[`r5-relater7-ts2859-reporter.diff`](r5-relater7-ts2859-reporter.diff).

The stack-depth half (TS2321, `sourceStack`/`targetStack` at 100) is not
ported: the port's raw `MAX_DEPTH` refusal stays an unpublished `Unknown`.
It counts every recursion, not native's per-side stacks, so turning it into
TS2321 would report where native's stacks never reach 100.

**Measured** against §8's commit, both loss checks (vs §0) empty; no verdict
or line moves:
- `relationComplexityError`: 24 s → 2.9 s (base: 67 s), off the slowcases
  list. The f2 check overflows as native's does (one overflow per
  top-level check that reaches it);
- `Ir`: generic-imports 342,947,034 → 342,948,787 (+0.0005%); domain-model
  1,179,272,821 → 1,179,657,930 (+0.033%). CLI output identical. The cost is
  the per-publication decrement and the two tests in
  `recursive_type_related_to`.

**Convention record** (the new cache state): native operation
`relation.set(id, Failed|ComplexityOverflow)` (relater.go:373); key identity
the port's `RelationKey` of the regular source and target, per relation, owned
by `Checker::relation_results` for the checker's lifetime; published only when
a check overflowed; read by `recursive_type_related_to` (False) and
`cached_object_relation` (`Unknown`, the divergence above); no receiver
context; the expensive work it saves is the whole overflowing walk.

**Falsifier.** A check that overflows in the port but not natively: the port
publishes more results per pair than native in places (it caches identity
and literal pairs native answers without `recursiveTypeRelatedTo`), so its
budget can run out sooner. None is known in the corpus (no verdict moved).

## 10. Item 4 (`tsr-2zk.1068`): the variance-probing blow-up was a recursion-identity gap

**What the profile showed.** `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`
(and `…2`) took 37-41 s. An instrument counting nested relations per
top-level check found the four variance-marker comparisons of `Left`/`Right`
(`getVariancesWorker`'s `Left<L, sub> -> Left<L, super>` and the union
probes) at 450k-800k nested relations and ~10 s each. A full trace of one
showed a single walk descending to the port's `MAX_DEPTH` (100) again and
again along

    Left<L, X> -> <B>(fab: Either<L, (a: X) => B>) => … -> Either<L, (a: X) => any>
      -> Left<L, (a: X) => any> | Right<…> -> Right<…> -> Left<L, (a: (a: X) => any) => B> -> …

that is, `ap`'s parameter growing one function layer per step. Native cuts
this at the third growing occurrence: `isDeeplyNestedType` (relater.go,
called from `recursiveTypeRelatedTo`) counts stack entries with the same
`getRecursionIdentity`, and both stacks expand, so the pair is `Maybe`.

**Cause.** The hypothesis in the brief (a nested top-level relation in
`inference_variances` hiding the stack) was not it: the trace shows one walk.
The port's `relation_recursion_identity` gives a class or interface reference
its target symbol, **except** a reference recorded in
`reference_types_from_nodes` (every class/interface reference produced from a
type node, including each re-walk of an alias body under a binding frame),
which falls back to its unique `TypeId`. Every `Left<L, …>` in the chain is
such a reference, so no two stack entries ever shared an identity and the
cut never fired.

Native's `getRecursionIdentity` has no such exclusion: an eager reference is
tracked by its target's symbol, and a *deferred* reference (one created for a
type node inside a type alias body, such as `Either`'s `Left<L, A>`) by its
node.

**Ported.** The exclusion is removed: written class and interface references
are tracked by their target symbol like every other reference.

**Stated divergence.** For a deferred reference native's identity is its node,
which is finer than the symbol: `Left<L, A> | Left<A, L>` in one alias body
are two identities natively and one here, so the port may count expansion
sooner. The node is not recorded with the type (the insertion site is
`declared.rs`, r5-declared3); recording `reference_types_from_nodes` as a map
from type to reference node would let this read `RecursionIdentity::Node`.

**Measured** against §9's commit, both loss checks (vs §0) empty; no verdict
or line moves:
- `varianceProblingAndZeroOrderIndexSignatureRelationsAlign`: 40.8 s → 0.37 s;
  `…2`: 40.9 s → 0.39 s (CLI, `--strict`); both off the slowcases list;
- `performanceComparisonOfStructurallyIdenticalInterfacesWithGenericSignatures`:
  28.7 s → 6.9 s (CLI), 878 → 393 MiB in the dump. Its remaining cost is
  heritage-conformance signature relations, 74% of `Ir` under
  `instantiate_signature_in_context` → `infer_from_types_within` →
  `infer_from_members` (`inference.rs`, main's);
- `Ir`: generic-imports 342,948,787 → 342,945,931 (−0.0008%); domain-model
  1,179,657,930 → 1,179,512,821 (−0.012%). CLI output identical.

**Falsifier.** A relation native decides that now ends `Maybe` (Related) at the
expansion cut because two distinct written references to one generic class
count as one identity. None moved in either dump.

## 11. Item 5: the binder's declare-module imports, and the TS2536 reporter — both held

### 11.0 Rebased baseline ("base2")

Before item 5 the branch merged the integration head `51d8da2` (batch AM,
which carries batch AF with r5-relater6). The merge was clean. Frozen there:
- `diagverdictdump`: RIGHT 5460, EMPTY_RIGHT 5595, WRONG 1131, EMPTY_WRONG 52;
- `verdictdump`: RIGHT 549033, WRONG 6382, GAP 876;
- `Ir`: generic-imports 342,889,305; domain-model 1,099,819,306;
- no slowcases rows.

### 11.1 The binder fix (held)

**Forcing constraint.** `declareModuleMember` (binder.go:376-380) decides an
alias *before* `container.Flags & NodeFlagsExportContext`: only an export
specifier, or an import-equals declaration with `export`, goes to the
container's exports; every other alias (`import { B } from …`, `import * as
N`, an unexported `import x = …`) is a local, for every container. The port's
`is_exported_from_container` (`tsr-binder` `binder.rs`) applied that only when
the container is a source file; inside `declare module "U" { … }` (an
ExportContext container) imports fell through to `export_context` and were
filed as exports, where name resolution declines an ambient alias. Repro:
`declare module "B" { export type Bit = 0|1 } declare module "U" { import { Bit }
from "B"; export const y: Bit }` reported a false TS2552 for `Bit`. Its own
comment already described native's rule.

**The fix** ([`r5-relater7-binder-declare-module-imports.diff`](r5-relater7-binder-declare-module-imports.diff))
makes the alias arm container-independent, as native's. The repro is clean.

**Measured unfiltered** against base2: both dumps abort (OOM row) at
`ramdaToolsNoInfinite2`, the case the fix was for. Once its `Boolean`/`Bit`
imports resolve, the port evaluates its recursive conditional aliases to
a 54 MB type text (`evaluate_conditional_alias` → `get_instantiated_type_reference`
→ `type_reference_text`, recursively). Native bounds that walk with
`instantiationDepth == 100 || instantiationCount >= 5,000,000` (TS2589,
checker.go:22111); the port's conditional-alias road counts neither (r5-harness
§5 item 2). Re-measured with every other case (a `TSR_FILTER` prefix list
that excludes `ramdaToolsNoInfinite2` and, unavoidably, `ramdaToolsNoInfinite`,
whose name is its prefix), base2's binaries against the fix's:
- diagnostics: `moduleAugmentationInAmbientModule1` WRONG → RIGHT,
  `moduleAugmentationInAmbientModule5` EMPTY_WRONG → EMPTY_RIGHT;
- types: +18 lines (`moduleAugmentationInAmbientModule1`/`5`, `privacyGloImport`,
  `privacyGloImportParseErrors`), **5 lost**:
  - `privacyImportParseErrors:0:564`, `:569`, `:594`, `:599`: `var v:
    use_glo_M2_public` where the name is an unexported `import x =
    require("glo_M2_public")` inside `export declare module "use_glo_M1_public"`.
    Native prints the written name (a type reference to an alias with no type
    meaning); as a local alias the port's `declared.rs` road answers `any`
    (types-triage `TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL`);
  - `moduleAugmentationImportsAndExports3:3:4`: `a.foo().n` where `foo(): B`
    is declared in `declare module "./f1" { import {B} from "./f2"; … }`; `B`
    now answers `any` from the augmentation's locals. Not traced further
    (name resolution of an augmentation's locals, `symbols.rs`/`module_*.rs`).

**Held until** the conditional-alias evaluation is bounded (`declared.rs`,
r5-declared3) and the two type-reference roads above read a local alias as
they read an exported one; then re-measure unfiltered.

### 11.2 The TS2536 reporter (still held)

r5-relater6's [`r5-relater6-ts2536-reporter.diff`](r5-relater6-ts2536-reporter.diff)
still applies to this branch. Its two false TS2536s
(`ramdaToolsNoInfinite2` 45:83 and 60:42) are exactly the unresolved imports
§11.1 fixes, so it waits on §11.1.

## 12. Item 6: `for_constraint_extra` retired; `keyofAndIndexedAccessErrors` 114 not reached

**`for_constraint_extra`.** The relater joined `getConditionalType`'s
`forConstraint` extra true branch onto the captured distributive constraint of
a conditional source, because `declared.rs`'s capture once evaluated without
it. Both capture paths now apply it themselves: `with_for_constraint_extras`
for an alias reference and `distributive_conditional_constraint` for an
inline mint (`declared.rs`, `getConditionalTypeInstantiation(…, forConstraint
= true)`, checker.go:24383). The relater's copy only re-derived the same
union, so the conditional source arm now relates the capture directly and the
function is deleted.

**Measured** against base2: both dumps byte-identical to base2's (verdicts and
lines); `Ir`: generic-imports 342,889,305 → 342,908,758 (+0.006%); domain-model
1,099,819,306 → 1,099,819,162 (−0.00001%). CLI output identical.

**`keyofAndIndexedAccessErrors` 114** (`tj = tk`, `T[K] -> T[J]` with `K
extends Extract<keyof T, string>`, `J extends K`) still fails on `K -> J`,
which ends `Unknown` in the conditional source arms (r5-relater4 §2's
decline, as r5-relater6 §4 recorded). Not taken up this session.
