# Declared member references: private progress, retention refused

The `tsr-1yb.33.1` continuation at frozen TSR
`0fd93183b39aba84d7ed16a98a3243aed94528f4`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, repairs declared-class and inherited
interface `this` admission in the private member writer. All **198 library tests
pass**, including two new controls that fail before their fixes. Production
retention remains **refused**: the final unfiltered candidate loses **49 previously
RIGHT type assertions and four passing diagnostic cases**. Canonical runtime
is unchanged; no coverage gain or speed improvement is qualified.

The [receipt](checker-declared-member-references.json) includes exact source
payloads, six fresh parsed native observations, terminal runs, compiling mutations,
all changed corpus rows and complete ordinary CLI output. The
[replay patch](checker-declared-member-references.patch) applies to frozen
`0fd93183`. The [earlier natural-publication experiment](checker-natural-member-publication.md)
keeps its `d417a8c9` results and source binding unchanged.

## Why a declared target needs mapping

The original `collectionPatternNoError` fixture exposes ten wrong assertions in
32 rows of the entry replay. A first trace hypothesis was signature arity or an
extra hidden `this` argument. The trace disproves it: `MsgConstructor<U>` has
exactly one registered argument. Inference collects `U`; its constraint relation
rejects `Message` because a declared `Message.clone` still returns the unbound
formal `this` while the source clone is mapped. No signature arity fix was made.

Native `getDeclaredTypeOfClassOrInterface` (`checker.go:17319`) admits every class
as a reference with a formal `this`, even with zero ordinary type parameters.
`resolveTypeReferenceMembers` (`checker.go:19095`) pads the declared target with
its own receiver, and `resolveObjectTypeMembers` (`checker.go:19106`) maps fields
when formal parameters differ from those arguments. Raw TypeId equality is not
evidence that this mapping can be omitted.

Three fresh cold/checked/warm native programs show reference admission,
self-target identity, zero ordinary parameters, one all-parameter and a clone
return identical to its declared receiver. The private test fails before the fix
with a formal-this return instead of the receiver. The writer now establishes
the existing class-symbol this identity before declared fields, and uses that
actual formal parameter in deciding whether fields must be instantiated. This
repairs the original collection fixture and the new positive/negative class CLI
controls without changing inference, relation success or diagnostic policy.

## An interface can inherit its need for this

The class-only version passes 197 library tests but introduces 13 type losses
in `contextualThisType` and `intersectionThisTypes`. The minimal derived test is
`interface X { a:(p:this)=>this } interface Y extends X {}`. Its inherited function
uses `X` rather than `Y` before the second fix.

Native `isThislessInterface` (`checker.go:17356`) considers merged declarations
and resolved extends bases as well as direct `ContainsThis`. Three new parsed
orders show `Y` admitted as a self-target reference with one formal-this parameter;
the inherited function's input and return both have `Y` identity. The private
writer now follows those admission checks, uses the existing interface-declaration
this identities and establishes them before field preparation. Its active admission
guard means the native this slot is still absent during admission; it never means
that structured member publication is complete.

The existing reference registry remains authoritative. Comparing two raw TypeIds
for one owner still cannot establish reference identity. Member-image publication,
natural reset, active-frame continuation, inherited suppliers and mapper records
remain the preceding experiment's actual paths, rather than fabricated getters.

## Verification and wider failures

Both new tests execute cold, checked-first and warm orders. Each final mutation
compiles, then makes its focused control fail: omitting the declared formal-this
map, and omitting inherited-this interface admission. Sources restore exactly
after each mutation; the full **198-pass, zero-ignored** library repeats on the
exact final binary source inventory. A class-only mutation and its 197-pass
restoration are preserved separately. Three setup failures (trace edit, test API
and restoration package-name typo) remain recorded and do not count as semantic
sensitivity.

