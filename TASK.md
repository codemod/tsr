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


OPEN TRACE (build 149+): §72's alias-name mint fires and carries the
name ("H" verified by instrumentation), yet `>H :` and `>h : H`-want
assertions print the STRUCTURAL signature — some road between
get_declared_type_of_symbol and the assertion consumes a different
TypeId (suspects: the variables road's written/signature rendering in
symbols.rs ~1390, get_regular_type_of_literal_type, or an
earlier-resolution cache). Three TupleUnionFunc rows + p18's probe
reproduce it. Worth one focused trace next session; §72's +39 came
from OTHER positions.


ADDENDUM AFTER BUILD 155 (§92, 84.63%, commit 88d0e63):

The FIFTEENTH session's block — the generic-alias instantiation
subsystem opened, four seams landed (+796 right, six builds):
  - §89 keep-text set closed the open trace (+178); §89.1 degenerate
    leading-operator unions kept as real nodes across parser/checker/
    printer (+8/0 — printer_round_trip fell to 99.97% mid-build and
    was restored; all four 100% suites verified after).
  - §90/§90.1 instantiated alias references carry their body's
    members (+280/0). The r_1 rename is PRINT-ONLY (semantic rename
    measured −182: inference maps params by declaration TypeIds) and
    EMPIRICALLY gated (alias body + returns-container +
    FunctionTypeNode). The faithful rule is typeParameterToName's
    byText/shadow context (nodebuilderimpl.go:1404) — §20.1's owed
    print-context study; underscoreTest1's 149 still hang there.
  - §91 conditional alias bodies EVALUATE at arm-3 rebuilds (+217/0,
    chain3 whole): env-stack bindings, env-gated keyof/key-set-
    intersection arms, extends-never only, computability is the gate.
    chain1 pins the other half: PLAIN aliases keep their name.
  - §92 the shape property road (+113 at 9.4:1): intersection
    constituents (gated to evaluator-produced types — written
    intersections measured 134 G→W), Omit<T,K> via
    global_type_symbol_with_arity("Omit", 2) (the arity-1 default
    answers None — trap), evaluate_alias_body for non-literal bodies
    (TypeLiteral spines refused — they must stay with §90's
    instantiating road or members print WRITTEN types).

NEXT HEADS in this subsystem: §92's 12 priced residues
(exactOptional through the Omit arm 6, discriminated alias-name
variants 4, dependentDestructured 2); general extends forms beyond
NeverKeyword (templateLiteralTypes3's §36 decline is the annotation
entry); keys through index signatures/mapped types; ramdaTools +
jsxGenericComponent singles. THE BIGGER BOARD UNCHANGED: contextual
inference arc (CallExpression 5,200 + ArrowFunction 3,705 gap
roots), per-site printing (3 heads incl. temporal), predicate
inference 158, JSDoc generics 127.

PROCESS NOTE: an aborted python batch edit (assert mid-script) left
its EARLIER replacements unwritten too — the script writes at the
end, so nothing landed, but the follow-up fix re-applied only the
edit that raised. A TypeId-printing probe found the missing
registration two hours later. Re-verify every edit in an aborted
batch, not the one that errored.

NEXT SEAM DIAGNOSED (post-§92.2 probe): fatarrowfunctionsOptionalArgs'
111 gaps are arrows IN ARGUMENT POSITIONS under a callee with no
usable contextual signature (`foo(...arg: any[])`) — statement-position
arrows already print (451 RIGHT). The fix is the §68 family's next
arm: when contextual dispatch declines at a call argument, fall back
to the arrow's STANDALONE type instead of erroring the argument.
Owner: checker-notes-ctx.md's dispatch; the §56.3 argument-position
retention build is the precedent. ArrowFunction own-root is 3,705
gap lines; this is its cheapest measurable slice.

