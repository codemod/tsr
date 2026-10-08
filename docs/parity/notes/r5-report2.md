# r5-report2: excess properties, literal elaboration, the parse-error gate

Lane continuing `r5-report` ([r5-report.md](r5-report.md)) on epic `tsr-2zk`,
vendor `5b1047d`. Issues, in order: `tsr-2zk.974` (excess-property reports),
`tsr-2zk.975` (array/object literal elaboration), `tsr-2zk.981` (the
whole-file `file_has_parse_errors` gate). Case lists come from
[r5-triage2322.md](r5-triage2322.md) buckets X2, X5 and M3.

Frozen baseline: integration head `9cccd51` plus r5-report's `6ce0ff4` and
`2394566` (both on `claude/beautiful-shannon-ar5gh0-r5-report`, not yet on the
integration branch when this lane started). Diagnostics dump: 5125 RIGHT,
1466 WRONG, 5557 EMPTY_RIGHT, 90 EMPTY_WRONG.

## 1. hasExcessProperties: the remaining declines (tsr-2zk.974)

r5-report's `6ce0ff4` runs `excess_properties_verdict` before the structural
relation for every fresh literal, and `2394566` handles optional members
(`T | undefined` constituents) and spread literals. That commit and this lane
both fixed the `undefined`-constituent decline independently; r5-report's
version is kept. This lane had also written a syntactic spread walk
(overrides read from later spreads' property names); it was dropped in favour
of r5-report's walk over the fresh type's *final* properties, which is
`shouldCheckAsExcessProperty` itself rather than a re-derivation of
`getSpreadType`'s override rules.

What was still declining, and why:

### 1a. Computed member names

`excess_properties_verdict` read every member name with `identifier_text`, so
`{ [Symbol.dispose]() {}, value: 1, extra: "" }` and `{ [sym]: "" }` were
undecidable. Native walks `getPropertiesOfType(source)`, where a computed
member is a property exactly when its name type is
`StringOrNumberLiteralOrUnique` (`checkObjectLiteral` folds any other computed
name into an index signature).

`object_literal_member_name` now spells a computed name the way
`late_bound_member_names` (`members.rs`) spells a declared type's computed
member: `property_name_from_index` for a literal name type, else
`late_bound_symbol_member_name` (`[Symbol.dispose]`, `[sym]`). A name type
that is not a literal or unique symbol is no property, and the member is
skipped. A literal or unique-symbol name that cannot be spelled declines.

`isKnownProperty` (`relater.go:719`) meets index signatures with the
property's own name type, and accepts a late-bound (symbol) name against a
`string` index signature "for backwards compatibility". `is_known_property`
keyed every name as a string literal, so `{ [sym]: "" }` against
`{ [key: symbol]: string }` or `{ [key: string]: string }` read as excess.
`is_known_property_keyed` carries the unique-symbol type for those names.
The first measurement without the key reported two false TS2353 lines in
`indexSignatures1` (308, 310), which is how the key was found to matter.

### 1b. Spelling suggestions against unions and intersections

`report_excess_property` declines when it cannot list the error target's
property names (`getSuggestionForNonexistentProperty` reads
`getPropertiesOfType(errorTarget)`). For `Cover | Cover[]` and
`A & ThisType<any>`, `get_property_names_of_type` declines on the array or
generic-interface constituent.

`property_names_for_suggestion` computes the composite's properties from its
constituents:
- union: `getPropertiesOfUnionOrIntersectionType` drops a `ReadPartial`
  property, so a union property is one every constituent has, through its own
  property, an applicable index signature, or (an object literal type without
  a spread) the implied `undefined` of `createUnionOrIntersectionProperty`.
  The candidates are one enumerable constituent's names filtered by
  `is_known_property` on the others; a constituent that cannot decide a
  candidate declines;
- intersection: every constituent's names.

A decided verdict whose suggestion still cannot be computed now falls through
to the report that ran before (`report_relation_failure` returned the
`report_excess_property` result directly, which would have silenced the old
TS2322 once the verdict became decidable).

### 1c. Empty-object exemption over mapped targets

`is_empty_object_type_for_excess` asked `relation_property_table`, which
declines every mapped type. `{ [key in `&:${string}`]: string }` resolves to
an index signature and no properties; `certified_property_names` (the
relation table, else `getPropertiesOfType`'s names) answers it.

### Remaining in .974 (not converted here)

- `excessPropertyCheckIntersectionWithRecursiveType` 13/26/39: the target is a
  conditional-type alias instantiation (`Schema1<Request>`); its resolution
  (relater's conditional arms, `tsr-2zk.976`) is upstream of the report.
- `excessPropertyCheckWithEmptyObject` 4:58: `Object.defineProperty`'s
  argument against `PropertyDescriptor & ThisType<any>`: the generic-interface
  member table, which r5-report's
  [generic-reference-table diff](r5-report-generic-reference-table.diff)
  supplies (member_completeness.rs, the integrator's).
- `objectLiteralExcessProperties` 45:76: `T` constrained to an unresolved
  `IFoo`; the pair is not reportable in TSR.
- `objectLiteralNormalization` 17: native reports TS2322 on the normalized
  union, TSR reports TS2353 at the member (literal normalization,
  `getNormalizedType` of object literals, not ported).
