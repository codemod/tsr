# Lane `flow` notes (tsr-2zk.7)

Control-flow narrowing, definite assignment, truthiness and comparison overlap.
Baseline for every number below: `06f25e0` lane lists, measured on the box
branch head `0d996e8` with `diagverdictdump` (3,427 RIGHT diagnostics cases).

## 1. TS2367 asks the comparable relation

`crates/tsr-checker/src/comparison_overlap.rs` substituted assignability for
`isTypeComparableTo` and carried three declines to keep that substitution from
over-reporting: a union/intersection veto (`either_is_composite`), the enum veto
inside `pair_is_reportable`, and a syntactic "an earlier `if` in this block
already tested this name" test (`narrowed_away_by_an_earlier_test`).

**Forcing fact.** `Relation::Comparable` has existed since
`checker-notes-diag2.md` §750 and TS2352 already uses it. The substitution's
own premise (no comparable relation) had gone stale.

**What changed.** The rule now asks `relate_ternary(_, _, Comparable)` in both
directions, as `isTypeEqualityComparableTo` (`checker.go:12861`) is written, and
prints `getBaseTypesIfUnrelated`'s pair (`checker.go:12745`). The error-type
gate stays (`assignability_pair_is_reportable`: an unbuilt type is `any`
upstream). `Unknown` in either direction is still silence.

**Measured.** Corpus-wide TS2367/TS2678 lines: missing 141 → 72, extra 175 → 0.
Twelve cases converted, no verdict lost. Each decline was then removed one at a
time; removing the narrowing test changed **no** case's output (byte-identical
dump), so it was deleted rather than kept as dead insurance — the operands'
flow types already carry the narrowing it approximated.

**Would be wrong if** a future TS2367 extra traces to a comparable-relation
`NotRelated` that upstream answers `Related`. That is a relater bug (relate
lane), not a reason to restore a decline here.

**Remaining TS2367 misses** are type-parameter pairs
(`comparisonOperatorWithTypeParameter`, `…NoRelationshipTypeParameter`,
`compareTypeParameterConstrainedByLiteralToLiteral`): `T === U` with both
unconstrained. Upstream's comparable relation answers `False`; this port's
relater does not reach a confident `NotRelated` for the pair (the arm that
declines is not yet pinned down). Owned by `relater.rs` — reported, not changed.

**Perf.** `domain-model` 1.018 (9 samples). `generic-imports` read 1.045/1.065
at 21 samples, but the project contains no `==`/`!=`/`===`/`!==` at all, the
baseline binary against itself reads 1.027 in the same slot arrangement, and
the swapped arrangement (old in the `--tsr` slot, new in `--tsgo`) reads 0.998.
The excess is slot bias on this box, not the change.

## 2. TS2678 is `checkSwitchStatement`'s comparable arm, not a class test

`check.rs::check_switch_case_comparable` reported TS2678 only for a switch on
an intrinsic primitive with a `case` naming a class (§414), with an empty
source type in the message. Ported instead as written (`checker.go:4188`), in
`comparison_overlap.rs::check_switch_case_comparability`:
`!isTypeEqualityComparableTo(expr, case)` then `checkTypeComparableTo(case,
expr)`. Note the asymmetry with the equality operator: only a nullable *case*
type takes the flag disjunct.

**One decline, and why.** A fresh object-literal case type fails the
comparable relation in `hasExcessProperties` first, whose reporter emits
TS2353 on the property rather than TS2678 on the clause (`switchStatements`,
line 35). That reporter is the excess-property one, so the clause stays
silent here; the TS2353 stays missing. It would be wrong if a case with a
fresh literal *and* no excess property were expected to report TS2678 — none
in the corpus does at this commit.

**Measured.** TS2678 missing 47 → 0, extra 0 → 0; 17 cases converted.

## 3. TS2873's `undefined` arm compares against the seeded symbol

`getSyntacticTruthySemantics` reads `getResolvedSymbol(node) == c.undefinedSymbol`
(`checker.go:12907`). The port tested for *no* resolution, which stopped holding
once the binder seeded a global `undefined` (`declare_synthesised_globals`), so
every `undefined && x`, `undefined ? a : b` and `void 0 || void 0` was silent.
Now compares against `binder.undefined_symbol()`; a shadowing local still
resolves elsewhere and stays `Sometimes`. Seven cases converted, no losses.

## 4. TS2454 asks `getAssignmentTargetKind`, not "left of `=`"

