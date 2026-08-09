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

  FRESH GAPROOT ATTRIBUTION (build 107, gap lines over 47,120):
  own-rule 53.7% / unmatched 17.9% / type-node 17.3% / no-value-decl 6.0%.
  Top roots by line count: CallExpression errors ~2,900 (contextual +
  generic overload resolution — the promise family lives here), property-
  access receiver misses ~3,600 (lib member resolution through generics),
  ArrowFunction ~1,300 (contextual parameters), NewExpression ~900
  (generic instantiation; Intl.Locale 86 is lib), ElementAccess ~1,000,
  and TWO no-value-decl rows (~1,700 combined, want-any heavy —
  "declaration name, symbol has no type", probed SAME session: they are
  ES-import aliases, and the §31-class admission REFUSED at 2.7:1 —
  resolver parity, not rule shape; see checker-notes-callres §31.1.
  They stay with bd tsr-9or.1 until the host learns symlink/path-mapping
  resolution — DO NOT re-derive).

  BUILDS 108–110 addendum: §58.1 closed the anonymous-object union-print
  seam (TSR_JOIN_DEBUG one-line diagnosis) and lifted §56's let gate
  (+33); §59 landed switch(true) clause narrowing — the §5x condition
  family composes there free (+67, the §16 CaseKeyword trap's THIRD
  firing is noted at the type); §56.3 landed argument-position retention
  (+91, TEMPORAL'S FIRST 79 LINES). Temporal's remaining 799 decompose
  as: qualified `Temporal.X` spellings inside signature prints (the §41
  site-sensitivity mountain — the case's true owner) + method results
  through those signatures. §56.4 (new-argument via interface road) and
  §31.1 (ES-import findability) measured zero/refused — recorded, do not
  re-derive.

  AUTO-VAR WALK, SHARPENED (build 111 analysis): the §14.1 mountain's
  trip is NOT read-side hop counting (measured zero — our iterative walk
  breaks at the nearest assignment) but the AUTO-VAR TYPE CASCADE:
  upstream types `var v` (no annotation/initializer, JS) by checking
  EVERY assignment's RHS, each RHS typing its own references
  transitively — thousands of nested walks, the 2000-cap trips, global
  flowAnalysisDisabled, and every later read prints ` : error`. The port
  needs the assigned-union computation (with hop counting inside it) to
  reproduce the trip; the lazy any-typing never starts the cascade.
  parsingDeep's 333 + the auto-var families hang on this one mechanism.

  BUILDS 111–116 addendum: §60 qualified heritage (6:1, plus the
  inside-namespace refusal re-priced at 1.4:1 and the §33 const gate
  proved load-bearing at 850 R→G); §61 auto-var counter proved
  IMPOSSIBLE (43 ticks vs the 2000 cap — the recursion IS the count);
  §62 unique symbols per-declaration (30:1); §63/§63.1/§63.2 the tuple-
  context family complete (+92/0); §64 non-strict null-return widening
  (+388/0 — the census's biggest single find); §65 undefined-initializer
  un-refused with three keys (noImplicitAny plumbed into the producer);
  §65.1's null twin refused at 2.4:1 (fourth key undecoded — START THERE
  or at the argument-road generalization). 84% and 40% both crossed.

  BUILDS 117–119 addendum: the CONTEXTUAL DISPATCH RE-MEASUREMENT SWEEP
  — three once-rejected arms (return statements §68, parenthesization
  §68.1, array elements §68.2) all landed at zero adverse; every
  concentration-based rejection in checker-notes-ctx.md predates the
  §57 alignment jump and is suspect. §65.1/§66/§67 refused en route
  with the generatedContextualTyping decline point TRACED: the literal
  keeps its union (right); the residue is function-expression prints
  and deeper contextual hops (concise arrow bodies next).

  OMIT/MERGE DECOMPOSED (build 120): all 161 lines are
  longObjectInstantiationChain3 — `merge<A,B>` alias chains whose WANTS
  are the recursively-EXPANDED `Omit<...>` bodies. One mechanism:
  generic alias BODY instantiation (substitute the args through the
  body and print the result — the §46 members-carrying reference's
  EVALUATION half, plus §36's alias-declared-positions-evaluate rule
  applied to generic aliases). Single-case mountain; needs
  instantiate_type over alias bodies with the §42-v2 registration.

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


---

ADDENDUM AFTER BUILD 126 (session continues, 84.13%):

Builds 122-126 this window, +305 right lines, zero net adverse:
  - §70 overload-agreement contextual argument (+68) — id-walk mention
    test; a TEXT test collided rebound `<T>` names and killed the wins.
  - producer: binding-element property name prints any UNCONDITIONALLY
    (+85) — the IsTypeAny precondition REVERSED on measurement; its
    pinned test flipped. The position's answer is decided by upstream's
    getTypeOfNode trace, not by what this port computes.
  - §71/§71.1 pattern printing (+113) — renamed elements verbatim,
    initializers dropped, `{}` spelling.
  - §72 function-type aliases print their name (+39) — the three-arm
    getAliasForTypeNode rule was missing ONLY on signature-bearing
    nodes.

NEW PRICED REFUSAL: §73 JS-wide unresolved-prints-error, 199:1,743 —
upstream prints BOTH any and error for unresolved names in ONE file;
file kind is not the key. (checker-notes-narrow.md §73.)

NEW DEFERRED HEAD: import("...").Name spelling
(privacyFunctionCannotNameParameterTypeDeclFile, 128 lines, single
case) — needs PER-FILE printing context; type texts are minted
globally at creation. Architectural; do not attempt as a patch.

BOARD UNCHANGED otherwise: remaining concentrated wrongs are
subsystem-scale (temporal 400 = §53 residue + namespace types;
longObjectInstantiationChain3 166 = generic-alias body instantiation;
inferTypePredicates 158; jsdocTemplateTag6 127; dependent
destructuring 127; instanceof-hasInstance 96 = narrowing legs).


ADDENDUM AFTER BUILD 135 (84.33%):

Builds 127-135: +623 more right lines (403,242 at 131 → 403,865), all
zero-adverse or residue-owned:
  - §74 generic construct inference (+30); §74.1 order refused (-2).
  - §75 generic contextual signatures uninstantiated (+49).
  - §76 family: destructuring tuple contexts, index-correlated slot
    walk (+196/+3/+5) — three falsifiers each became a measured gate.
  - §71.2 nested pattern rendering (+89).
  - §77 SINGLE-QUOTE-GATED written-annotation reuse (+370, ZERO
    adverse — largest zero-cost build since §64) + §77.1 mint spelling
    (+53) + §77.3 member-name quotes (+54). The old blanket-reuse
    refusal's diagnosis was half-right and the gate is the fix.
  - §77.2 union WRITTEN ORDER refused TWICE (35:249, then 0:20 with
    the mechanism located: the order-wanting unions are built by
    OPTIONALITY and narrowing rebuilds, not the annotation mint).

PROCESS: the §88 byte-identity trap fired TWICE this window through
my own git pull --rebase (the parallel session moved checker_types
+57 and +60 under me); both caught by stash/re-accept/re-measure.
One shipped-red landing (build 127) corrected in follow-up; gates are
now READ before the landing commit, separate commands.

REMAINING quote-family residue (~228): union order (optionality
builder, §52.1 site-sensitivity), aliased-condition discriminant
narrowing (controlFlowAliasing 77), readonly const-context inference
(inferFromNestedSameShapeTuple).

NEXT HEADS on the shape census: true|boolean 97 (best-common-type
freshness retention — subsystem), T|any residue, temporal 400 (§53
residue + namespace types), generic-alias body instantiation
(longObjectInstantiationChain3 166 + Omit/merge).


ADDENDUM AFTER BUILD 138 (84.40%):

Builds 136-138: §78 exactOptionalPropertyTypes (+14, option plumbed
end-to-end, missingType minted, write-position removal; §78.1
element-write leg a kept measured zero), §79/§79.1 optional-element
tuples + tuple alias names (+184 net at 4.7:1, three measured gates),
§80 labeled tuple members (+71 net at 3.2:1; residues: label-losing
splices, rest-parameter expansion positions).

REFUSED: §81 blunt qualified names at 114:6,769 — per-site printing
context now owns THREE heads (import-spelling 128, qualified names,
temporal 400). Do not retry with narrower gates; the inside-view half
is structural.

PROCESS: two more §88-trap firings (parallel session moved the
gradient AND the population 470,619 → 470,657 under rebases); one
inherited red-gate pile (bind_source_files signature change left 11
needless_borrow sites + an anchor cite with a doubled prefix) cleared
in the §80 landing.

FRONTIER: shape-level census is mined out — every remaining head ≥50
lines is subsystem-scale: contextual typing through generic overload
resolution (temporal's literal-retention rows), aliased-condition
discriminant narrowing (controlFlowAliasing 77), type predicate
inference (158), JSDoc generics (127), dependent destructured
variables (131), generic-alias body instantiation (166+142),
best-common-type freshness (true|boolean 97), per-site printing
(3 heads). Next session should pick ONE subsystem and build its
prerequisite seam rather than continue shape-mining.


ADDENDUM AFTER BUILD 141 (84.44%):

The narrowing-subsystem block, builds 139-141:
  - §82/§82.1 aliased conditions (+43/0): const-initializer inlining
    (depth 5), the logical arms narrow_type lacked (an inlined
    initializer has no flow branch nodes), and the constant-reference
    gate (const vars / never-assigned params via mark_node_assignments;
    ungated measured 12 adverse, gated ZERO).
  - §83 instanceof, TRUE branch only (+126 at 18:1): identity +
    extends-chain slice. THE FALSE BRANCH IS EVIDENCE-SPLIT: 
    typeGuardOfFormInstanceOf's else keeps the whole union while
    instanceofWithStructurallyIdenticalTypes narrows by derived-from —
    global var vs parameter is the visible difference; needs an
    upstream trace before the false arm lands. The narrowing stand-in
    fixture expired a FIFTH time (now: computed-name in).
  - §84 sibling-truthiness discriminant (+6/0) — and a REDUNDANCY
    caught by measurement: a hand-rolled dependent-destructuring
    equality arm measured zero transitions (the §50 pseudo-reference
    road already owned every position) and was REMOVED, not landed.

dependentDestructuredVariables residue (148 wrong): callback
contextual tuple-rest parameters (f50/f51), generic alias unions
(AB<T>), Iterator.next destructuring, f22's
parent-flow-at-declaration, f23's exhaustive-never. Each is its own
machine; none is a narrowing arm.


ADDENDUM AFTER BUILD 143 (84.45%):

Builds 142-143: §85 T&{}-family adjusted facts for type variables
(+42 at 22:1 over three measured iterations: truthy dropped, the
refinement lattice, and the JOIN reduction — a mint beside its own
base is subsumed in get_union_type) and §85.1 truthy NonNullable<T>
spelling (+12 at 5.3:1).

PROCESS — THE TRAP'S WORST FIRING YET: §85.1 was REFUSED for one
commit on an "unexplained 8 R→G in exportNestedNamespaces2" that was
actually the parallel session's arrivals measured against a
pre-rebase baseline. The refusal was reversed and the record
corrected loudly. NEW RULE (in checker-notes-narrow §85.1): before
pricing ANY adverse, re-run scorepair on the clean tree — a
transition that survives the revert is not yours. This is the
stash-and-remeasure rule extended to REFUSAL evidence.

§85 residue: NonNullable-in-annotation positions (the conditional-
type utility, different head), row-113 condition-position leak, one
JSDoc-generic disturbance.


ADDENDUM AFTER BUILD 144 (84.46%):

§86 rest-tuple contextual parameters landed (+33/0). Its residue
diagnosed: `[number, boolean, ...string[]]` rest-tuples are §40
PRINT-ONLY (no element list), so positional expansion declines —
extending needs variadic element-list modeling (elements + a trailing
rest slot), which also unlocks restTuplesFromContextualTypes' 100+
and the `(args_0: number, ...)` expanded-signature prints.

GENERIC-ALIAS INSTANTIATION diagnosed by probe: annotation positions
already print `merge<{a},{b}>` correctly — the Omit/merge WANTS
expand because instantiation THROUGH CALL RETURNS drops the alias
(upstream's instantiateType maps the body and prints it expanded;
`doMerge(x, y)`'s inferred return). The subsystem's entry is
instantiate_type over alias-reference returns, not the annotation
mint. longObjectInstantiationChain3 166 + Omit/merge 142 hang off it.

Session window closes at build 144: 23 landed builds (122-144),
+1,807 right, 84.07% → 84.46%, ten priced refusals, one false
refusal corrected loudly. Everything pushed.


ADDENDUM AFTER BUILD 148 (84.48%):

The tuple/rest seam block, builds 144-148: §86 rest-tuple contextual
parameters (+33/0), §87 variadic tails consumable lazily (+3/0 after
the eager version appeared to cost 16 lines that turned out to be
unicodeEscapesInJsxtags' NONDETERMINISTIC alignment — instrument
caveat on record: any single-case ±16 there is noise), §88/§88.1 rest
expansion in signature prints (+50 across two shards — written reuse
wins at annotation signatures, expansion at fresh value signatures;
NO site-sensitivity needed, two types two prints), §86.1 the
function's own trailing rest (+26/0 — tail array at/past the prefix
boundary; tuple-SLICE minting is the next machine in this seam).

restTuplesFromContextualTypes fell 117 → ~85; genericRestParameters
1/2 largely converted. Remaining in-seam: tuple slices, TupleUnionFunc
alias prints, the union-of-variadic contextual forms.
