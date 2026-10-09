# r6-jsdoc2 — JS file checks, JSDoc tag semantics, the r6-jsdoc remainder (round 6)

Lane `r6-jsdoc2`, epic `tsr-2zk`, items `tsr-2zk.1165` (JS-FILE-CHECK-DECLINE),
`tsr-2zk.1178` (JSDOC-TAG-SEMANTICS) and `tsr-2zk.1254` (the
[r6-jsdoc](r6-jsdoc.md) remainder). Successor owner of `jsdoc_*.rs` and
`type_argument_arity.rs`. Native source is `vendor/typescript-go` @
`5b1047d`. The cluster definitions are r6-triage's (`r6-triage.md` §2,
`r6-triage-issues.json`).

## 1. Base and method

Frozen base `2e26f22` (main after batch BQ; batch BX, r6-jsdoc, had not
landed), measured in this container: types **550,360 RIGHT** of 556,303
aligned lines (752 GAP, 5,191 WRONG); diagnostics **5,637 RIGHT / 5,606
EMPTY_RIGHT** of 12,238 rows (956 WRONG, 39 EMPTY_WRONG). Perf is Callgrind
Ir of `tsr -p <project> --singleThreaded --pretty false --noEmit`; the base
binary reads domain-model **1,092,664,269** and generic-imports
**343,107,808**, and two runs of one binary differ by up to ~0.07% on
domain-model in this container.

Every expectation was checked against a native `tsgo` built from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`). Every measurement is
unfiltered against the base, both dumps, and `slowcases` runs on both.

Lane ownership leaves the producers of item 1 in main's files. Each change
there ships as a measured diff here; the lane's own commits are neutral
alone (dead-code handshake: an owned function only a diff calls carries
`#[expect(dead_code)]` naming the diff, and the diff deletes the attribute,
so forgetting either half fails `-D warnings`).

## 2. `tsr-2zk.1165` JS-FILE-CHECK-DECLINE, read one site at a time

r6-triage's 17 diagnostics cases share a symptom — a checked JS file is
silent where `tsgo` reports — but not a producer. Native has no JS arm in
any of them except `reportImplicitAny`'s first line (`checker.go:18276`),
which declines a JS file **without `checkJs`**; this port's program already
drops such a file's semantic diagnostics
(`tsr_compiler::program_diagnostics::bind_and_check_diagnostics`), so a
checker-side `in_js_file` decline is never native's. Per case:

| case(s) | missing | producer | where |
|---|---|---|---|
| `commonjsAccessExports`, `constructorTagOnObjectLiteralMethod` | TS7009 | `check_implicit_any_new_expression`'s JS decline | §2.1, converted |
| `asyncArrowFunction_allowJs`, `checkJsdocSatisfiesTag15` (42,21), `argumentsReferenceInFunction1_Js` (1,25), `overloadTag2` (25,20) | TS7006 | `check_implicit_any_parameters`' JS decline | §2.1 |
| `argumentsObjectCreatesRestForJs`, `argumentsPropertyNameInJsMode1`/`2`, `callWithMissingVoidUndefinedUnknownAnyInJs` ×2 | TS2554 | `getArgumentArityError`: `calls.rs`/`call_arity.rs` JS gates | r6-callreport, left |
| `contextuallyTypedParametersOptionalInJSDoc`, `argumentsReferenceInFunction1_Js` (13,29) | TS2345 | `isSignatureApplicable`: same gates | r6-callreport, left |
| `jsFileCompilationBindStrictModeErrors` | TS2703 | `check_reference_expression`'s JS decline (`check.rs`) | §2.2, converted |
| `jsFileCompilationBindReachabilityErrors(alwaysstrict=true)` | TS7028 | `checkLabeledStatement`'s unused-label arm, unported (TS too) | §2.2, converted |
| `controlFlowInstanceof` | TS2339 on `{}` | `getInstanceType`'s `emptyObjectType` leg (`flow.go:979`) declines in `flow.rs`; the TypeScript twin fails the same | main, routed |
| `classFieldSuperAccessibleJs1` | TS2565 | `getFlowTypeOfAccessExpression`'s `assumeUninitialized` arm (`checker.go:11416`) unported in `members.rs` | main, routed |
| `checkJsdocSatisfiesTag10` | TS2353 | excess property against `Partial<Record<Keys, unknown>>`; the TypeScript twin (`satisfies`) fails the same | relater lane, routed |
| `checkJsdocSatisfiesTag8`, `checkJsdocSatisfiesTag15` (9,20) | TS2322 | `Object.<K,V>`, and the `@type Foo` trace | item 3 (`tsr-2zk.1254`) |

