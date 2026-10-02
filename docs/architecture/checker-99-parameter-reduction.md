# Subtype reduction for constrained type parameters

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline fe8f3a22: 454,492/478,855 correct assertions (94.91%).
The 99% target requires 474,067 correct assertions.

## Forcing constraint and native operation

The current mismatch inventory contains 94 wrong assertions in
subtypesOfTypeParameterWithConstraints2. Most are conditional-expression results
such as T extends U beside U, or T extends Date beside Date. The reducer refused
all type-parameter constituents and all generic references with parameter
arguments, even though the relater now follows constraints and instantiated
members. A conditional-expression shortcut separately classified some parameter
pairs as reduction-free, retaining T beside its primitive constraint.

Native checkConditionalExpression (checker.go:10934) builds its branch union with
UnionReductionSubtype. removeSubtypes (checker.go:25937) uses ordinary directed
strict-subtype relations except for one essential branch: a type parameter with a
union base constraint is checked against the union of every other surviving
constituent (checker.go:25959). T extends A | B can inhabit A | B without inhabiting
A or B separately. Reduction walks backwards through the surviving list.

The port now follows that branch and removes the two historical blanket refusals.
Conditional expressions share the subtype reducer; only the existing identity,
any and primitive-only fast paths bypass it. An undecidable relation still
refuses the reduction rather than inventing a subtype fact. Existing class
heritage and fresh-object excess-property checks remain in the reducer.
Type parameters are excluded from the legacy equal-printed-text shortcut because
same spelling cannot establish parameter identity.

## Controls and limits

Native declaration probes pass strictness explicitly. The focused controls cover
constraint chains, primitive versus boxed primitive constraints, object
constraints, unrelated parameters, fully covered and partially covered union
constraints, and bounded/unbounded parameters beside {}. In particular native
returns 1 | T for the boxed-Number return probe; its literal must not be widened
just because the fixture's variable-initializer counterparts print number | T.
The initial focused test fails before implementation and passes after the port.

The relater and union representation retain other documented capability limits;
this unit does not claim every native subtype rule. Unknown relation results,
legacy structural identity approximations, rendering, and generic inference must
remain visible in the full corpus. A lost previously correct assertion, collapse
of unrelated parameters, removal under a partially covered union constraint or
an isolated-checkout source mismatch falsifies the corresponding claim.

Review and simplification run sequentially in the primary thread under the user's
AGENTS tool map; no independent or cross-model review is claimed. Work is tracked
in tsr-6.36. Full-corpus attribution and committed isolated verification follow.

## Candidate measurement and review boundaries

The full candidate adds 265 correct assertions: 254 WRONG-to-RIGHT and 11
GAP-to-RIGHT, with zero RIGHT losses and no GAP-to-WRONG or WRONG-to-GAP
transitions. Sixteen answers change while remaining wrong: twelve concern
shadowed parameter serialization, two contextual branch rendering, and two
polymorphic this results. The leading gains are the constrained-parameter
fixtures (82, 20 and 12), string-literal type arguments (74), recursive constraint
fixtures (48), and Array.from (13). These populations are measured conversions.

Additional native controls confirm Box<T> reference reduction and fully versus
partially covered object-union constraints. They also expose limits outside this
branch: recursive<T extends {next:T}> still refuses the strict-subtype relation
to a separately constructed {next:T}; Holder<number>.choose with a shadowing
method parameter retains an outer T in its return. Native returns {next:T} and
number | "x", respectively. These are tracked with the sixteen changed wrong
answers in tsr-6.37. Distinct parameter identities survive even though the printer
still emits T | T instead of native T_1 | T. This unit does not claim those
remaining relation, receiver substitution or name-allocation paths are complete.

The frozen final candidate is byte-identical to the first full measurement:
454,757 RIGHT, 2,952 GAP and 16,534 WRONG among 474,243 aligned assertions.
Against all 478,855 expected assertions, this is 94.97%; 19,310 remain to 99%.
Two native-controlled focused tests pass in the release workspace. Disabling the
union-constraint branch in an isolated scratch copy makes both tests fail on
fully covered constraints, as intended. Release workspace tests pass 197 result
blocks; clippy with warnings denied, formatting, whitespace and 3,341 native
anchors pass. The review preserves explicit refusals for Unknown relations and
confirms backwards survivor order and parameter identity. No new abstraction is
needed; conditional expressions use the shared reduction path.

Checker sources plus trace_case SHA-256:
27bede7b204ef72f52a384e38ddedc1c291e10b629b8b0a923a25fbbd3f0b7e9
