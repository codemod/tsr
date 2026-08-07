THIS FILE IS THE `.types` GRADIENT WORKSTREAM'S HANDOFF.
The `diagnostics` workstream's is **TASK-diagnostics.md** — the two were
overwriting each other in this one file, and the ninth session's diagnostics
handoff was lost that way before it was read.

FIRST: git pull. Read STATUS.md §1 and §7's top rows (the ninth session's
continuation appended SIXTEEN rows, builds 25–40), then
docs/architecture/checker-notes-narrow.md §12.6–§22 — that run is the
session's spine: the fixpoint landed by exonerating it, the too-large bail
(+10,000 in one build), and fourteen more single-rule decodes. The last
~600 lines of docs/conventions.md still pay.

STATE AT HANDOFF (verify with a fresh coverage run):
  checker_types 3,575/9,538 (37.48%) · 388,917/478,954 = 81.20% · gap 55,979
  · wrong 24,019. The continuation (builds 25–57): +32,264 right lines,
  +532 cases, +6.74 points, THIRTY-FOUR bar-scored builds and FIVE measured
  refusals/reverts, every fired leg honoured in writing. Chain in STATUS §1.
  Late finds worth reading first: §35 (tsgo prints `error` in JS chains —
  123 baseline files, ADR-0038's premise refined), §36 (the `unknown`
  inference fallback), §37 (tuples instantiate — `instantiate_type`'s
  oldest decline closed by `tsr-5ll`'s element lists), §33 (`globalThis`),
  §26-callres (the unique-symbol mint).

  THE BOUNDARY-ARGUMENT CHAIN was the continuation's largest seam — six
  hops, each "upstream's deliberate error-answer's observable IS `any`":
  §14 (TS2563 too-large, +10,000) → §27 (readonly targets, +155) → §31
  (TS2304 unresolved names, +6,267, five gate sets priced) → §32 (access
  through minted unresolveds, +4,263) → callres §23/§24/§25 (calls, super,
  new through the same provenances, +2,960) → callres §26 (the
  unique-symbol mint, +377). The gates that keep it honest: any-meaning
  resolution, `arguments`/`globalThis` (port misses = future rules),
  import-machinery files (by NODE kind — `export default interface`
  modifiers are invisible to it, a recorded miss), JS files at the call
  hops. ADR-0038's boundary is argued at each §; the fixtures that pinned
  the old reading were renamed with their new truth, eight stand-ins came
  due (twenty-second through twenty-eighth plus re-anchors).

  THE §29 SEAM IS NEW CAPABILITY (and §31's chain multiplied through it): an on-stack alias mention answers a
  memoized NAME placeholder (read-only `Resolutions::on_stack`, no failure
  marking) — upstream's member-type laziness at the one seam
  print-at-creation permits. It took the 2,000-line `BigUnion` mountain
  whole and dropped "type node unresolvable" 9,247 → 7,160. Anything else
  that errors only because a cycle passes through an alias should re-probe
  against it.

THE METHOD, unchanged and now ~40-for-40: counterfactual/probe sizing → bar
in docs committed BEFORE code → build anchored to upstream file:line →
verdictdump pair → score IN WRITING → five gates VERIFIED BY GREP (rtk
masks exit codes — a commit shipped claiming a green clippy that was red;
`grep -c "^error"` is the gate now) → STATUS → push.

TRANSFERABLE LESSONS THE CONTINUATION PAID FOR:
  - Trace ONE line before theorizing. The eight-cycle fixpoint "semantic
    residue" was cache poisoning (§12.6); the switch arm's +14/17 first
    pair was a token-kind confusion (§16) — both named by a single print.
  - The baseline's weird want usually IS upstream's rule. `foo(x) : never`
    = intersection of overload returns (callres §21); `AA : any` at targets
    = declared-type-at-definite-targets (§12.7); 10k want-any = TS2563's
    deliberate bail rendered as any (§14).
  - Identity breaks masquerade as relation gaps: the enum member's
    fresh/regular twin (§18) made `Choice.One -> Choice` undecidable.
  - Plumbing edits must assert their anchors (§21's silently-failed
    python replace, caught only by a byte-identical re-measure).

THE BOARD AFTER BUILD 40 (all remaining heads are subsystem-scale):
  1. contextual typing / inference aggregate — `number|string ← any`,
     ~3,000 lines across inferFromGenericFunctionReturnTypes2,
     intraExpressionInferences, contextualTyping… This is the REFUSED
     inference-legs territory (STATUS §5); re-open only with a new
     mechanism-level argument, not a population count.
  2. compiler/temporal (433) — namespace-qualified types
     (`Temporal.ZonedDateTime`), the modobj workstream's row.
  3. longObjectInstantiationChain3 (166) + the `Omit<…> ← merge<…>` pair
     row (142) — conditional types / mapped instantiation, unported.
  4. underscoreTest1 (149) — the DOUBLE-REFUSED `_1` within-print half;
     checker-notes-callres §20.1 forbids a third join placement without
     the print-context study.
  5. inferTypePredicates (147) — predicate INFERENCE from bodies
     (getTypePredicateFromBody), distinct from build 40's call-condition
     narrowing which landed.
  6. controlFlowOptionalChain residue (175) — needs closure-callee
     narrowing (§13's residue) + effects signatures at CALL flow nodes
     (statement-position asserts) + chain-link marker mechanics
     (deleteChain).
  7. Small named residues, each priced in its section: §22's 3 Record
     lines, §16's typeof-facts granularity for function/object (§23's
     faithful arm landed at zero — the constituents arrive lazily
     unresolved), the §12.7-era assignment-LHS leftovers in JS files.
  8. NEW prerequisite named by the freshest refusal (§6.1, +23/388):
     SPREAD-AWARE CALL RESOLUTION. The SpreadElement expression arm is
     one line, but landing it converts every spread-bearing call from gap
     to confident wrong until selection understands spread arity. Build
     the resolution half first; the expression arm then lands free.
  9. Builds 42–45 opened the TERMINAL board (rank_board, fresh run in
     the §24 bar): template expressions LANDED (+1,342), void/delete
     LANDED (+438), non-null LANDED (+167), array-spread containment
     LANDED (+85). Remaining TERMINAL heads: CallExpression 2,790,
     ArrowFunction 1,395 (both contextual-typing-bound), NewExpression
     733 (generic instantiation), ElementAccess 640, AsExpression 228
     (type-node resolution bound).

DO NOT RE-DERIVE: STATUS §5's refusals all stand. The fixpoint patch doc
(fixpoint-patch-§12.md) is now HISTORY — the mechanism landed in §12.6/§12.8;
read it only for the investigation record.
