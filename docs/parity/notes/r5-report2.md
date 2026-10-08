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

## 2. Literal elaboration (tsr-2zk.975)

`elaborateError` (`relater.go:440`) runs before the whole-expression report;
when it reports, the outer TS2322/TS2345 is never issued. TSR's
`elaborate_array_literal` and `elaborate_object_literal_members` declined
four shapes, so the outer head was printed instead of the member lines.

### 2a. Array literal against a union target

`elaborateElement` reads each element's target through
`getBestMatchIndexedAccessTypeOrUndefined` (`relater.go:620`): the union's own
indexed access, else the element of `getBestMatchingType`'s constituent. The
common case is an optional parameter, `[string, number, boolean] | undefined`:
the union has no property `"0"` (`undefined` makes it `ReadPartial`), so the
best match decides.

Ported:
- `union_array_literal_target_element`: the union's access when every
  constituent is an object with the element (property or index signature);
  an index a tuple-like union (`isTupleLikeType`: the union has a property
  `"0"`, which needs one tuple constituent) lacks is skipped, as native skips
  it; otherwise the best match's element.
- `best_matching_type_for_array_literal`, `getBestMatchingType` for the
  forced-tuple source (`elaborateArrayLiteral` re-checks the literal with
  `CheckModeForceTuple`: a plain, mutable, unlabeled tuple of the literal's
  arity):
  - `findMatchingDiscriminantType` cannot match: the only non-object
    constituents admitted are `undefined`, `null` and `void`, which carry no
    members, so no union property, and so no discriminant, spans them. Any
    other primitive constituent declines (a `string` constituent shares
    `length` with a tuple and can make it a discriminant);
  - `findMatchingTypeReferenceOrTypeAliasReference`: a tuple constituent with
    the forced tuple's target (same arity, all required, not readonly, no
    labels);
  - `findBestTypeForObjectLiteral` and `findBestTypeForInvokable` never match
    a tuple source;
  - `findMostOverlappyType`: the one constituent whose keys overlap the
    tuple's. `overlaps_tuple_keys` answers from certified property names: a
    numeric name, `length` or an `Array` member overlaps; no such name and no
    index signature gives a `never` overlap, and the constituent is skipped.
    Two overlapping constituents would need the unit-key counts compared, so
    they decline, as does an index signature alone (a non-literal overlap
    native does not count).
- A literal in a const context (a readonly forced tuple) declines.

`Style = StyleBase | StyleArray` (`interface StyleArray extends Array<Style>`)
is why overlap is asked of names rather than of "is an array reference".

### 2b. Spreads

- **Array spreads.** `forced_tuple_entries` builds the forced tuple's element
  list: a tuple operand's elements in place (`createTupleTypeEx` normalizes a
  variadic tuple element), an array or iterable operand as one rest element
  of `array_spread_element_type`. `forced_tuple_element` then reads node
  index `i` as native's `getIndexedAccessTypeOrUndefined(tuple, i)` does:
  before the rest element, element `i`; from it on,
  `getTupleElementTypeOutOfStartCount`, the union of the elements from `i`.
  Node indices are *not* realigned after an inlined tuple; native does not
  realign them either. A list that is one rest element and nothing else is
  the array type itself (`getTupleTargetType`), which `isTupleLikeType`
  rejects, so `[...xs]` never elaborates; the first measurement elaborated it
  and turned `destructuringArrayBindingPatternAndAssignment2` 23:5 into a
  member line. Declines: an array-literal operand of more than one element
  (native types it as a tuple, TSR as an array), optional tuples, and any
  tuple-like operand that is not a plain tuple or an `Array` reference (a
  variadic `[string, boolean, ...boolean[]]` normalizes its fixed prefix in
  place; reading it as a rest element was `spliceTuples`' one loss in the
  first measurement), a second rest element.
- **Object spreads.** `elaborateObjectLiteral` skips a spread member and reads
  every written member's source type from the final literal type. The
  decline ("a spread contributes properties this port cannot enumerate") was
  the excess check's concern, not elaboration's, and is removed.

### 2c. Numeric and computed member names