### 2.1 `reportImplicitAny` in a checked JS file

**Forcing fact.** `implicit_any.rs` returned on `in_js_file` in all six of
its reporters. The parameter decline's own comment gave the reason: "a
`@param {string} x` supplies the type upstream reads and this port does not
parse into one" — written before ADR-0046's replay existed.

**Port.** The six declines go. What native reads as `node.Type()` in JS is
read through the existing replay instead:

- parameters: `jsdoc_reparsed_parameter_type` (own `@type`, then the
  matched `@param`), then `getParameterTypeOfFullSignature`
  (`checker.go:16726`, `jsdoc_full_signature_parameter_type`);
- members: `jsdoc_self_hosted_type`; variables: `jsdoc_type_annotation`;
- TS7009 needs nothing: tsgo has no `isJSConstructor` arm, so `new F()` of
  a JS function is TS7009 exactly as in TypeScript (checked against `tsgo`).

Measured with only that, the lift was **+3 / −10** diagnostics cases: every
loss a false TS7006 where JSDoc supplies context the port's written-tree
proof of absence (`has_no_contextual_type`, `signatures.rs`) cannot see.
Native's reparser puts that context in the tree (`parser/reparser.go`): a
`@type` or `@satisfies` cast around a parenthesized or returned expression
(`:378`, `:396`), or a reparsed `Type()` on the variable, property, property
assignment, export assignment or assignment-declaration binary that holds
the function (`:346`–`:376`). Three producers, three fixes:

1. **The proof of absence.** Owned
   `jsdoc_reparse_gives_context(position)` (`jsdoc_annotations.rs`) answers
   whether one of those reparsed nodes is the position's parent or the
   parent's `Type()`; `has_no_reparsed_contextual_type` (diff, beside
   `has_no_contextual_type`) asks it at every step of the same climb. The
   retained `ReturnStatement` allow-list arm in `implicit_any.rs` skips a
   function whose parent is such a cast (its parent upstream is the cast).
2. **`@param` names with unicode escapes** (`unicodeEscapesInJSDoc`:
   `@param {number} aa` documents `aa`). `ScanJSDocToken`'s backslash
   arm and its `scanIdentifierParts` continuation (`scanner/scanner.go:1490`,
   `:1515`) were unported, and `parseJSDocIdentifierName` read the token's
   raw text. Diff [`r6-jsdoc2-jsdoc-name-escapes.diff`](r6-jsdoc2-jsdoc-name-escapes.diff)
   (`tsr-scanner/src/jsdoc.rs`, `tsr-parser/src/jsdoc.rs`).
3. **A full signature this port cannot read** (`callbackTagNamespace`'s
   `@type {NS.Nested.Inner}`, the qualified-alias gap of item 3): owned
   `jsdoc_full_signature_undecided` answers an error-typed or undecided
   full-signature type, and the parameter rule declines it instead of
   answering "no signature".