§95 RESIDUE CENSUS (temporal, post-§95, checker-1 worktree): the 399
gaps are METHOD-RESULT resolution, not printing — `.until()`-family
calls wanting `Temporal.Duration` (49), method signatures embedding
conditional lib utilities (`Temporal.PluralizeUnit<"day">` 36 — the
template-literal conditional machine again), and array-of-instance
results (`string[]`/`Temporal.ZonedDateTime` reads through lib
generics). Owner split: call resolution through §41-family qualified
reference members + the template-literal conditional evaluator
(same blocker as templateLiteralTypes3's §36 decline). The remaining
288 temporal WRONGS after §95's 112: baked outer texts that are not
name<args>-shaped — signature-embedded slots §10.13 routes.

§96 TRACE COMPLETE (checker-1): temporal's method-read gaps are
owned by `DateUnit | TimeUnit` — a WRITTEN UNION OF ALIAS-NAMED
UNIONS as a type argument (lib.esnext.temporal.d.ts:247/:314). The
union worker answers errorType for named constituents (the
§42.1-family origin-denormalization gap), which errors the argument,
the reference, the signature bake, and every downstream read — the
working `until` overloads (:164/:197/:399) take single names, which
is the controlled experiment. §96 = the origin-carrying union for
written unions of named unions, built on §53's rebuild_union_subset
machinery: flatten the constituents, carry origin text as the
constituent NAMES, qualify each per-site (§95's road). Candidate
conversions: the 49 Temporal.Duration results + 60 signature prints
+ downstream reads (~150+); PluralizeUnit was NEVER conditional
(it is `T | {...}[T]` — indexed access; the §36 speculation in the
superseded probe note was wrong and is corrected here).


ADDENDUM AFTER THE PARALLEL SESSION (checker-2, builds §93-§103,
84.87% at a370cfa):

THE TWO-SESSION PROTOCOL IS LIVE. checker-1 and checker-2 share
this checkout's main (checker-1 works from a worktree for code,
both push to main); lanes: checker-1 = printing/alias-instantiation
(§95 +146, §97, §99 +55, §100 +24 predicates; §102 REFUSED at
88:763 with the written-reuse carriage named as prerequisite),
checker-2 = contextual dispatch arc. Rules that earned their place
TODAY: stash/accept/pop counterfactual before scoring ANYTHING (the
§93 stale-baseline trap fired its worst — a LANDED score inflated
19×, corrected in-commit at 45436ae; the commit MESSAGE there is
wrong, the docs inside it right); announced measurement windows on
target/verdict_baseline.tsv; section numbers claimed by committing
the bar to main FIRST; tail-append doc conflicts resolved
mine-then-theirs in section order (four today, all mechanical).

checker-2's landings this session, each with bar-before-code and
its score in checker-notes-narrow.md:
  - §93 correction: true score +16/4 at 4:1 (not +300).
  - §94 statement position shows contextual absence (+93 G→R /
    +14 W→R / 1 G→W at 107:1) — has_no_contextual_type is now the
    nil-ladder of getContextualType's dispatch (checker.go:29343).
  - §96 initializer-branch optionality (+6/0; bar missed at 6 vs
    12-25 — exact-insertion sizing buckets by SPELLING not
    mechanism, the lesson recorded).
  - §98 retention's roots widen (+93 W→R / ~152 G→R / 1 R→W at
    245:1): assignment roots (checker.go:29843), distributing
    union/intersection member lookup (checker.go:30555), root
    discrimination as upstream's TERNARY algorithm
    (relater.go:1212 — constituents lacking the member SURVIVE).
    Two boolean over-corrections built, measured R→W 23/25, and
    REVERTED — bare `boolean` retains; read §98's fired legs
    before touching type_wants_literal.
  - §101 template fold consults the constant evaluator (+152/0):
    the symbol-free evaluate slice (checker.go:7991), spelling via
    tsr_core::jsnum::format_number. Const-reference spans still
    decline — the evaluate-entity slice is the follow-up.

DEFERRED WITH DECOMPOSITION: §103 const-T inference (41+ lines) =
FOUR machines shared with as-const (const-context arrays, readonly
tuples, readonly members+printing, inference plumbing); §33's
decline in calls.rs:481 stays until built as a unit; the as-const
OBJECT gap (assertions.rs) is the natural first slice.

PARKED, JOINT-ONLY: the ~215-line scanner escape family
(octalLiteral 94, templateLiteralEscape 91, numericSeparators 30) —
invalid-escape cooked-text divergences. Touches the scanner under
FOUR 100% suites; requires a joint bar with per-suite regression
legs, and possibly the user's nod.

