# Synchronous iteration and inherited member substitution

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 3ef6d975: 452,625/478,855 correct assertions (94.52%).
Goal: 99%, requiring 474,067 correct assertions.

## Native behavior and representation

getIterationTypesOfIterableSlow (checker.go:6460) reads the iterator method,
selects signatures callable with zero arguments and intersects their return
types. getIterationTypesOfIteratorSlow (:6533) combines next, return and throw.
getIterationTypesOfIteratorResult (:6649) separates result union constituents
using assignability of false to the done property; a missing done means false.
Only yield constituents contribute value types. A result containing only final
return values leaves the yield type absent, and the for-of/spread consumer
falls back to any. That fallback occurs after combining iterable unions: a
final-return-only iterator unioned with a number-yielding iterator still yields
number. A real never yield remains distinct from an absent yield. The port
carries a vector of yield contributions until the consumer applies the fallback.
A non-callable return/throw contributes no iteration types.

The port uses semantic property lookup for this synchronous yield calculation,
resolves the port's named alias bodies before result partitioning and guards
active iterable resolution. Existing array/tuple shortcuts remain. Native
callable objects resolve return types lazily; this port still constructs them
eagerly. An iterator property already on the Type resolution stack therefore
declines without pushing a second frame that would invalidate its callable.
The existing declaration-based class fallback remains for that limitation.

Indirect generic inheritance now composes the receiver mapper at every base
level. Member-local type parameter names survive the walk, so the outer mapper
does not replace a generic method's shadowing parameter. This fixes ordinary
member access as well as ArrayIterator and MapIterator next methods.

getDeclaredTypeOfTypeAlias (:23857) recognizes the zero-parameter intrinsic
BuiltinIteratorReturn; getBuiltinIteratorReturnType (:6400) selects undefined
or any through strictBuiltinIteratorReturn, independently of strictNullChecks.
The checker and conformance option projection now read that strict-family flag.
Explicitly annotated null/undefined variables retain their written types under
non-strict null checking; only unannotated declarations take the existing
widening path. Type queries perform the native getWidenedType step before
regularizing. The global undefined symbol is widening upstream (:1345); because
this port currently shares its ordinary undefined identity, symbol resolution
preserves that provenance specifically at the type-query seam. Shadowing
undefined parameters retain their explicit type. These fix pre-existing widening
errors exposed by the intrinsic.

## Experiments and controls

The initial semantic walk gained 12 matches but lost 11, including unresolved
T/TReturn substitutions and two recursive iterator method declarations. The
first inherited mapper correction alone left IteratorObject<T, error, unknown>:
BuiltinIteratorReturn had no intrinsic implementation. Implementing it allowed
the mapper composition to complete. Avoiding in-flight callable symbol reads
recovered the recursive declarations. The next full run gained 79 with zero
RIGHT losses (45 GAP-to-RIGHT, 34 WRONG-to-RIGHT).

Native edge probes then caught an incorrect never[] result for iterators with
only final return values. They also established that a non-callable return
must not erase a valid next yield. Regression tests cover both, along with
array interfaces, maps, unions, overloads, return/throw yields, four combinations
of strict and strictBuiltinIteratorReturn, inherited member substitution and
method-local shadowing. Two pinned recursive iterator cases retain () => any.
The option tests caught a missing conformance directive projection, then the
non-strict explicit-annotation widening issue; both were corrected before
final verification. The first annotation correction gained 58 more matches but
lost 11 uses of typeof undefined. The native type-query widening step restored
all 11 in the focused comparison. Clippy also required backticks on the new
intrinsic's documentation. A final adversarial native probe exposed that
applying the any fallback inside an iterable union erased a number yield. Yield
contributions now combine before fallback; a companion Iterable<never> control
keeps the distinction between absent and never yields. All six focused tests
cover the final representation.

## Remaining limits

This is the synchronous yield slice, not the full native IterationTypes record.
Async iteration, next/return input checking and iteration diagnostics remain
partial. Some invalid next signatures and missing value results still decline.
The port retains eager callable construction and its declaration fallback.
Direct generic iterator fast paths and semantic member completeness are not
fully ported. Expanded iteration exposes existing constructor inference and
destructuring precision gaps; these remain tracked separately. Spread arguments
before ordinary parameters and complete spread applicability also remain open.

## Verified checkpoint

At 87402686 the final full aligned comparison records
452,762 RIGHT, 3,316 GAP and 18,165 WRONG among 474,243 aligned assertions.
This is +137 RIGHT versus 3ef6d975: 92 WRONG-to-RIGHT, 45 GAP-to-RIGHT, zero
RIGHT losses, 23 GAP-to-WRONG, six WRONG-to-GAP and 23 changed wrong answers.
The last union correction preserves those counts. The 23 new wrong answers
remain chiefly constructor inference and destructuring precision; six gaps
replace wrong object-literal nullish contextual types. No corpus denominator,
expected type or verdict was changed.

Coverage is 452,762/478,855 assertions (94.55%) and 6,714/9,538 fully matching
cases (70.39%). The 99% goal still requires 21,305 additional correct assertions.

Evidence:
- /tmp/tsr-99-iteration-sealed-verdict.tsv
- /tmp/tsr-99-iteration-sealed-verdict.log
- /tmp/tsr-99-iteration-sealed-transitions.txt
- /tmp/tsr-99-iteration-sealed-sourcehash.txt

Release workspace tests finish with exit 0 and 189 passing result blocks. All
six focused tests pass. Clippy with warnings denied and all 3,361 upstream
anchors pass; the refreshed checker_types snapshot confirms the counts above.
The earlier test-only raw-string delimiter correction was verified before the
final source change and full rerun. Checker sources plus trace_case SHA-256:
a92232fdf70de60bfb00de35803c6fddf2759ec01f3c5450b56f39c2038d5d69.


Format and whitespace checks pass. The depend instrument remains non-authoritative
because its C1/C4 controls are stale (tsr-6.29); the aligned comparison and
checker_types snapshot above provide the coverage evidence.

```text
## Controls
  C1 construction: roots that do not gap  578  (expect 0)
  C2 construction: cycles 256, depth-cap hits 0
  C3 arithmetic:   root buckets sum to 4306, gap lines walked 4306
  C4 frozen:       STATUS.md publishes ~127,736 gap lines at 63.66%
```

Remaining work is tracked in tsr-6.28 and tsr-6.30. The 99% goal remains active.
