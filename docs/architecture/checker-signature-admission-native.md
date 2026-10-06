# Native signature admission and deferred return forcing

The native phase of `tsr-1yb.27` now has **80 passing parsed-program controls**
and three detected compiling mutations. The run is bound to TSR `51960e07`,
native `5b1047d10d32e7d5b446be4de56b126ff42f82bb` and fixtures `4d4f005c`.
It defines the next Rust admission seam; it does not implement or compile-qualify
that seam. `.27` remains in progress, `.28` remains open and production reuse is
still gated. There is no runtime, coverage or speed gain.

The [receipt](checker-signature-admission-native.json) preserves complete
sources, commands, raw events, failed expectation, mutations, CLI output and
restoration hashes. The [observer/test patch](checker-signature-admission-native.patch)
applies to a fresh native `5b1047d` archive. It is passive evidence, not compiler
code loaded by TSR. The [member replay](checker-member-admission-contextual-read.md)
keeps its existing 18 type/one diagnostic loss gate unchanged.

## Missing metadata can precede a supported identity answer

The archived [C3 cause](checker-inherited-this.md#a-newly-demanded-mapper-refuses-a-merged-function-value)
already identified missing `typeof f` TS2411 when f merges function and namespace.
The current member replay reproduces that boundary. A generic interface's this
mapper reaches `instantiate_type`; its pending-return guard sees an Anonymous
function without a `signature_types` entry and refuses it. The property becomes
intrinsic errorType, so the index checker declines the diagnostic.

Native `instantiateTypeWithAlias` (`checker.go:22104`) first applies type-variable
admission. For these anonymous function values, `getObjectTypeInstantiation`
(`:22304`) then checks outer type parameters. A top-level function has none and
retains its identity without resolving signatures or return. Missing metadata
is therefore an uncomputed state, not evidence of unsupported work.

The cold merged control begins with MembersResolved false and zero signatures.
A this-only map preserves the same Type pointer and those fields. The explicit
later `getSignaturesOfType` demand resolves one declaration signature with a nil
return. Only `getReturnTypeOfSignature` (`:20001`) executes the return worker and
publishes number. Warm signature/return queries execute no additional workers.
The repeated mapper may call native's object-instantiation worker again; no claim
is made that its whole method or outer-parameter walk disappeared.

## Mapping and recursion keep their own publication states

For a function capturing outer T, mapping T to number creates a distinct anonymous
target whose signature and return remain deferred. Later member resolution
retains a signature target; return demand resolves the original T, maps it and
publishes number. An inner function's own U remains U with one own parameter.
Nil and identity maps retain their original Type pointers. These are actual
native objects, not manufactured getter records.

The producer covers merged, plain, annotated, ambient, direct recursive,
circular-annotation, unresolved-name, outer-generic and inner-generic shapes.
Fresh cold, signatures-first, return-first and checked-first programs execute
the relevant this-only, nil, outer-number and no-op mapper combinations. Raw
snapshots carry Program and Checker pointers, type/signature identities, member
flags, target presence and the actual resolution stack. Observer events add no
semantic queries; explicit producer queries and rendering remain recorded.

The initial direct-recursion expectation was wrong. `function f(){return f()}`
computes never and is accepted by native, without failed return re-entry. The
original 60-row failed run is preserved. The corrected control retains that
case and separately adds `function f():ReturnType<typeof f>{return f()}`.
Its actual return-resolution stack re-enters an active signature, returns error
to that inner query, and eventually publishes any. Native reports TS2577. An
unresolved-name return also prints any but is *intrinsic errorType*, a distinct
completed failure. Formatting is not an admission or completion predicate.

## Mutation and public qualification

Three mutants compile and fail their controls: eager signature/return forcing
during mapping, skipped merged-function signature resolution, and premature any
publication that erases natural active re-entry. Restoring the exact qualified
sources repeats all 80 controls. The original baseline then restores all **4,988
Go files and 108 libraries** exactly; no canonical vendor or Rust source changes.
The initial bundled-library copy failure is a setup error, recorded separately.

Ten public families produce **60 terminal CLI children and 20 worker-mode pairs**.
Complete diagnostics and loaded-file lists agree within default/single modes.
Main matches native in 18 pairs; the private member replay matches in 16. Both
merged nongeneric and plain-function controls retain TS2411. Only the merged
generic private replay drops it. Main and private replay both omit the circular
annotation's native TS2577; the separate existing-behavior gap is filed as
`tsr-6.74`. Generic positive/negative, inner-generic, unresolved-name and direct
never controls preserve their complete native output.

Pinned ordinary native and previously built baseline/private CLIs are reused;
all 650 baseline Rust hashes match frozen `51960e07` and the binary hashes are checked.
These are bounded public diagnostics, not a fresh full-corpus or performance run.
The delivery pull advanced main to `3aa6b319`, including binder and call-checking
changes. Those changes are preserved but are outside this receipt's qualification;
refresh the Rust baseline before implementing the dependent seam.

## Exact Rust requirements for the next seam

At `inference.rs::instantiate_type` and
`signatures.rs::complete_pending_signature_returns_of_type`, preserve direct
mapper hits and distinguish these states before deciding eligibility:

- An absent signature vector remains uncomputed. A proven native no-outer-
  parameter callable identity may pass through without forcing or publishing
  a completed signature result; do not generalize this to anonymous values whose
  lexical/alias context is unresolved.
- Required outer mapping retains original owner and mapper identity. The inner
  U control prevents accidental substitution by an outer same-spelled parameter.
- Pending and active originals cannot be treated as completed success. Preserve
  captured alias keys and the existing parameter-only active-reader refusals.
- Return completion distinguishes a supported result, an intrinsic error result
  and natural recursive re-entry. A string spelling, empty vector or nil result
  cannot license reuse or erase a required property diagnostic.

Task `.28` must add the reproducing Rust control and one bounded compiling seam
in existing test homes, then qualify active/pending/no-op and query-order behavior.
Full type/diagnostic no-RIGHT-loss gates, complete changed-case native output and
package/lint checks remain required before runtime retention. Preserve receiver
`tsr-6.69.2`, mapper ownership and the original `.33.1` replay. This contract adds
no cache and does not resolve the reported PR #5 slowdown or the <=0.50 speed target.

## Delivery checks

The published patch applies to the restored archive and reproduces all three
qualified source files exactly. Reverse application restores every original Go
file and library. Documentation anchors (4,423 references) and section checks
pass. The issue-ID gate fails on 191 existing IDs; all 737 reported citation
lines are unchanged from `51960e07`. This is a scoped check, not a global pass.
The single-context delivery review covers these five passive paths. Rust package,
full-corpus and performance checks are not part of this native contract phase.

## Compiling Rust continuation at `f2325620`

The [bounded Rust handoff](checker-signature-admission-rust.md) completes the
remaining compiling seam for `.27` and `.28`. It preserves deferred callable
identity and pending/active ownership, and qualifies native nil versus nonempty
mapper resource admission. The earlier source-bound native phase remains intact.
Only the bounded handoff is complete; production reuse and speed gates remain.
