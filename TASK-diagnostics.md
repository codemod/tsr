THE `diagnostics` WORKSTREAM'S HANDOFF.

`TASK.md` is the `.types` gradient workstream's and the two have been
overwriting each other — the ninth session's diagnostics handoff was replaced in
`TASK.md` by the parallel session's before it was read. They are separate files
from now on. Nothing below touches `checker_types`, and every `diagnostics` build
this session left it byte-identical.

FIRST: git pull. Then read, in this order:
  STATUS.md §1's `diagnostics` block (rewritten — the tenth session's chain and
  the reason 50% was not reachable), STATUS.md §5's new `diagnostics` refusals,
  and docs/architecture/checker-notes-diag2.md §15–§20 end to end. Every section
  there is a build or a refusal with its number.

STATE AT HANDOFF (verify with a fresh coverage run):
  diagnostics    1,109/5,488 = 20.21%   (was 717 = 13.06%; running total 80 → 993)
  checker_types  3,043/9,538 · 74.46% gradient — the other workstream's

WHAT THE SESSION WAS ASKED FOR, AND WHAT IT MEASURED
  The instruction was "do not stop until 50%". **50% is not reachable by adding
  rules, and this is now a measured statement rather than an opinion.**
  Measured three ways, not asserted once. (1) `diaggap.rs`'s single-code column
  sums to 3,217 cases and 50% of the suite is 2,744 passing, so it means
  converting essentially every row. (2) **The session then built the members
  subsystem the ninth handoff named as the prerequisite** — `member_completeness`
  — and ran three rules on it: TS2339's 143-case row converted **12**, TS2741's
  40-case row **1**, TS2353's 37-case row **6**. The `STILL SHORT` column says
  why: 22, 15 and 2 cases gain a *correct* diagnostic of that code and still
  fail. **The single-code column measures which code a case is blocked on, not
  how many of that code it needs** — this project had been reading it as the
  latter. (3) TS2403 and ~70 cases with it are behind `isTypeIdenticalTo`, a
  relation mode that does not exist (§24).

  The costed forecast: the relation and the members table together are worth on
  the order of 800–1,000 cases, landing the suite near **30%**; the rest is a
  long tail of rows worth 10–30 cases each. That is a multi-session number.

THE NUMBER THE NEXT SESSION SHOULD START FROM
  The first TS2322 build was gated on nothing but the error type and measured
  **947 right against 988 wrong** — the relation disagreeing with upstream on
  half the assignments it was asked about. That is `checker_types`' 26%
  non-gradient arriving as diagnostics, and it is the first direct measurement
  of *"an incomplete relation does not report less, it reports wrongly."*
  It is also the sizing for the subsystem: finish the members table and roughly
  900 already-correct TS2322 lines stop being wrong.

THE INSTRUMENTS — use them, do not rebuild them
  examples/extragap.rs   **NEW and the one to run first.** Splits the *extra*
                         column into `displaced` (same code missing elsewhere in
                         the file — one diagnostic in the wrong PLACE) and
                         `invented` (the baseline never mentions it), and ranks
                         by "cases where one code, right in both directions,
                         finishes the case". It found TS1160 — 12 cases for one
                         argument — which `diaggap.rs` could not have.
  examples/diaggap.rs    the board. Single-code column is the ONLY forecastable
                         one. RE-RUN AFTER EVERY RULE — rows GROW as rules land.
  examples/diag2307.rs   the per-rule counterfactual. Put the rule's codes in
                         RULE_CODES **alone** to isolate it, measure, then
                         restore the full list. CONVERTS / LOST / RIGHT / WRONG.
                         LOST must read 0. It calls `diagnostics_suite::
                         reported_for`, so probe and build cannot diverge.

READ §25 FIRST. The single most valuable thing this session found is that
`crate::relater` is **three-valued** and every consumer that acts on a *negative*
must call `relate_ternary`, not `is_type_assignable_to`. The binary projection
collapses `Ternary::Unknown` into `false`, and that one wrong call produced 988
wrong lines, a bogus diagnosis ("the relation disagrees half the time"), and a
primitives-only gate that suppressed four whole rows. Correcting it was **+37
cases across four builds** with zero losses. **Before writing any rule that
reports because a relation failed, check which function you are calling.**

