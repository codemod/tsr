THE `diagnostics` WORKSTREAM'S HANDOFF.

`TASK.md` is the `.types` gradient workstream's. They are separate files because
they were overwriting each other. Nothing below touches `checker_types`, and
every `diagnostics` build of the eleventh session left it unmoved.

FIRST: git pull. Then read, in this order:
  STATUS.md §1's `diagnostics` block and §5's `diagnostics` refusals, then
  docs/architecture/checker-notes-diag2.md **§54 first** (the instrument that
  reframed the board), then §42–§55 in order. Every section is a build or a
  refusal with its number.

STATE AT HANDOFF (verify with a fresh coverage run):
  diagnostics    1,239/5,488 = 22.58%   (was 1,118 = 20.37%; +121 over 17 builds,
                 **zero cases lost in any of them**)
  checker_types  3,683/9,538 · 82.85% — the other workstream's. Do not touch it;
                 re-check it is byte-identical after every build.

## READ §54 FIRST, AND RUN ITS INSTRUMENT FIRST

`examples/diagreach.rs` (new) counts the cases that report **nothing extra** and
whose every missing diagnostic carries a code some rule here **already emits** —
cases that need no new rule at all, only completeness.

```
cases reachable by deepening existing rules: 1,283      (against 1,239 passing)
TS2322 549 · TS2345 137 · TS2339 128 · TS2741 80 · TS2454 56 · TS2353 52
TS2304  40 · TS2554  36 · TS2564  36 · TS2411 34 · TS2352 32 · TS7006 28
```

**This is the board now.** Eleven builds of this session each took a
`diaggap.rs` single-code row of 12–16 cases and converted 1 to 3, because that
column says *which* code a case is blocked on and not *how many* of it the case
needs (§2, and §53 is the extreme at **71 right lines for one conversion**).
`diagreach` asks the question that actually predicts a conversion. §55 was chosen
off it and is the session's largest build at **+43**.

**The arithmetic trap it comes with, paid for once.** A *converted* case leaves
the reachable set by definition, so `before − after` over-counts the damage from
new false positives by exactly the conversions. §55's real cost was **10 cases
pushed out of reach against 43 banked**, not the 53 the naive subtraction
suggested. **Count what a wrong line costs in CASES, not in lines** — every
"gained per wrong" figure older than §55 in `checker-notes-diag2.md` is a line
ratio, and the two differ by whatever the concentration happens to be.

## THE INSTRUMENTS — use them, do not rebuild them

| instrument | answers |
|---|---|
| `examples/diagreach.rs` | **NEW, run first.** Cases reachable by deepening the rules that already exist, ranked by which rule |
| `examples/diagmissing.rs` | **NEW.** `diagmissing -- 2454` prints every baseline line of that code the port does not emit, restricted to cases that code alone blocks. Four of this session's builds were steered by reading it |
| `examples/diagcase.rs` | **NEW.** One case's expected and actual diagnostics side by side |
| `examples/diag2307.rs` | the per-rule counterfactual. Put the rule's codes in `RULE_CODES` **alone**, measure, then **restore the full list** (one commit shipped it pinned and needed a follow-up). CONVERTS / LOST / RIGHT / WRONG |
| `examples/extragap.rs` | splits the *extra* column into `displaced` and `invented` |
| `examples/diaggap.rs` | the old board. Now an ordering over **new** rules only |

**Isolate a code before quoting its wrong column.** `diag2307.rs` prints only the
first 400 distinct entries; §50's bar was registered against "16 wrong lines"
read off a truncated full-list printout when the real figure was **96**.

## THE SESSION'S ONE TRANSFERABLE FINDING

**Five of the first seven builds turned on a sentence about *why*, not on a
threshold.** In each, the port had transcribed something faithfully-looking that
was answering a different question:

1. **A printing guard applied where nothing prints** (§42.1). `union_type_worker`
   answers `errorType` for a union with a named constituent so `E | undefined`
   does not print `E.a | E.b | undefined`. TS2454 compares
   `(file, line, column, code)`, and the guard was silencing it on every
   enum-typed declaration in the corpus. `get_union_type_unprinted` is the fix.
2. **A comment describing three cases above code handling one** (§43).
3. **`== self.intrinsics.error` where `Checker::is_error` was meant** (§43, §44).
   This port has **two** error types — the intrinsic, and the `Named` an
   unresolved type reference mints — and upstream has one, `TypeFlagsAny`-carrying.
   **31 more identity tests against `intrinsics.error` remain in the checker and
   each is the same question.**
4. **A decline copied with its conclusion and not its cause** (§45). §31's
   `same_primitive_family` reads as a statement about comparability and is really
   a stand-in for `getBaseTypeOfLiteralType`, which only assertion sites apply.
   Inheriting it into TS2367 read 2 conversions instead of 15.