`check.rs::is_definite_assignment_target` only recognised `x = …`. Upstream's
`checkIdentifier` returns before the flow section for any
`AssignmentKindDefinite` target (`checker.go:11109`), and `GetAssignmentTarget`
climbs array literals, spreads, parentheses and object-literal positions — so
`[...[a, b]] = …` and `for ([a] of …)` writes were reported as unassigned reads.
`expressions.rs` already ports the walk (`assignment_target_kind`); the rule now
asks it. TS2454 extras in the lane 23 → 1; twelve cases converted, no losses.
`is_write_only_access` (the `crate::unused` access-kind reuse) stays: it was the
earlier partial cover for the same gap and removing it is a separate question.

## 5. TS2774 / TS2801 / TS2845: `checkTestingKnownTruthyType` ported whole

`truthiness.rs` had a narrow stand-in for TS2774: an identifier condition of an
`if`, declared as a function or with a function-type annotation, whose `then`
branch does not mention the name. Ported instead as written
(`checker.go:3814`–`3953`): the three call sites (`if` → `then`, `?:` →
`whenTrue`, the left of `&&` always and of `||`/`??` inside an `if` chain), the
`||`/`??` left-operand walk, the enum-member arm (TS2845), call signatures *or*
a promised type (TS2801), `isSymbolUsedInBinaryExpressionChain` and
`isSymbolUsedInConditionBody` with its receiver-chain comparison.

**Declines (this port's, all toward silence).** A type whose call signatures
cannot be read, a promised-type lookup that hits an unported step (a `then`
with a non-`void` `this` parameter, whose subtype filter is not reproduced),
and a generic (`INSTANTIABLE`) type in that lookup all answer nothing.
`getSymbolAtLocation` is reduced to the two positions the rule compares
(property-access name → property of the receiver's apparent type; anything
else → its value resolution).

**Duplicate reports.** `if (a || b)` reaches `a` from both the `if` arm and the
`||` arm; upstream's diagnostic collection drops the identical second report.
`report` here does not deduplicate, so the rule checks for an identical
(file, span, code) before reporting.

**Perf (§5's ordering).** Every `if`, `?:` and `&&` reaches the rule. The first
version resolved call signatures and `then` before asking anything cheap and
read ~1.04 new/old on `domain-model` (swapped slots 0.96–0.965 old/new, i.e. a
real ~3–4%). An exact early-out — every constituent primitive ⇒ no signatures
and no promised type — moved ahead of those lookups brought four alternating
21-sample runs to 0.996 / 0.984 / 1.015 / 1.058 (mean ≈ 1.008).

**Measured.** TS2774/TS2801/TS2845 lines missing 92 → 18, extra 0 → 0; eight
cases converted. Left: `nanEquality`'s TS2845 is `checkNaNEquality` (operators
lane), and `truthinessCallExpressionCoercion2` line 116 reads
`window.console.error` through the DOM lib's `Window & typeof globalThis`.

## 6. TS2355 / TS2366 / TS2534 read end-of-body reachability

`check.rs::check_empty_body_returns_value` reported TS2355 for an *empty* body
only, because `functionHasImplicitReturn` was "reachability, this port's
standing refusal" (§440). That refusal is stale: `function_has_implicit_return`
(§743) reads the binder's end flow node through `is_reachable_flow_node`. The
whole of `checkAllCodePathsInNonVoidFunctionReturnOrThrow` (`checker.go:3728`)
is now ported as `flow.rs::check_all_code_paths_return_or_throw` and called for
function/method declarations, get accessors, function expressions and arrows.

**Not ported: TS7030 (`noImplicitReturns`).** The checker carries no
`no_implicit_returns` field and `apply_compiler_options` lives in the hub
`checker.rs`; adding one is reported to the integrator rather than made here.
That arm is 26 missing lines in six lane cases (`noImplicitReturnsExclusions`,
`noImplicitReturnsInAsync2`, `noImplicitReturnsWithoutReturnExpression`,
`reachabilityChecks5/6/7`).

**Declines.** JavaScript, generators (iteration return type), unannotated get
accessors (`getTypeOfAccessors` infers from the body), and error types. An
unannotated function returns *before* the reachability query: only TS7030 can
speak there, and the query types `never`-returning calls in the body, which
re-entered the function's own inferred return type and reported TS7023 on
`thisTypeInObjectLiterals2` in the first measurement (one EMPTY_RIGHT loss,
fixed before commit).

**Measured.** TS2355/2366/2534 lines missing 23 → 4, extra 2 → 0; seven cases
converted, no losses. Perf on this box has a slot bias of several percent that
flips sign between projects, so each project was run both ways (21 samples):
`domain-model` 0.955 / 1.052 direct and 1.051 / 0.946 swapped-inverted,
`generic-imports` 1.068 / 1.070 direct and 0.966 / 0.969 swapped-inverted —
geometric means ≈ 1.00 and ≈ 1.017.

## 7. TS2448/TS2449: `isUsedInFunctionOrInstanceProperty` and the class arm

`check.rs::use_is_not_deferred` deferred a use at **any** property declaration,
computed property name or decorator ancestor. Upstream
(`isUsedInFunctionOrInstanceProperty`, `checker.go:2011`) defers only a
property's *instance* initializer (for the variable/class/enum declarations
this rule asks about), a non-IIFE function, a static block whose declaration
precedes the use, and a decorator only when its decorated method or
parameter's function is itself deferred. Ported as written, so
`static p = After.x` before `class After` is TS2449
(`scopeCheckStaticInitializer`).

