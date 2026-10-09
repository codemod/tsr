# r6-smallcodes5: sole-code clusters for seven checker codes

Lane on epic `tsr-2zk`, item `tsr-2zk.1134`, vendor `5b1047d`. The method is
the one in `r5-smallcodes3.md` and `r6-smallcodes4.md`: list the diagnostics
cases that are WRONG on exactly one code, classify each against a native
`tsgo` built from the pinned submodule (`scripts/offline-cargo/build-tsgo.sh`),
cluster by native operation, and port each root cause.

Frozen base: `claude/beautiful-shannon-ar5gh0` `b18aec06` (batch BC had not
landed when this lane started).

- Diagnostics dump: 12,238 rows. 5,530 RIGHT, 5,596 EMPTY_RIGHT, 1,063
  WRONG, 49 EMPTY_WRONG.
- Types dump: 556,303 lines. 549,853 RIGHT, 843 GAP, 5,607 WRONG.

Setup: PyPI answers 403 here, so `tomlkit` could not be installed
(r5-operators3 §4). A stdlib-only stand-in for `parse`, `inline_table` and
`dumps`, kept in the session scratchpad and not committed, built the vendored
tree.

## 1. Ownership and how the work ships

This lane owns only the files it creates. Every hook lands in a file it does
not own, so each port ships as r6-smallcodes4 §1 describes:

- the logic in a new `tsr-checker` file, committed with its `mod` line in
  `lib.rs` (a hub file where adding is allowed), its `impl` block under
  `#[expect(dead_code)]` until the hook is applied;
- a hook diff in this directory that adds the call in the owner's file,
  removes the attribute, and carries the unit test (the test fails without
  the hook).

Every diff applies to the base alone and on top of the ones before it.

## 2. Ported

### 2.1 Private names are per class — `private_name_identity.rs`

Cases on the base, all WRONG on TS2416 or TS2739 alone:

| Case | Shape |
|---|---|
| `privateNamesAndFields` | extra TS2416 on `B`'s `#foo: string` (`A` has `#foo: number`) |
| `privateNamesAndMethods` (es2022, esnext) | extra TS2416 on `B`'s `#foo(a: string)` |
| `privateNamesConstructorChain-1`, `-2` | extra TS2416 on `Child`'s `#foo = "foo"` |
| `privateNamesAndStaticFields` | extra TS2739 on `const willErrorSomeDay: typeof A = class {}` |

Native: a private identifier's symbol name is mangled per declaring class
(`binder.GetSymbolNameForPrivateIdentifier`, `binder/binder.go:369`:
`__#<class symbol id>@#foo`). Two consequences:

1. **Instance members.** `class B extends A { #foo }` declares a name `A`
   does not have, so `addInheritedMembers` keeps `A`'s `#foo` in `B` beside
   `B`'s own. Relating `B` to `A` (`checkClassLikeDeclaration`,
   `checker.go:4340`) finds `A`'s own symbol in `B` (`sourceProp ==
   targetProp`), so the class is assignable and `issueMemberSpecificError`
   never runs.
2. **Static members.** `getUnmatchedPropertiesWorker` (`relater.go:988`)
   skips every `isStaticPrivateIdentifierProperty` target. Its own TODO names
   `privateNamesAndStaticFields`. The property loop then finds no source
   member under the target's mangled name and does not relate it.

TSR keys members by their text (`relater.rs` says so at its private-name
arm). So `B`'s `#foo` shadows `A`'s, the arm saw a source `#foo` with another
declaration and answered `NotRelated`, and `issue_member_specific_error` then
compared `string` with `number`. For the static case, the property loop's
missing-member arm rejected `#foo` and `#bar`.

The new file answers what the mangled lookup would, from declarations:

- `is_static_private_identifier_property`: the target property's value
  declaration is a class element named by a private identifier with
  `static`;
- `source_inherits_private_member`: the class declaring the target's private
  member is the source's class or one of its bases (the walk is
  `base_symbols_of_ex`, memoized in `members.rs`). It declines (`None`) for a
  source with no class symbol, an unfollowable base, or a **generic**
  declaring class, whose inherited member's type depends on the base's type
  arguments. That decline is new; it reports `Unknown`, where the base
  answered `NotRelated`, only for a source that inherits the target's
  declaration from a generic class. No corpus case reaches it today.

