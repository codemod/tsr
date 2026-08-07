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
  diagnostics    893/5,488 = 16.27%   (was 717 = 13.06%; running total 80 → 893)
  checker_types  3,043/9,538 · 74.46% gradient — the other workstream's

WHAT THE SESSION WAS ASKED FOR, AND WHAT IT MEASURED
  The instruction was "do not stop until 50%". **50% is not reachable by adding
  rules, and this is now a measured statement rather than an opinion.**
  `diaggap.rs`'s single-code column — the only forecastable one — sums to 3,217
  cases. 50% of the suite is 2,744 passing. So reaching it means converting
  essentially every single-code row *plus* a share of the multi-code tail, and
  the column's head is TS2322 510 + TS2339 143 + the
  TS2345/2741/2353/2411/2430/2416/2420 family ≈370 — **all of them behind one
  subsystem**: a resolved members table and a structural relation. A session that
  does not build that subsystem is working the long tail, and the long tail is
  what this one spent itself on (+176 across seven builds, 0 lost).

THE NUMBER THE NEXT SESSION SHOULD START FROM
  The first TS2322 build was gated on nothing but the error type and measured
  **947 right against 988 wrong** — the relation disagreeing with upstream on
  half the assignments it was asked about. That is `checker_types`' 26%
  non-gradient arriving as diagnostics, and it is the first direct measurement
  of *"an incomplete relation does not report less, it reports wrongly."*
  It is also the sizing for the subsystem: finish the members table and roughly
  900 already-correct TS2322 lines stop being wrong.

THE INSTRUMENTS — use them, do not rebuild them
  examples/diaggap.rs    the board. Single-code column is the ONLY forecastable
                         one. RE-RUN AFTER EVERY RULE — rows GROW as rules land.
  examples/diag2307.rs   the per-rule counterfactual. Put the rule's codes in
                         RULE_CODES **alone** to isolate it, measure, then
                         restore the full list. CONVERTS / LOST / RIGHT / WRONG.
                         LOST must read 0. It calls `diagnostics_suite::
                         reported_for`, so probe and build cannot diverge.

RANKED NEXT ITEMS, with what each is actually blocked on
  1. TS2741 / TS2739 / TS2740 — 40 cases, and the *right* next slice. When the
     relation fails because the source is **missing properties**, upstream
     reports one of these instead of TS2322 (`assignmentCompat1`). It needs one
     thing this port has no API for: **enumerate a type's properties**.
     `members.rs` exposes `get_property_of_type` and nothing that lists them.
     Building that list is the first step into the members subsystem, and it is
     the step that also retires the 988-wrong refusal above.
  2. TS2322's remaining anchors — object-literal property assignments, JSX
     attributes, `as`/`satisfies`. Each is worth its own measurement against the
     existing gate in `assignreport.rs`, which is proven at 0 lost.
  3. TS2304 residual 81 and TS2454 residual 62 — the two largest non-relation
     rows. Diffuse; the ninth session's notes name TS2583 lib suggestions and
     class/namespace scope divergences in `resolve_name` as the owners.
  4. The strict-mode binder family — TS1212 (22 cases) and TS1100 (11).
     `checkStrictModeIdentifier` / `checkStrictModeEvalOrArguments` are
     **unported in `tsr-binder`** and the crate's own module header says so.
     Needs compiler options plumbed into `tsr_binder::bind`, which nothing does
     today.
  5. TS7031 / TS7005 / TS7008 / TS7010 — the rest of the `noImplicitAny` family.
     Same option gate as `implicit_any.rs`, so the same bounded blast radius.
  6. The extras column: 36 cases blocked by an unexpected diagnostic **alone**
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