RANKED NEXT ITEMS, with what each is actually blocked on
  0. **More consumers of `relate_ternary`.** TS2411 is DONE (+2). TS2352 is DONE (+4, 0 wrong) with
     assignability standing in for comparability behind two named declines (§31).
     **TS2367 (no-overlap comparison, 23 cases) remains** and is the same
     substitution at a different site — §31 prices what `isTypeComparableTo`
     would buy.
  0a. **TS2454's row is on the MISSING side, and that is now measured — the
     best-priced single build left on this page.** Its extras are closed (114 →
     10 wrong lines across three declines) and *none of the three converted a
     single case*. `extragap.rs` says **66 cases have TS2454 as their sole
     obstacle**, so ~60 are missing diagnostics. The bound is documented at
     `check_used_before_assigned` and excludes **outer variables, parameters,
     aliases and binding elements** — the ninth session's named residual,
     "outer variables and assignment marking". Worth ~60 cases.
     **ONE APPROACH ALREADY REFUSED, tenth session's last measurement.** Lifting
     the `isOuterVariable` refusal in favour of a *syntactic* `isNeverInitialized`
     — "no identifier of this name stands in a write position anywhere in the
     file" — measured **+5 converts, 4 LOST, wrong 10 → 65** and was reverted.
     The scan is not the question: upstream's `isSymbolAssignedDefinitely` reads
     `markNodeAssignments`, which records assignments *per symbol* during the
     bind, and a name-based scan cannot tell one `x` from another's. The build is
     `markNodeAssignments` in the binder, not a walk in the checker.
  0a1. **THE DECLINE AUDIT IS EXHAUSTED — do not re-run it from scratch.**
     Every condition in every rule of this workstream was disabled and measured
     at the tenth session's close. Twelve paid (+34 cases, `checker-notes-diag2.md`
     §35–§40.6); **~25 more returned zero or negative** and are listed there as
     confirmed-exact. Re-running the whole set costs about 25 coverage runs for
     nothing. **Run it again only after a build that changes what the port can
     *decide*** — §25 (`relate_ternary`) and §35 (`Anonymous` completeness) are
     the two that invalidated gates written before them, and both were
     invisible until the audit.
  0a2. **BUILD A PASS-SET DIFF PROBE BEFORE ANY MORE POSITION WORK.**
     `diag2307.rs`'s *before* side removes a code entirely, so it cannot see a
     rule being **moved**: TS2300's `export =` arm measured +2 converts / −17
     wrong in the probe and **−3 on the suite** (§33.1). What is needed is a
     probe that diffs the suite's pass set across two builds, not across a code
     list. §30's three position fixes were additive and got away with it.
  0b. **The rest of `extragap.rs`'s displaced column.** DONE and worth +29
     between them: TS2300 (47 displaced, +11), TS1160 (12, +12), TS1125 (86, +5),
     TS1002 (16, +1). **TS1109's 18 are the only untried remainder.** TS1005's
     **241 displaced are NOT this shape** — JSX and conflict-marker parser
     *recovery* differences, 32 lines at one EOF position in
     `jsxUnclosedParserRecovery` alone; that is a parser project, not a position,
     and this line exists so nobody re-derives it. A measured negative beside it:
     the `s.pos` correction does **not** generalise to the unterminated *regex*
     and *JSX string* sites — applying it there cost 2 cases and was reverted.
     The rule is "report where upstream's call reports", not "the scanner always
     reports at `pos`".
  0c. **More definite negatives the relater declines to give** (§29). It answers
     `Unknown` for any pair it did not reach structurally, and two of those are
     decidable from flags alone: an object source against a primitive target, and
     a primitive source against a target requiring a property. Both are asked in
     the rule rather than in `crate::relater`, because widening what the relater
     calls a definite negative moves `checker_types`. **Look for more of these
     before writing another rule** — the first one was worth +12 cases.
  1. **`isTypeIdenticalTo` — the relation in identity mode.** `crate::relater`
     has assignability and subtype and no identity. It is a relater build rather
     than a diagnostics one and it is the best-priced item on this page: TS2403
     30 cases, TS2320 14, TS2717 and TS2394 13 each — **~70 cases behind one
     function**, and §24 is the measurement that says interning cannot stand in
     for it.
  2. **TS2741's four residual owners** (§22). The machinery is built and switched
     off behind `REPORT_MISSING_REQUIRED_PROPERTY`; the blockers are the binder's
     numeric-name normalisation, private-identifier table keys, an unread
     inherited modifier, and narrowing. Turning the constant back on is the whole
     of the code change.
  3. TS2322's remaining anchors — object-literal property assignments, JSX
     attributes, `as`/`satisfies`. Each is worth its own measurement against the
     existing gate in `assignreport.rs`, which is proven at 0 lost.
  4. TS2304 residual 86 and TS2454 residual 62 — the two largest non-relation
     rows. Diffuse; the ninth session's notes name TS2583 lib suggestions and
     class/namespace scope divergences in `resolve_name` as the owners.
  5. The strict-mode binder family — TS1212 (22 cases) and TS1100 (11).
     `checkStrictModeIdentifier` / `checkStrictModeEvalOrArguments` are
     **unported in `tsr-binder`** and the crate's own module header says so.
     Needs compiler options plumbed into `tsr_binder::bind`, which nothing does
     today.
  6. TS7031 / TS7005 / TS7008 / TS7010 — the rest of the `noImplicitAny` family.
     Same option gate as `implicit_any.rs`, so the same bounded blast radius.
  7. The extras column: 36 cases blocked by an unexpected diagnostic **alone**
     and 586 by both. Removing a false positive can only help, and it is the one
     kind of work that cannot produce a new wrong line.

