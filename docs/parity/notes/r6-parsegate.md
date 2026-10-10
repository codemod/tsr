# r6-parsegate — the checker's `file_has_parse_errors` early returns (`tsr-2zk.1152`)

Round-6 single owner of one cross-cutting contract: the `file_has_parse_errors`
early returns in `crates/tsr-checker`. This box owns no source files, so every
change ships as a diff in this directory, measured alone and in stack order.
The integrator lands them. Native is `vendor/typescript-go` @ `5b1047d`.

Status at the round-6 usage checkpoint: diffs 1–4 are measured and green.
Diff 5 is a **WIP** diff. Its diagnostics were measured only as part of the
union of all lifts; types and Ir were not measured. See §5.

## 0. Base and setup

Frozen base: `claude/beautiful-shannon-ar5gh0` @ `7dba1e1` (batch BS plus the
snapshot refresh). Batch BW (r6-triage) had not landed when the base was
frozen. None of these diffs touches r6-triage's files.

- diagnostics: 12,238 cases: 5,643 RIGHT / 5,606 EMPTY_RIGHT / 950 WRONG /
  39 EMPTY_WRONG;
- types: 556,303 aligned lines: 550,412 RIGHT / 5,139 WRONG / 752 GAP.

Setup followed r5-operators3 §4. PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept outside the repo. The
oracle is `build-tsgo.sh`'s `tsgo` (`Version 7.1.0-dev`).

## 1. The native contract

`Checker.hasParseDiagnostics` (`checker.go:14066`) is
`len(sourceFile.Diagnostics()) > 0`. Native reads it at the following places,
and nowhere else in the checker:

- **Explicit tests:**
  - `checkWithStatement`'s TS2410 (`checker.go:4164`);
  - `checkGrammarModifiers`' trailing-decorator arm (`grammarchecks.go:266`);
  - `checkGrammarForInOrForOfStatement`'s two for-await arms (`:1210`, `:1236`);
  - `checkGrammarAwaitOrAwaitUsing`'s two arms (`:1693`, `:1749`).
- **Inside the grammar helpers**, which report nothing in such a file:
  - `grammarErrorOnFirstToken` (`grammarchecks.go:19`);
  - `grammarErrorAtPos` (`:29`);
  - `grammarErrorOnNode` (`:38`);
  - `grammarErrorOnNodeSkippedOnNoEmit` (`:47`);
  - `checkGrammarRegularExpressionLiteral` (`:69`);
  - `checkGrammarDecorator` (`:127`).
- **In the binder**, with the same whole-file shape: `checkContextualIdentifier`
  (`binder/binder.go:1303`, `len(b.file.Diagnostics()) == 0`).
- **At node level:** `reportUnused` (`checker.go:7092`) tests
  `NodeFlagsThisNodeOrAnySubNodesHasError`. The parser sets
  `NodeFlagsThisNodeHasError` in `finishNodeWithEnd` (`parser.go:5908`), and
  the binder propagates it upward (`binder.go:725-741`).

Plain `c.error`, `c.addDiagnostic` and `errorOrSuggestion` are never gated.

## 2. Inventory: 168 lines, classified site by site

`grep file_has_parse_errors crates/tsr-checker/src` gives 168 lines, apart
from the field and its setter. Each was read against its native counterpart.

| verdict | count | outcome |
|---|---|---|
| NATIVE-GATE: reported through a gated helper or an explicit test | 64 | kept |
| NO-NATIVE-GATE: native reports it with an ungated `c.error`/`addDiagnostic` | 91 | lifted (diffs 3–5) |
| PARTIAL: one gate covering gated and ungated codes | 7 | split (diff 2) |
| comment lines, not gates | 7 | unchanged (check.rs:4468, 4470, 5189, 10092, 10491, 12656; operator_operands.rs:39, 193) |
| no diagnostic: a TSR-only pre-resolution order (`contextual.rs:2978`) | 1 | lifted (diff 3) |
| contradicts native (`grammar.rs:343`, `grammar.rs:95`) | 2 | corrected (diff 2) |
| node-level stand-in (`unused.rs:643`) | 1 | kept, filed (§5) |