The **class arm** (`checker.go:1955`) was missing entirely: a use *after* the
class starts is still illegal inside its own computed property names, and —
without `experimentalDecorators` — inside decorators on the class, its members
or their parameters, unless a non-IIFE function intervenes. Added before the
deferral gate, as upstream asks it.

**Two declines.**
- `isInAmbientOrTypeNode(usage)`: a use inside `declare class C { [k]: … }`
  is never evaluated (`forwardRefInTypeDeclaration`, which the first
  measurement lost).
- A use under a **parameter** of the function whose body declares the name.
  Upstream's `useOuterVariableScopeInParameter` (in `resolveNameHelper`)
  resolves such a name to the *outer* scope for ES2015+ targets; this port's
  binder resolves the body's declaration, so the position comparison would be
  about the wrong symbol (`parameterInitializersForwardReferencing1_es6`'s
  `function f7({[foo]: bar}) { let foo … }`, lost in the first measurement).
  Silence until the resolver is fixed; reported to the names lane.

**Measured.** Two lane cases converted (`classDeclarationShouldBeOutOfScopeInComputedNames`,
`useBeforeDeclaration_classDecorators.1`), 40+ new correct lines in still-WRONG
cases (`decoratorUsedBeforeDeclaration`, `scopeCheckStaticInitializer`,
`computedPropertyNamesWithStaticProperty`, …), no losses, no new extras.
Class-expression self-references (`(class C2 { [C2.p]() {} })`) stay missing.
Perf bias-corrected ≈1.021 (`domain-model`) and ≈1.010 (`generic-imports`).

## 8. TS2448: the binding-element and variable-declaration arms

The rule only knew `VariableDeclaration` declarations and "inside its own
initializer". Upstream's `checkResolvedBlockScopedVariable` picks any
block-scoped declaration, a `BindingElement` included, and
`isBlockScopedNameDeclaredBeforeUse` has two arms for a use positioned after
the declaration starts (`checker.go:1937`): a binding element is illegal when
the use is in the same binding element (`let {[a]: a}`, `const { f = f }`) and
otherwise defers to its `VariableDeclaration` (`let [x1] = x1`); a variable
declaration is illegal when the use is inside it or in its `for-in`/`for-of`
expression (`for (let v of v)`) — `isImmediatelyUsedInInitializerOfBlockScopedVariable`
over `isSameScopeDescendentOf` (IIFEs see through, async generators do not).
Ported as `variable_declared_before_use`; the old `reference_is_in_own_initializer`
is removed. The deferral gate that precedes the arms is unchanged.

**Measured.** Five lane cases converted (`blockScopedBindingUsedBeforeDef`,
`recursiveLetConst`, `tryCatchFinallyControlFlow`,
`awaitUsingDeclarationsInForAwaitOf.2`, `destructuringObjectBindingPatternAndAssignment4`),
19 new lines all expected, none unexpected, no losses. Perf bias-corrected
≈1.011 / ≈1.006.

## 9. Round 2: the flow walk's tail and its assignment sources (tsr-2zk.40)

Baseline for §9–: branch head `15f1743`, 3,846 RIGHT diagnostics cases,
467,200 RIGHT type lines.

