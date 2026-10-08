# r5-relater3 — relation arms that answer Unknown or the wrong verdict (`tsr-2zk.962`)

Lane under `tsr-2zk.962`. Owns relation arms in `crates/tsr-checker/src/relater.rs`
and `variances.rs`. Native anchors are `vendor/typescript-go` @ `5b1047d`.

Frozen base: `ac56208` (main). `diagverdictdump`: RIGHT 5070, EMPTY_RIGHT 5551,
WRONG 1521, EMPTY_WRONG 96. `verdictdump`: RIGHT 543119, WRONG 8301, GAP 1113.
Perf is a self-comparison against the base binary (median child CPU, new/old)
plus callgrind `Ir` of `--singleThreaded true` runs, which repeat exactly.

## 1. `tsr-2zk.929` — comparable optional-property handling

Ported as recorded in [`r4-relater2.md`](r4-relater2.md), both sites in
`Relater::properties_related_to_with_optionals`:

1. **Missing optional target member.** Native `propertiesRelatedTo`
   (relater.go:4232) sets `requireOptionalProperties` only for the subtype and
   strict-subtype relations. The arm admitted a missing optional member only
   under `Assignable`; it now admits it under every relation except the two
   subtype ones (this port has no identity relation). Comparable was the only
   relation that changed.
2. **Optional source member against a required target member.** Native
   `propertyRelatedTo` receives `skipOptional = relation == comparableRelation`
   (relater.go:4259) and tests it at :4318 for every source shape. The port
   skipped the check under Comparable only for an intersection source. The
   gate is now `relation != Comparable` alone.

Not changed: the weak-type check under Comparable is already native's
(`relater.go:2675`, unit sources only).

Measured against the frozen base: diagnostics +4 cases (`contextualTyping37`,
`optionalProperties01`, `optionalProperties02` EMPTY_WRONG→EMPTY_RIGHT;
`thislessFunctionsNotContextSensitive3` WRONG→RIGHT), types +6 lines, both loss
checks empty. Perf (21 samples): domain-model 0.904, generic-imports 0.994;
`Ir` 1,345,711,888 → 1,345,841,577 (+0.01%) and 399,687,303 → 399,684,492.
The realworld repro `assertion_to_weak_or_optional_target_is_comparable`
passes and loses its `#[ignore]`.