The 64 kept sites and their native anchors:

- **check.rs grammar arms, all `grammarErrorOnNode`/`OnFirstToken`/`AtPos`:**

  | check.rs line | code(s) | native |
  |---|---|---|
  | 1401 | TS1042 | `grammarchecks.go:668` |
  | 1607 | TS1231/1319 | `:206` |
  | 2875, 5804 | TS1268 | `:831` |
  | 2928 | TS1184 | `:576` |
  | 2977 | TS1114 | `checker.go:4214` |
  | 3314 | TS1242 | `:473` |
  | 3356 | TS1182/1492/1155 | `:1563-1580` |
  | 6357 | TS1235 | `:209` |
  | 6399 | TS2480 | `:1632` |
  | 6457 | TS1009/1097 | `:868`, `:874` |
  | 8171, 8242 | modifier chain | `:214-560` |
  | 8642 | TS1071 | `:292` |
  | 8706 | TS1155 | `:1582` |
  | 8814 | modifier shapes | `:478-563` |
  | 9107 | TS1046 | `:2029` |
  | 9201 | TS1039/1254 | `:1968` |
  | 9772 | TS1108/1107 | `checker.go:4100` |
  | 9806 | TS1172-1176 | `:903-966` |
  | 9869 | TS1014/1047/1048/1016 | `:695-715` |
  | 9944 | accessor grammar | `:1311-1349` |
  | 10052 | TS1015 | `:712` |
  | 10316 | TS1118 | `:1144` |
  | 10406 | TS1117 | `:1139` |
  | 11010 | TS1163 | `:1780` |
  | 11112 | TS1206 | `:646` |
  | 12257 | TS18058-60 | `:2129-2135` |
  | 12353 | TS1221/1222 | `:998` |
  | 12388 | TS1169/1170 | `:1420` |
  | 12495 | TS18060/1323 | `:2169` |
  | 12544 | TS1206 | `:246` |
  | 12580 | TS18006 | `:1889` |
  | 12861 | TS1107/1104/1105/1115/1116 | `:1480` |
  | 12992 | TS1036/1183 | `:2047` |

- **check.rs, explicit native tests:**

  | check.rs line | code(s) | native |
  |---|---|---|
  | 7717 | TS1103 | `:1236` |
  | 10174 | TS1308 | `:1749` |
  | 3461 | TS1359 | binder `:1303` |

- **Other files:**

  | site | code(s) | native |
  |---|---|---|
  | grammar.rs:95, 1134 | `grammar_error_on_first_token` and the decorator target | `:19`, `:222` |
  | grammar.rs:1404 | TS1211 | `checker.go:4286` |
  | strict_mode.rs:268 | TS1212-1214 | binder `:1303` |
  | strict_mode.rs:424 | TS2410 | `checker.go:4164` |
  | jsx_intrinsic.rs:969 | TS18007 | `:1192` |
  | jsx_component.rs:1356 | JSX grammar | `:1156`, `:1180` |
  | import_call_grammar.rs:35 | TS1009 | `:2182` |
  | import_call.rs:23 | TS1324/1450/1325 | `:2162` |
  | import_meta.rs:58 | TS17012/18061 | `:1831` |
  | import_attributes.rs:36, 106 | attribute grammar | `checker.go:5408`, `:3332` |
  | nullish.rs:69 | TS5076 | `checker.go:12920` |
  | module_format.rs:273 | TS1202 | `checker.go:5497` |
  | module_format.rs:310 | TS1203/1218 | `checker.go:5673` |
  | module_format.rs:405 | TS1216 | `:1617` |
  | module_format.rs:558 | top-level await | `:1693`, `:1210` |
  | module_format.rs:733 | TS7059 | `checker.go:12290` |
  | module_format.rs:751 | TS7060 | `:785` |
  | module_format.rs:786, 821 | TS18057/TS1003 | `checker.go:5388` |

