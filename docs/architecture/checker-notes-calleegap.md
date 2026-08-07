# The two unexplored gates of the call funnel — `examples/calleegap.rs`

Sixth session, measured at `250d01a`. The residue of `bd tsr-klm` after
`bd tsr-4sa` shipped.

`callgate.rs` re-run at `a57a04b` reports, of 7,303 admitted call lines, two
gates that have never been split:

```
  1,398  want-any 247 (17.7%)   callee type is not an object type
    878  want-any 196 (22.3%)   identifier: symbol types as a non-object
```

2,276 lines. The standing hypothesis was that they are **one** mechanism —
*"the callee does not type as something with signatures"* — seen from two
positions. This page records the test, the split, and two items.

Companion pages: `checker-notes-callres.md` §13 (the funnel these two rows come
from), `checker-notes-namedcallee.md` (the `tsr-4sa` build that converted part
of this family and left six refusals behind).

---

## 1. The answer to "one mechanism or two" is **two**, and the test is a cross-tab

The impression — both rows are "the callee has no signatures" — is true and is
not the question. The question is whether one arm reaches both.

**The test.** Cross-tabulate the counter gate against the blocking node's own
**syntactic kind**. Two gates that were one mechanism seen from two positions
would each carry both sides.

```
  gate                                              new   call
  callee type is not an object type                1398      0
  identifier: symbol types as a non-object            0    878
```

Perfectly disjoint. They are two *positions*, and no line crosses.

### 1.1 Why, read out of `calls.rs` before the probe ran

`callgate.rs`'s `gate_of` attributes a node to the **last bumped counter in
declaration order**, because `define_counters!` declares them in funnel order.
Two facts about the call side then compose:

- `bump(&COUNTERS.callee_not_anonymous)` (`crates/tsr-checker/src/calls.rs:737`)
  fires when `resolve_call_signature`'s `TypeData::Anonymous` destructure fails
  *and* `get_signature_of_named_type` declined;
- `classify_unresolved_callee` (`calls.rs:400`) fires under the **identical**
  condition — `!matches!(self.store.get(callee_type).data, TypeData::Anonymous { .. })`
  on the same `callee_type` — and every one of its arms
  (`calls.rs:183`–`:256`) is declared **after** `callee_not_anonymous`
  (`calls.rs:179`).

So on the call side, row 3 is *always shadowed*. It can never be the last bumped
row. What produces the 1,398 is `new_callee_not_anonymous`
(`calls.rs:325`, bumped at `crates/tsr-checker/src/expressions.rs:807`), which
carries the **same label string** and is declared last — and `callgate.rs` keys
its `gates` map by the trimmed label, so the two collapse into one row.

> **The 1,398 row is `new`. The 878 row is `call`. The shared label is an
> artefact of two counters spelled the same, and nothing in `callgate.rs`'s
> output says so.**

**Control C3** was registered against exactly this: a `CallExpression` blocking
node *could* still reach that gate if a `NewExpression` somewhere in its subtree
bumped the later row. Expected 0; **measured 0**. The cross-tab is stronger than
C3 asked for — the cell is empty outright.

---

## 2. B0 — the "everything is handled, something downstream gapped" bucket reads **zero**

`docs/conventions.md`, from the object-literal row: *"put the 'every part is
handled, something downstream gapped' bucket **first**, so it cannot inflate the
rest. It was 77% here."*

Bucket B0 is the blocking call node *itself* answering a real type — nothing
about the callee is what stops the line, and the gate row was bumped by some
other expression in the same subtree. It is printed first and it reads
**0 of 2,276**.

This row is not the object-literal row. Every share below is against the full
population and none of it is downstream inflation.

---

## 3. The split

Unit: assertion lines. Population and reachability rule copied **verbatim** from
`callgate.rs` — a different rule makes the two instruments incomparable, and C2
reproduces its two rows exactly (1,398 / 878, want-any 247 / 196).

