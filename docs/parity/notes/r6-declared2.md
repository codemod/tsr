# r6-declared2: `declared.rs`, `mapped.rs` and neighbours, round 6

Lane files: `declared.rs`, `instantiation_expressions.rs`, `unique_symbols.rs`,
`mapped.rs`, `intersections.rs`, `intrinsics.rs`. Issue `tsr-2zk.1143`.
Successor of r6-declared (`r6-declared.md`), whose commits (batch BM) had not
landed on the integration branch when this box started. Vendor pinned at
`5b1047d`.

## 0. Frozen base

`claude/beautiful-shannon-ar5gh0` at `0854d36` (batch BI's snapshot
refresh). Batch BM was not on the branch, so the brief's fallback (the
current tip) applies. Unfiltered release build:

- `diagverdictdump`: RIGHT 5595, EMPTY_RIGHT 5606, WRONG 998, EMPTY_WRONG 39
  (12,238 rows);
- `verdictdump`: RIGHT 550,131, WRONG 5,404, GAP 768 (556,303 rows).
- Callgrind Ir (`tsr -p <project> --singleThreaded --pretty false
  --noEmit`): domain-model 1,091,238,874; generic-imports 343,094,406.

Setup: PyPI answers 403, so `assemble.py`'s three `tomlkit` calls ran on a
stdlib-only stand-in in the session scratchpad, not committed
(`r5-operators3.md` §4). The native oracle is
`scripts/offline-cargo/build-tsgo.sh`'s tsgo; every expectation below was
checked against it (`--declaration --emitDeclarationOnly` prints, or an
assignment's diagnostic).

## 1. An unevaluable conditional alias reference in an alias body defers

**Forcing constraint.** `get_instantiated_type_reference` answered `error`
for a reference to a conditional alias in an alias-declared position (an
alias body, `in_alias_declared_position`) whenever
`evaluate_conditional_alias` declined. Native's getTypeAliasInstantiation
(checker.go:23641) reaches getConditionalType (checker.go:24339), which
**defers** when the instantiated check type is generic, or the extends type
is (`isDeferredType`): the result is a ConditionalType with its root, mapper
and the reference's alias, printed as the alias reference. Inside a mapped
node that `error` made `mapped_type_info` decline, so `Exclude<P, K>` in an
`as` clause or `Remap2<T[P]>` in a template fell back to written text
(`r6-mapped.md` §3a).

**The plain fallthrough was measured and refused.** Letting the arm fall
through to the named reference (the port's deferred conditional: a reference
whose `type_reference_targets` entry `alias_declares_conditional` reads as a
conditional) measured, on this base, types **+78 / −17**. The losses show
what the `error` stood for:

- **A decided check whose branch native also cannot compute**:
  `awaitedType:0:27/34`, `awaitedTypeStrictNull:0:27/34` (`Awaited<BadPromise>`),
  `excessivelyLargeTupleSpread:0:1` (`BuildTuple<…>`),
  `recursiveConditionalTypes:0:32` (`_Flatten<InfiniteArray<string>>`).
  Native reaches its instantiation guard (TS2589) and answers errorType; the
  port's evaluator declines at the same guard. `awaitedType:0:14`
  (`Awaited<any>`) is native `any` by evaluation, which this port's
  evaluator does not do for an `infer`-bearing nested conditional; the gap's
  `any` print coincides with it.
- **`intersectionWithIndexSignatures` ×5**: the fallthrough printed
  `constr<{}, …>` where native names the declaring alias `s`.
- **`contextualTypesNegatedTypeLikeConstraintInGenericMappedType3` ×5**: the
  mapped type now built, but its `as Exclude<P, K>` did not exclude
  `onChange`, so the parameter widened to `number | Event` (see §1.1).

**Port.** The arm falls through only when native defers:
`conditional_alias_check_is_deferred` resolves the alias's check type (and,
without `infer` parameters, its extends type) under the alias bindings and
asks the evaluator's own generic test (`INSTANTIABLE_NON_PRIMITIVE`, or a
registered type parameter mentioned). Every other decline keeps the gap. The
first and second loss groups have concrete check types, so they keep `error`.

