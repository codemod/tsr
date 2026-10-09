# Lane notes: r6-errorsplit2 (tsr-2zk.1140, continuing r6-errorsplit)

Single owner of the intrinsic/error contract, round 6, second box. The
previous step is [r6-errorsplit](r6-errorsplit.md); the decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md):
a producer switches to `native_error` only where the native identity probe
confirms every line it moves, and a rewrite is narrowed only where that costs
zero RIGHT lines. Pinned upstream: `vendor/typescript-go` @ `5b1047d`. Owned
files: `indexed.rs`, `spreads.rs`, `jsx_intrinsic.rs`. Everything else ships
as a measured diff here.

## §1 Base and instrument

**Base.** Batch BH (r6-errorsplit's commits and diffs O, L, G, S and M) had
not reached the integration branch when this session started; batches BF and
BG were ahead of it in the queue. The base is therefore the integration branch
at `c7736c8` (batch BF) merged with r6-errorsplit's branch (`c3b27b2`), with
the five diffs applied in BH's order (O, L, G, S, M) exactly as shipped. That
is BH's content. Frozen there, unfiltered:

- types 550,093 RIGHT / 784 GAP / 5,426 WRONG (556,303 aligned lines);
- diagnostics 5,574 RIGHT / 5,602 EMPTY_RIGHT / 1,019 WRONG / 43 EMPTY_WRONG
  (12,238 cases);
- `ceiling`: credited gap **2,129**; `native_error` lines 31,277 (31,173
  matched); wholesale narrowing would cost **2,647** RIGHT lines
  (`HadErrorBaseline` 1,983, `AtLocation` 575, `AccessOrQualifiedParent` 62,
  `StatementName` 27).

Those are r6-errorsplit §9's "+ S" column, so the emulation reproduces BH. The
diffs below apply on this base; they are re-checked against the real tip
once BH lands.

**The probe** is r5-errorsplit4 §2.1's instrument, rebuilt from scratch. The
pinned tsgo toolchain is built with `scripts/offline-cargo/build-tsgo.sh`. The
compiler test runner is compiled with `go test -c -overlay`, replacing two
files (nothing tracked changes):

- `type_symbol_baseline.go`: under `TSR_ERRPROBE`, the writer tags a line
  ` @@E` when its type is `GetErrorType()`, ` @@A` when it is
  `GetAnyType()`, and, new in this round, ` @@X` for any other any-flagged
  type. It writes the `.types` text to the directory instead of diffing it,
  and skips `.symbols`.
- `compiler_runner.go`: under the same variable, only `verifyTypesAndSymbols`
  runs.

`-test.run '^TestSubmodule$'` writes 12,157 baselines in 48 s. The port side
is a measurement-only copy of `ceiling` (never committed). Its `LINE` rows add
the rewrite, the `gap_reason` producer and the expected baseline text. A join
on `(case, file, position)` tags each row, and requires the probe's text to
equal the expected text; a row that fails that is reported as unaligned.

**Control.** Of the base's 31,173 matched `native_error` lines, 31,166 are
`errorType` natively. Of the other 7, 4 are unaligned (parse recovery), 2 are
`@@X` and 1 is `anyType`. The 2 `@@X` lines were "untagged" in
r6-errorsplit §1. The instrument agrees with every earlier switch.

**Setup.** PyPI is blocked. `assemble.py`'s three `tomlkit` calls ran against
a stdlib-only stand-in kept outside the repository (r5-operators3 §4):
`parse` is `tomllib.loads`, `inline_table` a marked `dict`, `dumps` a small
table writer.

### §1.1 `HadErrorBaseline`, classified

`HadErrorBaseline` is the writer's SS180 rewrite: a case with an error
baseline prints every any-flagged type `any`, and the rewrite applies it to
the port's gap too. Its lines at the base, joined with the probe:

| verdict | `errorType` | `anyType` | other any (`@@X`) | not any natively | unaligned |
|---|---:|---:|---:|---:|---:|
| RIGHT (narrowing cost) | 1,923 | 60 | 0 | 0 | 0 |
| WRONG | 10 | 1 | 13 | 806 | 18 |

So the three populations the dispatch asked for are:

- **`errorType` natively: 1,933 lines.** The port lacks upstream's
  `errorType` producer, or a consumer propagates the port's gap where
  upstream propagates `errorType`. These are the switch candidates.
- **A native producer the port lacks: 74 lines** (60 + 1 `anyType`, 13 other
  any-flagged types). Upstream computes `any` there, and the port has no arm
  for it. The 60 RIGHT ones sit in:
  - binding elements with neither annotation nor initializer
    (`fallbackToBindingPatternForTypeInference`, `declarationsAndAssignments`,
    `destructuringArrayBindingPatternAndAssignment2`,
    `iterableArrayPattern21`, `bindingPatternCannotBeOnlyInferenceSource`;
    `destructure.rs`, main's);
  - a class extending `any` (`extendFromAny`, `classExtendingAny`:
    `anyBaseTypeIndexInfo`, `checker.go:19147`/`:20693`, which the port's
    index image in `index_signatures.rs` does not add);
  - `yield` results (`crashInYieldStarInAsyncFunction`,
    `genericCallAtYieldExpressionInGenericCall1`);
  - two private names outside class bodies (§4 ports these).
- **A gap: 806 lines**, all already WRONG. Upstream computed something other
  than any; the port computed nothing.

By the port's producer (`gap_reason`'s leading clause), the 1,923 RIGHT
`errorType` lines are led by:

- unreached nodes in no expression position (§2, 382);
- property-access misses (369);
- calls (170) and variable declarations (≈260), most of them downstream
  of the first two;
- aliases with no value (116);
- private names (50, §4).

## §2 Diff N (`types_producer.rs`, the harness): `getTypeOfNode`'s fall-through

[`r6-errorsplit2-type-of-node-fallthrough.diff`](r6-errorsplit2-type-of-node-fallthrough.diff)

`getTypeOfNode` (`checker.go:31927`) ends with `return c.errorType` (`:32035`)
for a node that no arm claims and that is no expression node. The harness's
`type_id_at_location_arm` mirrors that arm order and ended with the port's
gap (`ProducerArm::NoArm`). Its own comment already said the answer is
`errorType`: neither half of a `JsxNamespacedName` is an expression node.

Behind a temporary switch, every line it moves is `errorType` natively:
**384 of 384** (382 RIGHT, 2 WRONG):

- both halves of namespaced JSX names: `jsxNamespacePrefixInName{,React}`
  136, `tsxNamespacedTagName{1,2}`, `tsxNamespacedAttributeName{1,2}`,
  `jsxNamespacePrefixIntrinsics`, `checkJsxNamespaceNamesQuestionableForms`;
- names in no expression position, such as `genericClassPropertyInheritanceSpecialization`
  and `declFileGenericType2`;
- import attribute names (12).

The diff switches the fall-through to `native_error`.

**Measured** on §1's base (unfiltered, both dumps): zero transitions. Credited
gap 2,129 → **1,747**. Narrowing: `HadErrorBaseline` 1,983 → 1,663,
`AccessOrQualifiedParent` 62 → **8**. The harness is main's, so this ships
as a diff. It carries `tsr-conformance/tests/type_of_node_fallthrough.rs`,
which pins `<a:element />`'s two names as `native_error` beside the
declared `a`.

Alternative considered: switching only the `JsxNamespacedName` positions
(the largest family). Rejected: the arm is one upstream line with one
identity, and the probe found no false claim anywhere under it, so a narrower
switch would be a special case.

## §3 Diff P (`members.rs`, with `indexed.rs`): the property-access miss

[`r6-errorsplit2-property-access-miss.diff`](r6-errorsplit2-property-access-miss.diff)
applies on diff N (order N, P).

`checkPropertyAccessExpressionOrQualifiedName` reports a missed name and
returns `errorType` (`checker.go:11353-11369`). The port answers `any` where
its §123 walk establishes the miss, and its gap everywhere else. This is the
dispatch's item 3. `(options || {}).a = 1` widens to `{}` and misses there,
and it is one line of this population.

**Measured arm by arm** (behind temporary switches, on N):

| gate on the miss | `errorType` | false claims |
|---|---:|---:|
| none (TS files) | 523 + 12 | 157 untagged, 7 `anyType`, 1 unaligned |
| `element_access_receiver_is_complete` | 430 + 12 | 41 untagged, 5 `anyType`, 1 unaligned |
| … and a flow-narrowed identifier or access receiver kept as the gap | 391 + 12 | 40, 5, 1 |
| … and writes only | 14 | 1 unaligned |
| complete, no mapped constituent, receiver no flow reference | **15** | **0** |

The false claims under the complete-receiver gate trace to three causes:

- **The port's narrowing falls short of upstream's** (`instanceof`, `in`,
  aliased conditions, truthiness, incomplete loop types):
  `truthinessCallExpressionCoercion2` 12, `controlFlowWithIncompleteTypes` 3,
  `discriminatedUnionTypes2/3` 3, `controlFlowAliasing` 2,
  `inKeywordAndIntersection` 2, `typeGuardNarrowBy*UntypedField` 2,
  `narrowingNoInfer1`, `typeGuardsWithInstanceOfByConstructorSignature`,
  `controlFlowGenericTypes`. Upstream's narrowed receiver has the member;
  the port's does not. The narrowed-receiver gate in the table above
  compares the flowed type with the declared one, and it barely helps: the
  port usually fails to narrow at all, so the two are equal.
- **Module augmentation and namespace merging**: `exportAsNamespace_augment`
  8, `ambientModuleDeclarationWithReservedIdentifierInDottedPath{,2}` 6,
  `namespaceImportTypeQuery`.
- **Receivers the port types wrong**: an `Extract<…>` constraint
  (`deeplyNestedConstraints`), a destructuring default
  (`nonPrimitiveAndEmptyObject`), and a class extending `any`
  (`extendFromAny`, `anyType`; see §1.1).

None of them is visible at the lookup; each is the port's receiver being
wrong first. So the diff switches only where the receiver cannot be one of
them:

- **The receiver is no flow reference**: none of `isMatchingReference`'s
  source kinds (`flow.go:1606-1621`: identifiers, `this`, `super`,
  meta-properties, property and element accesses, through parentheses,
  non-null and `satisfies`). Narrowing applies only to references, and every
  narrowing false claim has a reference receiver. A qualified name's left is
  an entity name, also a reference.
- **The receiver is complete** (`element_access_receiver_is_complete`,
  r6-errorsplit's test).
- **No union or intersection constituent is a mapped type.**
  `complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound`'s
  `Pick<T, 'type'> & Partial<Omit<T, …>> & {…}` misses `phoneNumber`, which
  upstream finds. The element road keeps such receivers: putting this
  exclusion in the shared test moved two element-access lines
  (`narrowingMutualSubtypes`, a `Record<string, any>` constituent, both
  `errorType`) back to the gap.
- **The receiver is not a primitive that maps to a global interface**
  (`String`, `Number`, `BigInt`, `Boolean`, `Symbol`). The lookup receives
  the apparent type, and such a primitive is still itself only when its
  global is missing. That is the port's gap, as
  `tests/members_apparent_type.rs` pins: the first cut answered
  `native_error` there and the test went red. A first version used
  `TypeFlags::PRIMITIVE`, which also caught `void` and moved
  `parserNoASIOnCallAfterFunctionExpression1`'s two `errorType` lines back.

The new test is `property_access_receiver_is_complete` in `indexed.rs`. It
is used only by this diff, so it ships inside it.

**Refused, with the numbers:**

- *Reads with a reference receiver*: +391 to +430 `errorType` lines against
  40 to 46 false claims. The fix is the narrowing and augmentation ports that
  make those receivers right (main's `flow.rs`, `symbols.rs`), not a gate.
- *Writes with a reference receiver*: 13 more lines, probe-clean
  (`externalModuleImmutableBindings` 9, `intersectionWithConflictingPrivates`,
  `jsdocInTypeScript`, `propertyAccess1`, `contextualReturnTypeOfIIFE2`).
  A write's receiver is narrowed exactly as a read's is
  (`checkPropertyAccessExpression` checks the left with
  `checkNonNullExpression`), so no upstream reason separates writes from
  reads. They are probe-clean only because the corpus has few narrowed write
  receivers. Taking them would be a heuristic, so they stay out.

**Measured** on N (unfiltered, both dumps): zero transitions. 15 lines move
to `native_error`, all `errorType` natively:

- `doYouNeedToChangeYourTargetLibraryES2016Plus` 4,
  `orderMattersForSignatureGroupIdentity` 4;
- `parserNoASIOnCallAfterFunctionExpression1` 2;
- `propertyAccessWidening:0:55`, the dispatch's `(options || {}).a = 1`;
- `exhaustiveSwitchStatements1`, `inferentiallyTypingAnEmptyArray`,
  `mergedClassNamespaceRecordCast`, `missingDomElements`.

Ten `AtLocation` member-name lines stop being gap rewrites with them. Credited
gap 1,747 → **1,732**; `HadErrorBaseline` 1,663 → 1,648; `AtLocation` 575 →
565. The diff carries `tsr-checker/tests/property_access_miss.rs`.

## §4 Diff Q (`members.rs`, `readonly_target.rs`): a private name no class declares

[`r6-errorsplit2-private-name-without-declaration.diff`](r6-errorsplit2-private-name-without-declaration.diff)
applies on diff P (order N, P, Q).

When `lookupSymbolForPrivateIdentifierDeclaration` finds nothing, upstream
still checks the receiver, then (`checker.go:11284-11310`, and the `prop ==
nil` exits at `:11327-11353`) answers:

- `anyType` for an any-like receiver outside every class body (TS18016);
- otherwise `errorType`: after `checkPrivateIdentifierPropertyAccess`'s
  shadowing or not-accessible report, or after the plain miss. The
  `globalThis` exit answers `anyType` instead.

The port returned its gap before checking the receiver. The diff ports the
arm after the receiver's non-null strip and widening, the same point
upstream reaches it. A JS file keeps the gap, because its
`isUncheckedJSSuggestion` and `isJSLiteralType` exits are not consulted on
this road. `getContainingClassExcludingClassDecorators` already exists in
`readonly_target.rs` and is made `pub(crate)`.

**Measured** on N + P (unfiltered, both dumps): zero transitions.

- 37 lines move to `native_error`, all `errorType` natively, across 20
  `privateName*` cases and `classStaticBlock16`.
- 2 lines leave the gap for `any`: `privateNameBadAssignment`'s
  `exports.#nope` and `A.prototype.#no`, both `anyType` natively.

Credited gap 1,732 → **1,693**; `HadErrorBaseline` 1,648 → 1,609. The diff
carries `tsr-checker/tests/private_name_without_declaration.rs`. It also
flips `tests/private_names.rs`' pin of the absent name from the gap's
printed `error` to `native_error`'s `any`.

## §5 The stack, measured

N, P and Q applied in that order on §1's base, unfiltered, both dumps:

| | base | N | N, P | N, P, Q |
|---|---:|---:|---:|---:|
| types RIGHT / GAP / WRONG | 550,093 / 784 / 5,426 | same | same | same |
| diagnostics | 5,574 / 5,602 / 1,019 / 43 | same | same | same |
| transitions (both dumps) | — | 0 | 0 | 0 |
| moved to `native_error` | — | 384 | 399 | 436 |
| of those `errorType` natively | — | 384 | 399 | 436 |
| credited gap | 2,129 | 1,747 | 1,732 | **1,693** |
| `native_error` lines (matched) | 31,277 (31,173) | 31,661 (31,555) | 31,676 (31,570) | 31,713 (31,607) |

The identity work moves no verdict: every moved line printed the same before
and after, because the writer's rewrites treat both identities alike
(ADR-0048 decision 3).

- **slowcases** is clean on both dumps.
- **Ir** (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded
  --pretty false --noEmit`), against the base binary:
  - domain-model +0.055%, +0.001% and +0.055% on three runs, while the
    base binary alone read 1,091,405,440, 1,091,971,929 and 1,091,389,383
    (0.05% apart);
  - generic-imports −0.013%, −0.006% and −0.001%.

  Both are inside the noise. The added work is one receiver-kind test per
  property miss that the §123 walk does not establish, and the private arm
  where the port used to return.
- **Tests**: `cargo test --workspace --release` passes. Clippy reports
  nothing in the touched code (it flags pre-existing code in
  `enum_initializer.rs`, `signatures.rs`, `templates.rs`, `printing.rs`,
  `index_signatures.rs`, `unique_symbols.rs`, `symbols.rs` and
  `tsr-dts/tests/accessibility.rs`).

## §6 Narrowing, re-measured

| rewrite (RIGHT→GAP cost) | base | N | N, P | N, P, Q |
|---|---:|---:|---:|---:|
| `HadErrorBaseline` | 1,983 | 1,663 | 1,648 | 1,609 |
| `AtLocation` | 575 | 575 | 565 | 565 |
| `AccessOrQualifiedParent` | 62 | 8 | 8 | 8 |
| `StatementName` | 27 | 27 | 27 | 27 |
| **total** | 2,647 | 2,273 | 2,248 | 2,209 |

Every rewrite still prints RIGHT gap lines, so none is narrowed. The rule is
zero RIGHT lines, and no sub-population was found where narrowing costs
nothing. The falsifier ("the residual stops falling") has not fired:
2,647 → 2,209.

`AccessOrQualifiedParent`'s last 8 and `StatementName`'s 27 were item 4.
Item 4 waits on item 1, and item 1 is not exhausted. They are left
unprobed this session.

## §7 Diff W (`types_producer.rs`, the harness): the `with` body

[`r6-errorsplit2-with-statement-body.diff`](r6-errorsplit2-with-statement-body.diff)
applies on diff N (order N, P, Q, W, M).

`getTypeOfNode` opens with `if node.Flags&ast.NodeFlagsInWithStatement != 0
{ return c.errorType }` (`checker.go:31932`). The harness recomputes the
flag by an ancestor walk and returned the gap. That was the port's own
miscoloring: its comment quotes upstream's `errorType`. The diff answers
`native_error` there and stops setting the port's-failure flag, which belongs
to the gap only (ADR-0048 decision 3).

**Measured** on N, P, Q (unfiltered, both dumps): zero transitions. 62 lines
move to `native_error`, all RIGHT and all `errorType` natively:

- the `withStatement*` cases;
- `sourceMapValidationStatements` and `superCallsInConstructor`;
- `letDeclarations-{scopes,validContexts,invalidContexts}`;
- `jsFileCompilationBindStrictModeErrors`, `functionExpressionInWithBlock`,
  `plainJSBinderErrors` and `elidedEmbeddedStatementsReplacedWithSemicolon`.

Credited gap −62. The diff carries
`tsr-conformance/tests/with_statement_body.rs`, which pins a literal in a
`with` body as `native_error` beside the `with` expression's own `any`.

## §8 Diff M (`import_meta.rs`, with `check.rs`): `checkMetaProperty`'s type half

[`r6-errorsplit2-meta-property-type.diff`](r6-errorsplit2-meta-property-type.diff)
applies on §1's base alone (order N, P, Q, W, M). `import_meta.rs` has no
lane owner this round, and `check.rs` is main's.

Upstream (`checker.go:10753-10797`) answers:

- `import.defer` (outside a call) and `import.<other>`: `errorType`. The
  port's doc comment said so, and the code returned the gap.
- `new.target`: `checkNewTargetMetaProperty` (`:10768`). With no
  `GetNewTargetContainer`, TS17013 and `errorType`. In a constructor, the
  class symbol's type. In a function declaration or expression, the
  function's own type.

The port answered the gap for every `new.target`. Its TS17013 report
(`check_new_target_meta_property`) already computes the container through
`new_target_this_container`, so the diff makes that walk `pub(crate)` and
ports the type half beside the `import` arm.

**Measured** on N, P, Q, W (unfiltered, both dumps): **zero losses; types +9
WRONG→RIGHT**:

- `shadowedReservedCompilerDeclarationsWithNoEmit`'s `new.target : typeof C2`
  and the two lines it initializes;
- `misspelledNewMetaProperty`'s `new.targ : () => void` and its
  initialized variable;
- `invalidNewTarget.es{5,6}`' object literal `O` and its initializer, both
  printed once their methods' `new.target` answers `any`.

Diagnostics unchanged, zero transitions. 57 lines move to `native_error`,
all `errorType` natively (`invalidNewTarget.es6` 42,
`dynamicImportDeferInvalidStandalone` 10, `importMetaPropertyInvalidInCall`
5). Among them are 23 `AtLocation` lines whose rewrite printed `any` for the
gap. Credited gap −31; `AtLocation` 565 → 542.

The diff carries `tsr-checker/tests/new_target_meta_property.rs`. It also
moves `tests/tagged_templates.rs`' anti-vacuity span off `import.foo`, which
no longer gaps (as that test's own comment foresaw), onto a missed
property of a declared reference receiver (§3 keeps that the gap).

## §9 The stack after W and M

N, P, Q, W, M on §1's base, unfiltered, both dumps:

- types **550,102 / 784 / 5,417** (+9 WRONG→RIGHT, all from M);
- diagnostics unchanged (5,574 / 5,602 / 1,019 / 43), zero transitions;
- zero losses on both dumps;
- moved to `native_error`: 555 lines, every one `errorType` natively;
- credited gap 2,129 → **1,600**; `native_error` lines 31,832 (31,726
  matched);
- slowcases clean on both dumps;
- Ir against the base binary: domain-model +0.004%, generic-imports
  −0.006% (one run each, inside §5's spread).

| rewrite (RIGHT→GAP cost) | base | N, P, Q | + W, M |
|---|---:|---:|---:|
| `HadErrorBaseline` | 1,983 | 1,609 | **1,517** |
| `AtLocation` | 575 | 565 | **542** |
| `AccessOrQualifiedParent` | 62 | 8 | 8 |
| `StatementName` | 27 | 27 | 27 |
| **total** | 2,647 | 2,209 | **2,094** |

No rewrite costs zero, so none is narrowed.
