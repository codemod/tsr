# Lane notes: r4-heritage (tsr-2zk.939, tsr-2zk.3.2)

Judgment calls made by the `r4-heritage` parity box. Numbers are measured with
`verdictdump` / `diagverdictdump` against the baseline frozen at `5ad60b1`
(integration head at dispatch): diagnostics RIGHT 4312, EMPTY_RIGHT 4978,
WRONG 1190, EMPTY_WRONG 90; `checker_types` lines right 470138 of 477979.
Native behaviour is reproduced with a `tsgo` built from the pinned submodule
(`5b1047d`) by `scripts/offline-cargo/build-tsgo.sh`.

## 1. `links.interfaceChecked` is the first *checked* declaration

**Forcing constraint.** `checkInterfaceDeclaration` (`checker.go:4991`) runs
its once-per-symbol block — `checkInheritedPropertiesAreIdentical`, the
TS2430 loop over `getBaseTypes`, and `checkIndexConstraints(t, symbol,
false)` — from whichever declaration of the merged symbol is checked first,
guarded by `declaredTypeLinks.interfaceChecked`. The port ran it only from the
symbol's first interface declaration. A user `interface Object { data: A;
[x: string]: Object }` merges with lib.es5's `Object`, whose declaration is
first in the merged symbol and is never checked (the bundled libraries are
never walked), so the block never ran and `data`'s TS2411 was lost
(`objectTypeHidingMembersOfExtendedObject` 10). The port now picks the first
interface declaration that is not in a bundled default library
(`Checker::in_default_library`). Native, reached from the user declaration,
also reports on the lib side (`lib.es5.d.ts(--,--)` lines in that baseline);
those have no position and are not scored, and this port reports them too
because the check runs over the merged declared type.

**Alternatives.** Keep a once-per-symbol flag set at check time, as native
does. That needs a new per-symbol side table for a boolean whose only effect
over "first non-library declaration" is a program in which a non-library
declaration is not checked. Every non-library file is checked here (the
corpus and the CLI walk every root file), so the two agree. Falsifier: a
corpus case whose first non-library interface declaration lives in a file the
port does not walk (e.g. under `skipLibCheck`).

**The parse-error gate is dropped for interfaces.** `check_heritage_conformance`
returned on `file_has_parse_errors`; `checkInterfaceDeclaration` has no such
gate. `interfaceExtendingClass2` (11) has a recovered parse error later in
the file and natively reports TS2411 for the interface inheriting `Foo`'s
string index. The class arm keeps the gate for now (§2 measures it).

**Index constraints are called on the type, not the node.** The interface
arm called `check_index_constraints(node)` in `index_constraint.rs`, which
repeats the first-declaration rule and the parse-error gate on its own. This
lane now calls `check_index_constraints_of_type(declared, symbol, false)`
directly — the native call — which needed that function's visibility raised
to `pub(crate)`; no behaviour of `index_constraint.rs` changed. Its
`InterfaceDeclaration` arm in `check_index_constraints` is now unreached
(the class and type-literal arms are unchanged); removing it is left to that
file's owner.

Port convention record: no cache, side table, mapper or traversal is added;
the existing declared-type, base-type and index-constraint queries run once
per symbol from the chosen declaration.

**Measured** against the frozen baseline: diagnostics RIGHT 4312 -> 4314
(`objectTypeHidingMembersOfExtendedObject`, `interfaceExtendingClass2`);
`checker_types` unchanged; both loss checks empty.

## 2. `issueMemberSpecificError` reads late-bound member names

**Forcing constraint.** `issueMemberSpecificError` (`checker.go:4494`) skips a
member only when `declaredProp.Name == ast.InternalSymbolNameComputed`. A
computed name `lateBindMember` can bind — a well-known symbol
(`[Symbol.toPrimitive]`) or a literal (`["a"]`) — has a real symbol name, so
the member is related and reported as TS2416 at the member. The port skipped
every `ComputedPropertyName`, so the walk found no member error and fell back
to the broad TS2420 at the class name (`symbolProperty24`: pinned `tsgo`
reports TS2416 at `[Symbol.toPrimitive]`, the port reported TS2420 at `C`).
The port now reads the name `late_bound_members_of` (the existing
`lateBindMember` image) assigned to the member, and keeps skipping a computed
name it did not bind. The diagnostic prints the name as written
(`symbolToString` of a late-bound symbol), through
`computed_member_name_text` (raised to `pub(crate)` in `index_constraint.rs`).

Port convention record: no new table; `late_bound_members_of` is the
existing per-(owner, static) memo, read only on the failure path (after the
class-level relation has already said `NotRelated`).

**Measured** against the post-merge baseline (`0ca5b7e`, diagnostics RIGHT
4341): RIGHT 4341 -> 4342 (`symbolProperty24`); both loss checks empty.

## 3. tsr-2zk.3.2 was already closed by the static-side check

The issue's witnesses, `overridingPrivateStaticMembers` and
`derivedClassOverridesPrivates`, are RIGHT in the frozen baseline (`5ad60b1`):
the instance relation no longer fails on static-only differences, and
`check_static_side_assignability` (`check.rs`) reports the TS2417. No change
was needed in this lane.

## 4. Cluster 2 stopped: main's property lane owns `issueMemberSpecificError`

`main` landed `0ecbd92` (`property: relate heritage members with the class
this type; admit merged implemented interfaces`, tsr-2zk.4) while this lane
worked. It ports `issueMemberSpecificError`'s `typeWithThis`/`baseWithThis`
member reads and drops the merged-declaration decline for implemented
interfaces and interface bases — the same native functions
(`checkClassLikeDeclaration`'s base and implements arms,
`issueMemberSpecificError`, `checkInterfaceDeclaration`'s TS2430 loop) as this
lane's TS2415/2416/2420/2430 cluster. Per the round-4 rule the cluster stopped
there; nothing below is committed as code. Each held change is a measured
diff, all against the post-merge baseline `0ca5b7e` with this branch's §2
applied (diagnostics RIGHT 4342), both loss checks empty for each:

| Diff | Native function | Converts | Owner |
|---|---|---|---|
| [r4-heritage-class-parse-gate.diff](r4-heritage-class-parse-gate.diff) | `checkClassLikeDeclaration` has no parse-error gate | `bases`, `interfaceDeclaration4` (RIGHT +2) | this file; held for main's lane |
| [r4-heritage-member-enum-veto.diff](r4-heritage-member-enum-veto.diff) | `issueMemberSpecificError`'s `checkTypeAssignableToEx` is a `checkTypeRelatedTo` reporter, so its member pair takes `assignability_pair_is_reportable` (no enum veto), as the TS2322 reporters do | `undefinedIsSubtypeOfEverything` (RIGHT +1) | this file; held for main's lane |
| [r4-heritage-parameter-property-modifiers.diff](r4-heritage-parameter-property-modifiers.diff) | `getDeclarationModifierFlagsFromSymbol` reads a parameter property's modifiers off the parameter; `property_has_modifier` ignored `ParameterDeclaration`, so `constructor(private p1)` vs a public base `p1` related | `readonlyConstructorAssignment`, `assignmentCompatability40` (RIGHT +2) | `members.rs` |

[r4-heritage-main-0ecbd92-resolution.diff](r4-heritage-main-0ecbd92-resolution.diff)
is the resolution of the one textual conflict between this branch's
`heritage_conformance.rs` and `0ecbd92` (both insert lines at the top of
`issue_member_specific_error`), written against main's file. Applied on this
branch's head together with `0ecbd92`'s other files it builds, and
`symbolProperty24`, `objectTypeHidingMembersOfExtendedObject`,
`interfaceExtendingClass2`, `classWithMultipleBaseClasses`,
`elaboratedErrors` and `implementArrayInterface` are all RIGHT.

**Measured and dropped.** Narrowing the merged-*source* decline from "one
declaration" to "one class or interface declaration" (a class merged with a
namespace adds no instance member) converted nothing and was not kept. Moving
the clodule base past `check_static_side_assignability`'s single-declaration
decline (`check.rs`) does not convert `clodulesDerivedClasses` either: the
relater answers `Unknown` for `typeof Path.Utils` against `typeof
Shape.Utils`.

## 5. Remaining heritage clusters (root causes outside this lane's files)

Counted from the frozen dumps (`/tmp/box/base2`, cases whose only wrong
lines are TS2415/2416/2417/2420/2430/2720, unless noted):

- **Polymorphic `this` in the base relation** — `contextualThisType` (extra
  TS2430), `thisTypeInFunctions3` (extra TS2415), likely
  `indexSignatureAndMappedType`. Native relates `typeWithThis` to
  `getTypeWithThisArgument(base, t.thisType)`; here inherited members keep the
  base's minted `this` and the relater substitutes each side's receiver, so
  `(p: this) => void` fails contravariantly. Minimal witness: `interface X3 {
  a: (p: this) => void } interface Y3 extends X3 {}` (pinned `tsgo`: no
  error; port: TS2430). Main's `0ecbd92` starts this for member reads; the
  class-level relation still needs it.
- **Protected target arm** — `implementingAnInterfaceExtendingClassWithProtecteds`,
  `interfaceExtendingClassWithProtecteds`, `interfaceExtendingClassWithProtecteds2`
  (5 lines). `relater.rs`' property privacy arm answers `Unknown` for a
  protected target (`isValidOverrideOf` unported).
- **Private names** — `privateNamesAndFields`,
  `privateNamesConstructorChain-1`, `-2` (extra TS2416). The binder keys `#foo`
  by its text, so a derived class's `#foo` meets the base's; native's key is
  per declaring class (`__#N@#foo`), and the two never relate.
- **Implemented type aliases** — `implementsIncorrectlyNoAssertion`. Native
  implements `getReducedType(getTypeFromTypeNode(typeRefNode))` and checks
  `isValidBaseType`; the port resolves only class/interface symbols. Needs
  `base_types.rs`' `is_valid_base_type` (private) and an alias-reference
  resolution for an `ExpressionWithTypeArguments`.
- **Merged class + interface** — `classExtendsNull2` (TS2417 for `extends
  null`), `interfaceClassMerging`: the merged-source decline (§4 table
  owner).
- **Namespace static sides** — `clodulesDerivedClasses` (TS2417), relater
  `Unknown` above.