NEXT HEADS (checker-2 lane): the as-const object slice (unlocks
§103); the evaluate-entity slice (const refs into §101's fold);
§98's one-line attributes2 residue; the §68/ctx dispatch arms into
CallExpression/ArrowFunction own-roots (still the largest owner).

TWO ATTRIBUTION LESSONS from the classAbstractManyKeywords:0:5
dispute (both sessions were wrong about the owner; the line predates
both builds — full chain in checker-1's 4b9fafc):
  - A REVERT TEST needs a verified recompile: cargo clean -p on the
    crate, count the 'Compiling' lines, and grep the reverted arm
    absent from the tree — a conflicted or stale revert measures the
    unreverted binary and reads exactly like a confirmed bisect.
  - A PULL-DIFF R→W needs the PRE-WINDOW CHECKOUT before it names an
    owner: a baseline that silently absorbed a standing wrong makes
    an old defect surface as "new" in whichever pull happens to
    re-expose it. `git worktree add --detach <dir> <pre-window-sha>`
    and one filtered run answers it in under a minute.

FRESH BOARD (rank_board at right 408,314 / 85.25%, residual 70,640):
top-250 rows hold 96.64% of residual lines. NEAR-MISS gold: of
3,931 cases within 10 lines of finishing, 1,184 need ONE row.
Case-gate ranking: CallExpression-answered-error 770 near-miss
cases (the contextual/call arc, checker-2's lane, still #1),
ALIAS/no-value-decl 428+177 (ES-import resolver parity — REFUSED at
2.7:1 in the §31.1 era, but that price predates §106/§108's
findability machinery; RE-PRICE before believing it), ArrowFunction
315, ObjectLiteral 279, member/property-access no-such-property
269+264 (post-§110 lib/member residue). The §112 refusal's two
const-T prerequisites and the §98 DISCRIM mask-pairing trace stand
as recorded entries.

CALLEEGAP RE-READ (at 85.25%): 5,830 admitted lines / 952 cases
across the funnel's two unexplored gates, and the cross-tab settles
their question — TWO mechanisms: `callee type is not an object
type` rides the NEW side (613 vs 4), `identifier: symbol types as
a non-object` rides the CALL side (494 vs 0). The small-bucket tail
is single-case mountains (controlFlowSelfReferentialLoop 68 of the
generic-candidate 90); the near-miss aim favors the spread rows.
NEXT WINDOW'S SIZING INPUT: pair this against the 770 one-row
near-miss CallExpression cases — the intersection names the bar.

CALL-SIDE GATE DECOMPOSED (the sizing's conclusion): the 494-line
`identifier: symbol types as a non-object` gate's largest bucket is
87 lines of "callee is any — UNANNOTATED PARAMETER, contextual
typing refused", SPREAD (parenthesizedContexualTyping2 10,
dependentDestructuredVariables 8, callWithMissingVoid 5,
taggedTemplateContextualTyping1 5, tail wide) — the funnel's own
evidence that get_contextually_typed_parameter_type's coverage is
the contextual arc's entry: callbacks whose parameters the §56
roads cannot contextually type, whose CALLS then error. The want
census there (void/unknown returns) says the payoff includes the
callback-invocation family. Second finding: the recursive-types
single-case noise (declarationsWithRecursiveInternalTypes…, 5
enormous want-texts) pads the smaller buckets — exclude it from any
conversion prediction. NEXT WINDOW'S BAR: extend the contextual
parameter road one measurable context at a time (the §68-family
method), starting from parenthesizedContexualTyping2's shapes.

PARKED, JOINT-OR-USER-NOD (second entry): the LOADER/VFS UNLOCK —
symlink realpath + `paths` mapping in the host resolver. Priced by
checker-1's §113 re-price: ~240 gradient lines + the 428-case
near-miss ALIAS row; the era refusal (35:13) reproduced EXACTLY
after two years, proving the blocker sits beneath the checker.
module_resolution's 100% suite must hold through any attempt.

BOARD RE-CENSUS AT 85.29% (post-§115, the day's close): the
near-miss top is UNCHANGED by the day's twenty-three landings —
the cheap heads are exhausted. The four remaining owners, each
gated: (1) CallExpression 772 = deep generic-overload resolution
(the §33-family machinery; §114/§115 took its dispatch fringe);
(2) ALIAS 428+177 = the LOADER UNLOCK, parked joint-or-user-nod
with checker-1's exact price; (3) member/property no-such-property
270+265 = the lib-member residue, checker-1's lane; (4) the
FUNCTION-row 203 SAMPLED and FOLDED into (1)'s known cases
(intraExpressionInferences, generatedContextualTyping,
complexRecursiveCollections) — no new cheap head exists there.
§116 measured zero and is recorded; §114's ledger correction and
the gain-column artifact rule are in conventions.md. The next
session opens on subsystem-scale work or the parked unlocks —
nothing smaller remains at the top.

THE BOARD'S SPREAD ROWS ARE CLOSED (the marathon session's final
census act): ElementAccess 146 — the last unclaimed spread row —
sampled and FOLDED (elementAccessChain's wrongs are the §77.2
optionality union-order family; its gaps are the §13 optional-chain
flow machinery; the error concentrations are the known subsystem
cases). EVERY near-miss row now has a named owner and a gate:
the callres2 summit (whole-or-nothing, its study complete to the
struct-field level), the flow machines (f22, optional chains,
auto-var), the printing residues (§77.2 order, per-file spelling),
the relater-gated families, and the two user-nod unlocks. There is
no cheap head left — the port's remaining distance is subsystem
builds, each with its document, each opened from its record and
not from a label. The two-lane loop's continuing work: checker-1's
census heads and the summit build in fresh windows.

## checker-2 window close (2026-08-09, §133–§142)

Landed: §133 fixing-fill (ladder complete, +111 net), §134 returnMapper
guard (+10), §135 intra-expression slice 1 (+103, the benign-return
unlock), §137 grounded gate (+490), §138 methods+tuples (+15), §140
candidate-order buckets (+15, the execution-order law), §141
reference-member instantiation (+112 from a 6-row bar). Refused with
full records: §139 (thisType subsystem), §142 (the gate's third state
— the next window's opening rung, complete spec in
checker-notes-callres2.md). Board: right 412,603/478,954 = 86.15%
at 962a5adb; day across both lanes 84.63% → 86.15%+.

Next window opens on: the gate's THIRD STATE (ungrounded-adopt for
literal members under an active inference context + pass-3 re-serve),
then InferenceInfo step 2 (priority bits; E1/0 common-supertype pair
— rows banked in §140/§141 records).


---

CHECKER-1 HANDOFF, THE STALE-REFUSALS SESSION (2026-08-10, board at
86.15%, right 412,603/478,954 at 962a5adb-merged):

THE DAY: 84.63% → 86.15%, +1.52 points across both lanes — the
largest on record, twice re-broken. checker-1's ledger: §118–§142
in checker-notes-narrow.md plus §124.1, TWENTY-EIGHT numbered
outcomes. Landed: the @symlink/@link harness fix (§118), five
un-refused stale entries (§119 ES-import any +235, §120
intersections +172, §123/§124 established misses +464, §137 union
order +308 at 154:1), the boundary arms (§121/§122/§130/§131),
the CALL-flow seam (§126/§127/§128), generators (§135), dynamic
import() (§140 +122), the DEFAULT_LIBRARY-gated default-fill
(§136 +478 — the printseam study validated end-to-end), §124.1's
three-detector gate. REFUSED with maps: §125 (union-miss,
narrowing-owned), §138 (union order is PER-SITE — closed three
ways), §141→§142 (the literal-self this mint, PARKED at nine
probes with the mechanism proven and the fault cornered).

READ FIRST: checker-notes-printseam.md (the five-walls-are-one
study + its §5-§8 arc), then §142's nine-probe ladder tail.

THE JOINT HEAD both lanes' records agree on: the LITERAL
ERROR-PROPAGATION SEAM in check_object_literal (one member's error
gaps the whole literal; upstream errors only the member) — it
unlocks checker-2's pass-3 re-serve AND §142's combined form.
Probe 10 is written at the §142 tail. After it: per-site rendering
arm 2 (modulespecifiers), the thisType model (three consumers
recorded), resolver parity (tsr-9or.1).

TRAPS THIS SESSION PAID FOR: rtk masks cargo exit codes AND
Compiling lines (rtk proxy + recompile-check before any
byte-identical claim); size bars by MECHANISM SAMPLE not census
bucket (five under-counts); tag the CONTAINER in multi-object
probes; the stash→accept-clean→restore cycle before ANY
measurement when the other lane is mid-landing.

### Addendum (2026-08-10): the joint head closed three ways

§142 (−21), §143 (0/+73), §144 (−16/+23) — predicate+fill, seam
condition, gate third-state, each measured whole and reverted. Final
verdict at 6675ba7d: the fixing-mapper pipeline (per-consumption
fixing, InferenceInfo priorities, ordered member sites, pass-3
re-serve) is ONE build. The three refusal ledgers in
checker-notes-callres2.md are its requirements document. Board:
right 413,128/478,954 = 86.26% (checker-1's §145 included), clean.

### Next-window census pointer (2026-08-10, checker-2)

instanceofOperatorWithRHSHasSymbolHasInstance (94 wrong, family 317 R):
the §111 predicate road exists and fires; the wrong cluster answers
`any` where wants are `false | Point`-class — the wall is UPSTREAM of
narrowing: expressions involving `declare class RhsN { static
[Symbol.hasInstance](...): value is T }` answer any/error (likely the
class value type under a static COMPUTED well-known member, or the &&
result road). One trace at the `x instanceof Rhs10 && x` shape decides.
Board at window end: right 413,136/478,954 = 86.26%, all synced.

### checker-1 window handoff (2026-08-10 late): §145–§158, and the board is wall-bounded

**Board at handoff: right 414,058/478,954 = 86.46%** (from 84.63% at the
two-day open), all suites that were 100% still 100%, tree clean, baseline
accepted at HEAD.

**Landed this window** (checker-notes-narrow.md + checker-notes-ctx.md):
§145 nameless-leaf qualified ImportEquals (+71/6); §146 TS2304 member
tolerance (+4/0, head refused to fixing-mapper with the SS146p1
histogram); **§147 scanEscapeSequence cooking parity (+257/0 — octal
cook-vs-raw by report mode, invalid \x/\u keep raw, tagged substitution
templates never fold, §24's length-decline deleted)** + tagged-no-sub
rider (+41/0); §150 evaluator bitwise/ToInt32 (+72/0) + legacy-octal
normalise_number (+10/0); **§152–§155 the contextual dispatch arc
(+300 net: assignment arm 13.9:1, PropertyDeclaration 58:1, assertion
family +70/0, new-args +5 under-bar-stated)**; §157 ImportEquals-alias
written-name references (+59/7).

**Refused with mechanisms** (do not re-attempt without new evidence):
§148 JS uninitialized-var error (needs assignment analysis: cycles→error,
parameter-fed→any); §156+retry class-alias constructor mint (the blocker
is the EXPRESSION-road short-name print — `x.c` sites want `typeof c`,
best_name at expression positions); §158 ES-import alias references
(minted texts TRAVEL across units through inferred types — the per-site
re-render wall; no declaration-kind gate cuts it).

**Cross-lane custody event**: 51d583a5 (diag lane, ".d.ts alias is a
LOCAL") cost types −56 unmeasured; its CODE is reverted at ef0c2501 with
records kept and TS1035 intact — the §807 residue carries the rebuild
bar. Rule reaffirmed in STATUS: binder/loader changes measure
checker_types before landing.

**The five walls** (§151's census, both lanes' ledgers agree): variadic
tuples; the fixing-mapper inference unit (requirements = §146-narrow +
callres2's three closures); conditional-type instantiation; flow/
predicate synthesis (checker-2's, incl. the 48-line never-want equality
lead handed over); per-site re-rendering (§156/§158/printseam). The
gate-sized middle is thin — a fresh window should OPEN A WALL, and the
fixing-mapper unit is the ranked first per both lanes.

### Phase verdict (2026-08-10, checker-2): the tail-sweep is exhausted

The wrong-mass board above 80 lines is now ALL subsystem-scale:
inferTypePredicates (98 — TS5.5 predicate INFERENCE from bodies),
dependentDestructuredVariables (93 — TS4.6 dependent destructuring),
restTuplesFromContextualTypes (90), recursiveTypeReferences1 (90),
typeParameterConstModifiers (85 — behind §112's guarded prereqs),
jsdocTemplateTag6 (82), temporal (261 — relater-heavy lib),
parsingDeepParenthensizedExpression (330 — parser-lane print).
Every sub-80 family my lane censused either closed (hasInstance
439/2/0), shrank to a named subsystem block (optionalChain 67 → the
asserts machinery), or converted whole (capturedLetConstInLoop 48:0
on checker-1's lead). The next checker_types points come from
SUBSYSTEM builds: the fixing-mapper unit (requirements complete, both
lanes), the asserts/effects-signature machinery, dependent
destructuring, predicate inference. Board at phase close:
right 414,106/478,954 = 86.47%.
### Banked lead batch (2026-08-10, checker-1's narrowing-shaped sweep)

For the next window, want-inside-got wrong pairs by family: (a)
`T | undefined`-got ~113 lines (strictOptionalProperties1 12,
controlFlowInstanceof 12+12, controlFlowOptionalChain 10,
narrowCommaOperatorNestedWithinLHS 10, controlFlowDestructuring 8) —
flow lane; (b) `[] ||| never[]` 39 (destructuring initializer prints —
possibly the PRINT lane, check ownership first); (c) `E ||| E | never`
16 (never-dropping at union joins); (d) `symbol ||| unique symbol` 31
(uniqueness widening at mutable locations — declared side). The 48:0
conversion of the first such lead (capturedLetConstInLoop → checker-2's
§155) is the precedent: censuses here convert at ratio when the
mechanism is one function away.

## checker-2 window close #2 (2026-08-10, §159–§168)

**Landed**: §159 checkDerived worker slice 1 (+13), §160 reference
assignability in assignment narrowing (+20), §161 written type args on
constructor interfaces (+192 at 64:1 — the day's largest arm until
checker-1's §162), §162v2 the interface-constructor road transcribed
(+44, zero R→W where its induced twin churned 23), §162.1
getInstanceType's erased leg (+19), §163 isTypeDerivedFrom's structural
arms (+17), §165 the hasOwnProperty arm (+10/2). **Total +315 landed,
zero R→W across every transcription.**

**Refused/reverted with banked texts**: §157 (graft 4:14), §162
(induced 59:41), §164 global-target arms (+0), §166 typeof-switch
exhaustiveness (+0 both suites), §167 predicate-road tails (+0), §168
this-read (+0). The tail sweep priced at 1-in-3 reachable.

**Codified**: the transcription law (conventions.md) + the pins
corollary (induced reasoning hides in test pins).

**Board at close**: right 414,927/478,954 = 86.63% with checker-1's
§162. Ranked rocks: (1) fixing-mapper unit [checker-1's amended Phase 1:
lattice + inferToMultipleTypes TOGETHER], (2) asserts/effects-signature
machinery, (3) the thisType subsystem [twice-measured requirement],
(4) dependent destructuring, (5) predicate inference.

**Rock (6), added after close**: the JS **assignment-analysis** road
(~344 lines). checker-1's §148 measurement + checker-2's §169
baseline-writer read, merged: a JS var's type comes from its
ASSIGNMENTS — cycles land errorType (which the baseline writer spells
`error`), parameter-fed lands any, no assignments lands the implicit
any. The blanket gate was measured and refused (131 W→R / 302 R→GAP,
net −171), so the road must be built properly or not at all. This is
the largest single family on the wrong board and it is NOT
incrementally winnable.

### checker-1 window handoff #2 (2026-08-10, post-§162)

**Board: right 414,927/478,954 = 86.63%**, cases 4,312; all 100% suites
still 100%; anchors 2,820 resolved; tree clean, everything pushed.

**§162 is the session's largest arm** (+432/55 at 7.9:1) and its method is
the handoff's real content: checker-2's under-searched-tail rule found a
dispatch arm that was **absent entirely** (`signature_parts_of` had no
`ConstructorDeclaration`), and closing it forced a SECOND transcription
(`getCovariantInference`'s literal widening, `inference.go:1442`, gated by
`isTypeParameterAtTopLevelInReturnType` at `:1501`) which took the pair
from 1.3:1 to 7.9:1. Three test pins were corrected in the process; two of
them had encoded an induction ("keep the fresh literal exactly as the call
road does") that reads as evidence once it is in a test — now a conventions
corollary.

**Banked, specced, not built:**
- **§163 the this-type at a member read** — 317 wrong lines,
  `getTypeWithThisArgument` (`checker.go:19573-19596`) transcribed, the
  decidable member-read slice named, and a **representation defect found
  while speccing: `this` is minted TWICE** (per-INTERFACE in
  `declared.rs:163`, where a class's `this` type node answers errorType;
  per-CLASS-symbol in `expressions.rs`'s `this_types`). Unify before
  substituting. checker-2 ranks the same item as rock #3 from the other
  side.
- **The parked `same_base_literal_supertype` widen branch is DISCHARGED**,
  not pending: rebuilt with §162's topLevel predicate, measured zero,
  unreachable, reverted. Only its isFixed half remains live and that
  arrives with fixing-mapper Phase 2.
- **The qualified-name census (155 lines) is NOT a fresh opportunity** —
  probed, the plain shape already works both roads, the residue is
  alias-shaped and belongs to §157/§158's already-refused per-site wall.
  Only the enum-merging shapes (~17) are worth a fresh probe.
- **rock (6), the JS want-error class**, is my §148 merged with checker-2's
  §169: ~344 lines, mechanism = upstream's JS assignment-analysis road,
  blanket gate already refused at net −171. Not incrementally winnable.

**Fixing-mapper unit (rock 1)**: Phase 0 complete INCLUDING the
`inferToMultipleTypes` extension; two Phase-1 probes priced and reverted
(+4/9 lattice-alone; −41 structure-without-the-priority-out-param). The
build that lands carries lattice + structure + the priority-report
out-param together, and §162 has now supplied one of its pieces
(the widening predicate) as a proven landing.

**Rock (3) rescoped after checker-1's §164 (+89/2 at 44:1, board
415,016 = 86.65%)**: the thisType subsystem's BARE-`this` half is
CLOSED (member read at access_member_lookup + the call return when the
callee is a property access — the substitution's whole effect there is
"answer the receiver"). Remaining scope, all three untouched by that
slice: the EMBEDDED 233 (`() => this` inside a signature — needs
signature re-rendering), the reference this-ARGUMENT representation
(checker.go:19573-19582), and the DUAL-MINT unification (declared.rs
mints per-INTERFACE; expressions.rs mints per-CLASS-symbol — unify
before substituting or the road lands on one and not the other).

**Rock (3) requirements, amended after checker-1's §165 (+26/0; board
86.66%)**:

1. **The this-argument is the ORIGINAL receiver, never the apparent
   one.** Upstream reads members from the APPARENT type but passes the
   receiver itself as the this-argument
   (`getTypeWithThisArgument(apparentType, receiver)`). `members.rs:416`
   overwrote `receiver_type` with its apparent form, so both §164's and
   §165's substitutions were answering the CONSTRAINT: `x: T` with
   `T extends A` calling `x.self(): this` gave `A` where `T` is wanted.
   Capturing the pre-apparent receiver fixed four real wrongs and gained
   them back. **Anything in rock #3 that substitutes a this-argument
   must take the original receiver.**
2. **No new rendering machinery is needed at the member-read site** — a
   this-type is TYPE_PARAMETER-flagged, so `instantiate_type`'s arm 1
   already substitutes it and rebuilds the signature text.
3. **Scope kept honest**: only 26 of the embedded 233 are reachable at
   the member-read site. The rest arrive through DECLARATION lines and
   `get_type_of_symbol`, and still need the representation piece (the
   reference this-ARGUMENT extension) plus the dual-mint unification.
   Nobody should read "embedded half done".

**Rock (3) rescoped again after checker-1's §166 (+26/0)**: the dual
mint is NOT a prerequisite for READING through a this-type — consulting
both tables at the substitution site (`this_types` per class symbol,
`this_type_nodes` per interface declaration) without unifying them
works, and paid 26 lines (valueOfTypedArray's lib-interface
`valueOf(): this`, controlFlowInstanceof, tail). The unification stays a
prerequisite for the REPRESENTATION work (the reference this-ARGUMENT
extension) and nothing else. Rock #3's remaining scope is now: the
embedded lines NOT reachable at member reads (through declaration lines
and `get_type_of_symbol`), the representation change, and the
unification that gates it.

**Pattern across §164/§165/§166** — three arms, ONE transcription, three
different SITES, **+141/2 combined**, zero new machinery in any of them.
Corollary 2 paying three times consecutively is the strongest evidence
either lane has produced that site enumeration, not invention, is where
this port's remaining points live.

**Rock (7), added 2026-08-10: contextual naming for synthetic symbols.**
The naming wall's FOUR measured instances (checker-1's ledger): the
namespace mint, the class-alias mint, ES-alias references, and now
anonymous class expressions (`typeof __class` where upstream takes the
name from the BINDING). One build could discharge all four, which is
why it outranks its individual line counts. Its immediate dependent is
the ClassExpression dispatch arm (checker-1's §168, e7b0ca4d): the arm
is correct and measures +190/322 ALONE, and lands only paired with the
naming rule.


### checker-1 handoff #3 (2026-08-10, the near-miss pivot + three corrections)

**Board: cases 46.05% (4,392/9,538), gradient 86.79%** (415,692/478,954).
All other suites 100%. Tree clean, everything pushed.

**The strategic finding, and it should shape the next window.**
`checker_types` reports two numbers that reward *different work*: the line
GRADIENT and the CASE pass rate (a case passes only when every line in it
is right). Censused: **1,656 cases are blocked by ≤2 non-right lines, 851
by exactly one.** Converting that whole pool is worth **0.69% of the
gradient but +17.4 points of the case rate.** Measured leverage: §167
moved 328 lines / 30 cases (0.09 cases/line); §173 moved 27 lines / 16
cases (0.59). **Whoever sets the target must say which number it is** —
the answers point at different months of work. This remains unanswered.

**Landed** (near-miss pivot, +254 lines / 45 cases): §173 export-assignment
is not import machinery (+27/1); §175 unannotated setter parameter takes
the accessor's type (+92/0); §176 the accessor arms in `signature_parts_of`
(+3/0); §177 element access reads the APPARENT type's index signatures
(+130/2); §179 the `+` arm tests assignability for type parameters (+2/0).
Earlier: §167 regex literal is `RegExp` (+328/3), §169 function expressions
take no contextual return when showable (+39/3).

**THREE CORRECTIONS to my own published claims — read these before trusting
this file's older entries.**
1. **§180 (most important)**: I cited "cross-file binding is unported" as a
   blocker EIGHT times, from a `tsr-compiler` module note, without ever
   probing it. It is partly stale: later-lib members resolve today
   (`flat`, `at`, `padStart`, `includes`, `Object.entries` prints its full
   overload set). §174's *measurement* stands; its *explanation* does not,
   and the ~150 single-blocker cases it priced need a fresh probe.
2. **§178**: I proposed enumerating upstream's `getApparentType` call sites
   as "a census with a known yield" and then misused it — matched a
   call-site NAME to the wrong enclosing function and cost **−4,400 lines**
   before reverting. A census output is a list of places to READ, never a
   list of places to change.
3. **The fixing-mapper's fourth piece** is structural MEMBER inference
   (`inferFromProperties`/`inferFromSignatures`, `inference.go:822-825`),
   not "reference matching" as I first wrote. Upstream's reference arm
   requires the same target exactly as ours does.

**Where the remaining mass actually is, re-probed rather than assumed.**
Overload resolution WORKS (`o("s")`/`o(1)` pick correctly). Generic
inference through `T[]` WORKS (`g([1,2])` is `number`). What fails is a
union parameter with an object-literal argument (`Object.entries({a:1})`
errors though its signature resolves) and element types lost through
generic instantiation (`[1,2].flat()` gives `FlatArray<unknown, 1>[]`).
Both are the **fixing-mapper unit**, whose four pieces must land together:
lattice, `inferToMultipleTypes`, the recorded-priority channel (all three
written and preserved in the session scratchpad), and structural member
inference (untranscribed — the opener).

**Method that worked, for whoever continues**: pre-check the corpus before
building (three of my +0 arms would have been skipped); iterate with
`scorepair` only and gate once before pushing; batch arms; and claim a
census target before building it — a shared cheap census collides by
construction, which cost this window one duplicated arm.

### checker-1 addendum: the conditional wall, transcribed and ordered (2026-08-10)

Nine measured partial attempts across two subsystems this window
established that neither yields partial credit. Both now have their
whole-function texts in `checker-notes-narrow.md`:

- **§185 `getConditionalType`** (`checker.go:24300+`) — a LOOP over
  nested false-position conditionals, tail-capped at 1000; per iteration
  it instantiates both sides, applies the DEFERRAL test (`checkTuples`
  makes `[X] extends [Y]` defer like `X extends Y`), handles `infer` via
  an inference context, and only then resolves. Two rules that indict my
  own partials: **an `any` check yields the UNION of both branches**, and
  nothing resolves while the check type is deferred.
- **§186 `getConditionalTypeInstantiation`** (`checker.go:22485`) —
  where DISTRIBUTION actually lives (§183 claimed it was in the former;
  it is not). Gated on `root.isDistributive`, a property of the
  conditional's ROOT. `Never` shares the flag test with `Union`, so the
  empty distribution falls out of `mapTypeWithAlias` — which is why the
  special case I hand-wrote never fired.

**Build order, which is the reverse of how I attacked it**: conditional
ROOT (with `isDistributive`/`outerTypeParameters` — this port has no root
concept at all, the structural prerequisite) → `getConditionalTypeInstantiation`
→ `getConditionalType`'s loop and deferral test → the resolution rules.
**§182's landed primitive-domain decider is the LAST step of the LAST
item.** Everything above it is unbuilt; four measurements were spent
widening the bottom of a stack whose top does not exist.

**The same shape holds for the fixing-mapper** (four attempts, its
pre-registered falsifier fired) — and §181 established that the promise
families it kept failing on are blocked by THIS wall, not that one.

Two decisions remain open for whoever picks this up: **which metric is
the target** (cases and gradient reward different work; priced twice in
this file), and **whether to commit a session to a multi-hour build**
against these texts, which measures nothing until it lands.