Two ordinary CLI batches have **384 terminal children**, including native,
baseline and candidate in default and single modes. The final batch has 198
children and **54 of 66 complete baseline/candidate output pairs identical**.
Ten changed pairs match native. Two changed derived-interface negative pairs
remain wrong: the receiver spelling improves, but false TS2430 and missing native
error elaboration persist. Native diagnostic agreement is 40 baseline / 50 candidate
pairs; no previous agreement is lost in this bounded matrix. Default/single output
and loaded-file lists match within each tool.

The derived-interface positive fixture adds `extra:number`: native accepts it,
while both baseline and final candidate still emit false TS2430. This retained
heritage-context gap is tracked under receiver `tsr-6.69.2`. A focused property
identity result is not full public diagnostic parity.

Fresh current release baseline and class-only candidate binaries, then fresh final
candidate binaries run the unfiltered producers at fixture pin
`4d4f005c8541e0255a9d8791205fdce326e462bc`. The final comparison reuses the current
baseline binaries and streams only after 650 Rust, 22 Cargo, 108 bundled-library
and all three binary hash guards. The prior `d417a8c9` baseline is not reused.

| Frozen `0fd93183` comparison | Class-only candidate | Final private candidate |
| --- | ---: | ---: |
| Aligned type assertions | 477,970 | 477,970 |
| Previously RIGHT type losses | 62 | **49** |
| All changed type rows | 112 | 101 |
| Aligned diagnostic cases | 10,570 | 10,570 |
| Previously passing diagnostic losses | 6 | **4** |
| All changed diagnostic rows | 14 | 12 |

Final type transitions are 37 WRONG-to-RIGHT, seven changed already-WRONG,
41 RIGHT-to-WRONG, eight RIGHT-to-GAP and eight GAP-to-RIGHT. Losses remain in
`temporal` (8), `thislessFunctionsNotContextSensitive3` (18),
`checkJsxChildrenProperty16` (10), `checkJsxUnionSFXContextualTypeInferredCorrectly`
(4) and `override20` (9). The interface fix repairs the 13 class-only type losses
above and adds two slice gains; no other type rows change between those versions.

Final diagnostic losses are `temporal`, `intersectionAsWeakTypeSource`,
`override20` and `subtypesOfUnion`. The last is a **new loss relative to the
class-only version**. Arrays ES5/ES6 and loose-this diagnostics improve, but the
already-wrong `contextualThisType` case changes adversely. All 12 changed cases,
including two WRONG-to-WRONG and one EMPTY_WRONG-to-EMPTY_WRONG, are retained.
Neither net counts nor focused improvements clear the no-RIGHT-loss gate.

## Delivery and next boundary

Only this receipt, replay patch, documentation and status notes are delivered.
The final private source has 651 Rust files; all unowned private source hashes
match the current 650-file baseline. Canonical baseline hashes also match before
delivery. Native restoration is qualified for the checker and two observer helpers
only. Raw corpus streams remain in the private archive with exact hashes in the
receipt; all changed rows are included in the committed evidence.

The concrete 49 type/four diagnostic losses are the next correction gates.
`publication_signature` still consults the class `this_types` store while admitted
interface identity lives in `this_type_nodes`; stored call/construct signature
formal-this mapping is **unqualified**. The existing numeric nonempty-field test
does not certify it. Preserve signature `.27`/`.28` and receiver `tsr-6.69.2`
ownership. Full native flags/MapsThisOnly, mapper identity, synthesized metadata,
unions, augmentation and anonymous constructor/static field behavior remain
unqualified. `tsr-1yb.33.1` stays in progress and builder `tsr-1yb.4.2.1` stays gated.

The bounded private fixes were investigated with the `ce-debug` skill. Delivery
is passive evidence, so runtime simplification is skipped; focused tests, exact
restoration and a review of the delivery remain required. These observations lack
CPU/RSS and equivalent performed-work qualification and overlap other runs.
The PR #5 regression `tsr-1yb.34` remains unresolved; the TSR/native median wall
target <=0.50 remains unmet.
