THE `diagnostics` WORKSTREAM'S HANDOFF.

`TASK.md` is the `.types` gradient workstream's. They are separate files because
they were overwriting each other. Nothing below touches `checker_types`, and
**every `diagnostics` build of the eleventh and twelfth sessions left it
byte-identical**.

FIRST: git pull. Then read, in this order:
  STATUS.md §1's `diagnostics` block and §5's `diagnostics` refusals, then
  docs/architecture/checker-notes-diag2.md **§75 and §79 first** (the board and
  the metric), then §76–§83 — the twelfth session, eight sections, seven builds
  and one refusal.

STATE AT HANDOFF (verify with a fresh coverage run):
  diagnostics    1,346/5,488 = 24.53%   (was 1,302; +44 over 7 builds,
                 **zero cases lost in any of them**)
  checker_types  3,742/9,538 · 83.41% — the other workstream's. Do not touch it;
                 re-check it is byte-identical after every build.

## THE ONE THING THAT DECIDES WHETHER YOUR BUILD LANDS

**Take the bar off `diagreach`'s CASE count. Never off `diagmissing`'s line
count.** The twelfth session ran the experiment six times without meaning to:

| build | bar taken off | bar | measured |
|---|---|---:|---:|
| §76 | lines | +8 | **+5** |
| §78 | lines | +6 | **+2** |
| §79 | **cases** | +9 | **+13** ✓ |
| §80 | lines | +8 | **+5** |
| §81 | **cases** | +9 | **+10** ✓ |
| §82 | cases | +9 | **+3** (see the limit below) |
| §83 | **cases** | +7 | **+6 for ZERO wrong** |

The three line-barred builds all came in at roughly a **15:1** concentration —
`diagmissing` counts lines and the board counts cases, and a bar taken off the
first overshoots the second by whatever the concentration happens to be.

**§82 found the metric's own limit, and it is the exception to watch for.**
`reachabilityChecks1`…`11` is **one file re-run under eleven option
combinations**. They share every shape and convert together, and `diaggap`
counted them as eleven independent cases. **Discount a row concentrated in one
file-name *stem* the way you would one concentrated in one case** — check the
case names before believing the count.

## THE BOARD, re-taken at `21202b0`

```
cases reachable by deepening existing rules : ~1,300
  wants only relation-bound codes           :   939   <- NOT YOURS
  wants a mix                               :    50
  wants NO relation-bound code              :   308   <- YOURS
```

939 is the assignability family (TS2322/2345/2339/2741/2353/2352/2416/2430/
2420/2415/2403/2411) — `checker_types`' structural relation and members table
arriving through a second door. They already report nothing extra and convert
with **no diagnostics work** once that subsystem lands. Do not build into them:
§16 measured TS2322 at 988 wrong lines, §24 refused TS2403 at 6 losses, §49's
type parameters cost 48. **Re-take the split when the .types workstream lands
relation work.**

Relation-free head by cases, and by **cases one line short** — the second column
is the one that predicts:

```
code      cases   one line short
TS2454      45        25          <- the biggest, and §76 only took its enum-union half
TS7006      20        17          <- §80 turned the option on; re-measure the row
TS2554      19        12          <- §78 took the callee kinds; the rest is `new`
TS2464      14        10
TS2365      12         9
TS2540      12         9
```

## RANKED NEXT ITEMS

0. **Run `diagreach.rs` and `diaggap.rs` and pick from them.** Everything below
   is that list read at this commit.

1. **PLUMB THE COMPILER OPTIONS IN ONE BUILD.** Three of the twelfth session's
   six builds turned on an option's tristate, and a fourth (§82's residual) is
   still blocked on one. This is now the cheapest thing on the board.
   - **§80's finding, and check every sibling**: `GetStrictOptionValue`
     (`core/compileroptions.go:294`) answers `options.Strict != TSFalse` for an
     unset option — **unset is TRUE**. `diagnostics_suite.rs` had
     `noImplicitAny` at `unwrap_or(false)` *ten lines below* a `strictNullChecks`
     at `unwrap_or(true)`, and the two disagreed for eleven sessions.
     `strictFunctionTypes`, `strictBindCallApply`, `noImplicitThis`,
     `useUnknownInCatchVariables` and `alwaysStrict` are the rest of that family
     and **none of them has been checked**.
   - **Not every option is strict.** `noUnusedLocals`/`noUnusedParameters` read
     as `IsTrue()` (`unusedIsError`, `checker.go:7104`) and are correctly
     opt-in. `allowUnreachableCode` is a **Tristate with three meanings** —
     §82 needed `unreachableCodeIsError` = *explicitly* `TSFalse`, because unset
     is a *suggestion* that never reaches a `.errors.txt`.
   - **`preserveConstEnums` is the one §82 wanted and could not ask for.** It is
     three wrong lines today and it gates `isInstantiatedModule`, which several
     rules will need.