5. **A literal's text compared against a literal's value** (§47). A
   `NumericLiteral`'s `Text` upstream is the scanner's **normalised** value —
   `0.0` reads `"0"`.

And the site trap fired **four** times: `registerForUnusedIdentifiersCheck` (§15),
`checkTruthinessExpression`'s seven call sites (§47), an earlier `match` arm
already claiming `+`/`<`/`>` (§49 — the rule measured zero because it never ran),
and `checkNonNullType` being conditional for `+` alone (§50.2).
**When a new rule measures zero, check that it ran before checking what it
decided.**

## THE THREE-VALUED RELATION HAS THREE CORRECT PROJECTIONS

Which one is right is a property of the **caller's direction**, and getting it
wrong is worth tens of lines each way:

| the caller | the reading |
|---|---|
| reports **because** a relation failed (§25, §52) | fire only on `== NotRelated` |
| stays silent **because** a relation held, upstream three-valued (§49) | `!= NotRelated` |
| stays silent because a relation held, upstream **binary** (§50.2) | `== Related` — reading `Unknown` as a positive declined 28 correct lines |

## RANKED NEXT ITEMS

0. **Run `diagreach.rs` and pick from it.** Everything below is that list read
   at this commit.
1. ~~`Binder::names_in_scope` should take a meaning~~ **DONE, §57, +7** — and
   the *value* arm wanted the filter too, which this file had twice argued was
   unnecessary. `names_in_scope_with_meaning` exists now.
2. **TS2454 (56) / TS2554 (36) / TS2564 (36) / TS2411 (34) / TS7006 (28)** —
   five rules `diagreach` says are incomplete and **none of which needs the
   relation**. Read `diagmissing.rs` for each; that is exactly how §43, §45,
   §47, §55 and §56 were found. TS2564's remaining 27 are behind a *type* that
   does not resolve (`missingTypeArguments1`, `privacyVarDeclFile`) and are a
   `checker_types` question; TS2554's are `callWithMissingVoid`'s `void`
   parameter, which §56 left to its own measurement.
3. **`checkNonNullType` at the OTHER call sites** — property access, element
   access, call targets. §51 built the six messages and restricted them to
   binary operands, and its twelve non-converting cases are all `STILL SHORT`
   for want of those sites.
4. **TS2341 (15) and TS2305 (15)** are the largest unbuilt rows left.
   TS2305 (`Module '{0}' has no exported member '{1}'`) has many sibling codes
   — TS2459, TS2613, TS2614 — and picking the branch **is** the rule
   (`reportNonExportedMember`, `checker.go:14908`).
5. **TS1212 (22)** still needs compiler options plumbed into `tsr_binder::bind`,
   which nothing does today. Unchanged from the tenth session's handoff.
6. **The assignability family (TS2322 548, TS2345 138, TS2741 81, TS2353 52)**
   is §5's standing refusal and `diagreach` now prices it at more cases than the
   suite currently passes. It is a `checker_types` build, not a diagnostics one.

## TRAPS PAID FOR, DO NOT REPAY

- **The parse-error gate is a per-rule measurement, not a house style.**
  §40.3 deleted TS2304's for +6 and §50.1 re-measured it at −6 seven builds
  later; §43 refused TS2564's at −1 even though it zeroed that rule's wrong
  column. A zero wrong column is not the objective.
- **`GetErrorRangeForNode` exists now** (§48, `Checker::error_span`) and every
  `report` site goes through it. Four of upstream's arms need the file's text
  and are unported — `SourceFile`, `ArrowFunction`, case/default clauses, and
  `return`/`yield`/`constructor`. A displaced `return` diagnostic is that.
- **Three declared flags are set by nothing**: `NodeFlags::AMBIENT`,
  `NodeFlags::JAVASCRIPT_FILE`, `SymbolFlags::OPTIONAL`.
- **A rule's yield is not its row.** Check `STILL SHORT` before pricing.

## THE LOOP, sixty-two for sixty-two

`diagreach.rs` for the target → `diagmissing.rs` for the lines → isolate the
codes in `diag2307.rs`'s `RULE_CODES` → **register the bar in
checker-notes-diag2.md BEFORE the code** → build anchored to upstream file:line
→ measure → READ THE TOP ROW OF THE WRONG COLUMN and **decline with a named
owner** (never "tighten") → **restore RULE_CODES** → coverage → five gates each
its own invocation (fmt · clippy `grep -c "^error"` == 0 · test · anchors ·
issue-ids) → commit → STATUS → push. LOST must read 0 in every measurement.

Push before you stop. A build that is not pushed did not happen.
