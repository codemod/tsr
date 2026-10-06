# Native shared-property identity

The shared-property slice of `tsr-1yb.29` is qualified against native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The parsed-program adapter passes
39 checks, detects five incorrect native mutations, and agrees with 34 ordinary
CLI controls in default and single-thread modes. Rust and canonical native
runtime code are unchanged. This establishes a member-consumer prerequisite;
it does not establish saved time or the required comparable TSR/tsgo ratio <=0.50.

The [receipt](checker-native-shared-property.json) retains full outputs,
fixtures/options, process identities, source and binary hashes, mutation patches,
drivers and restoration evidence. The [private helper](checker-native-shared-property-helper.go)
and [parsed-program tests](checker-native-shared-property-test.go) are executable
assets for an isolated native checkout. They deliberately execute queries and
construct separate metadata countercontrols; they are not passive work observers.

## The actual receiver boundary

`getPropertyOfTypeEx` (`checker.go:18899`) first calls
`getReducedApparentType`. For an intersection, `getApparentTypeOfIntersectionType`
(`21796`) calls `getTypeWithThisArgument` with the original intersection receiver.
The suppliers passed to `createUnionOrIntersectionProperty` (`21452`) therefore
belong to that apparent intersection, rather than the raw declared constituents.

The initial adapter compared raw suppliers against a result from the apparent
view and misclassified ordinary symbols as synthesis. The corrected adapter
records both input and actual apparent-container identities and inspects the
suppliers from the actual container. The failed run remains diagnostic history;
its source hashes and outputs do not qualify those incorrect classifications.

| Parsed program | Native property result |
| --- | --- |
| Common inherited `Base.self: this` | One common instantiated symbol; read and expression retain `Both` |
| Distinct `Left.self` and `Right.self` | Synthesis, even though both supplier reads are exactly `Both`; expression prints `Left & Right` |
| Common `Base<T>.value: T`, equal concrete arguments | One common instantiated symbol |
| Common `Base<T>.value: T`, string versus number | Distinct property set; read becomes `never` |
| Distinct declarations with equal `string` reads | Synthesis; equal reads cannot establish common origin |
| Common getter returning number, setters accepting string versus number | Generic merged-instantiation clone retaining the first write type |
| Common scalar-only generic member with different arguments | Original-symbol reuse or a merged clone, according to the recorded query order |

The three query orders each use a fresh parsed Program: `cold` performs the
receiver type query first; `expression-first` additionally queries the member
expression; `checked-first` completes semantic diagnostics before acquiring the
checker. Each repeats the property query after the public expression query and
requires stable property/read identity within that checker.

These labels do not mean the direct composite lookup sees an empty cache.
Reduction can itself force property publication. All 39 recorded direct lookups
already have the non-function-augmented cache entry after reduction; the receipt
records this rather than treating a repeated getter as a fresh worker execution.
Raw addresses and TypeIds are meaningful only within their recorded Program and
private Checker, not across runs or workers.

## Equality, clones and write order

When supplier pointers differ, the native merge branch requires common
`getTargetSymbol` identity and
`compareProperties(..., compareTypesEqual) == TernaryTrue`. The property comparer
(`27672`) checks accessibility, declaration origin for nonpublic members,
optionality for public members, readonly state, and exact non-missing read-type
pointer equality. Equal printed types are insufficient.

Write equality is absent from that comparison. The accessor controls have equal
number reads and unequal string/number writes; both intersection orders produce
a merged clone. Reversing the order reverses which assignment is accepted by the
ordinary CLI. Intersecting the writes or requiring equal writes would diverge
from this native behavior.

The generic clone restores the declaration's parent, records the containing
intersection, and keeps the first supplier's mapper and write type. It has a
`links.target` pointing to the first supplier but does not copy the instantiated
check flag. Consequently `getTargetSymbol` returns the clone itself. A link target
and a native root-symbol identity cannot be used interchangeably.

`instantiateSymbol` (`20753`) can reuse a symbol once its read is resolved and
cannot contain type variables; setters additionally require a resolved invariant
write. The scalar-only generic controls observe original-symbol reuse in the
receiver-first and checked-first orders, and an earlier merged clone in the
expression-first order. Disabling this native reuse changes those six private
controls. The ordinary read diagnostics remain identical across worker modes.
The port must preserve eligible native completion/reuse boundaries without
requiring raw pointer equality between separate query histories.

## Qualification and Rust handoff

Five independently executed source mutations are rejected by semantic assertions:
unconditional synthesis, common-target-only merging, skipped optionality,
skipped readonly checks, and disabled resolved-scalar reuse. The optionality and
readonly negatives are explicitly constructed copies of parsed properties; they
retain declaration/root/read context and do not mutate natural declarations.
They qualify the comparison boundary, rather than pretending source syntax
can independently toggle one inherited declaration's metadata.

The ordinary CLI matrix checks 13 read fixtures and both write assignments for
each accessor order, in two modes: 34 completed processes with distinct PIDs.
Complete stdout/stderr and ordered loaded-file identities agree between modes.
The shared-versus-distinct alias controls reproduce the externally visible
[supplier experiment](checker-supplier-mapper.md) distinction. Wrong scalar and
accessor assignments remain errors; the incompatible string/number intersection
read is `never` and is assignable to the boolean sentinel. Private-member errors
remain in the complete output rather than being treated as public access success.

For `tsr-1yb.31`, preserve the original receiver through apparent-member
preparation, identify the supplying property in the private checker domain,
apply the qualified equality branch, and retain clone metadata and write order.
Do not fold every mapped constituent read with an alias-free intersection;
do not merge on bare declaration SymbolId or equal read TypeId alone. Completion
and reuse must refer to the published native member view and its context, not to
an unqualified name projection. The actual expensive worker boundary is member
resolution/property construction; this active adapter supplies no execution
counts or benefit ceiling for a production cache.

Union partial/index properties, deferred composites with more than two suppliers,
late-bound/augmentation recovery and reset behavior remain outside this slice.
Recursive base publication and heritage admission remain `tsr-1yb.30` and
`tsr-6.69.2`; signature admission remains `.27`/`.28`. Cross-worker receiver
qualification remains `tsr-1yb.4.1.5.2`. Closing this producer does not certify
those broader obligations or the concrete-member builder.

## Reproduction and restoration

Create an isolated checkout/archive of native 5b1047d, copy the helper and test
assets into `internal/checker/shared_property_contract.go` and
`internal/checker/shared_property_contract_test.go`, and run with Go 1.26:

```sh
GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off GOMAXPROCS=2 \
  go test -p 1 ./internal/checker \
  -run TestNativeSharedPropertyParsedPrograms -count=1 -v -timeout 2m
```

Use a private writable `GOCACHE` and an already available compatible toolchain.
The receipt includes mutation patches and the public driver; apply each mutation
only in that isolated checkout, restore it before the next case, and retain the
expected failing output. The failed system-Go 1.25 setup, adapter field typo,
raw/apparent classification failure and initial overly broad clone expectations
are preserved separately from the final qualified controls.

All 55,103 regular files from the pinned native archive match their original
bytes after mutation restoration; only the two intentional helper/test files
are added. The restored native source passes all 39 controls again. Canonical
native remains clean at 5b1047d. Canonical TSR was `9c0cc090` during qualification;
the earlier archive setup identifies `e1677fce`, and no result is relabeled as a
Rust run at the later revision. Full Rust corpora and timing gates were not run
for this native-only contract publication. The original performance target
remains active and unverified.