2. ~~TS2449, the `extends` slice~~ **DONE, §83, +6 for ZERO wrong.** What is
   left is **`isBlockScopedNameDeclaredBeforeUse` (`checker.go:1922`) proper** —
   the eighty lines of *deferral* arms §83 bounded its way around: a use inside
   a function body, an instance property initialiser, an export specifier, a
   binding element, a decorator, a computed property name. Its five
   non-converters (`classDeclarationShouldBeOutOfScopeInComputedNames`, the tail
   of `resolvingClassDeclarationWhenInBaseTypeResolution`) are exactly those
   arms. **Build it once for three codes**: `checkResolvedBlockScopedVariable`
   picks between TS2449, TS2448 (block-scoped variable) and TS2450 (enum) off
   the symbol's flags and shares everything else, and TS2448's own row has not
   been sized yet.

3. **The rest of `diaggap`'s relation-free single-code column**: TS2693 9,
   TS2364 7, TS2703 7, TS2558 6.

4. **TS7026 — still refused, and its row is GROWING.** 28 sole-obstacle cases,
   the largest relation-free row on the board. §13's number (12 conversions for
   47 wrong) stands and the blocker is unchanged: the corpus's JSX cases declare
   `namespace JSX` inside `declare global { … }` and **global augmentation is
   unported in this binder**. This is worth a session on its own the moment
   `declare global` merging lands.

5. **TS1212 (104 lines)** needs `alwaysStrict` inside `tsr_binder::bind`, which
   nothing plumbs. **But check §82 first**: the same sentence was carried for
   TS7027 across three handoffs and turned out to be false there — the binder
   already recorded the answer per node and the checker could just read it.
   **Ask what the binder already knows before accepting a binder blocker.**

6. **The assignability family** is §5's standing refusal and `diagreach` prices
   it at 939 cases — more than the suite currently passes. A `checker_types`
   build, not a diagnostics one.

## THE INSTRUMENTS — use them, do not rebuild them

| instrument | answers |
|---|---|
| `examples/diagreach.rs` | **Run first.** Cases reachable by deepening rules that exist, and §75's relation-bound split |
| `examples/diaggap.rs` | The single-code column — which code a case is blocked on **alone**. Now the ordering over *new* rules, and the source of every §81/§82 target |
| `examples/diagmissing.rs` | `diagmissing -- 2454` prints every missing baseline line of that code. **Read it for the shapes, never for the bar** |
| `examples/diagcase.rs` | One case's expected and actual side by side. **Diffing a case whole is how §78 was found** |
| `examples/diagemit.rs` | `want` against `have` per code. A large `want` with **zero** `have` is a rule that is not running |
| `examples/extraonly.rs` | Cases blocked by an **extra** diagnostic alone — one false positive from passing. 13 of the 49 are ours; the rest are the parser's TS1005/TS1012 |
| `examples/diag2307.rs` | The per-rule counterfactual. **Isolate the codes in `RULE_CODES`, and RESTORE the full list.** It truncates at 400 distinct entries — §79's four-code run read 482 and its diff was unusable until the codes were isolated |
| `examples/extragap.rs` | Splits the *extra* column into `displaced` and `invented` |

## FINDINGS THE TWELFTH SESSION PAID FOR — do not repay

- **PRINT WHETHER THE RULE RUNS BEFORE ASKING WHAT IT DECIDED.** §76 and §80
  were both found by one `eprintln!` behind an env var at a rule's early
  returns, in one run each. §80's rule was not declining on arrows — it was
  **off**, because an option default was wrong. That instrument has now paid
  five times across two sessions and has never cost more than five minutes.
- **A stateful port of a stateful algorithm can be wrong because the state means
  something different here**, while every line reads as a faithful
  transcription. §82 modelled `reportedUnreachableFlow` as mutable walk state —
  correct upstream, wrong here, because **this binder starts a fresh flow for a
  namespace body and upstream does not**. Asking the tree the same question
  *statelessly* (no ancestor and no preceding sibling carries the fact) was
  shorter and right. Wrong went 19 → 3 and RIGHT never fell.