The hook (`r6-smallcodes5-private-name-identity.diff`, `relater.rs`,
r6-relater's file) adds both to `properties_related_to_excluding`: the
static skip at the top of the property loop, and the inherited answer before
the private-name arm's `NotRelated`.

- **Rejected: mangling the names in the binder.** That is the faithful data
  model, but it changes every member table, every printer and every reader of
  `#name` text across main's files, for six cases. Revisit it if a second
  private-name reader needs the per-class identity (for example
  `privateNamesInGenericClasses`).

Checker port convention: native operation `getPropertyOfType` under a mangled
name and `isStaticPrivateIdentifierProperty`; no new cache or side table; the
expensive work is the base walk, memoized by `base_symbols_of_ex`, and it runs
only for a `#name` whose source member has another declaration.

Probe (the four case files): native and TSR with the hook report the same
rows. Without it TSR adds the TS2416 and TS2739 rows above.

**Measured** unfiltered against the frozen base:

- Diagnostics: +6 cases, zero losses (5,535 RIGHT, 5,597 EMPTY_RIGHT). Only
  the six cases above changed rows.
- Types dump: verdicts unchanged.
- slowcases: clean on both dumps.
- Ir (callgrind, `--singleThreaded --pretty false --noEmit`): the first
  version called `is_static_private_identifier_property` for every target
  property and read domain-model +0.50% in two runs (1,090,830,746 →
  1,096,254,643; 1,091,397,002 → 1,096,875,974). Testing `#` at the call
  site removed it: domain-model 1,091,273,886 (−0.01%), generic-imports
  343,064,507 (−0.006%). CLI output is byte-identical on both projects.

Unit test: `tests/private_name_identity.rs`, in the diff. Three of its four
fixtures fail without the hook; the fourth (two unrelated classes with the
same `#foo` still fail to relate) guards the other direction.

Correction (same session): the first commit's `#[expect(dead_code, …)]`
spanned one line longer than `rustfmt` allows, so `cargo fmt` reflowed it and
the diff no longer applied. Every new file now carries the one-line
`#[expect(dead_code, reason = "r6-smallcodes5 hook diff not applied")]`, and
the private-name diff was regenerated against it.

### 2.2 TS2532 on a qualified name's left in a type query — `type_query_receivers.rs`

Case on the base: `narrowingOfQualifiedNames`, missing TS2532 at (33,25) and
(38,29), the `foo.a.b` of `type C = typeof foo.a.b.c` inside `if (foo.a)`
(once directly, once in a `for…of` body).

Native: `checkQualifiedName` (`checker.go:8122`) checks its left through
`checkNonNullExpression`, or for a `this` head in a type query through
`checkNonNullType(checkThisExpression(left), left)`. Checking the left of
`foo.a.b.c` is checking `foo.a.b`, whose left `foo.a` is checked first, so
the reports come innermost first. `reportObjectPossiblyNullOrUndefinedError`
(`checker.go:7455`) names an entity name **expression** (`'u' is possibly
'undefined'`); a qualified name is not one, so it takes `Object is possibly
'undefined'`. `getTypeFromTypeQueryNode` reaches this once per query node.

TSR: `check_qualified_name` (`members.rs`) already stripped the left with
`check_non_null_type`, with no reporter, because diagnostics come from the
check walk. Nothing in the walk asked for a `TypeQuery` node's receivers. The
new file is that half: the same calls the type side makes, and for an
identifier left the receivers' reporter (`report_nullable_operand_of_type`,
`nullable_operand.rs`). The `unknown` arm (TS18046) stays declined as it is
for every receiver (`nullable_operand.rs`, r6-smallcodes4 §3.1). Unlike the
expression receivers (`check_null_or_undefined_receiver`, `check.rs`), this
runs in a file with parse errors, as upstream's does.

The hook (`r6-smallcodes5-type-query-receivers.diff`) adds one call to the
check walk's `TypeQueryNode` arm in `check.rs` (main's).

Probe, native and TSR identical:

```text
type C2 = typeof foo.a.b.c;   (foo: { a?: { b?: { c?: string } } })
  (7,18) TS2532 ×2
type X = typeof u.x;          (u: { x: number } | undefined)
  (9,17) TS18048 'u' is possibly 'undefined'.
```

### 2.3 `implements` a type alias — `implemented_alias.rs`

Case on the base: `implementsIncorrectlyNoAssertion`, missing TS2416 at
(9,5): `type Wrapper = Foo & Bar; class Baz implements Wrapper { x: number }`.

Native: the implemented-type loop of `checkClassLikeDeclaration`
(`checker.go:4365`) takes `getReducedType(getTypeFromTypeNode(typeRefNode))`
whatever declares the name. A valid base (`isValidBaseType`: an object type,
`object`, `any`, or an intersection of those) is related through
`getTypeWithThisArgument`, then `issueMemberSpecificError`. The message is the
class one when `t.symbol` is a class. `getTypeWithThisArgument` rebuilds an
intersection from its constituents, so the alias is dropped and the base
prints `'Foo & Bar'`, not `'Wrapper'`.

