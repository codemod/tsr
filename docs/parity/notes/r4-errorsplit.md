# Lane notes: r4-errorsplit (tsr-2zk.944)

Single-owner box for the error-type contract: the port's "could not compute"
gap and upstream's `errorType` become two intrinsics. The decision record is
ADR-0047, written with this lane's final numbers; it supersedes
[ADR-0038](../../adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md).
Pinned upstream: `vendor/typescript-go` @ `5b1047d`. Baseline frozen at
`0399578` (integration head): types 470,235 RIGHT / 893 GAP / 6,851 WRONG;
diagnostics 4,343 RIGHT / 4,979 EMPTY_RIGHT.

## §1 Step 1: the split, with no producer moved

**Which identity keeps the old name.** `Intrinsics::error` has 579 uses in 40
files, most of them in hub files every lane edits (`declared.rs` 102,
`symbols.rs` 66, `signatures.rs` 57, `expressions.rs` 55). Every one of them
today means "the port's gap" — that is what the type has been. Renaming the
field to `gap` and reusing `error` for upstream's type would be the tidier
spelling, but it touches every one of those lines in one commit and conflicts
with every concurrent lane. So the **existing** intrinsic stays `error` and is
documented as the gap, and upstream's type is the **new** field
`native_error`. Every existing use is therefore "moved to the gap" by
construction, with a zero-line diff.

**What `native_error` is.** `TypeFlagsAny` (so `IsTypeAny` holds), created right
after the gap (upstream creates `errorType` at `checker.go:979`), with printed
name `any`: the node builder renders every `TypeFlagsAny` type as the `any`
keyword, and only the baseline writer's intrinsic-name fast path
(`type_symbol_baseline.go:378`) prints `errorType`'s intrinsic name `"error"`.
That fast path is `types_producer::render`, which restores `"error"` by
identity; the writer guards then route guarded positions (and every line of a
case with an `.errors.txt`, SS180) to `any` exactly as they did for the gap.

**What treats it as an error.** `Checker::is_error` (upstream `isErrorType`)
answers true for both identities. Union and intersection construction
(`IncludesError`, `checker.go:25659` / `:26092`) answer upstream's `errorType`
while every error constituent was upstream's, and the gap as soon as one was
not (`Checker::included_error`): a gap in a constituent is a gap in the whole.
The other 274 `== intrinsics.error` comparisons are left as they are: each
producer switched in step 2 has its consumers checked and measured.

**Measured.** Both dumps byte-identical to the baseline (`cmp`), as required.

**The new instrument.** `types_producer::assertions_for_case_with_error_kinds`
reports each line's top-level identity (gap / upstream `errorType` / other)
from the very walk that rendered it, and `examples/ceiling.rs` now prints the
split. At the baseline:

| population | lines |
|---|---:|
| upstream exactly `any`, this port `error` (ADR-0038's bucket) | **36** (ADR-0038: 35,508 at its time) |
| matched lines whose top-level type is the **gap** (printed `any` by a writer guard) | **4,644** |
| of those, with a name-resolution error on the same source line | 152 |
| gap lines not matched | 2,214 |

The 4,644 are the population ADR-0038 refused to create, and it exists anyway:
SS180 (`hadErrorBaseline`) and the positional guards print the gap `any`, and
those lines match. Until this split nothing could count them.

## §2 Not done this round (stopped at the integrator's wrap-up)

Step 2 (switching producers to `native_error`, class by class) and the ADR
superseding 0038 are **not built**. What the round established for them:

- **(a) unresolved names.** `checkIdentifier`'s unresolved exit
  (`expressions.rs`, the `Expression::Identifier` arm after the §31 gate)
  already answers `intrinsics.any` as a stand-in for `errorType` where the
  miss is deterministic, and the gap where the port might be the one failing.
  The faithful switch is `any` → `native_error` on the deterministic arms
  only (the §475 type-parameter arm, the empty-name arm, the final `else`).
  Type lines barely move (both print `any`); the effect is on `== any`
  identity tests and on `isErrorType`-gated diagnostics, so it needs the
  diagnostics dump.
- **(b) TS2563.** `flow.rs` `get_type_at_flow_node` and the
  `flow_disabled_containers` arm of `get_flow_type_of_reference_ex` answer
  `any` (TS) / gap (JS, §14.1) for upstream's `errorType`. With the writer's
  fast path now restoring `"error"` by identity, both halves become
  `native_error`. The §14.1 JS split then goes away.
- **(c) P4/P10/P11/P12** (r4-anyaudit §2): not verified against native yet.
- **The writer's gap rewrites** (SS180 and the positional guards in
  `render_case`) print the gap `any`. That is where the 4,644 falsely credited
  lines come from. Once producers answer `native_error`, those rewrites can be
  narrowed to it. That turns falsely-credited RIGHT lines into GAP, so it is a
  §5 gate decision for the integrator, not a box's.

Pre-existing test failures seen at the baseline, not caused here:
`tsr-checker/tests/globals.rs` (2 tests: the store holds 29 intrinsics, the
test asserts 26 — the three zero literals) and
`tsr-conformance/tests/undefined_widening_modes.rs` (the same count). With this
lane the store holds 30; the tests need updating by their owner.