Line numbers are at `7dba1e1`.

## 3. Lifting exposed four missing native tests, not parser divergences

Lifting every NO-NATIVE-GATE site at once measured diagnostics +26 / −7. Each
loss was bisected to a single site with a per-site switch (an experiment-only
`TSR_PG_KEEP` environment list, not shipped). In every case the lost case is
in a file with parse errors, and the extra report comes from a **native
test the gate had been standing in for**. None of them is a recovery
divergence: TSR's tree matched native's in each case.

1. **`super<T>()`**: `superWithTypeArgument`, `errorSuperCalls`, extra TS2558.
   Both parsers fold the type arguments into the call
   (`parseCallExpressionRest`, `parser.go:5462`). Native `resolveCall` then
   reads no type arguments for a super call: `!isSuperCall(node)`,
   `checker.go:8851`. Ported in `check_candidates_arity`.
2. **Incomplete tagged template**: `taggedTemplatesWithIncomplete…1/2`, extra
   TS2554. Native `hasCorrectArity`'s `callIsIncomplete` (`checker.go:9118-9131`,
   `:9165`) skips the lower bound when the template's last literal is missing
   or unterminated. The old doc comment said this was "not modelled: such a
   file has parse errors". Ported as `tagged_template_call_is_incomplete`.
3. **`export * from Aaa;` in a namespace**: `exportDeclarationInInternalModule`,
   extra TS1194. `checkExternalImportOrExportDeclaration` (`checker.go:5333`)
   returns after TS1141, before TS1194.
4. **`{ get e, }`**: `objectLiteralShorthandPropertiesErrorFromNotUsingIdentifier`,
   extra TS2378. `checkAccessorDeclaration` requires `NodeIsPresent(body)`
   (`checker.go:2941`). The recovered block is empty (`pos == end`).

These four ship together as diff 1, ahead of the lifts that need them.

**Two kept sites contradicted native.** Diff 2 corrects them:

- **`grammar.rs:343`** reported TS1141 *only* in files with parse errors,
  outside a module context. Native's `checkGrammarModuleElementContext`
  returns `!isInAppropriateContext` whether or not its own report was
  silenced, so callers never reach TS1141 there. The appropriate contexts now
  also include `ModuleDeclaration`.
- **`grammar.rs:95`** returned `true` and marked the modifier chain as
  reported when the file's parse errors had silenced the report. Native's
  `return c.grammarErrorOnFirstToken(…)` yields `false`, so the callers'
  `!checkGrammarModifiers` rules still run.

## 4. Diffs, in apply order

Each diff was measured on the previous one, both dumps unfiltered. A loss
counts when a base RIGHT/EMPTY_RIGHT case or line changes verdict. slowcases
ran on both dumps.

| # | diff | content | diagnostics | types | slowcases |
|---|---|---|---|---|---|
| 1 | `r6-parsegate-1-native-tests.diff` | the four native tests in §3 (calls.rs, check.rs) | byte-identical | byte-identical | clean |
| 2 | `r6-parsegate-2-partial-splits.diff` | the 7 PARTIAL sites split; grammar.rs:343 and :95 corrected | **+2**, 0 lost | identical | clean |
| 3 | `r6-parsegate-3-lift-calls.diff` | call resolution lifts (10 sites) | **+17**, 0 lost | identical | clean |
| 4 | `r6-parsegate-4-lift-access.diff` | member/element access lifts (24 sites) | **+5**, 0 lost | identical | clean |
| 5 | `r6-parsegate-5-WIP-lift-declarations-relations.diff` | declaration/statement and relation/iteration lifts (58 sites) | **+6**, 0 lost (union run only) | UNMEASURED | UNMEASURED |