TSR: `check_class_implemented_types` (`heritage_conformance.rs`) resolved
only class and interface symbols and skipped everything else. The new file
answers a type-alias entry: the alias's declared type, the intersection
rebuilt without its alias, and `isValidBaseType`'s object/intersection arms.
It declines:

- a generic alias or one written with type arguments (the alias
  instantiation road is not called from this loop);
- a gap;
- an invalid base, which is TS2422 upstream and is not ported;
- a type-parameter alias body (`type W = T` inside a generic function),
  where `isValidBaseType` would ask the constraint.

The hook (`r6-smallcodes5-implemented-alias.diff`) is the non-class,
non-interface branch of the loop in `heritage_conformance.rs` (no listed
owner this round; shipped as a diff).

Probe, native and TSR identical: the case file's TS2416 (`'Foo & Bar'`), and
`type Shape = { x: string; y: string }; class C implements Shape { x = "" }`
→ TS2420 at `C`.

### 2.4 §2.2 and §2.3 measured

The two diffs were applied together on the base. They emit different codes
(TS2532/TS18048 against TS2416/TS2420), so each changed row is attributed by
code. Unfiltered against the frozen base:

- Diagnostics: +2 cases (`narrowingOfQualifiedNames`,
  `implementsIncorrectlyNoAssertion`), zero losses.
