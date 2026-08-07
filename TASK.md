FIRST: git pull. Read STATUS.md §1 (the `diagnostics` block is rewritten and the
old "structurally blocked" reading is corrected there), then
docs/architecture/checker-notes-diag2.md end to end — it is this session's whole
product and every section is a build or a refusal with its number.

NOTE ON THIS FILE: it was untracked at the start of the eighth session and got
committed by a `git add -A` (95b52ee). It is now the tracked handoff. Delete it
from the index if that was not wanted.

STATE AT HANDOFF (commit d1cfef3, verify with a fresh coverage run):
  diagnostics    717/5,488 = 13.06%   (was 80/5,488 = 1.46% — 8.96x)
  checker_types  2,848/9,538 · 73.70% gradient — UNTOUCHED by this workstream,
                 byte-identical across all nine builds
A second agent worked the .types gradient in parallel (ArrowFunction,
ObjectLiteral, FunctionExpression rows); those numbers are theirs.

WHAT MOVED, AND WHY IT WAS AVAILABLE
  ADR-0040 diagnosed the flat `diagnostics` number correctly and structurally,
  and two handoffs then quoted that diagnosis as a SIZING. Nobody had measured
  what the blocked cases were blocked ON. `examples/diaggap.rs` (new) does:
  3,258 of 5,488 judged cases were blocked on EXACTLY ONE code. The suite is a
  long tail of independent rules, not one wall.

  Then ADR-0040 decisions (1) and (2) — a Checker-owned diagnostic collection
  and a `check_source_file` traversal (`crates/tsr-checker/src/check.rs`) — and
  nine builds: TS2307/2882 +50, TS2564 +133, TS2304 +115, TS2454 +233,
  TS2369/2695 +40, break/continue grammar +25, TS1036/1183 +24, overload
  implementation-expected +17. Two refusals with numbers: TS2339 (2 converts /
  254 wrong) and TS7026 (12 / 47).

THE INSTRUMENTS — use them, do not rebuild them
  examples/diaggap.rs    the board. Single-code column is the ONLY forecastable
                         one; the ceiling column is printed after it on purpose.
                         RE-RUN AFTER EVERY RULE — rows GROW as rules land.
  examples/diag2307.rs   the per-rule counterfactual. Add the code to
                         RULE_CODES and run: CONVERTS / LOST / RIGHT / WRONG.
                         It calls `diagnostics_suite::reported_for`, so probe
                         and build cannot diverge. LOST must read 0.

FIVE THINGS THAT COST A MEASUREMENT EACH — do not repay them
  1. `tsr_ast::NodeFlags::AMBIENT` is declared, documented and SET BY NOTHING.
     It caused three separate residuals (86 wrong lines, 4,781, 2). The check
     traversal carries the bit explicitly via `check::FileContext`; the real fix
     is the parser, and it is filed on `bd tsr-o9tl`.
  2. `Pos()`/`End()` in checker.go are FULL positions (trivia included) and
     `tsr_core::Span` is not. `prev.End() == next.Pos()` transliterated
     literally cost 480 wrong lines. Ask what the expression MEANS — usually
     "next sibling" — and answer that. ~2,000 sites carry this trap.
  3. A rule reporting on a NEGATIVE is worth nothing until its declines are
     right, and less than nothing before that: TS2304 scored -85 and TS2454
     -210 on `passing + single-code reachable` before their last refusals.
  4. A wrong column dominated by one case is one predicate. Read the top row
     before tightening anything — `genericDefaults`'s 227 lines were
     `declare const`.
  5. Upstream suppresses every grammar diagnostic in a file with parse errors
     (`grammarchecks.go:21`). This port's `file_has_parse_errors` gate is
     therefore FAITHFUL for the grammar family, not a deviation.

THE ORDERING RULE THIS SESSION BOUGHT
  A rule that reports on a SYNTACTIC fact has no incompleteness to leak. The
  three grammar builds converted 89 cases for ZERO wrong lines and needed no
  tightening pass; all four semantic rules needed two or three. Take the
  syntactic rows first even though they are smaller.

THE BOARD AT 717 (re-run diaggap.rs; these will have shifted)
  TS2322  543  needs assignability AND the reporting positions. ADR-0040's
               falsifier 3 measured 28 distinct anchors, modal BinaryExpression.
  TS2339  141  REFUSED, §9 — an absent property and an unbuilt members table are
               the same `None`. Returns when resolveStructuredTypeMembers
               carries an explicit resolved state per type.
  TS2345  103  assignability at argument positions.
  TS2304   82  residual of a shipped rule: TS2583 lib suggestions unported, plus
               class/namespace scope divergences in resolve_name.
  TS6133   80  noUnusedLocals. Needs reference marking (the walk already
               resolves every value identifier) plus upstream's grouping rules —
               TS6199 "All variables are unused", import grouping, the `_`
               prefix. Blast radius is confined to cases that SET the option,
               which makes it the safest large item left. NOT COUNTERFACTUALLED.
  TS2454   61  residual: outer variables and assignment marking.
  TS2564   37  residual: the constructor disjunct, needs a synthesised flow
               reference on an immutable tree (ADR-0012). Worth 32-37 cases.
  TS7026   27  REFUSED, §13 — `declare global` augmentation unported in the
               binder. Same prerequisite `checker-notes-jsx.md` names.
  Small syntactic rows, ~150 cases total and each worth roughly its row at
  roughly no risk: TS1212 22 (a BINDER diagnostic, not a checker one), TS1029
  12, TS2369-adjacent modifier grammar, TS1039, TS2391's siblings.

THE LOOP THAT WORKED NINE TIMES
  diaggap.rs for the size -> add the code to diag2307.rs's RULE_CODES -> build
  the rule -> measure -> READ THE TOP ROW OF THE WRONG COLUMN and tighten ->
  register the bar in checker-notes-diag2.md -> coverage -> five gates EACH ITS
  OWN INVOCATION (fmt · clippy 0 · test ok-blocks 110 · anchors · issue-ids) ->
  commit -> STATUS -> push.

Diagnostics is no longer blocked. Keep grinding.
