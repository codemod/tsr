# Compiling signature-admission handoff

At TSR `f2325620`, the private admission seam restores native TS2411 in both
the merged generic minimum and the original `subtypesOfUnion` fixture. The
final public matrix has 120 terminal CLI children and 24 case/mode comparisons:
main and seam agree with native in 20 pairs, the member prototype in 16, and
member plus seam in 20. No bounded native agreement is lost. This completes
the bounded compiling handoff for `tsr-1yb.27` and `.28`; runtime retention and
the performance goal remain open.

The [full receipt](checker-signature-admission-rust.json) and
[Rust replay patch](checker-signature-admission-rust.patch) bind all sources,
commands, binaries, outputs, controls, failures and restoration. The patch is
passive evidence; production compiler files are unchanged. It consumes the
[native contract](checker-signature-admission-native.md), native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, fixtures `4d4f005c`, and the existing
[member prototype](checker-member-admission-contextual-read.md).

## An identity proof can precede signature publication

Native `getObjectTypeInstantiation` / `getOuterTypeParameters`
(`checker.go:22304/23756`) inspect a callable's outer window before requiring
its members/signatures/return. The old Rust guard instead refused an anonymous
FUNCTION with no signature vector. Under a generic receiver mapper this turned
`typeof f` into intrinsic errorType, making the property diagnostic ineligible.

The seam checks the real TypeId/symbol pair and merged declaration owner.
It admits an uncomputed FUNCTION only when the original declaration has no
outer parameters, enclosing class/interface this, contextual generic source or
infer/mapped window. The function's own U does not belong to that outer window.
Alias/template/unresolved and pending/active/foreign-key owners remain ineligible.
This is an immutable declaration proof, not an empty-table or spelling proof.
It publishes no signature, return, object image or completed absence.

`instantiate_type` preserves direct mapper hits first. The proven original
identity then precedes the existing pending-return forcing guard. Completed or
required mapped metadata still uses that guard; active/unsupported work cannot
become a successful mapper image. No cache or new side table is introduced.
Requests for this bounded identity leave signature/return work deferred. The
parsed identity test executes 90 mapper requests and checks actual identity and
raw metadata after each; this is a test-derived count, not a workload profile.

## Identity still obeys native resource admission

Review caught an error in the initial seam: its early identity return bypassed
native `instantiateTypeWithAlias` (`checker.go:22104`) depth/count admission.
Six fresh parsed native controls with explicitly injected budgets confirm that
a nonempty mapper consumes one instantiation on success, refuses depth 100 or
count 5,000,000, and leaves member/signature metadata uncomputed. A nil mapper
precedes that budget and consumes nothing. The corrected Rust control first
fails on the actual depth bypass, then passes with the same distinctions.

These are explicit budget fault controls, not natural deep-recursion programs.
All 4,988 original native Go files are restored. The original natural native
80 controls retain their own source-bound evidence. Existing port differences
in per-checker versus native per-statement count reset and active mapper caches
are not resolved by this bounded handoff.

## Qualified tests and full public outputs

The final source passes 189 library tests, the complete checker package, strict
all-target Clippy and formatting checks. With the archived member patch applied
to frozen `f2325620`, 204 library tests pass. The member patch applies by hunks,
preserving concurrent binder/call changes; it is then reversed exactly. Its
pre-existing unused `containing` field warning remains, so the combined member
prototype is not strict-lint qualified.

Four compiling mutants fail semantic controls: omit callable identity, bypass
pending protection, ignore pending/active ownership, and bypass the resource
budget. Restoring the exact final source passes all 189 library tests. The
unmodified member prototype is also a public omission control: it drops required
TS2411 where member plus seam agrees with the complete native output, in both
worker modes. Outer T maps to number; an inner own U remains distinct.

Three complete 120-child public matrices retain all diagnostics and loaded-file
lists. Each default/single pair agrees internally. Final budget-preserving
outputs are byte-identical to the two earlier seam revisions; source/binary
bindings remain separate. Main/seam output is unchanged. Only the member-plus-
seam output gains the two TS2411 families. The circular annotation's TS2577
and `thislessFunctionsNotContextSensitive3` still differ from native.

Failed test setup is retained explicitly: SourceFile field access, a property
read that had already substituted this to its receiver, and a u64/u32 counter
mismatch. The original empty exact filter selected zero tests and is excluded;
the corrected complete restoration is the qualification. None is counted as
a semantic mutation failure.

## Remaining runtime gates

The replay applies to frozen `f2325620` and reproduces the two exact final files;
reverse application restores every baseline Rust file. Canonical Rust and all
108 bundled libraries are unchanged. Refresh current main and ownership before
applying it with the member repair. Full current type/diagnostic corpora must
lose zero previously RIGHT results, and changed-case native output plus package
and lint gates must pass before runtime retention.

The previous member loss gate remains **18 types/one diagnostic at `53826afe`**.
This turn fixes the bounded public diagnostic reproduction, not a fresh full
diagnostic corpus. The remaining signature-this/member work stays in `.33.1` /
`tsr-6.69.2`; TS2577 stays in `tsr-6.74`. Broader JS/augmentation and outer-context
admission is not certified here. PR #5's reported slowdown and the <=0.50
equivalent-work speed target are still unmet. No speed or coverage gain is claimed.

Delivery checks: 4,438 upstream anchors and the section gate pass. The issue-ID
gate retains 191 existing failures; all 737 reported citation lines are unchanged
from `f2325620`. The single-context delivery review covers five passive paths and
the exact prototype replay. Full runtime retention and performance remain unqualified.
