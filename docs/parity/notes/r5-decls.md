# r5-decls: declaration checks (tsr-2zk.1023)

Lane `r5-decls`, round 5 (`docs/parity/round5.md`). Populations come from the
frozen `diagverdictdump` at `a1e453d`: the WRONG/EMPTY_WRONG rows whose
expected-vs-actual code multiset difference touches the lane's codes. "Alone"
means the case's whole difference is in those codes. Pinned upstream:
`vendor/typescript-go` @ `5b1047d`.

| code | alone | mixed | owned home |
|---|---|---|---|
| TS2391/2389/2390 | 7 (6 extra, 1 missing) | 6 | `check_function_or_constructor_symbol` (check.rs) |
| TS2507 | 6 | 1 | `check_extends_primitive` (check.rs) |
| TS2502 | 5 | 5 | `symbols.rs` (main's names lane) |
| TS2300 | 5 | 8 | `check_object_type_for_duplicate_declarations` (check.rs), `merge_conflicts.rs`, binder |

No cache, side table, mapper or traversal is added by this lane's commits;
the checker port convention has nothing to record.

## 1. Overload adjacency is `End() == Pos()`, not "next sibling"

`checkFunctionOrConstructorSymbolWorker` (`checker.go:3628`) reports a
non-adjacent overload when `previousDeclaration.End() != node.Pos()`, and
`reportImplementationExpectedError` (`checker.go:3565`) only looks at the
subsequent node when `subsequentNode.Pos() == node.End()`. `Pos()` is the
full start — the end of the preceding token.

The port had replaced both with "the next child of the same parent"
(`next_sibling`), because `tsr_core::Span` starts after trivia and the literal
span test reported TS2391 on 480 ordinary lines (the comment on
`next_sibling`). Sibling adjacency is right when nothing lies between the two
declarations except trivia — and wrong when a token the parser **skipped**
does: `function f1(), function f1();` parses two sibling declarations with a
stray `,` that belongs to no node. Upstream's positions see the `,`; sibling
order does not. `compiler/overloadConsecutiveness` misses all six of its
first-declaration TS2391s that way.

`declaration_follows_immediately` is the composition: next sibling **and**
only whitespace and comments (`tsr_scanner::is_whitespace_single_line`,
`is_line_break`, `//`, `/* */`) between `previous.end` and `node.start` in the
file's source text. Without a source text (a host that supplies none) the
sibling test stands alone, which is the previous behaviour.

*Rejected:* computing a full start per node in the parser. It would make the
test literal, but every node's span would grow a field for one consumer.
*Falsifier:* a case where trivia-only text separates two declarations upstream
treats as non-adjacent — impossible by construction, since `Pos()` is exactly
the end of the preceding non-trivia token.

## 2. TS2507 — `getBaseConstructorTypeOfClass`

`check_extends_primitive` mirrors `checker.go:16983`: report unless the base
expression's type is `any`, the null widening type, or `isConstructorType`.

- **The whole-file parse-error gate is removed.** Upstream has none, and
  `classExtendsEveryObjectType` (a stray `{ foo: string; }` literal is a parse
  error) and `classExtendingPrimitive` (`extends void` is TS1109) both lost
  every TS2507 to it.
- **`undefined` is decided.** The undefined intrinsic (and its widening
  form) has no construct signatures under any reading; it joins the four
  primitives `is_decidable_primitive` already admits. `extends undefined`
  prints `undefined` (`classExtendingNonConstructor`, `classExtendingPrimitive`).
  It is not added to `is_decidable_primitive` itself, whose other callers
  compare for identity and whose list is load-bearing (§865 there).

Converted: `classExtendingNonConstructor`, `classExtendingPrimitive`,
`classExtendsEveryObjectType`, `classExtendsEveryObjectType2`.

**Remaining (diff, not owned):** `this` in a class's own `extends` clause.
`ast.GetThisContainer` (`utilities.go:1790`) has no class arm — a class is a
`this` container only through its members — so `class X extends this {}`
types `this` from the enclosing container (`typeof globalThis` in a script,
`undefined` in a module). `check_this_expression` (`expressions.rs`) stops at
the class. With that arm and `typeof globalThis` answering no signatures in
`signatures_of_type_kind` (`flow.rs`), `thisInInvalidContexts` and
`thisInInvalidContextsExternalModule` convert:
`r5-decls-this-heritage.diff`.

## 3. TS2300 — late-bound members

`checkObjectTypeForDuplicateDeclarations` (`checker.go:3142`) keys its table
by `getSymbolOfDeclaration(member).Name`, and `getSymbolOfDeclaration` is
`getLateBoundSymbol` (`checker.go:14408`): a member with a late-bindable
computed name (`hasLateBindableName`: an entity-name expression whose type is
usable as a property name) answers the symbol `lateBindMember` gathered by
property name. The port keyed by the binder's symbol, which is one symbol
per computed member, so `interface I { [Symbol.isConcatSpreadable]: string;
[Symbol.isConcatSpreadable]: string }` never met itself (`symbolProperty37`).

`late_bound_duplicate_key` gives such a member a `Late` key — the literal's
property name, or the unique-symbol type itself — and the declaration count
is the number of members of this declaration carrying that key. That count
differs from the late symbol's declarations only across merged declarations,
and the walk's state table is per declaration, so a key seen once in this
declaration cannot report either way. The message argument is
`symbolToString` of the late symbol: its first declaration's written name
(`[c0]` for `[c0]: number; [c1]: string` with `c0 = "1"`, `c1 = 1`, as
`dynamicNamesErrors` records).

**Remaining (diff, not owned):** `lateBindMember`'s own conflict report
(`checker.go:16033`). Two `get [Symbol.hasInstance]()` declarations do not
reach the object-type walk (accessor-and-accessor is not its error); the
late symbol's `GetAccessorExcludes` rejects the second getter.
`late_bound_members_of` (`members.rs`) is the faithful home:
`r5-decls-late-bind-member.diff` (`symbolProperty44`). Its first draft
reported for every name `late_bound_members_of` returns and lost
`symbolProperty1` and `symbolProperty2`: that function also names members
keyed by a plain `symbol` (`var s: symbol; { [s]: 0, [s]() {} }`), which
are index-info components, not late-bound members. The diff now requires
`isTypeUsableAsPropertyName` (a literal or a unique symbol) before the
conflict arm, as `lateBindMember` does.

## 4. Measured and not shipped from owned files

- **TS2391 false positives on a missing body** (6 alone cases:
  `dottedModuleName`, `parserErrantEqualsGreaterThanAfterFunction1/2`,
  `objectTypesWithOptionalProperties2`, `destructuringParameterDeclaration6`,
  `parserSkippedTokens16`; plus `reservedWords3` mixed). Upstream's
  `parseFunctionBlockOrSemicolon` (`parser.go:3481`) returns a **missing
  block** — non-nil, `NodeIsMissing` — when neither `{` nor a semicolon
  follows a signature (`function f() => 4;`, `x()?: number;`). The worker
  then treats it as no implementation (`NodeIsPresent` is false) but skips
  the final report, which tests `Body() == nil`. TSR's parser returns `None`
  for that body, so the checker cannot tell the two apart. The fix is the
  parser's (`tsr-parser/src/declaration.rs`, `parse_function_block_or_semicolon`)
  and then every checker site reading `body.is_some()` has to choose between
  `Body() != nil` and `NodeIsPresent`, as upstream does site by site. Not
  attempted in this lane: it is a cross-crate change with checker-wide
  fan-out.
- **TS2502 for an unused self-referencing variable** (`typeofAnExportedType`,
  `typeofANonExportedType`: `export var r12: typeof r12;`). The circularity
  report in `symbols.rs` works — `var r: typeof r; let k = r;` reports — but
  upstream's `checkVariableLikeDeclaration` calls `getTypeOfSymbol` for every
  variable (`checker.go:5790`), and the port's variable check computes the
  symbol's type only when something asks. The variable check is
  `assignreport.rs` (r5-report2) and the dispatch in `check.rs`; not attempted.

## 5. Measurements

Owned commit (§1–§3, without the two diffs), against the frozen baseline at
`a1e453d`:

- diagnostics: plain 8,745 → 8,751 / 9,816, configured 2,135 → 2,135 / 2,422
  (RIGHT + EMPTY_RIGHT); converted `overloadConsecutiveness`,
  `classExtendingNonConstructor`, `classExtendingPrimitive`,
  `classExtendsEveryObjectType`, `classExtendsEveryObjectType2`,
  `symbolProperty37`. Both loss checks empty.
- type lines: 543,727 RIGHT of 552,533, unchanged; no RIGHT line lost.
- perf, median child CPU over 21 samples, new/old: domain-model 0.926,
  generic-imports 0.990; diagnostics match the baseline binary.

With both diffs applied on top (one combined unfiltered diag run), the
additional conversions were `thisInInvalidContexts`,
`thisInInvalidContextsExternalModule(target=es2015)` and
`classUsedBeforeInitializedVariables(target=es2015)` (this-heritage) and
`symbolProperty44` (late-bind member). That run's only losses were
`symbolProperty1/2`, from the late-bind diff's first draft; after the
`isTypeUsableAsPropertyName` fix (§3) a filtered rerun over `symbolProperty*`
and `dynamicNames*` shows them restored, but the fixed diff has not had a
full unfiltered run. Neither diff has
had its own unfiltered types run or perf run; the integrator's batch gate
measures them.
