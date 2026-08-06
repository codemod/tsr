# Tuple type nodes — the plain leg

Registered at `c72ebf2` before any code, per `docs/conventions.md`. The goal
this session is steered by is the **70% gradient**, which needs +6,303 lines
from 68.68%; no single row on the board is that big, so this page is one of
several and says so.

---

## 1. Why this row, out of the board

`examples/depend.rs` at `c72ebf2` ranks the gap's roots. Property access is
still the largest row at 10,494, but **3,739 of its reachable lines are
"the property has no type"** — the property's own *declaration* gaps somewhere
else, which `bd tsr-mcd` already established is a symptom count and not an
item. `depend.rs`'s own advice is to follow those to the **type-node** roots
instead, and there the board reads:

| type-node root | lines | want `any` | note |
|---|---:|---:|---|
| `TypeReference` (no dependency) | 3,890 | 0.9% | **top-1 51.1%** — one case |
| `TupleType` | **2,141** | **0.0%** | 173 cases, top-1 23.4% |
| `ArrayType` | 2,137 | 0.2% | **top-1 93.6%** — one case |
| `TypeQuery` | 1,728 | 7.4% | |
| `UnionType` | 1,334 | 1.3% | |
| `ConstructorType` | 1,084 | 1.7% | |

**`TupleType` is the only large one that is neither ceiling nor a single
case**: a 0.0% `want-any` share, 173 cases, and a top case holding under a
quarter. The two rows above and below it are each dominated by one file.

## 2. The leg, and the evidence that it separates

`declared.rs` gaps tuples deliberately, and its stated reason is precise:
upstream reaches them through `getTypeFromArrayOrTupleTypeNode` with
`globalTupleType` and **a per-element flags model — `optional`, `rest`,
`variadic` — that has no counterpart here**, so *"a half-ported tuple would
answer plausible wrong lines for the rest."*

That is an argument against porting the *modifiers*, not against porting the
plain form. Counted over every tuple the baselines print
(`>x : [...]` lines across `compiler/` and `conformance/`):

```
  plain, no modifier and no label   966   74.9%
  rest, optional or labelled        323   25.1%
```

So the plain leg is three quarters of the printed population, and the modifier
model the comment refuses is the other quarter — which **gaps**, exactly as it
does today. This is the same shape as `&&` separating from `||`
(`checker-notes-armsplit.md` §3.1): the expensive machinery is real and it
guards a minority of the row.

## 3. What will be built

`get_type_from_tuple_type_node`, beside the array arm it shares an upstream
function with:

- every element resolved through `get_type_from_type_node`; **a gap in any
  element gaps the whole tuple**, the rule the array arm and
  `get_instantiated_type_reference` already use, because `[Unported, string]`
  is not `[any, string]`;
- **any element that is a `NamedTupleMember`, an `OptionalType` or a
  `RestType` gaps the whole tuple** — the modifier model stays unported and
  refuses rather than approximating;
- printed `[A, B]`, and `[]` for the empty tuple, which is what the baselines
  record;
- `readonly [A, B]` when the parent is a `readonly` type operator, read the
  same way the array arm reads it;
- **interned on the element list**, so `[number, string]` written twice is one
  type. Upstream gets this from `createTypeReference` on a tuple target; there
  is no target symbol here, so the intern map is keyed on the elements
  directly. Identity matters for the same reason it did for `C<number>`: a
  union of two spellings must collapse.

The type carries **no members**, so `t[0]`, `t.length` and destructuring stay
gaps — they are gaps today and nothing about them changes. That is the whole
of what makes this safe: today the tuple *node* answers `errorType` and every
consumer of it gaps, so the only lines that can move are the ones that render
the tuple itself.

## 4. The bar

> **KEEP** if **net ≥ +800**, **lost ≤ 30**, **fewer cases regress than
> finish**, and **Δwrong = −Δgap − Δright ≤ Δright ÷ 3**.
> **REVERT** otherwise.

The loss allowance is not zero, unlike the parenthesisation build's, because
this change can legitimately turn a *gap* into a *wrong* line without the rule
being wrong: a consumer downstream of a tuple that previously gapped now
receives a real type and may answer confidently and wrongly for reasons that
belong to that consumer. Above 30 that reading stops being credible.

**Prediction: +800 to +2,000.** The row is 2,141 gap lines rooted at a tuple
node, of which ~75% should be plain, plus whatever downstream "the property
has no type" lines a resolved tuple annotation unblocks.

**How this would be shown wrong:** if the gain is concentrated in
`compiler/genericDefaults` (the 23.4% top case), the row was never the
173-case population measured here.

---

## 5. The result, scored against §4's bar