- `2.0:` binds the property `"2"` (`getPropertyNameForPropertyNameNode`);
  `written_member_name` canonicalizes a numeric literal name with
  `printing::normalise_number`. It also fixed the excess check, which read
  `1.0` as excess against `{ [x: number]: A }`.
- A computed member whose name type is a literal or unique symbol is
  elaborated (`object_literal_member_name`), with the target looked up by the
  name's own type, and its report carries TS2418 ("Type of computed
  property's value…") when the name is not a string or numeric literal
  (`ast.IsComputedNonLiteralName`). A computed member against a union target
  is still skipped, as before.

### Remaining in .975

- `didYouMeanElaborationsForExpressionsWhichCouldBeCalled` 10:8: TS2741 vs
  TS2560 for `typeof Bar` against `Bar` — the weak-type/common-property
  report order, not elaboration.
- `intersectionPropertyCheck` 7:3: the source `T & { a: boolean }` relation is
  `Unknown` (relater).

## 3. The whole-file parse-error gate (tsr-2zk.981)

### Forcing constraint

`checkSourceFile` (`checker.go:2196`) checks every statement of a file whatever
its parse diagnostics; `checkVariableLikeDeclaration`, `checkReturnStatement`,
`checkAssignmentOperator` and the rest carry no syntax gate. TSR's report
functions in `assignreport.rs` returned early whenever the file had any parse
diagnostic (`file_has_parse_errors`): assignment, `in`, for-of reference,
binding-element initializer, yield, return and arrow-body reports outright, and
variable and parameter initializers unless a source-text scan
(`has_complete_source_variable_initializer` and its four helpers, ~350 lines)
certified the declaration's own text as complete.

### What was done

The gate is removed from all nine sites in `assignreport.rs`, and the
certification scan, which only fed the gate, is deleted. Parse recovery
already hands the checker `error` types for missing nodes, and the relation
reporters treat an `error` side as related (`assignability_pair_is_reportable`),
which is native's protection too.

The checker-notes history (`checker-notes-diag2.md` §40.3, "The parse-error
gate") already measured this gate per rule: §40.3 deleted it for TS2304 at
+6, and another rule kept it at −1. Measured here for the assignment reporters:
+3 cases, no loss.

### Tests that pinned the gate

`tests/function_initializer.rs` and `tests/source_variable_initializer.rs`
(tsr-conformance) asserted the gate's declines. Their own comments record
the native answer where it differs: the bodiless function expression's TS2322
at (4,5)/(6,34)/(7,5) and the escaped `\u006fbject` binding's TS2739 at (7,5)
were "pending parity gaps, not native silence". TSR now reports exactly those,
and the assertions are updated to them. The host-identity tests now assert the
reports do not depend on the module host's source text, and the ambient
declines are unchanged. Three recovery fixtures (`MissingHeader`, `Signature`,
`ParameterRecovery`) record no native bag, so nothing is asserted of them
either way. These files are not this lane's; the change is the gate's.

### What it exposed (already WRONG, not losses)

Five TS2322 lines, all from defects outside this file that the gate had been
hiding in files that happen to carry a parse error:
- `functionsMissingReturnStatementsAndExpressions(target=es2015)` 111/115/
  120/124: an empty arrow body against a contextual `() => undefined`
  returns `void` instead of `undefined` (triage bucket X13,
  `signatures.rs` `return_type_from_body`). The file's TS1003 at 153 had
  gated every declaration in it.
- `bigintPropertyName` g.ts 26:18: a bigint-literal member name (`4n = 0`,
  `{ 4n: "" }`) binds no property natively (the TS2741s at 20:7 and 30:7 show
  the literal's `3n`/`5n` are not `"3n"`/`"5n"`); TSR binds `"4n"` on both
  sides and relates them. Binder/parser naming, not the reporter.

### The calls.rs / call_arity.rs half (not applied: not owned)

`check_call_expression_diagnostics`, `check_new_expression_diagnostics`,
`check_tagged_template_diagnostics` (`calls.rs`) and `check_call_arity`,
`check_new_arity` (`call_arity.rs`) carry the same gate. The diff removing it
is [r5-report2-calls-parse-gate.diff](r5-report2-calls-parse-gate.diff),
measured below on top of this lane's commits.
