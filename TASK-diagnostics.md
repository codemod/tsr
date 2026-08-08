THE `diagnostics` WORKSTREAM'S HANDOFF.

`TASK.md` is the `.types` gradient workstream's. They are separate files because
they were overwriting each other. Nothing below touches `checker_types`, and
**every `diagnostics` build of the eleventh, twelfth and thirteenth sessions
left it byte-identical**.

FIRST: git pull. Then read, in this order:
  STATUS.md §1's `diagnostics` block and §5's `diagnostics` refusals, then
  docs/architecture/checker-notes-diag2.md **§75 and §79 first** (the board and
  the metric), then **§86–§90** — the thirteenth session, five sections, four
  builds.

STATE AT HANDOFF (verify with a fresh coverage run):
  diagnostics    1,443/5,488 = 26.29%   (was 1,346; +97 over 27 builds and
                 THREE priced refusals — §94, §96, §97 —
                 **zero cases lost, and the wrong column FELL in two of them**)
  checker_types  3,842/9,538 · 84.13% — the other workstream's, and it moves
                 HOURLY: it changed three times inside this one session. Do not
                 touch it, and do not compare against this number — remeasure.

## CHECK BYTE-IDENTITY THE ONLY WAY THAT WORKS

**`git stash push <your files>` → coverage → `git stash pop`.** That compares two
states of the *same checkout*, which is the only comparison that means anything
while another workstream lands builds between your measurement and your push.

Two sessions in a row have misread a `.types` build arriving through their own
`git pull --rebase` as their own drift (83.40 → 83.41 in the twelfth, 3,784 →
3,786 in the thirteenth). Both times the number written down before the last
push was simply stale. **Never compare against it.**

Related, and it cost this session its first hour: **`coverage` is run in DEBUG in
`CLAUDE.md` and the examples are run `--release`.** Release has `overflow-checks`
off. The `.types` workstream landed a `span.end - span.start` that wrapped
harmlessly in release and aborted every debug run (§86). If coverage exits 101,
suspect this before suspecting your own change.

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
TS2454      45        25          <- the biggest, but §85 DIAGNOSED the rest: 13 of the
                                    24 one-line cases are `declared == errorType`
                                    because MODULE AUGMENTATION is unported (the same
                                    blocker as TS7026), and the for-of ones are a
                                    blanket decline worth re-measuring. Not a
                                    diagnostics build — read §85 before picking it
