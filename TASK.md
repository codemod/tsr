THIS FILE IS THE `.types` GRADIENT WORKSTREAM'S HANDOFF.
The `diagnostics` workstream's is **TASK-diagnostics.md** — the two were
overwriting each other in this one file, and the ninth session's diagnostics
handoff was lost that way before it was read.

TOOLING (build 96): the scoring pair is now ONE command —
`cargo run --release -p tsr-conformance --example scorepair` runs the
corpus, diffs against `target/verdict_baseline.tsv`, and prints the
transition matrix with per-case attribution; `-- --accept` advances the
baseline after a landing (full runs only, enforced). `TSR_FILTER=case`
on scorepair or verdictdump gives the sub-second inner loop (41s -> 0.36s
measured). Iterate filtered; LAND only on a full-run matrix.

FIRST: git pull. Read STATUS.md §1 and §7's top rows (the ninth session's
continuation appended SIXTEEN rows, builds 25–40), then
docs/architecture/checker-notes-narrow.md §12.6–§22 — that run is the
session's spine: the fixpoint landed by exonerating it, the too-large bail
(+10,000 in one build), and fourteen more single-rule decodes. The last
~600 lines of docs/conventions.md still pay.

STATE AT HANDOFF (verify with a fresh coverage run):
  checker_types 3,774/9,538 (39.57%) · 400,173/478,954 = 83.55% · gap
  46,181 · wrong 22,561. The continuation (builds 25–103): +43,520 right
  lines, +731 cases, +9.09 points, SEVENTY-NINE bar-scored builds and
  ELEVEN measured refusals, every fired leg honoured in writing.

  BUILDS 89–103 (the newest window): §37 arguments->IArguments (+637),
  §38/§38.1 for-of/for-in bindings (+790), §51–§51.5 the DISCRIMINANT
  FAMILY whole (property switch/equality/truthiness + chain containment's
  full table and composition, ~+500), §52 equality's comparable-filter
  half (+40; the operand MEMO is load-bearing — condition chains are
  exponential without it and compiler/con* hung the corpus; full run 21s
  since), §53 ORIGIN-CARRYING UNIONS (+237 — the §39 reshape landed with
  four falsifier-driven refinements; filters PROJECT origins via
  rebuild_union_subset), §54 &&'s non-strict falsy source (+104/0, one
  line), §55/§55.1 ENUM MEMBER VALUES (+274 net — the bd tsr-8pz folder's
  first slice: value-keyed literal interning, ambient-no-auto, and the
  single-member SPELLING SPLIT as divergent fresh/regular twins with an
  access-road swap; right crossed 400,000 here).

  NEXT HEADS, each with its section: the enum residue (string-length
  folding, cross-enum refs, §53's entry-order class), the || early-return
  divergence (7 lines, blocked on non-strict per-constituent facts), the
  §53 object/generic-alias entry shapes (temporal's 82), the §12.7
  element-access write seam (1 line, recorded), the auto-var container
  walk (parsingDeep 333), and the CONTEXTUAL-INFERENCE ARC — still the
  largest single owner (~8-12k lines; see the 100%-decomposition answer
  in the session log). Chain in
  STATUS §1. Builds 73–88 (the latest window): §48 pattern renders, §49
  union property projection, §50/§50.1/§50.2/§50.3 dependent
  destructuring WHOLE (equality, switch, walk-composition, tuple
  parameters — checker.go:13751/13806), §30 new-through-untyped-gate
  (+279), §31-callres unresolvable require() aliases (+390 — findability
  alone decides, TS1147 is not a resolution bar), §32 non-union index-
  signature keys, §33 const type-parameter prints (three contained legs),
  §6.3 tuple-spread minting (context-split), §34 deferred indexed-access
  prints, §35 deferred keyof (+324; §35.1 concrete keyof REFUSED +3/32 —
  consumer-unlock class), §36 template-literal types (+453 right, −219
  wrong — THE ANNOTATION-REUSE vs ALIAS-EVALUATION SPLIT, the window's
  biggest discovery: upstream reuses written annotation nodes but
  evaluates alias-declared positions), §36.1 v2 written-name reuse via
  the written_text seam (v1's type-level mint refused +18/52). New
  refusals priced: §6.2 union spread contribution (+2/11, wrong premise —
  upstream mints tuples), §14.1 kept at zero (the parsingDeep 324-line
  owner is the auto-var container walk, a largeControlFlowGraph-class
  mountain, queued not refused).
  The late run's spine: the §41–§46 members-carrying-reference design
  (qualified names, generic qualifieds, alias bodies — three standing rows
  converted at once, including 264 of the double-refused underscoreTest1),
  the §17 un-refusal (+784 — a refusal's diagnosis is itself a claim the
  next probe must test), skip-with-agreement over constructor overloads
  (+891, typed arrays whole), Record<string, V> (+274), plain-function
  `this` (+672), `this`-results answer their receiver (+66), variadic
  tuple prints with concrete splices (+525). The §20.1 double-refusal's
  print-context diagnosis CONFIRMED from the outside: underscoreTest1's
  residue wants T_1 renames in exactly the same-member-list context the
  refusal said it could not see.

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
