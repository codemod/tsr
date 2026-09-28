# Deferred type constructs — `keyof`, indexed access, mapped and conditional types

This page owns the family of type constructs the checker answers with a
**print-only mint** or not at all: `keyof X`, `X[Y]`, `{ [K in K']: T }` and
`A extends B ? C : D`. `keyof` and indexed access have partial arms (§34, §35 of
[checker-notes-callres.md](checker-notes-callres.md), extended by §786 and §806);
mapped and conditional type *nodes* have **no arm at all** and fall through
`get_type_from_type_node`'s catch-all to `errorType`.

They belong on one page because one mechanism serves all four, and because the
interesting work is never the mint itself — it is deciding which operand shapes
it may serve without manufacturing a wrong line, and which *consumers*
downstream have to know a mint when they see one.

---

## §812 — the mints were `ANY`, and an `ANY` constituent eats its own union

Measured at `8187d2d9` (§811), on a freshly accepted `scorepair` baseline:

```
TOTAL 474,243   right 437,946   gap 7,448   wrong 28,849
checker_types 6,116/9,538 = 64.12% cases, 437,946/478,855 = 91.46% lines
```

§34 and §35 mint a deferred `keyof T` / `T[K]` as a print-only citizen: the
written text, registered in `unresolved_types` so `is_error` stays true and the
diagnostic channel is untouched. Both minted it with `TypeFlags::ANY`.

An `any` constituent **absorbs its whole union**. So `keyof T | keyof U` was
reduced to a bare `any` before anything could print it, and the corpus says so
in the plainest possible way — `keyofAndIndexedAccessErrors` holds 24 lines
reading:

```
want  keyof T & keyof U          got  any
want  keyof T | keyof U          got  any
want  (T | U)[keyof T & keyof U] got  any
```

**§36 had already met this exact problem and solved it, and recorded the reason
at the site**, in `declared.rs`'s template-literal arm:

> OBJECT rather than the §31 mints' ANY: a template mint in a UNION must not
> trip any-absorption or string-literal reduction.

So §812 is that one word, two constructs over. It is not a new idea; it is a
comment that had been sitting one match arm away for several hundred builds.

### Score, §812 alone

```
right 437,946 → 438,008  (+62)     gap 7,448 → 7,418     wrong 28,849 → 28,817
GAP→RIGHT 24   WRONG→RIGHT 44   |   RIGHT→WRONG 6   GAP→WRONG 6
```

The 6 `GAP→WRONG` are the finding, not the cost — see §813.

---

## §813 — the intersection gate was narrower than upstream, and the gap was hiding it

The six lines §812 turned from gap into wrong all wanted the same thing:

```
want   T[K] & ({} | undefined)          got   T[K]
want   T[K] & ({} | null)               got   T[K]
```

That is §85's `T & {}` family one construct over. **Before §812 the whole line
was `error`** — an `ANY` mint under narrowing stayed `any` — so the gap was
concealing the missing arm rather than protecting anything. Converting a gap into
a wrong line is how a build finds out what the gap was covering for, which is the
§90-diagnostics finding (*a decline can conceal a bug rather than prevent one*)
arriving from the other direction.

### What upstream gates on — read, not inferred

`get_type_with_facts` gated the intersection road on
`TypeFlags::TYPE_PARAMETER | TypeFlags::UNKNOWN`. Upstream gates on nothing of
the kind. `getAdjustedTypeWithFacts` (`checker.go:31159`) dispatches on the
**fact**, and `removeNullableByIntersection` (`checker.go:31179`) then maps over
the operand's constituents asking only `hasTypeFacts(t, targetFacts)` — *can this
constituent compare equal to the nullable I am removing?* A deferred `T[K]` can,
so upstream intersects it exactly as it intersects `T`.

The type-parameter gate was therefore an artefact of what this port could express
when §85 was written, and widening it to the deferred mints moves **toward**
upstream rather than away.

### Why a new set rather than an existing one

`Checker::deferred_index_mints`, populated at both mint sites. Not the existing
alternatives:

- **`unresolved_types` is too wide.** The §31 type-reference mint is in it, and
  upstream *has* a real resolved type for `Foo<T>` whose facts it can read.
- **`deferred_keyof_types` (§786) answers a different question** — *is this mint
  usable as a generic index?* — and holds only the `keyof` half, not the
  indexed-access half this arm needs.

Two sets over the same types, each named for the question it answers, is cheaper
than one set whose meaning depends on the caller.

---

## §814 — the ORDER was wrong, and that is what made the FIRST guard answer bare

With §813 in, three of the six converted and three did not. The split is the
finding:

| the reference | its operand | before §814 |
|---|---|---|
| the first `x;` after `if (x === undefined) return` | `T[K] \| undefined` | `T[K]` — **wrong** |
| the second `x;`, after a second identical guard | `T[K]` | `T[K] & ({} \| null)` — right |

The first narrowing sees the **union**; a union is neither a type variable nor a
mint, so the gate declined and the filter road answered a bare `T[K]`. Only the
*second* narrowing, whose operand is already that bare mint, reached the
intersection at all.

Upstream has no such asymmetry, because it does the two things **in order**:

```go
func (c *Checker) getAdjustedTypeWithFacts(t *Type, facts TypeFacts) *Type {
    reduced := c.recombineUnknownType(c.getTypeWithFacts(…, facts))   // FILTER first
    if c.strictNullChecks {
        switch facts {
        case TypeFactsNEUndefined:
            return c.removeNullableByIntersection(reduced, …)         // then INTERSECT
```

This port folded both into `get_type_with_facts` and put the intersection
**before** the filter, so the intersection could only ever see an operand that
was already free of its nullable. Restored: attempt the mint on the operand,
then, if the filter changed anything, on what the filter left. The mint block is
now `Checker::non_null_mint` returning `Option`, so both attempts share one
implementation instead of one being a copy of the other.

### The second attempt is restricted to deferred mints, and the restriction is measured

Ungated, the second attempt also reaches type parameters. Measured over the whole
corpus (on the §88.1-era rehearsal of this build, where the rows were
individually attributable):

| | lines |
|---|---|
| gained — `typeVariableTypeGuards:88` (`obj : NonNullable<T>`), `unknownControlFlow:349` (`y : T & ({} \| undefined)`) | +2 |
| lost — `indexedAccessConstraints:26` wants a bare `T` and got `NonNullable<T>`; `controlFlowGenericTypes:291` wants `T` and got `T & ({} \| null)` | −2 |

**Net zero for two regressed cases.** So the type-parameter half is left out and
filed as its own item: it needs a measurement of *when* upstream's `mapType`
declines, which is a different question from this build's. §85's measured road is
untouched, and the `// PROBE:` comment already sitting in that function
(`flow.rs`, the `NE_UNDEFINED_OR_NULL` arm) is about the same seam and is
likewise left alone.

---

## §814.1 — the score, and the one leg that fired

**LANDED.** `scorepair`, full run, against the `8187d2d9` baseline:

```
right 437,946 → 438,014  (+68)     gap 7,448 → 7,418     wrong 28,849 → 28,811
GAP→RIGHT 30   WRONG→RIGHT 44   |   RIGHT→WRONG 6   GAP→WRONG 0
gradient 91.46% → 91.47%
```

**Zero `GAP→WRONG`**: every one of the six §812 alone produced is converted by
§813 and §814.

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **74 gained, net +68** | PASS |
| 2 | RIGHT→WRONG ≤ 5 | **6** | **FIRED** — see below |
| 3 | not one case | 6 cases converted | PASS |
| 4 | cases regressed == 0 | 0 | PASS |

### Leg 2 fired at 6, and all six are one net-zero spelling swap

Every one is in `narrowingByTypeofInSwitch`, and **every one is paired by a
`WRONG → RIGHT` in the same case**: 6 out, 6 in, net zero.

The case asks for **two different spellings of the same narrowing at two
adjacent sites** (`narrowingByTypeofInSwitch.types:781-790`):

```ts
case 'number': assertNumber(k); assertKeyofS(k); return;
>k : number                 ← the argument to assertNumber(x: number)
>k : keyof S & number       ← the argument to assertKeyofS(k1: keyof S)
```

`k` is `keyof S` narrowed by `typeof k === 'number'`, and upstream prints the
narrowed type differently according to the contextual type at each site. This
port computes one position-independent narrowing and therefore prints **one** of
the two spellings at both. Before this build the `ANY` mint made that spelling
`number`; now the `OBJECT` mint makes it `keyof S & number`. Which site is right
was a coin flip, and the coin landed the other way round.

> **This was first written up as an alignment artefact, and that was wrong.** The
> baseline was then read and it holds two genuinely distinct `>k :` lines. The
> correction is recorded rather than edited away, because *"it is only an
> alignment artefact"* is exactly the kind of dismissal that should cost
> something to make.

The real owner is **site-sensitive narrowing spellings** — the per-site printing
context that §81 refused and that `TASK.md` lists as three separate heads. Not
this build's, and not gateable from here.

### What converted, by shape

| wanted shape | mechanism |
|---|---|
| `keyof T \| keyof U`, `keyof T & keyof U`, `(T \| U)[keyof T & keyof U]` | §812 — the mint no longer absorbs its union |
| `T[K] & ({} \| undefined)`, `T[K] & ({} \| null)` | §813 + §814 |
| `T[K] \| undefined`, `T[K] \| null` | §812 |
| `JSX.IntrinsicElements[T]` | §812, the indexed-access half |

Cases: `keyofAndIndexedAccessErrors` 24, `indexedAccessAndNullableNarrowing` 12,
`mappedTypeGenericWithKnownKeys` 6, `optionalParameterRetainsNull` 6,
`jsxIntrinsicElementsCompatability` 3, and the `narrowingByTypeofInSwitch` swap.

Pinned by `crates/tsr-checker/tests/deferred_index_mint.rs`.

### §814's residue, recorded as unmeasured rather than guessed

`T[K] | null | undefined` under two guards of opposite kind answers
`T[K] & ({} | undefined)`, not `T[K] & {}`: the first guard's filtered result is
the union `T[K] | null` rather than a mint, so §814's second attempt declines and
no prior kind is recorded for the second guard to refine.

**The expectation written first was `T[K] & {}`**, from reading
`removeNullableByIntersection`'s lattice. It was withdrawn on finding that the
string `X[Y] & {}` **occurs in no `.types` baseline in the corpus** — so there is
no evidence for either answer, and the test pins current behaviour while saying
in as many words that it is a description and not an oracle. The fix, if a
baseline for the shape ever appears, is to carry the prior kind through a
filtered union rather than only through a bare mint.

---

## §815 — what this family has left, sized and NOT built

Three heads, in the order the evidence supports. **None is built**; this section
states what exists, not what is coming.

1. **`keyof` over a CONCRETE operand — the premise §35 rests on is false.**
   §35 declines any operand that is not a bare type parameter, on the recorded
   grounds that *"concrete operands resolve upstream and decline"*. Upstream
   prints `keyof A` for a concrete `A` too: `getIndexTypeEx`
   (`checker.go:26684`) falls through to `getLiteralTypeFromProperties` — so the
   *type* is a union of key literals — but that function sets
   `origin = c.newIndexType(t, IndexFlagsNone)` whenever the operand is a
   `ClassOrInterface`, a `Reference`, or aliased, and the node builder prefers a
   union's origin: `nodebuilderimpl.go:3439`. An `Index` type then prints
   `keyof <target>` unconditionally (`nodebuilderimpl.go:3472`). The corpus wants
   `keyof FooBar`, `keyof Thing`, `keyof A`, `keyof React.ReactHTML` verbatim.
   **The caution that must travel with it**: upstream's *type* there has members,
   and a mint that prints `keyof A` without them will answer a later property
   lookup wrongly. Price it as a print-only citizen, not as a resolution.
2. **`MappedTypeNode` has no arm at all** — 291 non-right lines want a
   `[K in …]` text, and `{ [K in keyof P]: P[K]; }` is wanted verbatim in
   quantity. `Checker::written_type_text` (§77) is the bounded renderer to
   extend.
3. **`ConditionalTypeNode` has no arm at all** — 177 non-right lines, plus 71
   wanting `infer`.

### The family's ceiling, and the over-attribution that nearly got costed

**Under 2,000 lines, about 0.4 points.** Stated here because the first sizing of
this family was case-level and wrong by roughly 5×: 438 of the 12,444 corpus
cases use conditional, mapped or `infer` syntax somewhere in their source, and
those cases hold **9,396 non-right lines**. Quoting that as this family's
population would be `docs/conventions.md`'s rule — *a population identified by
the shape of the answer is not thereby attributed to a mechanism* — one level up:
a population identified by the shape of the **source**. A case that uses a mapped
type also has contextual-typing and inference defects, and those own most of the
9,396.

The line-level rows (`keyof` 662, mapped 291, conditional 177, `infer` 71, with
overlap between them) are the honest figure.

---

## §816 — `keyof <concrete>` prints its INDEX ORIGIN

§815's head 1, and the measurement turned it from *"build a mint"* into
**"print what is already computed"**, which is a much better item.

### The port already computes the right type

The census of non-right lines whose wanted text is exactly `keyof X`:

| want | what this port answers |
|---|---|
| `keyof Thing` | `"a" \| "b" \| "c"` |
| `keyof Object` | `"constructor" \| "hasOwnProperty" \| … ` (the correct seven) |
| `keyof JSX.IntrinsicElements` | the full, correct 180-odd tag list |
| `keyof A` | `"#fooField" \| "#fooMethod" \| "#fooProp" \| "bar" \| "baz"` |

**Every one of those key sets is right.** §730 already evaluates a concrete
`keyof` through `keys_of` + `literal_key_union`, and its own note records the
residue exactly: *"its 90 GAP→WRONG were the PRINTED form, which the
written-text arm in `signatures.rs` now supplies"* — supplied for *signature*
positions only, which is why the bare positions still print the expansion.

So this is not a type-construction item at all. It is the origin.

### Upstream's mechanism, and its gate

`getLiteralTypeFromProperties` (`checker.go`) sets

```go
origin = c.newIndexType(t, IndexFlagsNone)
```

when `includeOrigin && t.objectFlags&(ObjectFlagsClassOrInterface|ObjectFlagsReference) != 0 || t.alias != nil`,
and the node builder prefers it: `nodebuilderimpl.go:3439` —
`if t.flags&TypeFlagsUnion != 0 && t.AsUnionType().origin != nil { t = t.AsUnionType().origin }` —
after which the `Index` arm at `:3472` prints `keyof <target>` unconditionally.

**The gate matters and is honoured here**: an *anonymous* object type is
`ObjectFlagsAnonymous`, so `keyof { a: string }` gets **no** origin and prints
its expansion. Attaching the origin unconditionally would break every such line.

### The bar, registered before the code

The machinery exists — §53 built origin-carrying unions and
`create_union_with_text` already takes an `origin_text`.

- **Leg 1 (primary).** `gained ≥ 25`. The bare `keyof X` rows are 55 lines and
  581 mention `keyof <Capital>` inside a larger text, so composites should ride
  along; 25 is deliberately below the bare row.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 10`. This is the leg that can fire.
  A union with an origin text is a **distinct interned type** from the same union
  without one, so anything comparing key unions by identity can change answer.
- **Leg 3 (the gate's falsifier).** If anonymous-operand lines move at all, the
  `ClassOrInterface | Reference | alias` gate is not doing its job and the build
  is wrong however good the number looks.
- **Leg 4.** `cases regressed == 0`.

**What would make me wrong about the design.** If leg 2 fires above 10 and the
losses are lines where a key union is *consumed* rather than printed, then the
origin belongs on a separate print-side channel rather than on the type's own
interned text — which is the per-site printing architecture this page's §814.1
already names as an unowned head.

### §816's score — LANDED, and leg 3 fired twice before it passed

```
right 438,014 → 438,054  (+40)     wrong 28,811 → 28,771     gap 7,418 unchanged
WRONG→RIGHT 40   |   RIGHT→WRONG 0   GAP→WRONG 0
gradient 91.47% → 91.48%, cases 6,119 → 6,120
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 25 | **40** | PASS |
| 2 | RIGHT→WRONG ≤ 10 | **0** | PASS |
| 3 | anonymous operands do not move | **fired twice, then 0** | PASS after the fix |
| 4 | cases regressed == 0 | 0 | PASS |

**Leg 3 earned its place.** The first cut gated on *"the target is a type
reference, or a `Named` with a member table"*, which read `true` for two shapes
upstream gives no origin to, and the leg named both before the number could
disguise them (the leaky cut scored +38; the fixed one scores +40, so tightening
the gate was not even a trade):

1. **`checkJsObjectLiteralHasCheckedKeyof`** — the operand is
   `{ x: number; y: number; }`, upstream's **anonymous** object type, which takes
   no origin: the expansion `"x" | "y"` prints. This port stores a JS
   object-literal type as `TypeData::Named` with a *structural* text rather than
   as `TypeData::Anonymous`, **so the variant cannot distinguish them and the
   printed text has to**. A text test is a poor instrument and it is used here
   with that on the record; the principled fix is for the producer of that type
   to say which it is.
2. **`divideAndConquerIntersections`** — the operand is `Omit<Update, "update_id">`.
   Upstream's origin gate *does* include `t.alias != nil`, so reading the gate
   alone would license this — but `getIndexTypeEx` (`checker.go:26684`) never
   reaches `getLiteralTypeFromProperties` for it, because a mapped type is
   routed to `getIndexTypeForMappedType` several branches earlier. **This port
   has no mapped types to test for**, and the signal it does have is that the
   keys were found by *evaluating an alias body* rather than by reading a member
   table — so an alias-symbol reference declines.

> **The transferable part**: leg 3 was written as *"if anonymous-operand lines
> move at all, the gate is not doing its job however good the number looks"*.
> Both firings were gate defects rather than build defects, and neither would
> have been visible in the net, which went **up** when they were fixed. A bar leg
> pinned to a *mechanism's boundary* rather than to a count is what caught them.

