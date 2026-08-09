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

STATE AT HANDOFF, fourteenth session (verify with a fresh run):
  diagnostics    1,961/5,488 = 35.73%   (+202 over ONE HUNDRED AND NINE builds (plus §235, a retraction), zero
                 lost — §156-§219).  The list-loop sweep is DONE (§219): ten
                 loops, four fixed, seven measured as having no case asking for
                 them.  `extragap` shows ~1,150 invented parser lines against
                 `extraonly`'s 25 cases — **`extraonly` is the narrowest
                 projection of the extra column** — and those lines are NOT in
                 the list loops.  Locating them is the open question.  `extraonly` fell 50 -> 26 cases, ALL of it from
                 four parser/harness fixes and not one rule.

  THE HIGHEST-YIELD MOVE THIS SESSION FOUND, stated for reuse: **when a row is
  expensive, check what has landed underneath it since it was priced.** Three
  instances — §166 turned §163's dead code into §167's +8; the `.types`
  workstream's `declare global` merge turned §171's refusal into §189's +14;
  §193's same-position guard turned §204 from a two-error problem into a
  one-condition one. None was predictable from the row.

## READ THIS FIRST: THREE REFUSALS WERE FILED AND ALL THREE WERE WRONG

This session filed three refusals that named a blocking subsystem, and reversed
every one of them **within the same session**, for a combined +20 cases that had
been parked:

| § | refusal said | truth | reversed by |
|---|---|---|---|
| 164 | `is_value_reference` is broken | it is not | §165, one measurement |
| 186 | alias resolution is unported | ported in the CHECKER | §187, one `grep` |
| 171 | `file_loader` ignores `<reference path>` | it follows it, with a test | §189, restoring the build |

**Every refusal that named a MEASUREMENT stood** — §163's "+2 for 238", §177's
"6 reports in 563 entries", §184's "8 not 25". What never stood was a sentence
of the form *"this row is blocked on X"*.

> **A refusal may state what was measured. It must not name a blocker unless the
> blocker was grepped for by the name upstream calls it.** A refusal is read for
> many sessions; a row is re-measured every time.

Corollary that paid twice here: **when a resolver or binder fix lands, re-run
the refused builds before believing their numbers.** §166 alone turned §163's
dead code into §167's +8, and §171's 65 wrong into §189's 9.

WHAT IS ACTUALLY LEFT, with fresh numbers:
  · relation + members — TS2322's split re-run after §166 and unchanged:
    335 wholly relation-gated, 132 anchor-gated (an UPPER bound, §177), 14 mixed
  · **THE PARSER IS DONE, and it was never the parse-error set.** §190 said one
    conjunct, §192 found the real defect one layer lower — `error_at` had no
    same-position guard — and both are landed (§193 +14, §194 checker_types +6).
    Historical note, since three attributions were needed: §190/§191 checked `tsr_parser` emits
    TS1005; what it lacks is the `IsLeftHandSideExpression` conjunct in
    `parse_assignment_expression`. **One line, and it works** — four right
    lines appear immediately. Refused only because recovery emits a spurious
    TS1012 beside each and the newly-erroring files lose rules to §179's gate:
    `checker_types` +6, `diagnostics` −6. Fix the recovery double-report first,
    then re-run the one line
  · partial symbol tables — TS2694's residual 4 (§187): an empty export table
    can be declined and a partial one cannot
  binder_symbols 8,459/8,459 = 100%     — held across a build that changed
                 `merge_symbol`'s behaviour
  checker_types  3,937/9,538 · 84.47% — the other workstream's, and it moves
                 HOURLY. Do not touch it, and do not compare against this
                 number — remeasure.

## READ THIS BEFORE PICKING ANYTHING OFF THE BOARD BELOW

**Two of this session's two builds were rows the board had already refused or
priced as a subsystem, and both were wrong for the same reason.** §156-§160 in
`checker-notes-diag2.md` have the detail; the operational version:

- **`diagmissing <code>` prints case names, and a row's case names are its
  attribution.** TS7026 was carried for six sessions as `declare global`
  merging. It is JSX (`jsx.go:1253`), and twenty-eight case names beginning
  `tsx` said so. §141's "~52-case merge owner" is really ~10.
- **Read the fixture before the FIRST refusal, not the fourth hypothesis.**
  TS1100 was priced as needing strict-mode tracking. Upstream's `Binder` struct
  has no such field, and `parserStrictMode3-negative.ts` — one line, no
  prologue, no module — reports TS1100 anyway. Twelve cases sat behind an
  inference nobody spent a `cat` on.