### 3.1 `callee type is not an object type` — 1,398 lines, 100% `new`

| lines | want-any | top-1 | bucket | owner |
|---:|---:|---:|---|---|
| 835 | 5.9% | 19.4% | a **generic** construct candidate | **inherited** — `tsr-4sa`, inference |
| 186 | 16.1% | 32.8% | `Named` whose members symbol has **no interface declaration** (class instance, enum, alias) | **inherited** — `tsr-4sa` |
| 121 | 97.5% | **88.4%** | callee is **`any`**, callee is not a bare name | **new** — §5 |
| 73 | 5.5% | 27.4% | an overload set whose candidates **disagree** | **inherited** — `tsr-4sa` |
| 47 | 4.3% | 93.6% | callee is a **union** | **new** |
| 41 | 2.4% | 97.6% | callee is an **intersection** | **new** |
| 25 | 0.0% | 88.0% | **REFUSED namespace-qualified** | **inherited — and STALE, §4** |
| 20 | 80.0% | 40.0% | callee is `any` (an annotated variable) | **new** — §5 |
| 16 | 31.2% | 37.5% | `Named` with **no members table** | **new** |
| 13 | 84.6% | 61.5% | the interface declares no construct signature | |
| 8 | 100% | | an intrinsic (`number`, `undefined`, `unknown`) or a literal | ADR-0038 ceiling |
| 7 | 0.0% | 28.6% | the interface has a **heritage clause** | **inherited** — `tsr-4sa` |
| 4 | 75.0% | 100% | callee is `any` from an unannotated parameter | refused family, §5 |
| 1 | 0.0% | | the return is a type parameter | **inherited** — `tsr-4sa` |
| 1 | 0.0% | | **C6 fired** — §6 | |

### 3.2 `identifier: symbol types as a non-object` — 878 lines, 100% call

| lines | want-any | top-1 | bucket | owner |
|---:|---:|---:|---|---|
| 291 | 0.0% | 8.2% | **C6 fired** — `SymbolConstructor`, the `unique symbol` refusal | **inherited** — §6 |
| 121 | 3.3% | 37.2% | callee is a **union** | **new** |
| 96 | 34.4% | 19.8% | callee is `any` from an **unannotated parameter** | refused family, §5 |
| 88 | 0.0% | 77.3% | a **generic** call candidate | **inherited** — inference |
| 82 | 28.0% | 8.5% | `Named` with no interface declaration | **inherited** |
| 71 | 100% | 11.3% | callee is `any` (an annotated variable) | **new** — §5 |
| 31 | 100% | | an intrinsic (`never`, `number`, `string`, `undefined`, `unknown`) | ADR-0038 ceiling |
| 22 | 72.7% | 40.9% | the interface declares no call signature | |
| 20 | 0.0% | 45.0% | the interface has a heritage clause | **inherited** |
| 15 | 13.3% | 40.0% | an overload set whose candidates disagree | **inherited** |
| 13 | 23.1% | 23.1% | `Named` with no members table | **new** |
| 12 | 50.0% | 41.7% | callee is `any` (an unannotated variable) | **new** — §5 |
| 12 | 41.7% | 50.0% | callee is an **intersection** | **new** |
| 2 | 0.0% | | the return is a type parameter | **inherited** |
| 2 | 100% | | callee is `any` (binding element / annotated parameter) | **new** |

### 3.3 New versus inherited, counted

**1,338 of 2,276 = 58.8%** belongs to refusals that already exist and were
priced elsewhere:

```
  923   a generic candidate            inference — REFUSED this session at ~17 conversions (STATUS.md §5)
  268   Named, no interface decl       tsr-4sa (was 274)
  291   `unique symbol`                tsr-4sa §4.1 (was 291 — reproduces exactly)
   88   disagreeing overload set       tsr-4sa (was 48; grew)
   27   heritage clause                tsr-4sa (was 27 — exact)
    3   return is a type parameter     tsr-4sa (was 3 — exact)
   25   namespace-qualified            tsr-4sa (was 75) — STALE, §4
```

