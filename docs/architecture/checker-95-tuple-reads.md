# Generic tuple reads, iteration and destructuring

This continuation uses typescript-go `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Against checkpoint `18ce162e`, correct assertions increase from 447,414 to
447,501/478,855 (93.45%). Complete cases increase from 6,529 to 6,532/9,538
(68.48%). The 95% target requires another 7,412 correct assertions.

## Upstream mechanisms

`shouldDeferIndexedAccessType` runs before positional property lookup. A generic
variadic tuple resolves positions below its total fixed element count; subsequent
positions and numeric reads retain captured indexed-access metadata. For
`[string, ...T, number]`, position 1 reads `number | T[number]`, while position 2
retains `[string, ...T, number][2]`. Numeric tuple iteration unions its fixed
slots with each generic spread operand's numeric projection. Ordinary array
spread inference still retains the whole source's deferred numeric access.

`getIteratedTypeOrElementType` and apparent property lookup follow a type
parameter's constraint to resolve iteration. The port follows constraint chains
with a cycle guard before its array and tuple shortcuts. `U extends T` with
`T extends unknown[]` therefore iterates `unknown`; readonly string array
constraints iterate `string`.

`sliceTupleType` (`internal/checker/relater.go`) preserves element flags and
labels and removes readonly from a destructured copy. Slices at the fixed prefix
retain generic rests; slices beyond the prefix form an array of the remaining
element union. This prevents a removed fixed prefix from contributing to the
rest binding's element type. Optional type arguments include undefined under
strict null checks, except when exact optional property types omit implicit
missing from the tuple slot. Written source annotations keep their spelling.
Positional variadic destructuring uses the same indexed-access deferral rule as
ordinary element access.

These are bounded ports: transformed mapped spread operands still decline when
the numeric projection is unavailable. Rest destructuring does not yet implement
the upstream union-wide tuple base-constraint mapping. Unknown iterator protocols
continue through the existing fallback. Empty tuple slices are returned only for
known tuple exhaustion, not for an unresolved rest projection.

## Evidence and review

The three measured units add 34 wrong-to-right plus 11 gap-to-right assertions,
then 27 wrong-to-right plus one gap-to-right, then 14 wrong-to-right. Combined:
75 wrong-to-right and 12 gap-to-right, with zero right losses and no other status
transitions. The denominator and corpus remain fixed. The largest tuple case,
`variadicTuples1`, now has 135 remaining wrong assertions rather than 165.

Pinned tsgo declaration controls verify the generic index, constraint iteration,
readonly slice, generic rest slice, strict optional, exact optional and nonstrict
optional behavior. Five bundled-library controls and the checker destructuring
tests pass. Release workspace tests and clippy with warnings denied pass; all
3,380 remaining upstream references resolve. The checker snapshot is refreshed.

Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread instruction still conflicts with the skill's independent review
requirements, as recorded for `18ce162e`. A manual diff scan checks constraint
cycle termination, fixed-prefix versus total-fixed-count arithmetic, saturating
slice bounds, optional flags and labels, readonly removal, unresolved spread
recovery, and the shared indexed-access metadata/cache path. This is not an
independent review. Simplification reuses tuple normalization and element-union
helpers and removes the superseded plain-tuple-only rest branch.

Scratch evidence:

- `/tmp/tsr-95-generic-tuple-index-verdict.tsv`
- `/tmp/tsr-95-iteration-constraint-verdict.tsv`
- `/tmp/tsr-95-tuple-slice-final-verdict.tsv`
- `/tmp/tsr-95-tuple-slice-final-{tests,clippy,anchors,coverage}.log`
- `/tmp/tsr-95-oracle-{iteration,tuple-slice,optional-rest}*`

Checker sources plus `trace_case.rs` SHA256:
`5fe2bb1272d3aeecb55e3efb4bbdd97acdb8478beae4b8f1b01faca0f8db6a3b`.
The goal and Beads parent `tsr-6` remain active.
