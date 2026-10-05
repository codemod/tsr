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