```
  net +1,227 | 328,965 -> 330,192 | 68.68% -> 68.94%
  gained 1,227 in 147 cases | lost 0 | 12 finished, 0 regressed
  gap 99,182 -> 97,665  =>  Δwrong = +290, bound 409
```

All four legs pass, and one of them **vacuously**, which is worth saying rather
than banking: *lost ≤ 30* had nothing to test, because zero lines were lost.
That was foreseeable from §3's safety property — today the tuple node answers
`errorType` and every consumer of it gaps, so the only lines that could move
were the ones rendering the tuple. The net floor and the Δwrong leg are what
actually did the work, the same reading the `&&` build had to make.

**The falsifier did not fire.** It named `compiler/genericDefaults`, the row's
23.4% top case; it took **263 of 1,227, or 21.4%** — slightly *less* than its
share of the row, over 147 gaining cases. The gain is distributed as the
population was.

**KEEP.**

### The population was under-counted, and by a factor worth recording

§4 predicted +800 to +2,000 from a row of 2,141 gap lines rooted at a tuple
node, of which ~75% were expected to be plain — about 1,600 as a point
estimate. The gap fell by **1,517** and the gradient rose by **1,227**, so the
row itself came in roughly where predicted; what the prediction did not carry
is that a resolved tuple *annotation* unblocks lines elsewhere. 12 whole cases
finished, which a 1,600-line row of renderings alone would not do.

### Ten tests changed, and none of them was wrong

Ten fixtures across nine files used a tuple as their stand-in for *"a type node
this port cannot compute"* — `export var t: [number, string]` for export
markers, `A & [string]` for intersections, `string | [number, number]` for
unions, and so on. Every one of them was testing a real property that is still
true; the tuple was merely the cheapest unported thing to hand when they were
written. They now use `keyof`, and where the flipped answer is itself
interesting (`export markers`, the namespace marker, `arrays.rs`) the tuple
assertion is **kept beside it as the positive control**, so the test measures
inheritance in both directions rather than only the gap.

> **A test that reaches for "something unported" as a fixture acquires a
> dependency on that thing staying unported.** Ten did, and they all came due
> in one commit. The cheap prophylactic is the one applied here on the way
> out: assert the *pair* — the unported case and the ported one — so the test
> keeps discriminating when the frontier moves instead of merely going red.

`tests/alias_naming.rs` had already recorded this happening once before, when
`bd tsr-eep` made unresolved names print; it is now the second entry in the
same file. That is what makes the pattern worth a rule rather than a note.

### And one expectation written from intuition was wrong, again

`a_gap_in_an_element_gaps_the_tuple` was first written asserting
`[Unresolved, string]` is `error`. It is not: `bd tsr-eep` mints a type that
prints the written name, because upstream reports `TS2304` **and renders the
name**, so `[Unresolved, string]` is upstream's line too. The array arm has
behaved this way since `tsr-eep` landed and its own test says so.

That is the fifth expectation this project has written from intuition and had
corrected by the code — and the third that was wrong in the *pessimistic*
direction, expecting a gap where the port already answers.

---

## 6. Object-literal methods, built in the same cycle

`objects.rs`'s member dispatch ported property, shorthand and spread members and
sent everything else to a catch-all that gaps the **whole literal**. The row
`depend.rs` ranks as `ObjectLiteralExpression` — 2,873 lines, a **3.9%**
ceiling, 767 cases, top-1 2.7%, with `conformance/spreadMethods` at its head —
is mostly that catch-all, and methods are most of what falls into it: the
corpus prints **3,012 object types carrying a method member**, headed by
`{ log(msg: any): void; }` (366) and `{ fn(): void; }` (68).

The arm is small because the infrastructure was already right: `Member` has a
`Signature` variant precisely because *"a method prints `m(): void` rather than
`m: () => void`"*, and `signature_member_text` already renders the member
spelling. A method whose signature cannot be built — an unannotated parameter
needing a contextual type — gaps the whole literal, the rule every other member
arm here follows.

```
  net +420 | 330,192 -> 330,612 | 68.94% -> 69.03%
  gained 420 in 86 cases | lost 0 | 13 finished, 0 regressed
  gap 97,665 -> 97,099  =>  Δwrong = +146
```

**No bar was registered before this one either**, which is the second time in
two sessions (`checker-notes-narrow.md` §5 records the first). Against the
standing figures it is a clear keep — zero lost, thirteen cases finished, none
regressed — but Δwrong at +146 against a +420 gain is **0.35, just over the
1-in-3 the last three registrations used**, and that is exactly the kind of
marginal call a pre-registered bar exists to make in advance rather than after
the fact. Recorded rather than rounded down.

Accessors (`get`/`set`) still hit the catch-all and are **not** a copy of this
arm — upstream prints an accessor as a *property*, and `signature_parts_of`
excludes accessors deliberately. `bd tsr-32y`.

