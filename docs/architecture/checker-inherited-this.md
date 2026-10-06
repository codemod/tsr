# Inherited polymorphic-this candidates: both rejected

The receiver prerequisite `tsr-6.69` remains unfinished. Candidate C1 corrected
seven inherited member assertions but lost one previously correct inference
assertion. C2 preserved that assertion and every previous RIGHT corpus result,
yet introduced a native-false interface-extension diagnostic. Neither candidate
is shipped. This investigation is a correctness prerequisite for concrete-member
reuse, not a measured speed improvement. The verified comparable TSR/tsgo median
wall target remains at most 0.50 and remains unverified.

[The receipt](checker-inherited-this.json) contains frozen binary/source hashes,
full public fixture inputs/options/outputs, corpus transitions, rejection controls
and the runtime trace. It preserves the distinction between corpus verdicts and
complete native CLI output.

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

The next attempt must represent the derived this argument at the native heritage
relation boundary, retain the correct receiver and key metadata, and add positive
and genuinely incompatible override controls. Every changed diagnostic row must
be checked against native, including cases already marked WRONG. Run the full
checker package and unfiltered corpus pair before retaining the next candidate.

To replay the public controls, write each receipt case's `inputs` and `config`
to a temporary project and run both pinned release CLIs with the captured flags.
Builds stay outside the check. Corpus replay uses the existing
`tsr-conformance` examples `verdictdump` and `diagverdictdump`, with `TSR_FILTER`
unset and the same fixture pin. Frozen receipts describe these source revisions;
they must not be relabeled as qualification of later main revisions.