- **`error_span` is not universal.** §48 routed every report site through
  `GetErrorRangeForNode`, and §82 found the first site that must **not**:
  `errorOnEachUnreachableRange` reports a statement's own range, where
  `error_span` narrows a `namespace A { … }` to its name (column 11 against
  upstream's 5). The centralisation is what made the exception visible.
- **`ambient` as threaded by `crate::check`'s walk is NOT `NodeFlagsAmbient`.**
  It widens at `VariableStatement` and `FunctionDeclaration` and **nowhere
  else**, so any rule asking "is this ambient" of a **class member** must read
  the member's own `declare` modifier. §81's single wrong line was
  `declare #whatMethod()`. Nothing else in `crate::check` does this today, and
  every rule that takes `ambient` for a member is one `declare` away from it.
  (`NodeFlags::AMBIENT`, `NodeFlags::JAVASCRIPT_FILE` and
  `SymbolFlags::OPTIONAL` are still declared and set by nothing.)
- **A decline found in one family can pay several times over outside it.** §79
  added `ALIAS` to TS2304's meaning-ladder to fix two lines in
  `typeofAnExportedType` and removed **ten** — the ladder is consulted by every
  TS2304 in the corpus. Read the wrong column, not the case.
- **The three-valued relation has three correct projections** and which is right
  is a property of the CALLER's direction:
  reports **because** a relation failed → `== NotRelated`;
  silent because it held, upstream ternary → `!= NotRelated`;
  silent because it held, upstream **binary** → `== Related`.

## TRAPS PAID FOR, DO NOT REPAY

- **The parse-error gate is a per-rule measurement, not a house style, and
  TS2304's now has three.** §40.3 deleted it for +6, §50.1 re-measured at −6,
  and §79 converted three recovered trees (`parserTypeQuery3`, `6`, `9`) with no
  wrong line. **Absent is right for TS2304.** Do not generalise it.
- **`== self.intrinsics.error` vs `Checker::is_error`** — this port has two error
  types and upstream has one. Identity tests remain in the checker and each is
  the same question (§43, §44).
- **This port propagates `errorType` outward through type constructors where
  upstream CONTAINS it** (§77, refused). `() => X` with an unresolved `X` is
  `errorType` here and an anonymous object type upstream, which silences every
  rule gating on the declared type. Owner: `checker_types`, and it disappears
  when printing reads the store instead of a cached string.
- **A JS decline does NOT transfer between arms** (§74). The rule is "a
  JSDoc-sourced ANNOTATION is unreliable", not "JS files are unreliable".
- **A rule's yield is not its row.** Check `STILL SHORT` before pricing.

## THE STANDING LOST, carried with their names

`diag2307` prints LOST on both sides of an edit, so a rule's existing loss reads
as furniture. All three below are also `extraonly.rs` entries — each is **one
false positive from passing** — and all three are diagnosed:

- **`compiler/validRegexp`** — an extra TS2304. The parser reads an ambiguous
  `/` as division, so the regex body parses as identifiers. Owner:
  `tsr_parser`.
- **`conformance/resolutionModeTripleSlash1` and `3`** — extra TS2304 on
  `MODULE`. `/// <reference types="foo" />` against an `@types` package whose
  `exports` map sends `import` to `.d.mts` and `require` to `.d.cts`; the port
  loads neither. The sibling `2` is in `extraonly` too with two extra TS2552,
  meaning the port *did* load the `.d.mts` there. **Owner: `file_loader`.**
  Three cases behind it.

## THE LOOP, eighty-two for eighty-two

`diagreach`/`diaggap`/`extraonly` for the target → `diagmissing.rs` for the
**shapes** (not the bar) → isolate the codes in `diag2307.rs`'s `RULE_CODES` →
**register the bar in checker-notes-diag2.md BEFORE the code**, off the CASE
count, with falsifiers → build anchored to upstream `file:line` → measure both
sides → READ THE TOP ROW OF THE WRONG COLUMN and **decline with a named owner**
(never "tighten") → **restore RULE_CODES** → coverage → five gates each its own
invocation (fmt · clippy `grep -c "^error"` == 0 · test · anchors · issue-ids) →
commit → STATUS → push.

LOST must not grow in any measurement. `checker_types` must stay byte-identical.

Push before you stop. A build that is not pushed did not happen.
