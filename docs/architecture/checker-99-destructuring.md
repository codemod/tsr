# Tuple binding, inherited indexes and union members

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 87402686: 452,762/478,855 correct assertions (94.55%).
The 99% target requires 474,067 correct assertions.

## Native paths

getBindingElementTypeFromParentType (checker.go:17707) maps instantiable base
constraints before slicing array-rest bindings. When every constituent is a
tuple, it slices each and unions the slices. Mixed array/tuple and other iterable
parents produce arrays of the iterator yield type. The port now follows those
branches, preserving optional flags, labels, variadic tails and mutable copies.
The port's generic alias wrapper is expanded before inspecting tuple shape.

Non-rest array bindings use isArrayLikeType (:23520): positional indexing for
array-like sources, synchronous yields for other iterables. A custom iterator
with a numeric property therefore binds its yield type. Defaults run after
projection, and noUncheckedIndexedAccess is supplied to the positional or
iterable read. Unknown array-like relations remain unresolved. A definite
missing required property can reject a structured relation even when another
member axis is unknown; Unknown must not erase that negative result.

Inherited index values now compose every explicit heritage mapper before the
receiver's own mapper. instantiateIndexInfo (:20642) substitutes the value and
retains the key. Own indexes and earlier bases win for the same key. Property
name enumeration can follow generic bases because names need no substitution.

createUnionOrIntersectionProperty (:21452) combines named properties with
applicable indexes from constituents missing that property. At least one named
property must exist; a union made entirely of indexes still follows the indexed
access path, including unchecked undefined. Bracket-encoded symbol names do
not fall back to string indexes. IndexInfo now carries readonly through ordinary
and inherited declarations, instantiation, union (OR) and intersection (AND)
combination. Mapped indexes copy the modifiers source unless explicitly
adding/removing readonly, and object-literal indexes retain const context.
Property writes through a union observe readonly indexes.

getUnionSignatures (:21108) considers each candidate separately. Unsupported
generic or predicate candidates no longer prevent matching an ordinary overload
in every constituent. Exact matching now compares pure nongeneric callback
signatures structurally, including nested returns and this parameters. Callback
images can have different TypeIds after instantiation. This limited identity
comparison does not use printed text or bidirectional assignability, and it
declines recursion beyond 32 levels and unsupported object/generic structures.
The second native union-signature pass remains unported.

## Experiments and controls

The first binding change gained 15 assertions but lost 11. Generic alias views,
inherited index substitution and property enumeration reduced the losses to
three. Combining definite property failures with unknown relation axes recovered
two generator bindings. Matching callback images recovered the tuple-union every
call; this candidate gained 70 with zero RIGHT losses.

A new focused mixed tuple/array test found that the array's numeric index did not
contribute to the union's named positional property. Adding that native fallback
gained further matches but exposed three previously hidden failures: readonly
index writes and symbol names accidentally falling back to string indexes.
Readonly metadata and the symbol-name boundary recovered all three in the
focused comparison. The workspace suite also contained an obsolete assertion
that explicit generic index inheritance must stay a gap; it now checks the
native number result.

Native declaration probes cover tuple and array unions, generic constraints,
variadic tuples, optional readonly labels, custom iterators with numeric
properties, defaults, unchecked indexed access, multi-level generic index
inheritance, overriding indexes and ordinary callback matching. The pinned
unionTypeWithIndexSignature baseline checks readonly writes and symbol keys.
A negative callback control retains the explicit gap for native's unported
second-pass signature combination instead of ignoring nested return types.

## Remaining limits

Readonly tuple-union constituent order still differs from native in the probe.
Some empty-tuple inference normalizes to never[]; RegExpMatchArray unions expose
that and existing union-order differences. Full destructuring flow, generic
signature identity and composite predicates, union signature parameter
intersection, implicit default arguments on bases and qualified heritage remain
incomplete. Tuple-rest fallback indexes, object-literal partial union properties,
general index-write diagnostics and unresolved mapped modifiers sources remain
partial. The port still represents late-bound names by bracket spelling.
No assertion denominator or expected native baseline was changed.

## Verified checkpoint

At UNIT_CODE_CHECKPOINT the full aligned comparison records 452,875 RIGHT,
3,297 GAP and 18,071 WRONG among 474,243 aligned assertions. This is +113
versus 87402686: 100 WRONG-to-RIGHT, 13 GAP-to-RIGHT, zero RIGHT losses,
six GAP-to-WRONG and 11 changed wrong answers. The six newly answered wrong
assertions expose empty-tuple normalization and union-order differences in
bestChoiceType. The final readonly metadata propagation preserves the counts.

Coverage is 452,875/478,855 assertions (94.57%) and 6,723/9,538 fully
matching cases (70.49%). The 99% goal still requires
21,192 additional correct assertions.

Evidence:
- /tmp/tsr-99-destructure-complete-verdict.tsv
- /tmp/tsr-99-destructure-complete-verdict.log
- /tmp/tsr-99-destructure-complete-transitions.txt
- /tmp/tsr-99-destructure-complete-sourcehash.txt

Release workspace tests finish with exit 0 and 190 passing result blocks,
including all seven focused tests. Clippy's documentation backticks and a
test-only raw-string delimiter were corrected; the final delimiter change does
not alter checker sources or test contents. Checker sources plus trace_case
SHA-256: 56fe34b4b0bcff248db122a81f905f2cdf3d3ba6edd9590a5ef4b72b32347d03.

Remaining work is tracked in tsr-6.28 and tsr-6.30. The 99% goal remains active.

Clippy with warnings denied, all 3,355 upstream anchors, formatting and
whitespace checks pass. The refreshed checker_types snapshot confirms the
counts above. The final focused rerun passes all seven tests after the raw
string formatting correction. Review ran sequentially in the main thread;
no independent or cross-model review is claimed.

The depend instrument completes but remains non-authoritative because its
C1/C4 controls are stale (tsr-6.29). The aligned comparison and snapshot are the
coverage evidence.

```text
## Controls
  C1 construction: roots that do not gap  572  (expect 0)
  C2 construction: cycles 254, depth-cap hits 0
  C3 arithmetic:   root buckets sum to 4275, gap lines walked 4275
  C4 frozen:       STATUS.md publishes ~127,736 gap lines at 63.66%
```
