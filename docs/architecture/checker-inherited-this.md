# Inherited polymorphic-this candidates: all rejected

The receiver prerequisite `tsr-6.69` remains unfinished. Candidate C1 corrected
seven inherited member assertions but lost one previously correct inference
assertion. C2 preserved that assertion and every previous RIGHT corpus result,
yet introduced a native-false interface-extension diagnostic. C3 fixes that
positive control but loses seven previously correct diagnostic cases. None of
the three candidates is shipped. This is a correctness prerequisite for concrete-member
reuse, not a measured speed improvement. The verified comparable TSR/tsgo median
wall target remains at most 0.50 and remains unverified.

[The receipt](checker-inherited-this.json) contains frozen binary/source hashes,
full public fixture inputs/options/outputs, corpus transitions, rejection controls
and the runtime trace. It preserves the distinction between corpus verdicts and
complete native CLI output.

The [C3 receipt](checker-inherited-this-c3.json) adds its full corpus deltas,
252 completed control children, focused checks, source/binary bindings, runtime
traces and restoration proof. The [C3 rejected patch](checker-inherited-this-c3-rejected.patch)
and [C3 boundary probe](checker-inherited-this-c3-probe.patch) both apply directly
to frozen 36c9bb5b. The probe includes C3; do not layer it on C3 again.

The [C1 rejected patch](checker-inherited-this-c1-rejected.patch),
[C2 rejected patch](checker-inherited-this-c2-rejected.patch), and
[heritage probe](checker-inherited-this-heritage-probe.patch) are retained as
evidence files. The probe diff includes C2 plus its temporary instrumentation;
apply each full patch to its receipt baseline, rather than layering the probe
on C2 again. These files are not applied to production code.

## Three observed boundaries

On `b2169d73570fe61be0fe8681012eab4a7047d8ca`, an inherited `self: this` read on
`Derived<string> extends Base<string>` produces `Base<string>`. A valid assignment
to `Derived<string>` falsely reports missing `own`. Runtime tracing observes the
original Derived receiver entering `get_type_of_property_with_this_argument`,
then `generic_heritage_member` binding Base's `this` to Base before composing the
outer generic arguments. Native `resolveTypeReferenceMembers` and
`resolveObjectTypeMembers` (`checker.go:19095` and `:19138`) carry the original
receiver through instantiated bases. C1 forwards that receiver through the
existing `instantiate_for_reference_with_this` helper.

C1 loses `compiler/inferenceErasedSignatures:0:45`: number becomes never.
The proposed signature-erasure explanation was ruled out: failure occurs before
erasure. Substitution of `set<K extends keyof this>` requests `keyof Inherited`.
`collect_keyof_property_names` uses `base_symbols_of(owner, true)`, whose generic
base refusal is appropriate for raw member typing but prevents declaration-key
metadata from resolving. The constraint becomes error and the conditional
inference becomes never. Native `getLiteralTypeFromProperties` (`checker.go:26717`)
projects keys from resolved property symbols; numeric and nonpublic decisions
use the winning declaration metadata (`:26746`). C2 carries `(name, SymbolId)`
through the existing key walk and uses `base_symbols_of_ex(owner, false)` only
there. It retains own-before-base shadowing, declaration order, cycle/unsupported
refusals, and the original anonymous/alias name-to-symbol paths. It adds no cache
or shared state. All 47 erased-signature rows then match baseline byte for byte.

C2 exposes a third boundary in `complexRecursiveCollections`. Isolating its
first virtual unit shows native and baseline accept `N2<T> extends N1<T>`, while
C2 falsely reports TS2430. Native `checkInterfaceDeclaration`
(`checker.go:4991/:5011`) compares against a base carrying the derived interface's
polymorphic-this argument. Rust `check_interface_heritage_conformance` passes the
ordinary base to the relater, whose property reads substitute each side's own
receiver. The runtime probe observes:

| Member | Source callback `iter` | Ordinary target `iter` | Target with original receiver | Ordinary / corrected-context member relation |
| --- | --- | --- | --- | --- |
| map | N2<T> | N1<T> | N2<T> | NotRelated / Related |
| flatMap | N2<T> | N1<T> | N2<T> | NotRelated / Related |
| toSeq | unchanged | unchanged | unchanged | Related / Related |

This confirms the missing heritage target context. It does not certify a general
representation or cache for that context. `tsr-6.69.2` owns the native boundary,
including identity and propagation through instantiation and relation reuse.
An ambient override, forced successful relation or fixture exception would
change other semantics and is not an acceptable fix.

## Frozen qualification and rejection

