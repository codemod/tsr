# Parity box notes: destructure-iteration (tsr-2zk.10)

Lane `tsr-2zk.10` — destructuring, iteration, tuples, spreads, array literals —
plus the routed `types-triage.md` clusters (tsr-2zk.16.x). Upstream is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/checker.go`. This page
records the judgment calls the box made and the boundaries it could not cross
from the files it owns.

## 1. `checkYieldExpression`'s annotated arm (tsr-2zk.16.3, GENERATOR-ANNOTATION-ITERATION-TYPES)

**Forcing constraint.** Upstream answers a non-star `yield` in an annotated
generator with `getIterationTypeOfGeneratorFunctionReturnType(Next,
returnType)` orElse `anyType` (`checker.go:10982-11004`), after filtering a
union annotation by `checkGeneratorInstantiationAssignabilityToReturnType`
(`checker.go:29697`). TSR read the third written type argument (or the third
type parameter's default) off the annotation node (§225,
`next_type_of_annotated_generator`), which is a syntactic stand-in for the fast
path's resolved type arguments. It could not see a structural iterator
(`{ next(...args: [] | [number | PromiseLike<number>]): any }` in
`types.asyncGenerators.es2018.1`), an alias, or a union.

**What was ported** (`crates/tsr-checker/src/iteration.rs`):
`getIterationTypesOfGeneratorFunctionReturnType` (iterable with the
`GeneratorReturnType`/`AsyncGeneratorReturnType` uses, then the type read as an
iterator), `getIterationTypeOfGeneratorFunctionReturnType`,
`createGeneratorType` (with the `IterableIterator` and empty-object fallbacks)
and `checkGeneratorInstantiationAssignabilityToReturnType` as a predicate.
`check_yield_expression` calls them on `getTypeFromTypeNode(annotation)`.
`next_type_of_annotated_generator` (declared.rs) lost its only caller and was
deleted.

**Undecided stays a gap.** Every step keeps the engine's
`Err(Unsupported)`: an undecidable relation in the union filter, or a
protocol the engine cannot decide, answers `error`, never `any`. Only a
completed query falls to upstream's `anyType`.

**Known boundary — inherited `Iterator` members.** The global-method arm of
`getIterationTypesOfMethod` (`checker.go:6585`), which reads `mapper.Map` of
`Generator`/`Iterator`'s type parameters when the method comes only from that
global, is not ported. It cannot be reached yet: `get_property_of_type`
(`members.rs`) answers no symbol for a member inherited through a type-argument
heritage entry (`interface I1 extends Iterator<0, 1, 2>`), and
`declared_property_table` (`member_completeness.rs`) declines the same receiver,
so even `[Symbol.iterator]`'s absence is not established and the iterable query
declines first. `generatorReturnTypeIndirectReferenceToGlobalType` therefore
still gaps. An attempt that walked the receiver's heritage to the global
reference was written and removed: with no method symbol the arm's identity
test never fires, so it was dead code. Falsifier: once `get_property_of_type`
returns the base's symbol for generic heritage, that case should still gap
here, and porting the arm is then the fix.

## 2. `isLegalUsageOfSuperExpression`'s call arm (tsr-2zk.16.53, SUPER-CALL-CONSTRUCTOR-ONLY)

A `super(...)` is legal only when `getSuperContainer(node, true)` is a
constructor (`checker.go:7867`). `check_super_expression` accepted a super call
in any member. Now the first member the container walk settles on decides: a
call whose member is not a `Constructor` answers the deliberate error-any (the
same convention as the existing arrow arm: upstream's `errorType` after
TS2337, printed `any`). A container the walk does not model (a class static
block, a plain function) keeps its existing answer.

## 3. `AccessFlagsAllowMissing` for defaulted binding elements (tsr-2zk.16.35, BINDING-ELEMENT-ALLOW-MISSING-DEFAULT)

`getBindingElementTypeFromParentType` indexes an object pattern's parent with
`AccessFlagsAllowMissing` when the element has a default (`checker.go:17736`),
and `getPropertyTypeForIndexType` answers `undefined` for that flag when the
(apparent) object type is an object-literal type and neither a property nor an
index signature matched (`checker.go:27187`). The default then supplies the
element's type through the existing default union. `destructuring_property_lookup`
takes the flag and applies the same test with `is_object_literal_type`; a
non-literal parent (an interface, a primitive's apparent type) keeps the miss,
as upstream does. The arm sits after the index-signature lookup, the same
order as upstream, so a literal with a matching index signature is unaffected.

## 4. Pattern-implied type of a declaration with no source (tsr-2zk.16.47, BINDING-PATTERN-IMPLIED-TYPE, partial)

`getTypeForVariableLikeDeclaration` ends with `getTypeFromBindingPattern(name,
false, true)` for any pattern-named declaration that reached no annotation,
initializer, catch, for-in or for-of arm (`checker.go:16790`), and
`getTypeForBindingElementParent` reads it unwidened. For a
`VariableDeclaration` (`declare var [a, b];`), `get_type_for_binding_element_parent`
answered `error`; it now returns `binding_pattern_implied_type`, and a pattern
the builder cannot spell still gaps. The for-in arm (`checker.go:16658`) was
added ahead of it in the same function so a (grammatically invalid) for-in
pattern reads the key type rather than the implied type, as upstream orders
them.

**Not done here (files not owned).** The cluster's other two legs live in
`symbols.rs` `get_widened_type_for_variable_like_declaration`: the rest
parameter exclusion (`...{ a, b }` must take the implied type before the
`any[]` fallback, `restParameterWithBindingPattern1`) and the container gate
that excludes bodyless signatures (`FunctionType`, `ConstructorType`,
`MethodSignature`, call/construct signatures — `renamingDestructuredPropertyInFunctionType`).
The parameter gate in `destructure.rs` mirrors that gate and must move with it,
or the element and the declaration would disagree.

## 5. Computed binding names through `getIndexedAccessTypeEx` (tsr-2zk.16.43, BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS)

Upstream indexes every non-rest object element with
`getIndexedAccessTypeEx(parent, getLiteralTypeFromPropertyName(name), ExpressionPosition | AllowMissing?)`
(`checker.go:17741`); a computed name's literal type is its checked regular
type. TSR read a computed key only against the parent's applicable index
signature. Now:

- a literal key (`["a"]`, `[2]`) names a property exactly as `{ a: x }` does
  (`destructuring_property_lookup`, defaults or not);
- a generic key defers through `resolved_indexed_access_type` (`T[K1]`,
  `{ [_ in T]: number }[T]`). Only the INDEX decides deferral: in expression
  position a generic object with a concrete key reads its apparent type, which
  is why a generic parent with a literal key does not go this way;
- any other key reads the applicable index signature, and under
  AllowMissing an object-literal parent misses to `undefined`.
  The index-signature result no longer returns early: it takes the same default
  strip/union as every other element, as upstream does.

`getRestType` (`checker.go:17792`) collects the same key types for the
`Omit` road: a generic computed key (`isGenericIndexType(omitKeyType)`) mints
`Omit<source, K1 | K2>` even over a concrete source, and unique-symbol keys
enter the omitted union as themselves (`Omit<T, unique symbol | unique symbol>`).

**Boundary — unique-symbol keys against a concrete parent.** TSR names a
late-bound member by its source text (`[Key]`, `indexed.rs`'s element-access
arm), not by the unique symbol's identity (`__@Key@id`). Matching a binding
key to that name by text would be a syntactic guess, so a unique-symbol key
against a concrete parent, and a concrete rest beside one, still gap
(`genericObjectRest` 0:42/0:44, `declarationEmitComputedNameCausesImportToBePainted`).

## 6. `getContextualReturnType` in full (tsr-2zk.16.3: CONTEXTUAL-RETURN-GENERATOR-FILTER, CONTEXTUAL-RETURN-IIFE-ARM)

**Forcing constraint.** Upstream answers every generator- and return-position
context through one function, `getContextualReturnType` (`checker.go:29665`):
the written annotation, else the contextual signature's return type —
filtered for a generator by `AnyOrUnknown | Void | InstantiableNonPrimitive`
or `checkGeneratorInstantiationAssignabilityToReturnType`, for an async
function by `getAwaitedTypeOfPromise` — else, for an immediately invoked
function, the contextual type of the call. TSR had three partial copies
(the `ReturnStatement` arm, the arrow concise-body arm, and
`contextual_type_for_yield_operand`), each reading the iteration slots off a
single `Generator`-family type reference (`contextual_generator_iteration_type`),
so a union context (`number | Generator<…>`), a structural iterator, and every
IIFE had no context.

**What was ported** (`crates/tsr-checker/src/contextual.rs`):

- `get_contextual_return_type` — `Ok(None)` is upstream's nil, `Err` a lookup
  the port cannot finish. Only function expressions, arrows and object-literal
  methods ask for a contextual signature
  (`getContextualSignatureForFunctionLikeDeclaration`); an IIFE is a callee,
  which `getContextualType` answers nil for, so its signature is decidably
  absent and the IIFE arm reads `get_contextual_type(call)`. A `None` there
  is nil only when `has_no_contextual_type(call)` proves it.
- `get_contextual_iteration_type` (`checker.go:29656`) on the iteration-types
  engine of §1.
- `getContextualTypeForReturnExpression`'s generator arm: a union is narrowed to
  the constituents with a RETURN iteration type before that slot is read.
- `getContextualTypeForYieldOperand` (`checker.go:29719`): the non-star union
  filter and YIELD slot; the star arm mints `Generator` (and `AsyncGenerator`)
  through `create_generator_type`.
- `check_yield_expression`'s unannotated arm is
  `getContextualIterationType(Next, fn)` orElse `any`; an unfinished lookup
  still answers `error` unless §224's predicate proves the container
  uncontextualised.

The annotation arm keeps reading the four function kinds' written return
type. `getReturnTypeFromAnnotation`'s constructor and setter-paired getter arms
are not ported here: neither container holds a `yield`, and a `return` in
either kept its previous answer.

**Two bounded deviations, both kept loss-free and both with a falsifier.**

1. *Undecidable generator filter on a single type keeps the type.* TSR's
   relater answers `Unknown` for `AsyncGenerator<Awaited<T>, Awaited<R>, any>`
   against `AsyncGenerator<T, R>` (the filter's instantiation check under a
   generic contextual signature, `typeParameterConstModifiersReturnsAndYields`
   `test4`). Treating that as a gap removed the yield's context and widened the
   const-inferred `[10, "1"]` to `[number, string]` (3 RIGHT lines lost). On a
   non-union, an undecided filter now keeps the type — the answer the lookup
   gave before the filter existed; on a union it is a gap. Falsifier: once the
   relater decides that relation, the fallback is unreachable and is deleted.
2. *Async contextual slots drop the `Awaited<…>` wrapper.* The async resolver
   awaits a generic slot to `Awaited<T>`, exactly as upstream does, but
   `is_const_type_variable` (`assertions.rs`) does not see through the
   conditional's constraint as upstream's `isConstTypeVariable`
   (`checker.go:13656`) does, so the const context was lost (same 3 lines).
   `unwrap_contextual_awaited_slot` returns the unwrapped type at the yield
   operand's YIELD slot and the return expression's RETURN slot of an async
   generator — what the pre-port `getAwaitedTypeNoAlias` read gave. Falsifier:
   when `is_const_type_variable(Awaited<T>)` answers `true` for a const `T`,
   deleting the unwrap must lose nothing.

**Not done here (file not owned): `return_type_from_body`'s generator arm**
(`signatures.rs`). It still reads the NEXT fallback and the async gate off
`contextual_generator_iteration_type`, and returns `None` (error) where
`createGeneratorType` answers `{}` with neither global
(CREATE-GENERATOR-TYPE-EMPTY-FALLBACK). Moving those three reads onto
`get_contextual_iteration_type` / `createGeneratorType`'s `{}` fallback is
handed to the calls box as a measured patch (§7).

## 7. `return_type_from_body`'s generator arm — patch for the calls box (CREATE-GENERATOR-TYPE-EMPTY-FALLBACK, YIELD-NEXT fallback)

`signatures.rs` belongs to the calls box, so this box does not commit it. The
measured patch is `docs/parity/notes/destructure-iteration-signatures.patch`
(`git apply` from the repository root). It makes three changes to
`return_type_from_body`'s generator arm and moves three test pins with them:

1. **NEXT fallback.** An empty next aggregate reads
   `getContextualIterationType(Next, fn)` orElse `unknown` (`checker.go:20251`)
   through §6's `get_contextual_iteration_type`, instead of the third type
   argument of a single `Generator`-family contextual reference.
2. **Async gate.** The async generator expression declines only when
   `get_contextual_return_type` cannot finish.
3. **`createGeneratorType`'s `{}` fallback** (`checker.go:20442-20447`): with
   neither `Generator` nor `IterableIterator` (or the async pair) the arm
   answers the empty object type rather than `None`.
   `generatorReturnTypeFallback.2` (`@lib: es5`) wants `() => {}`. Three unit
   pins that recorded the old decline (`error`/`None`) for a lib-less
   generator now read `{}`/`() => {}`: `signature_positions.rs`,
   `tests/return_inference.rs`, `tests/types.rs`.

**The fallback inside (1) and (2).** Where the new lookup is undecidable, the
patch keeps the pre-port read of a single generator-family reference. Without
it, `types.asyncGenerators.es2018.1` lost 11 RIGHT lines: an async generator
IIFE inside `yield*` takes its context from the `yield*` operand
(`Generator<…> | AsyncGenerator<…>`), and for the sync `Generator<…>`
constituent under the async use the engine cannot establish that
`[Symbol.asyncIterator]` is absent (`declared_property_table` declines
generic heritage; members lane). Falsifier: once that absence is decidable,
the fallback is unreachable and both arms reduce to the upstream call.

§6's `contextual_type_for_yield_operand_result` is the prerequisite: an IIFE
in a yield operand takes `getContextualTypeForYieldOperand`'s answer with its
nil kept apart from an unfinished lookup, where `get_contextual_type`'s
`None` would have read every nil there as undecidable. Alone it changes no
verdict (measured: 467725 RIGHT lines before and after).

**Measured on top of §6 (`1a2f0d7`):** checker_types RIGHT lines
467725 → 467740, both loss checks empty; cases newly fully RIGHT:
`contextualTypeOnYield1`, `contextualTypeOnYield2`,
`generatorReturnTypeFallback.2`, `generatorTypeCheck27`, `29`, `30`, `64`,
`types.forAwait.es2018.3`. CPU-median self-ratio 0.94 (domain-model), 0.97
(generic-imports); workspace tests pass with the patch applied.

## 8. A for-of pattern's parent through `checkRightHandSideOfForOf`; `never` has no members (tsr-2zk.10)

**Forcing constraint.** `getTypeForVariableLikeDeclaration`'s for-of arm is
`checkRightHandSideOfForOf` (`checker.go:17678`) =
`checkIteratedTypeOrElementType(ForOf, checkNonNullExpression(expr))`, the
iterable road whenever the global `Iterable` exists (`checker.go:6116`).
`get_type_for_binding_element_parent` read the pattern's parent through
`symbols.rs`'s `for_of_element_type`, which declines a degenerate element:
`for (const [,] of [])` (strict) had parent `error`, so the empty pattern's
`TS2488 Type 'never' must have a '[Symbol.iterator]()'…`
(`checkVariableLikeDeclaration`'s pattern check) never fired
(`omittedExpressionForOfLoop`, 2 missing).

**What was ported.** `for_of_iterated_type` (`iteration.rs`) is that
expression on the iteration engine: `any`/`never` input answer `any` (the
`nil → anyType` of `checkIteratedTypeOrElementType`; `never` is reported by
the statement's own check), else the YIELD type orElse `any`. With no global
`Iterable` (the array-like road, unported) or an undecidable protocol it
answers `None` and the destructure arm keeps the old element road. Only the
pattern-parent arm uses it; plain for-of names stay on `symbols.rs`.

**`never` has no members.** The iterated element of `never[]` is the
implicit-never variant, which is not `intrinsics.never`, so it reaches the
protocol walk rather than `getIteratedTypeOrElementType`'s identity arm.
`getPropertyOfType` on `never` answers nil for every name, so
`iteration_member_decidably_absent` now counts `NEVER` beside the primitives,
and the walk reports TS2488 as upstream's does.

**Measured** at the merge `26d6033`: missing diagnostics 4374 → 4372 (both
`omittedExpressionForOfLoop` TS2488), extras unchanged, both loss checks
empty, no verdict change (the case still lacks `checkNonNullExpression`'s
TS18050, not this lane).