**Residue, owned and not built**: a mapped-type operand declines by proxy (the
alias-body signal), not by knowing it is a mapped type. When
[§815](#815--what-this-family-has-left-sized-and-not-built)'s head 2 ports
mapped types, this predicate should be re-derived against the real test and the
text test in half 1 should go with it.

---

## §818 — a dynamic `import()` that cannot resolve is still a `Promise`

Found through §4.-6's near-miss board (`STATUS.md`), on the row *`want =
Promise<got>`*: every case in its head is named `importCallExpression*`, which is
about as uniform as a corpus row gets.

### What §140 built, and the three returns it left out

§140 landed `import("./m")` → `Promise<typeof import("./m")>` and its doc comment
says what it declines: *"an unresolvable or non-literal specifier, a module file
with no symbol, or a missing Promise global each decline"*. Three of those four
declines are **not** what upstream does. `checkImportCallExpression`
(`checker.go:8267`) has exactly three returns and all three are a promise:

```go
if len(args) == 0 {
    return c.createPromiseReturnType(node, c.anyType)      // :8273
}
…
if moduleSymbol != nil { … return c.createPromiseReturnType(node, syntheticType) }   // :8311
return c.createPromiseReturnType(node, c.anyType)          // :8314  ← the fall-through
```

The fall-through at `:8314` catches a non-literal specifier (upstream's own
comment: *"resolveExternalModuleName will return undefined if the
moduleReferenceExpression is not a string literal"*) **and** an unresolvable
module. `createPromiseType` (`:20348`) then answers `Promise<any>`, because
`getAwaitedTypeNoAlias(any)` is `any`.

The **missing-Promise-global** decline is the one that stays: upstream reports
`A_dynamic_import_call_returns_a_Promise…` and returns `errorType`
(`:20374-20379`), so declining there is faithful.

### The sizing, and the one row deliberately excluded

Non-right lines wanting `Promise<any>` and answering `any` or `error`:
**~67 lines across 12 cases**, head `importCallExpressionReturnPromiseOfAny` (12,
and the case name is the specification), `importCallExpressionSpecifierNotStringTypeError`
(11), `importCallExpressionDeclarationEmit1` (8), `importCallExpression6ES2020` (8),
`importCallExpression5ES2020` (8), `importCallExpressionGrammarError` (7).

**`Promise<unknown>` (47 lines) is NOT this item and must not be counted into
it**: its cases are `promisePermutations` (16), `promisePermutations3` (16),
`promisePermutations2` (12) — async-function return inference, a different road.
Checked rather than assumed, because the two shapes differ by one type argument
and the §4.-6 board's own warning is that one diff can hide two mechanisms.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 40` of the ~67.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 3`. Low, because this replaces an
  `errorType` with a real type at positions that were previously *gaps* — the
  ADR-0038 direction that cannot break a right line by construction. Anything
  above 3 means the new type is flowing somewhere a gap used to stop.
- **Leg 3 (the excluded row's falsifier).** `promisePermutations`,
  `promisePermutations2` and `promisePermutations3` must read **zero
  transitions**. If they move, the two shapes are one mechanism and the sizing
  above is wrong.
- **Leg 4.** `cases regressed == 0`.

**What would make me wrong about the design.** If leg 2 fires, then answering
`Promise<any>` where the specifier is unresolvable is feeding a real type into
consumers that were relying on the gap to stop — and the fix would be to keep the
promise for the *no-arguments* and *non-literal-specifier* returns only, leaving
the unresolvable-module one declining, which is a narrower build than upstream
but a measured one.

### §818's score — LANDED, every leg passed, and nothing regressed

```
right 438,054 → 438,145  (+91)     gap 7,418 → 7,386     wrong 28,771 → 28,712
GAP→RIGHT 32   WRONG→RIGHT 59   |   RIGHT→WRONG 0   GAP→WRONG 0
gradient 91.48% → 91.50%, cases 6,120 → 6,130  (+10)
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **91** — 136% of the sized 67 | PASS |
| 2 | RIGHT→WRONG ≤ 3 | **0** | PASS |
| 3 | `promisePermutations*` reads zero | **0**, verified by diff | PASS |
| 4 | cases regressed == 0 | 0 | PASS |

**Zero adverse transitions of any kind**, which is what leg 2 predicted on
principle: this replaces an `errorType` with a real type at positions that were
previously gaps, so it cannot break a right line by construction. The prediction
being *right* is worth as much as the number.

**Leg 3 was the one that mattered, and it is why the sizing is trustworthy.**
`Promise<unknown>` and `Promise<any>` differ by one type argument, and the 47
`Promise<unknown>` lines sit in `promisePermutations`, `promisePermutations2` and
`promisePermutations3` — async-function return inference, a different road. The
leg required those three to read **zero transitions** and they do, measured by
diffing the full dumps rather than by reading the top-three summary. Had they
moved, the two shapes would have been one mechanism and the ~67 sizing wrong.

Conversion was **136% of the sized row**, the §4.1 effect: 18 cases moved, all
dynamic-import, and the six the sizing did not name
(`importCallExpressionShouldNotGetParen`, `dynamicImportInDefaultExportExpression`,
`asyncImportNestedYield`, `importCallExpressionNested*`, `dynamicImportTrailingComma`,
`jsdocInTypeScript`) came along because the mechanism is the *call's own type* and
reaches every position downstream of it.

Pinned by `crates/tsr-checker/tests/import_call_promise.rs`, four tests: both
routes into the `:8314` fall-through (a non-literal specifier and an unresolvable
module), §140's return unchanged, and **the decline that stays** — with no
`Promise` global the call answers `errorType`, which is upstream reporting
`A_dynamic_import_call_returns_a_Promise…` at `:20374`.

---

## §819 — a heritage base reached through an IMPORT ALIAS

The `got = typeof want` row of §4.-6's near-miss board: ~50 lines whose head is
the `aliasUsageIn*` family, every one of them reading

```
want  Backbone.Model        got  typeof Backbone.Model
```

### The rule is ported; the receiver shape is not

Upstream's baseline writer carries an explicit workaround
(`type_symbol_baseline.go:371-377`):

```go
// Workaround to ensure we output 'C' instead of 'typeof C' for base class expressions
if ast.IsExpressionWithTypeArgumentsInClassExtendsClause(node.Parent) {
    t = fileChecker.GetTypeAtLocation(node.Parent)
}
if t == nil || checker.IsTypeAny(t) { t = fileChecker.GetTypeAtLocation(node) }
```

So the node **whose parent is the heritage `ExpressionWithTypeArguments`** records
the base *instance* type, while a node one level further in keeps its own. The
corpus shows both, two lines apart:

```
export class VisualizationModel extends Backbone.Model {
>Backbone.Model : Backbone.Model          ← parent IS the EWTA
>Model : typeof Backbone.Model            ← parent is the property access
```

`types_producer.rs` ports this as SS194/SS195 for an **identifier** base and §60
for a **qualified** base — `extends N.C`, resolving `N` with `SymbolFlags::NAMESPACE`
and reading `C` off its `exports`.

**`Backbone` is neither.** It is `import Backbone = require("./backbone")`, an
import-equals **alias to a module**, so its own symbol carries no `exports` and
`exports.get("Model")` answers `None` — §60's arm declines and the ordinary
expression road answers `typeof Backbone.Model`. The alias has to be followed to
the module symbol whose exports actually hold the member, which is
`checker.resolve_alias` and is already used twice in this same file.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 20` of the row's ~50.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 5`.
- **Leg 3 (the shadowing falsifier).** §479's decline must survive:
  `typeValueConflict1` and `typeValueConflict2` record `>M1.A : any` because a
  VALUE binding shadows the namespace, and their own comments say *"M1 should bind
  to the variable, not to the module"*. They must read **zero transitions** —
  following an alias must not smuggle the compensation past a shadow.
- **Leg 4.** `cases regressed == 0`.

**What would make me wrong about the design.** If leg 2 fires on self-extending
or circular bases, the alias hop is re-entering resolution where SS195's
fall-back is what upstream relies on, and the fix is to decline when the resolved
target is (or merges with) the extending class — which §60 already does for the
namespace case and would simply need to apply after the hop rather than before.

### §819's score — LANDED at +11, and **leg 1 FIRED because the sizing was wrong**

```
right 438,145 → 438,156  (+11)     wrong 28,712 → 28,701     gap 7,386 unchanged
WRONG→RIGHT 11   |   RIGHT→WRONG 0   GAP→WRONG 0
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 20 | **11** | **FIRED** |
| 2 | RIGHT→WRONG ≤ 5 | 0 | PASS |
| 3 | `typeValueConflict*` reads zero | 0 (no adverse at all) | PASS |
| 4 | cases regressed == 0 | 0 | PASS |

**The build is right and the forecast was wrong, and the two are separable.**
Eleven lines converted, nothing regressed, and every guard §60 already had is
still in force. What failed is the sizing: I priced *the whole `got = typeof
want` row* at the alias mechanism because its **head cases** were all named
`aliasUsageIn*`.

Splitting the 56-line residue by the shape of the wanted text settles it:

```
remaining   56 lines
  a QUALIFIED want (`A.Foo`, `base.W`, `Collision.Shapes.b2Shape`)     6
  a BARE want (`A`, `Base`, `C`, `D`, `(Anonymous class)`, `Enum`)    50
```

**50 of 56 have a bare want, so they never reach the qualified arm at all** —
they go through SS194/SS195's *identifier* arm and decline there for unrelated
reasons the residue names out loud: anonymous class expressions
(`classExpression3`, `missingPropertiesOfClassExpression`, 8 lines),
`export default abstract class`, enum/alias merges
(`importedEnumMemberMergedWithExportedAliasIsError`), mixins
(`declarationEmitMixinPrivateProtected`), `verbatimModuleSyntax`. The alias hop
was never going to touch them.

> **This is the third time in one session that a population was attributed from
> the shape of its case NAMES rather than from its mechanism** — after §4.-6's
> `arguments`-in-JS correction and the conditional/mapped family's case-level
> 9,396. The pattern is specific enough to name: **when a row's head cases share
> a naming prefix, that prefix is evidence about the row's provenance and none at
> all about its mechanism.** A fixture family is named after the *bug it was
> filed for*, which is often one instance of a rule that has many.
>
> The cheap defence, which would have caught all three, costs one `awk`: split
> the row by a property of the **wanted text** before pricing it. Here that is
> `want contains a dot`, and it would have forecast 6 rather than ≥20.

Landed at +11 with leg 1 recorded as fired, on the precedent of §87 (+3) and
`93b540a` (0, stated): a strictly-positive zero-adverse build is worth keeping,
and the forecast being wrong is a note about the forecaster rather than a reason
to revert the code.

---

## §821 — the conditional branch gate, re-priced against the TERNARY relation

The board says to build a subsystem; this is the cheapest real slice of one, and
**a prior note in the code points straight at it**. `evaluate_conditional_alias`
(`declared.rs`) decides a conditional alias's branch, and its general arm is
gated on a hand-rolled `primitive_domain` — both sides must be primitives — with
the reason recorded at the site:

> Both sides must sit in the relater's PROVEN domain — primitives, literals and
> unions of them. Outside it `is_type_assignable_to` answers `false` rather than
> guessing, which is a safe DECLINE but a confident WRONG DECISION here: false
> would pick the false branch. Measured: the ungated arm cost 58 adverse
> (`conditionalTypes1` 16, `recursiveArrayNotCircular` 15, `unknownType2` 13)
> against 47 gains.

**That cost belonged to a different design.** The relater became three-valued at
`d8590ff` (`bd tsr-kmzf`) — `relate_ternary` answers `Related` / `NotRelated` /
`Unknown` — so *"answers `false` rather than guessing"* is no longer true of the
instrument this gate was priced against. This is §4.2's standing conclusion
("the highest-expected-value move is re-measuring §5") applied to a refusal
inside the code rather than one on the board.

SS177 also left an instruction, and it is honoured rather than built on:

> the next attempt must find the real cause of `unknownType2`'s 13 rather than
> build on this.

### Upstream's rule, read in full — and it names `unknownType2`'s cause

`getConditionalType` (`checker.go:24300`), the non-deferred case at `:24372-24429`,
is exactly three outcomes:

```
FALSE branch  iff  extends is NOT any/unknown
                   AND ( check is any
                         OR !isTypeAssignableTo(permissive(check), permissive(extends)) )   :24377

TRUE branch   iff  extends IS any/unknown
                   OR isTypeAssignableTo(restrictive(check), restrictive(extends))          :24415

otherwise     DEFERRED                                                                       :24432
```

**`extends` being `any` or `unknown` takes the TRUE branch unconditionally, with
no relation test at all** — it is the first disjunct at `:24415` and it is
excluded from the false branch at `:24377`. `T extends unknown ? A : B` is
therefore always `A`. That is the mechanism SS177 could not name, and it is
*structural*, not a relation question — which is why no amount of widening the
relation domain would have found it.

### The design

Replace `primitive_domain` with upstream's own three outcomes:

1. `extends` is `ANY_OR_UNKNOWN` → **true branch**, no relation call.
2. Otherwise `relate_ternary(check, extends, Assignable)`:
   `Related` → true branch · `NotRelated` → false branch · **`Unknown` → decline**,
   which is the deferred outcome and the port's safe direction.
3. The `check is any` sub-rule (upstream unions *both* branches via `extraTypes`)
   is **declined and stated**, not approximated: it is a separate shape and
   folding it in would make this measurement unreadable.

The permissive/restrictive instantiations are not ported; the existing
`!mentions_any_type_parameter(check, 2)` guard stands in for `isDeferredType`,
so a generic check still defers and the two instantiation forms never differ.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 40`. The ungated arm already measured **47**
  gains; the any/unknown rule should add to that, and nothing here removes a gain.
- **Leg 2 (the whole point).** `RIGHT → WRONG ≤ 15`, against the 58 the binary
  relation cost. **If this fires above 15 the ternary is not the answer** and the
  primitive gate should go back with a second number on it.
- **Leg 3 (SS177's named falsifier).** `unknownType2` must **not** regress — it
  was 13 of the 58, and rule 1 is precisely what should fix it. If it still
  regresses, the cause is not the any/unknown rule and SS177's instruction is
  still open.
- **Leg 4.** `cases regressed ≤ 2`, loosened from the usual 0 because this widens
  a decision rather than adding a print, and stated as a loosening rather than
  applied quietly.

**What would make me wrong about the design.** If leg 2 fires mostly on
`conditionalTypes1` and `recursiveArrayNotCircular` (30 of the original 58), the
residue is the *deferral* rule rather than the relation: upstream defers a
conditional whose check or inferred-extends type is generic, and
`mentions_any_type_parameter` is a shallow stand-in for `isDeferredType`. The fix
would then be to port `isDeferredType` rather than to re-gate the relation.

### §821's score — LANDED at +37, and the hypothesis it was built on is CONFIRMED

```
right 438,156 → 438,193  (+37)     gap 7,381 → 7,367     wrong 28,692 → 28,683
GAP→RIGHT 12   WRONG→RIGHT 25   |   RIGHT→WRONG 0   GAP→WRONG 7
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **37** | **FIRED**, narrowly — see below |
| 2 | RIGHT→WRONG ≤ 15, against the binary relation's 58 | **0** | **PASS, and this is the result** |
| 3 | `unknownType2` must not regress | **0 transitions** | PASS |
| 4 | cases regressed ≤ 2 | 2 | PASS at the boundary |

**Leg 2 is the finding. 58 → 0.** Every one of the binary relation's
`RIGHT → WRONG` losses is gone, and the gate that was built to avoid them is
gone with it. The refusal's cost belonged to the binary relation, exactly as
§4.2's standing conclusion predicts for a refusal older than the machinery it
was priced against.

**Leg 1 fired at 37 against 40, and the explanation is the same fact.** The 47
"gains" the ungated *binary* arm measured were not all real: where the relation
could not tell, it answered `false`, and a `false` that happens to pick the right
branch scores as a gain. The ternary declines there instead. So **fewer gains and
far fewer losses is the expected shape of this change**, not a disappointment —
the trade moved from 47-for-58 to 37-for-0.

**Every remaining adverse line is `GAP → WRONG`, none is `RIGHT → WRONG`** — a
strictly cheaper class of trade, and the two families are named below.

### Two gates were built, measured and removed, each with its number

Both looked obviously right. Both cost more than they saved, and the code now
carries the number rather than the intuition.

**Gate (a), distributivity — cost 8 `RIGHT→WRONG` to fix 2.** Upstream's
`root.isDistributive` is a property of the check *node* (a bare reference to a
type parameter), and a distributive conditional over `never` or a union
distributes rather than testing — so declining that node shape removes
`distributiveConditionalTypeNeverIntersection1`'s 2 adverse. It also declined
**eight lines of `conditionalTypes1` that were already right**, because a
distributive conditional whose check has been *substituted to a concrete
argument* evaluates correctly by testing, which is what this road already did.
Distribution changes the answer only when the **substituted** check is a union or
`never` — a far narrower shape than "the node is a naked parameter". The decline
therefore belongs on the substituted type, which is a different build with its
own measurement.

**Gate (b), the unsubstituted branch — cost 8 `RIGHT→WRONG` to fix 5.** It
declined whenever the evaluated branch still mentioned a type parameter, on the
argument that an unsubstituted body is not an evaluation. But **a conditional's
branch legitimately IS a type parameter** in the deferred shapes
`conditionalTypes1` is made of, and upstream prints it. The gate could not tell
*"the frame failed to substitute"* from *"the answer is a type parameter"*, and
those are different facts.

> Both gates took the build from **+37 to +14**, and the two runs were
> byte-identical — which is how the second one was identified: removing (b) alone
> changed nothing, so the loss was (a)'s. Reverting both returned exactly
> 438,193, confirmed on a re-measure rather than assumed.

### The residue, stated

- `recursiveArrayNotCircular` 5 lines: the branch is chosen correctly and its
  body answers a bare `P`/`T` where upstream has `number`/`boolean`/`string`/`ActionType`.
  **The frame does not reach nested type parameters.** That is the real fix, and
  it is upstream of this site rather than in it.
- `distributiveConditionalTypeNeverIntersection1` 2 lines: genuine distribution,
  unported. Gate it on the *substituted* check being a union or `never`.
- The `check is any` sub-rule (`:24383-24386`, upstream unions both branches
  through `extraTypes`) remains declined and unported.

---

## §822 — the evaluated branch is INSTANTIATED, not just chosen: **BUILT, MEASURED AT ZERO, REVERTED, and the DIAGNOSIS IS REFUTED**

§821's named residue, and the interesting part is that the explanation was wrong.
`recursiveArrayNotCircular` reduces to four lines of source:

```ts
type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P }
type ReducerAction = Action<ActionType.Bar, number> | Action<ActionType.Baz, boolean> | …
```

§821 chooses the branch correctly — the check `P` *is* substituted, which is how
it passes the guard and reaches the relation — but the answers come out bare:

```
want ActionType   got T          (action.type)
want number       got P          (action.payload)
want boolean      got P
want string       got P
```

### The diagnosis, and the bar leg that killed it

I argued this was the **frame**: `alias_evaluation_bindings` is a stack popped on
exit, this port's object types carry `members: Option<SymbolId>` — a pointer at a
symbol table resolved at the *access* site — and that access happens in
`reducer`'s `switch`, long after the frame is gone. The fix followed: instantiate
the chosen branch eagerly with `instantiate_type` while the bindings are in hand,
the same idiom §136's default-fill uses twenty lines away.

**Leg 3 required those 5 lines to convert, and the full run read `no transitions
vs baseline` — literally zero, corpus-wide.** The leg was written as *"if they do
not convert, the frame is not the cause and the diagnosis is wrong, not merely
incomplete."* It is wrong.

### Why — and this is the part worth keeping

Two facts refute it, both checkable in seconds and neither checked before the
code was written:

1. **`mentions_type_parameter` is not blind through a members symbol.** It falls
   back to a *text scan* of `type_to_string` (`inference.rs:2010`), and
   `{ type: T; payload: P; }` contains both names, so `instantiate_type` would
   have proceeded. The "laziness seam" story predicted a blindness that does not
   exist.
2. **The `P`/`T` answers do not come from this site at all.**
   `evaluate_conditional_alias` is reached only `in_alias_declared_position`
   (`declared.rs:2143`). The failing lines are *property accesses* on the union's
   constituents, and those read `Action<ActionType.Bar, number>` as a **type
   reference**, substituting through `type_reference_targets` on the `bd tsr-4qx`
   member seam. §821 made those lines non-gap; it is not what answers them.

So instantiating this function's result could not have moved them, and a full-run
zero was the only possible outcome. **Reverted**: a 25-line no-op carrying a
refuted explanation is worse than nothing, and the §34 / `93b540a` precedent for
shipping a measured zero applies to a *correct* mechanism the corpus does not
exercise, not to a wrong one.

> **The transferable bit is the ordering.** Both refuting facts are one `grep`
> each — read the function I claimed was blind, and find who actually calls the
> function I was editing. I wrote 25 lines of code and a bar first. **When a
> diagnosis names a mechanism, confirm the mechanism is on the path before
> building on it**; a bar leg catches the error afterwards, which is what it did,
> but the greps were cheaper than the build.

### What the residue actually needs

The 5 lines belong to the **type-reference member road**, not the conditional
road: `Action<…>`'s members come from an alias whose body is a *conditional*, and
`get_type_of_property_of_type`'s substitution has no evaluated body to read
members off. The probe that would settle it is one line — print what
`get_type_of_property_of_type` answers for `payload` on
`Action<ActionType.Bar, number>`, and whether `type_reference_targets` holds the
alias or its evaluated branch. **Not done**, and not to be guessed at again.


---

## §823 — a CONDITIONAL-bodied alias reference gets its members from the chosen branch

§822's refutation said the residue belonged to the type-reference member road and
named a one-line probe. **The probe was run first this time**
(`crates/tsr-checker/tests/conditional_alias_members.rs`), and its control is
what localises the defect:

| fixture | `.payload` answers |
|---|---|
| `Plain<string, number>` where `type Plain<T, P> = { type: T, payload: P }` | **`number`** ✓ |
| `Action<ActionType.Bar, number>` where `type Action<T, P> = P extends void ? … : { type: T, payload: P }` | **`error`** |

So `bd tsr-4qx`'s member substitution is **not** the problem — it works perfectly
on a plain generic alias. One line decides which:

```rust
// alias_body_literal_symbol, declared.rs
let Some(TypeNode::TypeLiteralNode(literal)) = alias.r#type else { return None };
```

`create_type_reference_with_display` asks that helper for the member table an
instantiated alias reference answers lookups from (`§90`/`§46`), and it admits a
body that **is** a type literal and nothing else. A conditional body returns
`None`, the reference falls back to the alias's own symbol, whose member table is
structurally empty — and the access gaps.

**The corpus answers `P` rather than `error` for the same shape**, which the probe
also records: in `recursiveArrayNotCircular` the access goes through a *union* of
these references narrowed by `switch (action.type)`, so the bare `P` arrives on
the narrowed-union road. The direct reference declines outright. Two roads, one
cause.

### The design

Where the body is a conditional, evaluate it over **the reference's own
arguments** — which `create_type_reference_with_display` has in hand — and take
the members symbol of the chosen branch. Substitution then proceeds exactly as it
does for `Plain`, because `type_reference_targets` still records
`(Action, [Bar, number])` and the branch's `payload: P` is substituted through it.

This is the piece §822 guessed at and got wrong: the frame was never the issue,
the *admission test* was.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 5`, the named residue.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 5`. A reference that used to gap now
  answers, so consumers downstream of it change.
- **Leg 3 (the mechanism's own check, independent of the corpus).** The probe test
  must flip from `error` to `number`. If the corpus moves and the probe does not,
  the gain is something else.
- **Leg 4 (a PERFORMANCE falsifier, new for this build).** The corpus run must
  finish in its usual time. This adds a conditional evaluation to
  `create_type_reference_with_display`, which is hot, and
  `recursiveArrayNotCircular` is *genuinely recursive* — `Action<ActionType.Batch,
  ReducerAction[]>` where `ReducerAction` contains `Action<…>`. The
  `instantiation_depth` guard should bound it; a hang means it does not, and that
  is a failure rather than a slow pass.

### §823's score — LANDED at a MEASURED ZERO, and the tests are the deliverable

```
right 438,193 → 438,193      no transitions vs baseline
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 5 | **0** | **FIRED** |
| 2 | RIGHT→WRONG ≤ 5 | 0 | PASS |
| 3 | the probe flips `error` → `number` | **flipped, but only without the enum** | **FIRED, then explained** |
| 4 | the run finishes in its usual time | yes | PASS — the depth guard bounds the recursion |

**The mechanism is correct and the corpus does not reach it**, which is the §34 /
`93b540a` precedent and is why this ships where §822 was reverted: there the
mechanism was *wrong*. The difference is not a judgement call, it is a test —

```
Action<string, number>        → a.payload : number      ✓  §823 working
Action<ActionType.Bar, number> → a.payload : error      ✗  blocker 1
type Bar = Action<string, number>; a: Bar → a.payload : P   ✗  blocker 2
```

Leg 3 fired on the original probe, and **the explanation first written here was
wrong**. It said *"the enum argument, not the road"* and filed blocker 1 as *"an
enum member as a type argument does not resolve"*. **The corpus refutes that
flatly**: **5,723 RIGHT lines carry a dotted enum-member answer**
(`ambientEnum1` → `E1.y`, `assignToEnum` → `A.foo`), and 14,133 RIGHT lines carry
a dotted answer of any kind. Enum members in type position work.

### The real finding, and it invalidates a class of reasoning used four times

**The minimal unit harness is not a faithful oracle, and four of this block's
diagnoses leaned on it.** `type_of_last_expression` is `Checker::new` over **one
file with no `lib.d.ts` and no `ModuleHost`**, and it does not run
`types_producer`'s position rules. It answers *"does this arm fire"* well. It
answers *"can the port express this"* **badly**, and every time this session drew
the second conclusion from it the corpus disagreed.

So blocker 1 is withdrawn: what fails is the fixture. One blocker survives, and it
is the one the corpus corroborates —

**The alias-declared road hands back an UNINSTANTIATED branch.** Through an
intermediate alias the reference answers bare `P`, and
`recursiveArrayNotCircular`'s five wrong lines answer exactly that bare `P`/`T`.
`in_alias_declared_position`'s road (`declared.rs:2143`) evaluates the conditional
and never substitutes. **That is the next item**, and it is corpus-backed rather
than harness-backed.

> **What this block cost and what it bought.** §821 landed +37. §822 was built on
> a guess, measured zero, and was reverted. §823 was built on a *probe*, measures
> zero too — but its probe converted a vague residue into **three localised facts
> with a test each**, and the next session can act on any of them without
> re-deriving anything. The lesson §822 paid for is already visible in the
> difference between the two write-ups.

---

## §824 — an EXPANDO property's `@type` tag: **NOT LANDED, three zeros, and the diagnosis finally VERIFIED by instrumentation**

Corpus-evidenced population, and the cheapest-looking build left on the near-miss
board. It took three measured zeros to find out why, and the value of this section
is the verified end-state, not the attempt.

### The population and the shape, both sound

```
want `object`, got `any` or `error`   52 lines / 26 cases
  45 in JS-named cases, 44 in argumentsReferenceIn{Constructor,Method}1-6_Js
```

```js
class A {
    constructor(foo = {}) {
        /** @type object */
        this.arguments = foo;      // a PROPERTY named `arguments`; the name is incidental
    }
}
```

`argumentsReferenceInConstructor1_Js` localises it to three lines:

| line | verdict |
|---|---|
| `this.arguments = foo : object` | **RIGHT** — the `@type` *is* read for the assignment |
| `this.arguments : object` | GAP (`error`) |
| `arguments : object` | WRONG (`any`) |

### Three attempts, three corpus-wide zeros

1. **Widened `jsdoc_type_annotation` to `PropertyAccessExpression`.** Zero. Wrong
   node kind — guessed, not read.
2. **Read the binder and corrected it to `BinaryExpression`.**
   `bind_this_property_assignment` (`binder.rs:3500`, `:3525`) pushes the
   *`BinaryExpression`*'s `id` onto `declarations`, so the docs are one parent up
   on the `ExpressionStatement`. Still **zero**.
3. **Instrumented it** (`TSR_DEBUG_824`, the project's own env-gated idiom) and ran
   the single case. The answer is unambiguous:

```
824: jsdoc_type_annotation entered, kind=Parameter js=true
```

**Entered exactly once, for the constructor's `foo` parameter, and never for the
expando property at all.** Both arms were wiring a door this declaration never
reaches.

### The verified facts, so nobody re-derives them

- The expando property's declaration is the **`BinaryExpression`**
  (`binder.rs:3500`, `:3525`).
- **`jsdoc_type_annotation` is never entered for it** — instrumented, not inferred.
- **Nothing in `tsr-checker` reads `SymbolFlags::ASSIGNMENT`** for type
  computation; `grep ASSIGNMENT crates/tsr-checker/src` finds only diagnostics,
  flow flags and export-assignment messages.
- `get_type_for_variable_like_declaration` is called **unconditionally** at
  `symbols.rs:3544`, so the short-circuit is **upstream of it**, inside
  `get_type_of_symbol` for a `PROPERTY | ASSIGNMENT` symbol.

**The next probe is one instrumented run**: print which branch `get_type_of_symbol`
takes for that symbol. That is where the arm belongs — and it is an arm for a
declaration shape the road has none for, not a widening of an existing door, which
is what all three attempts here assumed.

### Why this is reverted rather than shipped as a zero

Same test as §822 against §823: §823's mechanism was demonstrably *correct* and
unreached by the corpus, so it shipped. This one **never fires at all** — it is
dead code by instrumented proof. A no-op arm plus a wrong story about where the
type comes from is worse than an empty space with a verified note in it.

> **The session's arithmetic on guessing.** §822, §823's corpus half, and §824's
> three cuts are five measured zeros, and every one of them came from assuming a
> mechanism was on the path instead of checking. The two checks that settled §824
> — `grep` the binder for what it files as the declaration, and one `eprintln!` at
> the function entry — cost minutes between them, and would have prevented all
> three cuts. **Instrument the path before widening anything on it.**


---

## §825 — mapped types in the bounded WRITTEN renderer

§815's head 2, and the first build of this block chosen by **instrumenting the
path before touching it** rather than after.

### The population is not what the census first said

A census of *"the want is a bare mapped type"* reads 125 lines, but almost every
one of those texts continues past the closing brace:

```
{ [P in K]: TakeString; }) => void
{ [K in keyof T]: T[K]; }) => T
{ [K in keyof S]: Reducer<S[K]>; }) => Reducer<S>
```

The `) => void` tail is the tell: **these are mapped types as parameter
annotations inside SIGNATURE prints**, not standalone type answers. 291 non-right
lines contain a mapped clause somewhere; 125 have one at the head of the want.

That relocates the build entirely — from `get_type_from_type_node` (where a
mapped type has no arm and answers `errorType`) to
`Checker::written_type_text_flags`, §77's bounded written renderer, which is what
a signature print reuses.

### Why this road is a materially safer bet than §822–§824's

Those three built on a mechanism they had not shown was on the path, and produced
five measured zeros between them. **This road is live by construction**: §77
landed **+370 at zero adverse** through `written_type_text`, §108 added generic
references to it and §730 added `keyof`. The question here is not *"is this
function called"* but *"does it have the arms these texts need"* — and it does
not.

### Exactly three arms are missing

Reading the existing match, `keyof` (§730), generic references (§108), arrays,
unions, parentheses, literals and type literals are all present. The mapped texts
above need three more:

| arm | spelling |
|---|---|
| `MappedTypeNode` | `{ ` *readonly?* `[` *name* ` in ` *constraint* ( ` as ` *nameType* )? `]` *question?* `: ` *template* `; }` |
| `IndexedAccessTypeNode` | `T[K]` |
| `IntersectionTypeNode` | `A & B` — needed by `keyof T & string` |

The two modifier tokens each have three spellings upstream, and the corpus shows
all of them: `-readonly [P in keyof T]: Awaited<T[P]>;`, `[x in K]?: Lower<T>[];`,
`[P in keyof T & string as Capitalize<P>]: V;`.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 40` of the 291.
- **Leg 2 (safety, and this is the real risk).** `RIGHT → WRONG ≤ 10`. Written
  reuse **replaces** a computed print, so a wrong spelling turns a right line
  wrong rather than merely failing to convert. Every new arm is a spelling claim.
- **Leg 3 (the flag falsifier).** `single_quoted` and `array_headed` must be
  threaded through **every** new recursive call. §77's whole gate is those flags;
  a mapped template containing `'a'` or an `Array<…>` head that does not set them
  is a silent divergence that no count would show. This is checked by reading, not
  by the number.
- **Leg 4.** `cases regressed ≤ 2`.

**What would make me wrong about the design.** If leg 2 fires on the modifier
spellings, the `+readonly` / `-?` forms are rarer than the corpus census suggests
and should be declined rather than guessed — decline is free here, because a
`None` from this renderer just keeps today's computed print.

### §825's score — LANDED at +6, leg 1 fired at 6 against 40, and the direction is EXHAUSTED

```
right 438,193 → 438,199  (+6)     wrong 28,683 → 28,677     gap unchanged
WRONG→RIGHT 6   |   RIGHT→WRONG 0   GAP→WRONG 0
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **6** | **FIRED** |
| 2 | RIGHT→WRONG ≤ 10 | **0** | PASS |
| 3 | the flags threaded through every new call | yes, by reading | PASS |
| 4 | cases regressed ≤ 2 | 0 | PASS |

The three arms are correct — `declarationEmitMappedTypeDistributivityPreservesConstraints`
(3), `indexedAccessTypeConstraints`, `invariantGenericErrorElaboration` — and they
cost nothing. **The 291-line population is simply not behind them**, and the
residue says what it is behind instead:

```
still failing with a mapped clause in the want   291 lines
  we answer `error`   151     the checker computes nothing — gap work
  we answer `any`     100
  a partial signature  ~40
```

and the head of it is not a mapped-type problem at all — it is **lib overload
sets**:

```
want  { <T>(values: Iterable<T | PromiseLike<T>>): Promise<Awaited<T>[]>;
        <T extends readonly unknown[] | []>(values: T): Promise<{ -readonly [P in keyof T]: Awaited<T[P]>; }>; }
got   any
```

That is `Promise.all`'s type. The mapped clause is incidental to it; what fails is
**printing a multi-signature interface type**, and the `awaitedType` family alone
accounts for the repeated rows. Rendering mapped types correctly cannot reach a
line whose enclosing print is an overload set the port cannot render at all.

> **The direction is exhausted, and that is the finding.** §815 filed mapped types
> as head 2 on the strength of 291 lines. The print-side arm for them converts
> **6**. The other 285 are two other subsystems — outright gaps (151) and
> overload-set printing — and no amount of work on the mapped renderer moves them.
> The population was real; the attribution to this mechanism was not, which is the
> naming-prefix error one more time in a new costume: **a want containing a
> construct is not a want blocked by that construct.**

---

## §826 — the gap boards can finally see the lines the producer hid

Zero gradient, and the point. §4.-5's correction measured that **8,826 of 13,295
audited `any` lines sit behind a branch that answered `error`** — the checker
computed nothing and the producer printed `any` anyway, faithfully, because
upstream's own baseline writer does (`type_symbol_baseline.go:383`). Every
gap-root instrument selects on `type_string == "error"`, so all of them were blind
to that population, and every row they have ever printed was a **floor presented
as a ceiling**.

### The blocker, and why it was not a one-liner

`type_id_at_location` **returns** `checker.intrinsics().any` at three sites where
the checker answered `error`, so the fact is gone before any caller sees it.
`any_audit.rs` recovers it only by **mirroring** the whole branch order in a
parallel implementation.

Fixed the way `tsr_conformance::verdict` already solves this class of problem —
one computation, so two probes cannot drift:

- `type_id_at_location_tracking(…, saw_checker_error: &mut bool)` is the existing
  body, with the flag set at exactly the three `error → any` substitutions. The
  other three `return any` sites are positions where *upstream* also prints `any`
  (label names and friends — the audit's WRITER rows) and are deliberately not
  flagged.
- `type_id_at_location` delegates with a throwaway, so **all fourteen existing
  callers are untouched and behaviour is unchanged by construction**. Verified,
  not asserted: `scorepair` across the change reads `right 438,199` with only
  §825's six transitions and nothing else.

### The A/B, because the claim is about an instrument

`TSR_NO_826=1` restores the old string-only selection, so both populations come
from one build:

```
old selection   7,432 lines walked
new selection   9,145 lines walked   (+1,713, +23%)
```

| row | old | new |
|---|---:|---:|
| `CallExpression / dependency` | 1,201 | 1,362 |
| `Parameter / no further dependency` | **979** | 985 |
| **`Identifier: the member NAME of a.b`** | **absent** | **864** |
| `PropertyAccessExpression / dependency` | 703 | 825 |
| `Identifier: symbol has no value declaration` | 264 | 385 |

**The new row is `Identifier: the member NAME of a.b` — 864 lines at want-any
`0.0%`.** It accounts for half the newly admitted lines and it is *exactly* the
SS183 position: a name whose parent is a property access, which is the branch that
substitutes `any` for `error`. The row that was invisible is the row the
substitution hides, which is the internal consistency check this change needed.

At **0.0% want-any** it carries no ADR-0038 ceiling at all — every one of those
864 lines is reachable in principle. It is now the most reachable row of its size
on the board.

> **I nearly published a wrong causal claim here.** The first read of the new board
> said `Parameter / no further dependency` (985, 0.4% want-any) was newly visible,
> because it appears on no board recorded in `STATUS.md`. The A/B shows it was
> already there at **979** — those boards are from the stale 84% base, not from
> this one. Two runs of one binary settled in minutes what cross-commit comparison
> would have got wrong. **A/B an instrument change inside one build; never against
> a recorded number from another base.**

### What this does not fix

1,713 of the ~8,800 became visible, not all of them. `depend.rs` also requires the
line to have the `name : type` shape (`want.text.strip_prefix("{got} : ")`), and
most of the remainder fail *that* filter, which is a separate and untouched
restriction. So the board's population is still a floor — just a much better one,
and now an honest one, because the count prints the §826 share beside the total.

---

## §827 — the member-name row splits, and 693 lines stop being "not an item"

§826 made the 864-line member-name row visible; this asks what is actually in it.
`access_reason` already separates the two readings, and the split is lopsided:

```
member name / property access, "the property has no type"      649
everything else (receiver has no such property, etc.)          215
```

`STATUS.md` §4.3 has read that shape the same way since the sixth session — *"a
downstream symptom: the property's own declaration gaps elsewhere. `bd tsr-mcd`
established this and it is not an item"* — and named the fix: *"follow to the
type-node roots, which is how tuples were found."*

`step` never took that edge. For a member name **and** for the access itself it
returned the **receiver**, so when the receiver typed, the walk stopped and the
board called the lookup the root. §827 adds the missing edge: when the receiver
types and the property resolves, step to the **property's declaration** (its name
node, which is what `step`'s declaration arm keys on).

### A/B from one binary, `TSR_NO_827=1`

| row | 827 off | 827 on | Δ |
|---|---:|---:|---:|
| `Identifier: the member NAME of a.b` | 864 | **693** | −171 |
| `PropertyAccessExpression / dependency` | 825 | **678** | −147 |
| `TypeReference / dependency` | 321 | 367 | +46 |
| `MappedType / NO STEP ARM` | 281 | 302 | +21 |
| `CallExpression / dependency` | 1,362 | 1,384 | +22 |

**318 lines leave the two symptom rows**, and where they land is §4.3's own
prediction coming true: the largest single destinations are `TypeReference` and
`MappedType` — *type-node* roots.

### The residue, CORRECTED at §828 — it is 339 lines, not 693

**The first version of this section was wrong and is replaced rather than
edited.** It read: *"693 lines stay on the member-name row after the edge exists…
that is a member-resolution defect"*, and it drew that from the row's size plus
its head case.

Two measurements refute it.

**§828 split the ending.** `gaps()` tests `!= error`, and **`any` passes that** —
so a dependency answering a *wrong* `any` looked like a typed dependency and the
walk stopped, blaming this node when the real root is upstream. The head case is
exactly that shape: `mixinAccessModifiers` wants `Protected & Public`, the
mixin's intersection return, and this port answers `any`, so its **receiver** is
wrong and its lookup is innocent. A new ending now says so — and it caught **137
lines, all on `CallExpression`** (28.5% want-any), not on the member-name row.

**And the row was never one item.** Reading `access_reason`'s own breakdown across
the §827 A/B settles the actual number:

| bucket | 827 off | 827 on |
|---|---:|---:|
| `member name, the property has no type` | 344 | **173** |
| `property access, the property has no type` | 305 | **166** |
| **total for that shape** | **649** | **339** |

So §827 re-rooted **310** of the 649 — those really were downstream, and §4.3's
reading was right about them — leaving **339** corpus-wide where the property's
declaration types fine and the lookup still fails. The rest of the 686-line row
is a *spread* of receiver-shape refusals, none an item on its own: `T` 51,
`this` 31, `unknown` 24, `typeof globalThis` 23, and a tail.

**The corrected claim**: `bd tsr-mcd`'s measurement stands, its blanket reading
holds for about half the shape on this base, and the real item is **339 lines at
0.0% want-any** — the `bd tsr-4qx` instantiation seam reached through a receiver
the substitution cannot serve. Not 693, and not one row.

> **This is the ninth population this session that needed splitting before it
> could be priced, and the sixth claim I have had to correct.** The pattern no
> longer needs restating case by case — it needs a rule, and the rule is that
> **a row is not an item until its own instrument's reason column has been read.**
> `access_reason` was printing the split the whole time, three sections below the
> row I was pricing. Every one of this session's misattributions — case-name
> prefixes, source-shape censuses, the harness-as-oracle, want-contains-construct,
> and this — is the same failure to read one column further before quoting a
> number.

### The two `depend.rs` fixes §4.0 deliberately left

§4.0's correction named two — relabel a length-1 cycle, and add `step` arms for
`ArrayType`/`TupleType`/`UnionType`/`IntersectionType` — and left them so the
board stayed comparable across sessions. **§827 breaks that comparability on
purpose and pays for it with the env gate**: both boards come from one binary, so
no future reader has to trust a number from another base. The same treatment is
what the remaining two should get.

---

## §829 — the 339 splits again, and the item is **187 lines with one mechanism**

§828 corrected the size; this asks the one question that decides whether any of it
is an item at all: **does the property have a type, or not?**

`access_reason` found the property symbol and then said *"the property has no
type"* — but it never asked `get_type_of_symbol` on that symbol. One call
separates two different pieces of work, and it was the column §828's own rule
said to read.

### The split

```
property access, the property TYPES; the projection fails; receiver generic reference   115
member name,     the property TYPES; the projection fails; receiver generic reference    72
                                                                                 ------
                                                                                    187

member name,     DOWNSTREAM: the property itself gaps; receiver named                    84
property access, DOWNSTREAM: the property itself gaps; receiver named                    41
member name,     DOWNSTREAM: the property itself gaps; receiver type parameter            14
                                                                                 ------
                                                                                   ~139 + tail
```

**187 lines where the property has a real type and only its projection through
*this* receiver fails — and the receiver is a generic reference in every one of
them.** That is `bd tsr-4qx`'s substitution seam, named precisely rather than by
analogy: the member table is found, the property's own type is computed, and
pushing the reference's type arguments through it does not produce the answer.

The remaining ~150 are genuinely downstream and §4.3's reading holds for them.

### Why this is the first properly-priced item of the block

Every earlier number on this row was a row, not a mechanism:

| reading | size | what it actually was |
|---|---:|---|
| §826's new row | 864 | a row |
| §827's residue | 693 | a row minus one edge |
| §828's correction | 339 | one *reason*, two mechanisms |
| **§829** | **187** | **one reason, one mechanism, 0.0% want-any** |

Each step was one column further into an instrument that was already printing the
answer. **The receiver-shape label is read off the printed text**, not the flags,
because the producer has no flags accessor and adding public checker surface for a
probe is worse than a coarse label that admits it is coarse — so treat
"generic reference" as *"the printed receiver contains `<`"*, which is what it is.

### What has NOT been done

The mechanism is localised and **not diagnosed**. The next step is one instrumented
run inside `get_type_of_property_of_type` on a line from the 187 — print the
receiver's target, its arguments, and the property's own type — to see *where* the
substitution is dropped. On this session's evidence that run should happen
**before** any arm is written: five of this block's builds measured zero because a
mechanism was assumed to be on a path it was not on.

### §829.1 — where the 187 actually live, and the question that is now open

`TSR_PROJECTION_CASES=1` names the cases behind the bucket, which the aggregate
cannot. **193 lines across 21 cases**, and it is concentrated:

```
56  conformance/genericCallWithConstraintsTypeArgumentInference
48  conformance/genericCallTypeArgumentInference
13  compiler/genericClassWithStaticFactory
12  compiler/recursiveTypeAliasWithSpreadConditionalReturnNotCircular
11  conformance/genericClassWithFunctionTypedMemberArguments
 8  conformance/genericClassWithObjectTypeArgsAndConstraints
    … 15 more
```

**104 of 193 sit in two cases, and both are named for generic-call type-argument
inference.** Their failing lines say the same:

```
want Object                                    got unknown
want number                                    got unknown
want Base                                      got error
want <U extends Derived2>(t: Base, u: U) => Base  got error / any
```

`want … got unknown` is §36's uninferred-parameter fallback firing where upstream
*did* infer — so on these lines inference is what did not happen, and the property
projection is downstream of it.

### The open question, stated rather than answered

Type-argument inference's remaining legs are **already refused, and well**:
`checker-notes-infer2.md` §6–§7, priority lattice **11 converts / 1 wrong**,
contravariant bucket **6 own-node lines** (and it needs `strictFunctionTypes`,
which this port does not model), **~17 together**, measured as deltas over the
shipped arm with at-risk **0** across 792 admitted right lines. That refusal is
not stale in the way §821's primitive gate was — it was priced against controls,
not against a superseded instrument.

**So the question is whether these 193 lines ARE those refused legs, or a distinct
projection defect that merely co-occurs in the same cases.** On this session's
record I am not going to assert either. Two facts bear on it and they point
different ways:

- *For the same mechanism*: the cases are the inference cases, and `got unknown`
  is the inference fallback.
- *For a distinct one*: the refused legs were sized at ~17 conversions over the
  whole corpus, and this bucket is 193 lines — an order of magnitude apart, which
  is exactly the gap that appears when a row is mistaken for a mechanism **in
  either direction**.

**One instrumented run settles it**: inside `get_type_of_property_of_type` on a
line from the 193, print the receiver's target, its type arguments, and the
property's own type. If the arguments are absent or `unknown`, this is inference
and the refusal governs. If they are present and correct and the projection still
fails, it is a separate defect and the 193 is a real item.

That run has not been done, and **writing an arm before it would be the sixth
zero-measuring build of this block** — five of them happened for exactly that
reason.

### §829.2 — the trace, and the open question is ANSWERED: the 193 is a real item

`TSR_PROJ_TRACE=<name>` prints what the projection has in hand. On the dominant
case's `c.foo(d1, d2)` / `i.foo(d1, d2)`:

```
PROJ `foo` on `C<Base, Derived>`: is_reference=true args=Some(["Base", "Derived"]) property_own_type=Some("(t: T, u: U) => T")
PROJ `foo` on `I<Base, Derived>`: is_reference=true args=Some(["Base", "Derived"]) property_own_type=Some("(t: T, u: U) => T")
```

Every input the substitution needs is present and correct:

| input | value |
|---|---|
| the receiver is a reference | **yes** |
| its type arguments | **`["Base", "Derived"]`** — inferred, and right |
| the property's own type | **`(t: T, u: U) => T`** — computed, not `error` |

**So inference already succeeded on these lines**, and `§829.1`'s test comes down
on the side it named: *"if they are present and correct and the projection still
fails, it is a separate defect and the 193 is a real item."* They are. It is.

`T → Base, U → Derived` into `(t: T, u: U) => T` should give
`(t: Base, u: Derived) => Base`, and the corpus wants `r4 : Base` from calling it.
The refused inference legs (~17 conversions, priced against controls) **do not
govern here** — they are about finding candidates, and the candidates are found.

### What is now known, and what the next arm must not assume

Known: the projection is reached with a reference receiver, correct arguments and
a typed property, and the answer does not come out. Not known: **which step drops
it** — the substitution itself, or the *call* through the substituted signature.
The failing lines in this case are call results (`r4`, `r5`, `r6`, `r7`) and
signature prints, so the call road is a live candidate and `§829.1`'s bucket
reached them through the member-name root rather than through the call.

That distinction is the next trace, not the next guess: print the substituted
signature before the call resolves. **This block's record is five zero-measuring
builds, every one from assuming a mechanism onto a path**, and the two traces that
have now run (`TSR_DEBUG_824`, `TSR_PROJ_TRACE`) each cost minutes and each
overturned a written conclusion.

The probe stays, env-gated, beside `TSR_DEBUG_2454` and `TSR_JOIN_DEBUG` — the
port's existing idiom for exactly this.

---

## §830 — a generic member projected through an instantiated reference keeps its OWN type parameters

### §829.2's evidence was wrong, and the corrected trace is sharper

§829.2 traced `TSR_PROJ_TRACE=foo` and concluded *"the inputs are present and the
projection fails"*. **`foo` is RIGHT.** `c.foo : (t: Base, u: Derived) => Base`
matches the baseline exactly, so I traced a *working* line and generalised from it.
The conclusion (*the 193 is real, not the refused inference legs*) survives; the
evidence for it did not, and the corrected trace names a much more specific
mechanism.

Tracing the methods that actually fail:

```
PROJ `foo`  on `C<Base, Derived>`: property_own_type = "(t: T, u: U) => T"                              ✓ RIGHT
PROJ `foo3` on `C<Base, Derived>`: property_own_type = "<T extends Derived>(t: T, u: U) => T"           ✗
PROJ `foo4` on `C<Base, Derived>`: property_own_type = "<U extends Derived2>(t: T, u: U) => T"          ✗
PROJ `foo5` on `C<Base, Derived>`: property_own_type = "<T extends Derived, U extends Derived2>(…)"     ✗
```

The split is **exactly** whether the member has its own type parameters:

| member | own params | corpus wants | port |
|---|---|---|---|
| `foo(t: T, u: U)` | none | `(t: Base, u: Derived) => Base` | ✓ |
| `foo4<U extends Derived2>(t: T, u: U)` | `U` shadows the class's `U` | `<U extends Derived2>(t: Base, u: U) => Base` | `error` / `any` |
| `foo5<T extends Derived, U extends Derived2>` | both shadow | as written | `error` / `any` |

### The defect, in one function

`instantiate_for_reference` (`members.rs`) builds its map from the **class's**
parameters to the reference's arguments and substitutes:

```rust
let names  = parameters.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>();
let types  = parameters.iter().map(|&(id, _)| id).collect::<Vec<_>>();
let map    = types.iter().copied().zip(arguments).collect::<Vec<_>>();
self.instantiate_type(declared, &map, &types, &names)
```

For `foo4` that map says `U → Derived`, and **`foo4`'s own `U` is a different `U`**.
Upstream never has this problem because it instantiates the *symbol*
(`instantiateSymbol`, `checker.go:19676`) and a generic signature's own parameters
are not in the class's mapper. Here the substitution is name-based —
`mentions_type_parameter` falls back to a text scan — so a shadowing name is
substituted anyway.

**The member's own type parameters must be removed from the map.** The caller at
`members.rs:1028` already holds the property symbol, so the shadowed names are one
declaration read away.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 40` of the 193; the two head cases hold 104.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 10`. These lines answer `error`/`any` today,
  so converting them is mostly gap→right — but a generic signature that now prints
  must print its parameter list and constraints exactly, and a wrong spelling is a
  wrong line.
- **Leg 3 (the control that says plain projection is intact).** `c.foo`,
  `c.foo2`, `i.foo` and the other **non-generic** members must stay RIGHT. They are
  what proves the change is confined to the shadowing case.
- **Leg 4.** `cases regressed ≤ 2`.

**What would make me wrong about the design.** If leg 2 fires on the constraint
spellings (`<T extends Derived>`), the printing of a preserved parameter list is a
separate arm from the substitution and should be declined until it is measured on
its own — the substitution can be correct while the render is not.

### §830's score — LANDED at +214, all four legs pass, ZERO `RIGHT→WRONG`

```
right 438,199 → 438,413  (+214)    gap 7,367 → 7,286    wrong 28,677 → 28,544
GAP→RIGHT 68   WRONG→RIGHT 146   |   RIGHT→WRONG 0   GAP→WRONG 13
gradient 91.51% → 91.55%, cases 6,130 → 6,133
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **214** — 111% of the sized 193 | PASS |
| 2 | RIGHT→WRONG ≤ 10 | **0** | PASS |
| 3 | non-generic members stay RIGHT | **0 R→W anywhere** | PASS |
| 4 | cases regressed ≤ 2 | **0** | PASS |

Converted across more cases than the sizing named — `genericClassWithStaticFactory`
51, `genericCallTypeArgumentInference` 36, `genericClassWithFunctionTypedMemberArguments`
35, `genericClassWithObjectTypeArgsAndConstraints` 32,
`genericCallWithConstraintsTypeArgumentInference` 30 — because the mechanism is
*any* generic member reached through *any* instantiated reference, not just the two
head cases.

### The 13 `GAP→WRONG`, priced and owned

**12 of 13 answer `unknown` where the corpus wants `Derived`/`Base`.** That is
§36's uninferred-type-parameter fallback: the call through the newly-projected
generic signature now resolves far enough to *attempt* inference, and inference
does not find the candidate. So these lines have moved from "no signature at all"
to "a signature whose type argument is not inferred" — and their owner is the
already-refused inference legs (`checker-notes-infer2.md` §6–§7, ~17 conversions
priced against controls). **§829.1 predicted exactly this**: *"if the arguments are
absent or `unknown`, this is inference and the refusal governs."* It governs here,
on 12 lines, and it did not govern the 214.

The 13th (`bivariantInferences:7`) is two pre-existing print concerns in one line:
upstream keeps a union of four *identical* signatures where this port reduces to
one, and spells `readonly T[]` where this port spells `ReadonlyArray<T>`. Neither
is this build's.

### What the chain cost and what it returned

```
§826  instrument   0 lines   made 1,713 lines visible at all
§827  instrument   0 lines   re-rooted 318; 864 → 693
§828  instrument   0 lines   corrected 693 → 339, split by reason
§829  instrument   0 lines   corrected 339 → 187, one mechanism
§829.1 probe       0 lines   located to 21 cases; found it landing on a refusal
§829.2 trace       0 lines   answered "real item" — on evidence that was itself wrong
§830  BUILD     +214 lines   the mechanism, once it was actually named
```

Six zero-gradient steps and then the largest build of the session. **The five
earlier zero-measuring builds of this block were all attempts to skip that
sequence** — and the one time the diagnosis was carried to a named mechanism with
a trace behind it, the arm converted 111% of its sizing at zero cost to right
lines.

### §830.1 — the same rule at the PROPERTY spelling: measured at zero, shipped with a test

`member_own_type_parameter_names` read only `MethodDeclaration` and
`MethodSignatureDeclaration`, so `foo: <U>(t: T, u: U) => T` — the *property*
spelling of the same two parameter sets — kept substituting the shadowing name.
Extended to `PropertySignatureDeclaration` and `PropertyDeclaration` whose type is
a `FunctionTypeNode` or `ConstructorTypeNode`.

**Corpus-wide: zero transitions.** Shipped on §823's test rather than §824's —
the distinction being whether the mechanism *fires*, which a unit test answers and
which is the one question the minimal harness is a good oracle for:

```ts
class C<T, U> { foo<U>(t: T, u: U): T { … } }      // §830   → <U>(t: string, u: U) => string
class C<T, U> { foo: <U>(t: T, u: U) => T; }       // §830.1 → <U>(t: string, u: U) => string
```

Both fire, and both are right: the class's `T` substitutes to `string`, the
member's own `U` survives. The corpus simply has no generic-function-typed
*property* on a generic class in its failing set.

> **My expected value in that test was wrong and the code was right** — I wrote
> `=> T` where the answer is `=> string`, having forgotten that the class's `T`
> *should* substitute. That is the sixth test expectation written from intuition
> in this project's history and the sixth time the port was right; `STATUS.md`
> §7's process notes already count five.

### §830.2 — the INHERITED half, also zero, also proven

`get_type_of_property_of_type` has **two** sites that pair
`get_property_of_type` with `instantiate_for_reference`: the own-member road (§830,
`members.rs:1028`) and the generic-heritage walk (`:1158`). §830 fixed the first and
left the second with the identical shape, so a generic method reached through a
generic **base** — `class D extends B<string>` reading `B<T>`'s `m<U>(t: T, u: U)` —
would still substitute the base's argument into the method's own shadowing name.

Same one-line fix. **Zero corpus transitions**, and pinned:

```ts
class B<T> { m<U>(t: T, u: U): T { … } }
class D extends B<string> { }
declare const d: D;
d.m;                                   // <U>(t: string, u: U) => string
```

Shipped on §823's precedent, not §824's: the arm fires and is correct, and the
corpus has no generic method inherited through a generic base in its failing set.

> **The three-site pattern is the transferable part.** §830 converted +214 at one
> of two structurally identical call sites; §830.1 and §830.2 cover the other
> spelling and the other road, each for zero. **When a defect is found at one site
> of a pair, fixing the sibling is free and the corpus will usually not show it** —
> which is an argument for doing it anyway, with a test, rather than waiting for a
> future session to rediscover the same bug through a different case.

---

## §831 — an OPTIONAL METHOD keeps its `?` in a type-literal print

### How this was found, and a measurement defect of my own

Chosen as *"overload-set printing"*, §825's named head. Sizing it found **998
non-right lines across 191 cases** whose want holds two or more call signatures —
the largest properly-measured population of this session. Sampling the 470 of
those that answer *something* showed the actual difference is one character:

```
want  { f(n: number): number; g(s: string): number; m: number; n?: number; k?(a: any): any; }
got   { f(n: number): number; g(s: string): number; m: number; n?: number; k(a: any): any; }
```

**An optional method loses its question mark.** Not overload sets.

> **A defect in my own scratch censuses, recorded because several numbers in this
> page came through it.** They stripped the `name : ` prefix with
> `sub(/^[^:]*: */,"",w)`, which removes everything up to the **first** colon — and
> in a signature text the first colon is inside `(values: …`. So long signature
> wants were silently truncated mid-expression, which is why the first overload
> census read **39 lines** where the correct strip reads **998**. The safe form only
> removes a leading `^[A-Za-z_$][A-Za-z0-9_$]* : `. Any number in this page taken
> from a long signature want should be re-derived with it.

Isolated exactly — lines whose want equals our got after deleting every `?(`:

```
44 lines across 13 cases
  typesWithOptionalProperty 7, assignmentCompatBug2 6, elementAccessChain 5,
  methodSignaturesWithOverloads2 5, declarationEmitOptionalMethod 3, … 8 more
```

### The fix

`get_type_from_type_literal` builds a method member as
`(method.node_id, Some(name), "", true)` and renders `{prefix}{name}{text}` —
`MethodSignatureDeclaration::postfix_token` is never read. The property half
already honours it (§77's `written_type_text` reads `postfix_token` for a
`PropertySignatureDeclaration`); the method half does not.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 30` of the 44.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 3`. The risk is emitting `?` where upstream
  drops it.
- **Leg 3 (the control).** A **non**-optional method must not gain a `?` — zero
  lines may move in that direction, and since the whole change is one character
  that control is what separates "reads the token" from "adds a character".
- **Leg 4.** `cases regressed == 0`.

### §831's score — LANDED at +56, all four legs pass, ZERO adverse

```
right 438,413 → 438,469  (+56)     wrong 28,544 → 28,488     gap unchanged
WRONG→RIGHT 56   |   RIGHT→WRONG 0   GAP→WRONG 0
gradient 91.55% → 91.57%
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 30 | **56** — 127% of the sized 44 | PASS |
| 2 | RIGHT→WRONG ≤ 3 | **0** | PASS |
| 3 | no non-optional method gains a `?` | **0 adverse anywhere** | PASS |
| 4 | cases regressed == 0 | 0 | PASS |

`methodSignaturesWithOverloads2` 10, `callChain.3` 8, `typesWithOptionalProperty`
7, and a tail — wider than the sized row because the same literal text is printed
at many positions.

**One character, +56 lines, zero risk.** The cheapest conversion of the session by
a wide margin, and it was reachable only because the 998-line census was taken with
a *correct* prefix strip: the buggy one read the same population as 39 lines and
would have priced this off the board entirely.

> **The sequence worth keeping.** *"Overload-set printing"* was the item §825 named
> and this section set out to build. It does not exist as a defect here — what the
> 998 lines contain is a missing `?`, and 470 of them were already printing every
> signature correctly. **The item was named from the shape of the want and the
> mechanism was one character inside it**, which is the same failure mode as
> case-name attribution, one level finer: *a want containing N signatures is not a
> want blocked by signature printing.*

---

## §832 — an OPTIONAL METHOD's type carries `| undefined`

The corrected near-miss census (see §831's measurement-defect note) puts a row at
the top that the buggy strip had shown as 16 lines:

```
105 lines / 38 cases    want `(fn) | undefined`, got `fn`
  optionalMethods 20, elementAccessChain 7, deleteChain 5, superMethodCall 5, …
```

### Two probes, and the second is the one that matters

The property spelling **already works**:

```ts
interface I { f?: () => void; g?: string; }
i.f   →  (() => void) | undefined     ✓
i.g   →  string | undefined           ✓
```

The **method** spelling does not:

```ts
interface I { f?(): void; }
i.f   →  () => void                   ✗  no `| undefined`
```

`is_optional_declaration` (`optionality.rs:218`) *does* list
`MethodSignatureDeclaration` and `MethodDeclaration`, so the token is read. The
question was whether the function is ever called for them — and
`TSR_DEBUG_832` answers it:

```
832: add_optionality_for_declaration kind=VariableDeclaration optional=false   ×8
832: add_optionality_for_declaration kind=Parameter           optional=false   ×3
832: add_optionality_for_declaration kind=PropertyDeclaration optional=true    ×3
832: add_optionality_for_declaration kind=PropertySignature   optional=true    ×1
```

**No method kind appears at all.** A method symbol's type is built by
`get_type_of_func_class_enum_module_worker` (`symbols.rs:2448`, the
`new_anonymous` signature type) and never passes through the optionality road —
the same shape as §831, where the method half ignored `postfix_token` in the
*print*; this is the *type*.

Routing that site through `add_optionality_for_declaration` is a no-op for every
non-optional declaration, and for an optional method gives
`get_optional_type(ty, is_property = false)` — which adds `| undefined`, because
upstream's `isProperty` (`checker.go:16674`) is `PropertyDeclaration ||
PropertySignature` and a method is neither. That is the wanted spelling exactly.

### The bar fired on both primary legs — REFUSED at −4, with the correct placement named

```
WRONG→RIGHT 28   |   RIGHT→WRONG 23   RIGHT→GAP 9   WRONG→GAP 1
right 438,469 → 438,465   =  −4
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 60 | **28** | **FIRED** |
| 2 | RIGHT→WRONG ≤ 10 | **23** | **FIRED** |
| 3 | the property spelling unchanged | held | PASS |
| 4 | cases regressed ≤ 2 | 4 | FIRED |

**Net negative, so reverted.** And leg 2 fired *exactly where it was written to*:
the bar said *"an optional call `i.f?.()` must not show it"*, and the losses are
`callChain.3` **10** — the optional-chain case — plus `assignmentCompatBug2` 8 and
`unionTypeReduction2` 4, all positions that strip or reduce the `undefined` again.

**The diagnosis stands; the placement was wrong.** Optionality on the *symbol's
type* is too broad: every consumer sees it, including the ones upstream expects to
strip it. Upstream does not put it there either — `getTypeOfFuncClassEnumModule`
adds nothing, and the `| undefined` a corpus line wants arrives at the
**property-access road**, which is also what lets an optional chain remove it
again. That is where the next attempt belongs, and it is a different function.

> **A process note worth more than the build.** The first measurement read **−4**
> against a baseline I had not re-accepted after §831, so it silently contained
> §831's +56 and the two were inseparable. Stashing §832, accepting the §831
> baseline, and re-measuring gave the clean **28 / 23 / 9 / 1**. This is the §88
> trap in its original form — *never measure against a baseline you have not
> re-accepted since your last landing* — and it cost one confused reading even with
> the rule written down two sections above.

Both spellings are now pinned in
`crates/tsr-checker/tests/conditional_alias_members.rs`: the property one as the
control that works, the method one as the refusal, **as it behaves**, with the
number and the correct placement in its doc comment.

### The original bar, for the record

- **Leg 1 (primary).** `gained ≥ 60` of the 105.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 10`. The risk is adding `| undefined` in a
  position that strips it — an optional *call* `i.f?.()` must not show it, and the
  type-literal text `{ f?(): void; }` must keep §831's `?` **without** gaining a
  union inside the braces (that text is built from the declaration, not the symbol
  type, so the two should not collide — but "should not" is what leg 2 is for).
- **Leg 3 (the control).** The property spelling must be unchanged: it already
  answers correctly through a different road, and 0 lines may move there.
- **Leg 4.** `cases regressed ≤ 2`.

### §832.1 — where the `| undefined` already comes from, and why that fixes the placement question

One more read, and it changes the item's shape again. These lines are **already
RIGHT** today:

```
conformance/callChain.3:  a?.m : (<T>(obj: { x: T; }) => T) | undefined
```

**The optional-CHAIN access road already adds the `| undefined` for an optional
method.** So §832 did not fail by adding something absent — it failed by adding it
a *second* time, at a site every consumer sees, and the chain road's own
strip/re-union then produced a different spelling. That is why `callChain.3` lost
10 lines it was getting right.

So the 105 are at the positions the chain road does **not** cover:

| position | today |
|---|---|
| `a?.m` (optional chain) | **already right** — `(fn) \| undefined` |
| `x.g` (plain access) | missing the `\| undefined` |
| `g` (the declaration name of `g?(): number`) | missing it |
| the symbol's type | missing it, and **must stay missing** — §832's −4 |

Upstream's own site was not located in this session. `getTypeOfSymbol` sends a
Method symbol to `getTypeOfFuncClassEnumModule`, which adds no optionality, and
the property road (`getTypeForVariableLikeDeclaration`) is gated to
`Variable | Property` — so the `(() => number) | undefined` on
`optionalMethods.types:15`'s **declaration-name** line arrives somewhere neither
of those explains. **That gap in the reading is the reason no third placement was
attempted**: two of the three sites are now known to already work or to be wrong,
and guessing the third with the upstream road unread is how §822, §824 and §832
each cost a build.

**The next step is one read, not a build**: find what upstream answers for the
declaration name of an optional method — `getTypeOfNode`/`getSymbolAtLocation`'s
road for a member name, not `getTypeOfSymbol` — and place it there. The plain
access and the declaration name are then the same fix, and the chain road must be
left alone.

### §832.2 — the read was done, and it says my model of upstream is INCOMPLETE

§832.1 named one read as the next step. It was done, and the result is negative in
a way worth recording rather than working around.

**Three roads read, and none of them explains the baseline:**

1. **The declaration-name road adds nothing.** `getTypeOfNode`
   (`checker.go:32000`) for a declaration name is
   `getSymbolAtLocation` → **plain `getTypeOfSymbol`** — no optionality, no
   `addOptionalityEx`.
2. **`getTypeOfSymbol` sends a method to a road that adds nothing.**
   `:16512` dispatches `Method` to `getTypeOfFuncClassEnumModule`, whose worker
   (`:16912`) has no optionality arm at all.
3. **The binder really does make it a Method, not a Property.**
   `binder.go:661` — `ast.SymbolFlagsMethod | getOptionalSymbolFlagForNode(node)`.
   So `Method | Optional`, and the `Variable | Property` branch at `:16509` is not
   taken.

Yet `optionalMethods.types:15` records `>g : (() => number) | undefined` for
`g?(): number`.

**So the `| undefined` arrives by a route I have not found.** The most likely
candidate on the evidence is `missingType`: `:11398` does
`removeMissingType(propType, prop.Flags&Optional != 0)`, and if an optional
member's type carries `missingType` then "removing" it under
non-`exactOptionalPropertyTypes` is what *produces* the `undefined`. That is a
hypothesis, not a reading — `getOptionalType`'s `isProperty` arm is the only
`missingType` producer I located, and it is not called for methods.

### Why this stops here rather than proceeding

This session has three reverted builds — §822, §824, §832 — and every one placed a
**correct diagnosis on an unread road**. The 105-line population is real, the chain
road is already correct, the symbol-type site is refused with a number, and the
remaining two sites are unbuildable until the route above is actually found rather
than inferred.

**The next session's step is still a read, and now a specific one**: follow
`missingType` — where it enters an optional member's type, and what
`removeMissingType` does to it under each setting of
`exactOptionalPropertyTypes`. §78 plumbed that option and minted `missingType`, so
the machinery exists in this port and the answer may be that the port's optional
*properties* already work through it while methods never acquire it.

**A negative read is a result.** Recording "three roads read, none explains it, here
is the fourth to try" is worth more than a fourth placement that measures −4.

---

## §833 — the `{ [x: string]: any; }` row dissolves into an UNPORTED FEATURE

From the corrected near-miss census: **36 lines / 6 cases** want
`{ [x: string]: any; }` and answer `any`. Head `mappedTypeRecursiveInference` 23,
then `commentsAfterSpread` 6, `computedPropertiesInDestructuring1` 2,
`objectRest` 2.

### The chain, read rather than guessed

1. **The baseline's position.** `computedPropertiesInDestructuring1.types:132`
   records `>{[foo2()]: bar3} : { [x: string]: any; }`. Reading the fixture, that
   line is in its *"// destructuring assignment"* section (source line 24 onward):
   `({[foo2()]: bar3} = {bar: "bar"})`. So the node **is** an
   `ObjectLiteralExpression`, not a binding pattern — the earlier `let {…} = …`
   forms are a different section and are not these lines.
2. **Upstream's rule.** `checkObjectLiteral` (`checker.go:13196`): a computed
   **string-typed** property name gives the literal a string index signature —
   `hasComputedStringProperty` → `getObjectLiteralIndexInfo(…, c.stringType)`.
   (`getTypeFromObjectBindingPattern` at `:17921` only adds one for a `...rest`
   element, which is why the *pattern* road does not explain these lines.)
3. **Why this port answers `any`, and it is not the index signature.**
   `objects.rs`'s own comment, written when the shorthand arm was added:

   > **Unobservable today and load-bearing under one named edit** … The only way to
   > write this form is as an assignment target, and `check_binary_expression` gaps
   > the whole assignment before this literal is ever checked — verified by making
   > this arm answer `never` and watching the result stay `error`. **It becomes live
   > the moment destructuring assignment is ported.**

**Destructuring assignment is not ported.** The literal is never checked, so no
index-signature arm could have converted these lines — the whole expression gaps
and the producer prints `any`.

### What this row is, then

Not a print slice and not an index-signature item: **36 lines of an unported
feature**, and the smallest visible piece of it. Building the computed-index arm
first would have measured **zero**, for the same reason §824 did — a mechanism
placed on a path nothing reaches.

The item is *destructuring assignment* (`checkDestructuringAssignment`), it is
subsystem-scale, and its size is unmeasured beyond these 36 lines. The port's own
comment has been pointing at it since the shorthand arm was written; this section
only supplies the corpus number.

> **Four rows traced to true causes this session, and not one was what its shape
> suggested**: the mapped row was overload sets that turned out to be a missing
> `?`; the 864-line member row was three mechanisms in a trench coat; the optional-
> method row is blocked on an unread upstream route; and this one is an unported
> feature. **The shape of a want has now failed as a predictor four times out of
> four** — which is the strongest form of this session's one finding.

### §833.1 — `checkDestructuringAssignment` sized, and it corrects my own recommendation

I named this subsystem as the one I would pick. Sized before starting it, which is
the rule this page has been accumulating:

```
statement-level destructuring assignment appears in   144 of 12,444 cases
those cases hold                                      6,175 right / 83 gap / 682 wrong
                                                      765 non-right = a CEILING
```

**765 is a ceiling, not a sizing** — those 144 cases have contextual-typing,
inference and printing defects like every other case, and this file's fourth rule
says a population is not thereby attributed to a mechanism. The mechanism's own
share is smaller, and §833's 36 lines are the only part measured to it.

Two things follow, and the second is the point:

1. A direct probe for the **assignment expression's own line** —
   `^[({[].* = .* : ` over the wants — finds **3 non-right lines**, all in one case.
   So the assignment expressions themselves are largely *not* what is failing; the
   36 are the *targets inside* them.
2. **At the observed 15–57% conversion band, 765 as a ceiling is +115 to +435.**
   That is a real subsystem's worth of work for less than §830's single arm
   returned (+214), and nowhere near the ~16,400 lines 95% needs.

> **So the recommendation is withdrawn as stated.** `checkDestructuringAssignment`
> is the best-*bounded* of the three remaining subsystems, which is not the same as
> the highest-value, and I had been treating those as interchangeable.
> Variadic-tuple and generic-call inference are both larger populations
> (`variadicTuples1` 267 wrong lines, `strictBindCallApply1` 204, and
> `genericCall*` was §830's own head at 104) — and both are refused with
> *controlled* measurements, which is a different and weaker kind of obstacle than
> "unported".
>
> **Whichever is chosen, the honest framing is that no single subsystem on this
> board closes the gap**: the three together, converted at the top of the observed
> band, are low four figures against a five-figure target. That is the measured
> version of §4.4's standing conclusion, and it has now been re-derived from the
> current base rather than inherited.

---

## §834 — a WRITTEN type argument reaches the contextual parameter

The largest re-measurable population left, and it is **not** the refused inference
legs — nothing is being inferred.

```
lines answering `unknown` where the want is a real type:  291 across 56 cases
  unknownType2 42, typeArgumentInference 19,
  typeArgumentInferenceWithConstraints 19, genericFunctionInference1 16,
  genericCallWithConstraintsTypeArgumentInference 16, …
top wants: number 89, string 40, {} 17, Base 16, string[] 10
```

Reading the head case's failing positions gives the shape in one line of source
(`typeArgumentInference.ts:51`):

```ts
function someGenerics6<A>(a: (a: A) => A, b: (b: A) => A, c: (c: A) => A) { }
someGenerics6<number>(n => n, n => n, n => n);     //  n is number
```

Probed directly: `f<number>(n => n)` answers **`(n: unknown) => unknown`** where
upstream answers `(n: number) => number`.

### Why the existing code is right about its own case and wrong about this one

`contextual_type_for_argument_resolving` (`contextual.rs:930-962`) is upstream's
**FIXING mapper**: a context consumed with no inference candidates fixes its type
parameters to `unknown` (`getInferredType`'s final leg, `inference.go:1317`). Its
comment states the intent exactly:

> This road fires only when NO memo exists, i.e. no pass-1 candidates were
> collected for this call, so the fill is total: `someGenerics6(n => n, …)` wants
> `(n: unknown) => unknown`, not the adopted `(n: A) => A`.

**That is correct — for the call with no type arguments.** `someGenerics6<number>(…)`
is the same road with `A` *already decided by the programmer*, and the fill
overwrites it with `unknown`. Upstream never infers there at all: written type
arguments instantiate the signature before arguments are checked.

So the map at `:954` should take a **written argument where one exists** and fall
back to `unknown` only for the positions that have none.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 40`. The 291 is the whole `unknown` population and
  most of it is genuine inference; the written-type-argument share is the part this
  can reach, and the two head cases hold 38 between them.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 10`. The `unknown` fill is *load-bearing* —
  `contextualTypingTwoInstancesOfSameTypeParameter` and
  `genericContextualTypes1` are named in the code as depending on it, and the §134
  return-mapper guard above it must keep working. This leg is the real question.
- **Leg 3 (the control).** A call with **no** type arguments must be unchanged —
  `someGenerics6(n => n, …)` must still answer `(n: unknown) => unknown`. If that
  moves, the fill has been widened rather than informed.
- **Leg 4.** `cases regressed ≤ 2`.

### §834's score — LANDED at +105, all four legs pass, ZERO `RIGHT→WRONG`

```
right 438,469 → 438,574  (+105)    gap 7,286 → 7,282    wrong 28,488 → 28,387
GAP→RIGHT 3   WRONG→RIGHT 102   |   RIGHT→WRONG 0   GAP→WRONG 1
gradient 91.57% → 91.59%, cases 6,133 → 6,135
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 40 | **105** | PASS |
| 2 | RIGHT→WRONG ≤ 10 | **0** | PASS — the `unknown` fill's dependents are intact |
| 3 | a call with no type arguments unchanged | pinned by test | PASS |
| 4 | cases regressed ≤ 2 | 1 | PASS |

`typeArgumentInference` 33, `typeArgumentInferenceWithConstraints` 33,
`mismatchedExplicitTypeParameterAndArgumentType` 24 — and **leg 2 was the real
question**, because the `unknown` fill is load-bearing for
`contextualTypingTwoInstancesOfSameTypeParameter` and `genericContextualTypes1`.
Zero right lines moved, which says the change *informs* the fill rather than
widening it: a position with a written argument takes it, every other position
still fixes to `unknown`. The control test pins exactly that.

The single `GAP→WRONG` is `importTypeGenericArrowTypeParenthesized:1:24`, one line,
unowned.

### Why this was reachable when the inference refusal was not

The 291-line `unknown` population reads as *"inference failed"*, and inference's
remaining legs are refused at ~17 conversions with controls — a refusal §832.2's
sibling reasoning would have honoured. **But nothing is being inferred on these
lines.** `someGenerics6<number>(n => n)` has `A` decided by the programmer, and
upstream instantiates from written type arguments *before* any argument is
checked, so the FIXING mapper never applies there at all.

> **This is the session's finding in its cleanest form.** The population was named
> by the shape of its answer — `got unknown` — and that shape pointed at a
> subsystem which is correctly refused. One read of the head case's source
> (`typeArgumentInference.ts:51`) showed a third of it is not that subsystem, and
> the fix was nine lines. **Four rows in a row dissolved when read; this one
> dissolved into something buildable.**

---

## §835 — `x === v` narrows an `unknown`

The largest unread piece of §834's 291-line population: `unknownType2` **42 lines**.
Read rather than assumed, and it is not inference either.

The failures want primitives and `object` where the port answers `unknown`, and the
source says why (`unknownType2.ts:107-117`):

```ts
declare const u: unknown;
if (u === NumberEnum)   { let enumObj: object = u; }      // wants object
if (u === NumberEnum.A) { let a: NumberEnum.A = u; }      // wants NumberEnum.A
if (u === StringEnum.B) { let b: StringEnum.B = u; }      // wants StringEnum.B
```

### Upstream's rule, and the arm this port does not have

`narrowTypeByEquality` (`flow.go:580-588`):

```go
if assumeTrue {
    if !doubleEquals && (t.flags&TypeFlagsUnknown != 0 || someType(t, c.IsEmptyAnonymousObjectType)) {
        if valueType.flags&(TypeFlagsPrimitive|TypeFlagsNonPrimitive) != 0 || c.IsEmptyAnonymousObjectType(valueType) {
            return valueType
        }
        if valueType.flags&TypeFlagsObject != 0 {
            return c.nonPrimitiveType
        }
    }
    …
```

So on an `unknown`, a `===` against a **primitive** value narrows to that value's
type, and against an **object** value narrows to `object`. That is both wanted
shapes: `u === NumberEnum.A` gives the enum literal, `u === NumberEnum` gives
`object` because the enum *object* is an object type.

`narrow_type_by_equality` (`flow.rs:6641`) has **no `UNKNOWN` arm at all** — it goes
straight to the constituent walk, and `filter_type` on a non-union `unknown` leaves
it whole. The port's `intrinsics.non_primitive` is already the `object` keyword's
type (`declared.rs:163`), so both answers are expressible today.

**Deliberately NOT ported**: the `IsEmptyAnonymousObjectType` half of the same
condition — the `{}` receiver and the `{}` value. It is a second predicate this port
has no equivalent of, and folding it in would make the measurement unreadable. The
`unknown` half is the whole build.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 25` of the 42.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 5`. This adds a *narrowing* where there was
  none, so anything downstream of an `unknown` reference in a true branch changes.
- **Leg 3 (the control).** `assume_true == false` and `==` (double-equals) must be
  untouched — the arm is gated on both, exactly as upstream gates it, and a
  regression in the false branch would say the gate was dropped.
- **Leg 4.** `cases regressed ≤ 2`.

### §835's score — LANDED at +23, ZERO adverse; leg 1 fired by 2 and the residue says why

```
right 438,574 → 438,597  (+23)     wrong 28,387 → 28,364     gap unchanged
WRONG→RIGHT 23   |   RIGHT→WRONG 0   GAP→WRONG 0        all in `unknownType2`
```

| leg | bar | read | |
|---|---|---|---|
| 1 | gained ≥ 25 | **23** | **FIRED by 2** |
| 2 | RIGHT→WRONG ≤ 5 | **0** | PASS |
| 3 | false branch / `==` untouched | 0 adverse | PASS |
| 4 | cases regressed ≤ 2 | 0 | PASS |

Leg 1 fired because the 42-line sizing was the *case's* `unknown` population, not
this arm's. The residue names the difference:

- `want true`, `got isTrue<true>` — a **conditional-type alias** left unevaluated,
  §821's family, not narrowing.
- `want string` / `boolean` / `number` / `object`, `got unknown` — the
  `IsEmptyAnonymousObjectType` half of upstream's same condition, deliberately not
  ported, plus forms this arm's gate excludes.
- `want 'idk' : "idk" | "no" | "yes"` — a contextual literal union.

**And the `typeof` sibling was probed before assuming it needed the same fix.** It
does not: `typeof u === "string"` on an `unknown` already answers `string`. §830.2's
*"fix the pair"* lesson does not apply when the pair is already correct — checking
cost one test and saved a build. Both are pinned.

---

## §836 — the `() => any` row REFUSED at 237 : 3,063, before any code

The largest unopened row of the corrected near-miss census, and it looked like the
best-shaped item left: **69 lines / 27 cases, every single want identical** —
`() => any` where the port answers `any`. Head `destructuringParameterProperties3`
24, `witness` 9, `functionImplementations` 6.

Reading the head case gives a coherent mechanism:

```ts
class C1<T, U, V> {
    constructor(private k: T, private [a, b, c]: [T,U,V]) { … }
    public getA() { return this.a }        // upstream: getA : () => any
}
```

A parameter property with a **binding pattern** is an error upstream, so the
members `a`/`b`/`c` do not exist, `this.a` is `any`, and `getA` is `() => any`. This
port cannot build the signature at all and answers `any` for the whole method.

So the candidate rule is: **a function whose return type cannot be computed still
has a signature, with `any` as its return** — rather than collapsing to `any`.

### The measurement that refuses it

Over every non-right line where the port answers `any` and the want is a signature:

```
want's return is `any`   :    237     convertible by this rule
want's return is real    :  3,063     would become WRONG lines
                                      void 668, string 567, number 452,
                                      boolean 72, unknown 34, this 32, …
```

**1 : 13 against.** The rule cannot distinguish "upstream also gave up here" from
"upstream computed `string`" — and the thing that would tell them apart is the
return type, which is exactly what is failing. Every refusal on this project's board
sits at a better ratio than this: `tsr-6ph` at 2.1 and 2.5, qualified naming at 2.7,
`removeSubtypes` at 1.03.

This is ADR-0038's argument arriving with a number: claiming `any` where nothing was
computed scores as right only on the lines where upstream also failed, and here those
are 7% of the population.

> **The row was refused for the cost of one `awk`, and the shape is what made it
> look safe.** 69 lines with a *single identical want* reads as the most uniform
> item on the board — and uniformity of the want is precisely what a want-shaped
> census cannot distinguish from uniformity of the mechanism. The 69 are the subset
> of 237 whose want happens to be nullary; nothing in the port can select them.
>
> **Five rows read this session, four buildable, one refused before it was built.**
> That ratio is the method working, not failing.

---

## §837 — indirect self-extension declines at ANY depth, not one hop

The `typeof` row of the corrected census splits cleanly in two directions, and the
second is a **loss** the port is currently taking:

```
we ADD  `typeof`  (got = typeof want)   56 lines / ~28 cases   — §819's residue
we OMIT `typeof`  (want = typeof got)   19 lines /   9 cases   — this
```

The 19 are all self-extension cycles: `classExtendsItselfIndirectly` ×3,
`classExtendsItselfIndirectly2`, `classExtendsItselfIndirectly3`,
`indirectSelfReference`, `recursiveBaseCheck`, `undefinedTypeAssignment4`. The
fixture is a **three-hop** cycle:

```ts
class C extends E { foo: string; }   // error
class D extends C { bar: string; }
class E extends D { baz: number; }
```

SS195 recorded the rule this depends on: a self-extending class has no resolvable
base, so upstream falls back to the identifier's own value type and records
`>C : typeof C`, while an ordinary `class B extends A` records `>A : A`. The port's
heritage compensation answers the instance type, so it must decline on a cycle.

**It declines on one hop only.** The identifier arm's guard is a *name comparison*
against the extending class (`owner_name != name.text`) — direct self-extension —
and §60's qualified arm adds exactly one more hop, with its own comment saying so:

> Self-extension DIRECT or through the base's own heritage … **one hop is what the
> corpus exercises.**

That was true of the corpus it was written against and is measurably false now: a
three-hop cycle walks straight past both guards, the compensation fires, and the
port answers `E` where upstream answers `typeof E`.

### The fix

Walk the resolved base's heritage chain for the extending class, with a visited set
and a depth cap, and decline if it is reached at any depth. Same decline as before —
only the detection changes.

### The bar, registered before the code

- **Leg 1 (primary).** `gained ≥ 12` of the 19.
- **Leg 2 (safety).** `RIGHT → WRONG ≤ 3`. Declining *more* can only lose lines the
  compensation was getting right, so this leg is asking whether any real
  (non-cyclic) base was caught by the walk.
- **Leg 3 (the control).** The existing direct and one-hop declines must be
  unchanged — `classInheritence` and `recursiveBaseCheck`'s already-right lines stay
  right. If they move, the walk has replaced the guards rather than extending them.
- **Leg 4.** `cases regressed == 0`.

### §837's score — LANDED at +21, ZERO adverse, all four legs pass

```
right 438,597 → 438,618  (+21)     wrong 28,364 → 28,343     gap unchanged
WRONG→RIGHT 21   |   RIGHT→WRONG 0   GAP→WRONG 0
gradient 91.59% → 91.60%, cases 6,135 → 6,140  (+5)
```

`classExtendsItselfIndirectly` 6, `classExtendsItselfIndirectly3` 6,
`indirectSelfReference` 2, and a tail.

**Leg 2 fired on the first two cuts and each firing named the next restriction** —
the same single line, `dynamicNames:21`, want `T1` against `typeof T1`:

1. **First cut walked every declaration kind.** Cost 1. Restricting to classes did
   **not** fix it, which is what said the cycle was not through an interface — the
   guess was wrong and the re-measure caught it before the reasoning did.
2. **Second cut walked every heritage clause.** Reading the case gave it away:
   `class T1 implements T2` beside `class T2 extends T1` is a cycle running through
   an **`implements`** clause, and upstream's fallback (SS195) is about a class with
   no resolvable *base*. `implements` is not a base. Restricting to
   `ExtendsKeyword` took the loss to **0**.

> **A one-line loss was worth two iterations.** It would have been easy to land
> +20/−1 at a 21:1 ratio and call the residue unowned — the trade passes every bar
> on this page. Reading the one line instead turned it into +21/−0 *and* produced
> the precise statement of the rule: the self-extension fallback follows the
> `extends` chain of **classes**, nothing else. The second cut also shows why
> re-measuring beats reasoning: "restrict to classes" was a plausible fix for the
> wrong cause, and only the number said so.

---

## §838 — the `Promise<IPromise<…>>` row is OVERLOAD SELECTION, read and not built

34 lines, 2 cases (`promisePermutations` 17, `promisePermutations3` 17), and the
diff is one wrapper:

```
want  Promise<IPromise<number>>
got   Promise<number>
```

The obvious reading is *"we unwrap a thenable upstream does not"*, and it is wrong.
`getPromisedTypeOfPromiseEx` (`checker.go:28926`) **would** unwrap `IPromise`: it
finds `then`, takes its candidates' first parameter, and reads that callback's first
parameter. Nothing in it declines a user-defined thenable.

The baseline's own context gives the real shape:

```ts
declare var s1: Promise<number>;
declare function testFunction(): IPromise<number>;

var s1a = s1.then(testFunction, testFunction, testFunction);
//  upstream:  Promise<IPromise<number>>
//  this port: Promise<number>
```

`Promise<T>` in that fixture declares **four** `then` overloads differing only in
whether each callback returns `Promise<U>` or `U`:

```ts
then<U>(success?: (value: T) => Promise<U>, error?: (error: any) => Promise<U>, …): Promise<U>;
then<U>(success?: (value: T) => Promise<U>, error?: (error: any) => U,          …): Promise<U>;
then<U>(success?: (value: T) => U,          error?: (error: any) => Promise<U>, …): Promise<U>;
then<U>(success?: (value: T) => U,          error?: (error: any) => U,          …): Promise<U>;
```

Upstream selects the fourth — `U = IPromise<number>`. This port selects one of the
first three, matching `IPromise<number>` against `Promise<U>` structurally and
inferring `U = number`. **No unwrapping happens anywhere; a different candidate
wins.**

### So the row belongs to overload selection

`STATUS.md` §4.2 prices overload selection's own gates at ~460 reachable lines at
**effort 5**, and the `undecidable pair` and `any` parameter gates there are two of
this project's own measured refusals working as designed. This 34 is a piece of that,
not a separate item, and there is nothing smaller inside it: the four candidates are
structurally distinguishable only by whether `IPromise<U>` should count as a
`Promise<U>`, which is the relation question the whole family turns on.

> **Third row this session to dissolve into an existing subsystem** — after §833
> (destructuring assignment) and §836's refusal. The reading cost four greps and no
> code, which is the only reason it is cheap to have been wrong about.

## §839: the `string |` narrowing family is not narrowing — it is `assumeInitialized`

The largest unopened near-miss population was the `string |` rows, ~131 lines split
in *two opposite directions*: 70 where the port **adds** `string |` (fails to narrow)
and 61 where the port **omits** it (over-narrows). Two opposite directions from one
shape is the signal that the shape is not the mechanism.

### What the 61 actually are

`typeGuardOfFormTypeOfString` supplies 14 of them, and all 14 are the **`else`
branch** of a `typeof x === "string"` guard. Every true branch is RIGHT:

```
A20  >strOrNum : string            true branch  — RIGHT
A23  >strOrNum : string | number   else branch  — want this, we answer `number`
A33  >strOrBool : string | boolean else branch  — want this, we answer `boolean`
```

The test's own comments say `// number` and `// boolean`, so the baseline
contradicts the test author's intent — which is what made this worth one more
column. `narrowTypeByTypeof` is not the answer: its false arm is
`getAdjustedTypeWithFacts(t, TypeFactsTypeofNEString)`
(`vendor/typescript-go/internal/checker/flow.go:646-653`), which filters `string`
out and would answer `number`.

The `.errors.txt` baseline is the answer:

```
typeGuardOfFormTypeOfString.ts(23,13): error TS2454: Variable 'strOrNum' is used before being assigned.
typeGuardOfFormTypeOfString.ts(29,5):  error TS2322: Type 'string | boolean' is not assignable to type 'boolean'.
```

Upstream *reports* the assignability error, so upstream's type at that reference
really is the full declared union. `checkIdentifier`
(`vendor/typescript-go/internal/checker/checker.go:11160-11192`), verbatim:

```go
default:
    initialType = c.getOptionalType(t, false /*isProperty*/)
...
} else if !assumeInitialized && !c.containsUndefinedType(t) && c.containsUndefinedType(flowType) {
    c.error(node, diagnostics.Variable_0_is_used_before_being_assigned, c.symbolToString(symbol))
    // Return the declared type to reduce follow-on errors
    return t
}
```

The whole mechanism, and it explains the true/else asymmetry exactly:

| | initial type | flow type | `undefined` survives? | answer |
|---|---|---|---|---|
| condition `typeof strOrNum` | `string \| number \| undefined` | unnarrowed | yes | **declared** `string \| number` |
| true branch | `string \| number \| undefined` | `string` | no | `string` |
| else branch | `string \| number \| undefined` | `number \| undefined` | **yes** | **declared** `string \| number` |

An uninitialized annotated `var` under `strictNullChecks` starts the flow walk at
`declared | undefined`; `TypeofNEString` keeps `undefined` because
`typeof undefined !== "string"`; the surviving `undefined` trips the
uninitialized-variable arm, which returns the **declared** type and discards the
narrowing. There is no narrowing defect here at all.

### Why the port cannot see it

The port has both halves and they are not connected. `check.rs`'s
`check_used_before_assigned` computes precisely `initial =
get_optional_type_unprinted(declared)` and the same `contains_undefined(flow)`
test (`crates/tsr-checker/src/check.rs:6534-6537`) — but it is a **diagnostic-only**
pass: it `report`s and returns `()`. The type road
(`crates/tsr-checker/src/expressions.rs:595`) calls
`get_flow_type_of_reference(node_id, Some(symbol), start)` with **no initial type**,
so the walk starts at `string | number`, `undefined` is never in play, and the else
branch narrows to `number`. Upstream fuses the two in one function; the port split
them, and the type half lost the rule.

### Population

WRONG lines whose `got` constituents are a **strict subset** of `want`'s: **278**.
The top missing constituent is `undefined` at **104** — the same root cause visible
directly, not via the return-declared-type half. The typeGuard family:

```
typeGuardOfFormNotExpr 16, typeGuardOfFormTypeOfBoolean 16,
typeGuardOfFormTypeOfString 14, typeGuardOfFormTypeOfNumber 12,
typeGuardsInModule 12, typeGuardOfFormTypeOfIsOrderIndependent 8,
typeGuardRedundancy 6                                          = 84 lines
```

### The bar, registered before any code

1. **Primary.** The 84-line typeGuard family: **≥ +50 net RIGHT**. Falsified below +30.
2. **Safety.** `RIGHT→WRONG ≤ 20`. The mechanism *widens* (declared in place of
   narrowed), so the exposure is every reference to an uninitialized annotated `var`
   where this port keeps `undefined` but upstream removed it — i.e. exactly where the
   port's narrowing is weaker. This is the leg that decides the build.
3. **Falsifier.** If threading `initial = declared | undefined` leaves
   `typeGuardOfFormTypeOfString` at 14 WRONG, the reading above is wrong and the
   change is reverted unmeasured-for-gain.
4. **Regression.** `cargo test -p tsr-checker` green; `clippy --all-targets -D warnings` clean.

### A prediction I registered and then measured wrong

I registered, as part of the bar, that the existing guard **must not be reused**:

> `reference_is_guarded_by_a_condition_on` returns true for a reference inside the
> branch of an `if` whose condition names the same identifier — i.e. for *every row in
> this family*.

That is false, and reading one function further would have shown it. The guard
requires the condition to name the reference **and** to contain
`subtree_has_unported_narrowing` — a call, an `instanceof`, or a `.constructor`
comparison (`crates/tsr-checker/src/check.rs`). A pure `typeof` condition, which is
every row of this family, does not match it. The guard was built for precisely the
distinction this build needed, by whoever wrote §8 and §58.

I built it the wrong way first and the corpus said so:

| | `WRONG->RIGHT` | `RIGHT->WRONG` | net |
|---|---|---|---|
| type road, no guard | 126 | **67** | +59 |
| type road, guard shared with the reporter | 126 | **20** | **+106** |

The 67 were `typeGuardOfFormIsType` 37 (`isC1(c1Orc2) && c1Orc2.p1`),
`typeGuardOfFormInstanceOf` 10, `parserindenter` 10 — the two un-modelled narrowings,
exactly the population §8's comment already named. Most of those rows read
`want string got any`, which is not a second mechanism but the **follow-on**:
`c1Orc2.p1` off an un-narrowed `C1 | C2` is an error, and `errorType` prints `any`
(ADR-0038). Sharing the guard cost **none** of the 126 wins.

So this landed as one predicate,
`Checker::uninitialized_variable_reads_declared`, holding the whole condition
including the guard, with two consumers — `check_used_before_assigned` for the
diagnostic and the identifier arm in `crate::expressions` for the type. That is also
nearer upstream, which asks one question and answers two things with it.

### Result

All four legs pass.

1. **Primary.** Predicted ≥ +50; measured **+106 net** (126 / 20). The six named
   cases went `14 -> 0`, `16 -> 0`, `16 -> 0`, `12 -> 0`, `6 -> 0`, and
   `typeGuardsInModule` `12 -> 6`.
2. **Safety.** `RIGHT->WRONG 20`, exactly at the registered bar — not under it. The
   residue is `parserindenter` 10, `typeGuardsInModule` 6,
   `classDoesNotDependOnBaseTypes` 2, `typeGuardsInExternalModule` 2, all the same
   over-widening: a narrowing upstream performs, this port does not, and
   `subtree_has_unported_narrowing` does not yet name. **This is the list to read
   before extending that mechanism set** — it is a ready-made falsifiable board for
   the next hand, and adding a fourth mechanism there is scored by whether these 20
   fall without disturbing the 126.
3. **Falsifier.** Cleared: `typeGuardOfFormTypeOfString` went 14 WRONG to 0.
4. **Regression.** `cargo test -p tsr-checker` 782 passed; `clippy --workspace
   --all-targets -D warnings` clean.

Pinned in `crates/tsr-checker/tests/uninitialized_reads_declared.rs` (5 tests: both
branches, the initialized control, the already-contains-`undefined` control, and the
§839.1 guard).

### What this row cost, and the one lesson

Eight tool calls of reading before any code: the branch asymmetry, the source, the
`.types` baseline with assertions numbered, `narrowTypeByTypeof`,
`getAdjustedTypeWithFacts`, the `.errors.txt`, `checkIdentifier`, and the port's two
disconnected halves. The decisive one was the **`.errors.txt` baseline** — TS2322
"Type 'string | boolean' is not assignable to type 'boolean'" proves upstream's type
at that reference is the full union, which no amount of reading `narrowTypeByTypeof`
would have revealed, because the narrowing is not where the answer comes from.

The lesson is the session's lesson again, with a new edge: *when a population splits
into two opposite directions, the shape is not the mechanism*. 131 lines that looked
like one narrowing bug in two flavours were one initialization rule and, in the other
70, presumably something else still unread.

## §839.2: the flow walk was given the wrong symbol, and the residue board named it

§839 left a 20-line residue and called it "the same over-widening, from a narrowing
`subtree_has_unported_narrowing` does not yet name". **Eight of the 20 were not that
at all — they were a defect in §839's own code**, and reading the board rather than
believing its label is what found them.

`typeGuardsInModule` 6 and `typeGuardsInExternalModule` 2 all read
`want string got string | number`, and `typeGuardsInModule` shows the discriminator
in one fixture: the plain `var var2` is RIGHT in both branches and the
`export var var3` is WRONG in the `then` branch.

A `TSR_DEBUG_839` probe over that case, for the two exported variables and them only:

```
839 var2: declared=string | number initial=string | number | undefined flow=string                     export_value=false fires=false
839 var3: declared=string | number initial=string | number | undefined flow=string | number | undefined export_value=true  fires=true
```

`flow` is the initial type *unchanged* — the walk found no narrowing at all and ran
to the top of the graph. §839's predicate resolved its symbol with
`SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE` (correct for the §251 structural
lookup) and then handed **that** symbol to `get_flow_type_of_reference_ex`. The type
road hands it the `SymbolFlags::VALUE` symbol. For an `export var` those are
different symbols, the export symbol fails every `is_matching_reference`, and the rule
then fired on every reference to an exported variable including the ones upstream
narrows.

Upstream cannot make this mistake: `getFlowTypeOfReferenceEx`
(`vendor/typescript-go/internal/checker/checker.go:11174`) is passed the reference
**node** and matches by `isMatchingReference`. The symbol parameter is this port's own.

### The wrong fix I nearly shipped

`assumeInitialized` has an `isModuleExports` disjunct
(`checker.go:11132`), and `export_value=true` fell out of the probe as a perfect
discriminator on this fixture. Gating on it would have been one line and would have
looked like a port of an upstream rule.

Two facts refuted it. First, `isModuleExports` is
`symbol.Flags&ast.SymbolFlagsModuleExports` — the `module.exports` symbol of a
CommonJS file, not an `export var`. Second, and decisively, upstream's
`typeGuardsInModule.errors.txt` reports TS2454 on `var3` **in the `else` branch
only** (`ts(28,20)`, not the `then` branch at `ts(26,…)`), so upstream narrows an
exported variable like any other and the arm fires for it exactly where it fires for
`var2`. Gating on `EXPORT_VALUE` would also have measured **net zero** — it buys the
six `then`-branch rows and gives back the six `else`-branch rows it is currently
getting right.

The real fix resolves the flow symbol with `SymbolFlags::VALUE`:
**+8, and not one `RIGHT->WRONG` line**. Both branches of an exported variable are now
right for the same reason the plain ones are.

### The residue after this: 12, and it is now two kinds, not one

- **`parserindenter` 10** — unread.
- **`classDoesNotDependOnBaseTypes` 2** — **not an over-widening.** Upstream records
  `>x : StringTree` (the declared type) inside `if (typeof x !== "string")`, so this
  port's new answer for `x` is *right*; the two wrong lines are `x[0]`, an indexed
  access on `string | StringTreeCollection` where the `string` constituent is now
  present again. A downstream gap newly exposed, §828's "root is UPSTREAM" category,
  not this rule's cost.

So of §839's 20-line leg-2 cost, 8 were a bug in the change, 2 are a downstream gap
made visible, and 10 are unread. **A leg that lands exactly at its bar is worth
reading line by line rather than accepting**: the bar said 20 and the honest number
was 10, and the difference was a symbol argument.

## §839.3: the loop cache was keyed by `(flow, symbol)` and not by the initial type

The last 10 of §839's residue were `parserindenter`, and they were **not** an
over-widening either. §839's type road never fires in that case at all — a probe at
the `return declared` site printed nothing for the whole file.

Three A/B runs located it, and the first two were wrong because of how they were
written:

1. Gating the type road as
   `uninitialized_variable_reads_declared(…) && env::var_os("TSR_NO_839").is_none()`
   changed nothing. **Rust's `&&` evaluates left to right**, so the predicate still
   *ran*; only its answer was discarded. The gate tested the wrong thing and read as
   "the type road is not the cause", which was true but not what the run showed.
2. Gating §839.2's symbol substitution changed nothing.
3. Returning `false` from the top of the predicate recovered all 10
   (`RIGHT 2036 -> 2046`).

So the cost is **the predicate's flow walk itself**, not its answer. The predicate is
now invoked from `check_expression_worker`, and it calls
`get_flow_type_of_reference_ex` with `initial = declared | undefined`.

`flow_loop_cache` is keyed `(flow node, symbol)` (`crates/tsr-checker/src/checker.rs`)
— **the initial type is not in the key**. `parserindenter` is
`while (parent != null && !parent.CanIndent())` over a `var parent: ParseNode;`. The
predicate's walk, seeded with `ParseNode | undefined`, converged and cached the loop
result; the real type query for the same `(flow, symbol)` replayed it; `parent`
answered `any` at nine sites plus the enclosing condition.

Upstream never needs the initial type in its key because `checkIdentifier` **walks
once** and uses the single answer for both the type and the diagnostic. This port
asks twice. Adding `state.initial_type` to the key is the minimum that makes two
callers safe: **+10, zero adverse**.

> The faithful alternative is to ask once — compute the flow type with the optional
> initial type on the type road and derive both answers from it, as
> `checker.go:11173-11192` does. That is the better shape and is not what landed
> here, because the two roads run in different traversals in this port and merging
> them is a larger change than this residue justified. **What would make it win**:
> a second consumer of the predicate, or any other measured cache interaction of
> this kind. Filed as reasoning rather than as a claim that the current shape is
> right.

### What §839 actually cost

| | |
|---|---:|
| §839 gross | **+126** |
| §839.1 (share the guard) | avoided **−47** |
| §839.2 (flow symbol) | **+8** |
| §839.3 (cache key) | **+10** |
| **net** | **+124** |
| genuine remaining cost | **2** (`classDoesNotDependOnBaseTypes`, a downstream indexed-access gap made visible, not an over-widening) |

The registered safety leg said "≤ 20" and the landing measured exactly 20. **None of
those 20 was the thing the bar was written about.** Eight were a symbol argument, ten
were a cache key, two were a pre-existing downstream gap. A leg that lands exactly at
its bound is the least informative outcome a bar can produce, and the only way to
tell it apart from a real cost is to read every line of it.

## §839.4: a correction to §839.3 — upstream's flow cache key DOES carry the initial type

§839.3 justified widening `flow_loop_cache`'s key with:

> Upstream never needs the initial type in its key because `checkIdentifier` walks
> once and uses the one answer for both the type and the diagnostic.

**That is false.** `writeFlowCacheKey`
(`vendor/typescript-go/internal/checker/flow.go:1665-1682`) writes the declared type
into the key and, when they differ, the initial type as well:

```go
b.writeByte(':')
b.writeType(declaredType)
if initialType != declaredType {
    b.writeByte('=')
    b.writeType(initialType)
}
if flowContainer != nil {
    b.writeByte('@')
    b.writeNode(flowContainer)
}
```

So the fix that landed is not a port-specific patch for a port-specific double walk —
it is **what upstream's key already does**, and this port's two-element key was
simply missing two of upstream's four components. The consequence matters for the
next hand: §839.3's "the faithful alternative is to ask once" framing was the wrong
recommendation. Asking once is still a reasonable simplification, but the key must
carry the initial type regardless of how many callers there are, because upstream's
does.

Still missing from this port's key, and unmeasured: the **flow container**
(`@` above) and the **declared type**. Both are reachable from `FlowState`. Whether
either is observable on this corpus is an open question and a cheap one — a third
and fourth tuple element and one `scorepair`.

I found this while reading `isMatchingReference` for §840, two functions below it. The
lesson is the one §839.1 already paid for once this session: **a claim about what
upstream does not do is worth the grep that would refute it**, and I wrote this one
having read only the call site.

## §840: `typeof a.b` does not narrow, because the binder records no flow for a `QualifiedName`

`narrowingOfQualifiedNames` (14 lines) localises cleanly. In

```ts
function init(properties: IProperties) {   // foo?: { aaa: string; bbb: string }
    if (properties.foo) {
        type FooOK = typeof properties.foo;   // A9  want { aaa: string; bbb: string; }
        properties.foo;                       // A13 RIGHT
```

the **statement** `properties.foo;` narrows correctly (A13–A15 all RIGHT); the
`typeof properties.foo` inside the type query does not (A9 answers
`… | undefined`), and the entity-name nodes inside it answer `any` (A10, A12).

`access_member_lookup` already ends in `get_flow_type_of_reference(id, None,
property_type)` keyed on the access node, so the call is there. What is missing is
upstream's two halves, both small and both exactly specified:

1. **The binder records no flow node for a `QualifiedName`.**
   `vendor/typescript-go/internal/binder/binder.go:605-608`:

   ```go
   case ast.KindQualifiedName:
       if b.currentFlow != nil && ast.IsPartOfTypeQuery(node) {
           node.AsQualifiedName().FlowNode = b.currentFlow
       }
   ```

   Note the gate — **only inside a type query**. A `QualifiedName` elsewhere is a
   type name, not a reference, and giving it a flow node would be a claim about
   nodes upstream deliberately leaves alone.

2. **`isMatchingReference` has no `QualifiedName` arm.**
   `vendor/typescript-go/internal/checker/flow.go:1639-1643`:

   ```go
   case ast.KindQualifiedName:
       if ast.IsAccessExpression(target) {
           if targetPropertyName, ok := c.getAccessedPropertyName(target); ok {
               return source.AsQualifiedName().Right.Text() == targetPropertyName &&
                   c.isMatchingReference(source.AsQualifiedName().Left, target.Expression())
           }
       }
   ```

   This is what equates the `QualifiedName` `properties.foo` in the type query with
   the `PropertyAccessExpression` `properties.foo` in the guard. Without it the walk
   finds the guard and declines to apply it.

### The bar, registered before any code

1. **Primary.** `narrowingOfQualifiedNames` 14 WRONG: **≥ +8**. Falsified below +4.
2. **Safety.** `RIGHT->WRONG ≤ 5`. The exposure is narrowing that now applies where
   it previously did not, in type-query positions only — which is why the binder gate
   is part of the port and not an optimisation.
3. **Falsifier.** If the case does not move at all, one of the two halves is not the
   mechanism and it is reverted rather than extended by guessing.
4. **Regression.** `cargo test --workspace` green; `clippy --all-targets -D warnings` clean.

**Not claimed:** that this fixes A10/A12, the `any` answers at the entity-name nodes
*inside* the query. Those are a separate question — whether `types_producer` finds a
recorded type at those positions — and if they remain `any` after the narrowing
lands, that is the next column to read, not a failure of this one.

### §840 result

All four legs pass, and one half of the bar was **already ported**.

`record_flow` (`crates/tsr-binder/src/binder.rs`) already had the `QualifiedName`
arm, gated on `is_part_of_type_query` exactly as `binder.go:605-608` gates it, with
the comment *"A qualified name only needs one inside `typeof X.Y`, where it denotes a
value that narrowing can have changed."* Whoever wrote it ported the binder half and
either did not reach, or did not need, the matching half — so the flow node was
recorded, the walk reached the guard, and `is_matching_reference` declined it. **A
one-arm gap, live behind a correctly ported binder for however long.**

| leg | registered | measured |
|---|---|---|
| 1 primary | `narrowingOfQualifiedNames` ≥ +8 | **+14** (the whole family) |
| 2 safety | `RIGHT->WRONG` ≤ 5 | **0** |
| 3 falsifier | case must move | moved |
| 4 regression | tests + clippy | 1,830 passed, clippy clean |

**+16 corpus-wide, zero adverse** — the 14 plus `controlFlowIfStatement` 2. Pinned in
`crates/tsr-checker/tests/type_query_narrowing.rs` (3 tests: the narrowing, the
outside-the-guard control, and the wrong-property control).

### The residue is the thing the bar declined to claim

The bar said explicitly: *"Not claimed: that this fixes A10/A12, the `any` answers at
the entity-name nodes inside the query."* They are still `any` —
`narrowingOfQualifiedNames` positions 9, 11, 24, 26, 36, 38. So the **type query
resolves correctly and the nodes inside it are still untyped**, which is a
`types_producer`/recording question rather than a narrowing one, and it is now
isolated: the same node the checker just computed a correct type for answers `any`
when asked for by position.

That is the next column, and writing the non-claim into the bar beforehand is what
makes it a lead rather than a disappointment.

## §841: the nodes inside `typeof a.b` have no road in `types_producer`

§840's declined residue, opened. `narrowingOfQualifiedNames` positions 9, 11, 24, 26,
36, 38 answer `any` where the oracle wants the property's type, and they are the
`QualifiedName` node `properties.foo` and its right-hand `Identifier` `foo`, both
*inside* a `typeof` query whose resolution §840 just made correct.

`types_producer` has an `IsRightSideOfPropertyAccess` arm
(`crates/tsr-conformance/src/types_producer.rs:610`) whose comment records what it
was for:

> Until this existed the name fell through to the identifier path and was resolved as
> a **free name in the enclosing scope** — `bd tsr-tl8`, and the one place this port
> could answer wrongly where a gap belonged.

Upstream's predicate is `isRightSideOfQualifiedNameOrPropertyAccess` — **one function
covering both spellings**. This port ported the property-access half. The qualified
half has no arm, and `check_qualified_name` does not appear in `types_producer` at
all, so neither the `QualifiedName` node nor its right identifier is ever typed.
`bd tsr-tl8`'s exact bug, live in the other grammar.

### The gate, and why it is not optional

A `QualifiedName` is *usually* a **type** name — the `M.I` of `let x: M.I` — and the
oracle prints a type there, not a value type. Typing every `QualifiedName` as a value
expression would turn a large population of correct type-reference lines into
confident wrong ones. The arm is therefore gated on the node being part of a
**type query**, which is the same gate the binder already applies when it decides to
record a flow node at all (`binder.rs`, `binder.go:605-608`). Inside `typeof`, and
only there, a qualified name denotes a value.

### The bar

1. **Primary.** `narrowingOfQualifiedNames` ≥ **+6**. Falsified below +3.
2. **Safety.** `RIGHT->WRONG ≤ 10`. The exposure is any `typeof M.x` line that is
   right today by some other route, plus `check_qualified_name` answering
   confidently where an honest gap stands (the §121 purely-nullish arm answers `any`,
   which prints, rather than gapping).
3. **Falsifier.** If the six named positions do not move, the road is not the one
   `types_producer` takes for them and the change is reverted rather than widened.
4. **Regression.** `cargo test --workspace` green; clippy clean.

### §841 result — the largest single landing of the session, from a residue the previous bar declined

| leg | registered | measured |
|---|---|---|
| 1 primary | `narrowingOfQualifiedNames` ≥ +6 | **+78** (the case went `90 WRONG -> 12`) |
| 2 safety | `RIGHT->WRONG` ≤ 10 | **1**, plus 2 `GAP->WRONG` |
| 3 falsifier | the six positions must move | moved |
| 4 regression | tests + clippy | 1,833 passed, clippy clean |

**`WRONG->RIGHT 242`, `GAP->RIGHT 69`, against 3 adverse lines: +310 right lines.**

The gains are far wider than the case that surfaced it, because the mechanism is any
`typeof` over a dotted path anywhere:

```
narrowingOfQualifiedNames 78, uniqueSymbols 23, uniqueSymbolsDeclarations 23,
declarationEmitGlobalThisPreserved 53 (gap->right), typeQueryWithReservedWords 3,
typeofUsedBeforeBlockScoped 3
```

The three adverse lines, each owned:

- `jsxLibraryManagedAttributesUnusedGeneric:0:2` wants **`error`** and gets `any`.
  The arm mirrors the property-access arm's `computed == error -> return any`, which
  that arm's SS183 comment justifies deliberately. Keeping the two spellings
  symmetric is worth one line; splitting them would need its own bar.
- `recursiveFunctionTypes1:0:3` and `symbolProperty61:0:3` are `GAP->WRONG`, which is
  **the only adverse direction this arm can produce** for a node nothing typed before
  (§620's accepted direction). The first is a recursive `typeof C.g`; the second wants
  `unique symbol` and gets the widened `symbol` — in a family this change took
  **+46** in.

### What the two entries together say about bars

§840's bar wrote down what it would *not* claim. That non-claim was worth **+310**,
twenty times §840's own +16, and it was sitting behind a landing that had already
passed all its legs. Neither the gap-root board nor the near-miss census pointed at
it: it only became visible because §840 fixed the resolution and left the six `any`
rows standing next to a now-correct answer.

**A residue named in advance and left standing is a lead with a known location. The
same lines, unnamed, are indistinguishable from the corpus's noise.**

## §842: REFUSED at 136 lines and ZERO cases — the flow-depth bail

`compiler/parsingDeepParenthensizedExpression` heads the wrong-line board outside the
priced subsystems at **137 WRONG**, and **136 of those want exactly `error`**. It is
the whole of the "oracle itself says `error`" population: across the corpus only
**153** WRONG lines want `error`, and the remaining 17 are spread one and two at a
time over nine cases.

The mechanism is understood. Upstream's `getTypeAtFlowNode`
(`vendor/typescript-go/internal/checker/flow.go:118-127`) bails at `f.depth == 2000`,
sets `c.flowAnalysisDisabled`, reports TS2563 and returns `errorType`; the flag is
global and sticky, so every later reference in the file answers `errorType` too. The
file is `a.js` inside a `.ts`-named case, and §14.1's JS half would print that
verbatim.

**The port never trips the bail at all.** A `TSR_DEBUG_842` probe at the bail site
printed nothing for the whole file, so `e` simply types as its implicit `any`.

Two hypotheses checked and one killed on the way:

- *"Upstream's `f.depth++` has no matching decrement, so it counts total calls per
  walk while the port counts recursion depth."* **False** — upstream decrements at
  `flow.go:138` and `flow.go:203`. One grep, and it was the reading the whole theory
  rested on.
- The remaining explanation is a **difference in flow-graph shape or in where the two
  walks recurse**: a long comma-and-assignment chain costs upstream no depth either
  (its loop follows antecedents without incrementing), so the 2,000 must accumulate
  at the nested `&&` branch labels, and the port's `BRANCH_LABEL` arm iterates where
  upstream recurses. Unverified.

### Why it is refused rather than sized

The case is **3,765 RIGHT / 137 WRONG**. Fixing all 136 leaves 1 WRONG, so the case
**still fails**:

| | |
|---|---:|
| lines recoverable | 136 (**0.028%** of the corpus) |
| cases recoverable | **0** |
| what it would take | a divergence in flow-graph shape or recursion structure, in `crate::flow`'s hottest function |

A change to `get_type_at_flow_node`'s recursion to win 0.028% and no cases, in the
function every narrowing in the port passes through, is the wrong trade at any effort.
**Refused at 136 : 0**, and the number that refused it is the case count, not the line
count — which is the distinction `STATUS.md` §4.-7's board did not previously carry.

If it is ever revisited, the falsifier is cheap and stated: instrument
`get_type_at_flow_node`'s max observed depth for this one file and compare it against
2,000. If the port's maximum is within an order of magnitude, the graph is the same
shape and the accounting is the bug; if it is 20, the graph is different and this is a
binder item, not a checker one.

## §843: an ALIAS is narrowable, and `is_narrowable_symbol` tests `VARIABLE` alone

`narrowedImports` is 29 RIGHT / 10 WRONG, so the whole case turns on it. Every wrong
line is the same:

```ts
import a0, { a1, a1 as a2 } from "./a";   // a0 : number | undefined
if (a0) x = a0;
>a0 : number      <- A11, the guarded reference; this port answers `number | undefined`
```

Truthiness narrowing of an **imported binding** does not happen. `checkIdentifier`
(`vendor/typescript-go/internal/checker/checker.go:11104-11118`) is explicit, and its
comment is the specification:

```go
isAlias := localOrExportSymbol.Flags&ast.SymbolFlagsAlias != 0
// We only narrow variables and parameters occurring in a non-assignment position. For all other
// entities we simply return the declared type.
if localOrExportSymbol.Flags&ast.SymbolFlagsVariable != 0 {
    ...
} else if isAlias {
    declaration = c.getDeclarationOfAliasSymbol(symbol)
} else {
    return t
}
```

Three outcomes, and **an alias is the second of them** — it narrows, taking the alias
declaration as the declaration for the container comparison that follows. This port's
`is_narrowable_symbol` (`crates/tsr-checker/src/flow.rs`) is one line and tests
`SymbolFlags::VARIABLE` only, so an alias falls into upstream's *third* outcome and
keeps its declared type.

### The bar

1. **Primary.** `narrowedImports` ≥ **+8 of 10**, and the case passes. Falsified below +4.
2. **Safety.** `RIGHT->WRONG ≤ 10`. The exposure is every alias that is *not* an
   aliased variable — an imported class, function, enum or namespace — which now
   enters the flow walk. Upstream lets those in too, so a loss here is a difference
   in what the walk does with them rather than in the gate.
3. **Falsifier.** If `narrowedImports` does not move, the gate is not the blocker and
   the alias road fails somewhere later.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §843 result

| leg | registered | measured |
|---|---|---|
| 1 primary | `narrowedImports` ≥ +8, case passes | **+10 of 10**, case passes (`10 WRONG -> 0`) |
| 2 safety | `RIGHT->WRONG` ≤ 10 | **0** |
| 3 falsifier | case must move | moved |
| 4 regression | tests + clippy | 1,834 passed, clippy clean |

**+10, zero adverse, +1 case**, for widening one `intersects` by one flag.

The registered exposure — imported classes, functions, enums and namespaces now
entering the flow walk — cost **nothing**, which is the answer upstream's own shape
predicted: those aliases enter upstream's walk too.

> **Reading the comment rather than the code would have preserved this bug.**
> Upstream's comment at that branch says *"We only narrow variables and parameters
> occurring in a non-assignment position. For all other entities we simply return the
> declared type"* — two outcomes. The code below it has three, and the alias arm is
> the line immediately after the comment. A port guided by the prose gets exactly
> this port's one-flag gate.

Pinned in `crates/tsr-checker/tests/type_query_narrowing.rs`
(`an_alias_narrows`): the corpus witness needs a `ModuleHost` the minimal harness
lacks, and `import a = M.x` reaches the same gate without one.

## §844: `references_match` strips parentheses but not `!`

`nonNullReferenceMatching` is 183 RIGHT / 18 WRONG, and the case name is the
mechanism. Every line is a dotted reference written with non-null assertions and
parentheses on one side of a guard and differently on the other:

```ts
typeof this.props.thumbYProps!.elementRef === 'function' && this.props.thumbYProps!.elementRef(ref);
typeof (this.props.thumbYProps!.elementRef) === 'function' && this.props.thumbYProps!.elementRef(ref);
typeof ((this.props).thumbYProps!.elementRef)! === 'function' && this.props.thumbYProps!.elementRef(ref);
```

`isMatchingReference` (`vendor/typescript-go/internal/checker/flow.go:1597-1620`)
strips these on **both** sides:

```go
switch target.Kind {
case ast.KindParenthesizedExpression, ast.KindNonNullExpression:
    return c.isMatchingReference(source, target.Expression())
...
switch source.Kind {
...
case ast.KindNonNullExpression, ast.KindParenthesizedExpression, ast.KindSatisfiesExpression:
    return c.isMatchingReference(source.Expression(), target)
```

This port's `references_match` (`crates/tsr-checker/src/flow.rs`) has both stripping
blocks already — and both list `ParenthesizedExpression` alone. `NonNullExpression`
is missing from both, and `SatisfiesExpression` from the source side. The blocks were
written from `flow.go`'s "both switches" (their own comment says so) and picked up one
of the two or three kinds each switch names.

### The bar

1. **Primary.** `nonNullReferenceMatching` ≥ **+12 of 18**. Falsified below +6.
2. **Safety.** `RIGHT->WRONG ≤ 10`. The exposure is narrowing that now applies
   through a `!` where it previously did not, anywhere in the corpus.
3. **Falsifier.** If the case does not move, the matching blocks are not what
   declines these and the reading is wrong.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §844 result — the primary leg FAILED as counted, and the change is kept

| leg | registered | measured |
|---|---|---|
| 1 primary | `nonNullReferenceMatching` ≥ +12 of 18 | **0** — the case did not move |
| 2 safety | `RIGHT->WRONG` ≤ 5 (of ≤10) | **0** |
| 3 falsifier | case must move | **did not move** |
| 4 regression | tests + clippy | 1,834 passed, clippy clean |

**The bar fired, and the change is kept anyway.** This is an override, stated loudly
with its evidence, not a quiet pass:

- It is a **verbatim** port of two upstream switch arms, both quoted at the call
  site. Nothing about it is a guess.
- It measures **+18 corpus-wide with zero adverse** — `narrowingUnionWithBang` 14
  `WRONG->RIGHT` and 4 `GAP->RIGHT`.
- **The named case's answers changed even though its count did not.** Before §844
  its wrong lines read `ElementRef | undefined` — no narrowing at all. After, they
  read **`ElementRef & Function`**. The reference now matches, the `typeof ===
  'function'` guard now applies, and a *different* defect one step downstream
  produces the wrong answer. A wrong-to-wrong transition is invisible to
  `scorepair`, which is why the leg as registered could not see the mechanism land.

> **Registering a leg as a case's WRONG count cannot distinguish "did not fire" from
> "fired and something downstream is also broken".** The leg should have been
> registered against the *answer text*, not the count. That is a defect in how I
> wrote the bar, not in the result, and it is the third time this session that a
> leg's phrasing rather than its threshold was the problem (§839.3's `&&`
> short-circuit, §839's ≤20 landing exactly at 20).

### The defect §844 exposed, located but not built: `t & Function`

Upstream's `narrowTypeByTypeFacts` (`flow.go`) returns the source **unchanged** when
it is a strict subtype of the implied type:

```go
case c.isTypeRelatedTo(t, impliedType, c.strictSubtypeRelation):
    if c.hasTypeFacts(t, facts) { return t }
    return c.neverType
case c.isTypeSubtypeOf(impliedType, t):
    return impliedType
case c.hasTypeFacts(t, facts):
    return c.getIntersectionType([]*Type{t, impliedType})
```

This port's `narrow_type_by_type_facts` (`crates/tsr-checker/src/flow.rs`) is a
faithful transliteration of exactly that, arm for arm. So the divergence is **inside
the relater**: `is_type_related_to(ElementRef, globalFunctionType, StrictSubtype)`
answers false, the first arm is skipped, and the third intersects.

Why it should answer true: `getPropertyOfTypeEx` gives a type with call signatures
the members of `globalFunctionType`, and any object type the members of
`globalObjectType`, so a function type structurally satisfies `Function`. **This port
already has that fallback** — `crates/tsr-checker/src/members.rs` pushes
`CallableFunction`/`NewableFunction`/`Function`/`Object` onto its lookup chain — so
the lookup is not the missing piece and the next read is `properties_related_to`:
whether its per-name lookup reaches that chain, and whether `StrictSubtype` adds a
requirement that `Function`'s members fail.

Size, measured: **28** WRONG lines corpus-wide already answer `… & Function`
(`narrowingByTypeofInSwitch` 13, `nonNullReferenceMatching` 6,
`typeGuardOfFormTypeOfFunction` 5, three cases with 1–2), **plus the 12 §844 has just
converted into that shape**, plus the `void`/`void | false` lines that depend on them
— `nonNullReferenceMatching` alone carries 6 such. Call it 50–60 lines at a
relater-depth effort.

**The next probe is one assertion**, not a build: assert
`is_type_related_to(<a function type>, <global Function>, StrictSubtype)` directly in
a test with `lib.d.ts` available, and read which of `properties_related_to`'s
branches answers.

## §845: diagnosed, not built — the `& Function` rows are the `CallableFunction` fallback

§844's exposed defect, taken to its cause with two probe runs and no code.

`narrow_type_by_type_facts` takes upstream's third arm and intersects, because its
first arm's test fails. Instrumenting all three branches over
`typeGuardOfFormTypeOfFunction`:

```
845 t=() => string  implied=Function -> (() => string) & Function | strict=false subtype=false assignable=false
845 t={ s: string; } implied=Function -> never                     | strict=false subtype=false assignable=false
845 t=unknown        implied=Function -> Function                  | strict=false subtype=false assignable=false
```

A function type is unrelated to `Function` at **every** relation, not merely
`StrictSubtype`. That rules out the relation's strictness and points at the
comparison itself.

The second probe asked what the source's `Function`-members resolve to:

```
src.bind = { <T>(this: T, thisArg: ThisParameterType<T>): OmitThisParameter<T>;
             <T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R,
                                                      thisArg: T, ...args: A): (...args: B) => R; }
src.name = string
```

**They resolve.** The lookup chain is not the gap — `src.name` is `string` and
matches `Function.name` trivially. The gap is `bind`: the port answers
**`CallableFunction`'s** two generic `this`-parameter overloads, and the target
`Function` declares `bind(this: Function, thisArg: any, ...argArray: any[]): any`.
Relating those two structurally is a hard question, and the port's relater answers
`false`. Upstream never asks it, because `getPropertyOfTypeEx` falls back to
**`globalFunctionType` only** — so upstream compares `Function.bind` with
`Function.bind`, which is identity.

### The cause, stated

`crates/tsr-checker/src/members.rs` builds its fallback chain as

```
CallableFunction | NewableFunction,  Function,  Object
```

and upstream's is `Function, Object`. The `CallableFunction`/`NewableFunction` entry
is a port-specific prepend. Upstream reaches those two types by a **different road**:
`bind`/`call`/`apply` under `strictBindCallApply` are resolved through
`getBindCallApplySignature`, not through the property fallback chain.

### Why this is filed rather than built

Removing the prepend is one line and would very likely fix the 50–60 `& Function`
lines — **and it is the road `strictBindCallApply1` (204 WRONG lines, one of the
corpus's largest single cases) depends on.** The two are the same line of code
pulling in opposite directions, so this is not a one-line change with a small blast
radius; it is "port `getBindCallApplySignature` and then remove the prepend", in that
order.

**Sizing both sides before touching it is the whole of the next session's first
step**, and it is now cheap, because the diagnosis above is the part that cost
anything:

| | |
|---|---:|
| `… & Function` WRONG lines | 28 measured, plus ~12 §844 converted, plus dependent `void` lines |
| `strictBindCallApply1` | 204 WRONG lines, unknown how many depend on the prepend |
| falsifier for the whole theory | delete the prepend, run `scorepair`, read both numbers |

That falsifier is one build and one run, and it answers the trade directly rather
than by argument. I did not run it because it belongs to a bar of its own, and
because a change that moves two large populations in opposite directions should not
be measured at the end of a session and landed on the strength of one number.

## §846: §845's cause was WRONG — the prepend is faithful, the missing thing is the FLAG

§845 concluded that `members.rs`'s `CallableFunction`/`NewableFunction` prepend was
"port-specific" and that upstream "falls back to `globalFunctionType` only", reached
`CallableFunction` by a different road, and that removing the prepend was the fix but
would collide with `strictBindCallApply1`. **Reading `getPropertyOfTypeEx` in full
refutes all of that**
(`vendor/typescript-go/internal/checker/checker.go`):

```go
var functionType *Type
switch {
case t == c.anyFunctionType:
    functionType = c.globalFunctionType
case len(resolved.CallSignatures()) != 0:
    functionType = c.globalCallableFunctionType
case len(resolved.ConstructSignatures()) != 0:
    functionType = c.globalNewableFunctionType
}
```

Upstream uses the callable/newable types in exactly the place this port does, keyed
on exactly the same thing. **The prepend is a faithful port.** §845's "cause" was an
inference from a two-line grep of `getPropertyOfType`, which forwards to `…Ex`; I did
not read the function I was drawing the conclusion from, and wrote a whole trade-off
analysis and a "do not build this" recommendation on top of it.

The actual difference is one function further:

```go
func (c *Checker) getGlobalStrictFunctionType(name string) *Type {
	if c.strictBindCallApply {
		return c.getGlobalType(name, 0 /*arity*/, true /*reportErrors*/)
	}
	return c.globalFunctionType
}
```

**`CallableFunction` and `NewableFunction` are used only when `strictBindCallApply`
is on.** Otherwise both *are* `globalFunctionType`. So for the ordinary case —
`typeGuardOfFormTypeOfFunction` and everything else that does not set the flag —
upstream resolves `f.bind` to `Function.bind`, compares it against the target
`Function`'s own `bind`, and the relation is **identity**. This port resolves it to
`CallableFunction`'s two generic `this`-parameter overloads unconditionally, asks the
relater a structurally hard question, gets `false`, and falls to the intersect arm —
producing `ElementRef & Function`.

The flag exists in `tsr-core` (`crates/tsr-core/src/options.rs`,
`strict_bind_call_apply`) and **the checker never reads it**.

> This also dissolves the collision §845 invented. The fix is not "remove the
> prepend and lose `strictBindCallApply1`"; it is "apply the prepend **when the flag
> says to**", which is what `strictBindCallApply1` sets and what the `& Function`
> cases do not. The two populations were never in opposition — that was an artefact
> of a cause I had not verified.

### The bar

1. **Primary.** The `… & Function` rows: ≥ **+40**. Falsified below +20.
2. **Safety.** `RIGHT->WRONG ≤ 10`, **and `strictBindCallApply1` must not regress at
   all** — it is the case that sets the flag, so it should be untouched. Any loss
   there falsifies the flag reading rather than costing a line.
3. **Falsifier.** If the `& Function` rows do not fall, the cause is wrong for the
   second time and I stop reading and start instrumenting.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §846 result — a faithful fix, a falsified primary leg, and a cause wrong TWICE

| leg | registered | measured |
|---|---|---|
| 1 primary | `… & Function` rows ≥ +40, falsified below +20 | **0** — **falsified** |
| 2 safety | ≤10, `strictBindCallApply1` untouched | **0 adverse**, that case untouched |
| 3 falsifier | rows must fall | did not fall |
| 4 regression | tests + clippy | 1,834 passed, clippy clean |

**Kept: +9, zero adverse** (`returnTypeParameterWithModules` 3,
`fatarrowfunctionsInFunctionParameterDefaults` 3, `genericTypeParameterEquivalence2`
3). Gating the prepend on `strictBindCallApply` is a verbatim port of
`getGlobalStrictFunctionType` and is right on its own terms — it just is not the
cause of the rows it was built for.

The bar said *"if the rows do not fall, the cause is wrong for the second time and I
stop reading and start instrumenting"*, and that is what settled it:

```
846 t=() => string flag=true
      src.bind={ <T>(this: T, thisArg: ThisParameterType<T>): OmitThisParameter<T>;
                 <T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R,
                                                          thisArg: T, ...args: A): (...args: B) => R; }
   target.bind=(this: Function, thisArg: any, ...argArray: any[]) => any
```

**`flag=true`.** `typeGuardOfFormTypeOfFunction` sets `strict`, so upstream takes the
`CallableFunction` branch there too — and still relates. The flag is ruled out **by
measurement**, and the cause is now definitively the **relater's signature
comparison**: a generic two-overload source against a target signature whose
parameters and return are `any`. Upstream's relation succeeds trivially there; this
port's answers `false`.

### Two wrong causes in a row, and they are the same mistake

- **§845** read `getPropertyOfType` — a two-line function that forwards to
  `getPropertyOfTypeEx` — and concluded from the forwarding that upstream "falls back
  to `globalFunctionType` only". `…Ex` says the opposite: it uses
  `globalCallableFunctionType` in exactly the place this port does. I then built a
  trade-off analysis, a size table, and a *"do not build this"* recommendation on top
  of a function I had not opened.
- **§846** read `getGlobalStrictFunctionType` correctly, and then **assumed the
  failing cases had the flag off** without checking. One probe — the one that
  eventually ran — prints `flag=true` and would have killed the bar before it was
  written.

Both are one failure: **acting on an unverified link in the chain.** This session had
already paid for it twice (§839.1's "the guard must not be reused", §839.4's
"upstream never keys by initial type"), which makes four. The cost each time was a
build and a full `scorepair`; the check each time was a single `sed` or one probe
line.

> **The rule this earns: before a bar is registered, every link it depends on must
> have been *read or measured in this session*, not inferred.** §845 and §846 both
> stated their link as fact in the bar's own text, which is exactly where an
> unverified claim is least visible.

The `& Function` row remains open, now correctly located, at an honest size: **~50–60
lines, in the relater's signature comparison**, which is `STATUS.md` §4.2's territory.

## §847: what 95% actually costs, measured

The session goal was 95% on `checker_types`' line gradient. This is what that is,
computed from the accepted baseline at 439,105 / 478,855 = **91.70%**.

```
need for 95%                   +15,807 lines
non-right in the compared set   35,132 lines across 3,215 cases

  top   10 cases =  1,971 lines   12% of what 95% needs
  top   25 cases =  3,950 lines   25%
  top   50 cases =  6,397 lines   40%
  top  100 cases =  9,845 lines   62%
  top  200 cases = 14,314 lines   91%
  top  400 cases = 19,504 lines  123%
  ALL 3,215 cases = 35,132 lines  222%
```

**95% requires completely fixing the ~230 worst cases** — every wrong and every gap
line in each — which is **45% of all remaining non-right lines in the corpus**.

The shape of that is the important part. There is no long-tail shortcut: fixing the
top 100 cases *perfectly* reaches 62% of the target. And the top of the list is not a
list of items, it is a list of subsystems:

```
variadicTuples1 265, temporal 223, complexRecursiveCollections 221,
strictBindCallApply1 204, genericFunctionInference1 202, genericRestParameters1 161,
conditionalTypes1 148, thisTypeInFunctions 141, inferFromGenericFunctionReturnTypes2 139,
contextualTypeWithUnionTypeMembers 136, genericDefaults 135, promisePermutations 134
```

Variadic tuples, generic inference, conditional and mapped types, contextual typing,
overload selection, `this` types, type-argument instantiation. `STATUS.md` §4 already
prices each at effort 5, and each is a multi-session port, not a slice.

### What this session is evidence for

Ten entries, **+487 lines and +39 cases**, at 91.46% → 91.70%. Every one came from
the same method — read one column further than the population's shape suggests — and
the largest, §841 at +310, came from a residue a previous bar had explicitly declined
to claim. That rate is real and it is worth continuing.

It is also **0.24 percentage points**. At this session's rate, 95% is on the order of
**fourteen more sessions of the same quality**, and only if the mechanism supply
holds — which the numbers above say it will not, because the remaining mass is
concentrated in exactly the places where a session's worth of reading produces one
subsystem rather than eight mechanisms.

**The honest plan for 95% is to port the subsystems in §4, in priced order, and to
stop treating it as a gradient target reachable by accumulating small faithful
fixes.** Recorded here so the next session does not re-derive it; the arithmetic
above is one script and should be re-run rather than trusted after any large landing.

## §848: row 6's refusal was broader than upstream's shape

`structured_type_related_to` refuses a pair when **either** side bears signatures:

```rust
if self.signature_bearing(source) || self.signature_bearing(target) {
    reasons::note(reasons::Site::SignatureBearing);
    return Ternary::Unknown;
}
```

`signaturesRelatedTo` (`vendor/typescript-go/internal/checker/relater.go:4441`)
starts at `TernaryTrue` and every one of its three branches iterates the **target's**
signature list. A target with no signatures is therefore vacuously related on the
signature axis, and the pair is decided by `propertiesRelatedTo` alone. So a
signature-bearing *source* against a plain object target is decidable, and refusing
it was this port's over-reach rather than row 6's.

Narrowed to `signature_bearing(target)`. A signature-bearing target still refuses —
that is the real `signatureRelatedTo` this port does not have.

**+4, zero adverse** (`narrowingMutualSubtypes`).

### This is the third falsified primary leg in a row, and that is the finding

| | registered | measured | adverse | kept |
|---|---|---:|---:|---|
| §844 | `nonNullReferenceMatching` ≥ +12 | 0 | 0 | yes, +18 elsewhere |
| §846 | `… & Function` ≥ +40 | 0 | 0 | yes, +9 elsewhere |
| §848 | ≥ +20 | +4 | 0 | yes |

Three bars, three failed primary legs, three correct changes, **zero adverse lines
between them**. Earlier in this session the same method produced §839 (+106), §841
(+310) and §843 (+10 of 10) with primary legs that hit or beat their predictions.

The difference is not that the changes got worse. It is that **the populations these
bars were sized against are blocked behind a subsystem, so fixing one link in the
chain moves nothing visible.** The `& Function` rows need `signatureRelatedTo`; until
that exists, every correct fix upstream of it pays only where some *other* path
already reached the answer — which is why all three landed small and clean rather
than large or negative.

That is §847's conclusion arriving from the other direction, and it is stronger
evidence than the arithmetic was: **my sizing predictions stopped working exactly
when the remaining work stopped being mechanism-shaped.** A bar that keeps failing
its primary leg while its changes keep being right is a measurement telling you the
items are no longer where you are looking.

### What actually unblocks the `& Function` rows

`signatureRelatedTo` — comparing a generic overload set against
`(this: Function, thisArg: any, ...argArray: any[]) => any`. It is the same machinery
overload selection needs (`STATUS.md` §4.2, effort 5), and it is the *only* thing
between this port and ~50–60 lines here plus the much larger overload populations.
Three entries in a row now point at it from different directions.

## §849: `signatureRelatedTo` scoped by reading, not estimated

§844, §846 and §848 all ended pointing at the same missing function. This is what it
actually costs, measured rather than guessed, so the next session starts from a
work-breakdown instead of rediscovering one.

### The two candidate shortcuts, both checked and both dead

1. **`isTopSignature` early return.** `compareSignaturesRelated`'s second line
   returns `TernaryTrue` when the target is a top signature
   (`vendor/typescript-go/internal/checker/relater.go:1675`): no type parameters, no
   `this` parameter or an `any` one, **exactly one parameter** which is a rest, whose
   element type is `any`/`never`, and an `any`/`unknown` return.
   `Function.bind` is `(this: Function, thisArg: any, ...argArray: any[]) => any` —
   it fails on the `this: Function` *and* on having two parameters. **Not the route.**

2. **"Every target parameter and the return is `any`, so short-circuit."** The
   soundness link checks out — `isSimpleTypeRelatedTo` (`relater.go:209`) returns
   true for an `any` target **unconditionally, in every relation including
   `strictSubtypeRelation`**, and the arity test passes whenever the target has a
   rest parameter. But the `this`-type comparison runs *before* the parameter loop
   and is not covered by it: `Function.bind` is declared as a **method signature**,
   which makes `strictVariance` false, so the comparison
   `compareTypes(sourceThisType, targetThisType)` really runs with source `this: T`
   against target `this: Function`. A shortcut that skipped it would be an
   approximation of the algorithm rather than a port of it — and this is the one
   consumer that acts on a `false` (`docs/conventions.md`), so an approximation here
   buys wrong overload resolutions, not gaps.

### The measured scope

`compareSignaturesRelated` is ~150 lines. Every helper it needs is **absent from this
port** — checked one by one, not estimated:

```
get_erased_signature                missing      get_non_array_rest_type            missing
instantiate_signature_in_context_of missing      try_get_type_at_position           missing
get_canonical_signature             missing      get_rest_or_any_type_at_position   missing
get_parameter_count                 missing      get_this_type_of_signature         missing
get_min_argument_count              missing      is_top_signature                   missing
has_effective_rest_parameter        missing      get_single_call_signature          missing
is_instantiated_generic_parameter   missing
```

Thirteen of thirteen. This is a subsystem port, not a slice, and it cannot land
inside one session honestly — which is what `STATUS.md` §4.2's **effort 5** already
said and what three converging entries have now confirmed from the code.

### Suggested order for whoever takes it

1. The arity family first — `get_parameter_count`, `get_min_argument_count`,
   `has_effective_rest_parameter`, `try_get_type_at_position`. These are pure
   functions over the existing `Signature`, testable without the relater, and they
   are what the `sourceHasMoreParameters` early `false` needs.
2. `compare_signatures_related` **returning `Unknown` for every shape it cannot yet
   decide** — generics, rest types, callback parameters. Wired in behind §848's
   `signature_bearing(target)` gate so it can only ever *narrow* a refusal and never
   replace a protective `Unknown` with a guess. Measure at this point: the
   non-generic single-signature pairs alone may pay.
3. Only then the generic half — `get_erased_signature`, `get_canonical_signature`,
   `instantiate_signature_in_context_of` — which is what the `& Function` rows
   specifically need.

Step 2 is the first point at which a `scorepair` number exists, and it is the right
place to re-price the whole item.

## §850: §849 steps 1 and 2 — the arity family and a bounded `compareSignaturesRelated`

Built to §849's plan. Step 1 is the arity family, pure functions over the existing
`Signature`; step 2 is `compare_signatures_related` wired behind §848's
`signature_bearing(target)` gate so it can only ever **narrow** a refusal.

### Step 1, in `crate::signatures`

`signature_has_rest_parameter`, `get_parameter_count`, `has_effective_rest_parameter`,
`try_get_type_at_position`, `get_min_argument_count` — ported from
`vendor/typescript-go/internal/checker/relater.go:1689-1781` and `checker.go:17038`.

**Every one returns `Option`, and `None` means *undecidable*, not zero.** Upstream's
tuple arms read `restType.TargetTupleType().fixedLength` and
`combinedFlags & ElementFlagsVariable`; this port models a tuple as a plain entry in
`tuple_element_lists` with no per-element flags, so the required/optional/variadic
distinction those encode is simply absent. Answering anyway would invent an arity,
and arity decides a `NotRelated` — the direction that promotes the next overload
candidate (`docs/conventions.md`).

`get_min_argument_count` rebuilds upstream's cached `signature.minArgumentCount` as
the number of leading parameters that are neither optional nor rest, then ports the
trailing-`void` trim on top.

### Step 2, in `crate::relater`

`signatures_related_to` follows upstream in iterating the **target's** list: empty
means vacuously related (§848), and anything other than one-against-one answers
`Unknown` — the `N * M` matrix and the paired-instantiation fast path are not ported.
`signatures_of_type` returning `None` also answers `Unknown`, because a type whose
signatures are written on an interface member is one `signature_bearing` can see and
this cannot; inventing an empty list for it would turn "unknown" into "vacuously
related".

`compare_signatures_related` refuses, explicitly, every shape needing absent
machinery:

| shape | upstream needs | here |
|---|---|---|
| either side generic | `instantiateSignatureInContextOf`, `getCanonicalSignature` | `Unknown` |
| a `this` parameter | the comparison at `relater.go:1500` | `Unknown` |
| a rest parameter | `getRestOrAnyTypeAtPosition`, tuple element flags | `Unknown` |
| a single-call-signature parameter | the callback path at `relater.go:1567` | `Unknown` |
| a type predicate | `compareTypePredicateRelatedTo` | `Unknown` |

What remains is upstream's own single-against-single branch with the hard parts
excluded: the arity early `false`, the parameter loop, and the return comparison
including `relater.go:1592`'s *`void` or `any` target return accepts anything*.

Two details taken from the code rather than assumed, both of which decide answers:

- **Parameters are bivariant source-first.** `if !strictVariance { related =
  compareTypes(source, target) }; if related == False { related =
  compareTypes(target, source) }`. Not contravariant-only, and not target-first.
- **`strictVariance` needs `strictFunctionTypes`**, which the checker did not read.
  Plumbed alongside §846's `strict_bind_call_apply`. It is
  `strictFunctionTypes && the target declaration is not a method, method signature or
  constructor` — getting it wrong in the lenient direction accepts pairs upstream
  rejects, which is a wrong overload resolution rather than a gap.

### The bar

1. **Primary.** Net ≥ **+15**. This is a capability, not a targeted fix, so the
   prediction is deliberately modest — §848 showed these populations sit behind the
   generic half, which is step 3 and is *not* in this change.
2. **Safety.** `RIGHT->WRONG ≤ 15`. This is the leg that decides it. The change
   converts `Unknown` into a verdict, and `Unknown` is what currently protects the
   overload road from confident wrong answers.
3. **Falsifier.** Any loss in `strictBindCallApply1`, `promisePermutations*` or the
   overload cases means the variance or arity reading is wrong, and it reverts —
   those are the cases that act on a `false`.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §850 result — BUILT, MEASURED, REVERTED, and §849's step order was wrong

The falsifier fired exactly where it was registered.

| leg | registered | measured |
|---|---|---|
| 1 primary | net ≥ +15 | **+12** |
| 2 safety | `RIGHT->WRONG` ≤ 15 | **10** — passes |
| 3 falsifier | **any** loss in `promisePermutations*` / the overload cases | `promisePermutations2` **−8**, `promiseVoidErrorCallback` −2 |
| — | (not registered, and the largest effect) | **`RIGHT->GAP` 83**, `promiseTypeStrictNull` 80 |

`WRONG->RIGHT 88` (`typeGuardOfFormIsTypeOnInterfaces` 37, `unionTypeReduction2` 22,
`arrayOfFunctionTypes3` 6) against **93 lines lost in the promise/overload family**,
for a net of +12 right lines. **Reverted.**

### Why it went that way, which is the finding

§849 proposed step 2 as *"`compare_signatures_related` returning `Unknown` for every
shape it cannot yet decide, wired behind §848's gate so it can only ever narrow a
refusal and never replace a protective `Unknown` with a guess"*, and predicted that
the non-generic single-signature pairs alone might pay.

The first half held: the implementation refuses generics, `this` parameters, rest
parameters, callback parameters and type predicates, and every verdict it produced
was upstream's. **The second half was wrong, and the reason is structural rather
than a bug.** Overload selection consumes the relation, and it ranks candidates by
*comparing them against each other*. Deciding **some** signature pairs while leaving
others `Unknown` does not leave the ranking untouched — it reorders it, because a
candidate that now answers `NotRelated` is eliminated while its neighbour, refused
for an unported shape, survives. `promiseTypeStrictNull`'s 80 `RIGHT->GAP` lines are
that: a `then` overload set where the newly-decidable candidates lose to the still-
undecidable ones.

So a *partial* signature relation is not conservative in the way "only narrows a
refusal" suggests. It is conservative pointwise and destabilising in aggregate.
**§849 step 2 cannot be measured in isolation**, and the step order it recommended —
bounded relation first, generic half later — is the wrong decomposition. The right
one is to port `compareSignaturesRelated` **whole**, generics included, and measure
once.

That is a genuinely worse-shaped item than §849 priced, and it is worth knowing
before someone spends a session on the easy half.

### What was built, and what is worth keeping from it

Reverted in full; nothing of it is in the tree. What the attempt established, in
descending order of value to the next attempt:

1. **The step order above.** Port it whole or not at all.
2. **`call_signatures_of_type` answers `Some(vec![])`, not `None`**, for a bare
   `(x: number) => void` whose signatures live only in the baked `signature_types`
   table. Reading that empty list as "no call signatures" makes a signature relation
   answer *vacuously related* for every such pair — it accepted
   `(x: number, y: number) => void` as a `(x: number) => void`. The corpus measured
   **zero transitions** while that bug was live; a unit test caught it.
3. **A function type has no members table**, so `has_members` is false and such a
   pair never reaches `structured_type_related_to` at all — it falls through to
   *not computed*. Any future signature relation must widen that gate as well as
   fill the refusal, and the first version of §850 measured zero transitions purely
   because it did not.
4. **Parameters are bivariant source-first** (`if !strictVariance { compareTypes(source,
   target) }; if False { compareTypes(target, source) }`), and `strictVariance` needs
   `strictFunctionTypes`, which the checker does not read. Both are `relater.go:1499`
   and `:1571`.
5. **`tryGetTypeAtPosition` returning `nil` means SKIP the position, not fail**
   (`relater.go:1553`). That is the whole reason a one-parameter source satisfies a
   two-parameter target, and getting it wrong refuses every unequal-arity pair.
6. **A pre-existing confident-wrong, found on the way and unrelated to §850**:
   `(...xs: number[]) => void` is accepted as `(...xs: string[]) => void`. Verified
   on the commit *before* §850 by running the same assertion there, so it is not this
   change's doing. It is decided somewhere above the structural arm and is a wrong
   answer in the consumer that acts on a `false`. Unfiled elsewhere; **this is the
   only record of it.**

The corpus could not have caught 2, 3 or 5 — all three measured zero transitions or
were masked by another defect — and unit tests caught all three. That is the
strongest argument this session produced for testing a relation directly rather than
only through the gradient.

## §851: a survey of the six next-largest populations — none is cheap, and here is why each

After §850 reverted, I surveyed the remaining boards rather than picking the next row
by size. Recorded so the next session does not repeat the survey; each line is a
population **checked and priced**, not a guess.

| population | size | why it is not a mechanism-level item |
|---|---:|---|
| `tryCatchFinallyControlFlow` | 17 W | The port **already ports `bindTryStatement` whole**, "including the `ReduceLabel` trick" (`crates/tsr-binder/src/binder.rs`). The residue is a divergence inside a ported construct, not a missing one. Shape: we answer `0 \| 1` where upstream answers `1` — the `return` inside `try` should make the alternative unreachable. |
| `controlFlowAliasing` | 19 W | Aliased conditional narrowing is **already partly ported** — `is_constant_reference` exists and §82.1 handles `&&`/`\|\|` inside an inlined aliased condition. Residue is in the discriminant half. |
| `intersectionReduction` | 92 W | Two shapes at once (`1 -> error`, `never -> any`); intersection reduction is its own subsystem. |
| `privacyFunctionParameterDeclFile` | **0 W / 90 GAP** | The best *profile* on the board — closing it wins the case with no wrong lines to lose. But the shapes it needs **already work**: a probe shows `(param: M.C) => void` renders correctly, and a namespace-qualified class reference resolves. Multi-file, and the cause is not the qualified-name road it looks like. |
| `privacyLocalInternalReferenceImportWithExport` | 104 GAP / 72 W | Same family, but with 72 wrong lines it has no safe profile. |
| `correlatedUnions` | 143 GAP | §806's case; the residue is generic instantiation. |

The pattern across all six, and it is the same one §848 measured: **every population
left at this size is either already partly ported with a deep residue, or behind a
priced subsystem.** That is what §847's arithmetic predicted and what three
successive failed primary legs demonstrated from the other side.

> **The gap board is worth re-ranking after any large landing** — it is how §805 and
> §806 were found, and it is a different lens from the wrong-line board. It was
> re-run here and the top of it is `correlatedUnions` 143, generic instantiation
> again.

### The one genuinely new finding from the survey

`privacyFunctionParameterDeclFile` is **605 RIGHT / 0 WRONG / 90 GAP**. A case with
no wrong lines at all is the safest thing on any board: every line closed is a line
gained and there is nothing to lose. It is worth checking whether other such cases
exist, because the boards this project keeps rank by *wrong* lines and by *gap*
lines, and neither surfaces "cases with gaps and no wrongs" as a class. That query is
one awk line and has never been run.

## §852: the board nobody had built — 344 cases with GAPS and ZERO wrong lines

§851 ended by noting that this project's two boards rank by *wrong* lines and by
*gap* lines, and that neither surfaces **"cases with gaps and no wrongs"** as a
class. That query is one script. Run:

```
cases with GAPS and ZERO wrong lines: 344, worth 1,637 lines

   90 gaps     605 right   compiler/privacyFunctionParameterDeclFile
   39 gaps       4 right   conformance/mappedTypes1
   34 gaps      55 right   compiler/renamingDestructuredPropertyInFunctionType2
   28 gaps      99 right   conformance/inferingFromAny
   26 gaps       5 right   compiler/readonlyFloat32ArrayAssignableWithFloat32Array
   22 gaps      12 right   conformance/emitter.asyncGenerators.functionExpressions.es2018
   22 gaps      12 right   conformance/emitter.asyncGenerators.functionExpressions.es2015
   20 gaps      77 right   conformance/genericCallWithOverloadedFunctionTypedArguments
   20 gaps      49 right   compiler/typedArrays-es6
   20 gaps      34 right   compiler/noCollisionThisExpressionAndLocalVarInAccessors
   19 gaps     143 right   conformance/assignmentCompatWithObjectMembers
   18 gaps     307 right   compiler/sourceMapValidationDestructuringForObjectBindingPatternDefaultValues
   18 gaps      77 right   conformance/optionalChainingInference
   18 gaps      75 right   conformance/stringLiteralTypesOverloads02
   17 gaps      18 right   compiler/nestedRecursiveLambda
```

### Why this is the best-shaped population left

**Every one of these cases passes the moment its gaps close, and not one of them has
a wrong line to lose.** That is a profile no other board on this page can offer:

- A gap is an honest "not computed", so closing one can only move `GAP->RIGHT` or
  `GAP->WRONG`, and §620 already established `GAP->WRONG` as this project's accepted
  adverse direction. There is **no `RIGHT->WRONG` exposure at all** in a case with
  zero wrong lines.
- The case yield is extreme relative to the line count: **1,637 lines for up to 344
  cases**, against the corpus's overall ratio of roughly 70 lines per case. These are
  cases sitting one small fix away from passing.
- For comparison, §847's arithmetic says 95% needs the ~230 *worst* cases fixed
  completely. This board is 344 cases that are almost entirely correct already.

It does not reach 95% — 1,637 lines is **0.34 points**, taking the gradient to about
92.0% — but it would take the **case** rate from 64.8% to as much as 68.4%, which is
the larger movement and the one `STATUS.md` §1's headline number tracks.

### How to work it, and the one caution

Rank by `gaps` ascending within the board, not descending: a case with 2 gaps and 300
right lines is a single missing arm, and there are many more of those than there are
90-gap cases. The list above is the head; the tail is where the ratio is best.

**The caution, measured:** `privacyFunctionParameterDeclFile` heads the board at 90
gaps and is *not* the place to start. Probes show the two shapes it appears to need —
a namespace-qualified class reference in a signature, and a `namespace`-declared
class — **already work**, so its cause is something else and it is multi-file. Size
at the top of this board is not a proxy for tractability; the same trap §851 records
for the wrong-line board applies here.

This board has never existed before, which is why a 344-case population was invisible
to every session that came before this one.

### §852.1: the board's tail, and what the `any` family is NOT

Ranked ascending, as §852 says to:

```
cases with 1-2 gaps and ZERO wrong lines: 173  (= 173 cases for 260 lines)
```

**173 cases sit one or two gap lines from passing.** At roughly 1.5 lines per case
that is the best ratio anywhere in this project; the corpus average is near 70 lines
per case.

Within the zero-wrong board, `any` is the single most common wanted answer: **221 gap
lines across 67 cases, and in 42 of those cases *every* gap wants `any`** — so 42
cases would fall to whatever explains them.

**They are not one mechanism.** Three of the 1-gap cases, read:

| case | the node | what it is |
|---|---|---|
| `conformance/autoAccessor10` | `#a2_accessor_storage in C3` | a **private identifier** as the left operand of `in` |
| `compiler/typeAliasExport` | `export type a = typeof a;` | a type alias whose own name shadows the `var a` its `typeof` reads |
| `conformance/classPropertyIsPublicByDefault` | `C.b()` | a call whose callee is an untyped static |

Three different causes. And the third is *not* the obvious one: a probe confirms
`declare const o: any; o.b()` and `declare const f: any; f()` both already answer
`any`, so calling an `any` callee is ported and that case fails for some other
reason.

So the `any` family is **42 separate one-line diagnoses**, not one fix. That is still
an excellent ratio — roughly one case per gap line, with no `RIGHT->WRONG` exposure
anywhere in it — but it is a *grind with a known good yield*, not a mechanism hunt,
and it should be planned as such rather than opened expecting a single cause.

> **What makes this board worth inheriting**: it is the first population this project
> has found where the work is bounded, the risk is structurally zero, and the case
> yield is an order of magnitude better than the corpus average. It does not reach
> 95% — 1,637 lines is 0.34 points — but on the number `STATUS.md` §1 leads with, it
> is worth more than everything §839–§850 achieved put together.

## §853: a `null` or `undefined` member poisons an object literal — but only non-strict should

Working §852's board ascending found a cluster of three one-gap zero-wrong cases with
the same shape:

```
declarationEmitInferredDefaultExportType    >{ foo: [], bar: undefined, baz: null } : { foo: never[]; bar: undefined; baz: null; }
declarationEmitInferredDefaultExportType2   >{ foo: [], bar: undefined, baz: null } : { foo: never[]; bar: undefined; baz: null; }
objectLiteralIndexerNoImplicitAny           >{ p: null } : { p: null; }
```

Reproduced in the minimal harness in four probes: `{ a: 1 }` is
`{ a: number; }`, and `{ a: 1, b: undefined }` and `{ p: null }` are both **`error`**.
One nullable member poisons the whole literal.

`crates/tsr-checker/src/objects.rs` says so outright:

```rust
// The declaration-level `getWidenedType` would turn this into `any`
// and this port has no call site for it. See the module docs.
if self.store.get(member_type).flags.intersects(TypeFlags::NULLABLE) {
    return error;
}
```

That reasoning is **correct for non-strict and wrong under `strictNullChecks`**, and
the deciding function says so in four lines
(`vendor/typescript-go/internal/checker/checker.go:25027`):

```go
func (c *Checker) createWideningType(nonWideningType *Type) *Type {
	if c.strictNullChecks {
		return nonWideningType
	}
	t := c.newIntrinsicType(nonWideningType.flags, nonWideningType.AsIntrinsicType().intrinsicName)
	t.objectFlags |= ObjectFlagsContainsWideningType
	return t
}
```

`undefinedWideningType` and `nullWideningType` are what a `null`/`undefined`
*expression* answers, and under `strictNullChecks` they **are** the plain types, with
no `ContainsWideningType`. `getWidenedTypeWithContext` is gated on
`ObjectFlagsRequiresWidening`, so under strict there is nothing to widen and
`{ p: null }` is `{ p: null; }`. The port's refusal stands in for a widening step
that upstream does not take.

### The bar

1. **Primary.** The three cases above close their gaps: ≥ **+5 lines and +2 cases**.
   Falsified if none of the three moves.
2. **Safety.** `RIGHT->WRONG ≤ 10`. These literals answered `error` before, so the
   available direction is `GAP->RIGHT` or §620's accepted `GAP->WRONG`; a
   `RIGHT->WRONG` would mean something downstream preferred the gap.
3. **Falsifier.** If the cases do not move, the refusal is not what gaps them.
4. **Regression.** `cargo test --workspace`; clippy clean.

**Non-strict keeps the refusal**, unchanged and for its original reason: there the
port really would need `getWidenedType`, and answering `{ p: null; }` where upstream
answers `{ p: any; }` would be a confident wrong line in place of an honest gap.

### §853 result — +108 lines, +16 cases, ZERO `RIGHT->WRONG`, and a test that repaired itself

| leg | registered | measured |
|---|---|---|
| 1 primary | ≥ +5 lines, +2 cases | **+108 lines, +16 cases** |
| 2 safety | `RIGHT->WRONG` ≤ 10 | **0** |
| 3 falsifier | the three cases must move | moved |
| 4 regression | tests + clippy | 1,835 passed, clippy clean, 3,252 anchors resolve |

`WRONG->RIGHT 80` and `GAP->RIGHT 28` against 5 `GAP->WRONG` — §620's accepted
direction — and **not one right line lost**. The reach is far wider than the three
cases that surfaced it, because a nullable member is common:
`destructuringParameterDeclaration1ES5`/`ES5iterable`/`ES6` 7 each,
`excessPropertyCheckWithNestedArrayIntersection` 6, `arrayFilter` 4,
`classExpressionNames` 4.

**An existing test caught the change before it was pushed.**
`a_nullable_member_is_a_gap_because_the_declaration_would_need_widening` in
`crates/tsr-checker/tests/objects.rs` pinned the old refusal, went red, and its own
comment contained the answer:

> Upstream records **two different types** for one source line:
> `>c : { x: any; }` at the declaration, `>{x: null} : { x: null; }` at the literal.

That observation is **the non-strict one**. Under `strictNullChecks` there is no
second type: upstream records `{ x: null; }` on both lines. The test now asserts the
strict answer and carries the reason; the refusal is untouched under non-strict,
where the original reasoning holds exactly. §800's rule again — *a residue pinned as
an assertion reports its own repair; a residue described in prose does not.*

### What this says about §852's board

§853 is the **first** item worked off the zero-wrong-gap board, it was found by
ranking that board ascending as §852 said to, and it returned **+16 cases for one
gated condition**. The three cases that surfaced it contributed 3 of those 16; the
other 13 came from cases the board never named, because the mechanism is common and
the board only shows where it is *load-bearing*.

That is the argument for the board in one data point: **it finds mechanisms by
pointing at cases that are nearly right, and the mechanism then pays everywhere
else.** The same shape as §841 (+310 from a residue named in advance), reached by a
different instrument.

## §854: the next two clusters on §852's board — one already refused, one not a checker item

After §853, the board is **336 zero-wrong gap cases / 1,615 lines, 168 of them 1–2
gaps from passing.** Re-deriving it and grouping the 1–2-gap cases by their *failing
assertion text* surfaces the clusters directly:

```
  6  >a : any
  4  >b : { a: boolean; b: string; }
  3  >x : any
  3  >1 + {} : any
  2  >data[0]() : any     2  >"A" : typeof import("A")     2  >$ : { x: number; }
```

### `1 + {}` — three cases, and **§257 already refused it, twice, with numbers**

Probed first: `1 + {}` and `{} + 1` answer `error` where upstream answers `any`;
`'a' + {}` is `string` and `1 + 1` is `number`, both right. Upstream's line is
unambiguous (`checker.go:12455`), inside `checkBinaryLikeExpression`'s `+` arm:

```go
if resultType == nil {
    c.reportOperatorError(...)
    return c.anyType
}
```

**And `crates/tsr-checker/src/binary.rs` already carries the refusal, with its
measurements:**

| attempt | result |
|---|---|
| whole fallback → `any` | +4 cases, `GAP->RIGHT` 10, `WRONG->RIGHT` 12, **`GAP->WRONG` 57** |
| narrowed to non-literal operands | +0 cases, `WRONG->RIGHT` 12, **`GAP->WRONG` 32** |

The adverse population is **literal and enum-literal arithmetic**, where upstream
computes a real `number`/`string` and this port cannot; every line reaching the
fallback from there is a gap this port owns, and answering `any` replaces it with a
confident wrong answer. §257's stated reopening condition: *"revisited when literal
arithmetic lands — at which point the adverse population stops reaching here at all
— and not before."*

> **This is the record paying for itself.** The cluster looked like three free cases
> and one verbatim upstream line; the refusal note turned that into a two-minute read
> instead of a build and a full `scorepair`. `STATUS.md`'s rule — *a refused item
> deleted rather than recorded costs the next session a cycle rediscovering the same
> negative* — is exactly what was avoided, and this would have been the **fourth**
> attempt.

Recorded here as well as in `binary.rs` because the *board* will keep surfacing this
cluster: it is three zero-wrong cases and will look inviting to every future session
that ranks by tractability.

### `{ a: boolean; b: string; }` — four cases, and not a checker item

`requireOfJsonFileWithModuleEmitUndefined` and its two siblings, plus
`isolatedModules_resolveJsonModule_strict_outDir_commonJS`. They need
**`resolveJsonModule`** — importing a `.json` file and typing the module as the
literal shape of its contents. That is `crates/tsr-compiler`'s loader and resolver,
not the checker; nothing in the checker gaps here.

`requireOfJsonFileWithModuleEmitUndefined` is **0 RIGHT / 0 WRONG / 1 GAP** — the
whole case is one assertion — so the four are cheap *if* the loader feature lands,
and unreachable from this workstream until it does. Filed against
`crates/tsr-compiler`, not here.

### Where that leaves the board

Two of the three largest clusters on §852's board are now priced: one refused with a
named reopening condition, one owned by another crate. The `>a : any` cluster (6
cases) and the `>x : any` cluster (3) remain unread, and §852.1's finding stands —
they are likely to be separate one-line causes rather than a shared mechanism.

## §855: a destructured `catch` binding is still a catch variable

Two more of §852's one-gap zero-wrong cases, found by reading the board's failing
assertions with their source lines:

```
asyncWithVarShadowing_es6   >x : any   src: catch ({ x }) {
objectRestCatchES5          >a : any   src: try {} catch ({ a, ...b }) {}
```

Probed: `catch (e) { e }` answers `unknown` — correct under strict — and
`catch ({ x }) { x }` answers **`error`**, while destructuring a plain `any` works.
So the port tries to *destructure* the catch variable and fails.

Upstream does not destructure it at all. `getTypeOfVariableOrParameterOrPropertyWorker`
(`checker.go:16678`):

```go
if ast.IsCatchClauseVariableDeclarationOrBindingElement(declaration) {
    ...
    if c.useUnknownInCatchVariables { return c.unknownType }
    return c.anyType
}
```

and the predicate (`ast/utilities.go:721`) is the whole point:

```go
func IsCatchClauseVariableDeclarationOrBindingElement(declaration *Node) bool {
	node := GetRootDeclaration(declaration)
	return node.Kind == KindVariableDeclaration && node.Parent.Kind == KindCatchClause
}
```

**`GetRootDeclaration`** walks `BindingElement -> parent.parent` until it is not one,
so every binding element inside a catch pattern *is itself* a catch variable and
takes `unknown`/`any` directly. The pattern is never destructured.

This port has the arm (`crates/tsr-checker/src/symbols.rs`) and tests

```rust
self.nodes.parent(declaration).is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::CatchClause)
```

— the **direct** child of the catch clause only. For `catch ({ x })` the declaration
is a `BindingElement`, its parent is the pattern, and the test fails one hop early.

> **The same shape as §837**, where a one-hop heritage check missed a three-hop
> cycle, and the same fix: replace the hop with the walk upstream already names.
> `root_declaration_of` exists in `crate::expressions` and is exactly
> `GetRootDeclaration`; it becomes `pub(crate)` rather than being written twice.

### The bar

1. **Primary.** Both cases close: ≥ **+2 lines, +2 cases**.
2. **Safety.** `RIGHT->WRONG ≤ 5`. The arm now fires for binding elements it did not
   reach, so the exposure is any destructured catch whose members this port was
   previously typing correctly by some other route.
3. **Falsifier.** If neither case moves, the one-hop test is not what gaps them.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §855 result — FALSIFIED and reverted; the one-hop test was not the blocker

| leg | registered | measured |
|---|---|---|
| 1 primary | both cases close, ≥ +2 lines / +2 cases | **0** |
| 2 safety | `RIGHT->WRONG` ≤ 5 | 0 |
| 3 falsifier | if neither case moves, the one-hop test is not what gaps them | **neither moved** |

**Zero corpus transitions, and the probe did not move either**: `catch ({ x }) { x }`
still answers `error` with the root-declaration walk in place. So the reading was
right about upstream and wrong about this port — the one-hop parent test is *a*
divergence, but it is not what produces the gap. Something earlier on the
binding-element road errors before
`get_widened_type_for_variable_like_declaration` is ever consulted.

Reverted. Keeping a faithful-but-inert change would have failed §823/§824's rule:
**ship a mechanism only when it demonstrably fires**, and this one cannot be
witnessed by any test.

What the attempt does establish, for whoever opens it next:

1. **Upstream's rule is confirmed and is not what this port does.**
   `IsCatchClauseVariableDeclarationOrBindingElement` uses `GetRootDeclaration`, so a
   binding element in a catch pattern is *itself* a catch variable and takes
   `unknown`/`any` **without being destructured**. The port's arm tests the
   declaration's own parent. That divergence is real and should be fixed *as part of*
   whatever does fix these cases.
2. **The blocker is upstream of that arm.** The next probe is to find what types a
   `BindingElement` whose root is a catch clause — `get_type_of_symbol` on the
   binding-element symbol — and where it answers `error`, rather than assuming the
   widened-declaration road is reached at all.
3. `catch (e)` answers `unknown` correctly, and destructuring a plain `any` works, so
   neither the catch arm nor the destructuring road is broken in isolation. It is
   their composition.

Two of the last three bars have now been falsified by their own primary leg
(§850, §855) and both changes were faithful ports of real upstream divergences. That
is §848's finding continuing to hold: at this depth, *being right about upstream no
longer predicts moving the corpus*, because the populations sit behind other defects.

## §856: the gap-root board, re-read — where all 8,889 gap lines actually root

§855's dead end argued for the instrument over case-by-case probing, so `depend.rs`
was re-run. It follows declaration edges to the end of each gap's dependency chain
and reports where the chain stops. Current reading:

| root | lines | % | cases | top case |
|---|---:|---:|---:|---|
| `CallExpression` (root is here) | 1,245 | 14.0% | 278 | `typeParameterConstModifiersReturnsAndYields` |
| **`Parameter` — no further dependency** | **987** | **11.1%** | **240** | `esDecorators-contextualTypes` |
| `Identifier`, member name of `a.b` | 621 | 7.0% | 268 | `mixinAccessModifiers` |
| `PropertyAccessExpression` | 573 | 6.4% | 182 | `esNextWeakRefs_IterableWeakMap` |
| `Identifier`, symbol has no value declaration | 385 | 4.3% | 103 | `requireOfJsonFileTypes` |
| `TypeReference` | 367 | 4.1% | 85 | `privacyFunctionParameterDeclFile` |
| `BindingElement` cycle | 360 | 4.0% | 73 | `coAndContraVariantInferences3` |
| `MappedType` — **no step arm** | 302 | 3.4% | 61 | `mappedTypes1` |
| `ElementAccessExpression` | 293 | 3.3% | 67 | `genericRestParameters2` |
| `NewExpression` | 263 | 3.0% | 65 | `mixinClassesMembers` |

`Parameter — no further dependency` is the largest row whose root is *here* with
nothing downstream to fix first, so it is the one to read. Its top case's gaps are:

```
want: (this: This, ...args: Args) => Return
want: (t: typeof C, c: ClassDecoratorContext<typeof C>) => void
want: (t: ClassAccessorDecoratorTarget<C, number>, c: ClassAccessorDecoratorContext<C, number> & { … }) => void
```

Decorator context types — **contextual typing**, `STATUS.md` §4's effort 5. And the
row is a *thin* spread: 240 cases with a top-1 share of **4.3%**, so it is not one
mechanism with a dominant witness but a long tail sharing a node kind.

### What the board says as a whole

Three rows are marked **"NO STEP ARM for this kind — not a finding"** — `MappedType`
302, `YieldExpression` 107, `ConditionalType` 99, `TaggedTemplateExpression` 98,
`SpreadAssignment` 95 — **701 lines the instrument cannot attribute at all**. That is
`STATUS.md` §4.0's two deliberately-unmade `depend.rs` fixes still outstanding, and
adding those arms is the cheapest remaining *instrument* work: it would re-root 701
lines, which is 8% of the gap population, and might move some of them out of the
"unattributed" column into something actionable.

Everything the board *can* attribute roots in the priced subsystems: calls and
`new` (overload selection and instantiation), parameters (contextual typing),
property and element access (member resolution through generics), mapped and
conditional types. **No row on this board is a mechanism-level item**, which is the
same answer §847's arithmetic, §848's three failed legs and §851's survey each
reached by a different route — now confirmed a fourth time, against the whole 8,889-line
gap population rather than a sample.

### The one piece of cheap work this leaves

Adding `depend.rs` step arms for `MappedType`, `YieldExpression`, `ConditionalType`,
`TaggedTemplateExpression` and `SpreadAssignment` — §4.0's outstanding item, 701
lines re-rooted, no risk to the corpus because `depend.rs` is an instrument and not
the checker. It should get §827's env-gate treatment (`TSR_NO_856=1`) so the
re-rooting can be A/B'd against today's board rather than replacing it silently.

### §856.1: the step arms, built — 701 unattributed lines re-rooted

§4.0's outstanding `depend.rs` item, done. Five arms added on §767's shape — follow
the first constituent that gaps; none gapping means the arm itself refused and the
root really is there — gated by `TSR_NO_856=1` so the re-rooting A/Bs against the
previous board rather than replacing it silently (§827's treatment).

| kind | before | after |
|---|---:|---|
| `MappedType` | 302 NO STEP ARM | **281**, root is here (21 re-rooted) |
| `YieldExpression` | 107 NO STEP ARM | **gone** — all 107 re-rooted |
| `ConditionalType` | 99 NO STEP ARM | **96**, root is here |
| `TaggedTemplateExpression` | 98 NO STEP ARM | **92**, no further dependency |
| `SpreadAssignment` | 95 NO STEP ARM | **91**, no further dependency |

`scorepair` reports **no transitions** — `depend.rs` is an instrument, not the
checker, which is why this was the one piece of work left with no corpus risk at all.

### `has_step_arm` is a second copy of the same fact, and it lied first

The first run moved every count and changed no label: `TaggedTemplateExpression` went
98 → 92 while still printing *"NO STEP ARM for this kind — not a finding"*. The
ending is chosen by `has_step_arm`, a hand-maintained list of node kinds parallel to
`step`'s own arms, and adding an arm without adding it there leaves the row
**labelled as unattributable while the attribution is in fact running**.

That is worth more than the re-rooting. A board whose label and whose behaviour
disagree is worse than one that admits ignorance, and this one would have under-
reported its own coverage to every future session. The list now carries a comment
saying it must move with `step`.

### What surfaced underneath

Four kinds were always unattributed and always below the printed cutoff, and are now
the visible remainder: `FunctionType` 87, `SpreadElement` 81, `UnionType` 78,
`IndexedAccessType` 77 — **323 lines**, which is `STATUS.md` §4.0's *other* listed
item (`ArrayType`/`TupleType`/`UnionType`/`IntersectionType`). Same treatment, same
absence of risk, and now the largest instrument gap left.

> **The general point, which the board itself demonstrates twice over:** a "not a
> finding" bucket hides two different things — kinds nobody wrote an arm for, and
> kinds whose arm exists but whose label was never updated. Both look identical in
> the output, and only the A/B distinguishes them.

### §856.2: `depend.rs` now attributes EVERY gap line — zero `NO STEP ARM` rows

§856.1's four newly-visible kinds plus the three §4.0 also names, same shape and same
gate. Seven more arms: `FunctionType`, `IndexedAccessType`, `UnionType`,
`IntersectionType`, `ArrayType`, `TupleType`, `SpreadElement`.

**The board now prints zero `NO STEP ARM` rows.** All 8,889 gap lines are attributed
to a root, and `scorepair` reports no transitions throughout — the whole of §856 is
instrument work and touched no checker behaviour.

`STATUS.md` §4.0's `depend.rs` item is **closed**: both halves of it (the
`ArrayType`/`TupleType`/`UnionType`/`IntersectionType` arms, and the relabelling)
are done, plus five kinds §4.0 did not name.

### The board, fully attributed

```
CallExpression  (root here)                           1,253  14.1%  278 cases
Parameter  (no further dependency)                      994  11.2%  240 cases
Identifier, member name of a.b                          621   7.0%  268 cases
PropertyAccessExpression                                579   6.5%  182 cases
Identifier, symbol has no value declaration             385   4.3%  103 cases
TypeReference                                           376   4.2%   87 cases
BindingElement cycle                                    360   4.0%   73 cases
ElementAccessExpression                                 293   3.3%   67 cases
MappedType                                              281   3.2%   56 cases
Identifier, decl name: FunctionDeclaration              286   3.2%   48 cases
NewExpression                                           264   3.0%   65 cases
```

No row is a mechanism-level item; every one is a priced subsystem. But the board is
now **honest about its own coverage**, which it was not before §856.1 — and that was
the finding worth having, because a "not a finding" bucket was concealing both kinds
nobody had written an arm for *and* kinds whose arm ran while the label said
otherwise.

> **What is genuinely left that is cheap and safe**: nothing on the instrument side.
> §856 closed it. The remaining work is §852's 336-case board (bounded, zero
> `RIGHT->WRONG` risk, ~0.3 gradient points) and the priced subsystems.

## §857: a CORRECTION to §852.1 — the board's cases are not one-liners

§852.1 priced the zero-wrong board as *"42 separate one-line diagnoses, not one fix
… still an excellent ratio, roughly one case per gap line … a grind with a known
good yield"*, and I have been recommending it as the handoff on that basis.

**Having now worked thirteen of its cases beyond §853, that estimate is too
optimistic and should not be inherited as written.** The tally:

| case(s) | outcome |
|---|---|
| nullable object-literal member | **§853 landed: +108 lines, +16 cases** |
| `1 + {}` ×3 | **refused** — §257, twice-measured, reopening condition named |
| JSON module ×4 | another crate — `resolveJsonModule` in `tsr-compiler` |
| destructured `catch` ×2 | **§855 falsified** — the obvious divergence is real but is not the blocker; cause still unknown |
| `Symbol()` wanting `unique symbol` | needs the `unique symbol` machinery |
| `{ ...value }` wanting `{}` | JSDoc `@template` generics in a `.js` file |
| implicit-`any` parameters ×2 | work correctly in isolation; the corpus gap is elsewhere in those cases |

So of thirteen sampled: **one mechanism landed, three refused, four belong to another
crate, and five need a feature or a diagnosis deeper than one line.** The
one-case-per-line ratio holds for *counting* the board, not for working it.

### What survives of §852, and it is still the best thing left

The board's two structural properties are unchanged and are what make it worth
inheriting:

- **Zero `RIGHT->WRONG` exposure**, because these cases have no wrong lines to lose.
  That is a property of the selection, not of my estimate.
- **The case yield when a mechanism does land is extreme.** §853 returned **16
  cases** from one gated condition, and *thirteen of those sixteen were cases the
  board never named* — the board pointed at three nearly-right cases and the
  mechanism paid everywhere else.

That second point is the real value and it survives the correction: the board is a
**mechanism-finder**, not a queue of one-liners. Worked that way — read a cluster,
find the shared cause, measure corpus-wide — it produced the session's second-best
landing. Worked as a checklist it will disappoint.

> **Why this correction is recorded rather than quietly dropped**: §852.1's number
> is the one a future session would plan against, and planning a grind that turns
> out to need four features is how a session gets spent with nothing to show.
> `STATUS.md`'s own rule — *correct the record when a number turns out to be wrong,
> and note that it was corrected.*

## §858: element access with a literal key does not take the APPARENT type

Found by working §852's board as §857 says to — hunting a shared cause rather than a
queue. Grouping every zero-wrong gap line by its failing *expression text* across
distinct cases surfaces this immediately:

```
 3 cases  x['doStuff']()            extendBooleanInterface, extendNumberInterface, extendStringInterface
 3 cases  x['doStuff']
 3 cases  x['doOtherStuff']('hm')
 3 cases  x['doOtherStuff']
```

Three cases, **4 gaps each, zero wrong lines** — `RIGHT 23 / WRONG 0 / GAP 4` for
each of `extendBooleanInterface`, `extendNumberInterface`, `extendStringInterface`.

The baseline shows the split precisely. For `interface Number { doStuff(): string }`
and `var x = 1`:

```
A8   >x.doStuff : () => string        <- RIGHT
A18  >x['doStuff']() : string         <- GAP
```

**The dotted form works and the element-access form does not.** Four probes confirm
the boundary is the *receiver*, not the syntax:

```
dot on number          => () => string     correct
index on number        => error
index on interface     => () => string     correct
index on literal type  => () => string     correct
```

So element access with a string-literal key resolves members fine — except on a
primitive, where the member lives on the global wrapper interface. `access_member_lookup`
reaches it through `get_apparent_type`; `element_access_lookup` passes `object_type`
straight to `get_type_of_property_of_type`.

This port's `apparent_type` is the **primitive arms only** (`crates/tsr-checker/src/members.rs`
documents exactly that), so it returns every non-primitive unchanged — which makes
the change additive rather than a redirection.

### The bar

1. **Primary.** The three cases close: **+12 lines, +3 cases**. Falsified if none moves.
2. **Safety.** `RIGHT->WRONG ≤ 5`. The arm now resolves members on primitive
   receivers where it gapped, so the available direction is `GAP->RIGHT` or §620's
   accepted `GAP->WRONG`.
3. **Falsifier.** If the cases do not move, the apparent type is not what the lookup
   is missing.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §858 result — +84, zero adverse, and the named cases did NOT close

| leg | registered | measured |
|---|---|---|
| 1 primary | the three cases close, +12 lines / +3 cases | **0** — all three still `RIGHT 23 / WRONG 0 / GAP 4` |
| 2 safety | `RIGHT->WRONG` ≤ 5 | **0** |
| 3 falsifier | if the cases do not move, the apparent type is not what is missing | see below |
| 4 regression | tests + clippy | 145 suites, clippy clean |

**`GAP->RIGHT 67` and `WRONG->RIGHT 17` — +84 right lines, not one lost** — in cases
the board never named: `propertyAccessOnTypeParameterWithConstraints` 14+10,
`propertyAccessOnTypeParameterWithConstraints2` 12, `optionalChainingInference` 10,
`stringPropertyAccessWithError` 3.

The falsifier needs care rather than a mechanical revert, and the probe is what
decides it. **The probe moved**: `index on number` went `error -> () => string`. So
the apparent type *was* missing and the fix *is* the fix — it simply is not the
*only* thing wrong in the three surfacing cases.

The difference between probe and corpus is `lib.d.ts`. In the corpus, `interface
Number { doStuff(): string }` is a **declaration merge** into the global `Number`,
and this port's `apparent_type` maps `number` to a `Number` symbol that does not
carry the local augmentation. The dotted road reaches it — A8 is RIGHT — so the two
roads resolve the global interface differently, and *that* is the second blocker.

Kept, because unlike §855 the mechanism **demonstrably fires** (§823's rule), it is
a verbatim alignment with `access_member_lookup`, and it measures +84/0.

> **Third time this session that a bar's named cases stayed shut while the change
> paid elsewhere** (§844, §846, §858). The pattern is now unmistakable and worth
> stating as a rule: **at this depth a surfacing case is usually the one with TWO
> defects — that is why it was still visible on a board everything else had fallen
> off.** Sizing a bar by the case that surfaced the mechanism systematically
> under-predicts, and the leg should be registered against the *probe*, not the case.

### The residue, located

`extendBooleanInterface` / `extendNumberInterface` / `extendStringInterface`, 12
lines, 3 cases: **global-interface declaration merging as seen by `apparent_type`**.
The next probe is to ask `apparent_type(number)` for its symbol's declarations in a
corpus run and compare against what `access_member_lookup`'s fallback chain reaches.

## §859: the object-literal index refusal, opened — upstream's filter verified, and the obstacle named

Continuing §858's method — cluster the zero-wrong board by failing *expression* text
— surfaced `[""]` gapping in three cases (`computedPropertyNames10_ES6` and
siblings). Probing walked it down to something much broader than the cluster:

```
index alone           ({ [s]() { } })                 => { [x: string]: () => void; }   correct
index + empty name    ({ [s]() { }, [""]() { } })     => error
index + named         ({ [s]() { }, ["a"]() { } })    => error
index + plain member  ({ [s]() { }, a() { } })        => error
numidx + numeric      ({ [n]() { }, [0]() { } })      => error
```

**Any object literal with both a computed index-producing member and a named member
answers `error`** — including the plainest possible spelling, `{ [s]() {}, a() {} }`.

This is `objects.rs`'s §206/§551 refusal, and it is deliberate. Its own text declines
three shapes and gives each a reason; the string-key one reads:

> **A STRING key beside named members.** Upstream's filter keeps everything but
> symbol-named properties, so every named member contributes to the value union — a
> different computation, not this one, and it is **measured separately or not at
> all**.

### Upstream's filter, verified

`getObjectLiteralIndexInfo` (`checker.go:19721`):

```go
for _, prop := range properties {
    if keyType == c.stringType && !c.isSymbolWithSymbolName(prop) ||
        keyType == c.numberType && c.isSymbolWithNumericName(prop) ||
        keyType == c.esSymbolType && c.isSymbolWithSymbolName(prop) {
        propTypes = append(propTypes, c.getTypeOfSymbol(prop))
        ...
```

So for a string key **every non-symbol-named property contributes its type**, union
with `UnionReductionSubtype`, and the named members *also* stay as members. The
corpus baseline agrees: `computedPropertyNames10_ES6` wants
`{ [x: string]: () => void; [x: number]: () => void; ""(): void; 0(): void; "hello bye"(): void; }`
— both index signatures **and** the literal-named members.

The refusal's description of upstream is exactly right. It was never wrong; it was
unmeasured.

### Why it is not built here, and what it needs

The union machinery already exists and is good — `union_with_subtype_reduction` with
SS331's callable/plain partition, immediately below the guard. The obstacle is
elsewhere:

**`Member` holds printed strings, not `TypeId`s.**

```rust
pub(crate) enum Member {
    Property { name: String, optional: bool, readonly: bool, printed: String },
    Signature { printed: String },
    Index { readonly: bool, name: String, key: String, value: String },
}
```

Upstream unions the member *types*; this port has only their rendered text by the
time the guard runs. Supplying them means threading a parallel `(name, TypeId)` list
through the **eight-plus** `members.push` / `upsert_member` sites in
`check_object_literal` — a medium refactor of a function whose every arm carries its
own §-numbered reasoning, against a population nobody has sized.

That is a real item, not a small one, and doing it at the end of a session against an
unmeasured population is how §850 went. **Left for a session that can start with the
sizing.**

### The sizing to do first, and it is one script

Count corpus lines in object literals that have both a computed member and a named
member. If it is small, the refusal stands as it is and should be annotated with the
number so nobody opens it a third time; if it is large, the parallel-type-list
refactor is justified and this note has the upstream rule already verified and the
guard already located.

`computedPropertyNames10_ES6` additionally needs the **mixed key kinds** half (it has
both `[s]` and `[n]`), so the three cases that surfaced this will not fall to the
string-key half alone — §858's lesson about surfacing cases having two defects,
holding for a third time in a row.

### §859 result — sized, and the refusal STANDS with the number attached

The sizing I said should come first, run: a corpus query for wanted object types
carrying **both** an index signature and a named member returns

```
131 non-RIGHT lines across 46 cases — WRONG 113, GAP 18
top: complexRecursiveCollections 18, objectFreeze 8, constAssertions 8, indexSignatures1 7
```

and that number is an **over-count**, deliberately measured loose: the query matches
any *declared* object type with an index signature, while the guard only governs
object **literals**. The honest population is the gap subset and a fraction of the
wrong lines — **tens of lines**.

Against that: `Member` holds printed strings, so the fix needs a parallel
`(name, TypeId)` list threaded through eight-plus accumulation sites in
`check_object_literal`, a function whose every arm carries its own §-numbered
reasoning. **A medium refactor of a delicate function for tens of lines is the wrong
trade, and the refusal stands.**

The number is now written into `objects.rs` beside the refusal itself, with the
reopening condition (*re-run the query*) and the warning that
`computedPropertyNames10_ES6` needs the mixed-key-kinds half as well. That is the
point of the exercise: **§257 saved this session a fourth attempt at the `1 + {}`
fallback because its numbers were recorded in the code**. This one now has the same
protection, and §851's survey shows how quickly a plausible-looking cluster comes
back around.

## §860: calling the result of a call or `new` on an `any`

The `newWithSpread` cluster from §852's board — `new f(1, 2, ...a)()` wanting `any`
in two zero-wrong cases. Probing separates it cleanly:

```
declare const f: any;  f()        => any      correct
declare const f: any;  f.x()      => any      correct
declare const f: any;  f()()      => error
declare const f: any; (new f())() => error
```

So calling an `any` works, and calling **the result of a call or `new`** does not,
even though that result is itself `any`.

`is_untyped_call_target` (`crates/tsr-checker/src/calls.rs`) requires
`any_is_written_in_an_annotation(callee)`, and that narrowing is load-bearing — its
own comment records the measurement:

> NARROWED after the first run measured **248 gap→wrong** against a bar of 20. …
> the test is not "is the type `any`" but "did the source **say** `any`". That is
> the only form in which this port's `any` and upstream's are the same claim.

For `f()()` the inner `any` comes from a call *result*, so the provenance test fails
and the outer call falls through to signature resolution on `any`, which gaps.

### The extension, and why it keeps the narrowing's guarantee

Upstream does not ask where the `any` came from — `resolveCallExpression` tests
`isTypeAny(funcType)` and answers `anySignature`. This port asks because its own
`any` is often an unported mechanism rather than a claim.

**A call or `new` whose own callee was an untyped call target is upstream's `any`
transitively.** The provenance is still *written*; it is one hop further out, which
is exactly the shape the function already handles for casts — its comment covers
`var u = (a2 as any); u()` as "the same written any one hop later". Recursing
through a call/new callee extends that reasoning without weakening it: the base case
is still a written annotation or an `as any`.

### The bar

1. **Primary.** `newWithSpread`'s 2 cases plus ≥ **+8 lines** corpus-wide.
   Falsified if the probe does not move.
2. **Safety.** `RIGHT->WRONG ≤ 10` **and `GAP->WRONG ≤ 30`**. The second is the real
   one: the narrowing this extends exists because the unrestricted version cost 248
   gap→wrong, and a transitive rule must not reopen that.
3. **Falsifier.** `f()()` must answer `any`.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §860 result — the probe moved, the corpus did not, and I had misread the cluster

| leg | registered | measured |
|---|---|---|
| 1 primary | `newWithSpread`'s 2 cases + ≥ +8 lines | **0** |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 30` | **0 and 0** |
| 3 falsifier | `f()()` must answer `any` | **it does** — `error -> any` |
| 4 regression | tests + clippy | 7 tests in the file, 145 suites |

The falsifier passed and the primary failed, which is the signature of a correct
change aimed at the wrong population — and here the misreading is mine and worth
naming.

**I read the cluster from its expression text and never opened the baseline.** The
clustering printed `new f(1, 2, ...a, "string")()`, and I took the trailing `()` to
mean *calling the result of a `new`*. The baseline says otherwise:

```
>new f(1, 2, "string") : any
>new f(1, 2, ...a) : any
>new f(1, 2, ...a, "string") : any
```

`f` is a **function declaration**, not an `any`. Upstream answers `any` because
`new` on a value with no construct signature is error recovery, and that is a
different rule entirely from the one I built. The cluster's 8 gaps in
`conformance/newWithSpread` (310 RIGHT / 0 WRONG / 8 GAP) belong to it.

> **Fifth time this session that acting on an unverified link cost a build.** The
> rule §846 earned — *every link a bar depends on must be read or measured in this
> session* — has a corollary I keep missing: **a clustering instrument reports a
> string, and a string is not a reading.** Opening the baseline is one command and
> it was skipped because the cluster looked self-explanatory.

### Kept anyway, and why

`declare const f: any; f()()` answering `error` is a real divergence — upstream's
`resolveCallExpression` tests `isTypeAny(funcType)` and answers `anySignature`
regardless of provenance. The extension fires (the probe moved), costs nothing
measured, and preserves the narrowing's guarantee: the base case is still a written
annotation or an `as any`, so a chain qualifies only if its root does. §800's rule —
*the corpus is the arbiter of VALUE, not of CORRECTNESS* — and the test is the
witness the corpus does not provide.

### The real item, identified

**`new` on a value with no construct signature answers `any`.** `newWithSpread` is
310 RIGHT / 0 WRONG / 8 GAP, so it is a zero-risk case on §852's board, and the rule
is upstream's error recovery in `resolveNewExpression`. Not built here — it is the
same *answer `any` where this port gaps* shape that §257 refused for `+`, and it
needs its own sizing and its own bar, with ADR-0038's question asked explicitly:
whether upstream's line is deliberate recovery (as `anySignature` is) or a failure
wearing `any`'s name.

## §861: `new` on a target with no construct signature is `any`

§860's real item, taken up with the baseline read first this time.
`conformance/newWithSpread` is **310 RIGHT / 0 WRONG / 8 GAP** — a zero-risk case on
§852's board — and every gap is the same shape:

```ts
function f(x: number, y: number, ...z: string[]) { }
new f(1, 2, "string")        // >new f(1, 2, "string") : any
new f(1, 2, ...a)            // >new f(1, 2, ...a) : any
```

`f` is a plain function declaration: **call signatures, no construct signature.**

Upstream is explicit and deliberate (`checker.go:8334-8342`), and the comment is the
specification:

```go
if ast.IsNewExpression(node) {
    declaration := signature.declaration
    if declaration != nil && !ast.IsConstructorDeclaration(declaration) &&
        !ast.IsConstructSignatureDeclaration(declaration) && !ast.IsConstructorTypeNode(declaration) {
        // When resolved signature is a call signature (and not a construct signature) the result type is any
        if c.noImplicitAny {
            c.error(node, diagnostics.X_new_expression_whose_target_lacks_a_construct_signature_implicitly_has_an_any_type)
        }
        return c.anyType
    }
}
```

**ADR-0038's question, asked explicitly because §860's note said to:** this is
`anyType`, not `errorType`; it carries its own comment saying the result *is* any;
and the diagnostic beside it is `noImplicitAny`-gated, which is what a deliberate
recovery looks like rather than a failure wearing `any`'s name. It is the same
standing as `anySignature` (`checker.go:1042`) that
`docs/architecture/checker-notes-calleegap.md` already argues from. Portable.

`resolveNewExpression` tries construct signatures first and falls back to call
signatures (`checker.go:8603` then `:8632`), so upstream reaches that arm exactly
when the target **has call signatures and no construct signature** — which is the
condition expressible on the type here, without resolving a signature first.

### The bar

1. **Primary.** `newWithSpread`'s 8 gaps close: ≥ **+8 lines, +1 case**.
2. **Safety.** `RIGHT->WRONG ≤ 5` and `GAP->WRONG ≤ 20`. The exposure is any `new`
   this port currently answers from the class road that would now short-circuit —
   which the class gate below is there to prevent.
3. **Falsifier.** `new f()` on `function f() {}` must answer `any`.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §861 result — +10, zero adverse, and the surfacing case shut for the FOURTH time

| leg | registered | measured |
|---|---|---|
| 1 primary | `newWithSpread`'s 8 gaps close, +8 lines / +1 case | **0** — still 310 / 0 / 8 |
| 2 safety | `RIGHT->WRONG ≤ 5`, `GAP->WRONG ≤ 20` | **0 and 0** |
| 3 falsifier | `new f()` on `function f() {}` must answer `any` | **it does** |
| 4 regression | tests + clippy + anchors | 145 suites, clean, 3,254 |

`GAP->RIGHT 10` — `privateNameMethodCallExpression` 5,
`privateNameStaticMethodCallExpression` 5 — **not one line lost**, and both controls
hold: a class still resolves its declared instance type, a constructor type still
resolves its construct signature's return.

`newWithSpread`'s 8 gaps are blocked by something further in (its arguments carry
spreads, which is the case's whole point). **Fourth consecutive bar whose named case
stayed shut while the change paid elsewhere** — §844, §846, §858, §861 — and the
rule §858 stated now has four data points behind it:

> A surfacing case is usually the one with **two** defects. That is exactly why it is
> still visible on a board everything else has fallen off, and it is why sizing a bar
> by the case that surfaced the mechanism systematically under-predicts. **Register
> the leg against the probe.**

Every one of those four changes was kept, all four measured zero adverse lines, and
three of the four paid in cases the board never named. The method is working; the
*prediction* is what is wrong, and only in one direction.

### ADR-0038, asked and answered

§860's note said to ask it explicitly before porting an `any`. Done: upstream returns
`anyType`, not `errorType`; it carries its own comment stating the result *is* any;
and its diagnostic is `noImplicitAny`-gated. That is the standing of `anySignature`
(`checker.go:1042`), which `checker-notes-calleegap.md` already argues is an honest
computation rather than a gap wearing `any`'s name. This is the second `any` this
session checked against that test — §257's `+` fallback failed it on measurement
rather than on provenance, and this one passes both.
