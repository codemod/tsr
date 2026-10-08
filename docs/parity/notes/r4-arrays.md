# r4-arrays lane notes (`tsr-2zk.908`, `.16.80`, `.16.93`, `.16.87`)

Judgment calls made while porting `checkArrayLiteral`'s tuple context and
its neighbours. Upstream anchors are to `vendor/typescript-go` @ `5b1047d`.
Native `tsgo` could not be built in the box (Go 1.26 toolchain download is
blocked; the container has Go 1.24.7), so every hypothesis below was
reproduced against the pinned source and the reference `.types` baselines,
not by running native.

## 1. `isTupleLikeType` replaces tuple-table membership (`.16.80`)

**Forcing constraint.** `checkArrayLiteral` (`checker.go:8021`) mints a tuple
when `someType(getApparentTypeOfContextualType(node), isTupleLikeType || …)`.
`isTupleLikeType` (`checker.go:23544`) is three disjuncts: a tuple type, a
type with a property named `"0"`, or an array-like type whose `length` is a
union of number literals. `array_literal_has_a_tuple_contextual_type` asked
only the first (`tuple_element_lists`/`variadic_tuple_elements`), so a
contextual `RegExpMatchArray` (which declares `0: string`), a constraint
`{ "0": … }` or an interface with a literal `length` never put the literal in
tuple context: `''.match(/ /) || []` printed `never[]` where native prints
`[]`.

**What was ported.** `Checker::is_tuple_like_type` and
`Checker::is_array_like_type` in `tuples.rs`, called from the generic
`someType` arm. The seven hand-rolled parent arms in front of it still test
tuple membership only; they answer `Annotated` or fall through to this arm,
so the change can only widen tuple context, never narrow it.

**One reordering.** The third disjunct is evaluated length-first: the
`length` read is a property lookup, `isArrayLikeType` is a relation to
`readonly any[]`, and every ordinary array context (`length: number`) fails
the literal test before the relation runs. The conjunction is the same; only
the evaluation order differs, and neither side has an observable side effect
the other depends on.

**Work boundary.** No cache or side table is added. The new work per array
literal with a contextual type is one `getPropertyOfType(t, "0")` and one
`getTypeOfPropertyOfType(t, "length")` per contextual constituent; the
`readonly any[]` relation runs only for constituents whose `length` is all
number literals (tuple-shaped interfaces), through the ordinary
`relate_ternary` publication.