**Judgment call: the JSDoc-aware proof is the implicit-any reporter's
only.** Applied inside `has_no_contextual_type` itself (every caller) it
measured +4 type lines lost: `asyncArrowFunction_allowJs`' async arrows
under `@type {function(): string}` (context `Function`) and
`typeTagOnFunctionReferencesGeneric`'s arrow under a generic `@type` went
from RIGHT to `any`/`error`. Both were RIGHT by coincidence: the
written-tree proof said "no context", and the inference roads that would
read the real context — the async non-generator contextual return
(`signatures.rs`' decline) and a generic contextual signature — gap. The
TypeScript twins gap identically (`const b: Function = async () => 0`
prints `error`). Making the producers JSDoc-aware waits on those two roads;
until then `has_no_contextual_type` keeps the written tree, and only the
reporter, which needs the context to *withhold* a report, reads the
reparsed one. What would change this: those two roads ported, after which
the global form should measure zero-loss.

**Measured** (unfiltered against `2e26f22`):

- [`r6-jsdoc2-jsdoc-name-escapes.diff`](r6-jsdoc2-jsdoc-name-escapes.diff)
  alone: types **550,360 → 550,368 RIGHT (+8 lines)**, **+1 case**
  (`unicodeEscapesInJSDoc`); diagnostics unchanged; zero losses on both
  dumps; `slowcases` clean. Ir domain-model 1,091,809,999, generic-imports
  343,078,729 (noise). Test `tsr-parser/tests/jsdoc_name_escapes.rs` (in
  the diff), failing without it.
- [`r6-jsdoc2-js-implicit-any.diff`](r6-jsdoc2-js-implicit-any.diff),
  **applied after** the escapes diff: diagnostics **5,637 → 5,639 RIGHT
  (+2 cases**: `commonjsAccessExports`, `constructorTagOnObjectLiteralMethod`);
  `argumentsReferenceInFunction1_Js`, `checkJsdocSatisfiesTag15` and
  `overloadTag2` gain their expected TS7006 and stay WRONG on other codes
  (above, and item 2); types unchanged on the escapes state; zero losses on
  both dumps against the base; `slowcases` clean. Ir (both diffs)
  domain-model 1,091,891,786 (−0.07%), generic-imports 343,102,910
  (−0.001%). Test `tsr-checker/tests/js_implicit_any.rs` (in the diff):
  five tests, each expectation checked against `tsgo`; the TS7006 and
  TS7009 reports fail without the diff, and the three "no report" tests are
  the losses above (they fail with the declines lifted but not the
  JSDoc-aware arms).

**Left.** `asyncArrowFunction_allowJs`' TS7006 at `(p) => {}` under
`@type {function(function(): string): void}`: the context is `Function`,
whose `getContextualSignature` is nil, so native reports. The reporter does
not trust this port's `ContextualSignature::Absent` (its own §3 measured
twelve losses), so it declines; converting it is the contextual-signature
producer, not this rule.

**Falsifier.** A checked JS case where `tsgo` reports TS7006 under a
reparsed wrapper this rule now reads as context — e.g. a `@type` cast whose
type has no call signature. The proof answers "has context" for any typed
wrapper, which can only withhold a report; a case wanting the report would
show the withholding is too wide.

### 2.2 `checkDeleteExpression` and `checkLabeledStatement`'s unused label

**TS2703 in JS.** `check_reference_expression` (`check.rs`; native
`checkReferenceExpression`, `checker.go:13130`, and `checkDeleteExpression`,
`:10808`) returned on `in_js_file`. Its comment named
`plainJSBinderErrors.js`'s three TS2703 lines; native reports them too and
the program drops them, TS2703 not being a `plainJSErrors` code — which this
port's program filter (`is_plain_js_error`) now applies the same way. Diff
[`r6-jsdoc2-js-delete-operand.diff`](r6-jsdoc2-js-delete-operand.diff): the
decline goes. Test `tsr-checker/tests/js_delete_operand.rs` (in the diff),
failing without it.

**TS7028.** Not a JS gate: `checkLabeledStatement`'s second arm
(`checker.go:4219`) was unported for every file. The binder already records
the fact native records as `NodeFlagsUnreachable` on the label
(`bindLabeledStatement`, `binder.go:2158`; this port's
`NodeFacts::UNUSED_LABEL`); the checker reports it at the label when
`allowUnusedLabels` is explicitly `false` (unset is `errorOrSuggestion`'s
suggestion, never in `.errors.txt`). Diff
[`r6-jsdoc2-unused-label.diff`](r6-jsdoc2-unused-label.diff)
(`check.rs` `check_unused_label`, `checker.rs` `unused_label_is_error`).
Test `tsr-checker/tests/unused_label.rs` (in the diff), failing without it.

**Measured** (both diffs together, unfiltered against `2e26f22`; they touch
disjoint hunks and their codes are disjoint, so the attribution is by
code): diagnostics **5,637 → 5,640 RIGHT (+3 cases)** — TS2703:
`jsFileCompilationBindStrictModeErrors`; TS7028:
`jsFileCompilationBindReachabilityErrors(alwaysstrict=true)` and
`reachabilityChecks3` (TypeScript). No other row changed; types unchanged;
zero losses on both dumps; `slowcases` clean. Ir domain-model
1,092,012,508, generic-imports 343,092,233 (noise). The diffs apply in
either order and independently of §2.1's.