Genuinely new and never priced: **unions 168, intersections 53, `any` callees
326, `Named` with no members table 29, ceiling intrinsics 39.**

> The single largest bucket in this row — **923 lines, 40.6%** — is inference's,
> and `STATUS.md` §5 refused inference's remaining legs this session at ~17
> conversions. Neither gate is an item in its own right; what they are is a
> second door onto seven populations that already have owners.

---

## 4. A stale prerequisite, predicted before the run and confirmed

`get_signature_of_named_type` (`crates/tsr-checker/src/signatures.rs:346`) still
declines when `needs_namespace_qualifier` is true, and the doc comment states
its premise:

> `TypeData::Named` bakes the symbol's own name, so a type declared inside
> `declare namespace Intl` prints as `NumberFormat` where upstream's
> `lookupSymbolChain` (`nodebuilderimpl.go:1061`) prints `Intl.NumberFormat`.

`docs/conventions.md`: *"a prerequisite in your own doc comment is checked the
way a handover's is."* **That premise died this session.** Design P added
`Checker::type_to_string_at` (`crates/tsr-checker/src/checker.rs:513`) →
`qualified_name_at`, and `crates/tsr-conformance/src/types_producer.rs:682`
renders *every* assertion through it. The bare name is no longer what gets
printed.

The prediction was written into the probe's header **before the first run**:
*the refusal is stale and its bucket's forecast now matches the baseline.*

Measured by the match test — `type_to_string_at(return, the assertion's own
node)` against the wanted string, per line, at the line's own reference site,
because a context-sensitive name has no meaning without one:

```
  25 lines   forecast matches 25   forecast misses 0
             own-node 18, of which 18 match
             22 of the 25 are compiler/temporal
```

**Deleting the refusal at `signatures.rs:346` converts 25 with a measured zero
at risk.** The population is smaller than `tsr-4sa` §4.2's 75 because the `Intl`
half (50) has since moved into the disagreeing-overload bucket, so the number
must be re-taken and not carried.

**The bar for the deletion is registered on
`checker-notes-namedcallee.md`**, appended the same session by a second party
who wrote neither this probe nor the original refusal. That page owns the item;
this one owns the number. The split is deliberate — the refusal lives in
`tsr-4sa`'s record and a sizing does not get to relocate it.

---

## 5. The item — `resolveUntypedCall`, and it was found by a control firing

`bd tsr-4y0s`.

### 5.1 The control, and why it was pinned to upstream

**C4**, registered before the run: *every line whose callee types as `any` must
want `any`*. The expected value is not mine — it comes from the construct being
claimed:

```go
// checker.go:9933
func (c *Checker) isUntypedFunctionCall(funcType *Type, apparentFuncType *Type, ...) bool {
	return IsTypeAny(funcType) || ...
```

reached on the call side at `checker.go:8529` and on the `new` side at
`checker.go:8593`, both going to `resolveUntypedCall` (`checker.go:9902`), which
returns `c.anySignature` — whose return type is `anyType`. Upstream prints
`any` for such a call and cannot be wrong about it.

**It read 94.5% on the `new` side and 61.9% on the call side.** The forecast
column says the same thing more precisely: 249 of 326 lines forecast the
baseline exactly, 77 do not, and every miss wants a *real* type — `void`,
`Foo`, `ConcreteA`.

> An arithmetic control could not have seen this: the partition summed
> correctly. What saw it is a control whose expected value is fixed by
> upstream's code rather than by my summary of it — `docs/conventions.md`'s
> rule, and the third time it has paid.

The `any` is **this port's**, not upstream's, on part of the bucket. So
`docs/conventions.md` applies: *"a population identified by the shape of the
answer is not thereby attributed to a mechanism."*