**`flow.go:111`, the `x!` arm.** `getFlowTypeOfReferenceEx` answers the
declared type when the reference is the operand of a non-null assertion and the
flow type, not itself `never`, is only `null`/`undefined`. The port had the
`unreachableNeverType` half only (three copies, one per entry point); both are
now `flow_result_or_declared`, called by all three. `typeGuardsAsAssertions`
read `x!` as `never` after `x = undefined` (15 type lines). The property lane's
`never`-receiver decline for an `x!` operand (`property.md` §3) no longer has
a producer; removing it is that lane's change.

**`getInitialOrAssignedType`'s missing arms.** The assignment source returned
`None` (keep the declared type) for every form but `x = e` and an initialised
declaration. Ported: `for..in` → `string`, `for..of` →
`checkRightHandSideOfForOf` (this port's `for_of_statement_element_type`,
`for await` included), `delete x.p` → `undefined`, for both an uninitialised
`for` head declaration and an expression target. The destructuring arms
(`getAssignedTypeOfArrayLiteralElement`, `…PropertyAssignment`,
`…ShorthandPropertyAssignment`, `…SpreadExpression`, binding elements) remain
`None`. That is upstream's answer whenever its projection is `errorType`
(the reduced type of `errorType` keeps every declared constituent), so the
gap is a missing narrowing and never a wrong one.

**`IsStringLiteralLike`.** `typeof x === \`string\`` compared only a
`StringLiteral`. The `in` arm read only a written string literal, where
upstream types the left operand (`getTypeOfExpression`) and asks
`isTypeUsableAsPropertyName`, so a template literal or a `const` key did not
narrow. Both now follow upstream; the `in` key goes through
`property_name_from_index`, which declines unique symbols (no narrowing,
upstream narrows), a known gap.

**Measured.** 0 diagnostic / 0 type losses; +36 RIGHT type lines
(`typeGuardsAsAssertions`, `controlFlowWithTemplateLiterals`,
`controlFlowForOfStatement`, `controlFlowInOperator`,
`controlFlowForInStatement2`, `controlFlowDeleteOperator`,
`assignmentTypeNarrowing`); `controlFlowWithTemplateLiterals` diagnostics
EMPTY_WRONG → EMPTY_RIGHT. Median CPU new/old (21 samples):
`domain-model` 0.960, `generic-imports` 0.989.

**Union-receiver decline (property lane).** With
`nonexistent_property.rs`'s union-receiver decline switched off as an
experiment, the corpus gained 61 expected TS2339/TS2551 lines and 18 false
ones. The false ones are flow gaps, one per narrowing shape:
`controlFlowWithTemplateLiterals` (fixed above), `controlFlowForOfStatement`
(fixed above), `typeGuardNarrowBy[Mutable]UntypedField` (predicate to a mapped
type over `ArrayLike | Iterable`), `controlFlowWithIncompleteTypes` (loop with
`typeof`), `inKeywordAndIntersection` (`instanceof` an intersection
constructor), `controlFlowAliasing` (aliased conditions through a destructured
`const`), `discriminatedUnionTypes3/4`, `returnTagTypeGuard` (JSDoc `@return`
predicate), `templateLiteralTypes3`, `typeGuardsWithInstanceOfByConstructorSignature`.

## 10. TS7030: `checkAllCodePathsInNonVoidFunctionReturnOrThrow`'s last arm

§6 left the `noImplicitReturns` arm unported for want of an options field.
`Checker::no_implicit_returns` (`checker.rs`, `NoImplicitReturns == TSTrue`,
no `strict` fallback — upstream compares with `core.TSTrue`) now carries it,
and the arm is ported in `flow.rs::check_all_code_paths_return_or_throw`:
an annotated type that survives the first three arms reports TS7030; an
unannotated function reports only when its body has an explicit `return` and
its inferred return type, unwrapped for `async`, is not `void`/`undefined`/
any-like. An inferred type this port cannot compute is upstream's `errorType`
(any-like), so it is silent.

**One reorder.** Upstream asks `functionHasImplicitReturn` before
`hasExplicitReturn`. Here the unannotated, no-`return` case returns first: the
reachability query types `never`-returning calls and can re-enter the
function's inferred return type (§6's `thisTypeInObjectLiterals2` TS7023).
No answer changes; only which functions pay for the query.