**Dependency outside this lane's files.** Making `[]` a tuple exposed a
relater fallthrough: `[] -> RegExpMatchArray` under `StrictSubtype` reaches
the undecided tail of `structured_type_related_to` (row 3, "no members
table"), so `removeSubtypes` for `RegExpMatchArray | []` declines and
`x || []` becomes a gap. Native's `propertiesRelatedTo` (`relater.go:4100`)
fails the pair on the missing required `0`. The fix is a relation arm in
`relater.rs`, which no round-4 box owns, so it is delivered as
[`r4-arrays-tuple-source-negative.diff`](r4-arrays-tuple-source-negative.diff)
for the integrator to route. Measured (box baseline at `b23dd3d`, full
unfiltered dumps):

| configuration | types RIGHT | type losses | diag cases converted |
|---|---|---|---|
| baseline | 469,946 | — | — |
| `isTupleLikeType` alone | 469,953 | **19** (`bestChoiceType`) | — |
| `isTupleLikeType` + relater diff | 469,985 | 0 | 2 (`arrayLiterals3`, `unionTypeFromArrayLiteral`) |

So the `isTupleLikeType` commit must be merged **with** the diff applied.

The diff takes only the definite negative: a required own member of the
target (property or method, `?` read from syntax because the binder writes no
`SymbolFlags::OPTIONAL`) that the tuple has neither as a tuple property
(`tuple_target_properties`) nor through `get_property_of_type`. Own members
suffice for a negative because an inherited requirement can only add
failures. It deliberately does not use `get_type_of_property_of_type` on the
tuple: that answers `Some` for `"0"` on the empty tuple `[]` (the numeric
index fallback), which native's `getPropertyOfObjectType` does not, and the
first draft of the arm with that call decided nothing.

**Falsifier.** A corpus case whose array literal native prints as an array
(`T[]`) while its apparent contextual type has a `"0"` property or a literal
`length` would mean the disjunct order or the apparent-type step differs.
None appeared in the full run.

## 2. `instantiateContextualType` with `ContextFlagsNone` — measured, not shipped

`getApparentTypeOfContextualType(node, ContextFlagsNone)` instantiates a
generic contextual type only through the **return mapper**
(`checker.go:30817`); the non-fixing mapper is consulted only under
`ContextFlagsSignature`. `array_literal_has_a_tuple_contextual_type` calls
`instantiate_contextual_inference_type`, which is the `Signature` form.

A faithful `ContextFlagsNone` variant (return mapper only, `true`/`false`
filtered from a union image, the resolved-signature mapper outside a live
inference) was written and measured with §1 applied: the full type and
diagnostic dumps were **identical line for line** with and without it. It is
not part of the shipped commit because it converts nothing, and a
one-root-cause commit names the cases it converts. The divergence stays
recorded here; a case that separates the two forms (a contextual `T` whose
non-fixing inference is tuple-like while its constraint is not, or the
reverse) would make it worth landing.

## 3. `.16.93` (`ARRAY-LITERAL-TUPLE-CONTEXT`): mostly not this lane's root

Reading the eight cases against native:

- `correctOrderOfPromiseMethod`, `awaitedType{,StrictNull}`,
  `unknownLikeUnionObjectFlagsNotPropagated`, `mappedTypesGenericTuples2`:
  `Promise.all([…])` resolves to the `Iterable<T | PromiseLike<T>>` overload
  in TSR where native picks `<T extends readonly unknown[] | []>(values: T)`.
  The literal's type follows the chosen signature, so the divergence is in
  overload selection (`chooseOverload`, `calls.rs`), not in
  `checkArrayLiteral`. `awaitedType`'s `MaybePromise(1)` additionally
  answers `any` (alias instantiation).
- `reverseMappedTupleContext`, `spreadsAndContextualTupleTypes`,
  `wideningTuples1`: element literal preservation and spread normalisation in
  tuple context; not reached this session.

## 4. `.16.87` (`ARRAY-LITERAL-ERROR-ELEMENT`): blocked on `tsr-2zk.31`

All nine cases are an array literal with an element whose type is native's
`errorType` (unresolved import, JSX element with no factory, a failed tagged
template). Native unions the element and prints `any[]`. TSR's array arm
declines the whole literal on an `error` element ("a gap in an element is a
gap in the array"). Removing the decline was measured on the nine cases:
every line moves `GAP/WRONG -> WRONG` printing `error[]`, because this port
prints its `error` intrinsic as `error` precisely so that "could not compute"
stays visible. Converting the cluster therefore needs the two kinds of
`error` separated (native `errorType` printing `any` vs. the port's gap
marker) — box-protocol §3a and `tsr-2zk.31` — which is not an array-literal
change. Not pursued.

## 5. Measurements

All numbers: box baseline frozen at `b23dd3d` (integration head), full
unfiltered `verdictdump`/`diagverdictdump`, the commit's code with
[`r4-arrays-tuple-source-negative.diff`](r4-arrays-tuple-source-negative.diff)
applied.

- `checker_types` cases 8,075 → 8,079 (`bestChoiceType`,
  `inferringAnyFunctionType1`, `arrayLiterals3`, `unionTypeFromArrayLiteral`);
  assertion lines 469,946 → 469,985 (34 WRONG→RIGHT, 5 GAP→RIGHT:
  `reverseMappedIntersectionInference2` 17, `bestChoiceType` 7,
  `flatArrayNoExcessiveStackDepth` 4, `literalTypes2` 4,
  `inferringAnyFunctionType1` 3, `initializedDestructuringAssignmentTypes` 2,
  `arrayLiterals3` 1, `unionTypeFromArrayLiteral` 1).
- `diagnostics` cases 4,232 → 4,234 (`arrayLiterals3`,
  `unionTypeFromArrayLiteral` WRONG→RIGHT).
- Loss checks (RIGHT/EMPTY_RIGHT diagnostics, RIGHT type lines): both empty.
  Without the relater diff: 19 type lines of `bestChoiceType` RIGHT→GAP/WRONG.
- Perf, median child CPU new/old against the baseline binary:
  `domain-model` 0.976 (41 samples), `generic-imports` 1.008 (21 samples),
  `diagnostics_match: true` on both.
- `cargo test --workspace --release --no-fail-fast`: two failures,
  `optional_tuple_check_types_preserve_named_enum_identity_and_reads` and
  `tuple_slice_optional_arguments_follow_null_and_exact_optional_options`,
  both failing identically at the baseline.

## 6. The array-like road of `getIteratedTypeOrElementType` (`tsr-2zk.908`)

**Forcing constraint.** `getIteratedTypeOrElementType` (`checker.go:6106`)
consults the iteration protocol only when the program has a global
`Iterable` (or the use allows async iterables); otherwise it takes the
array-like road and reports `getIterationDiagnosticDetails`' message:
TS2802 when the type is iterable after all or names an ES2015 collection,
else TS2495 (strings allowed) or TS2461. `check_iterated_type_or_element_type`
returned silently whenever `Iterable` was missing, so `for await (x of {})`
under `@lib: es5` reported nothing where native reports TS2495
(`types.forAwait.es2018.3`, 4 diagnostics).

**What was ported.** `check_array_like_iteration` in `iteration.rs`: the
async-protocol probe without an error node (return on a yield type), the
string-constituent removal (`UnionReductionSubtype`, through
`union_with_subtype_reduction`), `isArrayLikeType` as a three-valued
relation to `readonly any[]`, and the message selection.

**Accepted limits.** An undecided relation, protocol walk or subtype
reduction reports nothing — the existing iteration road's policy. The
`Did you forget to use 'await'?` related information is not attached
(`report_type_not_iterable_error` makes the same call; the baseline
comparison is per code and span).

**Measured** (same baseline, relater diff of §1 applied): diagnostics
4,234 → 4,235 (`types.forAwait.es2018.3` WRONG→RIGHT); no other diagnostic
row changed in any column; type dump identical; both loss checks empty;
perf new/old median child CPU `domain-model` 0.955, `generic-imports` 0.996
(21 samples, `diagnostics_match: true`). The road runs only in programs
without a global `Iterable`, which neither bench project is.

**Falsifier.** A corpus case under an ES5 lib whose native baseline is
silent on a `for…of`/spread/destructuring this road reports on would mean
`isArrayLikeType` answers differently here (most likely a relation decided
`NotRelated` where native relates).
