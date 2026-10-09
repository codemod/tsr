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
