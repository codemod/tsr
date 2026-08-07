FIRST: git pull. Read STATUS.md §1, §4.2a, §5, §7's top rows, then
docs/architecture/checker-notes-modobj.md §10 (eight bars this session, two
fired legs, both diagnosed) and checker-notes-jsx.md's tail. The last ~600
lines of docs/conventions.md still pay.

STATE AT HANDOFF (commit 07c4c6f, verify with a fresh coverage run):
  checker_types 2,793/9,538 · 351,053/478,954 = 73.30% · gap 79,429 · wrong 38,433
  Seventh session: +3,523 lines, +43 cases, ~11 lost, 0 regressed, across
  nine bar-scored builds and one measured refusal. Arithmetic chain in §1.

THE METHOD THAT WON, three times over: RE-MEASURE STALE PREMISES before
building anything. tsr-6ph's refusal priced dead designs (+859 across five
slices); the JSX "46% can't resolve" predated the /.lib mount (+1,153 from a
30-line arm); the composite-print seam's counterfactual WAS the build
(sigprint::compose == signature_to_string_at), which is why the twin landed
+1,500 on a 1,500 forecast with RIGHT->WRONG exactly its measured 2.

THE BOARD, honest read at 07c4c6f:
  - The naming/module family is HARVESTED to its edges. Refused with numbers:
    the overload braces form (14/30, 330 unmodelable), modulespecifiers'
    no-alias file half (107 import(...) wants), the ambiguity tie-break
    (~96% coincidence both ways), synthetic default, namespace-import export=.
  - tsr-fpti (P2): JSX Component<P> instance typing through heritage —
    1,955 gap + 96 wrong sized, tsx cascade behind it. Needs its own probe;
    the machinery is class heritage instantiation (tsr-4sa's refused family).
  - tsr-epnz (P2): ~29-line naming residue (export-specifier renames,
    default-qualifier chains).
  - The wrong bucket at 38,433 has never been re-split post-naming: a fresh
    wrongflip/depend pair may surface a new head — cheap, do it early.
  - Contextual typing (2,082, 86% entangled), call/inference legs, JSX —
    the remaining mass is subsystem-scale. §4.4's conclusion stands: pick ONE
    capability and accept it converts nothing until finished.

THE LOOP (nine-for-nine this session): counterfactual sizing the MECHANISM
(CONVERTS/WOULD-WRONG/AT-RISK, with a SELF-CHECK leg when modeling a
renderer) → bar in docs WITH sizing rules + falsifiers, committed BEFORE
code → build anchored to upstream file:line → tests FROM BASELINES →
verdictdump pair over a stash → score IN WRITING → five gates EACH ITS OWN
INVOCATION (fmt · clippy 0 · test ok-blocks ~110 · anchors · issue-ids) →
commit explicit paths → STATUS → push.

TRAPS PAID FOR, do not repay: ambient module names are UNQUOTED in this
binder (binder.rs:4091); a counterfactual blind to a form measures a false 0
at-risk for it (the rename leg-4 fired at 130; upstream's own
useOnlyExternalAliasing was the fix); `default` never prints as a name;
baselines recording `error` are upstream REFUSALS — computing a real answer
there is a wrong line; a probe zero is proven, never trusted.

Diagnostics is structurally blocked (ADR-0040, needs the check traversal);
no .types work moves it — say so rather than trying.

Do not stop for no reason. Keep grinding.