**Error node.** The return annotation, else the function's error span
(its name, or the node's start for an anonymous function or arrow).
`FullSignature` (a JSDoc `@type` on the function) is not modelled.

**Harness gap (not owned).** `crates/tsr-conformance/src/trace_case.rs` maps
`@noImplicitReturns` to nothing, so the corpus never turns the option on.
Measured with that one line added locally (not committed): 24 TS7030 lines,
0 extra, 0 losses; `noImplicitReturnsExclusions`, `noImplicitReturnsInAsync2`,
`reachabilityChecks5/6/7` convert. `noImplicitReturnsWithoutReturnExpression`
also needs `checkReturnStatement`'s `return;` arm (`checker.go:4123`,
non-strict only) in `assignreport.rs::check_return_statement` — reported.
Perf (option off on both bench projects): 1.025 / 0.993 CPU median.

## 11. TS2454: §839.1's "unported narrowing" guard removed

`uninitialized_variable_reads_declared` declined any reference under a
condition that named it and contained a call, an `instanceof` or a
`.constructor` access (`reference_is_guarded_by_a_condition_on`,
`checker-notes-deferred.md` §839.1), because those narrowings were unported
and left `undefined` in the flow type. All three are ported in `crate::flow`
now (type predicates, `narrowTypeByInstanceof`, `narrowTypeByConstructor`),
and the guard had already been weakened to "only when the walk made no
progress". With it removed, the false branch of `isFoo(value)` keeps
`undefined` exactly as upstream's does and TS2454 reports there.

**Measured.** 0 diagnostic / 0 type losses (the type road shares the
predicate; the 67 type losses §839.1 recorded no longer occur); +12 TS2454
lines, 0 extra; `narrowTypeByInstanceof`, `typeGuardNarrowsPrimitiveIntersection`,
`typeGuardNarrowsToLiteralType`, `typeGuardNarrowsToLiteralTypeUnion`
convert, `typeGuardOfFormInstanceOf` +6 lines. The guard's helpers
(`subtree_has_unported_narrowing`, `subtree_mentions`) had no other caller and
are deleted. CPU median: 1.001 / 0.988.

**Falsifier.** A new TS2454 extra under a guard naming the variable would
mean a narrowing arm over-keeps `undefined`; fix the arm, do not restore the
guard.

## 12. `getControlFlowContainer` skips immediately invoked functions

