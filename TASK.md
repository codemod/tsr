FIRST: git pull. Read STATUS.md §1, §4.2a, §5, §7's top rows, then
docs/architecture/checker-notes-assign.md §7–§12 (the ninth session's spine:
five bars, four fired legs honoured, two overridden loudly) and
checker-notes-callres.md §14–§18. The last ~600 lines of docs/conventions.md
still pay.

STATE AT HANDOFF (verify with a fresh coverage run):
  checker_types 2,942/9,538 (30.85%) · 355,098/478,954 = 74.14% · gap 77,733
  · wrong 36,084. Ninth session: +2,371 lines, +101 cases, 0 lost,
  0 regressed, across thirteen bar-scored builds, two measured refusals, and
  one infrastructure fix. Arithmetic chain in §1.

THE METHOD, twelve-for-twelve: counterfactual/probe sizing → bar in docs
committed BEFORE code → build anchored to upstream file:line → tests FROM
BASELINES → verdictdump pair → score IN WRITING → five gates → STATUS → push.
A fired leg is honoured by narrowing (nine times this session) or overridden
loudly on independent evidence (§8.1, §12.1). Read residuals even when the
ratio passes — §16.1 caught two latent wrong-rules inside a passing bar.

WHAT THE SESSION OPENED, in value order:
  1. DONE same session (§13.1, +33): the array consumer landed gated on the
     provably-uncontextual position. The annotated positions stay gaps —
     their members keep literals under contextual typing
     (isLiteralOfContextualType), which is that refused subsystem's row.
  2. DONE as a sizing (switchgap.rs, narrow notes §8.1): tsr-5kii is <=104
     reachable lines across ~23 cases — an effort-3 mechanism that does NOT
     clear the board. The 4 aggregate-exposure wrongs stay priced at that.
  3. HALF DONE (callres §19.1, +151/0): the enclosing-scope half landed —
     two scope rules learned from losses (shadowing chains keep the written
     name; computed names and heritage clauses sit outside their class's
     param scope). REMAINING: the within-print half, 322 lines — REFUSED TWICE
     (callres §20.1: 149 lost at the shared join, −645 at the synthesized
     joins). Written composites keep sibling names verbatim
     (declarationEmitTypeParameterNameReusedInOverloads) while the IPromise
     head renames; the discriminator is the print context's identity claims.
     Prerequisite: a per-line study of the two head families side by side.
     Do NOT attempt a third join placement.
  4. tsr-5o2 (written-annotation reuse in signature prints) surfaced 4 more
     lines in §11.2 — its row keeps growing as aggregates land.
  5. DONE same session, all three verses (assign notes §14/§15/§16):
     readonly, optionality (both directions) and privacy live in the relation
     now; the §9 syntactic decline is deleted whole. The relation's remaining
     unread axes: property-vs-method, index/call signatures — each a future
     verse with the same recipe.

NEXT HEAD, direction persisted before code (ninth session's tail): the
`any → undefined` W2 row — 548 wrong lines, 12 case-finishes, head cases
capturedLetConstInLoop* and jsxEsprimaFbTestSuite. Shape: `let x;` (auto
type) referenced inside a CLOSURE — upstream cannot track the assignment
across the function boundary and answers the declared auto → `any` (an
HONEST any, the anySignature precedent; write the ADR-0038 argument from
checker.go's auto-type conversion before building). Our flow narrows to the
initial `undefined` instead. CLOSED (narrow notes §9.1–§9.8): the
family landed as three sound arms — the non-strict auto initial (+1,016),
the compound-assignment target (+6), and the outer auto reference (+42,
four measurements teaching isNeverInitialized / mutable-local / var-vs-let
as ported clauses) — after the sentinel form was refused (+989, cache
pollution) and the discriminator studied. The binder's per-container START
nodes turned out to BE upstream's flowContainer bound. Remainder: the
ever-assigned scan's granularity, small, owned in §9.8.

THE NEXT SUBSYSTEM, from the closing W1' board: the LOOP FIXPOINT
(upstream's incomplete-types iteration, getTypeAtFlowLoopLabel). The
`intrinsic -> intrinsic` W1' row is 5,016 lines / 99 finishes across 768
cases; its two ceiling heads (binaryArithmetic 1,232, selfReferentialLoop
145) leave a ~3,600-line diffuse tail that every earlier residual attributed
to loop-label handling (e7a65fb's 260, §9.2's leak, capturedLet). The
current loop-label arm's shape and upstream's protocol (cache the incomplete
result, iterate to fixpoint, the evolving-type interaction) is the study;
this is subsystem-scale — one session's whole deliverable, per STATUS §4.4's
standing rule.

TRAPS PAID FOR THIS SESSION, do not repay:
  - NodeFlags::JAVASCRIPT_FILE and AMBIENT were both declared and set by
    nothing; JAVASCRIPT_FILE is now stamped at the program's TWO parse sites
    (Program::new AND the loader — a stamp at one site measures as zero).
  - The non-strict Base*Facts delta (checker.go:467) belongs at the
    whole-operand question in ||/??, NOT inside get_type_facts — a global
    placement leaks into truthiness narrowing (§8.1 lost 2 lines).
  - A never-falsy/never-nullish left short-circuits ||/?? to the LEFT type
    (`1 || 2` is `1`); multi-return unions KEEP regular literals
    (capturedLetConstInLoop8) — two intuition comments claimed otherwise and
    both were wrong (the sixth and seventh such).
  - get_union_type collapses `never` BEFORE any gate can see the pair — pair
    gates must run on the pair, not the union (§9's fourth measurement).
  - Upstream's useOnlyExternalAliasing is FALSE in the baseline path (only
    hover sets it); same-file `import a = b` renames chain SEGMENTS, never
    the whole printed name (privacyGloImport pins both in one file).
  - A `.js`-family unit inside a .ts case file is JS: check the UNIT name.

Seventeen unported-stand-in fixtures have now come due across the project.
When a build goes green somewhere unexpected, grep tests for the mechanism
name before assuming the build is wrong.

issue-ids SKIPS locally (no beads DB) — a skip, not a pass. Say so in every
session-close rather than calling the gate green.

Do not stop for no reason. Keep grinding.