- **Read the upstream function END TO END before writing the report.** Three of
  the four causes of the merge build's first (failing) measurement were
  upstream branches that had simply not been ported: `SymbolFlagsAssignment`,
  the plain-JS suppression, and the `NamespaceModule`/TS2649 arm.
- **Do NOT use `git stash` for before/after.** It fired here: the new file was
  untracked, `git stash push` refused the pathspec, the `&&` chain
  short-circuited, and the unconditional `git stash pop` popped an unrelated
  stash from a previous session into the working tree. **Comment out the rule's
  single call site instead** — same checkout, both states, no stash stack.
- **Read the wrong column as a multiset DELTA against a before-run**, never as
  the run's total. The absolute column carries `giant.ts` and `reservedWords2`
  lines that predate any current build.

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

0ab. **READ THE EXISTING RULE BEFORE ASSUMING THE MISSING ARM** (§143). TS2540's
   11 cases are `++M.x` and look like a missing operator arm; `assignment_target`
   already handles `++`/`--`, and **§144's probe showed all four
   receiver-type gates PASS** (`err=false anyunk=false complete=true
   prop=true`), so §143's own attribution was wrong too. The failure is at
   `is_readonly_symbol` — which already has upstream's const-variable arm — so
   either `M.x`'s symbol is not `VARIABLE` here (a namespace export filed as
   `PROPERTY`, §102's `classify` territory) or `is_constant_variable` misses the
   `const`. **Next probe, one line: print `record.flags` and
   `is_readonly_symbol(property)` at that gate.** `diaggap` ranks by case count and says nothing about which layer a row
   needs.

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
   Everything below is that list read at §90's commit, and seven builds have
   moved it. The board re-taken at `58b5ed2` is in §161/§163.

0v. **THE GRAMMAR SEAM IS THE ONE THAT PAYS, AND §105 CLOSED IT PREMATURELY.**
   61 of this session's 115 cases came from rules §105 called exhausted:
   TS1212/1213/1214 (§162, +32), TS1028/1071/1155 (§179, +19),
   TS1015/1117/1221 (§180, +8), TS2364/2703/2371 (§182, +14). The shape that
   keeps working: a `checkGrammar*` function, no types, no symbols, no flow —
   and **three rules of thumb that each cost a measurement to learn**:
     - **The `else if` order is the specification** (§103), and **the node is
       whatever the upstream function ITERATES**, not the node the walk visits
       (§180). Hanging a rule off the member instead of the list multiplies
       every report.
     - **The parse-error gate is a per-rule measurement** and is worth running
       both ways every time (§179 measured it at −20 wrong / +0 cases).
     - **A shared helper is not a shared convention** (§182): two functions one
       paragraph apart skipped different spines *and* reported at different
       nodes.
   Remaining priced rows in this seam: TS1038 (7, the sixth arm of the
   `declare` chain — needs the five ahead of it), TS1044 (6), TS2355/TS2378
   (6 each, both need flow), TS1109 (11, parser's), TS1359 (7, needs
   `AWAIT_CONTEXT`).

0w. **TS2322 IS `checker_types`', AND THIS SESSION PROVED IT TWICE.** §172 split
   the 2,441 missing TS2322 lines by which gate declined them; §175 ranked the
   reporting anchors; **§177 then built the top-ranked anchor and got +1 where
   the ranking said 19**, because a position the walk never visits has no gate
   to observe and the split silently assumed the relation would succeed once an
   anchor existed. It does so 6 times in 563. **Do not build anchors 2–4 from
   §175's table on the strength of that table** — the cheap check is in §177:
   add the anchor behind a counter and read `REPORTED` against `entered`.
   `bd` has the gate filed.

0x. **§166 CHANGED WHAT EVERY MEANING QUERY IN THE PROGRAM ANSWERS.** Until
   this session `resolve_name`'s `globals` fallback ignored `meaning`, so every
   name in `globals` — all of `lib.*.d.ts` included — resolved under every
   meaning. **Any number on this board taken before `2dfe2a1` that depended on a
   meaning ladder is stale**, including §164's 238 wrong lines. Re-measure
   before quoting.

0z. **THE MEANING-MISMATCH ROW — read §163-§168 before touching it.** The
   TYPE-position half is **DONE** (§167/§168, +8, zero wrong). What is left is
   the **value-position** arm — TS2693, TS2708, TS2661 — which §164 measured at
   238 wrong lines *against the pre-§166 resolver*. That number is not valid any
   more. The refused source is described arm by arm in §163. TS2693 (9),
   TS2709 (9), TS2661 (9), TS2749 (4) are four arms of `onFailedToResolveSymbol`
   (`checker.go:1564`). It was built in full, measured at **+2 cases for 238
   wrong lines**, bounded to +6, and **refused**. Do not re-derive it: the code
   is described arm by arm in §163 and the two wrong attributions are in §164
   and §165. **The next action is measurement, not code** — both type-position
   arms measure 0 wrong AND 0 converts, which means the row is blocked on
   *position*, not on the cascade. `diagcase moduleWithNoValuesAsType` first.