### 5.2 Split by where the `any` came from

By the declaration the callee's name resolves to — which is what an arm could
actually test at the call site:

| origin | lines | forecast match / miss | own-node | own match |
|---|---:|---:|---:|---:|
| call: an annotated variable (`var x: any`) | 71 | 71 / 0 | 56 | 56 |
| `new`: the callee is not a bare name | 121 | 118 / 3 | 36 | 34 |
| `new`: an annotated variable | 20 | 16 / 4 | 14 | 11 |
| call: a variable with no annotation | 12 | 6 / 6 | 7 | 4 |
| call: a binding element, an annotated parameter | 2 | 2 / 0 | 2 | 2 |
| **call: an UNANNOTATED PARAMETER** | **96** | **33 / 63** | 91 | 28 |
| **`new`: an UNANNOTATED PARAMETER** | **4** | **3 / 1** | 4 | 3 |

An unannotated parameter is `any` in this port and is **contextually typed**
upstream, so the call's answer is upstream's *parameter* type and not `any`.
That is `STATUS.md` §5's standing contextual-typing refusal — 2,082 lines, 86%
entangled, re-armed and reproduced three times — arriving through a new door.

**Refusing that origin positionally removes 64 of the 77 misses for 36
conversions.**

### 5.3 What is left

```
  after the refusal:  226 lines,  213 forecast converts,  13 forecast wrong   = 16.4 : 1
  own-node floor:     115 lines,  107 forecast converts,   8 forecast wrong   = 13.4 : 1
```

The own-node floor is the honest one, by `checker-notes-namedcallee.md` §3's
rule: a read of a variable two hops downstream is cascade and is deliberately
not in the floor.

The 13 residual misses, named in advance so anything else indicts the arm:
`compiler/classBlockScoping` 4 (a block-scoped class typing as `any` — a TDZ
defect of ours), `compiler/declarationsWithRecursiveInternalTypesProduceUniqueTypeParams`
5 (a twelve-deep recursive type; unreachable at any conversion),
`compiler/newExpressionWithCast` 2, `compiler/jsDeclarationEmitDoesNotRenameImport`
1, `compiler/simpleRecursionWithBaseCase3` 1.

### 5.4 The concentration, stated because it is a falsifier

`compiler/duplicateLocalVariable1` supplies **107 of the 121** in the largest
bucket — **47% of the whole forecast**. Ex that case the item is ~106
conversions. A bar for this must name that case and read the gain outside it,
the way design P's leg 3 did.

### 5.5 The objection a reviewer will raise, and the answer

ADR-0038 forbids rendering `errorType` as `any`, because a gap must stay
distinguishable from a wrong answer. **This is not that.** `anySignature`'s
return is `anyType` — an honest computation upstream performs, not a bail-out it
renders. It is the same argument `STATUS.md` §2 used when `Array<any>`'s
instantiated index signature collapsed the ceiling estimate from ~26,000 to
2,202 firm: *"the computed answer coincides with upstream's bail-out"* is not
the same as *"we printed `any` because we failed"*.

It still has to be **argued in the ADR before the code**, not assumed. If it
cannot be, the item does not exist, and that is the one thing that would kill
it.

---

## 6. Two controls fired, and both diagnose the probe

`docs/conventions.md`: a control that fires is diagnosed, never tuned.

### 6.1 C6 — `WOULD RESOLVE`, expected 0, read **292**

C6 was pinned to this port's shipped code: a `Named { members: Some(_) }` callee
passing all seven declines in `get_signature_of_named_type`
(`signatures.rs:303`–`:349`) would have been *answered* and could not be in
either gate.

