# Native recursive bases and heritage admission

`tsr-1yb.30` qualifies recursive-base publication at native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`: 30 parsed-program checks pass,
three semantic mutations are detected, and 20 ordinary CLI controls agree across
default and single-thread modes. Canonical Rust/native runtime code is unchanged.
This supplies the recursive-base handoff for `.31`; it does not establish a speed
gain or the required comparable TSR/tsgo median ratio <=0.50.

The [receipt](checker-native-recursive-bases.json) preserves complete outputs,
structured diagnostic chains/related information, fixtures/options, exact source
and binary bindings, all attempts, executed drivers and restoration evidence.
The [private probe and parsed-program tests](checker-native-recursive-bases-probe.patch)
apply to a pristine native 5b1047d archive. Hook snapshots read existing fields
and stack frames only; exported test wrappers deliberately execute native queries.
This is not an ordinary-workload observer or a whole-project execution count.

## Partial lists and completed lists

Native `getBaseTypes` (`checker.go:19167`) owns `resolvedBaseTypes` and
`baseTypesResolved` on the private checker's declared class/interface target.
References instantiate that target's bases when preparing their own member view.
Ordered bases, declaration identity, receiver and mapper contexts cannot be
replaced with a printed-name key. Rust's corresponding cache is checker-local
and keyed by the merged symbol; its target/identity contract remains applicable.

The recorded mutual cycle follows this sequence:

1. Resolving A asks whether B already inherits A.
2. Resolving B asks whether A already inherits B.
3. Re-entering A fails `pushTypeResolution`; A's existing partial list is returned
   with `baseTypesResolved=false`. Both active stack frames are marked false.
4. B retains its A base. A excludes B when `hasBaseType` sees A in B's ancestry.
5. Both targets publish completed base lists and both report TS2310. A has no
   retained bases; B retains A. Native compares B against A, and never A against B.

The nonempty-partial control declares A's bases as `Root, B<T>, Later`. At the
same re-entry boundary, A exposes `[Root]` while unresolved. Its final list is
`[Root, Later]`; B still retains A. Native compares A against both valid bases,
preserving order, and excludes B. Neither suppressing all heritage checks on a
cyclic interface nor freezing its active partial list matches that behavior.

Self cycles report TS2310 and retain no base without requiring a mutual re-entry.
The valid diamond retains Left and Right, while their shared Root remains one
native declaration. Completed empty A is clean; an invalid scalar base also
publishes an empty list but reports TS2312. Completion does not mean error-free
resolution. A failed circular/default resolution can even retain nonempty bases.
The cache flag is not a substitute for diagnostic or publication history.

## A natural member reset can resume an existing worker

The existing `circularConstraintYieldsAppropriateError.ts` fixture supplies this
natural class/default boundary:

```typescript
class BaseType<T> { bar: T }
class NextType<C extends { someProp: any }, T = C['someProp']>
  extends BaseType<T> { baz: string }