- One more case changed rows and stays WRONG: `typeofThis` gains its
  missing TS18048 at (32,19) (`typeof this.x`'s `this` head). Its other
  missing row (TS7017) is unrelated.
- Types dump: verdicts unchanged.
- slowcases: clean on both dumps.
- Ir: domain-model 1,091,476,917 → 1,091,280,812 (−0.02%), generic-imports
  343,083,923 → 343,069,469 (−0.004%). CLI output is byte-identical.

Unit tests (in the diffs): `tests/type_query_receivers.rs` (four fixtures)
and `tests/implemented_alias.rs` (three). Every fixture's expectation was
probed against native.

### 2.5 TS2344 on an import type's arguments — `import_type_constraints.rs`

Cases on the base, each missing one TS2344:

- `unmetTypeConstraintInImportCall`: `type Bar<T> = import('./file1').Foo<T>`
  with `Foo<T extends string>` (`file2.ts(1,37)`);
- `unmetTypeConstraintInJSDocImportCall`: the same through
  `@typedef {import('./file1').Foo<T>} Bar` in JS (not converted, below).

Native: `checkImportType` (`checker.go:3324`) ends in
`checkTypeReferenceOrImport` (`:2998`), the tail a type reference runs. With
type arguments and a resolved symbol that has type parameters,
`checkTypeArgumentConstraints` (`:3016`) relates each argument to its
constraint. The symbol is the one `getTypeFromImportTypeNode` (`:24575`)
resolves: the module with `export =` followed, then the qualifier through
each namespace's exports (`Namespace` meaning, `Type` for the last segment),
then `resolveSymbol`.

TSR's check walk ran the constraint check for `TypeReference` and heritage
entries only. The new file resolves the import type's symbol the same way;
the hook (`r6-smallcodes5-import-type-constraints.diff`) calls the existing
`check_type_argument_constraints_of` (`constraints.rs`, main's) from the
walk's `ImportTypeNode` arm in `check.rs`, and makes that function
`pub(crate)`. The constraint check keeps every gate it has for type
references.

One test is replaced rather than ported: `checkTypeReferenceOrImport` runs
only when the node's type is not an error. TSR's
`get_type_from_import_type_node` (`declared.rs`) answers the gap for every
import type written with type arguments, so the test cannot be asked. The
symbol resolution fails exactly where upstream's type is an error from a
missing module or member (TS2307, TS2694), so it stands in for the test.

- **Rejected: routing through r6-smallcodes4's held `import_type_node.rs`.**
  That port answers the declared type for non-generic targets only, and
  is held on printer losses (its §3.2). This check needs the symbol, not the
  type.

Probe, native and TSR identical:

```text
type Bar<T> = import('./file1').Foo<T>;      (1,37) TS2344 Type 'T' … 'string'.
type Ok = import('./file1').Foo<"a">;        none
type Bad = import('./file1').Foo<1>;         (3,34) TS2344 Type 'number' … 'string'.
type B = import('./f1').N.Box<number>;       (1,31) TS2344
```

**The JSDoc twin is not converted.** Two blockers, both main's:

- its `@typedef` comment is the last thing in `file2.js`, so it hangs on the
  end-of-file token, and the check walk never visits JSDoc there (with a
  statement after the comment, the walk reaches the import type);
- `file1.js`'s `@typedef … Foo` binds into file locals
  (`declare_jsdoc_symbol`, `binder.rs`), not into the module's exports.
  Upstream's reparsed `JSTypeAliasDeclaration` is implicitly exported
  (`ast.IsImplicitlyExportedJSDocDeclaration`, `binder.go:375`), so
  `import('./file1').Foo` resolves upstream and not here.

**Measured** with the diff stacked on §2.1–§2.3, unfiltered against the
frozen base: +1 case (`unmetTypeConstraintInImportCall`) beyond the stack's
eight, zero losses, no other row changed; types unchanged; slowcases clean;
Ir domain-model 1,091,558,539, generic-imports 343,087,791 (the base's own
three runs span 1,090,830,746–1,091,476,917), CLI output identical.

Unit test: `tests/import_type_constraints.rs` (three fixtures, two modules
through a `ModuleHost`), in the diff.

### 2.6 A qualified generic reference without its arguments is `errorType` — `qualified_reference_arity.rs`

Case on the base: `genericCloduleInModule2`, an extra TS2454 at (15,1) on
`b.foo()` after `var b: A.B`, where `A.B` is `class B<T>`.

Native: `getTypeReferenceType` (`checker.go:23146`) counts the written
arguments against `getMinTypeArgumentCount` (`:21938`) and the parameter
count. Outside that window it reports TS2314/TS2707 and answers `errorType`:
the class/interface arm (`getTypeFromClassOrInterfaceReference`, `:23170`)
outside JS (`!isJs`, `:23195`), the alias arm (`getTypeFromTypeAliasReference`,
`:23580`) in every file. `errorType` is `any` to its consumers: `b` prints
`any`, and `checkIdentifier`'s definite-assignment test assumes an
`any`-typed variable initialized (`checker.go:11156`), so there is no TS2454.

TSR's unqualified road already answered the error. The qualified road
(`qualified_type_reference`, `declared.rs`, r6-declared's) minted a named
object type for an argument-less reference to a generic class, so `b` looked
like a class instance and was reported as unassigned. The new file is the
arity test; the hook (`r6-smallcodes5-qualified-reference-arity.diff`)
answers `native_error` from the argument-less arm when it fails. The TS2314
report was already made (`type_argument_arity.rs`) and is unchanged.

Probe, native and TSR identical:

```text
namespace A { export class B<T> { foo() {} } export class D<T = number> { foo() {} } }
function f() { var b: A.B; b.foo(); var d: A.D; d.foo(); }
  (3,10) TS2314 Generic type 'B<T>' requires 1 type argument(s).
  (6,3)  TS2454 Variable 'd' is used before being assigned.
```

Measured with §2.7 and §2.8 (all three stacked on §2.1–§2.5; the codes are
disjoint, so rows are attributed by code): this diff converts
`genericCloduleInModule2` and `genericTypeReferenceWithoutTypeArgument`,
moves `genericTypeReferenceWithoutTypeArgument2` toward its baseline (an
extra TS2352 on `<C>x` goes; its missing TS2694 is unrelated), and turns 9
type lines WRONG → RIGHT (`genericCloduleInModule2` 0:8, 0:10–0:13 now
`any`; `genericTypeReferenceWithoutTypeArgument` 0:43–0:44;
`genericTypeReferenceWithoutTypeArgument2` 0:39–0:40). Zero losses. The
batch's totals are in §2.8.

Unit test: `tests/qualified_reference_arity.rs` (a required parameter, and a
defaulted one that still reports TS2454), in the diff.

## 3. Apply order

| # | Diff | New file | Cases |
|---|---|---|---|
| 1 | `r6-smallcodes5-private-name-identity.diff` (`relater.rs`) | `private_name_identity.rs` | +6 |
| 2 | `r6-smallcodes5-type-query-receivers.diff` (`check.rs`) | `type_query_receivers.rs` | +1 |
| 3 | `r6-smallcodes5-implemented-alias.diff` (`heritage_conformance.rs`) | `implemented_alias.rs` | +1 |
| 4 | `r6-smallcodes5-import-type-constraints.diff` (`check.rs`, `constraints.rs`) | `import_type_constraints.rs` | +1 |
| 5 | `r6-smallcodes5-qualified-reference-arity.diff` (`declared.rs`) | `qualified_reference_arity.rs` | +2, +9 type lines |

Each later diff is relative to the ones before it. Stacked, the four diffs
reproduce the measured tree byte for byte.
