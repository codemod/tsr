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