**Divergence kept.** With `infer` parameters, native tests
`inferredExtendsType`, the extends type under the inference mapper; this test
cannot form it and asks only the check type. A check that is decided but whose
branch the evaluator cannot compute (`Awaited<any>`, the guard cases) keeps
`error`, as before. Seven of the plain fallthrough's gains
(`intersectionWithIndexSignatures:0:39–49`) came from such concrete checks
printing as references; they are not taken, since that print is not native's.

### 1.1 isExcludedMappedPropertyName reads the conditional itself

getIndexedMappedTypeSubstitutedTypeOfContextualType (checker.go:30607) skips
a property whose name the `as` clause excludes: isExcludedMappedPropertyName
(checker.go:30624) asks whether the name type is a conditional whose true
type is `never`, whose false type is its check type, and whose extends type
the property name is assignable to. The port answered only for an inline
conditional minted in a mapped template (`mapped_conditionals`), not for a
reference to a conditional alias, which `Exclude<P, K>` now is.

`conditional_root_operands` (`declared.rs`) answers `[check, extends, true,
false]` for both: the inline mint's recorded operands, or the alias root read
under its bindings through `with_conditional_inference_node`.
`is_excluded_mapped_property_name` (`mapped.rs`) asks it for any
conditional-flagged or alias-reference name type. No cache: the read is the
existing root walk.

**Measured** (unfiltered, both dumps, against §0):
- types **+70 RIGHT, 0 lost**: `conditionalTypes1` 33,
  `mappedTypesArraysTuples` 8, `conditionalTypes2` 8 (172–184, a brief
  target), `mappedTypeOverlappingStringEnumKeys` 5,
  `propTypeValidatorInference` 4, `genericIsNeverEmptyObject` 4,
  `mappedTypeAsClauses` 3 (98/100/107), and
  `recursiveTypeAliasWithSpreadConditionalReturnNotCircular` 2,
  `recursiveMappedTypes:0:24`, `literalTypeWidening`,
  `recursiveTupleTypeInference` 1 each;
- diagnostics **+1 case**, `mappedTypeOverlappingStringEnumKeys` EMPTY_WRONG →
  EMPTY_RIGHT, 0 lost;
- slowcases clean on both dumps;
- Ir: domain-model 1,091,238,874 → 1,094,345,197 (+0.28%), generic-imports
  343,094,406 → 343,210,798 (+0.03%). The deferral test itself is 112k Ir.
  The rest is diffuse: `core.ts`'s `DeepReadonly<T[K]>` and
  `DeepReadonly<U>` in its own body now build as deferred references, so the
  mapped template and what relates through it exist where `error` stood
  (`evaluate_mapped_type_node` +0.62M, the evaluator +0.75M inclusive, and
  sub-1% per-call growth across the relater and member roads). That is
  native's work: tsgo builds the same deferred conditional. CPU, interleaved
  31 runs each against the base binary: single-threaded 1.000, multi-threaded
  0.970 (the harness's blocked 41-sample run read 1.047; a base-vs-base run of
  it read 1.015, so its ordering drifts). CLI output identical on both
  projects.

**Still not RIGHT among the brief's targets.**
- `mappedTypeAsClauses:0:72` (`GetKeyWithIf<S, V>`) and `:0:108` (`TN4<T, U>`):
  `keyof` of a mapped type with an `as` clause whose conditional nests
  another conditional in its check type
  (`(K extends U ? T[K] : never) extends T[K] ? K : never`); native prints
  the `keyof { … }` form. The evaluator still declines on the nested check.
- `reactReduxLikeDeferredInferenceAllowsAssignment:0:82`: the thunk's
  `Promise<string>` prints `unknown`. The deferred
  `HandleThunkActionCreator<TDispatchProps[C]>` now builds; the return type
  is inference's (`inference.rs`, MAIN).

Tests: `tests/r6_declared2.rs`
`a_deferred_conditional_alias_in_an_alias_body_keeps_its_alias` (`b.v` of
`Box<T> = { v: Ex<T, null> }` is `Ex<T, null>`, as tsgo prints; errorType on
the base) and `a_concrete_check_does_not_defer`.

**Falsifier.** A conditional alias in an alias body whose check type is
generic and which native nonetheless evaluates (an any/never/error check is
decided before the generic test natively too), or a mapped `as` clause whose
exclusion native does not make: a contextual parameter typed `any` here where
native types it.
