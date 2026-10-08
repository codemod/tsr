# r5-tuples (`tsr-2zk.1030`)

Lane: tuple types. Owns `crates/tsr-checker/src/tuples.rs`, `spreads.rs`,
`spread_overrides.rs`, and the tuple arm of the type printer. Pinned native:
`vendor/typescript-go` @ `5b1047d`.

## Where the tuple printer actually lives

`printing.rs` has no tuple arm. TSR prints a tuple once, when it is minted.
The printed text is the `new_named` type's name. Three places do this:

- `create_tuple_type` and `create_optional_tuple_type` (`declared.rs`), for
  tuples with no spread;
- `normalize_variadic_tuple` (`tuples.rs`), for normalized variadic tuples;
- `tuple_type_node_structural`'s print-only road (`declared.rs`), for a
  generic `...T` that does not normalize away.

The native counterpart for all three is `typeReferenceToTypeNode`'s tuple arm
(`nodebuilderimpl.go:2994`). Of those, only `normalize_variadic_tuple` and the
shared `optional_tuple_element_text` are in this lane's files. Printer fixes
on the other two roads ship as diffs (below).

## 1. TupleNormalizer.add's optionality (`.16.79`, `.13.4`)

**Forcing constraint.** `TupleNormalizer.add` (`checker.go:23440`) stores
every element as `addOptionalityEx(t, true, flags&Optional != 0)`. So under
strictNullChecks an optional slot's type carries `undefined` whatever
produced it. That includes tuples inferred from optional parameters, which
never pass through `getTypeFromOptionalTypeNode`.
`genericRestParameters1.types:581` records
`(y?: string | undefined, z?: boolean | undefined) => string[]` for
`bind(g20, 42)`. TSR printed `(y?: string, z?: boolean)`, because
`normalize_variadic_tuple` kept the producer's bare type.

**Port.** `normalize_variadic_tuple` applies `get_optional_type(t, true)` to
every non-spread optional element after splicing, gated on
`strict_null_checks` as `addOptionalityEx` is. Native's later steps then
follow:

- **Optional before the last required element.** `normalize` flips the flag
  to Required and leaves the type alone. TSR used to union `undefined` here
  explicitly. Now only the flag changes, and the type keeps `add`'s
  optionality.
- **The rest collapse.** It unions `n.types[i]`, which already carry
  optionality. TSR used to push an extra `undefined` per optional element.
  That push is gone.

Both removed `undefined` unions were TSR's stand-in for `add`'s optionality.
They also ran under non-strict, where native adds nothing.

**Rejected alternative.** Adding optionality where each producer builds its
elements (the inference rest tuple, mapped tuples, and so on). Native does it
once, in the normalizer, and every producer goes through it. Several producers
here (inference, mapped) are other lanes' files. A per-producer patch would
duplicate the normalizer's rule and miss the next producer.

**Falsifier.** A tuple minted with `create_optional_tuple_type` and no spread
(the `declared.rs` flat road) does not reach the normalizer. If such a tuple
prints `T?` where native prints `(T | undefined)?`, its producer is missing
`getTypeFromOptionalTypeNode`'s `addOptionality`. That is a different
producer's bug, not this rule's.

Test: `a_tuple_inferred_from_optional_parameters_carries_undefined`
(`crates/tsr-checker/tests/tuples.rs`), which fails on the baseline.

## Diffs outside owned files

Each diff is measured on top of section 1 and sent to the integrator.

- **D1, `declared.rs` `tuple_type_node_structural` (variadic road).**
  - *Change:* apply `getTypeFromOptionalTypeNode` /
    `getTypeFromNamedTupleTypeNode`'s `addOptionalityEx(t, true, optional)`
    (`checker.go:24206`, `:24170`) to each non-rest optional element. The
    non-variadic road below already does this (commit `04b1309`). Then print
    an `OptionalTypeNode` piece through `optional_tuple_element_text`, so the
    union gets parentheses.
  - *Converts:* `[...T, number?]` → `[...T, (number | undefined)?]`
    (variadicTuples1 ×4, mappedTypesGenericTuples, mappedTypesArraysTuples ×2).
- **D2, `declared.rs` `resolved_keyof_type_worker`, plus
  `is_generic_tuple_type` in `tuples.rs`.**
  - *Change:* `shouldDeferIndexType`'s `isGenericTupleType` arm
    (`checker.go:26836`). `keyof [1, 2, ...T]` stays a deferred index type
    instead of expanding to Array's keys.
  - *Converts:* variadicTuples1 ×10.