The final comparison source is `36c9bb5bbb9a642096e697b602b8bbcd2e7267fa`, after
pulling the concurrent call-reporting and flow changes. The three candidate-owned
baseline files were unchanged by those commits. Native is clean
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`; its TypeScript fixture submodule is clean
`4d4f005c8541e0255a9d8791205fdce326e462bc`. Baseline and C2 were separate ordinary
release builds, with source-diff guards and serial compiler children.

* All 477,652 type assertions ran unfiltered. Baseline has 467,229 RIGHT,
  9,210 WRONG and 1,213 GAP; C2 has 467,236 RIGHT, 9,203 WRONG and 1,213 GAP.
  Seven WRONG-to-RIGHT changes, zero previous RIGHT losses, no denominator change.
* All 10,570 eligible diagnostic cases ran unfiltered. Both have 4,943 EMPTY_RIGHT,
  3,942 RIGHT, 1,546 WRONG and 139 EMPTY_WRONG. Only
  `compiler/complexRecursiveCollections` changes, WRONG-to-WRONG, by adding the
  falsely reported TS2430. An unchanged failing bucket is not permission to add
  a false diagnostic. The isolated native-positive replay rejects C2.
* Twenty public families, native/baseline/C2 and default/single mode, produced
  120 distinct completed child PIDs. Ordered loaded identities agree throughout
  (52 or 53 inputs per family); candidate diagnostics agree between modes.
  Fifteen families match complete native output. Three generic-negative families
  retain the independent missing diagnostic child `tsr-6.55.1`; two cycle families
  retain the independent missing generic type arguments `tsr-6.70`. Countercontrols
  reproduce both gaps without inherited-self forcing. Full parity is not claimed.
* The isolated collection replay adds six completed children. Native/baseline
  exit 0; C2 exits 1 in both modes. Its full inputs and outputs are in the receipt.
* The candidate's library unit tests pass: 187 tests. The private checker ownership
  tests pass 13 with the existing augmentation `tsr-6.49` ignore. These check and
  query within each worker, not a retained post-pool API. The full checker package
  integration suite was not run; C2 was rejected before shipping it. Strict release
  checker/execute all-target Clippy, whole-workspace format and 4,064 native anchor
  checks pass. Passing mechanical checks do not override the native false error.

The diagnostic corpus oracle compares positioned codes and duplicate occurrences,
not message chains or related information. The public CLI controls retain complete
output independently. Variance, CPU/RSS or wall-speed conclusions cannot be drawn
from these correctness runs.

## C3: reference identity does not complete its consumers

C3 appends the explicit this argument to the existing checker-private
`(SymbolId, ordered arguments)` key, retaining separate display arity. It adds
native thisless-interface admission, maps the tail through member/signature
consumers, and preserves the measured variance prefix while using covariance
for the extra argument. Interface heritage compares the source view with a base
carrying the derived formal this. No additional cache, ambient override or
cross-checker identity is introduced. This representation remains rejected.

Two focused tests first fail on C2: hidden-argument member instantiation refuses
the arity, and same-target comparison loses its measured contravariant prefix.
C3 then passes five focused member, variance, rebuilding, thisless/merged identity
and call-signature checks. These omit nongeneric inherited suppliers and merged
function/namespace values. The full diagnostic corpus rejects C3:

| Corpus | Baseline | C3 | Result |
| --- | --- | --- | --- |
| 477,652 type rows | 467,229 RIGHT; 9,210 WRONG; 1,213 GAP | 467,236 RIGHT; 9,203 WRONG; 1,213 GAP | Seven improvements, no previous RIGHT losses |
| 10,570 diagnostic cases | 4,943 EMPTY_RIGHT; 3,942 RIGHT; 1,546 WRONG; 139 EMPTY_WRONG | 4,939 EMPTY_RIGHT; 3,939 RIGHT; 1,549 WRONG; 143 EMPTY_WRONG | Seven previous RIGHT losses |

Both unfiltered children complete with exit 0 and unchanged denominators. The
driver rejects the semantic result, rather than failing on infrastructure.
All 47 erased-signature rows remain byte-identical. Six cases gain false TS2430:
`collectionPatternNoError`, `subclassWithPolymorphicThisIsAssignable`,
`fluentInterfaces`, `interfaceThatInheritsFromItself`, `intersectionThisTypes`
and `thisTypeInBasePropertyAndDerivedContainerOfBase01`. `subtypesOfUnion`
loses two required TS2411 occurrences. Type gains cannot offset diagnostic losses.

The earlier 20-family receiver matrix completes 120 children with its same five
independent child-diagnostic/cycle-display gaps. Six additional heritage families
complete 48 children. Native and C3 accept the original complex collection and
generic interface-this positives in both modes; incompatible overrides still
reject. Rust still omits their native heritage message chains (`tsr-6.71`).

### The inherited supplier is mapped under the wrong owner

In the fluent trace for `B extends A`, both reference keys carry B's formal this,
TypeId 31. The target A mapper maps A's formal this 26 to 31, so `foo()` returns
31. The source B lookup finds `foo` through the ordinary symbol road; its supplying
parent is A. `members.rs:get_type_of_property_with_this_argument` then calls
`instantiate_for_reference_with_this` with receiver B. That helper selects B's
formal parameter 31; because its argument is also 31, the map is empty. The
supplied A return remains 26. Heritage receives distinct return identities and
answers `NotRelated`. The self-property and class-heritage controls show the
same supplying-base versus receiving-owner mismatch.

Native `resolveObjectTypeMembers` (`checker.go:19106/:19138`) resolves each base
with the receiver's this argument before adding inherited members. A correct
reference tail cannot replace this supplier mapper. The generic heritage
fallback has base and outer substitutions; the earlier successful symbol path
bypasses it for these nongeneric suppliers. This establishes the selected controls'
cause, not every changed case's cause. Recursive same-target inheritance still
requires its own relation/publication evidence.

### A newly demanded mapper refuses a merged function value

The missing TS2411 errors concern `foo14: typeof f`, where f merges a function
and namespace, not `foo16: T`. Corpus positions 26/47 map to original source
lines 28/49 after the loader removes two directive lines. Full ordinary native
and baseline replays report 29 errors; C3 reports 27, omitting exactly these
two function-value errors in both modes. The single-property T control preserves
its error.

C3 mints formal this for generic I1/I2 even without a this annotation, as native
generic types have that parameter. Ordinary member reads now carry a this-only
map. The trace observes `typeof f` as an Anonymous object with `signature=false`,
`FUNCTION | VALUE_MODULE` flags and no `signature_types` entry. Before the
parameter-mention walk, `inference.rs:instantiate_type` calls
`signatures.rs:complete_pending_signature_returns_of_type`. The absent function
signature entry makes admission false; the property becomes intrinsic errorType.
`index_constraint.rs:check_index_constraint_for_property` finds the pair
unreportable and skips TS2411 without executing a relation. The T properties
retain their identities, remain reportable and yield `NotRelated`.

Three independent public controls confirm the distinction. Generic I with the
merged function loses its error under C3. Removing I's generic parameter retains
the error, as does retaining generic I but removing the namespace merge. Native
and baseline reject all three. Blindly moving or disabling the pending-return
guard would affect unresolved originals and is not a qualified repair. Native
`instantiateTypeWithAlias` (`checker.go:22104`) has its own type-variable admission
and object/signature resolution; the port must establish the matching consumer
contract rather than declare this unsupported value safe.

C3 also prints zero-display-arity views as `A<>`, `Foo<>` and `Document<>` in
false reports. Hidden this must remain in semantic identity while presentation
retains the ordinary nongeneric name. The focused generic Box display checks
did not establish this boundary.

### Trace qualification and restoration

The new public rejection matrices contribute 48 completed children; probe-on/off
matrices contribute 36. With the earlier matrices, C3 has 252 distinct completed
child PIDs. Ordered loaded identities agree within each matrix. Both probes retain
the frozen ordinary C3 diagnostics and loaded identities with tracing disabled
and enabled. They observe actual entry/exit values and short-circuit decisions;
raw outputs, complete inputs/options, hashes and driver sources are in the receipt.

The first probe build had a mechanical missing-field error; its corrected build
and final refusal probe succeed. This is not a fourth semantic candidate. No
further repair variant was attempted after the third rejection. Eight private
files, including two probe-only files, are restored to clean 36c9bb5b, and all
three ordinary release outputs match qualified baseline hashes. Both full C3
patches pass apply-check against the restored source.

Full C3 checker-package, ownership-integration, strict Clippy, workspace-format
and anchor gates were not run after corpus rejection. Earlier C2 qualification
cannot qualify C3. No whole-project timing, CPU/RSS benefit, native-work
equivalence or 2x result is claimed.

## Review, restoration and next work

Scoped reuse, quality and efficiency passes ran inline under the repository's
sequential Task mapping. The final manual review covered all three candidate files;
there was no independent peer review. The anonymous/alias metadata paths were
preserved during that pass. No broad cache, raw declaration-based concrete value
reuse or cross-checker TypeId sharing was introduced.

All temporary probes were removed and the private source restored to clean
36c9bb5b. Its ordinary release outputs were restored to the qualified baseline
hashes. Both rejected candidate patches/binaries and trace receipts remain in
the local archive named by the JSON; canonical production code is unchanged.
`tsr-6.69`, `tsr-6.69.1` and `tsr-6.69.2` remain in progress. The broader receiver
consumer audit and concrete-member implementation are not approved by these tests.

The next repair must preserve the supplying base's mapper as well as the derived
this argument, and qualify the merged function/namespace consumer reached by
ordinary this mapping. Add nongeneric inherited, merged-function, zero-display-arity
and recursive publication controls to existing test homes. Preserve pending,
active and unsupported refusal distinctions; do not force a successful relation
or suppress another diagnostic to restore a pass count. Every changed diagnostic
row needs native triage, including already-WRONG cases. Run the full checker
package and unfiltered corpus pair before retaining another candidate.
`tsr-6.69.2` remains in progress; broader concrete-member reuse remains gated.

To replay the public controls, write each receipt case's `inputs` and `config`
to a temporary project and run both pinned release CLIs with the captured flags.
Builds stay outside the check. Corpus replay uses the existing
`tsr-conformance` examples `verdictdump` and `diagverdictdump`, with `TSR_FILTER`
unset and the same fixture pin. Frozen receipts describe these source revisions;
they must not be relabeled as qualification of later main revisions.