TRAPS PAID FOR THIS SESSION, do not repay
  - **A register site inside a conditional is a precondition of the rule.**
    `registerForUnusedIdentifiersCheck(sourceFile)` sits inside
    `if IsExternalOrCommonJSModule` (`checker.go:2210`). Missing that reported
    every top-level `class`, `function` and `namespace` of every script in the
    unused corpus: 108 of 158 wrong lines and all three losses. Same class of
    error as the ninth session's `Pos()`/`End()`: the code was copied and the
    question it was answering was not.
  - **The error node can differ by direction.** TS2554 reports on the callee for
    too *few* arguments and on the first excess argument for too *many*
    (`checker.go:9770` vs `:9804`). Getting one half right looks like a working
    rule and is 16 wrong lines.
  - **`isUse` is not always true.** `getResolvedSymbol` resolves with
    `isUse: !IsWriteOnlyAccess(node)` (`checker.go:13896`), so `y = 1` is not a
    reference to `y`. `unusedLocalsInMethod3` reads *"All variables are unused"*
    upstream for `var x, y; y = 1;`.
  - **A directive's position is `lastLineStart`, not the `/*`.**
    `processCommentDirective` (`scanner.go:674`). And `isCommentOrBlankLine`
    (`program.go:1445`) recognises `//` and **not** `/*`. Both are tests in
    `crate::comment_directives` now; each was three wrong lines.
  - **JSDoc is a wrong-answer generator for every option-gated rule.** A `.js`
    file's `@param` and `@type` supply types this port does not parse, so a
    checked JS file reports an implicit any on every *annotated* parameter.
    28 wrong lines in one measurement, 15 of them on one line of source.
  - **TS2322's message arguments are not compared.** The suite compares
    `(file, line, column, code)`. Nothing in that family needs type printing.
  - **Interning is not an identity relation** (§24). Two `{}` type literals are
    two anonymous types; `TypeId` equality models `isTypeIdenticalTo` for
    primitives, literals and named references and for nothing structural.
  - **`SymbolFlags::OPTIONAL` is declared and set by nothing** — the third
    writerless flag, after `NodeFlags::AMBIENT` and `NodeFlags::JAVASCRIPT_FILE`.
    Read optionality off the declaration's `?`.
  - **A rule's yield is not its row.** TS2339 converted 12 of 143, TS2741 1 of
    40, TS2353 6 of 37, and TS2322 emitted **294 correct lines to finish 15
    cases**. Check `STILL SHORT` before pricing anything from `diaggap.rs`.
  - **`is_type_assignable_to` is the wrong function for a reporting rule** — see
    the top of this file and §25.

THE LOOP THAT WORKED SEVEN TIMES
  diaggap.rs for the size -> isolate the codes in diag2307.rs's RULE_CODES ->
  register the bar in checker-notes-diag2.md BEFORE the code -> build anchored to
  upstream file:line -> measure -> READ THE TOP ROW OF THE WRONG COLUMN and
  decline (never "tighten" — every fix this session was a decline with a named
  owner) -> restore RULE_CODES -> coverage -> five gates EACH ITS OWN INVOCATION
  (fmt · clippy 0 · test · anchors · issue-ids) -> commit -> STATUS -> push.

issue-ids and anchors both RUN here (0 dangling, 0 unresolved) — earlier handoffs
said issue-ids skips locally; it did not this session. Check rather than assume.

Diagnostics is a long tail with one subsystem at its head. Keep grinding, and
build the members table when the tail stops paying.
