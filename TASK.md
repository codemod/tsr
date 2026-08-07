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
  checker_types 3,123/9,538 (32.74%) · 370,197/478,954 = 77.29% · gap 76,774
  · wrong 21,944. The continuation (builds 25–40): +13,544 right lines,
  +80 cases, +2.83 points, sixteen bar-scored builds, every fired leg
  honoured in writing. Arithmetic chain in STATUS §1.

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
     lines, §16's typeof-facts granularity for function/object, the
     §12.7-era assignment-LHS leftovers in JS files.

DO NOT RE-DERIVE: STATUS §5's refusals all stand. The fixpoint patch doc
(fixpoint-patch-§12.md) is now HISTORY — the mechanism landed in §12.6/§12.8;
read it only for the investigation record.