`check.rs::control_flow_container` stopped at the first enclosing function.
Upstream's predicate (`checker.go:11438`) is `IsFunctionLike(n) &&
GetImmediatelyInvokedFunctionExpression(n) == nil`: an IIFE's body belongs to
its caller's flow (the binder already threads it so). The port now skips a
function expression or arrow that `immediately_invoked_call` names. Every
caller (TS2454's `isOuterVariable`, the flow walk's container bound, the
property-initialisation and readonly-target readers) asks upstream's
question, so the change is made in the helper rather than at one caller.

**Measured.** `typeGuardsInFunction` converts (TS2454 on a variable of the
enclosing function read inside `function () { … } (param)` and
`((p) => { … })(param)`); 0 diagnostic / 0 type losses, no other output
changes. CPU median: `domain-model` 1.020, `generic-imports` 1.030 at 21
samples and 1.020 at 41.

## 13. TS2454 for initialised and destructured declarations

Baseline from here: the merge `210b098` (3,881 RIGHT diagnostics cases).

`uninitialized_variable_reads_declared` required a plain `VariableDeclaration`
with an annotation and **no initialiser** — §8's bound. Upstream has neither
restriction: `checkIdentifier` starts every strict, not-assumed-initialised
read at `getOptionalType(t)` and reports when `undefined` survives, which for
an initialised `var` happens when a path bypasses the initialiser (a `catch`
after a throwing call, a read before a hoisted `var`'s line). Ported:

- an initialiser no longer exits; only "no annotation *and* no initialiser"
  does, which is upstream's auto-typed road (a different diagnostic);
- a `BindingElement` is checked through its root `VariableDeclaration`
  (`GetRootDeclaration`), with `isSameScopedBindingElement`
  (`checker.go:11202`) and the element's own `isNeverInitialized == false`;
- `isNeverInitialized` now also requires no initialiser, as written, so an
  initialised outer variable is assumed initialised.

**Fast path (exact).** Walking every read of every initialised local cost
1.056 CPU on `domain-model` (41 samples). For an initialised `let`/`const`/
`using` read textually after its declaration and not in a `case` clause,
every flow path to the read passes the initialiser (a block's statements run
in order, loop back-edges re-enter after the head, exceptions leave the
scope); only `switch` can enter a block past a statement. Those reads return
before the walk. Output byte-identical with and without it; CPU median after:
`domain-model` 0.974, `generic-imports` 1.003 (41 samples).

**Measured.** +22 TS2454 lines, 0 extra, 0 diagnostic / 0 type losses;
`controlFlowDestructuringVariablesInTryCatch`, `useBeforeDeclaration_destructuring`,
`classStaticBlockUseBeforeDef3`, `parserS7.2_A1.5_T2`, `scannerS7.2_A1.5_T2`,
`parserUnicode1` convert; lines gained in `controlFlowFunctionLikeCircular1`,
`decoratorUsedBeforeDeclaration`, `controlFlowAliasing`, `exportBinding`.

**Falsifier.** A TS2454 extra on an initialised declaration means a path the
port's flow graph has that upstream's does not (a binder edge), not that the
bound should return.

## 14. TS2454: `for..in`/`for..of` heads are not assumed initialised

The rule returned early for any `for (… of/in …)` head ("assigns on entry").
Upstream only removes such a head from `isNeverInitialized`
(`!ast.IsForInOrOfStatement(immediateDeclaration.Parent.Parent)`,
`checker.go:11147`); `assumeInitialized` is otherwise unchanged, so a read of
a hoisted `var v` *before* `for (var v of …)` keeps `undefined` and reports.
Ported: the head is not auto-typed (it has the iterated type), is not a
`const`-without-initialiser, and is excluded from `isNeverInitialized` only.
§13's fast path extends to block-scoped heads for reads past the iterated
expression (the body runs only after the head's assignment).

**Measured.** `for-of8`, `for-of22` convert; 0 losses against `210b098`.
CPU median (41 samples): 1.001 / 1.006.

## 15. TS2454: catch variables, and `isNeverInitialized`'s definite assignment

Three upstream facts, one commit because each alone moved the same lines:

- **Catch-clause variables** are typed `any`, `unknown` or `errorType`
  whatever their annotation (`checker.go:16678`), all of which
  `assumeInitialized` accepts. The rule read `catch (e: number)`'s annotation
  as `number` and reported (`catchClauseWithTypeAnnotation`, one extra). Now
  a catch-clause root declaration exits.
- **The START arm's outer-variable stand-in** for `assumeInitialized` used
  "the symbol has *any* assignment". Upstream's `isNeverInitialized` asks
  for a *definite* one, so an outer `let i: number` touched only by `i++`
  keeps `undefined` (`unusedLocalsInMethod4`'s `rw`/`createBinder`). The arm
  now also requires `!is_never_initialized` (new `flow.rs` helper, the
  `checker.go:11147` predicate).
- **The definite-assignment record** (`mark_node_assignments`) was gated by
  `is_parameter_or_mutable_local_variable`, which refuses every file-level
  `let` (a known approximation the `.types` reader depends on, §42 of
  `checker-notes-diag2.md`). With the START change that turned module-level
  `let x2; x2 = "abc"` into "never initialised" and produced 4 false TS2454 in
  `narrowingPastLastAssignmentInModule`. The definite flag is now gated by
  upstream's predicate as written (`is_parameter_or_mutable_local_variable_faithful`);
  the position record keeps the approximate gate, so the `.types` reader is
  unchanged (0 type-line changes).

**Measured.** `unusedLocalsInMethod4` converts (+4), the catch extra is gone;
0 diagnostic / 0 type losses against `210b098`. CPU median (41): 1.003 / 1.005.

## 16. Recovery: semantic bigint zero and unique-symbol facts (tsr-2zk.15.5.1)

Recovery base: `0e7824dd`. Native:
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
`getTypeFactsWorker` / `isZeroBigInt` in `internal/checker/checker.go`.
The saved `box/parity-printing` ref was initially absent; after the parent
published it, an explicit fetch recovered `origin/box/parity-printing` at
`531e3c22`. Its `flow.rs` and this note are byte-identical to recovery base
`0e7824dd`; the latest flow commit on both is `49663469`. There are no owned
flow changes to cherry-pick. Snapshot `531e3c22` changes only parent-owned
`checker.rs` and symbol-chain tests and was not replayed or treated as verified.
No snapshot or old branch was merged. Only `flow.rs` and dedicated flow-facts
tests changed.
Protected narrowing, configuration and unary-expression work is unchanged.

`normalise_bigint` stores canonical decimal digits with no `n` suffix and no
negative zero. Comparing its payload with `0n` classified semantic zero as
truthy. Compare with `"0"`, corresponding to native `jsnum.PseudoBigInt{}`.
The symbol branch now uses `ES_SYMBOL_LIKE`, including unique symbols, rather
than `ES_SYMBOL`. Both bigint and symbol domains select native loose-mode
nullish/falsy facts as well as strict facts. No new cache, mapper or traversal;
these branches inspect existing checker-owned type payloads without allocation.

### Fresh evidence, not interrupted-run credit

Receipts: repository-ignored `target/recovery/flow/`. Both unfiltered corpus
runs finished with exit 0. `identities.sha256` records source, binary and TSV
identities. Baseline and candidate binaries were retained separately before the
paired runs; the isolated Box had no unrelated source changes.

- Direct pinned-tsgo strict diagnostic controls: exit 0, no diagnostics.
  TSR checkpoint: two TS2322 errors; candidate: exit 0, no diagnostics.
- Native declaration controls distinguish zero/radix-zero/nonzero bigint,
  unique-symbol `typeof`, and loose versus strict symbol truthiness.
  TSR CLI declaration emission produced no declaration file; its invocation is
  not counted as declaration proof. Permanent tests check reference types.
- Five dedicated flow-facts tests pass. Existing narrowing (71), lazy recursive
  returns (19), logical operators (9), and type predicates (7) tests pass.
- Diagnostics: 10,570 eligible case keys; 9,190 RIGHT/EMPTY_RIGHT before and
  after. Zero RIGHT losses, vanished keys, or new keys. One previously WRONG
  case changes: `numberVsBigIntOperations` loses its extra TS2345 at line 94.
- Types: 477,968 keyed rows; RIGHT 469,783 → 469,790, WRONG 7,190 → 7,183,
  GAP 995 unchanged. Zero RIGHT losses or vanished/new keys. Six unique-symbol
  logical-OR rows and one bigint truthiness row become RIGHT.
  The dump also prints auxiliary rows; keyed comparison excludes those.
- Anchors: 4,499 upstream references, zero unresolved. Sections: 16,699
  citations, zero dangling. Dedicated test formatting passes.

These harnesses exclude configuration-varied and known-divergence cases;
`diagverdictdump` compares diagnostic file/line/column/code, not exact message,
span length or emission order. The 99.9% full-configuration exact gate is **not
met or certified**. Historical accepted RIGHT receipts were absent, so
preservation is proved only against this fresh checkpoint, not vanished
historical keys.

### Performance gate remains unmet

Fresh-process, alternating order, 21 measured samples after one warmup per
binary/project; builds and corpus runs excluded from timing. Complete printed
diagnostic outputs agree among checkpoint/candidate/native on both projects.

| Project | Checkpoint median s | Candidate median s | Native median s | Candidate/checkpoint |
| --- | ---: | ---: | ---: | ---: |
| domain-model | 0.2179592 | 0.2143598 | 0.2142039 | 0.9835 |
| generic-imports | 0.1144116 | 0.1227134 | 0.1235722 | 1.0726 |

The generic-imports wall result is a measured slowdown, not a verified hotpath
regression attribution. No no-slowdown claim is made. Candidate/native ratios
are 1.0007 and 0.9931, respectively, **unverified for equivalent complete
work**, and do not meet 0.50 even as raw timings. The native extended diagnostic
output lacks a checked-file count; complete checked-scope and input equivalence
were not established. Parent performance-owner investigation remains required.

### Effects publication prerequisite (tsr-1yb.11.3)

Pinned `getEffectsSignature` (`internal/checker/flow.go`) reads/writes only
`signatureLinks.effectsSignature`. Native publishes a selected effect or
`unknownSignature` only after resolution. There is no active/provisional
publication by this getter. A recursion guard cannot be promoted to completed
unknown. Statement callees use explicit dotted-name typing to avoid flow
circularities; other callees retain their original optional/non-null receiver
context. Do not share by callee symbol, type or printed name: overload choice,
ordered arguments and written aliases belong to the original call node.

Exact serialized integration-owner state request (not applied here):

```rust
pub(crate) effects_signatures:
    FxHashMap<NodeId, Option<crate::signatures::Signature>>,