TS7006      20        17          <- §80 turned the option on; re-measure the row
TS2554      19        12          <- §78 took the callee kinds; the rest is `new`
TS2464      14        10
TS2365      12         9
TS2540      12         9
```

## RANKED NEXT ITEMS

0aa. **READ THE FIXTURE BEFORE THE FOURTH HYPOTHESIS.** Nine sections
   (§122–§130) attributed three wrong lines from the wrong column, the baseline,
   upstream's source, two probes and twice from the case's *name*. §131
   explained them by opening the 19-line test file, which none of the nine had
   done. **`diagmissing` prints case names, `diagcase` prints line numbers,
   `diag2307` prints columns — none of them prints the code.**

0a. **THE CHEAP GRAMMAR CODES — start here.** §103 took TS1029 (modifier order)
   for **+9 cases and zero wrong lines**, and it needed *no types, no symbols,
   no flow, no relation* — a `Vec` of seen keywords and a left-to-right walk.
   It had sat untouched for thirteen sessions because every session ranked with
   `diagreach`, which measures cases reachable by **deepening rules that exist**;
   a code with no rule appears only in `diaggap`. **§104 then took `TS1163` for +10 cases**
   (10 lines / 10 cases, concentration 1.0). **§105 PRICED the rest and two of the three are NOT the
   same shape**: `TS1100` (12 cases) is the **binder's** —
   `checkStrictModeEvalOrArguments` (`binder.go:1449`) from seven call sites,
   needing `b.inStrictMode` and a **three-way** message split of which only one
   is TS1100 (TS1210 in a class, TS1215 in a module); `TS1109` (11 cases) is the
   **parser's**, every line a yield/await recovery position; and `TS1183`
   measures **zero** and is off the board. **The cheap-grammar seam is now
   exhausted — re-read `diaggap` for the next one rather than assuming a `TS1xxx`
   code is cheap.**
   Before porting any rule that reads a `NodeFlag`, **grep whether anything sets
   it**: `NodeFlags::YIELD_CONTEXT` is a fourth declared-and-never-set flag
   alongside `AMBIENT`, `JAVASCRIPT_FILE` and `SymbolFlags::OPTIONAL`, and §104
   had to derive it structurally. Read `grammarchecks.go` for each. Two rules §103 paid for: at most
   **one** grammar report per node (every upstream arm is a `return`), and the
   `else if` **order is the specification**.

0. **Run `diagreach.rs`, `diaggap.rs` and `extraonly.rs` and pick from them.**
   Everything below is that list read at §90's commit, and four builds have
   moved it.

1. **TS2554's remaining families — the row is ALREADY DIAGNOSED in §90.** It was
   18 sole-obstacle cases over 29 missing lines, a **concentration of 1.6**,
   which is the best shape on the board. §90 took the `new` half; what is left
   is **rest-parameter ordering** (`genericRestArity`, `genericRestArityStrict`,
   `spreadOfParamsFromGeneratorMakesRequiredParams`, `iterableArrayPattern25`,
   `unionTypeCallSignatures4`), where both arms decline any signature carrying a
   rest parameter rather than guess a bound — upstream reads the rest's TUPLE
   type (`relater.go:1713-1726`), which is why it can say "expected 2" for
   `...args: [number, string]`. That is `checker_types`-shaped and should be
   priced before it is attempted. The overload question is DONE (§92) and the
   backward min-count is DONE (§91). Also still open: the
   JS arm, where **§74's rule applies: a JS decline does not transfer between
   arms.** Measure it, do not assume it.

2. ~~Duplicate PARAMETERS~~ **DONE, §93, +3 cases and +64 right lines for zero
   wrong, and the `binder_symbols` rail did not move.** What that build leaves
   is the *general* question it raises: `declare_into` still derives excludes
   for every other site, and upstream passes it at **all** of them. Auditing the
   remaining seven against `binder.go`'s call sites is the same shape as §84's
   option audit — **and it was DONE, §94.** Eleven derivations verified against
   `ast/symbolflags.go`; the one divergence (`ValueModuleExcludes`) measured at
   **+2 cases for +14 wrong** and was refused. **Its owner is `classify`, not
   `excludes()`** — every `ModuleDeclaration` is mapped to `VALUE_MODULE`, where
   upstream picks `NamespaceModule` for a non-instantiated one. The unlock is
   moving §89's `GetModuleInstanceState` out of `crate::check` into `tsr-ast`,
   after which the flag choice is one line and this refusal reverses. **DONE, §95** — the
   move landed, the flag choice is one line in `classify`, and the refusal
   reversed at +2 cases and **wrong 79 → 64**. Note that §95 is the one build of
   the session that moved `checker_types` (3,843 → 3,846, upward); if the
   `.types` workstream objects, the revert is that one line.

3. **TS2300's 61 false positives** — the largest wrong column this workstream
   owns, and `augmentedTypesModules` / `duplicateExportAssignments` /
   `es6ImportNamedImport*` name the family: declaration merging upstream permits
   and this binder rejects. **But see §89's correction to §88**: `extraonly`
   lists none of them, so none is a *sole* obstacle and they are not 61
   conversions. Price them with `extraonly`, not with `diag2307`'s wrong column.

4. ~~`isBlockScopedNameDeclaredBeforeUse`~~ **DONE, §99** — refused three times
   (§96, §97, §98) and landed on the fourth at **+2 cases, wrong 21 → 1, LOST
   0**. The last blocker was §83's `declaration_is_in_an_ambient_context`
   holding only two node kinds; a `declare const` puts the modifier on the
   enclosing `VariableStatement`. **§100 then closed it at ZERO wrong**: the last
   line was a `const enum` used early, which upstream ignores because a const
   enum is inlined, and `classify` had collapsed `const enum` into
   `REGULAR_ENUM`. Final: **9 converts, 0 LOST, 0 wrong.** TS2449's remaining
   rows still want the deferral arms this build did not port (JSDoc, the
   instance-property `isStatic` split).

   §101 then retired §83's `extends` bound (+0 cases, +2 right lines): the class
   arm now asks what `checker.go:1922` asks at every position. **Final state:
   9 converts, 33 right lines, 0 wrong, 0 LOST.** Unported deferral arms with no
   corpus case behind them today: JSDoc, the instance-property `isStatic` split,
   the binding-element recursion, the `legacyDecorators` arms.

   **The `classify` collapse audit is DONE, §102, and found nothing further.**
   All eleven excludes masks match `ast/symbolflags.go:52-74`; `S::ALIAS` over
   five import/export forms and `S::PROPERTY` over five node kinds are *not*
   collapses, because upstream makes no distinction there either. The
   §93/§95/§100 pattern had two instances, not four, and both are fixed. One
   divergence remains and is **not this workstream's**: enum members are filed
   in `Members` here and in the enum symbol's `Exports` upstream
   (`binder.go:436`) — already tracked with a pinning test.

   **Keep the rule the pattern produced:** *when a consumer's faithful test
   gives an unfaithful answer, suspect the flag before the test.* It cost two
   wasted first attempts (§94's mask, §100's checker-side test) to learn.

5. **The rest of `diaggap`'s relation-free single-code column** — re-run it;
   TS2693, TS2364, TS2703, TS2558 were 9/7/7/6 before this session's builds.

6. **TS7026 — still refused**, 28 sole-obstacle cases plus ~13 of TS2454's
   (§85). Both want binder-level `declare global` / module-augmentation merging.
   Worth a session on its own the moment that lands.

7. **TS1212 (104 lines)** needs `alwaysStrict` inside `tsr_binder::bind`. **But
   check §82 and §89 first**: the same "binder blocker" sentence was carried for
   TS7027 across three handoffs and was false — the binder already recorded the
   answer per node. **Ask what the binder already knows.**

8. **The assignability family** is §5's standing refusal, priced at 947 cases —
   more than half of what the suite passes. A `checker_types` build.

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


## FINDINGS THE THIRTEENTH SESSION PAID FOR — do not repay

- **A DECLINE CAN CONCEAL A BUG RATHER THAN PREVENT ONE.** §90 lifted one
  decline (a generic base class) and immediately measured **five new wrong
  lines** — which turned out to be a *pre-existing* defect the decline had been
  hiding: the base-class hop advanced while no class had a constructor **with a
  body**, so it walked past `declare class`'s ambient overloads into its base.
  Fixing it removed the five and **four more that predated the build**. Lifting
  a decline is the only way to find out which kind it was.
- **A flow analysis you cannot run still has inputs you can read.** §6 declined
  TS2564 for every class with a constructor because ADR-0012 forbids
  synthesising the node upstream's flow query needs. §87 took +16 by asking the
  one sub-question that needs no query: a constructor that never *mentions*
  `this.<name>` cannot assign it on any path. Look for this shape in every
  "cannot synthesise / cannot instantiate" decline.
- **Arity is independent of generics** (§90), and more generally: when a rule
  computes two answers at one gate, check whether the gate is really required by
  both. TS2564, TS2554 and TS2345 all shared one, and only one of the three
  needed it.
- **The binder is not where you think.** TS2300 on a duplicate type parameter is
  a **checker** rule comparing *symbol identity* (`checker.go:7002`), and it
  works precisely *because* the binder merged the duplicates silently. Before
  filing something as a binder gap, ask whether the binder's silence is the
  input to somebody else's test.
- **Unreachable code comes in RUNS, not statements and not lists** (§89), and
  `isSourceElementUnreachable` asks a **different question per kind**: a
  namespace that emits no JavaScript is not unreachable *code*.
- **A second message can hide behind one code.** `Expected 1-2 arguments, but
  got 0.` is TS2554, the same `Message` as `Expected 1 arguments`; upstream
  composes the range into `{0}`. There is no `Expected_0_1_arguments_but_got_2`
  to reach for in `messages.rs`.
- **`getMinArgumentCount` counts BACKWARDS** (§91). Upstream resets it at every
  non-optional parameter, so it is the position after the *last required* one,
  not the index of the *first optional* one. The two agree on every signature
  with trailing optionals — which is why the forward reading survived twelve
  sessions — and disagree on `function f1(a, b = 0, c)`. When a computation
  "obviously" has two equivalent formulations, check the one upstream wrote.
- **Price a false positive with `extraonly`, never with `diag2307`'s wrong
  column.** The wrong column counts *lines*; only `extraonly` says whether a
  case is one false positive from passing. §88 got this wrong about TS2300's 61
  lines and §89 corrects it.