0y. **TS7026 — PRICED AND REFUSED, §170/§171. Read it before touching the row.**
   It is a JSX rule (`jsx.go:1253`), it was built, and the forty lines are the
   *worthless* part. Ranked truth:
     1. **`file_loader` must follow `/// <reference path="/.lib/react16.d.ts" />`.**
        Every wrong line in the measured build is that one missing file. This is
        the same owner as the standing-LOST entries for
        `resolutionModeTripleSlash1`/`3`, so **that section is much bigger than
        three cases — it is the blocker on the board's largest row.**
     2. `declare global` merging — built and measured for the first time
        (§171): removes 20 of the 65 wrong lines, `binder_symbols` unmoved,
        `checker_types` +2, **`diagnostics` −1 and unidentified.** That one case
        is what the next attempt must explain, not bound away.
     3. the rule itself, already written twice.
   **Do not build (3) before (1) is measured.**

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

6. **TS7026 — 28 sole-obstacle cases, and it is a JSX build, not a merge one**
   (§158). `JSX element implicitly has type 'any' because no interface
   'JSX.IntrinsicElements' exists`, `jsx.go:1253`. It wants the `JSX` namespace
   and its `IntrinsicElements` interface resolved. **The largest single
   relation-free row on this board**, and nothing has priced it as what it
   actually is. §85's "~13 of TS2454's" was attributed by the same inference
   and is unverified — re-run `diagmissing 2454` and read the case names first.

6b. **What is left of the merge item is FOUR cases** (§160):
   `checkMergedGlobalUMDSymbol`, `umdGlobalAugmentationNoCrash`,
   `umdNamespaceMergedWithGlobalAugmentationIsNotCircular`,
   `duplicateIdentifierRelatedSpans_moduleAugmentation`. All four want UMD
   `export as namespace` merging or module augmentation. The cross-file global
   merge itself is DONE and took the other six.

6c. **TS1101 and TS1102 are the same shape as §156 and read no state at all** —
   `checkStrictModeWithStatement` and `checkStrictModeDeleteExpression`,
   `parserStrictMode14` and `15` are the fixtures. Both dispatch
   unconditionally from `bind` (`binder.go:637`, `:631`), like everything else
   in that family. Measure them with `diaggap` before building — §156's
   ceiling was real, but it was *measured*, not assumed.

7. **TS1212 (104 lines)** — this item said it "needs `alwaysStrict` inside
   `tsr_binder::bind`". **That sentence is now suspect for the reason §157
   gives**: its sibling TS1100 was priced the same way and needed no option and
   no strict-mode state. `checkStrictModeIdentifier` (`binder.go:1303`) is
   gated on parse errors, `NodeFlagsAmbient` and `NodeFlagsJSDoc` — **not** on
   strict mode. Read `binder.go:1303-1331` before pricing it again.

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

- **`compiler/validRegexp`** — an extra TS2304. ~~The parser reads an ambiguous
  `/` as division, so the regex body parses as identifiers. Owner:
  `tsr_parser`.~~ **CORRECTED, §213: the parser is right.**
  `Scanner::rescan_as_regular_expression` exists, is wired, and tracks
  character classes, so the literal scans correctly and the TS1005 lands at
  upstream's own column. The single remaining TS2304 is on the trailing `i`,
  and its owner is **TS2304's deliberate absence of a parse-error gate**
  (§40.3 +6, §50.1 −6, §79 *absent is right*) — a measured decision, not a
  defect. **A standing loss is a refusal with the bar left off; re-measure this
  list the way §194 says to re-measure a refusal.**
- **`conformance/resolutionModeTripleSlash1` and `3`** — ~~extra TS2304 on
  `MODULE`.~~ **CORRECTED, §214: they emit NOTHING now.** Both are one
  *missing* `TS2552` from passing, which is the opposite classification, and
  `extraonly` lists neither. Whatever produced the extra TS2304 is gone —
  §166's meaning filter and §208's export tables are the candidates, neither
  confirmed. **Re-measure before quoting this family as a false-positive
  blocker.** Original text, superseded: `/// <reference types="foo" />` against an `@types` package whose
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