// Checker constructor:
effects_signatures: FxHashMap::default(),
```

Owner/lifetime: private Checker, original call `NodeId`, fixed checker options.
Absent = uncomputed; present `Some` = completed effect; present `None` = completed
unknown. Existing eager return completion and the no-effects preflight must not
publish an unsupported/provisional result. Reuse must bound the existing dotted
callee/signature-resolution worker, not add a parallel cache. Query/hit/worker
counts are not measured; preserve the existing `tsr-1yb.11.3` boundary issue.

`checker.rs` is parent-owned and unchanged. No agreed field/API arrived on this
Box, so publication was not implemented against a guessed owner or substitute
map. `bd prime` succeeded, but `bd show`/`bd history` could not find
`tsr-2zk.15.5.1`; `tsr-1yb.11.3` is also absent in this Box's database. No duplicate
issue was created and neither issue was closed. Parent must update the existing
issues in its authoritative database. `binary.rs` also still creates a `0n`
payload in its logical-bigint path; it is outside this worker's ownership and
was reported for serialized integration rather than silently changed.

## 17. Template literal and loose scalar facts: native worker domains

Template truthiness is tracked by existing root `tsr-2zk.16.525`, distinct from
semantic bigint-zero / symbol-like recovery `tsr-2zk.15.5.1`. Native pin
remains `5b1047d10d32e7d5b446be4de56b126ff42f82bb`:
`getTypeFactsWorker` in `internal/checker/checker.go` tests
`String | StringMapping` first (broad string facts), then
`StringLiteral | TemplateLiteral`. Its `isEmpty` is true only for an empty
**string literal**. Every represented template literal therefore has
`NonEmptyStringStrictFacts` in strict mode; no analysis of prefix/substitution
contents or unary-consumer special case is involved.

All four scalar families use native loose `Base*Facts`: add `Falsy`,
`EQUndefined`, `EQNull`, and `EQUndefinedOrNull`, while retaining the strict
facts. Previously this was limited to bigint and partially to enum literals.
The worker now applies these bits uniformly after selecting the scalar domain.
String mapping retains broad string truthiness. No new helper, allocation,
cache, traversal or shared interface. Only the exclusive flow worker and its
dedicated tests changed; user flow and parent unary-consumer work are preserved.

Receipts: ignored `target/recovery/flow-template/`, relative to scalar-recovery
binary retained from `7d47ec72` (source unchanged by subsequent docs commit
`f91738cf`). Native strict declarations yield `never` for falsy branches of
`prefix${string}`, `${number}`, nonempty string, `1`, and `true`; loose mode
retains each original type. `Uppercase<string>` survives both modes. Two
new dedicated controls failed before and pass after. Direct CLI template
controls: native/candidate exit 0 with no diagnostics, baseline emits two
TS2322 errors. All 113 focused tests pass, including prior flow/effects suites.

Both unfiltered eligible-corpus runs completed with exit 0:

- Diagnostics: 10,570 keys, byte-identical rows, zero RIGHT losses or vanished
  keys.
- Types: 477,968 keys, RIGHT 469,790 → 469,797, WRONG 7,183 → 7,176, GAP 995
  unchanged; zero RIGHT losses, new keys, or vanished keys. Seven gained rows:
  four `stringLiteralTypesInUnionTypes04`, two `templateLiteralTypesPatterns`,
  one `logicalOrOperatorWithEveryType`.
- Baseline keys include every RIGHT gained by the scalar recovery. Historical
  parent RIGHT receipts and full configurations remain the parent's gate;
  eligibility and exact-diagnostic limitations from section 16 still apply.

Fresh-process whole-project timing, 21 alternating measured samples after
warmup, corpus/build work excluded: domain-model candidate/baseline median
1.0191 (0.2244103s / 0.2201963s), generic-imports 1.0004 (0.1001260s /
0.1000871s). Printed complete diagnostics agree among baseline/candidate/native.
No speed or no-slowdown claim: raw candidate/native is 1.0322 / 0.9331,
equivalent complete work is unverified, and the release 0.50 gate is unmet.
The saved recovery branch has no additional owned flow changes to replay.

Reviewer minimal control, run directly against the pin:

```typescript
declare const value: `${number}`;
export const result = !value;
```

Native strict declaration: `export declare const result = false;`.
Native loose declaration: `export declare const result: boolean;`.
Receipts: `flow-template/reviewer-strict` and `reviewer-loose` under the ignored
recovery directory. Native `computeBaseConstraint`'s template branch rebuilds
from substitution constraints; the numeric substitution preserves the template
rather than widening it to broad string. No base-constraint or unary-consumer
change was made by this worker. Parent owns exact unary-consumer acceptance.
`bd show tsr-2zk.16.525` also reports absent in this Box database; parent must
update the existing root, not create another task. Commit `42b7f878` carries the
older campaign attribution because the exact root arrived afterward.