Diff 2's PARTIAL splits each keep the native-gated code gated and lift the
rest:

| site | gated (native helper) | lifted (native `c.error`) |
|---|---|---|
| check.rs:1448 | TS18016, object literal | TS18016, type literal and interface (`checker.go:2718`, `:2813`) |
| check.rs:1641 | TS1120, TS1036 | isolated-module arms |
| check.rs:3194 | for-in/of declaration arms | TS2491 |
| check.rs:3584 | TS1156, type and interface | TS1156, variable form (`grammarchecks.go:1807`) |
| check.rs:3813 | TS1113 | TS2678 |
| check.rs:12157 | TS2462, binding pattern | TS2462, destructuring assignment |
| readonly_target.rs:717 | TS2803, TS18016 | TS18013, accessibility, TS2339 |

Cases converted:

- **Diff 2:**
  - `compiler/superInLambdas(target=es2015)`;
  - `conformance/privateInstanceMemberAccessibility(target=es2015)`.
- **Diff 3:**
  - compiler: `objectCreationExpressionInFunctionParameter`,
    `superWithTypeArgument2`, `superWithTypeArgument3`,
    `taggedTemplatesWithIncompleteTemplateExpressions3`–`6`;
  - conformance: `instantiationExpressionErrors`, `newOperatorErrorCases`,
    `parserMissingLambdaOpenBrace1`, `templateStringInObjectLiteral`,
    `templateStringInObjectLiteralES6`, `templateStringInPropertyName1`/`2`,
    `templateStringInPropertyNameES6_1`/`_2`, `typeAssertions`.
- **Diff 4:**
  - compiler: `extension`, `identifierStartAfterNumericLiteral`, `libMembers`;
  - conformance: `objectSpreadNegativeParse`, `parserRealSource7`.
- **Diff 5 (union run):**
  - compiler: `aliasErrors`,
    `functionsMissingReturnStatementsAndExpressions(target=es2015)`,
    `objectLiteralWithSemicolons5`;
  - conformance: `labeledStatementDeclarationListInLoopNoCrash3(target=es2015)`,
    `parserErrorRecovery_Block3`,
    `parserMemberAccessorDeclaration8(target=es2015)`.

Stack total: diagnostics 5,643 → 5,673 RIGHT (+30, 0 lost). Types were
byte-identical through diff 4. An earlier union run, without diff 2's splits
and without the contextual.rs lift, also left types byte-identical.

The lifted sites, by diff, numbered as in the per-site experiment:

- **Diff 3:**
  - check.rs:5926 (TS2511);
  - implicit_any.rs:848 (TS7009);
  - call_arity.rs:36, :193;
  - calls.rs:737, :2136, :2180;
  - type_argument_arity.rs:39, :245;
  - contextual.rs:2978.
- **Diff 4:**
  - readonly_target.rs:35, 448, 1060, 1107, 1330, 1789;
  - spread_overrides.rs:23, 129, 196;
  - delete_operand.rs:27;
  - tuples.rs:648, 685, 715, 748;
  - computed_name.rs:48, 292;
  - private_setter_read.rs:24;
  - nonexistent_property.rs:92, 119;
  - index_constraint.rs:171;
  - index_access_reports.rs:121, 360, 570, 714.
- **Diff 5:**
  - check.rs:1241, 1354, 1519, 1824, 1866, 2081, 2140, 2277, 2638, 3013,
    3285, 3538, 4488, 5772, 5886, 5970, 6004, 6046, 6279, 6602, 7216, 7781,
    7831, 7882, 8770, 9657, 10681, 10774, 11308, 11393, 11505, 11562, 11677,
    12308, 12453, 13529, 14251, 14328, 14433, 14558;
  - meaning_mismatch.rs:487;
  - parameter_self_reference.rs:21;
  - heritage_conformance.rs:326;
  - expressions.rs:3067, 3093;
  - flow.rs:8895;
  - enum_member_name.rs:66;
  - satisfies.rs:22;
  - iteration.rs:1249, 1346, 1364, 1438, 1519;
  - jsdoc_annotations.rs:28, 227;
  - comparison_overlap.rs:50;
  - destructuring_assignment.rs:63, 97.