class Foo extends NextType<Foo> { someProp: { test: true } }
const foo = new Foo();
foo.bar.test;
```

Native reports TS2310, retains Foo's NextType base, and clears a previously set
`MembersResolved` bit before setting `baseTypesResolved=true`. A complete member
view is subsequently published again. This is an executed parsed-program reset,
extending the earlier [synthetic state controls](checker-native-member-completion.md).

In the receiver/base-first and checked-first orders, the trace includes a new
member worker after the reset. In the member-first order, a nested worker has
published the partial view while an outer worker remains active. After the reset,
that outer worker finishes and publishes the complete view without another entry.
The initial stronger test incorrectly required a fresh worker entry in every
order. Its failure and first mutation batch are retained as superseded evidence;
they do not qualify mutation sensitivity. The final tests require the actual
reset and subsequent completion, accepting the native continuation boundary.

For member reuse, completion therefore concerns a field-complete published view
and the current active worker/context. A getter hit, a temporarily set resolution
bit, or a fresh-entry count cannot independently prove a reusable immutable image.
The selected member worker is `resolveObjectTypeMembers` (`19106`); base resolution
and its own/inherited publications are nested work, not additive saved-time claims.

## The cycle's wrong comparison is admitted from syntax

Rust `check_interface_heritage_conformance` reconstructs its base vector from
written heritage entries, then calls the relater on those entries. Native
`checkInterfaceDeclaration` (`4991`) calls `getBaseTypes` for both the inherited
property check and subsequent assignability loop. Rust's base resolver already
filters cycles, but this diagnostic consumer bypasses that result.

The receipt binds the current Rust `heritage_conformance.rs` and `base_types.rs`
to their byte-identical versions at the rejected supplier baseline ca47f70a.
Changing only the native admission loop to consult written bases when its
resolved list is empty adds the excluded A-to-B comparison to the mutual cycle.
Native then reports a false missing-`b` diagnostic, TS2741, with the declaration's
related information. The ordinary unmodified control reports only two TS2310s.
This independently demonstrates that the syntax-based admission can produce a
false heritage error even with native's correct receiver propagation.

The [archived Rust supplier repair](checker-supplier-mapper.md) instead reports
the broad TS2430. That diagnostic distinction is retained; the native mutation
does not reproduce Rust's exact message or every step of its runtime relation.
A fresh Rust runtime trace/fix remains owned by `tsr-6.69.2`. The earliest
source-qualified admission divergence is established; the separate derived-this
collection defect and diagnostic-chain gap are not silently closed. Existing
TS2310 generic display differences remain `tsr-6.70`.

## Executable qualification and handoff

Ten natural programs run in three independent Program/query orders: receiver/base
first (`cold`), member first, and checked first. Each uses one exclusive native
checker and repeats its target-base queries after semantic diagnostics, requiring
stable completed base identities. Hooks record target IDs, ordered bases,
resolution flags, member flags, active stack results, publication, member worker
entry/exit, and actual interface heritage comparisons. IDs are local to that
recorded checker; no IDs cross worker or Program domains.

Three qualified mutations fail meaningful controls: prematurely publishing an
active base list as completed, admitting excluded written bases, and retaining a
partial member view instead of clearing it. The restored observer source passes
all 30 tests again. The 20 uninstrumented CLI children retain complete diagnostics
and ordered loaded-file identities in both modes. Genuine acyclic incompatible
overrides still produce TS2430; valid diamonds remain clean. The early mistaken
expectation that the circular-default fixture was clean is also retained.

For `.31`, expose active/provisional versus published base state in the private
checker domain; preserve valid partial prefixes and later completion. Consume the
resolved native base list for supported heritage comparisons while retaining
other invalid-reference/heritage diagnostics. Preserve the target's member reset
and both fresh-worker and continuing-worker completion paths. Use the qualified
[shared-property context](checker-native-shared-property.md) when preparing reads.
Do not make completed emptiness a general absence result or skip every cyclic
declaration's valid bases. Unsupported constructor factories/mixins, outer
parameters, augmentation and cross-worker paths remain explicit broader work.

Apply the probe patch only to an isolated native 5b1047d checkout and run with an
existing Go 1.26 toolchain and private writable build cache:

```sh
GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off GOMAXPROCS=2 \
  go test -p 1 ./internal/checker \
  -run TestNativeRecursiveBaseParsedPrograms -count=1 -v -timeout 2m
```

The receipt includes each mutation patch and all public/private drivers. Restore
the baseline observer between mutants. Builds and these fixture observations are
not acceptance timing samples. Full Rust corpora were not run for this native-only
contract; production retention still requires the original fidelity gates.

After qualification, the observer/helper/test changes are removed from the
private archive; all 55,103 regular native archive files match their original
bytes, and the complete published probe passes apply-check there. Canonical
native remains clean at 5b1047d. Canonical TSR was ea90c364 during qualification;
no Rust execution or older supplier observation is relabeled as a new run.
The overall performance goal remains active and unverified.
