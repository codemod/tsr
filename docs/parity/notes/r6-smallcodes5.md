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
