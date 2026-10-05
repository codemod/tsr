# Lane notes: calls-inference (tsr-2zk.9)

Judgment calls made by the `calls-inference` parity box. Numbers are measured
with `verdictdump` / `diagverdictdump` against the baseline frozen at
`0d996e8` unless a section says otherwise.

## 1. Untyped calls: widening the provenance gate, not adopting `IsTypeAny` (tsr-2zk.16.5)

**Forcing constraint.** `isUntypedFunctionCall` (`checker.go:9933`) is
`IsTypeAny(funcType) || …`: any callee typed `any` makes the call an untyped
call answering `any` (`resolveUntypedCall`, `checker.go:9902`). TSR's
`is_untyped_call_target` admitted only an `any` the source *wrote* (annotation,
cast, unresolved name, `any` receiver), because when it last tried the plain
test it measured 248 gap→wrong. Cluster `UNTYPED-CALL-ANY-CALLEE` is 16 cases
whose callee is an `any` nobody wrote.

**Alternatives measured** (types lines, unfiltered, against the frozen base):

| Gate | gap→right | gap→wrong | right→other |
|---|---:|---:|---:|
| `callee_type == any` (upstream verbatim, error excluded) | 42 | 30 | 0 |
| ANY-flagged incl. minted unresolved | 42 | 32 | 0 |
| provenance arms below, all on | 38 | 0 | 0 |

The 30 gap→wrong of the verbatim test are callees this port types `any`
through unported machinery, where upstream has a real type: UMD-augmentation
members (`umd-augmentation-1..4`), private-name accessors
(`privateName*AccessorsCallExpression`), module augmentation
(`module_augmentUninstantiatedModule2`, `typeReferenceDirectives9`), the
class-expression-name resolver miss (`classBlockScoping`), dynamic import
unions, `returnInfiniteIntersection`, and a JS overload. Answering `any` there
converts a gap into a wrong line — the convention's "a gap beats a wrong
answer".

**Decision.** Keep the allowlist; add the implicit-any provenances upstream
computes too (`Checker::any_is_upstreams_implicit_any`, `calls.rs`). Each arm
measured alone, each zero gap→wrong:

| Arm | Upstream source of the `any` | gap→right |
|---|---|---:|
| unannotated parameter with no contextual type | `getWidenedTypeForVariableLikeDeclaration`, no context | 22 |
| `typeof globalThis` property no global declares | `checker.go:11337-11344` | 8 |
| element of unannotated `var x = []`, no `noImplicitAny` | widened auto-array `any[]` | 4 |
| member declared `: any` | the annotation | 2 |
| unannotated getter returning only `null`/`undefined` | widening of `null` | 1 |
| unannotated var initialised `undefined`/`null` | widening | 1 |

The parameter arm refines the old "positional refusal": that refusal was right
for parameters upstream contextually types, and `has_no_contextual_type`
(`signatures.rs`) is the existing proof that a position has none. Object-literal
methods and JS files decline (contextual typing and JSDoc respectively).

**Consequences accepted.** Four lines the verbatim test would also convert
stay gaps (`decoratorReferences`, `contextuallyTypedIife`, two private-name
`new` lines). The provenance machinery grows rather than shrinks.

**How to know this is wrong.** Any arm producing a gap→wrong in a later run.
**When to delete it:** when the verbatim test measures zero gap→wrong — i.e.
once the unported `any` producers listed above are ported, the allowlist should
be replaced by `IsTypeAny(funcType)`.

## 2. `Function`-typed callees are untyped calls (tsr-2zk.16.5)

`isUntypedFunctionCall`'s third disjunct (`checker.go:9936`): a callee whose
apparent type is not a union, does not reduce to `never`, has no call and no
construct signatures, and is assignable to the global `Function` interface is
an untyped call answering `any` (`fn: Function; fn()`). Ported as
`Checker::is_untyped_function_typed_callee` (`calls.rs`) on the call and
tagged-template roads; the `new` road has no such disjunct upstream.

**Judgment.** Both signature lists must come from a *complete* query
(`signatures_of_type_kind` answering `Some`). `None` means "unresolved", not
"zero"; reading it as zero would turn every unresolved callee into `any`. The
module-clone special case in `check_call_expression_worker` already computed
the `Function` assignability; it now shares `is_assignable_to_global_function`.

Measured alone on top of §1: +4 gap→right, 0 gap→wrong, 0 losses
(`functionType`, `callWithSpreadES6` converted).

**Not ported:** the type-parameter disjunct
(`IsTypeAny(apparent) && funcType is TypeParameter`); TSR does not answer an
`any` apparent type for an unconstrained type parameter.
