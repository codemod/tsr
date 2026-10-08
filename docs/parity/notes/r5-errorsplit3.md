# Lane notes: r5-errorsplit3 (tsr-2zk.1009, continuing tsr-2zk.960 / .944)

Single owner of the intrinsic/error contract, step 3. Step 2 is
[r5-errorsplit2](r5-errorsplit2.md); the decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md),
whose decision log now carries the integrator's ruling on its question 1
(narrow per producer, only where it costs zero RIGHT lines). Pinned upstream:
`vendor/typescript-go` @ `5b1047d`.

## §1 Baseline

Frozen at `2919d8c` (the integration branch after r5-jsx3; r5-errorsplit2's
commits `8d7a436`/`a6bb4dd` included, its held diffs not). Types 543,275
RIGHT / 1,106 GAP / 8,152 WRONG (552,533 aligned lines); diagnostics 5,189
RIGHT / 5,557 EMPTY_RIGHT / 1,402 WRONG / 90 EMPTY_WRONG. `ceiling`: credited
gap 4,494 (151 attributed); `native_error` lines 171, all matched; narrowing
the rewrites wholesale would cost 5,057 RIGHT lines.

## §2 The `Checker::is_error` audit

### §2.1 Method

All 205 calls of `.is_error(` (203 lines; `binary.rs` and `relater.rs` each
hold one line with two) were classified against the pinned Go code: the
enclosing function, the native function it ports, and what native tests at
that point. Four classes:

- **DECLINE** — the port's "nothing computed" bail. Upstream has no test
  there; `errorType` (and an unresolved alias, `isErrorType`'s second arm)
  flows on as an any-flagged type. → `Checker::is_gap`.
- **ANY** — mirrors `IsTypeAny` / a `TypeFlagsAny`(-or-Unknown) flag test, so
  plain `any` matches natively too. → `Checker::is_type_any` (the TS2564 arm:
  upstream's `AnyOrUnknown` flags, below).
- **ERRORTYPE** / **BOTH** — mirrors native `isErrorType` (`checker.go:26638`),
  alone or while also declining the gap. → `Checker::is_error`, unchanged: it
  is native `isErrorType` with the gap counted in, because the gap stands
  where upstream may have answered `errorType`.
- **DECLINE+ANY** — declines just before the function's own `IsTypeAny` arm
  (the iteration helpers). → `is_gap`, and the adjacent `== any` arm becomes
  `is_type_any`, so upstream's `errorType` takes the all-`any` answer as it
  does natively (`checker.go:6217`, `:6225`, `:6267`, `:6463`, `:6496`,
  `:6555`, `:6650`) — and `for_of_iterated_type` answers the input itself
  (`checker.go:6096`).

Totals: DECLINE 110, DECLINE+ANY 14, ANY 37, BOTH 25, ERRORTYPE 18, UNSURE 1
(a test). Declines are 124 of 205; ADR-0048's decision log records why
neither single definition of `is_error` was right.

### §2.2 What `is_gap` means

`is_gap` is the gap intrinsic **plus the OBJECT-flagged members of
`unresolved_types`**: the deferred `keyof T` mints (`deferred_keyof_types`)
and deferred `T[K]` mints (`deferred_index_mints`). Those are this port's
placeholders for index and indexed-access types it does not represent; they
were put in `unresolved_types` so that `is_error` would treat them as
failures, and nothing upstream computes corresponds to them. Upstream's own
unresolved reference is any-flagged with an alias (`checker.go:26641`), and
those — the any-flagged members of the set — now flow past the declines.

Measured (§3): with `is_gap` the intrinsic alone, four diagnostics cases
regress, all through those placeholders reaching relations they used to
decline: `keyofAndForIn` (TS2322 on `for (k1 in obj)` with `obj: { [P in K]:
T[P] }`, through `effective_constraint_of_intersection`'s decline in
`constraints.rs`) and `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1`–`3`
(TS2339 on a destructured key-remapped mapped type).

### §2.3 Held, and why

- **`members.rs` (9 sites)** is main's active file (commits in the 24 hours
  before this lane, last `8b91cae0`). Its sites stay on `is_error`; the
  classification is in the table for whoever lands them. The property-access
  receiver arm ships as a diff (§4).
- **`symbols.rs` `semantic_iterable_yield_types_worker` (4 DECLINE+ANY
  sites) and `array_literals.rs` `array_spread_element_type`'s two
  `[Symbol.iterator]` sites** stay on `is_error`. Their `IsTypeAny` arms
  return shapes that the port's semantic helper and the spread road do not
  compute the native way (`Ok(Some(vec![iterator]))`, a fall-through to the
  protocol-failure arm), so a predicate swap alone would send upstream's
  `errorType` down a road upstream never takes. Follow-up: port those arms
  together with their predicate.
- **`declared.rs` `get_resolved_type_parameter_default` (2 sites)** stay on
  `is_error`. Natively the default is cached unconditionally
  (`checker.go:22007`), but this port's unresolved reference mints a fresh
  identity on every uncached evaluation, and
  `an_unresolved_default_preserves_its_named_identity_without_completed_reuse`
  pins that such a default is not published as completed. Caching it is a
  `declared.rs` decision (r5-typeparams2's file), not a predicate swap.
- **`operator_operands.rs` `is_type_any`** keeps its own `is_error` (it is
  the `IsTypeAny` helper; the call keeps the OBJECT-flagged placeholders
  any-like, which is today's behaviour).

### §2.4 The two hazards r5-errorsplit2 measured

- **TS2564 (`check.rs` `check_property_initialization`, `checker.go:4946`).**
  Native skips `t.flags&TypeFlagsAnyOrUnknown`. The port now asks: the gap
  needs `annotation_is_failed_reference` (decls §17's proof that the gap is
  upstream's `errorType` and not a propagated `Array<errorType>`); every
  other type is skipped by the `ANY | UNKNOWN` flag test itself, so
  upstream's `errorType`, an unresolved reference and any other any-flagged
  type are skipped as natively. `annotation_is_failed_reference`'s own test
  on the type arguments is a DECLINE (`is_gap`).
- **TS2538 (`index_access_reports.rs` `report_invalid_index_types`).** Its
  receiver guard mirrors `checkElementAccessExpression`'s
  `isErrorType(objectType)` (`checker.go:8153`) → BOTH, stays `is_error`; the
  index guard is a DECLINE (`is_gap`): native has no test on the index, and
  an `errorType` index passes `isTypeAssignableToKind` by its Any flag, which
  `is_valid_index_access_key_type` already honours.

### §2.5 Files touched by the audit (predicate calls only)

`array_literals.rs`, `assignreport.rs`, `binary.rs`, `calls.rs`, `check.rs`,
`computed_name.rs`, `constraints.rs`, `declared.rs`, `delete_operand.rs`,
`flow.rs`, `grammar.rs`, `heritage_conformance.rs`, `identity.rs`,
`import_call.rs`, `index_access_reports.rs`, `index_signatures.rs`,
`inference.rs`, `intersections.rs`, `iteration.rs`, `jsx_attributes.rs`,
`jsx_component.rs`, `jsx_intrinsic.rs`, `nonexistent_property.rs`,
`private_setter_read.rs`, `readonly_target.rs`, `relater.rs`,
`signatures.rs`, `spread_overrides.rs`, `symbols.rs`, `tuples.rs`,
`union_signatures.rs`, `using_declaration.rs`, plus `checker.rs`
(`is_gap`). Beyond the predicate call: in `iteration.rs` the adjacent
`== any` arms of the DECLINE+ANY sites, in `check.rs` the TS2564 condition,
and where an ANY site read `x == any || is_type_any(x)` the redundant
identity test was dropped. The propagation arms (§3.1) touch
`array_literals.rs`, `tuples.rs` and `indexed.rs`.

### §2.6 The table

Line numbers are at the base (`2919d8c`) except `array_literals.rs` and
`tuples.rs`, read after this lane's propagation edits; the function is the
stable key. The native anchor is the test mirrored or, for a DECLINE, the
upstream function that has none.

| site | function | class | native | predicate now | reason |
|---|---|---|---|---|---|
| `iteration.rs:219` | `get_iteration_types_of_iterable_ex` | DECLINE+ANY | `checker.go:6267` | `is_gap`, then `is_type_any` | IsTypeAny(t) all-any; port declines before its ==any arm |
| `iteration.rs:247` | `get_iteration_types_of_iterable_worker` | DECLINE | `checker.go:6287` | `is_gap` | no test upstream |
| `iteration.rs:542` | `get_iteration_types_of_generator_function_return_type` | DECLINE+ANY | `checker.go:6225` | `is_gap`, then `is_type_any` | IsTypeAny all-any |
| `iteration.rs:568` | `get_iteration_type_of_generator_function_return_type` | DECLINE+ANY | `checker.go:6217` | `is_gap`, then `is_type_any` | IsTypeAny returns nil |
| `iteration.rs:743` | `get_iteration_types_of_iterable_slow_worker` | DECLINE+ANY | `checker.go:6463` | `is_gap`, then `is_type_any` | IsTypeAny(methodType) |
| `iteration.rs:753` | `get_iteration_types_of_iterable_slow_worker` | DECLINE | `checker.go:6471` | `is_gap` | no test |
| `iteration.rs:792` | `get_iteration_types_of_iterator_worker` | DECLINE+ANY | `checker.go:6496` | `is_gap`, then `is_type_any` | IsTypeAny |
| `iteration.rs:838` | `get_iteration_types_of_method` | DECLINE+ANY | `checker.go:6555` | `is_gap`, then `is_type_any` | IsTypeAny(methodType) |
| `iteration.rs:871` | `get_iteration_types_of_method` | DECLINE | `checker.go:6597` | `is_gap` | no test |
| `iteration.rs:876` | `get_iteration_types_of_method` | DECLINE | `checker.go:6599` | `is_gap` | no test |
| `iteration.rs:922` | `get_iteration_types_of_iterator_result` | DECLINE+ANY | `checker.go:6650` | `is_gap`, then `is_type_any` | IsTypeAny |
| `iteration.rs:977` | `iterator_result_value` | DECLINE | `checker.go:6695` | `is_gap` | no test |
| `iteration.rs:991` | `iterator_result_value` | DECLINE | `checker.go:6665` | `is_gap` | no test |
| `iteration.rs:1022` | `check_iterated_type_or_element_type` | ANY | `checker.go:6096` | `is_type_any` | IsTypeAny(inputType) |
| `iteration.rs:1069` | `check_iteration_sent_type` | DECLINE | `checker.go:6132` | `is_gap` | no test |
| `iteration.rs:1222` | `check_array_binding_pattern_iteration` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny(parentType) |
| `iteration.rs:1244` | `check_array_binding_pattern_iteration` | ANY | `checker.go:6096` | `is_type_any` | IsTypeAny |
| `iteration.rs:1279` | `destructuring_assignment_source` | DECLINE | `checker.go:12552` | `is_gap` | no test |
| `iteration.rs:1298` | `destructuring_assignment_source` | DECLINE | `checker.go:12673` | `is_gap` | no test |
| `iteration.rs:1344` | `check_spread_element_iteration` | ANY | `checker.go:6096` | `is_type_any` | IsTypeAny |
| `iteration.rs:1370` | `for_in_variable_type` | DECLINE | `checker.go:16659` | `is_gap` | no test |
| `iteration.rs:1465` | `for_of_iterated_type` | DECLINE+ANY | `checker.go:6096` | `is_gap`, then `is_type_any` | IsTypeAny returns inputType |
| `calls.rs:480` | `check_super_call_diagnostics` | BOTH | `checker.go:8481` | `is_error` | !isErrorType(superType) |
| `calls.rs:655` | `check_call_expression_head` | BOTH | `checker.go:8516` | `is_error` | isErrorType(apparentType) |
| `calls.rs:664` | `check_call_expression_head` | DECLINE | `checker.go:9935` | `is_gap` | port-only guard (unsure) |
| `calls.rs:723` | `check_call_expression_head` | BOTH | `checker.go:8530` | `is_error` | !isErrorType(funcType) TS2347 |
| `calls.rs:1002` | `check_single_candidate_arguments` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1012` | `check_single_candidate_arguments` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1214` | `check_overload_candidates_arguments` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1231` | `check_overload_candidates_arguments` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1330` | `report_overload_argument_failure` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1614` | `check_call_type_argument_constraints` | DECLINE | `checker.go:9225` | `is_gap` | no test |
| `calls.rs:1625` | `check_call_type_argument_constraints` | DECLINE | `checker.go:9241` | `is_gap` | no test |
| `calls.rs:1677` | `check_instantiated_candidate_arguments` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `calls.rs:1858` | `effective_call_arguments` | DECLINE | `checker.go:30087` | `is_gap` | no test |
| `calls.rs:1921` | `has_correct_arity` | DECLINE | `checker.go:9172` | `is_gap` | no test |
| `calls.rs:2190` | `check_tagged_template_diagnostics` | BOTH | `checker.go:8723` | `is_error` | isErrorType(apparentType) |
| `calls.rs:2269` | `check_new_expression_head` | BOTH | `checker.go:8586` | `is_error` | isErrorType |
| `calls.rs:2316` | `check_new_expression_head` | DECLINE | `checker.go:8635` | `is_gap` | TS2350 reported for errorType too |
| `calls.rs:2527` | `is_untyped_signatureless_call` | DECLINE | `checker.go:9937` | `is_gap` | port-only guard |
| `calls.rs:5431` | `chooseOverload walk` | DECLINE | `checker.go:9290` | `is_gap` | no test |
| `index_access_reports.rs:72` | `report_invalid_index_types` | BOTH | `checker.go:8153` | `is_error` | isErrorType(objectType) element-access caller; decline for type-node caller |
| `index_access_reports.rs:73` | `report_invalid_index_types` | DECLINE | `checker.go:27083` | `is_gap` | no test on index; errorType passes by Any flag |
| `index_access_reports.rs:181` | `report_implicit_any_element_access` | ERRORTYPE | `checker.go:8173` | `is_error` | access answered errorType (OrElse) |
| `index_access_reports.rs:185` | `report_implicit_any_element_access` | BOTH | `checker.go:8153` | `is_error` | isErrorType(objectType) |
| `index_access_reports.rs:186` | `report_implicit_any_element_access` | DECLINE | `checker.go:27083` | `is_gap` | no test on index |
| `index_access_reports.rs:225` | `report_implicit_any_element_access` | DECLINE | `checker.go:26978` | `is_gap` | no test per constituent (ANY clause beside) |
| `index_access_reports.rs:232` | `report_implicit_any_element_access` | DECLINE | `checker.go:27084` | `is_gap` | Any|Never flag beside |
| `index_access_reports.rs:241` | `report_implicit_any_element_access` | DECLINE | `checker.go:27088` | `is_gap` | no test |
| `index_access_reports.rs:350` | `report_indexed_access_type_misses` | ANY | `checker.go:27084` | `is_type_any` | Any|Never |
| `index_access_reports.rs:351` | `report_indexed_access_type_misses` | DECLINE | `checker.go:22949` | `is_gap` | no test |
| `index_access_reports.rs:426` | `report_missing_index_signature` | ANY | `checker.go:27083` | `is_type_any` | error key like any |
| `index_access_reports.rs:430` | `report_missing_index_signature` | ANY | `checker.go:27084` | `is_type_any` | Any|Never |
| `index_access_reports.rs:519` | `check_binding_element_index_access` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny(parentType) |
| `index_access_reports.rs:603` | `check_binding_element_index_access` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny(parentType) |
| `index_access_reports.rs:849` | `object_assignment_target_source` | DECLINE | `checker.go:12597` | `is_gap` | no test |
| `jsx_component.rs:79` | `check_jsx_component_bound` | BOTH | `jsx.go:567` | `is_error` | isErrorType(apparent) |
| `jsx_component.rs:88` | `check_jsx_component_bound` | DECLINE | `jsx.go:181` | `is_gap` | no test |
| `jsx_component.rs:128` | `check_jsx_signatureless_tag` | BOTH | `jsx.go:567` | `is_error` | isErrorType(apparent) |
| `jsx_component.rs:149` | `check_jsx_signatureless_tag` | DECLINE | `checker.go:9937` | `is_gap` | no test |
| `jsx_component.rs:204` | `check_jsx_class_attributes_member` | BOTH | `jsx.go:567` | `is_error` | isErrorType |
| `jsx_component.rs:218` | `check_jsx_class_attributes_member` | ANY | `jsx.go:1014` | `is_type_any` | IsTypeAny(instanceType) |
| `jsx_component.rs:294` | `check_jsx_attributes_assignable` | ANY | `jsx.go:776` | `is_type_any` | IsTypeAny(exprType) |
| `jsx_component.rs:326` | `check_jsx_attributes_assignable` | DECLINE | `jsx.go:699` | `is_gap` | no test |
| `jsx_component.rs:327` | `check_jsx_attributes_assignable` | DECLINE | `jsx.go:699` | `is_gap` | no test |
| `jsx_component.rs:1087` | `check_jsx_element_type_constraint` | DECLINE | `jsx.go:150` | `is_gap` | no test |
| `jsx_component.rs:1132` | `jsx_element_type_type_at` | BOTH | `jsx.go:1289` | `is_error` | isErrorType(t) |
| `jsx_component.rs:1173` | `check_jsx_string_literal_tag` | BOTH | `jsx.go:1143` | `is_error` | !isErrorType |
| `jsx_component.rs:1340` | `jsx_component_bound` | DECLINE | `jsx.go:1259` | `is_gap` | no test |
| `jsx_component.rs:1349` | `jsx_component_bound` | BOTH | `jsx.go:1269` | `is_error` | isErrorType(t) |
| `declared.rs:109` | `get_resolved_type_parameter_default` | DECLINE | `checker.go:22007` | `is_error` (held, §2.3) | no test |
| `declared.rs:132` | `get_resolved_type_parameter_default` | DECLINE | `checker.go:22007` | `is_error` (held, §2.3) | no test |
| `declared.rs:197` | `instantiated_heritage_base` | DECLINE | `checker.go:19498` | `is_gap` | memo skip |
| `declared.rs:255` | `instantiated_heritage_base_worker` | DECLINE | `checker.go:21954` | `is_gap` | no test |
| `declared.rs:1933` | `get_type_from_type_reference` | DECLINE | `checker.go:23580` | `is_gap` | no test |
| `declared.rs:2608` | `build_type_literal` | DECLINE | `checker.go:19655` | `is_gap` | isValidIndexKeyType flag test |
| `declared.rs:3389` | `deferred_alias_reference` | DECLINE | `checker.go:25121` | `is_gap` | no test |
| `declared.rs:7831` | `evaluate_conditional_node` | BOTH | `checker.go:24316` | `is_error` | checkType==errorType identity |
| `declared.rs:9311` | `test` | ERRORTYPE | `checker.go:26641` | `is_error` | test |
| `declared.rs:9963` | `test` | ERRORTYPE | `checker.go:26641` | `is_error` | test |
| `declared.rs:9967` | `test` | ERRORTYPE | `checker.go:26641` | `is_error` | test |
| `check.rs:1690` | `check_extends_primitive` | ANY | `checker.go:16983` | `is_type_any` | flags&Any (flag test beside) |
| `check.rs:2068` | `check_instanceof_right_operand` | ANY | `checker.go:8806` | `is_type_any` | !IsTypeAny(rightType) |
| `check.rs:4181` | `check_property_initialization` | ANY | `checker.go:4946` | `is_gap` + `ANY\|UNKNOWN` flags | AnyOrUnknown TS2564 |
| `check.rs:5903` | `check_for_in_variable_type` | DECLINE | `checker.go:4009` | `is_gap` | no test |
| `check.rs:5975` | `check_for_in_right_operand` | DECLINE | `checker.go:4018` | `is_gap` | no test |
| `check.rs:7152` | `uninitialized_variable_reads_declared` | ANY | `checker.go:11156` | `is_type_any` | AnyOrUnknown|Void (flag beside) |
| `check.rs:9034` | `check_subsequent_declaration_identity` | BOTH | `checker.go:5929` | `is_error` | isErrorType both |
| `check.rs:13367` | `signature_parameters_assignable` | DECLINE | `relater.go:1458` | `is_gap` | no test |
| `check.rs:15229` | `annotation_is_failed_reference` | DECLINE | `checker.go:4946` | `is_gap` | port heuristic |
| `readonly_target.rs:107` | `check_readonly_assignment_target` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike (flag beside) |
| `readonly_target.rs:200` | `check_readonly_index_signature_write` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike (flag beside) |
| `readonly_target.rs:224` | `check_readonly_index_signature_write` | DECLINE | `checker.go:27083` | `is_gap` | no test |
| `readonly_target.rs:514` | `is_readonly_assignment_target` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike (flag beside) |
| `readonly_target.rs:796` | `check_private_identifier_access` | DECLINE | `checker.go:11266` | `is_gap` | any_like dup + later decline |
| `readonly_target.rs:1282` | `inaccessible_property` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike (flag beside) |
| `readonly_target.rs:1340` | `check_binding_element_accessibility` | DECLINE | `checker.go:5837` | `is_gap` | no test |
| `readonly_target.rs:1564` | `class_declared_type_text` | DECLINE | `checker.go:11837` | `is_gap` | gap only |
| `readonly_target.rs:1830` | `property_of_access_for_use_before_init` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike (flag beside) |
| `members.rs:1771` | `completed_array_placeholder_length_body` | DECLINE | `checker.go:24115` | `is_error` (unchanged: `members.rs` is main's, see §5) | port shortcut |
| `members.rs:1782` | `completed_array_placeholder_length_body` | DECLINE | `checker.go:24115` | `is_error` (unchanged: `members.rs` is main's, see §5) | port shortcut |
| `members.rs:2325` | `get_property_of_union_or_intersection_type` | ERRORTYPE | `checker.go:21469` | `is_error` (unchanged: `members.rs` is main's, see §5) | !isErrorType(t) |
| `members.rs:3200` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `members.rs:3204` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `members.rs:3277` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `members.rs:3294` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `members.rs:3296` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `members.rs:3348` | `property_names_of_type` | DECLINE | `checker.go:18867` | `is_error` (unchanged: `members.rs` is main's, see §5) | no test |
| `symbols.rs:692` | `module_clone_type` | DECLINE | `checker.go:15610` | `is_gap` | no test |
| `symbols.rs:5818` | `semantic_iterable_yield_types_worker` | DECLINE+ANY | `checker.go:6463` | `is_error` (held, §2.3) | IsTypeAny all-any |
| `symbols.rs:5838` | `semantic_iterable_yield_types_worker` | DECLINE+ANY | `checker.go:6496` | `is_error` (held, §2.3) | IsTypeAny |
| `symbols.rs:5869` | `semantic_iterable_yield_types_worker` | DECLINE+ANY | `checker.go:6555` | `is_error` (held, §2.3) | IsTypeAny |
| `symbols.rs:5897` | `semantic_iterable_yield_types_worker` | DECLINE+ANY | `checker.go:6650` | `is_error` (held, §2.3) | IsTypeAny |
| `symbols.rs:5943` | `semantic_iterable_yield_types_worker` | DECLINE | `checker.go:6695` | `is_gap` | no test |
| `symbols.rs:5958` | `semantic_iterable_yield_types_worker` | DECLINE | `checker.go:6665` | `is_gap` | no test |
| `symbols.rs:6000` | `iteration_methods_decidably_absent` | ANY | `checker.go:6555` | `is_type_any` | IsTypeAny |
| `union_signatures.rs:39` | `union_signature_for_overload_failure` | DECLINE | `checker.go:9581` | `is_gap` | no test |
| `union_signatures.rs:57` | `union_signature_for_overload_failure` | DECLINE | `checker.go:9581` | `is_gap` | no test |
| `union_signatures.rs:474` | `union_signature_types_identical` | DECLINE | `relater.go:2167` | `is_gap` | no test |
| `union_signatures.rs:670` | `combine_union_signature` | DECLINE | `checker.go:21222` | `is_gap` | no test |
| `union_signatures.rs:690` | `combine_union_signature` | DECLINE | `checker.go:21222` | `is_gap` | no test |
| `jsx_intrinsic.rs:236` | `resolve_jsx_attributes_context` | DECLINE | `jsx.go:898` | `is_gap` | no test |
| `jsx_intrinsic.rs:248` | `resolve_jsx_attributes_context` | DECLINE | `jsx.go:898` | `is_gap` | no test |
| `jsx_intrinsic.rs:251` | `resolve_jsx_attributes_context` | DECLINE | `jsx.go:898` | `is_gap` | no test |
| `jsx_intrinsic.rs:963` | `check_jsx_expression` | DECLINE | `jsx.go:95` | `is_gap` | t != anyType identity: errorType reports TS2609 |
| `jsx_intrinsic.rs:1067` | `check_jsx_intrinsic_tag_exists` | BOTH | `jsx.go:1222` | `is_error` | !isErrorType |
| `constraints.rs:91` | `effective_constraint_of_intersection` | DECLINE | `relater.go:2290` | `is_gap` | no test |
| `constraints.rs:118` | `effective_constraint_of_intersection` | DECLINE | `relater.go:2315` | `is_gap` | no test |
| `constraints.rs:828` | `check_type_argument_constraints` | ERRORTYPE | `checker.go:3000` | `is_error` | !isErrorType(t) |
| `constraints.rs:880` | `check_type_argument_constraints` | DECLINE | `checker.go:3024` | `is_gap` | no test |
| `constraints.rs:892` | `check_type_argument_constraints` | DECLINE | `checker.go:3024` | `is_gap` | no test |
| `spread_overrides.rs:68` | `check_spread_property_overrides` | ANY | `checker.go:13506` | `is_type_any` | isValidSpreadType Any |
| `spread_overrides.rs:148` | `check_object_rest_of_non_object_type` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny(parentType) |
| `spread_overrides.rs:179` | `check_jsx_spread_of_non_object_type` | ANY | `checker.go:13506` | `is_type_any` | isValidSpreadType Any |
| `spread_overrides.rs:216` | `check_spread_of_non_object_type` | ANY | `checker.go:13506` | `is_type_any` | isValidSpreadType Any |
| `delete_operand.rs:56` | `check_delete_operand_symbol` | ANY | `checker.go:10827` | `is_type_any` | AnyOrUnknown|Never |
| `delete_operand.rs:107` | `delete_operand_resolved_symbol` | DECLINE | `checker.go:10814` | `is_gap` | no test |
| `delete_operand.rs:131` | `delete_operand_resolved_symbol` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike |
| `array_literals.rs:827` | `check_array_literal_value` | DECLINE | `checker.go:8072` | `is_gap` | no test |
| `array_literals.rs:840` | `check_array_literal_value` | DECLINE | `checker.go:8038` | `is_gap` | no test |
| `array_literals.rs:866` | `check_array_literal_value` | DECLINE | `checker.go:8093` | `is_gap` | no test |
| `array_literals.rs:1302` | `array_spread_element_type` | DECLINE+ANY | `checker.go:6463` | `is_error` (held, §2.3) | IsTypeAny(methodType) |
| `array_literals.rs:1316` | `array_spread_element_type` | DECLINE+ANY | `checker.go:6496` | `is_error` (held, §2.3) | IsTypeAny |
| `tuples.rs:156` | `tuple_array_like` | DECLINE | `checker.go:23522` | `is_gap` | isArrayLikeType no test (any-flagged is array-like) |
| `tuples.rs:611` | `check_element_access_tuple_bounds` | ERRORTYPE | `checker.go:8153` | `is_error` | isErrorType(objectType) |
| `tuples.rs:646` | `check_indexed_access_type_tuple_bounds` | DECLINE | `checker.go:22949` | `is_gap` | no test |
| `tuples.rs:675` | `check_binding_element_tuple_bounds` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny(parentType) |
| `binary.rs:273a` | `check_addition` | ERRORTYPE | `checker.go:12436` | `is_error` | isErrorType(left) |
| `binary.rs:273b` | `check_addition` | ERRORTYPE | `checker.go:12436` | `is_error` | isErrorType(right) |
| `binary.rs:354` | `check_logical_or_coalescing` | DECLINE | `checker.go:12509` | `is_gap` | no test |
| `binary.rs:379` | `check_logical_or_coalescing` | DECLINE | `checker.go:12511` | `is_gap` | no test |
| `binary.rs:443` | `check_logical_and` | DECLINE | `checker.go:12496` | `is_gap` | no test |
| `binary.rs:451` | `check_logical_and` | DECLINE | `checker.go:12501` | `is_gap` | no test |
| `index_signatures.rs:318` | `completed_original_array_alias_body` | DECLINE | `checker.go:19106` | `is_gap` | port cache |
| `index_signatures.rs:323` | `completed_original_array_alias_body` | DECLINE | `checker.go:19106` | `is_gap` | port cache |
| `index_signatures.rs:328` | `completed_original_array_alias_body` | DECLINE | `checker.go:19106` | `is_gap` | port cache |
| `index_signatures.rs:494` | `index_infos_of_symbol` | DECLINE | `checker.go:19106` | `is_gap` | memo guard |
| `index_signatures.rs:796` | `late_bound_index_infos` | ERRORTYPE | `checker.go:25780` | `is_error` | IncludesError |
| `flow.rs:7010` | `intersection_has_never_discriminant` | DECLINE | `checker.go:21469` | `is_gap` | no test on prop type |
| `flow.rs:9004` | `check_all_code_paths_return_or_throw` | ANY | `checker.go:3782` | `is_type_any` | Any flag |
| `flow.rs:9005` | `check_all_code_paths_return_or_throw` | ANY | `checker.go:3782` | `is_type_any` | Any flag |
| `flow.rs:9028` | `is_unwrapped_return_type_undefined_void_or_any` | ANY | `checker.go:3782` | `is_type_any` | Any flag |
| `heritage_conformance.rs:177` | `check_class_heritage_entry` | ERRORTYPE | `checker.go:4373` | `is_error` | !isErrorType |
| `heritage_conformance.rs:420` | `check_members_for_override_modifier` | DECLINE | `checker.go:4757` | `is_gap` | no test |
| `heritage_conformance.rs:557` | `check_interface_heritage_conformance` | ERRORTYPE | `checker.go:19504` | `is_error` | !isErrorType |
| `nonexistent_property.rs:560` | `check_computed_index_implicit_any` | ERRORTYPE | `checker.go:8153` | `is_error` | isErrorType(objectType) |
| `nonexistent_property.rs:1069` | `destructured_property_is_absent` | ANY | `checker.go:17709` | `is_type_any` | IsTypeAny |
| `nonexistent_property.rs:1183` | `class_static_side_lacks` | DECLINE | `checker.go:20692` | `is_gap` | == anyType identity |
| `import_call.rs:59` | `check_import_call_specifier` | DECLINE | `checker.go:8285` | `is_gap` | no test |
| `import_call.rs:75` | `check_import_call_specifier` | DECLINE | `checker.go:8290` | `is_gap` | no test |
| `import_call.rs:79` | `check_import_call_specifier` | DECLINE | `checker.go:8290` | `is_gap` | no test |
| `assignreport.rs:2255` | `report_jsx_attributes_relation_failure` | ERRORTYPE | `relater.go:4726` | `is_error` | !isErrorType |
| `assignreport.rs:2561` | `assignability_pair_is_reportable` | DECLINE | `relater.go:209` | `is_gap` | port veto |
| `assignreport.rs:3605` | `is_discriminant_property_of_union` | ERRORTYPE | `checker.go:21469` | `is_error` | !isErrorType |
| `inference.rs:515` | `inference_spread_argument_type` | DECLINE | `checker.go:29512` | `is_gap` | no test |
| `inference.rs:2842` | `instantiate_signature_in_context_worker` | DECLINE | `inference.go:1378` | `is_gap` | no test |
| `inference.rs:2866` | `propagate_return_type_parameters` | DECLINE | `checker.go:19296` | `is_gap` | no test |
| `relater.rs:1948` | `Relater::has_members` | DECLINE | `-` | `is_gap` | port certificate |
| `relater.rs:2350a` | `one_signature_related_to` | DECLINE | `relater.go:1547` | `is_gap` | no test |
| `relater.rs:2350b` | `one_signature_related_to` | DECLINE | `relater.go:1547` | `is_gap` | no test |
| `relater.rs:3131` | `structured_type_related_to_worker` | DECLINE | `relater.go:2889` | `is_gap` | no test |
| `satisfies.rs:48` | `check_satisfies_worker` | BOTH | `checker.go:10746` | `is_error` | isErrorType(targetType) |
| `implicit_any.rs:854` | `check_implicit_any_new_expression` | BOTH | `checker.go:8586` | `is_error` | isErrorType(apparent) |
| `implicit_any.rs:864` | `check_implicit_any_new_expression` | BOTH | `checker.go:8586` | `is_error` | isErrorType then IsTypeAny 8593 (flag test beside) |
| `jsx_attributes.rs:66` | `check_jsx_spread_property_overrides` | DECLINE | `jsx.go:779` | `is_gap` | no test |
| `jsx_attributes.rs:137` | `check_jsx_children_specified_twice` | ANY | `jsx.go:776` | `is_type_any` | hasSpreadAnyType=IsTypeAny (flag test beside) |
| `mapped.rs:1004` | `instantiate_mapped_sequence` | BOTH | `checker.go:22551` | `is_error` | isErrorType(s) |
| `signatures.rs:6605` | `annotation_alias_text_at` | DECLINE | `nodebuilderimpl.go:2181` | `is_gap` | no test |
| `identity.rs:106` | `identical_to` | DECLINE | `relater.go:184` | `is_gap` | no test |
| `identity.rs:107` | `identical_to` | DECLINE | `relater.go:184` | `is_gap` | no test |
| `printing.rs:657` | `written closure` | BOTH | `pseudotypenodebuilder.go:364` | `is_error` | error type assumes equality |
| `using_declaration.rs:100` | `global_disposable_type` | DECLINE | `checker.go:1069` | `is_gap` | no test |
| `computed_name.rs:64` | `check_computed_property_name` | ANY | `checker.go:26813` | `is_type_any` | any assignable to string|number|symbol |
| `expressions.rs:4156` | `(test)` | ERRORTYPE | `checker.go:22119` | `is_error` | test |
| `expressions.rs:4169` | `(test)` | UNSURE | `-` | `is_error` | test |
| `private_setter_read.rs:35` | `check_private_setter_read` | ANY | `checker.go:11266` | `is_type_any` | isAnyLike |
| `node_reuse.rs:335` | `reuse_annotation` | BOTH | `pseudotypenodebuilder.go:364` | `is_error` | error assumes equality |
| `unions.rs:1175` | `add_type_to_union` | ERRORTYPE | `checker.go:25780` | `is_error` | isErrorType -> IncludesError |
| `base_types.rs:279` | `resolve_base_types_of_class` | BOTH | `checker.go:19249` | `is_error` | isErrorType(baseType) |
| `base_types.rs:493` | `resolve_base_types_of_interface` | BOTH | `checker.go:19504` | `is_error` | isErrorType(baseType) |
| `intersections.rs:328` | `reduce_constrained_intersection` | DECLINE | `checker.go:26140` | `is_gap` | no test |
| `intersections.rs:606` | `add_type_to_intersection` | ERRORTYPE | `checker.go:26273` | `is_error` | isErrorType -> IncludesError |
| `operator_operands.rs:661` | `is_type_any` | ANY | `utilities.go:287` | `is_error` (inside `is_type_any` itself) | IsTypeAny itself |
| `grammar.rs:1164` | `check_catch_clause_declaration` | ANY | `checker.go:4254` | `is_type_any` | AnyOrUnknown (flag test beside) |
| `variances.rs:188` | `create_variance_marker_type` | BOTH | `relater.go:1416` | `is_error` | isErrorType(declared) |

### §2.7 Sites added by the merge of the integration branch (`a1e453dc`)

r5-relater4's `type_related_to_discriminated_type` (`relater.go:1077`) and
its `is_discriminant_property_of` (`relater.go:1087`) brought three new calls;
r5-intersections' `reduce_constrained_intersection` rewrite removed one
(DECLINE, already `is_gap`). The merge conflict in `iteration.rs`
(`get_iteration_types_of_iterable_ex`) took r5-iteration's
`iteration_reduced_type` step and this lane's `is_type_any` arm.

| site | function | class | native | predicate now | reason |
|---|---|---|---|---|---|
| `relater.rs:4474` | `type_related_to_discriminated_type` | DECLINE | `relater.go:1077` | `is_gap` | the discriminant's source property type has no native test; only the gap is undecidable |
| `relater.rs:4636` | `is_discriminant_property_of` | ERRORTYPE | `checker.go:21469` | `is_error` | `createUnionOrIntersectionProperty` skips `isErrorType` constituents |
| `relater.rs:4645` | `is_discriminant_property_of` | DECLINE | `checker.go:21469` | `is_gap` | no native test on the member type |

## §3 Measurements

### §3.1 Commit 1: the audit and the propagation arms

What the commit carries besides the predicate swaps (§2):

- **`array_spread_element_type`** (`array_literals.rs`): spreading upstream's
  `errorType` contributes it unchanged (`checkIteratedTypeOrElementType`'s
  `IsTypeAny(inputType)`, `checker.go:6096`).
- **The destructuring-target road** (`array_literals.rs`, the
  assignment-target arm of `check_array_literal`): upstream's `errorType` is
  array-like (`isArrayLikeType`, `checker.go:23520`) and stays Variadic;
  `normalize_variadic_tuple` (`tuples.rs`) makes an any-flagged Variadic a
  Rest of itself (`checker.go:23372`) for it as for `any`. This is
  r5-errorsplit2's last final-`else` loss
  (`assignmentRestElementWithErrorSourceType`, `[...c] : any[]`).
  `tuple_array_like` itself is not widened: it is the array/tuple half of
  `isArrayLikeType` and answers false for plain `any` too, so making it true
  for `errorType` alone would split the two any-flagged identities at every
  other caller.
- **Element access** (`indexed.rs`): an `errorType` receiver answers itself
  after the index is checked and without flow narrowing
  (`checkElementAccessExpression`, `checker.go:8151-8153`).

Measured unfiltered against §1's base, both dumps: **zero transitions of any
kind** (types 543,275 / 1,106 / 8,152; diagnostics 5,189 / 5,557 / 1,402 /
90), so both loss checks are empty. `ceiling` is unchanged: credited gap
4,494, `native_error` lines 171, narrowing cost 5,057. That is the expected
result: at this base only the three switched `checkIdentifier` arms answer
`native_error`, and none of their consumers reach a swapped site. The
commit is the precondition for the switches that follow, which is where its
effect shows.

`is_gap` as the bare intrinsic measured 4 diagnostics losses (§2.2) and 7
type gains (3 GAP→RIGHT, 4 WRONG→RIGHT) against 12 GAP→WRONG, all through
the deferred mints. With the mints in `is_gap`: zero transitions. The gains
are not taken: they came with the losses and with more gaps turned wrong
than right.

Tests: `cargo test --workspace --release` passes (the
`member_completeness` failure on `2919d8c` is fixed by the integration
branch's `f4ae684b`, merged here). One test caught a swap:
`an_unresolved_default_preserves_its_named_identity_without_completed_reuse`
(§2.3). Clippy (stable) reports nothing on touched lines; fmt clean.

Perf (median child CPU, new/old, the frozen base binary in the `--tsgo`
slot): domain-model 1.002 (21 samples); generic-imports 1.070 at 21 samples,
**0.990 at 41** (its check phase is about 1 ms of an ~80 ms run, r5-perf4).
`diagnostics_match: true` on both.

### §3.2 Rebased on the integration branch (`a1e453dc`): the flow leak closes

The integrator landed r5-errorsplit2's flow, P4 and empty-name diffs. The
base re-frozen there: types 543,727 RIGHT / 1,026 GAP / 7,780 WRONG;
diagnostics 5,304 RIGHT / 5,576 EMPTY_RIGHT / 1,287 WRONG / 71 EMPTY_WRONG.
`ceiling`: credited gap **14,715**, `native_error` lines 10,892, wholesale
narrowing cost 15,385 — the flow diff's 10,000 `largeControlFlowGraph`
`data[0]` lines, whose `native_error` receiver answered the gap through the
element-access road, exactly as r5-errorsplit2 §7 predicted.

This lane's tree merged onto it (plus §2.7's three sites), unfiltered, both
dumps: **zero transitions**, both loss checks empty. `ceiling`:

| | base `a1e453dc` | this lane |
|---|---:|---:|
| credited gap (matched lines whose type is the gap) | 14,715 | **4,710** |
| `native_error` lines (matched) | 10,892 (10,851) | 20,895 (20,854) |
| wholesale narrowing, RIGHT→GAP | 15,385 | 5,380 |

The 10,005 lines left the gap through `check_element_access_type`'s
`errorType` receiver arm (`checker.go:8152`): the element access now answers
upstream's identity and the SS180 rewrite prints it `any` as upstream's
writer does, so the line is matched by computation rather than credited.

Tests pass; perf (median child CPU against the `a1e453dc` binary, 21
samples): domain-model 0.954, generic-imports 1.007.

## §4 Held diffs

- [`r5-errorsplit3-final-else.diff`](r5-errorsplit3-final-else.diff), applies
  to `89f342f8`. It carries the producer switch and every consumer arm the
  switch needs, all native-verified (§6):
  - **`expressions.rs` `checkIdentifier`** (this lane's arm): the final
    `else` answers `native_error` (`checker.go:11048`), and §784's JS clause
    leaves the §31 gate (§6). `file_has_commonjs_machinery` and its memo
    field lose their last caller and are removed.
  - **`members.rs` `check_property_access_expression`**: an `errorType`
    receiver answers `errorType` (`checker.go:11314-11320`).
  - **`calls.rs` `check_call_expression_worker`**: an `errorType` callee is
    `resolveErrorCall` (`checker.go:8516`, `unknownSignature` returns
    `errorType`, `:1043`), before the untyped-call `any`.
  - **`expressions.rs` `check_new_expression`**: the same for `new`
    (`checker.go:8586`).
  - **Four tests** that pinned the stand-in: `types.rs`
    `an_unresolved_name_is_upstreams_any` (renamed `…_error_type`),
    `unresolved_identifier_in_js.rs` (§784's split, superseded in its own
    header), `narrowing.rs`'s helper, and `reader_error_any.rs`'s identity
    classifier (both identities are the computed error-any).

  `members.rs` and `calls.rs` are main's active files (commits on
  `origin/main` in the 24 hours before this lane, re-checked before
  shipping), and `check_new_expression` is outside this lane's arms, so the
  set is a diff for the integrator. It is measured whole: without the
  `members.rs` arm the switch loses (§5).

## §5 The final `else` switch, measured

Measured three ways, unfiltered, both dumps:

| tree | base | type losses | diag losses | other transitions | credited gap | `native_error` lines (matched) | narrowing RIGHT→GAP |
|---|---|---:|---:|---|---:|---:|---:|
| commit 1 + switch + `members.rs` arm | `2919d8c` | 0 | 0 | +2 WRONG→RIGHT, 5 WRONG→GAP | — | — | — |
| commit 1 + switch, **no** `members.rs` arm | `2919d8c` | **2** | **1** | as above | — | — | — |
| `ca1e75a6` + switch + `members.rs` arm (the diff) | `a1e453dc` | 0 | 0 | +2 WRONG→RIGHT, 1 WRONG→GAP | 4,710 → **4,554** | 20,895 → 24,385 (24,310) | 5,380 → 5,158 |

- **r5-errorsplit2's residual is gone.** `assignmentRestElementWithErrorSourceType`
  (`[...c] : any[]`) holds: the destructuring-target road and the
  normalizer arm of §3.1 carry upstream's `errorType` as a Rest of itself.
- **The `members.rs` arm is required.** Without it the property-access
  road answers the gap for an `errorType` receiver and the gap's contextual
  decline drops `parser509534:0:5`/`:0:11` (the function assigned to
  `module.exports.route`) and adds `destructuringParameterDeclaration4`'s
  extra TS2345 — r5-errorsplit2's §2 finding, reproduced.
- **Gains:** `jsFileCompilationExternalPackageError:2:6` (`c : error`, a case
  with no `.errors.txt`: upstream's `errorType` takes the writer's fast path)
  and `typeofInObjectLiteralType:0:0` (`c: typeof b` inside a type literal).
- **The WRONG→GAP lines are false `native_error` claims**, not upstream's
  `errorType`: names the port fails to resolve reach the final `else`.
  - `jsDeclarationsComputedNames(target=es2015):1:13`: a JSDoc
    `@param {typeof TopLevelSym | typeof InnerSym}` whose names resolve
    natively (`unique symbol | unique symbol`). JSDoc name resolution is
    not this lane's.
  - On `2919d8c` also `importMetaNarrowing` ×4, the `meta` name of
    `import.meta` reaching `checkIdentifier`. Gone on `a1e453dc`:
    r5-modules ported `checkMetaProperty`.

  Before the switch these lines answered `any` (WRONG). Now they print
  `error` (GAP), which is the honest verdict. The identity claim is still
  wrong, so they are listed here rather than counted as converted.
- **P10 (the return aggregate) needs no code.** With the switch,
  `function r() { return nosuch; }` aggregates `native_error`, which
  `awaited_type_no_alias` passes through (any-flagged) and
  `inferred_return_type` keeps. Its `error → any` stand-in still applies to
  the gap only, which is the native-verified rule (r5-errorsplit2 §6).
- **P11/P12 are left alone**, per r5-errorsplit2 §6: natively they are the
  unresolved-reference type, not `errorType`.

## §6 The §31 gate's JS clause, and the call/`new` arms

**The JS clause.** §784 sent an unresolved name in a `.js` file *without*
CommonJS machinery to the gap, because the final `else`'s `any` stand-in
was wrong there: upstream reports TS2304 and its writer records `error`
(`parsingDeepParenthensizedExpression`, `spellingUncheckedJS`: unchecked
JS, no `.errors.txt`). The final `else` now answers `native_error`, which
*is* that `errorType`, so the clause's reason is gone. A JS file with no ES
import machinery is the final `else`'s case as much as a TS file is. The
native probe (r5-errorsplit2 §1 method; source in §7) agrees:

| fixture | native |
|---|---|
| `var x = nosuch;` in a plain `.js` | `nosuch`, `x`: `GetErrorType()` |
| `var y = nosuch3;` in a `.js` with `require("fs")` / `module.exports` | `nosuch3`, `y`: `GetErrorType()`; `require`: `GetAnyType()` |
| `nosuch2.foo`, `nosuch6.a.b`, `nosuch7[0]` | every node `GetErrorType()` |
| `new Nosuch()` | `GetErrorType()` |
| `function r() { return nosuch4; }; r()` | `r()` `GetErrorType()` (P10's aggregate) |

**The call and `new` arms.** The JS clause alone measured **11 RIGHT→WRONG**
(`parsingDeepParenthensizedExpression` ×6, `spellingUncheckedJS` ×5). All
eleven are calls or `new` on an `errorType` callee
(`inmodule.toFixed()`, `b(288)` inside `… && b(288)`), in cases with no
`.errors.txt` where upstream prints `error`. The port's call road went
straight to `resolveUntypedCall`'s `any`, while upstream asks
`isErrorType(apparentType)` first and takes `resolveErrorCall`. With the
call arm the eleven hold and nine more lines convert.

**Measured** (the diff on `89f342f8`, against `a1e453dc`, unfiltered, both
dumps):

| | |
|---|---|
| type losses / diagnostics losses | **0 / 0** |
| WRONG→RIGHT | 9 (`parsingDeepParenthensizedExpression` ×7, `jsFileCompilationExternalPackageError`, `typeofInObjectLiteralType`) |
| WRONG→GAP | 3 |
| credited gap | 4,710 → **4,243** |
| `native_error` lines (matched) | 20,895 → 25,910 (25,828) |
| wholesale narrowing, RIGHT→GAP | 5,380 → 5,105 |

Tests pass with the diff applied. Clippy reports nothing on its lines.
Perf (median child CPU against the `a1e453dc` binary, 21 samples):
domain-model 1.023, generic-imports 0.967.

**The three WRONG→GAP lines are false `native_error` claims**: port misses
that reach the final `else`. Before they answered `any` (WRONG); now they
print `error` (GAP).
- `jsDeclarationsComputedNames(target=es2015):1:13`: JSDoc `typeof`
  names (§5).
- `dynamicImportsDeclaration:3:0`/`:3:1`: `import("./case0.js")` under
  `module: nodenext` reaches the identifier road. That is the dynamic-import
  call, which is r5-modules' area.

## §7 Probe

`/tmp/claude-0/probe/main.go`, built into the pinned module with
`go build -modfile=$WORK/tsgo.mod -overlay overlay.json ./cmd/errprobe`, the
overlay mapping `cmd/errprobe/main.go` to it (nothing tracked is touched;
`$WORK` is `scripts/offline-cargo/build-tsgo.sh`'s). For a directory with a
`tsconfig.json` it:
1. builds the program single-threaded;
2. prints each root file's semantic diagnostics;
3. walks every identifier and expression node, printing whether
   `GetTypeAtLocation` is `GetErrorType()`, `GetAnyType()` or another type
   (with `TypeToString`).

## §8 Narrowing (ADR-0048 decision log, question 1)

After each step, `ceiling`'s narrowing table: the RIGHT lines that print
`any` only because a writer rewrite spelled the port's gap that way.

| step | credited gap | RIGHT→GAP if narrowed | by rewrite (HadErrorBaseline / AtLocation / StatementName / AccessOrQualifiedParent / GlobalAugmentation) |
|---|---:|---:|---|
| base `2919d8c` | 4,494 | 5,057 | 3,531 / 1,113 / 312 / 78 / 23 |
| commit 1 on `2919d8c` | 4,494 | 5,057 | unchanged |
| base `a1e453dc` (flow, P4, empty name landed) | 14,715 | 15,385 | — |
| `ca1e75a6` (commit 1 merged) | 4,710 | 5,380 | 3,744 / 1,223 / 312 / 78 / 23 |
| + final-else diff (§4, §6) | 4,243 | 5,105 | 3,549 / 1,146 / 312 / 75 / 23 |

**No narrowing is landed.** Every rewrite still prints RIGHT gap lines, so
narrowing any of them, even the smallest (`GlobalAugmentation`, 23), costs
RIGHT lines, and the integrator's rule allows only zero-cost narrowing.
Narrowing per producer happens through the switches themselves: each
producer that moves to `native_error` leaves the table. That is the
10,005 + 467 lines of §3.2 and §6.

The residual's largest producer rows after the diff:
- 838 lines, references the §31 gate keeps as the gap: a name that resolves
  in another meaning, `arguments`, or a file with ES import machinery;
- 544 lines, declaration names of aliases this port does not type;
- 423 lines, `FUNCTION_SCOPED_VARIABLE` declaration names, whose
  initializer gapped.

The first is the next producer for this lane's arm. Its decidable
sub-population is the name that resolves only to a type-like meaning that
can never merge with a value. It needs per-meaning native verification and
is not attempted here.