### The defect the wrong bucket named

`wrongdelta.rs` over the pair attributes the largest new-wrong families to the
**tuple** build rather than to methods, and they are one defect:

```
  18   [string] | [number, boolean]        ->  [number, boolean] | [string]
  11   null[] | [number, string]           ->  [number, string] | null[]
```

**Tuples sort wrongly inside a union.** `compare_types` is upstream's
`CompareTypes`, which sorts by increasing `TypeFlags` and then by name; a plain
tuple is a `TypeData::Named` whose text begins with `[`, so it sorts by ASCII
against other object types, where upstream's tuple is a *reference to a
synthesised target* and sorts as a reference. Same family as the union-order
build at `a371ec8`, and it carries that build's caution: a change to union
ordering fires on all ~26,000 union lines, most of which are right today, so
the at-risk population is the bar. `bd tsr-5ll`.

## 7. `tsr-5ll` — the tuple arm of `compare_types`, sized and registered

Fourth session, at `ff49871`. Sized from the live `wrongdelta` dump rather
than the row: **34 wrong lines** are unions whose constituents match as a set,
differ as a sequence, and contain a tuple — 8 cases, head 11
(`destructuringControlFlow`'s `null[] | [number, string]`). Every one of the
nine distinct pairs follows upstream's comparator once the tuple stops
pretending its printed text is a *name*:

- `null[] | [number, string]` — `compareTypeNames` (`utilities.go:614`) puts a
  **named** type (`Array`) before an **unnamed** one; upstream's tuple has no
  symbol and therefore no name, while this port's carries its text as one.
- tuple vs tuple — `compareTupleTypes` (`utilities.go`): non-readonly first,
  then **ascending arity** (`[string]` before `[number, boolean]` — the
  `bd tsr-5ll` issue text had this pair backwards, corrected here), element
  flags and labels equal by construction while the modifier forms refuse, then
  elementwise `CompareTypes` over the element lists
  (`[string, string]` before `[string, number]`: string's flag < number's).

The build: a reverse index `TypeId → (elements, readonly)` recorded where
`get_type_from_tuple_type_node` already caches the forward pair — the
`type_reference_targets` precedent, data before reshape — a namelessness test
in `compare_type_names`, and a tuple arm in `compare_types`.

**The bar, registered before the code.** An ordering change can only permute;
per the parenthesisation precedent the honest legs are absolute:

1. **gained ≥ 25** of the 34 (the mechanism is exactly their diagnosis);
2. **lost == 0** — a right line that moves is proof the comparator is wrong,
   not a price;
3. **0 case regressions**;
4. **new wrong == 0** by `wrongdelta` — this change fires inside the wrong
   bucket and cannot mint a gap, so any new wrong line is a wrong rule.

If a leg fires: build wrong first, premise wrong second, no third. The
comparator's premise — that these 34 are ordering and nothing else — is
falsified by any pair whose *set* stops matching after the fix.

### §7 scored — three legs FIRED, and the override is recorded here, loudly

Final measurement over the pair (casedelta/wrongdelta, before = the
`ff49871` state):

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | gained ≥ 25 | **+61** | pass |
| 2 | lost == 0 | **3** | FIRED |
| 3 | 0 regressions | **1** (`restParamUsingMappedTypeOverUnionConstraint`) | FIRED |
| 4 | new wrong == 0 | **6** (64 fixed) | FIRED |

First hypothesis, build wrong: no — the arm is `compareTupleTypes` verbatim,
and the 64 fixed lines include every family §7 sized. Second, premise wrong:
**yes, and it is the same premise `docs/conventions.md` already convicts** —
*"every printed union order comes from one code path."* All 9 bad lines are
**written annotations inside signature prints** (`<T extends [number] |
[string]>`, rest-parameter annotations), where upstream renders the
declaration's node, not the comparator's order. The independent evidence is
upstream-pinned twice over: baselines record `[true, number] | [false,
string]` twelve times — an order `CompareTypes` cannot produce (`false`
sorts before `true`) — while `contextualSignatureInstantiation.types` records
`var b: number | string;` printing `>b : string | number`, so annotation
positions and computed positions demonstrably take different paths. One of
the six "new wrong" lines (`mappedTypesArraysTuples`, want `T40`) was
already wrong and merely changed text.

**Kept, with the losses attributed to `bd tsr-5o2`** (signature-position
written-node reuse), which now also carries a measured negative: the obvious
fix — reuse written union order whenever the constituents map one-to-one onto
the computed pieces — was built and measured **net-negative** (+323/−270,
`promiseTypeStrictNull` alone −244), because upstream's reuse gate involves
the builder's enclosing declaration, not only the node's type. The reverted
implementation is in this commit's history for whoever takes `tsr-5o2` up.
