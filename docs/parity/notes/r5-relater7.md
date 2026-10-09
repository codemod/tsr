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
