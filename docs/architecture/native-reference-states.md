# Native reference publication states

A cached reference handle does not prove that its members are resolved.
A cached error can be a legitimate returned answer, while a depth refusal can
occur before an existing active-cache answer is read. Reuse and its observer
must preserve these distinctions.

These private API controls pin native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The Rust source inventory freezes
`b8cbb92a999961b15e6a0f69588319e96e57192e`. They advance
**tsr-1yb.16.1.2.2**; they do not complete its actual Rust observer, public
diagnostic or default/single pool obligations, and establish no speed benefit.

| Controlled operation | Distinction the observer must retain |
| --- | --- |
| First `createTypeReferenceEx` request | One new handle is installed in the target-owned table with `MembersResolved` unset. |
| Repeat reference request | Same handle, no new type factory; members can still be unresolved. |
| Explicit `setStructuredTypeMembers` publication | The existing handle acquires resolved members without becoming a different reference identity. This test calls the primitive directly. |
| Empty arguments versus an error argument | Empty slices share a reference; a reference carrying an error argument is different from the intrinsic error value. |
| `tryCreateTypeReference` on the empty generic target | Nonempty arguments return unknown without a new handle or table entry. A later valid empty request can publish its own handle. |
| `getTypeAliasInstantiation` returning error | The alias-owned table can retain error. Repeats avoid another worker; a different argument tuple is evaluated separately. |
| Returned error under an active mapper | The nested error is reused within that frame. Pop clears it, allowing a later callback answer. |
| Recursive active reentry | A returned inner answer can be reused before the outer request finishes; entry itself is not a published answer. |
| Depth refusal with a cached active answer | Refusal returns error and TS2589 before cache lookup, without another worker or replacing the earlier answer. Removing the forced depth restores the hit. |
| Composition and ownership | Recursive composition reaches the concrete target reference; direct merge retains the first image. Both can remain lazy. Ordered arguments, target identity and checker identity remain distinct. |

Native reference creation is at `checker.go:25107`; the empty-generic refusal
is at `:25096`. Alias instantiation at `:23641` first resolves the declared
owner and publishes the returned result. Active instantiation at `:22104`
checks depth/count refusal before mapper lookup, publishes a returned nested
answer, and clears the frame at pop. Member publication at `:25145` changes
the resolved flag. The [receipt](native-reference-states.json) retains exact
function bodies, source hashes and line anchors, rather than applying these
lines to later revisions.

The current Rust reference producer, `declared.rs:5378`, combines several
routes in a checker-owned `(SymbolId, Vec<TypeId>)` table. String/identity
mapping can publish before the ordinary lookup. A named handle is inserted
before target metadata, mapped capture and sequence evaluation; subsequent
evaluation can replace that answer. NonNullable at `:5553` returns error
without publication when alias evaluation refuses. Other paths can publish
an evaluated error. These source observations require branch-forcing controls
before they become dynamic state evidence.

The existing mapper observer's `reference_active_hits` means a matching
diagnostic frame is present; `reference_nonactive_hits` means it is absent.
Neither name proves native member completion. The next observer extension
must separately record request, map answer, actual worker entry, handle
reservation, return/error/refusal and subsequent publication, with checker and
table identity. It must cover pre-lookup mapping and direct NonNullable paths,
not only the ordinary reference miss block. The completed
[construction inventory](checker-reference-key.md) supplies the call sites;
its bytes and clocks are not a semantic completion or saved-wall estimate.

Run the isolated controls with an installed Go 1.26 toolchain and cached
dependencies:

```sh
python3 docs/architecture/native-reference-states-controls.py \
  --native-source /path/to/clean/pinned-native-clone \
  --go /path/to/go1.26/bin/go \
  --output /tmp/new-reference-state-results
```

The driver refuses the artifact repository, wrong revision, dirty source and
existing output. It runs the focused tests five times, race detection, the
affected checker package and vet, then compiles two deliberate mutations:
falsely marking a reference's members resolved and changing a published active
error to unknown. Each mutation must fail an intended state test. It supervises
and reaps each compiler process before restoration, preserves raw outputs and
verifies that the native checkout is clean afterward. Builds and test times
are not performance samples.

The portable run passes all nine focused tests five times, race detection,
the checker package and vet. False member completion fails three intended
tests; replacing a cached active error fails one. Four actual driver refusals
preserve source/output state. The initial existing-output guard used mismatched
paths and accidentally ran another positive suite; its wrapper failure and
successful, restored replay are retained separately. Both isolated native
checkouts are restored and clean; canonical native and production Rust are
unchanged.

The tests initialize minimal private checker state, seed an alias's declared
owner, call the member-publication primitive directly and force the depth
guard explicitly. They do not prove natural CLI recursion depth, binding,
lazy member forcing, concurrent checker access, complete diagnostics/displays
or representative worker cost. Real public/error controls and actual-pool
off/on equality remain required. Existing fidelity failures must remain failed
gates. Broader mapper ownership/context work and the comparable whole-project
TSR/tsgo wall ratio at most 0.50 remain open.