**291 of the 292 are `SymbolConstructor`**, and the forecast column reads
**0 / 291** — every one a `want unique symbol / forecast symbol` miss. The cause
is that the `unique symbol` refusal is written at the **call site**
(`calls.rs:444`–`:449`, at the call's *return*) and not inside the arm, which is
exactly the design `checker-notes-namedcallee.md` §8.2 recorded as deliberate:

> a refusal placed where upstream places its *answer* fixes lines the item was
> not aimed at, while one placed inside the new arm would not have.

So the probe's re-implementation of the decline model was incomplete in the one
way that page predicted, and `tsr-4sa` §4.1's **291** reproduces **exactly**,
five builds later. The remaining 1 is the `new` side's
`node.type_arguments.is_empty()` guard (`expressions.rs:798`), which the probe
does not model.

Both are located in code. Neither changes any other bucket: C6 is its own row
and is not folded into anything.

### 6.2 C1 — classified-but-not-gap, expected 0, read **53**

`callgate.rs` was re-run at this commit and **it reads 53 too**, on the identical
7,303 admitted lines. **Inherited, not introduced.** It read 0 at the fifth
session; the compiler has moved eight builds since. 0.7% of the population, and
recorded here because `callgate.rs`'s own header still says "expect 0
violations" — that sentence now needs a number beside it.

### 6.3 The controls that passed

- **C2** — the two gates sum to **2,276**, reproducing `callgate.rs` exactly,
  including both want-any figures.
- **C3** — `call` nodes in the `new` gate reached via a nested `new`: **0**.
- **C5** — pinned to `getSignaturesOfStructuredType` (`checker.go:18964`,
  returns `nil` unless `t.flags&TypeFlagsStructuredType != 0`) and
  `getApparentType` (`checker.go:21744`–`:21753`, every primitive maps to a
  global interface declaring no signatures). A non-`any` intrinsic or literal
  callee is not callable upstream either, so upstream answers `errorType` and
  the baseline renders `any`. Registered at **≥ 90%**; measured **100%** on all
  39 lines. Those lines are ADR-0038 ceiling and are not work.
- **C7** (`Anonymous` callee) and **C8** (`error` callee) — both **0**, both
  structurally excluded by the gates' own `else` arms (`calls.rs:727`,
  `expressions.rs:790`) and by the admission rule.

---

## 7. Verdict

**Neither gate is an item.** 58.8% of the 2,276 belongs to refusals that already
exist, the largest single bucket (923 lines, 40.6%) is inference's, and the
"downstream gapped" bucket is empty so there is nothing hiding in it. That much
is a **re-refusal with a fresh number**, which is the honest product of this
probe.

Two things came out that are items:

| item | reachable | own-node floor | effort | feasibility | file |
|---|---:|---:|---:|---:|---|
| **`resolveUntypedCall`** — an `any` callee answers `any`, with the unannotated-parameter origin refused positionally | **213** (13 forecast wrong, 16.4:1) | **107 / 8** | **1** | 0.8 | `calls.rs`, `expressions.rs` |
| **delete the stale namespace refusal** at `signatures.rs:346` — **bar registered on `checker-notes-namedcallee.md`** | **25** (0 forecast wrong) | 18 / 0 | **1** | 0.95 | `signatures.rs` |

**Effort 1 for the first is argued from the code, not intuited**: an
`IsTypeAny` test at the head of `check_call_expression` (`calls.rs:387`) and
`check_new_expression` (`expressions.rs:774`), returning `self.intrinsics.any`,
plus one positional refusal reading the callee identifier's value declaration
for an unannotated `ParameterDeclaration`. No new data, no relation, no
inference — the same shape as the `new C<T>()` arm.

**Effort 1 for the second is a one-line deletion** whose prerequisite shipped
this session.

**What would show this page wrong.** For the item: the ADR-0038 argument in §5.5
failing review, or the 107-line `compiler/duplicateLocalVariable1` concentration
turning out to be a case whose baseline is upstream's *bail-out* rather than its
computation. For the stale refusal: `type_to_string_at` not being the printer on
some path a real build would take — the probe uses the producer's own printer,
so that is unlikely but it is the falsifier.