Every lifted site's native counterpart reports with an ungated `c.error` or
`addDiagnostic`. The classifier tables in the session scratchpad name each
anchor, and they are folded into the per-diff lists above.

**Coordination.** Each hunk is the gate line or its guarded block. Most of
them touch other lanes' files:

| owner | files |
|---|---|
| r6-callreport | calls.rs |
| r6-errorsplit3 | readonly_target.rs, import_meta.rs (untouched), jsx_*.rs (untouched) |
| r6-jsdoc2 | type_argument_arity.rs, jsdoc_annotations.rs |
| r6-relater3 | index_access_reports.rs |
| r6-modules4 | meaning_mismatch.rs |

Diffs 1 and 3 carry the only multi-line hunks in calls.rs: the super-call arm
and `tagged_template_call_is_incomplete`.

## 5. Not done, and why

- **Diff 5 is WIP.** Its diagnostics are measured only inside the all-lifts
  union run (0 lost). It still needs its own types dump, slowcases, and a
  re-measure on the integration tip.
- **Ir is UNMEASURED for every diff.** domain-model and generic-imports are
  parse-clean, so `file_has_parse_errors` is false and every lifted line is
  one boolean test fewer. They should be flat, but this has not been checked.
- **Stack not re-measured on the integration tip.** It was measured on
  `7dba1e1` only. Batch BW has not landed.
- **Stale comments next to lifted sites** still need a pass, for example
  nonexistent_property.rs:90 ("retain its existing parse-error decline") and
  `Checker::file_has_parse_errors`' own doc comment (checker.rs:688). The doc
  comment should now say that the flag mirrors `hasParseDiagnostics` and is
  read only at the sites in §2.
- **`unused.rs:643` is kept** as a stand-in for the node-level
  `NodeFlagsThisNodeOrAnySubNodesHasError` (§1). The faithful port has three
  parts:
  1. the parser sets `THIS_NODE_HAS_ERROR` in `finish_node` after an error
     (parser.go:5908; `tsr_ast::NodeFlags` already declares the bit);
  2. the binder propagates it upward (binder.go:725);
  3. `reportUnused` tests the flag on each location.

  That work is in the parser and binder (MAIN). It would convert
  `compiler/unusedLocalsAndParameters`. Needs a Beads issue.
- **Call/new `callIsIncomplete`** (a missing `)`: `node.ArgumentList().End()
  == node.End()`, `checker.go:9151`) is not ported. TSR's `CallExpression`
  keeps no argument-list end. No corpus case needed it after the lift.
- **Triage cases not converted by any lift.** These have other causes:
  - flow: `disallowedBlockScopedInPresenceOfParseErrors1` (TS2454);
  - destructuring and contextual typing: `destructuringControlFlowNoCrash`,
    `destructuringObjectBindingPatternAndAssignment3`,
    `destructuringParameterDeclaration2`, `destructuringParameterProperties4`;
  - `newOperator` (TS2351, TS7053);
  - JSDoc template: `jsdocTemplateTag3`, `jsdocTemplateTagDefault`;
  - `parser519458` (TS2591);
  - `parserRealSource10` (TS2449);
  - `taggedTemplatesWithTypeArguments2` (TS2345);
  - `tsxStatelessFunctionComponents1`;
  - `varianceAnnotationsWithCircularlyReferencesError` (TS2637 vs TS2314).

**Tests.** `cargo test -p tsr-checker --release` passes on the full stack.
`cargo clippy -p tsr-checker` flags only pre-existing stable-toolchain lints
in files the stack does not touch. The workspace-wide test run was not
repeated at the checkpoint.
