# Member reference admission and contextual reads

The private `tsr-1yb.33.1` replay at frozen TSR `53826afe` repairs two actual
member paths. All **200 checker library tests pass** after exact restoration.
Production retention remains **refused**: the final unfiltered candidate loses
**18 previously RIGHT type assertions and one passing diagnostic case**.
Canonical runtime is unchanged; the reported PR #5 slowdown remains unresolved.

The [receipt](checker-member-admission-contextual-read.json) retains exact source
payloads, native observations, terminal runs, compiling mutations, all changed
corpus rows and complete ordinary CLI output. The [eight-source replay
patch](checker-member-admission-contextual-read.patch) applies to `53826afe`.
The [previous declared-reference experiment](checker-declared-member-references.md)
keeps its `0fd93183` evidence unchanged. Five concurrent runtime files changed
between those baselines; this experiment preserves them and builds fresh inputs.

## Only admitted references receive a this argument

The entry replay manufactured a mapped reference for every named class or
interface. Native `getTypeWithThisArgument` (`checker.go:19573`) maps reference
objects and otherwise preserves identity. `getDeclaredTypeOfClassOrInterface`
and `isThislessInterface` (`:17319`, `:17356`) determine that admission first.
Thisless nongeneric interfaces therefore retain their original type.

The private writer now asks its existing formal-this admission helper before
adding an argument. Classes, generic interfaces and direct or inherited-this
interfaces still map; nongeneric thisless interfaces do not. This uses actual
class-symbol/interface-node identity and the existing reference registry.

Fifteen fresh native observations cover plain, derived plain, self-bearing,
generic and class shapes in cold, checked-first and warm orders. They check
reference flags, mapped identity, the whole receiver argument and repeated
reads. The matching parsed Rust control fails before the repair. Two mutations
compile and fail semantically: removing admission and dropping positive mapping.
Exact restoration then passes all 199 tests. This repairs 23 type assertions and
two diagnostic cases relative to the entry replay, including JSX and override
losses, with no previously passing entry loss.

## Read the instantiated property rather than its declaration

The next trace checks inherited `ExtendableConfig<Options>` members. It proves
the mapper changes `Options` to `O`; missing inherited arguments are not the
cause. The trace adds no semantic queries and preserves all 132 focused output
rows byte-for-byte. Its initial compile failure is recorded separately.

The contextual object-property reader subsequently extracts the binder
declaration target and reads its raw type. Its leaf-only substitution cannot
replace a base's formal parameter. The minimal control is:

```ts
interface Base<T> { a: (x: T) => T }
interface Child<U> extends Base<U> {}
const probe: Child<number> = {
  a: x => { const n: number = x; return x; }
};
```

Native and main accept it; the entry and admission-only replay report false
TS2322, `T` not assignable to `number`. Native
`getTypeOfConcretePropertyOfContextualType` (`checker.go:30638`) reads the
instantiated property symbol. The private `contextual.rs` reader now reads its
existing published symbol's semantic type, retaining the unsupported fallback
and existing mapped/intersection/inference policies. It does not change binder
origin metadata or receiver/signature ownership.

The regression test fails semantically in the cold order before the repair and
passes in cold, typed-reader-first and checked-first/repeated orders afterward.
Returning the unmapped declaration type again compiles and makes it fail.
Exact restoration repeats **200 passes, zero failures and zero ignored tests**
on the same 651-file source inventory used to build the final binaries.

## Full corpus and public gates

All unfiltered producers terminate successfully and align the same **477,970
type assertions and 10,570 diagnostic cases** at fixture pin `4d4f005c`.
Baseline, entry and admission-only binaries were built freshly from `53826afe`.
The contextual comparison reuses their complete streams after exact source,
22 Cargo, 108 library and twelve binary guards; its candidate is compiled fresh.

| Versus frozen main | Entry replay | Admission only | Contextual final |
| --- | ---: | ---: | ---: |
| Previously RIGHT type losses | 49 | 26 | **18** |
| All changed type rows | 101 | 78 | 74 |
| Previously passing diagnostic losses | 4 | 2 | **1** |
| All changed diagnostic rows | 12 | 10 | 9 |

The final type transitions are 41 WRONG-to-RIGHT, seven changed already-WRONG,
18 RIGHT-to-WRONG and eight GAP-to-RIGHT. All 18 losses belong to
`thislessFunctionsNotContextSensitive3`. Diagnostic transitions are four
WRONG-to-RIGHT, two changed already-WRONG, two EMPTY_WRONG-to-EMPTY_RIGHT and one
RIGHT-to-WRONG: `subtypesOfUnion`. The contextual repair changes 20 type rows
(12 WRONG-to-RIGHT, eight already-WRONG) and two diagnostic rows beneficially
relative to admission-only. It loses no previously passing admission/entry row.
The remaining main losses still fail the retention gate.

Two ordinary CLI matrices total **608 terminal children**, with eight additional
exploratory callback launches recorded separately. The final matrix has 312
children and 78 default/single pairs. All mode outputs and file lists agree
within each tool. Final output matches main in 62 pairs and admission-only in
74; native diagnostic agreement is 48 main, 52 admission-only and 56 final.
All four admission-to-final changes are the new positive/negative callback
pairs and match native, including the required negative TS2322.

Two main native-agreement pairs are lost on `subtypesOfUnion`: the private
replay omits TS2411 for `foo14` at lines 28 and 49. This loss was already present
in the admission-only matrix. Its stored summary records it; an intermediate
claim of no baseline agreement loss was incorrect. Complete outputs correct
that claim explicitly. The public strict configuration differs from corpus
directive policy, so these are separate gates.

Already-wrong changes are retained. Derived-interface negatives still have
false TS2430 and lack native elaboration. `thislessFunctionsNotContextSensitive3`
loses two correct duplicate-editor diagnostics while retaining false assertion
diagnostics. The constructor-intersection control restores missing-member
TS2339 but still omits native TS2510. No complete native parity is claimed.

## Delivery and remaining boundary

Only passive evidence, replay and status documentation are delivered; runtime
simplification is skipped for that reason. `tsr-1yb.33.1` stays in progress and
builder `tsr-1yb.4.2.1` remains gated. The concrete remaining work is inherited
context-sensitive option inference and the missing callable index diagnostics.
Preserve receiver `tsr-6.69.2` and signature `.27`/`.28` ownership. Stored
interface signature formal-this mapping, full native flags/MapsThisOnly,
synthetic metadata and the wider union/static/anonymous domains remain
unqualified. Native restoration covers only the checker and two observer helpers.

These fixes were investigated with `ce-debug`. The receipt is a correctness
prerequisite handoff, with no construction-cost, CPU/RSS or equivalent-work speed
qualification. Runs overlap other work. No runtime coverage or speed gain is
landed; `tsr-1yb.34` and the TSR/native median wall target <=0.50 remain unmet.

Delivery checks pass the upstream-anchor and section validators. The global
issue-id validator still fails on 191 pre-existing IDs; all 737 reported citation
lines are identical to frozen main. The issues named in this continuation resolve
in the authoritative Beads database. This is a scoped verification, not a global
issue-id pass. The delivery review covers these five passive paths only.

## Native signature-admission continuation at `51960e07`

The [native contract phase](checker-signature-admission-native.md) establishes
80 parsed controls, three detected mutants and full native restoration for the
remaining merged-function diagnostic boundary. It preserves deferred signature
and return work rather than removing the Rust guard. `.27` stays in progress
until the compiling seam handoff; `.28` and the earlier 18/one retention gates
remain. The separate missing circular-annotation diagnostic is `tsr-6.74`.
