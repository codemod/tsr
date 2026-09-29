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

## §862: calling the result of a constructorless `new`

§861's own residue, and this time the surfacing case **did** move — because I read the
baseline first and found the gap was not where the cluster's text suggested:

```
A61  >new f(1, 2, "string")() : any     <- the GAP
A62  >new f(1, 2, "string") : any       <- RIGHT after §861
A63  >f : (x: number, y: number, ...z: string[]) => void
```

The gap is on **calling** the `new` result, not on the `new`. §860 built exactly that
rule, but gated it on the root being a *written* `any`, and `f` here is a properly
typed function — so §860 could not fire.

§861 changed the premise. `new f(...)` where `f` lacks a construct signature is now
upstream's own `anyType` (`checker.go:8334-8342`), a deliberate recovery rather than
an unported gap — which is precisely the provenance
`any_is_written_in_an_annotation` tests for. It gains a **second base case**, beside
the cast hop that already covers `var u = (a2 as any); u()`.

The §861 condition is now one shared predicate,
`new_target_lacks_a_construct_signature`, read by both the `new` road and the
provenance test — one fact in one place rather than §856.1's `has_step_arm` mistake
of keeping two copies.

**`GAP->RIGHT 6`** (`newWithSpread` 3, `newWithSpreadES6` 3), zero adverse, 145
suites, clippy clean, 3,255 anchors.

### The reading that made the difference

§860 failed on this exact cluster because I took the clustering instrument's
expression text at face value. §862 is the same cluster, opened properly: one `awk`
over the baseline showed the gap sitting on A61 rather than A62, and that single line
is the whole difference between a change that measured zero and one that measured +6.

> **Two entries, same cluster, opposite outcomes, and the only variable was whether I
> opened the baseline.** That is the cheapest lesson available in this file, and it
> cost two builds and a `scorepair` to learn twice.

`§861 + §862` together: **+16 lines, +2 cases**, from one upstream rule and its
consequence.

## §863: an object-literal member's arrow cannot show it has no contextual type

Chasing §852's `callback(_this)` cluster (3 zero-wrong cases) to its cause, with the
baseline opened first per §862. `noCollisionThisExpressionAndLocalVarInConstructor`
gaps at A2 — the whole object literal:

```
A2   >x2 : { doStuff: (callback: any) => () => any; }
A4   >doStuff : (callback: any) => () => any
A6   >callback : any
A10  >callback(_this) : any
```

Probing walked it somewhere much broader than the cluster:

```
((callback) => callback(1))     => (callback: any) => any    correct
({ f: () => 1 })                => { f: () => number; }      correct
({ f: (c) => 1 })               => error
({ f: (c) => c(1) })            => error
```

**Any object-literal member whose value is an arrow with an implicitly-`any`
parameter errors** — the same arrow standalone is fine.

`signatures.rs` admits an implicit `any` for an arrow parameter only when
`has_no_contextual_type` can **show** there is no contextual type at the arrow's
position — §561's gate, whose own comment records that admitting them wholesale cost
**78 gap→wrong** (`coAndContraVariantInferences3`). The walk
(`has_no_contextual_type`) climbs through the arms of upstream's `getContextualType`
that answer nil, and ends:

```rust
_ => return false,
```

A `PropertyAssignment` parent hits that catch-all, so an object-literal member can
**never** show absence, and the gate correctly-but-uselessly declines.

### The arm

Upstream's contextual type for an object-literal element is
`getContextualTypeForObjectLiteralElement`, which asks the **object literal's** own
contextual type and looks the property up in it. So a member has no contextual type
exactly when the literal has none — the walk should *climb* through
`PropertyAssignment` and `ObjectLiteralExpression` rather than give up, which is what
it already does for parenthesized expressions and conditional branches, and for the
same reason.

Two climbing arms make `({ f: (c) => 1 })` resolve: arrow → property → literal →
parenthesized → expression statement → **no contextual type**. And
`const x: { f: (c: string) => number } = { f: (c) => 1 }` still declines, because the
climb reaches a `VariableDeclaration` *with* an annotation and the existing arm
answers false.

### The bar

1. **Primary.** The 3 `callback(_this)` cases plus ≥ **+10 lines** corpus-wide.
2. **Safety.** `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 40`.
3. **Falsifier — §561's own, inherited verbatim.** *"If
   `coAndContraVariantInferences3` loses lines, the predicate is not showing what it
   claims and this comes straight back out."*
4. **Regression.** `cargo test --workspace`; clippy clean.

### §863 result — +187 lines, +10 cases, ZERO `RIGHT->WRONG`

| leg | registered | measured |
|---|---|---|
| 1 primary | 3 cases + ≥ +10 lines | **+187 lines, +10 cases** |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 40` | **0** and **11** |
| 3 falsifier | `coAndContraVariantInferences3` must not lose lines | **in no transition list** — clears |
| 4 regression | tests + clippy + anchors | 145 suites, clean, 3,255 |

`GAP->RIGHT 110` and `WRONG->RIGHT 77`, spread far past the cluster that surfaced it:
`callSignaturesWithParameterInitializers` 30,
`noCollisionThisExpressionAndLocalVarInAccessors` 16, `asyncFunctionsAcrossFiles` 14,
`plusOperatorWithAnyOtherType` 11, `commentsAfterFunctionExpression1` 10.

The 11 `GAP->WRONG` are JSDoc (`checkJsdocSatisfiesTag2` 7, `jsdocTemplateTag2` 4) —
§620's accepted direction, and a family this port does not model.

**The biggest landing since §841**, and it came from two climbing arms in a predicate
that already climbed for parentheses.

### Why §561's gate was right and still lost 187 lines

The gate is not wrong: admitting an arrow's implicit `any` wholesale cost 78
gap→wrong when §429 tried it, and `has_no_contextual_type` is the machinery that
makes the admission safe. What was missing is that **the walk stopped at the first
node kind nobody had added an arm for**, and its catch-all is `return false` —
*cannot show absence*. That is the safe default, and it silently made an entire
common position unreachable.

> A conservative default in a *walk* is not conservative in the same way as a
> conservative default in a *test*. The test declines one question; the walk declines
> every question that passes through the node kind. `PropertyAssignment` is on the
> path from most arrows in real code to their enclosing statement, so one missing arm
> disabled the gate for a whole class of positions — 187 lines' worth, which nothing
> in the gate's own reasoning would have predicted.

Worth checking the other catch-alls in this walk for the same shape: any node kind
that commonly sits between an arrow and its statement, and has no arm, is disabling
§561's gate exactly as `PropertyAssignment` was.

## §864: auditing §863's walk — one arm right, two wrong, and the negative is the finding

§863's note said the walk's other catch-alls deserve the same audit. Probing the
positions an arrow commonly sits in:

```
([(c) => 1])                        => error
(!((c) => 1))                       => error
(function () { return (c) => 1; })  => () => any     (the inner arrow gapped)
({ f() { return (c) => 1; } })      => { f(): any; }  (likewise)
(((c) => 1) as any)                 => any
```

Three candidate arms, all justified by reading `getContextualType`'s dispatch
(`checker.go:29343`):

- **`ArrayLiteralExpression`** — upstream has an arm
  (`getContextualTypeForElementExpression`, `:29380`), which asks the *array's*
  contextual type, so an element should climb exactly as an object-literal member
  does.
- **`NonNullExpression`** (`:29394`) — answers the parent's own context, so it climbs.
- **Unary operands** — **no arm at all**, so the dispatch's default answers nil and
  absence is showable.

### Measured separately, and they disagree

| arms | measured |
|---|---|
| all three | `WRONG->RIGHT 1`, **`GAP->WRONG 3`** |
| `ArrayLiteral` + `NonNull` only | **0 gained, `GAP->WRONG 3`** (`nestedRecursiveLambda`) |
| unary only | **`WRONG->RIGHT 1`, zero adverse** |

So the unary arm is right and the climbing arms are wrong, and **the reasoning that
justified them was identical to §863's, which was right.** That is the finding.

> **Upstream having a dispatch arm is necessary for the climb to be correct, but not
> sufficient.** The climb asserts *this port can show the element has no contextual
> type because it can show the array has none* — and that second claim depends on how
> **this port** computes an array's contextual type, not on upstream's dispatch
> shape. §863's object-literal climb happened to be sound; the array one is not, and
> nothing in the upstream reading distinguishes them. Only the measurement did.

Kept: the unary arm, **+1 and zero adverse**. Rejected and recorded in
`signatures.rs` beside it: the `ArrayLiteralExpression`/`NonNullExpression` climb, at
**0 : 3**, so the next session reading §863's "audit the other catch-alls" note finds
the audit already done and its negative already paid for.

The `ReturnStatement` position remains unported — it needs the containing signature's
return annotation rather than a climb, which is a different mechanism and has not
been sized.

## §865: a `return` expression's contextual type

§864 left the `return` position unported, noting it *"needs the containing
signature's return annotation rather than a climb"*. Upstream's rule is two
functions and both are short.

`getContextualTypeForReturnExpression` (`checker.go:29621`) takes the containing
function and asks `getContextualReturnType` (`:29665`), whose first lines are the
whole story:

```go
// If the containing function has a return type annotation, is a constructor, or is a get accessor whose
// corresponding set accessor has a type annotation, return statements in the function are contextually typed
returnType := c.getReturnTypeFromAnnotation(functionDecl)
if returnType != nil {
    return returnType
}
// Otherwise, if the containing function is contextually typed by a function type with exactly one call signature
// and that call signature is non-generic, return statements are contextually typed by the return type of the signature
signature := c.getContextualSignatureForFunctionLikeDeclaration(functionDecl)
```

So a return expression has **no** contextual type exactly when the containing
function has **no return annotation** *and* has **no contextual signature** — and
that second condition is `has_no_contextual_type(fn)`, this walk itself, one level
out. The generator and async arms below only *narrow* a contextual return type; they
never create one, so absence propagates through them unchanged.

Both pieces already exist in this port: `containing_function`
(`crate::expressions`), `signature_parts_of(..).return_annotation`, and
`declaration_takes_no_contextual_return`, which already recurses into
`has_no_contextual_type` for an arrow or function expression (§169 built it for the
generator gate, for the same reason).

**The annotation check is not in that helper** — it answers `true` for any
`FunctionDeclaration` — so the arm tests the annotation itself before delegating,
which is the order `getContextualReturnType` uses.

### The bar

1. **Primary.** ≥ **+20 lines**. `(function () { return (c) => 1; })` currently
   answers `() => any` — the inner arrow gaps — and so does the method form.
2. **Safety.** `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 40`.
3. **Falsifier.** §561's, inherited for the third time: if
   `coAndContraVariantInferences3` loses lines, out it comes. Plus §864's warning —
   **an upstream dispatch arm justifies a climb but does not make it sound**, so a
   negative here is expected to be possible and the measurement decides, not the
   reading.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §865 result — +84 lines, zero `RIGHT->WRONG`

| leg | registered | measured |
|---|---|---|
| 1 primary | ≥ +20 lines | **+84** (`GAP->RIGHT` 38, `WRONG->RIGHT` 46) |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 40` | **0** and **4** |
| 3 falsifier | `coAndContraVariantInferences3` must not lose lines | absent from every transition list |
| 4 regression | tests + clippy + anchors | 145 suites, clean, 3,257 |

`asyncMethodWithSuper_es6` 24 across both directions,
`collisionThisExpressionAndLocalVarInMethod` 6,
`noCollisionThisExpressionAndLocalVarInMethod` 6,
`collisionThisExpressionAndPropertyNameAsConstuctorParameter` 8. The 4
`GAP->WRONG` are `jsdocSignatureOnReturnedFunction` — §620's accepted direction, and
JSDoc again.

§864's warning was heeded and turned out not to bite: unlike the array climb, the
return arm is not *"this port can show absence here because it can show absence
there"* through a computation the port does differently — it is a syntactic test (is
there a return annotation?) followed by the same walk on the enclosing function.
**That is the distinction worth carrying**: a climb is sound when the step is
syntactic or when the recursion is into this walk itself; it is unsound when it
routes through a *type* this port computes its own way.

### §863–§865 together

Three entries, one predicate: **+272 lines, +10 cases, zero `RIGHT->WRONG` across
all three**, from adding four arms and rejecting two. Every one of them was a node
kind sitting between an arrow and its enclosing statement, where
`has_no_contextual_type`'s `_ => return false` silently disabled §561's gate.

The walk now handles: expression statement, variable declaration, parenthesized,
conditional branch, binary operand, **property assignment**, **object literal**,
**unary operand**, **return statement**. Still unhandled and unmeasured:
`AsExpression`/`SatisfiesExpression` (upstream has arms at `:29368`/`:29396`),
`TemplateSpan`, `JsxExpression`, `SpreadAssignment`, and array elements — the last
**measured and rejected** at 0 : 3 in §864.

## §866: the last two sound arms, and the rule that predicted which ones

§865 stated when a climb is sound: **the step is syntactic, or the recursion is into
this walk itself** — and unsound when it routes through *a type this port computes
its own way*. §866 is that rule used as a filter rather than discovered afterwards,
over the four arms §865 listed as unhandled.

| candidate | upstream's arm | rule says |
|---|---|---|
| `SpreadAssignment` (`:29378`) | `return c.getContextualType(parent.Parent, …)` | **sound** — pure recursion |
| `TemplateSpan` (`:29390`) | tagged delegates, otherwise **nil** | **sound** — syntactic |
| `AsExpression`/`SatisfiesExpression` (`:29368`, `:29396`) | returns the asserted type | no change needed: a contextual type *exists*, and the catch-all already answers `false` |
| `JsxExpression` (`:29400`) | JSX contextual typing | **unsound by the rule** — routes through types |

And the counter-example the rule was learned from is right there in the same
dispatch: `ArrayLiteralExpression` (`:29380`) opens
`t := c.getApparentTypeOfContextualType(parent, contextFlags)` — a type — which is
exactly why §864 measured that climb at **0 : 3** while every syntactic-or-recursive
arm has measured positive or neutral.

**Measured: `+2`, zero adverse** (`templateStringWithEmbeddedArrowFunctionES6` 1,
`templateStringWithEmbeddedArrowFunction` 1). The primary leg asked for ≥ +10 and is
**falsified**; both arms are verbatim ports of two-line upstream functions, cost
nothing, and are kept on §800's rule. The two gaining cases are both template
strings, which names the `TemplateSpan` arm; the `SpreadAssignment` arm measured
nothing and is faithful-but-unwitnessed.

### The walk, finished

`has_no_contextual_type` now has an arm for every node kind between an arrow and its
statement that upstream's dispatch answers nil or a climb for: expression statement,
variable declaration, parenthesized, conditional branch, binary operand, property
assignment, object literal, spread assignment, template span, unary operand, return
statement. The two it deliberately does **not** have — array element and
`JsxExpression` — are both the type-routing shape, one measured at 0 : 3 and one
predicted by the same rule.

**§863–§866: +274 lines, +10 cases, zero `RIGHT->WRONG` across four entries**, from
one predicate whose catch-all was `return false`.

## §867: located, not built — `yield*` in an async generator

Grouping §852's zero-wrong board by **wanted type** (rather than by expression, which
after §863–§866 is dominated by bare identifier names) surfaces the async-generator
family at the top of the non-primitive wants:

```
  8 cases  AsyncGenerator<number, void, unknown>
  8 cases  () => AsyncGenerator<number, void, unknown>
```

Corpus-wide, async generators **partly work** — lines wanting an `AsyncGenerator<…>`
are 122 RIGHT / 107 GAP / 56 WRONG — and `crate::signatures` already has the mint
(§640): `is_async` selects `AsyncGenerator` over `Generator`, with the
`AsyncIterableIterator`/`IterableIterator` fallback §511 added.

So the feature is present and something narrower fails. Reading the failing files of
`emitter.asyncGenerators.functionDeclarations.es2018` (9 gaps, and the gaps are in
files F4 and F5 of a seven-file case) gives it in one line each:

```ts
// F4.ts
async function * f4() { const x = yield* [1]; }
// F5.ts
async function * f5() { const x = yield* (async function*() { yield 1; })(); }
```

**Both are `yield*`.** Every other file in the case — plain `yield`, `yield` with no
operand, `await` inside an async generator — is RIGHT.

### Why it is filed rather than built

`yield*` needs the **iteration-type machinery**:
`getIterationTypeOfGeneratorFunctionReturnType` and the async variant, which is what
`getContextualReturnType` (`checker.go:29627-29640`) reaches for and what
`checkYieldExpression`'s delegating half needs to compute the yielded and returned
types of the *operand*. That is a subsystem this port does not have, and it is the
same machinery the `regexMatchAll` cluster on this board needs (`[...matches]`,
`array[0]` wanting `RegExpExecArray` — three more cases).

Sized from the board: **8 async-generator cases plus 3 iterator-protocol cases**, and
an unknown share of the 107 `AsyncGenerator` gaps corpus-wide. That makes it the
largest single remaining item on the zero-wrong board, and a *feature* rather than a
mechanism — which is §857's finding holding: the board's cases are not one-liners,
and its value is in the mechanisms they surface, not the cases themselves.

**Next step if taken up**: port `getIterationTypeOfGeneratorFunctionReturnType` and
its async variant first, in isolation, with unit tests — they are pure functions over
a type — and only then wire `yield*`. The board will show the result without any
`RIGHT->WRONG` exposure, because all eleven cases have zero wrong lines.

## §868: `yield*` reads the delegated iterable's `TReturn` slot

§867 located the board's largest item to `yield*` in an async generator and filed it
as needing the iteration-type subsystem. Reading the port's existing arm shows a
narrower step is available first.

`checkYieldExpression` (`checker.go:10998-11001`):

```go
if node.AsYieldExpression().AsteriskToken != nil {
    use := core.IfElse(isAsync, IterationUseAsyncYieldStar, IterationUseYieldStar)
    return core.OrElse(c.getIterationTypeOfIterable(use, IterationTypeKindReturn, yieldExpressionType, node.Expression()), c.anyType)
}
```

So `yield* X` **is** the *return* iteration type of `X`. This port's unannotated arm
(§349) handles exactly one shape — **sync generator, `Array` operand → `undefined`**,
the lib's `ArrayIterator` TReturn — and answers `error` for everything else, which is
why `async function* f5() { yield* (async function*() { yield 1; })(); }` gaps.

But the whole `Generator` family declares `TReturn` **as its second type argument**:

```
Generator<T, TReturn, TNext>          AsyncGenerator<T, TReturn, TNext>
IterableIterator<T, TReturn, TNext>   AsyncIterableIterator<T, TReturn, TNext>
Iterator<T, TReturn, TNext>           AsyncIterator<T, TReturn, TNext>
```

so when the operand is a reference to one of them the return iteration type is
readable **without** the `[Symbol.iterator]` walk — the same sidestep
`for_of_element_type` already takes for the *first* slot (`crate::symbols`, whose
comment records that late binding blocks the protocol walk and that the lib's own
declaration makes the answer readable anyway). This is that argument at index 1
instead of index 0.

`emitter.asyncGenerators.functionDeclarations.es2018`'s F5 is exactly it:
`(async function*() { yield 1; })()` is `AsyncGenerator<number, void, unknown>`, and
the baseline wants `yield* … : void` — argument 1.

**Async-ness is matched**, not ignored: an async container reads the `Async*`
families and a sync container the sync ones, because `IterationUseAsyncYieldStar`
and `IterationUseYieldStar` are what upstream distinguishes.

**F4 is deliberately left out.** `async function* f4() { yield* [1]; }` wants `any`,
and the sync equivalent wants `undefined` — the async delegator's `TReturn` differs
from the array iterator's. Reading that from the lib needs the protocol walk this
sidestep is avoiding, and hardcoding `any` for async-plus-array would be a guess
about a declaration rather than a read of one.

### The bar

1. **Primary.** ≥ **+8 lines**; the async-generator board cases must move.
2. **Safety.** `RIGHT->WRONG ≤ 5`. The existing sync/Array road is strictly ahead of
   the new arm, so it keeps its answer.
3. **Falsifier.** If nothing moves, the operand is not typing as a `Generator`-family
   reference and the sidestep does not apply.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §868 measured at zero, and §869 is why

§868's `TReturn` read measured **no transitions**, and its own falsifier said what
that meant: *"the operand is not typing as a `Generator`-family reference"*. The gap
list for F5 confirms it — `(async function*() { yield 1; })()` **gaps**, and so does
the inner `async function*() { yield 1; }` itself. The operand chain is broken from
the inside, so nothing downstream of it could fire.

The inner one is a function **expression**, and the yield road refused every
`contextualisable` container through a predicate called
`container_is_provably_uncontextualised` — which recognised exactly **one** shape:

```rust
matches!(self.node_map.get(parent),
    Some(Node::VariableDeclaration(declaration)) if declaration.r#type.is_none())
```

`var x = function*() { }`, and nothing else. Meanwhile §863–§866 had just finished
building the general form of that same question for eleven node kinds.

## §869: the callee has no contextual type, and the one-shape predicate retires

Two changes, one bar.

**The arm.** A call's callee and a tagged template's tag have no contextual type, and
upstream says so in a comment on the line that returns nil
(`getContextualTypeForArgument`, `checker.go:29762-29768`):

```go
argIndex := slices.Index(args, arg)
// -1 for e.g. the expression of a CallExpression, or the tag of a TaggedTemplateExpression
if argIndex == -1 {
    return nil
}
```

An *argument* is contextually typed by its parameter and answers false. Purely
positional — §865's soundness rule admits it.

**The retirement.** `container_is_provably_uncontextualised` is replaced by
`has_no_contextual_type`. It is the same question §169 and §561 already route through
that predicate; the yield road had its own one-shape copy.

| leg | registered | measured |
|---|---|---|
| 1 primary | ≥ +10 lines, F5's lines move | **+47** (`GAP->RIGHT` 28, `WRONG->RIGHT` 19) |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 40` | **0** and **8** |
| 3 falsifier | `coAndContraVariantInferences3` | absent from every transition list |
| 4 regression | tests + clippy + anchors | 145 suites, clean, 3,258 |

`types.asyncGenerators.es2018.1` 7, `targetTypeCalls` 8,
`capturedParametersInInitializers1` 5, `contextuallyTypedIife`/`…Strict` 6 across
both directions.

The 8 `GAP->WRONG` are **IIFEs** (`asyncIIFE` 3, `contextualReturnTypeOfIIFE` 3,
`contextuallyTypedIife` 2), and they are a real tension worth naming: an IIFE's
function expression *is* a callee, so upstream's rule says it has no contextual type
— yet §768/§809 built a separate road that gives an IIFE's parameters their types
from the call's **arguments**. Both are faithful; they are different mechanisms
answering different questions, and this change makes the first one visible where the
second does not reach. §620's accepted direction, and the next thing to read if this
family is opened.

**One-shape predicates standing in for a general one are worth grepping for.** This
is the second in two entries — §856.1's `has_step_arm` was a *duplicate* of a fact,
and this was a *weaker* version of one. Both were invisible until something made the
general form better than the copy.

## §870–§871: async generator expressions, and the awaited yield

§869 made `async function*() { yield 1; }` in callee position *showably*
uncontextual, and the case still gapped. One more line explains it — in
`crate::signatures`, the generator mint:

```rust
if (is_async && generator_expression)
    || (!is_async && !generator_expression
        && !self.declaration_takes_no_contextual_return(declaration, may_return_never))
{
    return None;
}
```

The async-generator-**expression** half is **unconditional**, where every other arm
in that condition asks `declaration_takes_no_contextual_return` first — which routes
a function expression through `has_no_contextual_type`, the predicate §863–§866 and
§869 grew from one node kind to twelve. The refusal was written when the question it
wanted answered had no answer; it now has one.

**§870** gates it the same way as the sync arm: **`GAP->RIGHT 43`**
(`emitter.asyncGenerators.functionExpressions.es2015` 11, `…es2018` 11,
`…classMethods.es2015` 3), **zero `RIGHT->WRONG`**, and `GAP->WRONG 18` — all in
`types.asyncGenerators.es2018.1`, all one shape:
`AsyncGenerator<Promise<number>, …>` where upstream records
`AsyncGenerator<number, …>`.

**§871** is that shape's rule, verbatim
(`getYieldedTypeOfYieldExpression`, `checker.go:11026-11029`):

```go
if !isAsync { return yieldedType }
return c.getAwaitedTypeEx(yieldedType, errorNode, …)
```

In an async generator the yielded type is **awaited**. Applied at the yield-slot
push, with the operand's own type kept when the awaited type is not computable — the
pre-§871 answer rather than a new guess.

It converted **2** of the 18 and no more, so `awaited_type_no_alias` is not resolving
the rest; three of the residue are a different shape again (`void` wanted, `any`
given). Both are named residues rather than mysteries.

### Together

**+45 right lines, zero `RIGHT->WRONG`**, 18 `GAP->WRONG` confined to one case —
§620's accepted direction. `coAndContraVariantInferences3` absent from every
transition list.

> **My §870 bar did not register a `GAP->WRONG` limit**, and should have: 18 is large
> enough that a limit would have forced the §871 investigation *before* the landing
> rather than after. The two arrived in the right order by luck, not by the bar.

### The chain this closes

§867 filed `yield*`-in-an-async-generator as needing the iteration-type subsystem and
sized it at 11 cases. What it actually needed was: a `TReturn` read (§868, measured
zero), a call-callee contextual arm (§869, +47), an unconditional refusal made
conditional (§870, +43), and the awaited yield (§871, +2). **None of it was the
subsystem §867 predicted** — and §867's estimate was wrong in the same direction as
§852.1's, which §857 already corrected once: *the board's cases look like features
and turn out to be gates.*

## §872: the async-generator IIFE residue, localised to one road

`emitter.asyncGenerators.functionDeclarations.es2018` kept its 9 gaps through §868–
§871, and three probe runs narrow the cause to a single road rather than leaving it
as "the case still gaps".

The shape is `async function* f5() { const x = yield* (async function*() { yield 1; })(); }`,
and the gaps include the **inner** function expression's own type (positions 4:4 and
4:5, both wanting `() => AsyncGenerator<number, void, unknown>`).

Probe 1 — the §870 gate:

```
872 async gen expr: parent=Some(ParenthesizedExpression) no_ctx=true
```

**The gate passes.** §869's call-callee arm does what it was built to do: the inner
expression's position is showably uncontextual, so §870's refusal does not fire.

Probe 2 — the mint itself:

```
872 reached mint: AsyncGenerator=true yields=1 returns=0
```

**The mint is reached and succeeds.** `AsyncGenerator` resolves at arity 3, one yield
type was collected, no return types — everything `createGeneratorType` needs.

And the positions still answer `error`.

### What that leaves

The generator return type **is computed** and the function expression's own type road
does not use it. That is not the mint, not the gate, and not §869's predicate — all
three are now known-good by measurement. It is
`get_type_of_function_expression` or whatever the expression road consults between
them, and it is the **only** remaining candidate for these nine lines.

> Worth stating because the obvious diagnosis was wrong three times running here:
> §867 said *subsystem*, §868's arm said *operand doesn't type*, §870's refusal
> looked like the gate. Each was refuted by one probe, and the residue is smaller and
> better placed each time. **A case that survives four landings is not evidence the
> landings were wrong** — §868–§871 took 43 of this family's lines elsewhere while
> these nine stayed put.

**Next probe, named:** instrument `get_type_of_function_expression` for this
declaration and read which of its guards answers `error` when the generator type
behind it is already minted. `crates/tsr-checker/src/signatures.rs` §809's guard is
the first suspect — it declines a function whose contextual signature does not
materialise — though this function has no parameters, which is what §809's guard is
about.

## §873: the road is right and the producer does not use it — and the file gaps WHOLESALE

§872 named `get_type_of_function_expression` as the only remaining candidate. Probed:

```
873 fn-expr kind=FunctionExpression => () => AsyncGenerator<number, void, unknown>
```

**The road returns exactly the wanted type.** `check_expression`'s
`Expression::FunctionExpression` arm routes straight to it
(`crates/tsr-checker/src/expressions.rs:863`), so the type is computed and printable.
Positions 4:4 and 4:5 still report `error`.

So the diagnosis moves again, and this time the shape is different from anything
§867–§872 considered. Counting the gaps against the file:

```
F5.ts  assertions: f5, x, yield* …, (…)(), (…), async function*…, yield 1, 1
gaps:  4:0 4:1 4:2 4:3 4:4 4:5 4:6     — SEVEN of them
```

**Essentially the whole of F5 gaps**, while F4 — the same construct without the IIFE
— gaps only its two `yield*` lines. That is not a per-node failure. A file whose
every assertion answers `error` looks like a *container-level* bail, and this port
has one: `flow_disabled_containers` (§14), where "a reference inside a container whose
analysis tripped the too-large bail answers `errorType`".

### Why this matters beyond nine lines

Every diagnosis in this chain was refuted by the next probe, and each refutation was
cheap:

| § | said | refuted by |
|---|---|---|
| §867 | needs the iteration-type subsystem | §868–§871 landed 90 lines without it |
| §868 | the operand does not type | §869's gap list |
| §870 | the mint refuses async generator expressions | probe: gate passes, `no_ctx=true` |
| §872 | `get_type_of_function_expression` | probe: it returns the right type |

**Four wrong diagnoses, four probes, and the residue got smaller and better placed
every time.** The cost was four builds; the alternative — reasoning to a conclusion
and building on it — is what §845 and §860 did, and both were reverted.

**Next probe, named:** check whether `f5`'s container is in
`flow_disabled_containers` during this case's run. If it is, the nine lines are §14's
bail and not a generator question at all, and the *real* item is whatever trips the
bail on a seven-line file — which would be a much larger finding than these nine
lines, because §14's bail answers `errorType` for **every** reference in the
container it fires on.

## §874: §872 and §873 were written on a STALE reading, and both are corrected here

Both entries opened with "the case kept its 9 gaps through §868–§871". **It did not.**
That number was read from a `verdictdump` run taken immediately after committing
§869, *before* §870 and §871 were built. The case's real state:

```
before §870:  RIGHT 18   WRONG 0   GAP 9
now:          RIGHT 22   WRONG 0   GAP 5
```

So §870/§871 took **four** of this case's own lines as well as the 45 elsewhere, and
the "residue" §872 and §873 were diagnosing was smaller than either of them said.

§873's framing is worse than merely stale. It claimed:

> Essentially the whole of F5 gaps … A file whose every assertion answers `error`
> looks like a *container-level* bail.

The per-file distribution says otherwise — **F5 is 5 RIGHT / 3 GAP**, not seven gaps
out of eight:

```
file 0: 1 RIGHT              file 4 (F5): 5 RIGHT, 3 GAP
file 1: 3 RIGHT              file 5: 4 RIGHT
file 2: 4 RIGHT              file 6: 2 RIGHT
file 3 (F4): 3 RIGHT, 2 GAP
```

I counted the `:4:` lines in a gap list I had already established was stale, and
built a container-bail hypothesis on the count. The probe that followed refuted the
hypothesis on its own terms — `flow_disabled_containers` never fires in this case —
so the conclusion was not acted on, but the reasoning that produced it was worthless
and the entry asserted it as fact.

### What is actually true

- `get_type_of_function_expression` returns the right type for the inner async
  generator (§873's probe, and that measurement stands).
- The case is 22 RIGHT / 5 GAP: two `yield*` lines in F4, and three lines in F5.
- §14's flow bail is not involved.
- The remaining five are **not** diagnosed, and nothing in §872–§873 diagnosed them.

### The process failure, which is the point of writing this

**I re-read a number I had already used, instead of re-measuring it after two
landings changed it.** `scorepair` was re-run after §870 and §871 — that is how their
+43 and +2 were known — but the *case-level* figure was carried forward from before
them.

This session has now recorded five wrong diagnoses in this one chain (§867, §868,
§870's suspicion, §872, §873) and every one was refuted by a cheap probe. Four of
those were honest dead ends. **This one was avoidable arithmetic**, and it is worse,
because a stale number in a note reads exactly like a fresh one to whoever inherits
it.

`STATUS.md`'s own rule — *a number here without a fresh run behind it is worse than no
number* — applies to these notes and not only to the dashboard.

## §875: a parameter of a function TYPE has no contextual type, by construction

§852's board, re-derived fresh (§874's lesson): `renamingDestructuredPropertyInFunctionType2`
is **34 gaps, zero wrong** — third largest on the board and the largest that is not a
known subsystem.

The case walks the same construct through eight declarations, and the split is exact.
F1–F5 annotate the parameter and are RIGHT:

```ts
type F2 = ({ a: string }: O) => any;     //  RIGHT
```

F6 onward drop the annotation and gap:

```ts
type F6 = ({ a: string }) => typeof string;            //  GAP from here
type F7 = ({ a: string, b: number }) => typeof number;
```

An **unannotated binding-pattern parameter** — and §561's gate decides whether such a
parameter may take the implicit `any`:

```rust
if !self.nodes.parent(id).is_some_and(|f| match self.nodes.kind(f) {
    SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration => true,
    SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression => self.has_no_contextual_type(f),
    _ => false,
})
```

Every kind in that list is an **expression- or statement-position** function.
A `FunctionType` node is neither: its parameters live inside a *type annotation*,
where contextual typing does not apply at all — upstream's `getContextualType`
dispatches on expressions, and `getContextuallyTypedParameterType` requires a
function expression with a contextual signature. So these parameters are in
`FunctionDeclaration`'s category — **unconditionally true**, not
`has_no_contextual_type`'s.

The type-position function-like kinds are `FunctionType`, `ConstructorType`,
`CallSignature`, `ConstructSignature`, `MethodSignature` and `IndexSignature`.

> This is the **third** gate in this session found listing expression kinds and
> silently excluding a whole category — after §863's `PropertyAssignment` catch-all
> and §870's unconditional async-generator-expression refusal. The shape is always
> the same: a list written for the cases in front of its author, with a `false`
> default that reads as caution and behaves as a refusal.

### The bar

1. **Primary.** `renamingDestructuredPropertyInFunctionType2`'s 34 gaps: ≥ **+20
   lines, +1 case**.
2. **Safety.** `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 30`.
3. **Falsifier.** If the case does not move, the gate is not what refuses these.
4. **Regression.** `cargo test --workspace`; clippy clean; §561's
   `coAndContraVariantInferences3` must not lose lines.

### §875 result — +10, zero `RIGHT->WRONG`, and the named case barely moved

| leg | registered | measured |
|---|---|---|
| 1 primary | `renamingDestructuredPropertyInFunctionType2` ≥ +20 lines, +1 case | **2 of 34** — falsified |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 30` | **0** and **1** |
| 3 falsifier | case must move | it moved, by 2 |
| 4 regression | tests, clippy, §561's case | 145 suites, clean, absent from transitions |

**+10 right lines** (`destructuringInFunctionType` 5,
`renamingDestructuredPropertyInFunctionType` 2, `destructuringParameterDeclaration2`
1, plus the 2), **not one right line lost**. The surfacing case is still 55 RIGHT /
0 WRONG / 34 GAP — the gate was *a* refusal in its path, not *the* one.

**Fifth time a bar's named case stayed shut while the change paid elsewhere** —
§844, §846, §858, §861, §875. The rule §858 stated has held every time since, and
§875 is the cleanest illustration yet: the change is a verbatim category correction,
it pays in three other cases, and the case that revealed it keeps 32 of its 34 gaps.

### The recurring shape, now three for three

This is the third gate this session found listing expression kinds and silently
excluding a category:

| § | gate | excluded |
|---|---|---|
| §863 | `has_no_contextual_type`'s `_ => return false` | `PropertyAssignment` — every object-literal member |
| §870 | the generator mint's `is_async && generator_expression` | every async generator expression, unconditionally |
| §875 | §561's parameter gate | every function-**type** parameter |

Each was a list written for the cases in front of its author with a `false` default
that reads as caution and behaves as a refusal, and each was worth between +10 and
+187. **Grepping for `_ => false` in gates is now a demonstrated technique in this
codebase, not a hunch.**

## §876: a pattern-named default is refused for a reason that only applies to PARAMETERS

`sourceMapValidationDestructuringForObjectBindingPatternDefaultValues` is **18 gaps,
zero wrong** on §852's board. Probing separates the shape in four runs:

```
const { s: { p: pA } } = o;                      => any     correct (nesting alone is fine)
const { s = { p: "none" } } = o;                 => any     correct (a default alone is fine)
const { s: { p: pA } = { p: "n" } } = o;         => error   <- a PATTERN carrying a default
const { s: { p: pA } = { p: "n" } } = typed;     => error   <- and it is not about the source type
```

`crate::destructure` refuses it outright, and names its reason:

```rust
// The element's name must still be an identifier: a pattern-named default takes
// upstream through `padObjectLiteralType`/`padTupleType` (`checker.go:16808`), unported.
if element.initializer.is_some()
    && !matches!(element.name, Some(tsr_ast::BindingName::Identifier(_)))
{
    return error;
}
```

The mechanism it names is real. **The condition guarding it upstream is not the one
the refusal assumes** (`checkDeclarationInitializer`, `checker.go:16796-16820`):

```go
if ast.IsParameterDeclaration(ast.GetRootDeclaration(declaration)) {
    name := declaration.Name()
    switch name.Kind {
    case ast.KindObjectBindingPattern:
        if isObjectLiteralType(t) { return c.padObjectLiteralType(t, name) }
    case ast.KindArrayBindingPattern:
        if isTupleType(t) { return c.padTupleType(t, name) }
    }
}
return t
```

Padding runs **only when the root declaration is a parameter**, and only when the
initializer's type is an object literal or tuple respectively. For a `const`, `let`
or `for` destructuring the padding never runs and `checkDeclarationInitializer`
simply returns the initializer's type — which is exactly the corpus case
(`for (let { skills: { … } = { … } } of …)`).

So the refusal is correct for a parameter and unnecessary everywhere else. **Fourth
instance this session of a gate whose kind- or shape-list is narrower than the
condition it stands for** (§863, §870, §875, §876).

### The bar

1. **Primary.** The case's 18 gaps: ≥ **+10 lines, +1 case**.
2. **Safety.** `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 30`. The exposure is a
   pattern-named default whose initializer this port types differently from
   upstream's padded form — which is why the parameter half keeps the refusal.
3. **Falsifier.** If the probe's two failing lines do not move, the guard is not what
   refuses them.
4. **Regression.** `cargo test --workspace`; clippy clean.

### §876 result — +37, zero adverse, and the named case CLOSED

| leg | registered | measured |
|---|---|---|
| 1 primary | the case's 18 gaps, ≥ +10 lines / +1 case | **+37 lines**, and the case's 18 gaps **all closed** |
| 2 safety | `RIGHT->WRONG ≤ 10`, `GAP->WRONG ≤ 30` | **0** and **0** |
| 3 falsifier | the probe's failing lines must move | moved |
| 4 regression | tests, clippy, anchors | 145 suites, clean, 3,259 |

`GAP->RIGHT 26` (`sourceMapValidationDestructuringForObjectBindingPatternDefaultValues`
18, `…VariableStatementNestedObjectBindingPatternWithDefaultValues` 6) and
`WRONG->RIGHT 11` (`declarationsAndAssignments` 6,
`declarationEmitDestructuringArrayPattern2` 3), **not one line lost anywhere**.

**The first bar in six whose named case closed completely** — after §844, §846, §858,
§861 and §875 all paid elsewhere while their surfacing case stayed shut. The
difference is worth naming: those five were gates whose *category* was too narrow, so
the surfacing case had a second defect behind the first. This one was a refusal whose
*condition* was too broad — the mechanism it named was real and simply guarded
differently upstream — and removing the over-reach left nothing behind it.

### A test expectation I got wrong twice

My first assertion claimed `const { s: { p: pA } = { p: "n" } } = o` with `o: any`
answers `string`. It answers `any`, correctly: an `any` source destructures to `any`
through the nested pattern and the default is never reached. I then wrote the corpus
shape — with the inner binding carrying its own default too — and asserted `string`
for that, and it is also `any` in this harness.

Both are pinned as they behave, with the reason: the corpus case reaches `string`
through a `for…of` head over a typed source that this harness does not reproduce.
**The witness for §876 is the corpus — 18 gaps closed in the surfacing case — not
these two lines**, and saying so in the test is better than an assertion that looks
like a specification and is really a guess.

*(Seventh and eighth test expectations written from intuition this session; the port
was right both times. §7's running count.)*

## §877: the probe harness could not see inside a class, and read as a failure when it did

Working the small end of §852's board (206 cases with 1–3 gaps), the largest
remaining cluster is `callback(this)` / `callback(_this)` — five zero-wrong cases,
all wanting `any`. Every one puts the shape inside a **class constructor or method**.

Probing them returned `null` for all three fixtures, which looks like "the port
answers `null`" and is really "the harness never reached the node". The walker
descended into blocks, `if` branches and `try`/`catch` bodies, but not class members
— and worse, a fixture written `class K { … }\nnull;` types the trailing `null`,
because the walker takes the *last* expression statement and the class contributed
none.

**Two failure modes in one:** unreachable, and silently reporting the wrong node's
type as though it were the answer.

Extended to method, constructor and accessor bodies. With that, all three shapes
answer correctly:

```
class K { m() { ((callback) => callback(1)); } }                 => (callback: any) => any
class K { m() { ((callback) => callback(this)); } }              => (callback: any) => any
class K { constructor() { ({ d: (cb) => () => cb(this) }); } }   => { d: (cb: any) => () => any; }
```

The third is the corpus shape verbatim, and §863 is what made it type.

### What that rules out

The five `callback(this)` cases keep their 2 gaps each **and this construct is not
why**. That is a negative worth having: it was the obvious suspect, it is now
excluded by measurement, and the next reader does not spend the same three probes on
it.

Pinned as `an_any_parameter_call_types_inside_a_class` — **a probe that passes is
worth keeping when it rules something out**, and it is also the only test exercising
the new walker arm.

> This is the second harness limitation this session to masquerade as a result. §839
> recorded the first: the minimal harness has no `lib.d.ts`, so a probe of
> `typeof x === "function"` narrowing answered `error` and looked like a defect. Both
> times the tell was the same — **a probe answering something structurally unrelated
> to the question** (`null` here, `error` there) rather than a plausible wrong type.

## §878: the near-miss census, re-run — and the largest group located to a road

The board's small end is now mostly shapes needing `Promise`/`Array` from a
`lib.d.ts` the probe harness does not have (§839's limitation, hit four times in one
batch), so this switches lens back to the near-miss census that produced §839.

Fresh run:

```
port OMITS constituents:  179 lines / 57 cases    — 103 of them missing `undefined`
port ADDS  constituents:  482 lines / 120 cases   —  83 of them extra `undefined`
```

**103 lines where the only missing constituent is `undefined`** is the largest
coherent near-miss group left, and a third of it sits in three optional-chain cases:
`elementAccessChain` 18, `propertyAccessChain` 9, `deleteChain` 6.

`elementAccessChain` (275 RIGHT / 30 WRONG / 0 GAP) carries **two** shapes, not one:

```
want { b: undefined | { c: string; }; }            got { b: { c: string; }; }
want { d?: { e: string; }; } | undefined           got { d?: { e: string; }; }
```

The second is the optional chain's own result — `a?.b` must re-union `undefined`,
which `checker-notes-callres.md` §22 records as ported for **calls**.

The first is not a chain question at all. The source is

```ts
declare const o3: { b: undefined | { c: string } };
>o3 : { b: undefined | { c: string; }; }      <- A19, and the port drops the `undefined`
```

### Located, and it is not where it looks

Six probes say the port prints this correctly:

```
declare const o: { b: undefined | { c: string } };  (o);  => { b: undefined | { c: string; }; }
declare const o: { b: { c: string } | undefined };  (o);  => { b: { c: string; } | undefined; }
declare const o: { b?: string };                    (o);  => { b?: string; }
```

Written order preserved, `undefined` kept, optional members unaffected. **The
failing positions are declaration NAME nodes** (A19 is the `o3` of
`declare const o3: …`), and the harness types *references*. So the reference road is
right and the declaration-name road is not — the same split §841 found for
`QualifiedName` and fixed for **+310**.

**Next probe, named:** `types_producer`'s declaration-name branch, which types a
name through `get_type_of_symbol` rather than `check_expression`. Compare the two for
this declaration and read which drops the constituent.

Sized: 33 lines in the three chain cases, of a 103-line group, and those cases have
**zero gaps** — so every line is a wrong-to-right conversion with no gap-to-wrong
exposure at all.

## §879: the same expression answers two different types in one run

§878 named the declaration-name road as the suspect. It is not that either, and what
the probe found instead is more interesting than the lead.

Ruled out first, each by measurement:

- **The reference road** — six harness probes print
  `declare const o: { b: undefined | { c: string } }` correctly, written order and
  all.
- **`strictNullChecks`** — the same six, run with the flag off, print identically.
- **A declaration-name branch in `types_producer`** — there is none; a variable's
  name falls to the ordinary identifier road.

So the producer was instrumented to check `o3` through `check_expression` at every
position it asks about, in the real case. The result:

```
3  879 o3 via check_expression => { b: { c: string; }; }
3  879 o3 via check_expression => { b: undefined | { c: string; }; }
```

**The same identifier, the same expression road, the same run — and two different
answers, three times each.** One of them is right and one drops the `undefined`
constituent of a member.

### What that means

This is not a missing rule; it is **ordering or caching**. This port stores a named
object type as `TypeData::Named { text, members }` with the printed text computed
**at creation** (ADR-0003's side tables, and the §800 note about
`object_literal_members` records the same hazard from the other side). Two interned
types with the same structure and different text can only come from the text being
built at two different moments — one of them before the member's union is complete.

That makes it a different class of defect from everything else in this session's
chain: §863–§876 were all rules, gates or categories, and each was fixed by making
one condition match upstream. This one cannot be fixed that way, because **both
answers come from the same code path**.

### Next probe, named

Find the two interned `TypeId`s behind those answers and compare their creation
sites — whichever builds `{ b: { c: string; }; }` is minting a member's text before
the member's type is fully resolved. `text` computed at creation is the design
(ADR-0003), so the fix is to establish *when* it is safe to compute it, not to
recompute it later.

Sized: the three chain cases hold 33 of the 103 missing-`undefined` lines and have
**zero gaps**, so the whole group is wrong-to-right with no gap exposure — but the
cause is shared with however many of the other 70 lines come from the same
early-minted text, which is unknown until the creation sites are compared.

## §880: §879's "two answers" was three cases, and the real shape is one line

§879 reported *"the same expression answers two different types in one run"* and
concluded ordering or caching. **That conclusion was wrong, and the correction is
mundane**: `TSR_FILTER=elementAccessChain` matches **three separate cases**, not one.

```
conformance/elementAccessChain     // @strict: true    104 RIGHT  27 WRONG
conformance/elementAccessChain.2   // @strict: false    28 RIGHT   3 WRONG
conformance/elementAccessChain.3   // @strict: true    143 RIGHT   0 WRONG
```

`TypeId(27) snc=false` is case `.2`, which declares `@strict: false`, and dropping
`undefined` there is **correct** — upstream does the same
(`checker.go:25783`: `if !c.strictNullChecks && flags&TypeFlagsNullable != 0 { … return }`),
and the port's `unions.rs` carries that anchor already. Two answers, two cases, no
caching defect. §879's "different class of defect" framing is withdrawn.

### The real shape, from the strict case alone

```
7  want (() => { c: { d?: { e: string; }; }; }) | undefined   got () => { … }
6  want string | undefined                                    got string
4  want { d?: { e: string; }; } | undefined                    got { d?: { e: string; }; }
4  want { c: string; } | undefined                             got { c: string; }
4  want { c: { d?: { e: string; }; }; } | undefined             got { c: { … }; }
2  want <T>() => undefined | ({ x: number; })                  got <T>() => { x: number; } | undefined
```

**Every line but the last two is `X | undefined` wanted and `X` given.** One
mechanism. (The last two are a written-ORDER difference — upstream preserves
`undefined | T` as written — and are a separate, smaller question.)

### Five hypotheses, five refutations, all cheap

| hypothesis | refuted by |
|---|---|
| the declaration-name road in `types_producer` | there is no such branch; a variable name takes the identifier road |
| `strictNullChecks` handling | six probes, strict and loose, print the declaration correctly |
| interning / caching order (§879) | two cases, not two answers |
| the optional-chain re-union | the failing lines include non-chain positions |
| the member lookup dropping `undefined` | `o["b"]`, `o.b`, `b?: T` and `b: T \| undefined` all print correctly |

**The minimal harness reproduces every isolated shape correctly**, which is the
finding that matters: the cause needs the whole file, not a fixture. That is a limit
this session has now hit three times (§839's `lib.d.ts`, §877's class bodies, and
here), and the answer is different each time — here it is not a missing harness
feature but a genuinely context-dependent defect.

**Next probe, named and different in kind:** instrument inside the conformance
pipeline on the real case — print the type at each `o1`/`o2`/`o3` chain position with
the declared type beside it — rather than trying to shrink the case to a fixture.
Shrinking has failed five times and each failure cost a build.

## §881: the in-pipeline probe — the re-union works for some accesses and not others

§880's named probe, run: instrument `types_producer` to print every element- and
property-access type as the pipeline asks for it, across all three
`elementAccessChain` cases.

```
22  element  => any                          11  property => any
 6  element  => { c: string; } | undefined     6  property => { e: string; } | undefined
 6  element  => string | undefined            5  property => string | undefined
 5  element  => string                         4  property => string
```

**Both answers occur, in the same files, on the same construct.** Some accesses
re-union `undefined` and some do not — which confirms §880's conclusion that the
defect is context-dependent, and rules out any "this road never does it" explanation.

It does **not** localise it. Distinguishing which access is which needs the probe to
carry the baseline position alongside the type, and `type_id_at_location_tracking`
does not receive it — the caller holds it. That is a larger instrumentation change
than this thread has earned.

### Stopping this thread, and why

§878–§881 spent four entries and six probe builds on a 27-line case. Each probe
refuted a hypothesis cheaply and correctly — that part of the method worked — but the
sequence produced **no landing**, and the last two refuted my own prior entries
(§879's caching theory, withdrawn in §880).

The honest read: **this defect is not shrinkable, and the instrument needed to chase
it further does not exist yet.** The chain of five refutations is recorded so nobody
repeats it, and the next step is stated as an instrumentation task rather than
another guess:

> Thread the baseline position through `type_id_at_location_tracking` so a probe can
> print `position → node kind → computed type → wanted type` in one line. Every
> investigation in §872–§881 wanted that and none had it.

Set against the same period, §863–§876 landed **+311 lines** from the *other* lens —
gates whose category or condition was narrower than upstream's. That lens is not
exhausted: the `_ => false` sweep found 78 candidate sites and four of four audited
so far paid.

## §882: a census that names one function — 90 lines differing only by `?`

The `_ => false` sweep was the plan; the census that came out of auditing
`property_is_optional` is worth more. Querying the baseline for WRONG lines whose
want and got differ **only in `?` placement**:

```
90 lines across 10 cases
  48  sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2
  18  shorthandPropertyAssignmentsInDestructuring_ES6
   7  contextuallyTypedIife          7  contextuallyTypedIifeStrict
   3  assignmentTypeNarrowing        2  classExpressionNames
```

One shape:

```
want { skills?: { primary?: string; secondary?: string; }; }
got  { skills?: { primary: string; secondary: string; }; }
```

Note the port gets the **outer** `skills?` right and the inner members wrong — so the
optionality rule is applied somewhere and not everywhere.

### The function it names

`getTypeFromObjectBindingPattern` (`checker.go:17938`): a binding element that
**writes a default** implies an **optional** member. This port has the rule for
*matching* an object literal against an implied pattern —
`implied_pattern_member_is_optional` (§489, `objects.rs`) — but not for **building a
type from a pattern**, which is the direction the corpus needs. `render_binding_pattern`
renders the pattern's *names* (`{ a, b }`), not its implied type, and nothing else in
the port answers this.

Ruled out on the way, each by reading rather than guessing:

- **`property_is_optional`** covers `PropertySignature`, `PropertyDeclaration`,
  `MethodSignature`, `MethodDeclaration` and not `ParameterDeclaration` — a real gap
  for `constructor(public a?: string)`, but **invisible in the corpus**, because a
  class type prints by name and optionality never reaches the text.
- **`padObjectLiteralType`** is the neighbouring mechanism and is *not* this one: it
  adds members that are **missing** from the initializer's type, whereas here the
  members exist and only the `?` is absent.

### Sized and specified

**90 lines, 10 cases**, one function whose upstream body is 24 lines
(`checker.go:16823-16847` for the padding sibling, `:17938` for this one), and the
optionality predicate it needs already exists in this port at
`objects.rs:1799`.

Not implemented here: it is a new type-construction function in the destructuring
area, and this session has already produced one unbalanced-brace build break from
hand-editing under fatigue. **The right shape for it is its own session opening with
this note**, and the bar is easy to write because the population is one query away.

### §882.1: the census's node is an assignment PATTERN, and three guesses were wrong

§882 filed this as `getTypeFromObjectBindingPattern`. Mapping the wrong positions to
assertions — which §882 should have done before naming a function — says otherwise.
The first wrong position is **109**, not the interface at 6:

```
A110  >{ skills: { primary: primaryA = "primary", … } = { primary: "none", … } }
        want { skills?: { primary?: string; secondary?: string; }; }
        got  { skills?: { primary:  string; secondary:  string; }; }
A111  >skills : { primary?: string; secondary?: string; }
```

It is a **destructuring assignment pattern** in a `for ({…} of …)` head, not a
binding pattern and not an interface.

Three things ruled out by reading, each of which §882 or I had assumed:

1. **The interface member is RIGHT.** `A7 >skills : { primary?: string; … }` is not
   in the wrong set, and four probes confirm nested written `?` prints correctly at
   top level, nested, doubled, and through a property access.
2. **`is_assignment_pattern_target` already handles this**, including the
   `ForInOrOfStatement` head (§451) and the `=` left-hand side — so
   `in_destructuring_pattern` is true for the inner literal too.
3. **`check_object_literal` already sets `member_optional`** when
   `in_destructuring_pattern` and the member's initializer is an `=` binary — which
   is why the port gets the **outer** `skills?` right.

So the optionality rule is present and fires. What differs is **which node supplies
the inner type**: the port's `skills` member types as the binary
`{PATTERN} = {DEFAULT}`, whose value is the DEFAULT's type
(`{ primary: string; secondary: string; }` — no defaults, so no `?`), where upstream
reports the PATTERN's type.

### The one open question, and the exact place to read it

`checkPropertyAssignment` (`checker.go:13673`) computes the member type as
`checkExpressionForMutableLocation(node.Initializer())`, and the initializer here is
that `=` binary. **Hypothesis, unverified:** for a destructuring assignment
upstream's binary road answers the LEFT pattern's type rather than the right's, which
would make `skills` the inner pattern's implied type and carry the inner `?`s.

That is stated as a hypothesis on purpose. I asserted three things in this thread
without reading them and all three were wrong; the next step is to read
`checkBinaryLikeExpression`'s `=` arm and `checkDestructuringAssignment`'s return
value, **and only then** write a bar.

Still 90 lines across 10 cases, still worth doing, and now pointed at the right
construct.

### §882.2: the hypothesis is refuted, and the thread closes undiagnosed

§882.1's hypothesis — *"for a destructuring assignment upstream's binary road answers
the LEFT pattern's type rather than the right's"* — is **wrong**, and reading it took
two greps:

```go
// checkBinaryLikeExpression (checker.go:12336)
if operator == ast.KindEqualsToken && (left.Kind == ast.KindObjectLiteralExpression || left.Kind == ast.KindArrayLiteralExpression) {
    return c.checkDestructuringAssignment(left, c.checkExpressionEx(right, checkMode), checkMode, …)
}

// checkObjectLiteralAssignment (checker.go:12585)
for i := range properties.Nodes { … }
return sourceType
```

`sourceType` is the **right**'s type. So upstream's `{PATTERN} = {DEFAULT}` expression
types as the default, exactly as this port does, and
`checkPropertyAssignment`'s `checkExpressionForMutableLocation(initializer)` therefore
sees the default's type in both compilers.

**Which leaves the baseline unexplained.** It wants
`{ skills?: { primary?: string; secondary?: string; }; }` where every function read
so far predicts `{ skills?: { primary: string; secondary: string; }; }`. Some path
not yet found supplies the inner `?`s — a candidate is the `.types` writer itself,
which has its own rules for what to print at a pattern position and has already
surprised this session twice (§841's `isRightSideOfQualifiedNameOrPropertyAccess`,
§858's apparent-type split).

### Closing the thread

Four hypotheses on this census, four refutations, no landing:

| | claimed | refuted by |
|---|---|---|
| §882 | `getTypeFromObjectBindingPattern` | the node is an assignment pattern, not a binding pattern |
| §882.1 (a) | the interface member is wrong | it is RIGHT; four probes agree |
| §882.1 (b) | `in_destructuring_pattern` is false for the inner | the walk handles `ForInOrOf` and the `=` left already |
| §882.1 (c) | upstream's `=` answers the left's type | it returns `sourceType`, the right's |

Every refutation was one or two greps. **The method is not the problem — the target
is.** This census looked like one function and is not, and the three rounds spent on
it produced corrections rather than lines.

**Recorded so the next session does not re-open it on the same reading.** If it is
re-opened, start at `type_symbol_baseline.go`'s handling of an assignment-pattern
position, not at the checker — every checker path here has now been read and none
of them explains the baseline.

## §883: the instrumentation §881 named, built — and it localised the row in one run

§881 stopped because *"telling the two apart needs the probe to carry the baseline
position alongside the type, and `type_id_at_location_tracking` does not receive
it"*, and filed threading it as the next step. The threading was unnecessary: the
information was already computed and thrown away.

`verdict.rs` splits both the wanted and the got line into `(expression, type)` to
decide whether a row is *aligned* — and then builds the row from the types only:

```rust
let (Some((we, wt)), Some((ge, gt))) = ( … ) else { continue };
if we != ge.as_str() { continue; }
let verdict = if gt == "error" { "GAP" } else { "WRONG" };
out.push(format!("{}:{index}:{position}\t{verdict}\t{wt}\t{gt}", case.name));
```

`we` is the expression. **One line puts it back**, behind `TSR_VERDICT_EXPR=1` so
`scorepair`'s tab-parsing sees the same four columns it always has (§827's
treatment). Verified: four columns without the variable, five with, and
`scorepair` reports no transitions.

### What it found, immediately

§881 could say only *"some accesses re-union `undefined` and some do not"*. With the
expression column, one run over `elementAccessChain`:

```
4  o2?.["b"]!              want { c: string; } | undefined
2  o5["b"]?.()             want … | undefined
2  o5["b"]?.()["c"]
2  o5.b?.()                2  o5.b?.()["c"]
```

Two families, both the same mechanism seen from different sides:

1. **An optional chain followed by `!`.** `o2?.["b"]!` keeps the chain's `undefined`
   upstream — the non-null assertion removes the *operand's* nullability, not the
   chain's, which is re-unioned at the chain's end. This port applies the strip and
   the chain's `undefined` never comes back.
2. **An optional CALL in a chain.** `o5.b?.()` and the element access on its result.
   `checker-notes-callres.md` §22 records the call road as re-unioning `undefined`
   "when anything was stripped" — so this is the same rule reached through the
   `?.()` spelling.

That is a named construct with a named upstream rule, which is what three rounds of
probing could not produce.

> **The lesson is about instruments, not about chains.** §872–§882 spent eight
> entries and a dozen probe builds guessing at which node was failing, and the
> answer was one discarded local variable in a file every one of those runs already
> executed. **When a probe keeps asking "which one?", check whether the instrument
> already computed the answer before asking a different question.**

## §884: the bar — two chain defects, both with a named upstream function

§883's expression column turned `conformance/{element,property}AccessChain` and
`callChain` into a readable board. 86 non-RIGHT rows; grouped by expression, three
families, and the sources
(`.../optionalChaining/elementAccessChain/elementAccessChain.ts`) name each one:

| n | expression | want | got |
|---|---|---|---|
| 12 | `o2?.b!`, `o2?.["b"]!`, and the accesses on them | `… \| undefined` | `…` |
| 13+ | `o5.b`, `o5.b?.()`, `o5["b"]?.()["c"]`, … | `… \| undefined` | `…` |
| 9 | `o3`, `o6` | `{ b: undefined \| { c: string; }; }` | `{ b: { c: string; }; }` |

**Family A** is `declare const o2: undefined | { b: { c: string } }; o2?.["b"]!`.
Upstream `checkNonNullAssertion` (`checker.go:10622`) is *two* branches:

```go
if node.Flags&ast.NodeFlagsOptionalChain != 0 {
    return c.checkNonNullChain(node)
}
return c.GetNonNullableType(c.checkExpression(node.Expression()))
```

and `checkNonNullChain` (`checker.go:10631`) strips the marker, takes the
non-nullable, and **re-unions through `propagateOptionalTypeMarker`**. This port has
only the second branch (`expressions.rs`, the `NonNullExpression` arm), so `!`
removes the chain's `undefined` and nothing puts it back. `!` binds the *operand's*
nullability; the chain's `undefined` is a property of the chain's end.

**Family B** is `declare const o5: { b?(): { c … } }` — an optional **method**.
`o5.b` alone wants `(() => …) | undefined`; every later row is downstream of that
one type. The arm is `getTypeOfFuncClassEnumModuleWorker`'s tail
(`checker.go:16930`), `strictNullChecks && symbol.Flags&SymbolFlagsOptional` →
`getOptionalType(t, true)`. **This port already documents that arm as unwritten with
an expired excuse** — `symbols.rs` says in as many words *"Its stated reason is gone
… only the excuse expired."* A refusal kept past its reason, which is the §863
shape again.

**Family C** is `o3`/`o6`: the *declaration's* printed type, `undefined | { c:
string; }` in source order with upstream's parentheses, against this port's
`{ c: string; }`. Different subsystem (printing / declared-union order), **not**
opened here.

### The bar

- **Primary.** A closes the 12 Family-A rows; B closes `o5.b` and the rows that
  follow from it.
- **Safety.** No adverse transition outside the optional-chain cases. A `!` on a
  non-chain operand (`x!`) is untouched, and `(a?.b)!` stays a non-chain — a
  parenthesis breaks the flag upstream, and this port's `expression_is_optional_chain`
  walker already stops at one.
- **Falsifier.** If A's rows stay shut, the `undefined` is being lost before the `!`
  (in the element access), not at it — and the read of `checkNonNullChain` was the
  wrong cause. If B moves `o5.b` but not `o5.b?.()`, the call road's `chain_stripped`
  is computed from something other than the callee's own type.
- **Regression.** Unit tests in `tsr-checker` for both, pinning `x!` unchanged.

Each lands separately so `scorepair` attributes the transitions.

## §884.1: Family A landed — 22/0, and the first build's six losses were the finding

The first build derived the chain flag the obvious way — *does the operand spine
contain `?.`* — and measured **22 W→R / 6 R→W**. The six were not noise. With
§883's expression column they read in one glance:

```
m?.[0]!         want string   got string | undefined
o2?.["b"]!.c!   want string   got string | undefined
o2?.b()!.toString!  want (radix?: number) => string  got … | undefined
```

Every one is a **trailing** `!`. Upstream's own baseline has both answers three
lines apart (`elementAccessChain.types:163`):

```text
>o2?.["b"]!.c! : string
>o2?.["b"]!    : { c: string; } | undefined
```

The same `!` spelling, the same operand, opposite answers. The cause is in the
**parser**, not the checker. `parser.go:5375` builds every `!` with
`ast.NodeFlagsNone`:

```go
expression = p.checkJSSyntax(p.finishNode(p.factory.NewNonNullExpression(expression, ast.NodeFlagsNone), pos))
```

and the flag is added *retroactively* by `tryReparseOptionalChain`
(`parser.go:5414`), which walks *down* a run of `!`s looking for a stamped chain
and, on finding one, stamps the run. It is called from exactly three places —
`parser.go:5399`, `5444`, `5468`: the property-access, element-access and call
rests. **So a `!` is a chain link precisely when another link was parsed on top of
it.** `o2?.["b"]!.c` — `.c` reparsed it. `o2?.["b"]!.c!` — nothing follows the
last `!`. `m?.[0]! && …` — `&&` is not one of the three rests.

`non_null_is_optional_chain` (`members.rs`) mirrors that: the downward half is the
existing spine walker; the upward half climbs a run of `!`s and asks whether the
first other ancestor is one of the three kinds *with this run as its expression*.

Second build: **22 W→R, 0 R→W.** Six regression tests in
`crates/tsr-checker/tests/non_null_chain.rs`, including the two the first build
broke.

### What this says about reading upstream

I read `checkNonNullAssertion`, saw `node.Flags&ast.NodeFlagsOptionalChain`, and
supplied the flag from the AST shape because this port derives rather than stores
it. That is the right instinct in general — §22, §5 and the whole chain family
derive the flag from the spine and are correct. **It is wrong for exactly one node
kind, and only because the parser is order-dependent there.** A derived flag is a
claim that the flag is a function of the finished tree; `tryReparseOptionalChain`
is the case where it is a function of the *parse order*, and the finished tree
records that only as "what the parent turned out to be".

> **The falsifier that fired.** The bar said: *if A's rows stay shut, the cause was
> wrong.* They opened. It did not anticipate rows opening **and** others closing —
> the losses were what carried the actual mechanism. Worth adding to the bar shape:
> a measured adverse set with a readable expression column is evidence, not just a
> cost.

Family B (the optional **method** `b?(): T`) is next, unlanded.

## §885: Family B — and §832's refusal, re-measured from −4 to +119

`declare const o5: { b?(): { c … } }`. Upstream gives an optional **method**
symbol its `| undefined` at the tail of `getTypeOfFuncClassEnumModuleWorker`
(`checker.go:16930`):

```go
if c.strictNullChecks && symbol.Flags&ast.SymbolFlagsOptional != 0 {
    return c.getOptionalType(t /*isProperty*/, true)
}
```

This function's own doc comment has carried the refusal for many sessions, and
already said the quiet part: *"Its stated reason is gone … The arm is still
unwritten — only the excuse expired."*

**Measured: 143 W→R, 24 adverse (15 R→W, 9 R→G), 1 W→G.** `right` 439752 →
439871, `wrong` 27692 → 27563, `gap` 6799 → 6809. `conformance/optionalMethods`
26, `elementAccessChain` 15, `methodSignaturesWithOverloads2` 12.

### §832 refused exactly this and measured −4

§832 built the identical placement, measured **28 gained against 32 right lines
lost**, and refused it. The refusal's reasoning was:

> The correct placement is the property-ACCESS road, which is also where upstream
> puts it and is why an optional CHAIN can strip it again.

**That reasoning was wrong.** Upstream puts it on the *symbol's type* — for a
method at `checker.go:16930`, for a property through `addOptionality` — and
nowhere on the access road. The refusal was right about the *number* at the time
and wrong about the *reason*, and only the number changed since. A refusal
recorded with its number is re-testable; a refusal recorded only as a reason would
have stayed shut, because its reason still sounds plausible.

> **This is the second refusal this session to reopen on re-measurement** (§870,
> §876 were narrowed gates; §832 is a whole change). The board's refusals carry
> numbers *precisely* so a later checker can re-run them, and two hits suggests
> doing that sweep deliberately rather than by accident.

### The predicate is `PostfixToken`, not `HasQuestionToken`

This binder declares `SymbolFlags::OPTIONAL` and sets it nowhere, so the flag had
to be recovered from the declarations. The faithful predicate is
`getOptionalSymbolFlagForNode` (`binder.go:2727`) — `node.PostfixToken()` — not
`is_optional_declaration`, which is `ast.HasQuestionToken` and additionally counts
a parameter's `?`. Both were built and **measured identically** (143/24), so the
parameter difference is unobservable on this corpus; the postfix form is kept
because it is what upstream computes, not because it scored better. Saying
otherwise would credit the correction with a gain it did not produce.

### The 24 adverse are a *second* defect, named

`compiler/assignmentCompatBug2` declares `{ …; k?(a: any): any }` and assigns an
object literal to it. `k`'s contextual type is now
`((a: any) => any) | undefined`, and `contextual.rs` answers `None` for a union —
a refusal its own doc comment states:

> Upstream's union handling (`getContextualSignature`'s
> `compareSignaturesIdentical` loop) is not ported: a union-typed context answers
> `None`, because picking one member's signature is a guess.

It is not a guess. `getContextualSignature` (`checker.go:10264`) iterates the
constituents, and `undefined` contributes no call signature, so exactly one
survives and upstream uses it. §886 ports that; this entry lands with the adverse
measured and attributed rather than hidden inside a combined number.

## §886: the decidable half of `getContextualSignature`'s union arm

§885's 24 adverse rows all came from one refusal, and `contextual.rs` stated it
plainly:

> Upstream's union handling (`getContextualSignature`'s
> `compareSignaturesIdentical` loop) is not ported: a union-typed context answers
> `None`, because picking one member's signature is a guess and building the
> combined one needs `createUnionSignature`.

**Picking is not a guess when nothing else is on offer.**
`getContextualSignature` (`checker.go:10272`) iterates the constituents and calls
`getContextualCallSignature` on each; `undefined` has no call signature, so an
optional member's `((a: any) => any) | undefined` leaves exactly one. The
refusal conflated two cases that upstream keeps apart:

| constituents yielding a signature | upstream | ported? |
|---|---|---|
| 0 | `nil` | yes |
| 1 | that signature | **yes, §886** |
| 2+ | `compareSignaturesIdentical`, then `createUnionSignature` | no — still refused |

The 2+ half stays shut with its real reason: telling "identical, so combine" from
"different, so `nil`" needs `compareSignaturesIdentical`, and the combination
needs a signature whose return type is the union of the members'. Neither exists
here, and answering with an arbitrary member would be exactly the guess the old
comment described. The old comment's mistake was applying that reason to the
one-signature case, where it does not hold.

**Measured: 15 W→R, 10 G→R, against 6 G→W and 2 W→G.** `right` 439871 → 439896,
`wrong` 27563 → 27552, `gap` 6809 → 6795. `assignmentCompatBug2` (8) and
`objectLitGetterSetter` (4) — §885's two largest adverse cases — are recovered in
full. The 6 `GAP→WRONG` are in `contextualTypingOfOptionalMembers` and are §620's
accepted direction: a decline replaced by a computed answer that is not yet right.

Across §885 + §886 the pair is **+144 right, −140 wrong, −4 gap**.

> **The shape, for the third time this session.** A refusal whose *stated reason*
> covers a strictly narrower case than the refusal itself. §863, §870, §875, §876
> were gates whose category was too narrow; §832 was a change refused on a wrong
> reason and a real number; this is a reason that is sound for 2+ constituents and
> was applied to 1. Each time the fix was to read the upstream function and notice
> it branches where the port does not.

Three cases from §885 are not recovered and stay on the board:
`conformance/unionTypeReduction2` (4), `conformance/controlFlowSuperPropertyAccess`
(3), `compiler/interfaceClassMerging` (1).

## §887: the bar — an array literal spread into a call is in tuple context

### First, a refusal, measured and recorded

`compiler/parsingDeepParenthensizedExpression` heads the "want `error`, got
something" board with **136 of that family's 153 rows**. Its baseline records
`>T : error` inside a ~40-deep parenthesised assignment chain and `>T : any` for
the *same variable* elsewhere in the same file. Nothing about `T` differs; what
differs is the depth. **Upstream is hitting its own complexity limit and printing
`errorType` where it gives up.** Matching it would mean reproducing a limit, not
porting a rule, and the limit's exact threshold is not a documented interface.
**REFUSED by §887 at 136 lines.** Reopen only if upstream's depth cutoff turns out
to be a named constant this port can read. (Also: that baseline has
megabyte-long lines — read it with `grep -c`, never `sed -n`.)

### The array-literal board

Grouping every WRONG row by the SHAPE of §883's expression column, then by the
want/got relationship, over the whole corpus:

```
16192  got `any`          (across 1730 cases, top case 221 — no seam, diffuse)
  841  array literal      of which 412 are "want TUPLE, got array"
```

The `any` bucket is 59% of all wrong lines and has no dominant cause; saying so is
the finding. The array-literal bucket does have one:

| want | got | expression |
|---|---|---|
| `[symbol, false]` | `(symbol \| boolean)[]` | `[s, false]` |
| `[0]` | `number[]` | `[0]` |
| `[number, true]` | `(number \| boolean)[]` | `[1, true]` |
| `[]` | `never[]` | `[]` |

Upstream decides this in one line (`checker.go:8029`):

```go
inTupleContext := isSpreadIntoCallOrNew(node) || contextualType != nil && someType(contextualType, func(t *Type) bool {
    return c.isTupleLikeType(t) || …
})
```

**One question: is the contextual type tuple-like?** This port's
`array_literal_tuple_context_kind` instead enumerates **seven parent kinds** —
type assertion, `as`, variable declaration, assignment target, call argument
(twice), return statement — each re-deriving an annotation by hand, ending in
`_ => TupleContext::No`. It is the session's recurring shape at its largest: a
gate whose *category list* stands in for a question upstream asks generically.

**`isSpreadIntoCallOrNew` is absent entirely**, and it is three lines upstream:

```go
parent := ast.WalkUpParenthesizedExpressions(node.Parent)
return ast.IsSpreadElement(parent) && ast.IsCallOrNewExpression(parent.Parent)
```

`conformance/arraySpreadInCall` — `f(...[1, 2])` — is the array-literal board's
**top case at 24 rows**.

### The bar

- **Primary.** `conformance/arraySpreadInCall`'s 24 rows close.
- **Safety.** The arm is purely additive: it is consulted only where the seven-arm
  list already answered `No`, so a row it does not touch cannot move.
- **Falsifier.** If `arraySpreadInCall` stays shut, tuple *context* is not what the
  case lacks — the spread element's own type is — and the generic contextual
  fallback below would be equally pointless.
- **Regression.** A unit test for `f(...[1, 2])` and one for `f([1, 2])` (which
  must keep its existing answer).

The generic `get_contextual_type` fallback is the larger half and lands separately
if at all: this port's contextual road is itself partial ("three of its twenty
arms are here"), so whether it subsumes the seven hand-rolled arms is a
measurement, not a reading.

## §887.1: the spread arm landed — +59, zero adverse, and a pinned decline lifted

`isSpreadIntoCallOrNew` ported as written. **59 W→R, zero adverse.**
`conformance/arraySpreadInCall` 37, `conformance/callChain` 12,
`conformance/typeParameterConstModifiers` 6. The bar's primary leg closes and the
safety leg holds exactly as predicted — the arm is consulted only where the
seven-arm list already answered `No`, so nothing it does not touch can move.

A unit test in `tests/contextual.rs` had pinned the old answer as a decline:

```rust
// A SPREAD argument still declines — the variadic legs are not ported.
assert_eq!(type_of("((...sigma) => sigma)(...[5, 6]);", "sigma"), "any");
```

two lines below the positional form asserting `[number, number]`. The spread form
now gives the same answer, which is what it should always have been — the decline
was the array literal widening to `number[]` before the rest parameter ever saw
it. **A pinned decline sitting beside the working case is the cheapest possible
signal, and it went unread for however many sessions it has been there.** The
adjacent assertion was the specification.

### On the seven-arm list

The arm ported here is the disjunct that reads no contextual type. The other
disjunct — `contextualType != nil && someType(contextualType, isTupleLikeType)` —
is what the seven hand-rolled parent-kind arms approximate one parent at a time.
Replacing them with one `get_contextual_type` question is the larger, riskier
half and is **not** attempted here: this port's contextual road covers "three of
its twenty arms", so whether it subsumes the seven is a measurement. It is now the
best-sized remaining item on the array-literal board (412 rows want a tuple; 59 of
them just closed).

## §888: the seven-arm list gets the question it was approximating — +60, zero regressions

§887 ported `inTupleContext`'s first disjunct. This is the second, the one the
seven hand-rolled parent-kind arms exist to approximate:

```go
contextualType != nil && someType(contextualType, func(t *Type) bool {
    return c.isTupleLikeType(t) || …
})
```

One line: **ask `get_contextual_type` and test tuple-likeness.** That road
dispatches on the parent too, but covers a *strict superset* of the seven —
adding `SatisfiesExpression`, `PropertyDeclaration`, `NewExpression`,
`ConditionalExpression`, `PropertyAssignment`, `ParenthesizedExpression`, an
enclosing `ArrayLiteralExpression`, and an arrow's expression body.

**Measured: 60 W→R against one W→G. Zero `RIGHT→` of either kind.**
`conformance/genericCallWithTupleType` 18, `destructuringParameterDeclaration1ES5`
4, `…ES5iterable` 4. `right` 439955 → 440015.

### Why it was added behind the seven arms rather than replacing them

Not caution for its own sake. The two roads **disagree about which type a shared
parent kind yields**: the hand-rolled arms read the written annotation *node*
(`get_type_from_type_node`), while `get_contextual_type` may answer an inferred or
instantiated type for the same position. Replacing would put those disagreements
in play simultaneously with the new coverage, and a mixed measurement cannot be
attributed. Consulted only where the list said `No`, the arm can only widen, and
the 60/0/1 reading is unambiguous.

Deleting the seven arms is now a **separable** follow-up with its own falsifier:
it should measure zero if the two roads agree wherever both answer, and whatever
it does measure is exactly the disagreement, isolated. That is a better experiment
than it would have been bundled here.

### What stays shut

Upstream's `someType` maps over a **union's** constituents; this arm tests the
contextual type whole, so `[number, string] | undefined` — precisely the shape
§885 now mints for an optional member — is still not tuple context. The
`isGenericMappedType` half of the predicate is also unported. Both are named here
rather than discovered later.

Array-literal board after §887 + §888: **412 "want TUPLE, got array" rows, 119 of
them closed.**

## §889: `someType` maps over a union — the leg §888 named, closed

§888 named it: *"upstream's `someType` maps over a union's constituents; this arm
tests the contextual type whole, so `[number, string] | undefined` — precisely the
shape §885 now mints for an optional member — is still not tuple context."*

Ported. **3 W→R, zero adverse** — `typeInferenceLiteralUnion`,
`unionOfArraysFilterCall`, `unionsOfTupleTypes1`, one row each. `right` 440015 →
440018.

Three lines is a fair return for three lines of code, and the point of recording
it is not the number. **The leg was closed because §888 wrote down what it had
left open, in the same commit, with the shape named.** The alternative — leaving
it to be rediscovered by a future census — is how §872–§882 spent eight entries.

`isGenericMappedType`, the other half of upstream's predicate, is still unported
and now the only part of `inTupleContext` that is.

## §890: the bar — `checkExpressionForMutableLocation` has three branches and the port wrote one

Continuing §887's corpus-wide census. Classifying every WRONG row by whether the
port's answer is the **oracle's answer with its literal types widened**
(`"hour"`→`string`, `true`→`boolean`, `1`→`number`):

```
1239 rows across 207 cases — the port widened a literal the oracle kept
   161  compiler/temporal            45  compiler/staticFieldWithInterfaceContext
    47  compiler/reverseMappedType…   40  compiler/excessPropertyCheck…
    30  conformance/typeParameterConstModifiers
```

**Larger than the whole array-literal tuple family** (412), and one concept.

`objects.rs` names the cause in its own doc comment, which is why finding it took
a census and not an investigation:

> `getWidenedLiteralLikeTypeForContextualType(t, nil)` reduces to
> `getRegularTypeOfLiteralType(getWidenedLiteralType(t))` … when there is **no
> contextual type**.

Upstream (`checker.go:13878`):

```go
switch {
case c.isConstContext(node):   return c.getRegularTypeOfLiteralType(t)
case isTypeAssertion(node):    return t
default:                       return c.getWidenedLiteralLikeTypeForContextualType(t,
    c.instantiateContextualType(c.getContextualType(node, ContextFlagsNone), node, ContextFlagsNone))
}
```

Three branches. **This port wrote the third with `nil` hardcoded**, so it widens
unconditionally. `{ largestUnit: "hour" }` passed where the parameter is
`{ largestUnit: "hour" | "minute" }` records `{ largestUnit: "hour"; }` upstream
and `{ largestUnit: string; }` here.

### Every piece already exists

- `is_const_context` — **written**, and used by `array_literals.rs`, never here.
- `is_literal_of_contextual_type` — **written**, tri-state, in `signatures.rs`,
  faithful down to `core.Some`'s short-circuit.
- `get_contextual_type` — written, and made `pub(crate)` by §888.

Only the wiring is missing. That is now the sixth instance this session of *a
capability present and a caller that does not consult it*.

### The bar

- **Primary.** `compiler/temporal`'s literal rows close; `typeParameterConstModifiers`
  moves, since a `const` type parameter is precisely a literal-keeping context.
- **Safety.** No `RIGHT→` of either kind. Each branch only *keeps* a literal the
  port currently widens, so a row whose want is the widened type must not move —
  if one does, the contextual type being consulted is not the one upstream passes.
- **Falsifier.** If `temporal` stays shut, its literals come from
  `instantiateContextualType` (unported) rather than from the contextual type as
  this port computes it, and the branch is correct but starved.
- **Regression.** Tests for all three branches and for a control that must still
  widen.

**Undecidable (`None`) is treated as `false` — widen, i.e. today's behaviour.**
The tri-state's other callers keep a gap instead, but there is no gap to keep at
this position; declining here would print `error` where a widened literal is at
worst a near miss. Recorded so the alternative stays visible.

`instantiateContextualType` is **not** ported; the raw contextual type is passed.
Named now rather than discovered later (§889's lesson).

## §890.1: two of the three branches land at 71:2 — and the third is split off, measured

Built as specified. **The full change measured 151 W→R against 26 R→W**, and the
bar's safety leg said *no `RIGHT→` of either kind*. It was not honoured, so it is
reported rather than quietly relaxed.

### The 26, read

`compiler/thislessFunctionsNotContextSensitive2` supplied 22 of them, all of the
shape `tag : string → any`, `value : number → any`, `bbb : () => void → any` —
rows that were RIGHT collapsing to `any`.

```ts
const result1 = defineOptions({          // defineOptions<Context, Data>
  context: { tag: "A", value: 1 },
  produce() { return 42; },
});
```

**The returned value is not the problem.** `get_contextual_type` on a member
inside an argument whose signature is being resolved re-enters
`contextual_type_for_argument`, whose §469 sentinel answers
`Some(intrinsics.any)` (`contextual.rs:930`) — and `is_literal_of_contextual_type("A", any)`
is `Some(false)`, so the member widens to `string` exactly as before. The damage
is the **side effect**: §890 introduces a *new, earlier entry point* into
signature resolution, and what that resolution memoises while the object literal
is half-checked is read by the outer pass. **This port's contextual resolution is
order-dependent, and the census change exposed it.**

A first attempt short-circuited the contextual lookup for candidates carrying no
literal-flavoured constituent — answer-preserving, since `Some(true)` is reachable
only through a `maybe_type_of_kind(candidate, …literal…)` conjunct. It recovered
**one** row of 26. Kept anyway: it is free and it is correct.

### The split, and what it measures

Excluding members under a call or `new`:

| scope | W→R | R→W |
|---|---|---|
| all positions | 151 | 26 |
| **excluding call/new arguments** | **71** | **2** |
| (the difference: call arguments) | 80 | 24 |

**The 71:2 half landed.** The exclusion is **not upstream-faithful** — upstream has
no such condition — and saying otherwise would be the worst kind of quiet. It is a
deliberate scoping so that the order-dependent half carries its own number instead
of being averaged into this one.

The remaining 2 are `conformance/declarationsAndAssignments`:
`[a = 1, b = "abc"] = [2, "def"]`, where the RHS now keeps `[2, "def"]`. The
contextual type comes from the destructuring TARGET, whose element types this port
computes as the defaults' literal types where upstream widens them to
`[number, string]`. **A second defect in the target, surfaced — not a defect in
this branch.**

### Named prerequisite for the other half

The call-argument leg is worth **+57 net** and is blocked on one thing: making
contextual/signature resolution order-independent, so that asking for a contextual
type earlier cannot change what a later pass reads. That is the falsifier too — if
the leg is re-measured after such a change and still shows ~24 R→W, the cause was
never re-entrancy.

### §890.2: the order-dependence, named exactly

The prerequisite in §890.1 can be stated more precisely than "resolution is
order-dependent", because `contextual.rs` says it outright at line ~979, inside
`contextual_type_for_argument_resolving`:

> This road fires **only when NO memo exists**, i.e. no pass-1 candidates were
> collected for this call, so the fill is total: `someGenerics6(n => n, …)` wants
> `(n: unknown) => unknown`, not the adopted `(n: A) => A`.

**A branch selected by whether a memo has been populated yet.** §890 asks for a
contextual type at a *new, earlier* moment — while the object literal is being
checked, before pass-1 has collected candidates for the enclosing call — so this
road takes the fixing/`unknown` leg, and what it leaves behind is what the later
pass reads. That is the whole of the 24.

It is a correct implementation of upstream's fixing mapper *given* upstream's call
order, and this port now has a second caller with a different order. The fix is
not to remove the memo test but to make the contextual query **not** count as
pass-1 — i.e. distinguish "no candidates were collected" from "candidates have not
been collected yet". That is the shape of the follow-up, and it is a
one-distinction change rather than a re-architecture, which is worth knowing
before anyone budgets for it.

## §891: two guards built for §890's remaining half, both fail — and §890.2 was wrong

§890.1 sized the call-argument leg at **+57 net** and §890.2 named the blocker as
the memo-selected branch in `contextual_type_for_argument_resolving` — *"fires
only when NO memo exists"*. Two guards were built against that diagnosis.

### Guard 1: a probe flag that reads the memo and refuses to drive inference

A `contextual_literal_probe` on the checker, set around §890's contextual query,
making the argument road return `None` after its memo read rather than running the
fixing mapper.

**Measured 80 W→R / 23 R→W on top of §890's landed state — i.e. 151/25 from the
same base as §890's unsplit build, which measured 151/26.** The one-row difference
is the literal-flavour short-circuit, present in both. **The flag is inert.**

**So §890.2's diagnosis was wrong.** The fixing mapper is not what damages those
rows, and the entry recorded it with more confidence than the evidence carried: it
reasoned from a comment in the code to a mechanism without testing that the
mechanism was the one firing. Corrected here rather than left standing.

### What the case actually shows

Dumping `thislessFunctionsNotContextSensitive2` in position order is decisive, and
is what should have been done before writing §890.2:

```
:100  ok  { tag: "F", value: 6 } : { tag: string; value: number; }   ← the LITERAL is right
:101  XX  tag        want string   got any                           ← the property NAME is not
:102  ok  "F" : "F"
```

**The object literal's own type is correct and the property's is not.** The
baseline records a property-name position from the *symbol's* type, not by reading
the literal, so the damage is in `get_type_of_symbol` for that property — a
re-entry into the symbol's own type resolution, not into the call's signature.

### Guard 2: refuse the probe while any resolution is active

`resolutions.is_active()` gating branch 3. **Measured 42 W→R / 22 R→W — worse than
the landed version**, and 6 of the 22 are `arrayBestCommonTypes` rows that §890
had *won*. A symbol-type resolution is active at plenty of positions where branch
3 is both safe and needed, so the guard buys the 22 back by giving up more.

**Both guards reverted by file.** §890's landed 71:2 stands unchanged
(`no transitions vs baseline` after the revert).

### What the next attempt needs

Not another guard on a proxy for the recursion. The re-entry is specifically
*this property symbol's type* being asked for while it is being computed, so the
guard belongs at that identity — skip branch 3 when the contextual query would
reach the symbol currently on the resolution stack — and this port's resolution
stack is keyed by `SymbolId`, so the test is available. What is not yet available
is the link from "the node whose contextual type we want" to "the symbol that
would be asked for", which is the piece to build first.

**Cost of this entry: two builds, two full scorepairs, zero lines, one corrected
diagnosis.** Recorded because §890.2 is in the repository claiming a cause, and a
wrong cause left standing is worse than no cause.

## §892: object-literal member types belong on the symbol — and the cache is not a no-op

§891 isolated the mechanism behind §890's excluded half: a property-NAME position
is recorded from the **symbol's** type, and `get_type_of_symbol` for an
object-literal property **recomputes** the member instead of reading what
`check_object_literal` just computed. Upstream does not: `checkObjectLiteral`
writes `links.resolvedType` and `getTypeOfSymbol` reads it.

`property_node_id` was already in scope in the member loop and `binder.symbol_of`
already existed, so the change is one `entry().or_insert()`.

### It was expected to be a no-op. It is not.

The bar said caching a value the recompute would have produced anyway must measure
zero, and that a non-zero reading would mean the two roads already disagree.

**Measured: 41 W→R against 2 R→W.** `conformance/typeParameterConstModifiers` 18,
`conformance/jsdocTemplateTag6` 16, `compiler/objectFreeze` 4. **The two roads did
disagree, and the literal's own computation is the more often correct one** — which
is the answer upstream also keeps.

The 2 adverse are `compiler/objectFreeze`, where the oracle wants `any` and the
port now answers `string`: **the port being more specific than upstream**, in a
case that gained 4 on the same change.

### The prediction that failed

§892 was expected to retire §890's call-argument exclusion, since with no
recompute there would be no second entry. Removing it on top of §892 measures
**122 W→R / 28 R→W — with the same 22 `thislessFunctionsNotContextSensitive2`
rows §890 declined.** The cache does not help there and cannot:

> The probe for member `x` runs **while** `x`'s type is being computed, so a cache
> written **after** that computation cannot be read by it.

Circular by construction. The recompute was never the only entry — §891's "next
attempt needs the guard at the symbol's identity" was still describing the wrong
entry point. The exclusion stays, now with a reason that has survived a test
instead of one that merely sounded right.

**Three entries (§890.2, §891, §892) have now each named a different cause for the
same 22 rows, and the first two were wrong.** What distinguishes this one is that
it made a falsifiable prediction and the prediction failed cleanly, which is worth
more than the two that were merely plausible.

## §893: the bar — a binding-pattern element with a DEFAULT is an optional member

Sweeping the refreshed census for narrow want/got shapes turns up five small
families; the largest is **75 rows across 9 cases where the oracle wants an
OPTIONAL member and the port answers a required one**
(`sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2` 36,
`shorthandPropertyAssignmentsInDestructuring_ES6` 18).

```
{ primary: "none", secondary: … }   want { primary?: string; secondary?: string; }
                                    got  { primary: string; secondary: string; }
```

Upstream's `getTypeFromObjectBindingPattern` is one line (`checker.go:17938`):

```go
flags := ast.SymbolFlagsProperty | core.IfElse(e.Initializer() != nil, ast.SymbolFlagsOptional, 0)
```

and the member's type comes from `getTypeFromBindingElement`, whose first branch
is the initializer's widened type.

§429's arm in `symbols.rs` requires **every** element to satisfy
`element.initializer.is_none()`, and says so in its own comment — *"defaults,
rests and nested patterns keep the implicit any"* — so a pattern with any default
declines whole. Another gate standing in for a rule it is narrower than, which is
the shape this session has now hit seven times.

### The bar

- **Primary.** The two named cases close.
- **Safety.** The change only *admits* patterns the gate previously refused, so a
  row that has an answer today cannot move. Any adverse must therefore be
  `GAP→`, §620's accepted direction — and if a `RIGHT→` appears, the relaxation
  reached further than intended and comes straight back out.
- **Falsifier.** If the cases stay shut, the defect is the member's TYPE or its
  optional rendering, not the gate, and the remaining work is in
  `getTypeFromBindingElement` rather than in the admission test.
- **Regression.** Tests for a defaulted member, a plain one beside it, and the
  array pattern that stays refused.

**Scoped to OBJECT patterns.** An array pattern's defaulted element has its own
upstream answer (the element type from the initializer, not `any`), and the
existing arm fills `vec![any; len]`; admitting defaults there without also
computing the element types would mint a confident wrong tuple. Named rather than
bundled.

## §893.1: landed at +39 with zero `RIGHT→` — and the primary leg failed

`getTypeFromObjectBindingPattern`'s optionality line ported, scoped to object
patterns, with the initializer read **syntactically**.

### The stack overflow that set the shape

The first build called `check_expression` on the initializer, as upstream's
`checkDeclarationInitializer` does. It **overflows the stack**: a defaulted
parameter's initializer is checked with the parameter's own contextual type,
which is the implied type being computed. Upstream is re-entrant there; this port
is not. The initializer is therefore read off its literal form — `= "none"`,
`= 1`, `= true`, which is what the corpus holds — and anything else keeps §429's
`any`. A real limitation, stated rather than hidden behind a passing number.

### The two roads had to move together

First measurement: **+19** (14 W→R, 5 G→R, 6 G→W). Only the *printed* member had
changed; reading the binding still answered `any`, because `pattern_implied_members`
stored **names only**. Its doc said why:

> Only the names are stored: every member of a pattern-implied object is `any` by
> construction … with no initializer to infer from.

§893 admits elements that *do* have an initializer, so that reasoning expired with
it — a second stale refusal-reason found the same way §885 found the first.
Carrying the type in the table alongside the name took the measurement to
**+39 (32 W→R, 7 G→R) against 4 G→W, zero `RIGHT→`**. The safety leg holds
exactly as the bar predicted: the change only admits patterns that previously
declined, so `GAP→` is its only possible direction.

**The gain from consistency alone is +20 of the +39** — more than the printed
change was worth by itself. §56's rule ("the print road moves WITH the symbol
road") is not bookkeeping.

### The primary leg failed, and that is the finding to carry

The bar named `sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2`
(36 rows) and `shorthandPropertyAssignmentsInDestructuring_ES6` (18). **Neither
moved.** The 75 rows that motivated the change are a *different* road — a
destructuring assignment's implied type, not a parameter's binding pattern — and
the 39 came from `destructuringWithLiteralInitializers`,
`declarationsAndAssignments` and `contextuallyTypedParametersWithInitializers1`
instead.

That is now the **sixth** time this session a bar's named case stayed shut while
the change paid elsewhere (§844, §846, §858, §861, §875, §893). The pattern is
stable enough to state as a rule: **a case surfaces in a census because it has two
defects, so the one you can name is rarely the one that closes it.** The census
picks the target; it does not pick the fix.

## §894: renamed elements too (+10, zero adverse) — and what still blocks the 75

Reading the case §893's bar named
(`sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2`) with
the expression column shows exactly two remaining blockers:

```
{ primary: primaryA = "none", secondary: secondaryA = "none" }   ← RENAMED elements
{ skills: { primary: … } }                                        ← NESTED pattern
```

**Renamed elements ported.** §429 excluded them for a reason its code makes
plain: it read `element.name` for both the member's name and the binding's, and
for `{ primary: p }` those differ — the member is `primary`, the binding is `p`.
Admitting them means keying the member (and §565's side table) by the *property*
name while the local identifier stays the binding. **+7 W→R, +3 G→R, zero
adverse.**

**Nested patterns stay refused, and this is the named blocker for the 75.** A
nested element's member type is the inner pattern's own implied type, i.e. this
builder applied recursively — and the builder is written inline inside
`get_type_of_symbol`'s body, keyed off a `ParameterDeclaration`, with no form that
takes a pattern and returns a type. **Extracting it is the prerequisite**, and it
is a refactor rather than an arm, which is why it is recorded here rather than
attempted at the end of a long thread.

Its falsifier is cheap once extracted: the 36 rows of that case should close, and
if they do not, the remaining defect is the *outer* member's optionality
(`skills?`), which comes from a `= {}` default on the pattern itself rather than
from any element.

### The thread's arithmetic

§893 + §894: **+49 right, 4 `GAP→WRONG`, zero `RIGHT→`.** The 75 rows that started
it are still open, and every line gained came from somewhere else.

## §895: the builder becomes a method and recurses — +21, and the 75 rows were never on this road

§894 named the prerequisite: *"a nested element's member type is the inner
pattern's own implied type … and the builder is written inline inside
`get_type_of_symbol`'s body."* Extracted to
`Checker::object_pattern_implied_type`, which calls itself for a pattern-named
element — upstream's `getTypeFromBindingElement` → `getTypeFromBindingPattern`
recursion.

Making it a method also moved the per-element validity test **out of the gate and
into the builder**, where a nested pattern can decline on its own behalf. The gate
no longer has to predict what the builder can type; it asks.

**+11 W→R, +10 G→R, zero adverse.** A nested ARRAY pattern still declines — its
implied type is a tuple whose element types this arm does not compute — pinned by
a test.

### The falsifier resolved: the 75 rows were never on this road

§893's bar aimed at `sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2`,
§893.1 recorded that it did not move, and §894 predicted nesting was the blocker.
**It was not.** The source settles it:

```ts
for ({
    skills: {
        primary: primaryA = "primary",
        secondary: secondaryA = "secondary"
    } = { primary: "none", secondary: "none" }
} = multiRobot, i = 0; i < 1; i++) {
```

That is a **destructuring assignment**, not a declaration. Its nodes are
`ObjectLiteralExpression` and `PropertyAssignment`; there is no
`ObjectBindingPattern` and no `BindingElement` anywhere in it. Three entries aimed
at a road the rows do not travel.

**The census matched on the printed TYPE shape** — *"want an optional member, got
a required one"* — and two unrelated roads produce that same shape. A census
groups answers, and an answer's shape is not its cause; §800's *"the corpus is the
arbiter of VALUE, not of CORRECTNESS"* has a companion: **the corpus is the
arbiter of SIZE, not of LOCATION.** Confirming the road before sizing the work
costs one `grep` of the case source, and not doing it cost three entries here.

What did pay: `destructuringWithLiteralInitializers`, `declarationsAndAssignments`,
`contextuallyTypedParametersWithInitializers1`,
`destructuringParameterDeclaration1ES5/ES6`, `arrowFunctionExpressions`,
`emitArrowFunctionES6` — all genuine binding patterns.

**§893 + §894 + §895: +70 right, 4 `GAP→WRONG`, zero `RIGHT→`.**

## §896: the 75 rows, finally located — an ASSIGNMENT pattern as a contextual type

Having confirmed the road (§895), the case's residue resolves completely. Its
outer member is already right:

```
{ skills: { primary: primaryA = "primary" } = { primary: "none" } } = multiRobot
                                  ↑ want { skills?: { primary?: string; … } }
                                    got  { skills?: { primary: string; … } }
```

`skills?` **is** optional here — §365's `inDestructuringPattern && hasDefaultValue`
works, and the member's optionality proves the assignment-target predicate climbs
nesting correctly. What is wrong is the member's *type*.

The member's initializer is the BinaryExpression `{…} = {…}`, and an assignment
yields its **right-hand** type (`binary.rs`, upstream's own rule). So the member's
type is the type of the RHS literal `{ primary: "none", secondary: "none" }` —
which is row 4 of the residue, wanting `{ primary?: string; secondary?: string; }`
and getting required members.

Upstream gets that from `contextualTypeHasPattern` (`checker.go:13252`): the RHS
literal is contextually typed by the **LHS assignment pattern**, whose defaulted
properties are optional, and each property copies
`impliedProp.Flags & SymbolFlagsOptional`.

**§489 ported exactly this copy — for a BINDING pattern only.** Its
`contextual_binding_pattern` finds the pattern syntactically as *"the initializer
of a variable declaration whose name is such a pattern"*, or a property value
inside one. An assignment pattern is neither: it is an `ObjectLiteralExpression`
on the left of `=`, which that search does not look for.

**The whole of the 75 rows is §489's search, extended to find an assignment
pattern.** The optionality copy, the member loop and the `hasDefaultValue` test are
all already written and all already correct.

Not attempted here: the contextual type is the LHS pattern's own literal type, so
the extension has to type the LHS while checking the RHS of the same assignment —
the same re-entrancy question §890–§892 spent three entries on, and it deserves
its own bar rather than the tail of this one.

### The thread, in full

Four entries (§893–§896) aimed at 75 rows. **They closed zero of them** and
landed **+70 elsewhere**, all from genuine binding patterns. The rows are now
located precisely for the first time, which is what §896 is: a location, not a
change.

## §897: the bar — §489's optionality copy, for an ASSIGNMENT pattern

§896 located the 75 rows: they need `contextualTypeHasPattern`
(`checker.go:13252`) where the pattern is an **assignment** pattern, not a
binding one. §489 ported that copy already; only its *search* is
binding-pattern-shaped.

The two roads are structurally identical and this is what makes the extension
small:

| | binding pattern | assignment pattern |
|---|---|---|
| the pattern node | `BindingPattern` | `ObjectLiteralExpression` (LHS of `=`) |
| an element | `BindingElement` | `PropertyAssignment` / `ShorthandPropertyAssignment` |
| "has a default" | `element.initializer.is_some()` | initializer is a `=` binary, or a shorthand's object-assignment-initializer |
| where the literal sits | initializer of a `VariableDeclaration` | **right** of that same `=` |

`matching_pattern_element` and `implied_pattern_member_is_optional` already
encode the binding half; the assignment half is their mirror.

### The bar

- **Primary.** `sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2`
  (36) and `shorthandPropertyAssignmentsInDestructuring_ES6` (18) — the two the
  last three entries aimed at and missed — close.
- **Safety.** The arm only ever turns a member OPTIONAL, and only for a literal
  that is the right-hand side of an assignment whose left is an object pattern.
  A literal outside that position cannot move. `RIGHT→` must be zero.
- **Falsifier.** If the named cases stay shut a *fourth* time, the defect is not
  the optionality copy at all and the whole §893–§897 reading of these rows is
  wrong — at which point the right move is to stop and read the case's full
  `.types` block rather than another upstream function.
- **Regression.** A test for the assignment form, one for the binding form
  beside it (which must not change), and one for an assignment with no default.

**Named risk.** The contextual type is the LHS pattern's own literal type, so the
arm reads the left of an assignment while checking its right. §890–§892 spent
three entries on re-entrancy of exactly that kind. Here the search is
**syntactic** — it never asks for the LHS's *type*, only whether its matching
member writes a default — so no resolution is entered. That is a deliberate
design choice and the reason this is attempted now rather than deferred with the
rest of §896.

## §897.1: +72, zero adverse — the primary leg closes for the first time in the thread

`contextualTypeHasPattern`'s assignment half ported. **72 W→R, zero adverse.**
`sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2` **48**,
`shorthandPropertyAssignmentsInDestructuring_ES6` **18**,
`conformance/assignmentTypeNarrowing` 6.

Both cases the bar named closed — the first time in §893–§897 that a bar's primary
leg fired, after three consecutive entries whose named case stayed shut. The
difference is not luck: §893's target was chosen from a census row's **shape**, and
§897's from the case's **source**, read after §895 forced the question. The four
entries between them were the cost of that distinction.

The optionality copy, the member loop, `hasDefaultValue`, the assignment-target
predicate and the name-matching helper were **all already written and all already
correct**. What was missing was 60 lines of search — the mirror of §489's, with
`ObjectLiteralExpression` for `BindingPattern` and "the right of this `=`" for
"the initializer of this declaration".

The named risk did not materialise. The search never asks for the left-hand
pattern's *type*, only whether its matching member writes a default, so nothing
re-enters — which is why it could be attempted here rather than deferred with the
rest of §896.

### The thread's arithmetic, closed

§893–§897: **+142 right, 4 `GAP→WRONG`, zero `RIGHT→`**, of which the 75 rows that
started it account for 66. The three entries that missed were not wasted — §895's
extraction is what made the recursion possible and §896's location is what made
§897 a 60-line search instead of a fourth guess — but the ledger is honest: three
entries of the five aimed at the wrong road.

## §898: `getWidenedUniqueESSymbolType` — +16, and a regression that had nothing to do with symbols

The `unique symbol` family is **164 wrong rows across 8 cases**, 115 of them in
three (`uniqueSymbolsErrors` 41, `uniqueSymbols` 39, `uniqueSymbolsDeclarations`
35). Located at the source rather than guessed at (§897's lesson), it splits in
two, and the port already has the type node itself (§595):

1. **74 rows: the port KEEPS `unique symbol` where upstream widens it to
   `symbol`.** `const a = [s]` is `symbol[]` upstream and `(unique symbol)[]`
   here.
2. the rest: `unique symbol` written in an **invalid** position.

This entry is (1). `getWidenedUniqueESSymbolType` (`checker.go:25505`) was absent,
and upstream has exactly one call site — the `getWidenedLiteralType` pair inside
`getWidenedLiteralLikeTypeForContextualType`, which is
`check_expression_for_mutable_location`'s tail here.

**+16 W→R, zero adverse.**

### The union branch rebuilt a union it had not changed

The first build measured **4 `RIGHT→WRONG`** in
`conformance/assignmentCompatWithDiscriminatedUnion`:

```
{ type: IAxisType; }   →   { type: "categorical" | "linear"; }
```

Nothing to do with `unique symbol`. `mapType` hands back its input when the
mapper changes nothing; this port's branch called `get_union_type` on the mapped
constituents unconditionally, and **a union carries its alias name while a freshly
minted one does not**. Returning `id` when `widened == constituents` fixed it.

> **The lesson is about rebuilding, not about symbols.** Any helper that maps over
> a union's constituents and re-mints must check whether it changed anything
> first, because the name is carried by the type and not by the constituents.
> Worth checking the other `get_union_type` callers that map-then-rebuild.

### Why only 16 of the 74

The array-literal rows did not move. The port's element road *does* call
`check_expression_for_mutable_location` — and then applies
`get_widened_literal_type` to the result a second time, which upstream does not
(its element type is that call's result plus optionality). Whether the second
widening is what strands them is the next question and is **not** answered here.

The invalid-position half is §899's.

## §899: `isValidESSymbolDeclaration` — +7, and the baseline overruled the source twice

The second half of §898's split: `unique symbol` written where it may not be.
`getESSymbolLikeTypeForNode` (`checker.go:22982`) mints the unique type only when
`isValidESSymbolDeclaration` (`checker/utilities.go:961`) passes, and answers
plain `symbol` otherwise — a three-arm syntactic predicate (a `const` in a
variable statement, a `readonly static` property, a `readonly` property
signature).

Ported faithfully, it measured **40 W→R against 36 R→W.** Two corrections were
needed, and both came from the *baselines* rather than from a closer reading of
upstream.

### Correction 1: unrecognised ≠ invalid

Upstream answers `false` for anything it does not recognise, but its `node` is
always the real declaration. This port reaches the declaration by climbing a
syntactic parent chain, and **a JSDoc `@type` does not share it**:

```js
/** @type {unique symbol} */
const x = Symbol()
```

puts a `JSDocTypeExpression` between the operator and the `const`, so the walk
failed to recognise a valid position and answered `symbol` — 6 `RIGHT→WRONG` in
`compiler/uniqueSymbolJs2`. Declining only where an *invalid* declaration is
actually visible, and keeping the unique type otherwise, is the tri-state
discipline the relater and `is_literal_of_contextual_type` already use.

### Correction 2: a PARAMETER keeps the unique type, which the source denies

`isValidESSymbolDeclaration` returns `false` for a parameter. The baseline does
not agree:

```
>invalidArgType : (arg: unique symbol) => void
```

Upstream errors on the position and still **prints the written form**, because a
signature's text comes from the node builder reusing the written annotation, not
from the computed type. Declining there measured **10 `RIGHT→WRONG`**, all
parameter or `this` positions in that one case.

> **ADR-0006 in miniature.** The oracle is the generated baseline, not a reading
> of the checker source. Twice in one entry the faithful reading of a predicate
> produced the wrong answer, because what the baseline records is the *printed*
> type and the printer does not always ask the checker.

Final: **7 W→R, zero adverse.** The parameter arm would have been +5 raw lines
better (22 W→R / 10 R→W) and is not taken — `RIGHT→WRONG` is the disqualifying
direction, and the 5 lines are not worth 10 confident wrong ones.

`unique symbol` after §898 + §899: **+23 of the family's 164**, with the
array-literal residue (§898's note about the doubled `get_widened_literal_type`)
still open.

## §900: the near-miss board, and why the `import("…")` family is not an arm

**997 cases sit one or two non-RIGHT rows from passing** — 1,486 rows in total.
That board is worth stating on its own: it is +997 cases (65.6% → ~76% pass rate)
for 1,486 lines, a far better case-per-line ratio than anything else available.
Grouped, it is heterogeneous (737 other-WRONG, 435 `any`, 310 declines), but
scanning it for *repeated* micro-patterns gives:

```
123 rows / 103 cases   a bare identifier declines
110 rows /  95 cases   a CALL answers any/error
 76 rows /  57 cases   want `typeof X`, got something else
 54 rows /  46 cases   got `typeof X`, want something else
 40 rows /  36 cases   `new X()` answers any/error
 38 rows /  28 cases   want an `import("…")` type
```

### The `import("…")` family, probed rather than assumed

69 rows corpus-wide want `typeof import("…")` at the NAME of an ambient module.
§143 already ports the *printing* (`getSpecifierForModuleSymbol`'s ambient half),
so the obvious reading is that nothing reaches it from that position.

**Built and measured: zero transitions.** Probed rather than left as a guess:

- the arm **is** reachable — the node is a `StringLiteral`, its parent is a
  `ModuleDeclaration`, and `binder.symbol_of(parent)` is `Some`;
- it fires and returns the module symbol's type;
- that type is `typeof "fs"` (the baked placeholder) in one case and **`error`**
  in another.

Neither reaches a baseline, because `type_to_string_at` intercepts the
placeholder and answers `None`, which the producer renders as a gap — exactly the
guard `checker.rs` documents. So the answer is unchanged and the score is
unchanged.

**Two real blockers, both downstream of the position:**

1. **The module symbol's type is often `error`.** `get_type_of_symbol` on it does
   not compute.
2. **§143's printing gates out AUGMENTED ambients** — it requires
   `[declaration]`, a single declaration — and most of this family is exactly
   that: `ambientExternalModuleReopen`, `duplicateIdentifierRelatedSpans_moduleAugmentation`
   and friends declare the module twice.

So it is not an arm; it is the module-type road plus widening §143's gate to the
multi-declaration case, with the alias-ambiguity question §143 deliberately
declined. **Reverted by file**, and recorded here so the next attempt starts from
the probe's three facts instead of re-deriving them.

> Same shape as §896: the census sized the family correctly and located it
> wrongly. The difference is that this time the location was checked with a probe
> **before** anything was built on top of it, which cost one build instead of
> three entries.

### A hazard removed on the way past

`array_literals.rs` applied `get_widened_literal_type` to the result of
`check_expression_for_mutable_location` — a second widening upstream does not do
(its element type is that call's result plus optionality). It was harmless while
that function always widened. **§890 gave it two branches that deliberately KEEP
a literal**, and a caller that immediately widens them away is a trap waiting for
whichever array path first reaches one.

Removed at all three sites. **Zero transitions**, which is the point: it is
redundant today and would have been wrong tomorrow.

## §901: `resolveUntypedCall` reaches the tagged-template road — +32, two whole cases

The near-miss board's *"want `any`, port declines"* cut is **340 rows across 131
cases**, and unlike the other cuts it has a dominant shape: an access or call
whose receiver is `any`. The two largest cases are
`taggedTemplateStringsWithTagsTypedAsAny` and its ES6 twin, 16 rows each.

```ts
var f: any;
f `abc`            // want any, got error
f `abc`.member     // want any, got error
```

`resolveUntypedCall` (`checker.go:9899`) answers `anySignature` whenever
`isTypeAny(funcType)`, having first checked the template for its own lines. **The
CALL road already had this predicate** — `is_untyped_call_target`, argued from
upstream's two adjacent lines in `checker-notes-calleegap.md` — and the
tagged-template road went straight to `resolve_call_signature`, which answers
`None` for `any` and gapped.

One `if`, placed after the template check so the template still contributes its
own rows, which is upstream's order.

**+32 `GAP→RIGHT`, zero adverse — and both cases closed entirely**, so this is
**+2 whole cases** as well as +32 lines.

> **The transferable part: a predicate that exists on one road and not its
> sibling.** This is the eighth instance this session of a capability present and
> a caller that does not consult it, and the cheapest yet to find — the two roads
> sit 500 lines apart in the same file, one calling `is_untyped_call_target` and
> one not. Worth a deliberate sweep: for each predicate the call road uses, does
> the tagged-template / `new` / decorator road use it too?

### What the near-miss board is, stated plainly

997 cases sit 1–2 rows from passing (1,486 rows). Its micro-patterns were sampled
in §900 and the two largest — *a bare identifier declines* (132 rows) and *a call
answers any/error* (110) — were read row by row. **The identifier cut has 132
different causes**: conditional types, mapped types, inference, recursive types,
JSDoc. There is no seam in it.

That is the honest characterisation of what remains: after this session's arms,
the board is broad feature completion, not gates. The families that paid here
were all *a rule upstream states in one place that this port had not wired up*,
and that population is now visibly thinner.

## §902: the destructure road gets the catch variable's type — +19, another case closed whole

`conformance/destructuringCatch`, 16 rows, all `want any / got error`:

```ts
try { throw [0, 1]; } catch ([a, b]) { a + b; }
```

With `useUnknownInCatchVariables: false` the catch variable is `any`, and
destructuring `any` gives every element `any` — the `parent_type == any`
short-circuit this port already ports (`checker.go:17709`).

**`symbols.rs` has computed the catch variable's type since §21** and
`get_type_for_binding_element_parent` never asked. The reason it could not is
§429's: **a catch variable with a PATTERN name has no symbol of its own**, so the
symbol road that knows the answer is unreachable from the destructure road. The
two roads each had half of it.

**+19 `GAP→RIGHT`, zero adverse.** `destructuringCatch` 16 (the whole case),
`objectRestCatchES5` 2, `asyncWithVarShadowing_es6` 1.

> **Ninth instance this session** of a capability present and a caller that does
> not consult it — and the second in two entries (§901 was the eighth). Both were
> found the same way: take a small cluster off the near-miss board, read the
> case's *source*, then ask which road computes that answer already. Neither
> needed a new rule.

The harness could not reach a catch-clause binding at all — `type_of_binding`
walked variable statements, function parameters and `for-in`/`for-of` heads — so
the tests come with a `TryStatement` arm. **A test harness that cannot express a
shape is a silent coverage hole**, and this one hid a whole statement kind.

## §903: inferring from `any`, built and REVERTED — the adverse case is the one named for it

`conformance/inferingFromAny` holds 14 `want any / got error` rows:
`declare function f4<T>(x: { bar: T; baz: T }): T` called with `a: any`. Upstream's
`inferFromTypes` has an explicit arm:

> We are inferring from an 'any' type. We want to infer this type for every type
> parameter referenced in the target type …

Ported as a direct propagation — source is `any` ⟹ every parameter the target
mentions collects `any` — and **measured +6 net (4 G→R, 2 W→R) against 4
`GAP→WRONG`**.

**Reverted**, for two reasons, the second decisive:

1. **Only 4 of the 14 closed.** `type_mentions_parameter` follows type-reference
   arguments and union constituents, not object members or signatures, so
   `{ bar: T; baz: T }` — the shape most of the case uses — is not recognised as
   mentioning `T`. Widening that helper has blast radius: §787 uses it for the
   union strike-out.

2. **The 4 adverse are `compiler/nonInferrableTypePropagation1`**, and they are
   `Thing<number>` answered as `Thing<any>`:

   ```
   result1   want Thing<number>   got Thing<any>
   ```

   That case exists precisely to pin this hazard. Upstream's arm is **not** a
   direct propagation: it threads a *propagation type* through a self-inference
   (`inferFromTypes(target, target)`) under a lowered priority, and pairs with
   `isNonInferrableType`, so an `any` arising from a **failed nested inference**
   does not overwrite a real candidate. This port cannot tell the two apart —
   both are `intrinsics.any` — so the direct form turns a recoverable inference
   into a confident `any`.

**Shipping the partial rule would have made the port fail the one case named
after the hazard.** +6 lines is not worth that, and §620's tolerance for
`GAP→WRONG` is about honest gaps becoming visible, not about reproducing a defect
upstream engineered around.

**Prerequisite, named**: the propagation type and priority machinery, or at
minimum a way to distinguish "the argument is genuinely `any`" from "inference
produced `any`". Reopen behind that, not before.

## §904: `isConstantReference`'s two missing arms — transcribed, measured ZERO, kept

§856's `_ => false` sweep, resumed. Mapping all 49 sites in
`check.rs`/`flow.rs`/`symbols.rs`/`declared.rs` to their functions and comparing
the narrowing-relevant ones against upstream:

`isConstantReference` (`flow.go:1814`) has **four** `case`s; this port had two.
Missing:

```go
case ast.KindThisKeyword:
    return true
case ast.KindPropertyAccessExpression, ast.KindElementAccessExpression:   // ← the second kind
```

(The binding-pattern arm needs `isSomeSymbolAssigned` and is not attempted.)

Both transcribed. The element-access arm is reduced to a **literal** key, the only
one whose property symbol this port can resolve.

**Measured: zero transitions.**

Kept rather than reverted, on this project's own precedent — §179/§181 recorded a
guard at +0 twice and §253 later made its population non-empty, which is where
*"an arm can be correct and unmeasurable until an unrelated fix creates the nodes
it acts on"* comes from. §239 found this function's third identifier disjunct by
the same sweep. **Recorded as inert so nobody re-measures it expecting a number.**

### What the sweep says about the port

Four narrowing predicates were compared against upstream this round
(`isConstantReference`, `narrowTypeByCallExpression`, `isConstantVariable`,
`isParameterOrMutableLocalVariable`) and **three were already complete** —
`narrowTypeByCallExpression` has both its arms, including the transcribed
`hasOwnProperty` branch, and declines only `TypePredicateKindThis`.

**The `_ => false` heuristic is no longer a good filter.** §856's 4-for-4 hit rate
came from a population that later sessions have largely worked through; the
remaining sites are mostly faithful. Saying so is worth more than another five
audits: **this lead is spent**, and the next reader should not budget for it.

## §905: mapped types get a print-only mint — +416, the session's largest change

`members.rs` records `ObjectFlagsMapped` as **"not ported at all"**, and the
corpus agrees: 623 non-RIGHT rows across 113 cases have a mapped type somewhere in
the wanted text, 175 of them wanting *nothing but* a mapped type.

The first slice is not evaluation. **Upstream keeps a generic mapped type
DEFERRED**, and its node builder prints it from the mapped type's own parts —
which, for a type that was never instantiated, are exactly the written ones:

```
{ [P in keyof T]: T[P]; }
```

`signatures.rs`'s §77 renderer **already produced that spelling**, for written
annotations, complete with `readonly`/`+readonly`/`-readonly`, `?`/`+?`/`-?` and
`as` clauses. `get_type_from_type_node` simply had no `MappedTypeNode` arm, so the
type was `errorType` and every position that needed the *type* rather than the
annotation gapped. **The mint is that renderer plus `new_named`.**

**Measured: 210 `WRONG→RIGHT` + 208 `GAP→RIGHT` against 92 `GAP→WRONG` and 3
`RIGHT→WRONG`. Net +416 right, `gap` 6,721 → 6,420, `wrong` 27,180 → 27,065.**
`mappedTypeRelationships` 63, `reverseMappedTypeIntersectionConstraint` 39,
`mappedTypeContextualTypesApplied` 29, `typeParameterConstModifiersReverseMappedTypes`
18, `keyofAndForIn` 21.

### Print-only, and why that is safe here

The minted type carries no members, no key set and no template — §40's sense of
print-only, and §811's hazard shape. The guard is structural rather than
argumentative: **this port has no mapped-type member road for the mint to escape
into.** `get_property_of_type` on a `Named` with no table already declines, so the
type can be carried and printed and nothing can read through it.

### The 3 `RIGHT→WRONG`, and a guard that measured worse

`conformance/recursiveMappedTypes`: `type Recurse = { [K in keyof Recurse]: Recurse[K] }`
records `>Recurse : any`, upstream's **circularity** answer. The written render
resolves nothing, so it cannot observe the cycle.

A syntactic guard was built — the enclosing alias's own name appearing in the
rendered text — and **measured worse**: it recovered one of the three and cost
five elsewhere, because the corpus's other two are *mutual* recursion
(`Recurse1` through `Recurse2`), which no same-name test can see. Taking the three
at 208:1 is the better trade, and the guard is recorded rather than kept.

### Two stand-in tests came due, and both said how to fix themselves

`alias_naming.rs` asserts that an alias whose BODY gaps cannot be named, using
whatever is currently unported as the body. Its comment: *"Twice now this test's
frontier has advanced rather than the test being wrong … the assertions are
flipped and kept rather than deleted."* **Third flip.**

`type_predicates.rs` uses an unported type node as a stand-in and says: *"A MAPPED
type is the unported type node used as the stand-in — it came due once already …
If mapped types land, re-point again — not delete."* **Re-pointed**, to a
conditional type.

> **Both tests survived their own obsolescence because they wrote down what they
> were really testing and what to do when the stand-in landed.** A test pinned to
> a *capability gap* rots; a test pinned to a *rule*, carrying instructions for
> its stand-in, advances the frontier and stays useful. That is worth copying.

What remains of mapped types is the subsystem proper — `resolveMappedTypeMembers`,
homomorphic instantiation, key remapping evaluation — and the 92 `GAP→WRONG` are
its bill: positions that now carry a mapped type and are asked to do something
with it.

## §906: conditional types take the same mint — +123, and a stand-in that came due twice in one session

§905's slice applied to the other deferred form. 348 non-RIGHT rows across 69
cases have a conditional in the wanted text, and upstream's rule is the same:
a conditional whose check type is generic stays **deferred** and prints from its
parts, which for an uninstantiated one are the written ones.

The renderer needed two new arms — `ConditionalTypeNode` and `InferTypeNode` —
and one widening: §449's `ParenthesizedTypeNode` arm admitted only a wrapped
**union**, so `(infer U)[]` declined the whole render and
`T extends (infer U)[] ? U : never` was unreachable. A constrained infer
(`infer U extends string`) is declined rather than guessed, which keeps §77's
admission set bounded.

**Measured: 75 `WRONG→RIGHT` + 46 `GAP→RIGHT` against 28 `GAP→WRONG` and 3
`RIGHT→WRONG`. Net +123**, `right` 440,753 → 440,876.
`distributiveConditionalTypeConstraints` 17, `infiniteConstraints` 14,
`conditionalTypes1` 13, `reverseMappedTypeIntersectionConstraint` 12.

**The alias road is untouched.** §92's `evaluate_conditional_alias` runs before
any alias reference reaches this node, and its deliberate `error` for an
unevaluable conditional *alias* in an alias-declared position is a decision about
the alias, not about the node — the two do not meet.

The mint merged with §905's: the two arms were byte-identical, which is the
clearest possible statement that "deferred form prints as written" is one rule
and not two.

### A stand-in obsoleted twice in one session

`type_predicates.rs` pins *"a construct refuses WHOLE"* using whatever type node
is currently unported. Its history now reads:

| stand-in | retired by |
|---|---|
| `keyof T` | §35's deferred print |
| a mapped type | §905 |
| a conditional type | §906 — **the same session, before the comment's ink was dry** |
| `keyof` over a GENERIC operand | current |

And `alias_naming.rs` needed its conditional line **removed** rather than
re-pointed, because the `keyof string` stand-in above it still holds and one live
stand-in is what the test needs.

> **The tests were right to be written this way and the churn is the price.** A
> stand-in test costs an edit every time the frontier moves — which is exactly
> when you want to be told. The alternative, pinning to a capability gap with no
> instructions, produces a test that silently asserts the wrong thing instead.
> Both files said what to do; neither needed a judgement call.

Mapped and conditional types together (§905 + §906): **+539 right**, `gap` 6,721
→ 6,346, `wrong` 27,180 → 27,021. What remains is the two subsystems proper —
`resolveMappedTypeMembers`, distribution, `infer` binding — and the 120 combined
`GAP→WRONG` are their bill.

## §907: §722 re-measured — the ratio has NOT moved. Refusal confirmed.

`compiler/privacyLocalInternalReferenceImportWithExport` heads the board's
non-advanced cases at **176 rows**, wanting an internal import alias's own name
(`im_public_i_public`) where this port prints the qualified target
(`m_public.i_public`). That is §722's shape, and §722 refused it **with a number
and an explicit note that the number moves** — *"A refusal's ratio is not static;
this one has moved from 26:130 to 59:130."*

Re-measured, the way §832 rewarded this session. Every `best_name` call site
flipped to `admit_local_import_equals = true`:

```
WRONG->RIGHT  59   importStatements 12, importInTypePosition 6, constEnumOnlyModuleMerging 4
RIGHT->WRONG 130 ⚠ privacyImport 52, privacyImportParseErrors 52, privacyGloImport 13, privacyGloImportParseErrors 13
```

**Identical to §722's measurement. The ratio has not moved at all this time**
(26:130 → 59:130 → 59:130), and the 130 are the same four cases. §722's argument
stands unchanged: those lines become *knowingly incorrect output* —
`typeof m1_im1_private` where upstream writes `typeof m1_M1_public` — and they
look free only because their cases fail for other reasons.

**Reverted. Refusal confirmed, with today's number beside the old one.**

### And the 176-row case is not actually this

Worth recording, because it is why the re-measurement was run: the widening gained
**nothing** in `privacyLocalInternalReferenceImportWithExport`. Its rows want the
alias name and did not move, so `best_name` is not on their road at all — they are
a *third* mechanism, neither §722's filter nor §900's module printing.

Two sessions could easily spend themselves assuming the biggest case on the board
belongs to the nearest named refusal. It does not, and the check cost one build.

### The correct fix for §722's family, named

Upstream prints the alias name when the alias is **reachable from the reference
site** and the target's name when it is not — `isTypeAccessible` /
`getAccessibleSymbolChain`. §806 approximated exactly that distinction
syntactically for indexed-access objects (*"an alias whose declaration has a
function or block ancestor is not nameable from an arbitrary site"*). The blanket
`true` has no such test, which is precisely why it is wrong for 130 lines. **§664
sized `getAccessibleSymbolChain` and refused it; that remains the prerequisite**,
and a §806-style syntactic approximation is the cheaper thing to try first.

## §908: paying §906's bill by evaluating at the node — built, 13 `RIGHT→WRONG`, REVERTED

§905/§906 created a standing bill: **217 rows across 66 cases where the port now
EMITS a mapped or conditional type and is wrong**, the largest shape being

```
aliasOfGenericFunctionWithRestBehavedSameAsUnaliased
   want "y"   got a extends b ? "y" : "n"
```

— upstream resolved the conditional because its check type is concrete at that
point, and §906's mint printed the deferred form for every conditional
indiscriminately.

The decision procedure already exists: §821's three outcomes inside
`evaluate_conditional_alias`. Lifting it to a node-level
`evaluate_conditional_type_node`, called from the mint before it prints, is the
obvious payment.

**Measured: +23 net (1 `GAP→RIGHT`, 15 `WRONG→RIGHT` equivalent) against 7
`GAP→WRONG` and 13 `RIGHT→WRONG`.** Reverted.

### Why it fails, which is the finding

Two facts together:

1. **The bill barely moved** — one row. The check types in
   `aliasOfGenericFunctionWithRest…` are *not* decidable to this relater at the
   node, so the evaluation declines exactly where it was supposed to pay.
2. **It cost 13 `RIGHT→WRONG`** in `conditionalTypeAssignabilityWhenDeferred`,
   `reverseMappedTypeIntersectionConstraint` and others.

**§821's evaluation is safe because of where it runs, not because of what it
does.** An alias evaluation carries a *binding frame* (`alias_evaluation_bindings`)
that makes the check type genuinely concrete, so the relater's `Related` /
`NotRelated` are trustworthy. At an arbitrary node there is no such frame: the
check type is often a type parameter or a partially-instantiated reference, and
this relater answers a confident `NotRelated` for pairs it cannot really decide —
which is the exact hazard §821's own comment records having removed a gate for
(*"a `false` here is a confident WRONG branch rather than a decline"*).

The ternary's `Unknown` is supposed to absorb that, and for the alias road it
does. Away from a binding frame it does not absorb enough.

**Named prerequisite**: a relater whose `Unknown` is reliable for type-parameter
and partially-instantiated operands — or, cheaper and more likely, a gate that
runs the evaluation *only* where a binding frame is in scope, which is the
condition §821 already satisfies structurally and would make this a no-op rather
than a gain. **The bill is real and this is not how it gets paid.**

## §909: §906's bill, paid where it could be — +102

§908 failed to pay the bill by *evaluating*. This pays the other half by *naming*.

`conformance/mappedTypes1`, 21 rows, all of this shape:

```
T12   want T12   got { readonly [P in keyof Item]: Item[P]; }
```

Upstream attaches an **`aliasSymbol`** to a type minted from an alias body and its
node builder prints that name. §905's mint has no alias link, so it printed the
body everywhere — right at an anonymous site, wrong at a named one.

**Two restrictions, both measured rather than reasoned:**

- **Non-generic aliases only.** A generic alias is instantiated per reference and
  upstream prints the instantiated body — which is precisely what §905's 63-row
  gain in `mappedTypeRelationships` is made of. Naming those would take it back.
- **Mapped types only, not conditionals.** The unrestricted version measured **14
  `RIGHT→WRONG`** (`conditionalTypes1` 7, `inlineConditionalHasSimilarAssignability`
  4). §92's `evaluate_conditional_alias` owns the conditional-alias road, and a
  name in place of the evaluated branch is a wrong answer. Restricting to mapped
  took the reading from +89 with 14 `RIGHT→WRONG` to **+102 with 1**.

**Measured: 81 `WRONG→RIGHT` + 22 `GAP→RIGHT` against 2 `GAP→WRONG` and 1
`RIGHT→WRONG`.** `mappedTypes1` 21, `correlatedUnions` 14, `numericEnumMappedType`
9, `coAndContraVariantInferences3` 6.

### The bill, after §908 and §909

§905/§906 created 217 wrong rows by emitting deferred forms. §909 pays 81 of
them. What remains is the conditional half, and §908 established how it does
*not* get paid: **§821's evaluation is safe because it runs inside a binding
frame, not because of what it computes**, and lifting it to an arbitrary node
meets undecidable check types where this relater answers a confident
`NotRelated`.

> **A print-only mint has a bill, and it comes due in two currencies.** Where the
> port should have *named*, §909 pays it cheaply — the information was there and
> only the link was missing. Where the port should have *evaluated*, §908 shows
> the payment needs machinery the port does not have. Worth knowing before the
> next deferred form is minted: **ask which of the two the wanted text is, because
> only one of them is cheap.**

## §910: §890's blocked half, finally — +78, and the cycle needed TWO literals

§890 excluded call arguments from `checkExpressionForMutableLocation`'s third
branch and sized the loss at +57. Three entries then failed to recover it:
§890.2's diagnosis (the memo-selected branch) was wrong, §891's probe flag was
inert, §892's symbol cache was circular by construction.

All three looked for the cycle in the wrong place. Reading what §890 actually
lost — **every row a member of `context: { tag: "A", value: 1 }`** — gives it:

```ts
defineOptions({
  context: { tag: "A", value: 1 },   // ← the members that broke
  produce() { return 42; },
})
```

**The re-entry needs two levels of object literal.** Checking the INNER literal's
member asks for its contextual type, which asks for the inner literal's, which
asks for the OUTER literal's — and the outer literal is the argument whose
signature is being resolved. A member of the argument literal *itself* is one hop
short of that.

So the exclusion narrows from *"anywhere under a call"* to *"under a nested object
literal in a call argument"*.

**Measured: 81 `WRONG→RIGHT` against 3 `RIGHT→WRONG`.** `compiler/temporal` **63**
— the case that has headed the literal-widening family since §890 and resisted
three attempts — plus `inferFromGenericFunctionReturnTypes3` 6,
`destructuringParameterProperties1` 5. The 3 adverse are
`contextualTypingOfOptionalMembers`, `state : number` answered as `100`: a literal
kept where upstream widens, a near miss rather than a wild answer.

### The control test corrected me

I wrote the control asserting that a member of a nested argument literal *widens*,
since branch 3 no longer runs for it. It does not — it keeps its literal through
§56's `annotation_member_context`, which reads the written annotation **without
asking for a contextual type**.

That is the sharper statement of what the exclusion does: it does not decide
whether these members keep literals. **It keeps branch 3 from asking a question at
a position where the question re-enters**, and §56 answers the same question
another way. The assertion was corrected to the measured value and the comment
now says which road supplies it.

> **Four entries to find a two-level condition.** §890.2, §891 and §892 each named
> a mechanism and each was wrong; what settled it was reading the *fixture* of the
> rows that broke rather than the machinery they broke in. The same correction
> §896 recorded, in a different subsystem, one session later.

## §911: `getContextualThisParameterType`, built and reverted — the contextual type is not the answer

`this` as an expression is **173 wrong rows across 62 cases**, and 44 of them
answer `typeof globalThis`. The largest readable slice is `this` inside an
**object-literal method whose literal has a contextual type**:

```ts
function foo(bar: X | Y) { }
foo({ type: 'y', value: 'done', method() { this } })   // want Y, got typeof globalThis
```

`check_this_expression`'s own doc names the hole — *"`getContextualThisParameterType`
is unported and answers nothing here"* — and §142's literal-self arm is gated on
the literal having **no** contextual type, so nothing served the other side.

Built: when the literal has a contextual type, answer it.
**Measured +13 net against 4 `RIGHT→WRONG` and 9 `GAP→WRONG`. Reverted.**

### Why the contextual type is the wrong answer

The want is **`Y`**, not `X | Y`. Upstream's
`getApparentTypeOfContextualType` is followed by discrimination: the literal's
`type: 'y'` member selects the matching constituent of the contextual union
(`getContextualTypeForObjectLiteralElement`'s discriminant road). Answering the
whole union is wrong in exactly the cases the feature exists for — which is why
`contextualTypeShouldBeLiteral` took **5 `GAP→WRONG` from the very change meant to
fix it**, and `thisTypeInFunctions2` lost 4 that were right.

**The prerequisite is discriminated-union selection of a contextual type**, not
the `this` road. §750's `narrow_type_by_discriminant` is the nearest existing
machinery and it narrows a *flow* type rather than selecting a contextual
constituent.

> Third time this session that a feature's nearest-looking hole was not its actual
> one (§896, §908, §911). The pattern in all three: **the port had the input and
> the position right and the missing piece was one step further in** — a road that
> turns the input into the answer. Worth asking, before building: *is the thing I
> am about to supply the answer, or an ingredient?*

## §912: §911's prerequisite was already written — +24, zero `RIGHT→WRONG`

§911 reverted and named its prerequisite: *"discriminated-union selection of a
contextual type"*. **It exists.** `discriminate_union_root` (`symbols.rs`) has
selected a union constituent by an object literal's own members since §750's
family, and `check_this_expression` never called it.

**Tenth instance this session of a capability present and a caller that does not
consult it** — and the first found by reading a revert's own prerequisite line
rather than by census.

```ts
function foo(bar: X | Y) { }
foo({ type: 'y', value: 'done', method() { this } })   // this : Y
```

**Measured: 18 `WRONG→RIGHT` + 6 `GAP→RIGHT` against 4 `GAP→WRONG`, zero
`RIGHT→WRONG`.** `contextualTypeShouldBeLiteral` 15 + 6 — the case §911 made
*worse* by 5 — and `thisTypeInFunctions2` +2.

### One restriction, measured into existence

Unrestricted, this kept §911's 4 `RIGHT→WRONG` in `thisTypeInFunctions2`, whose
literal is contextually typed by an interface with the index signature
`((this: any, …args: any[]) => any)`. Upstream answers `any` there through its
**first** branch — the method's own contextual SIGNATURE carrying a `this`
parameter — which wins ahead of the literal branch entirely.

That branch was written and **measured inert**: this port's
`contextual_signature` cannot reach an index signature, so it answers `None` and
the literal road ran anyway. Removed rather than kept.

What works is restricting the literal road to a **union** contextual type — the
shape discrimination is for, and the shape this arm exists to serve. It removes
all four adverse rows and *gains* two in that same case. A non-union contextual
type keeps `any`, which is what it answered before, so the restriction costs
nothing measured.

> **§911 → §912 is the session's cleanest example of a revert paying.** §911
> measured +13 with 4 `RIGHT→WRONG` and wrote down one sentence about what it
> lacked. That sentence was the search query. The rebuild took one grep, and the
> difference between the two entries is entirely *which function computes the
> answer* — the position, the gate and the input were right the first time.

## §913: three "unported" claims, all stale — one was a live gap

§912 was found by reading a revert's prerequisite line. That suggested a sweep:
**grep the checker for claims that something is unported, and check each against
the code.** Twenty-three such claims; three examined, **all three stale**:

| claim | where | reality |
|---|---|---|
| *"`isTypeComparableTo` is unported"* | `assertion_overlap.rs` header | `Relation::Comparable` exists since **§750** |
| *"`getAssignmentReducedType` is unported"* | `symbols.rs` §56 comment | `get_assignment_reduced_type` exists in `flow.rs` |
| *"`isTypeDerivedFrom` … is unported"* | `unions.rs` reduction doc | §357 ported it; the loop tests `heritage_chain_contains` |

Two were comments describing gates that had already been removed — stale prose
over correct code. **The first was a live gap**: `check_assertion_overlap` really
did run `Relation::Assignable` in both directions, with a header arguing carefully
why the substitution was sound. That argument *was* sound when written; the claim
above it had simply expired.

Switched to `Relation::Comparable`, which is what `checkAssertionWorker` uses.
**`diagnostics` 2,611 → 2,612 cases; `checker_types` unchanged.**

### The sweep's real result

Three probes, three stale claims, and **each one had already cost me time in this
session**: I read `unions.rs`'s `isTypeDerivedFrom` line and started sizing a
port of it before finding §357; I read `symbols.rs`'s line and looked for a
reduction helper that was two files away.

> **A stale "X is unported" is worse than no comment.** It reads as a surveyed
> frontier and it sends the next reader to build something that exists. The three
> corrected here were all written truthfully and all outlived their subject, which
> means the failure mode is structural rather than careless: **a claim about what
> the port lacks is a claim about another file, and nothing makes it fail when
> that file changes.**
>
> The remaining twenty are not audited. That is the honest state, and the cheap
> discipline that would have prevented all three is to name the *function* that
> would have to exist — `isTypeComparableTo` did, which is why one grep settled
> it — rather than the capability in prose.

## §914: tagged templates get type arguments and arity selection — +14

**`taggedTemplate*` is 418 non-RIGHT rows across 16 cases**, all blocked by one
sentence in `check_tagged_template_expression`: *"a tagged template's arguments
are the template strings array and the substitutions, neither of which this port
builds"*. It passed `None` for the argument list and, one line earlier, rejected
the whole expression if it carried any written type arguments at all.

Two slices need no argument *expressions*:

1. **Written type arguments.** `` f<number>`x` `` instantiates the return exactly
   as a call's do — the same `fillMissingTypeArguments` surplus-half the call road
   ports, with the same refusal of the missing half (defaults are not modelled;
   filling with `any` measured 62 `RIGHT→WRONG` when the call road tried it). The
   guard `!node.type_arguments.is_empty() → error` fired *before* anything
   resolved, so this was unreachable rather than unported.
2. **Overload selection by ARITY.** `getEffectiveCallArguments` builds a synthetic
   `TemplateStringsArray` plus one argument per substitution, and **the COUNT of
   that list needs no synthetic expression**: it is `1 + spans`. `hasCorrectArity`
   is upstream's first pass in `chooseOverload`, and a single survivor is the
   answer — `callres2` slice 1's rule, reused.

**+14, zero adverse.** `taggedTemplatesWithTypeArguments1` 4,
`taggedTemplateStringsWithOverloadResolution1` 4 + 4,
`taggedTemplateContextualTyping2` 2.

A pinned test, `explicit_type_arguments_are_a_gap`, asserted the old refusal and
is flipped; `a_generic_tag_is_a_gap` beside it still holds and is the honest live
half.

### What the other 404 rows need, precisely

Argument *checking*, contextual typing and inference all need the expressions:

- `contextual_type_for_argument` takes a `&CallExpression` and has no
  tagged-template path, so a substitution is never contextually typed —
  `taggedTemplateContextualTyping1/2`, 52 rows.
- inference from the substitutions — `taggedTemplateStringsTypeArgumentInference`
  and its ES6 twin, **146 rows** — needs the synthetic first argument to occupy
  position 0 so substitution *i* pairs with parameter *i+1*.

Neither is blocked on an idea; both are blocked on **giving the resolution road a
notion of an argument that is a TYPE rather than an expression**. `choose_overload`
and `check_generic_call` are both index-driven over `&[Expression]`. That is the
one change, and it is the prerequisite for the remaining 404.

## §915: contextual typing of a tagged template's substitutions — built, zero, reverted

§914 named two remaining blockers for `taggedTemplate*`'s 404 rows and said the
contextual half *"does not wait on"* the argument-road refactor, because it needs
only the parameter's type.

Built: a `TemplateSpan` arm in `get_contextual_type` returning the tag signature's
parameter at `index + 1` — the `+1` being the synthetic `TemplateStringsArray` at
position 0. `has_no_contextual_type` has known this routing since §865 (it answers
*"there IS a contextual type here"* for a tagged span) and `get_contextual_type`
had no arm to produce one, which is the shape that has paid eleven times this
session.

**Measured: zero.** First build declined outright — `single_call_signature`
refuses a generic, and every tag in `taggedTemplateContextualTyping1/2` is
generic. Allowing a generic single signature through (§75's rule: the substitution
adopts the tag's type parameters) made the arm fire and still measured **no gain,
plus 3 `WRONG→GAP`**. Reverted.

### Why, and what it corrects in §914

§914 wrote that the contextual half needs only the parameter's type. **That is
wrong.** The parameter of a generic tag is `(x: T) => T`, and a contextual type
mentioning an uninferred `T` does not type the substitution — the substitution's
own type is what *drives* the inference that gives `T` a value. Supplying the
uninstantiated parameter hands the arrow a contextual type whose content depends
on the arrow.

So the contextual half does **not** come free of the inference half; it is the
same blocker seen from the other side, and §914's sentence is corrected here
rather than left standing. **The prerequisite for all 404 rows is the one §914
named second: an argument that is a TYPE rather than an expression**, so the
strings array can occupy position 0 and inference can run over the substitutions.

> The eleven-times pattern — *a capability present and a caller that does not
> consult it* — has a failure mode, and this is it. The caller was missing for a
> reason: what it would have consulted is not sufficient. **Checking that the
> capability's output is the ANSWER and not an ingredient is the same test §911
> failed**, and I did not apply it here despite having written it down two entries
> earlier.

## §916: inference from a tagged template's substitutions — +77, and the offset was never needed

§914 and §915 both pinned the same prerequisite: *"give the resolution road an
argument that is a TYPE rather than an expression"*, so the synthetic
`TemplateStringsArray` can occupy position 0 and substitution *i* can pair with
parameter *i + 1*. I sized that as a refactor across `choose_overload` and
`check_generic_call`, both index-driven over `&[Expression]`.

**It was not needed.** Every pairing in that road is `parameters[i] ↔
arguments[i]`, so the `+1` can come from either side — and **dropping the first
parameter from the signature achieves it while touching nothing**:

```rust
let mut shifted = signature.clone();
shifted.parameters.remove(0);
self.check_generic_call(&shifted, node.node_id, &substitutions)
```

The remaining parameters line up with the substitutions by construction; the
return type and type parameters are untouched, so the inference that runs is
exactly upstream's over exactly the same pairs.

**Measured: 64 `WRONG→RIGHT` + 13 `GAP→RIGHT` against 3 `GAP→WRONG`, zero
`RIGHT→WRONG`. +77.** `taggedTemplateStringsTypeArgumentInference` **29** and its
ES6 twin **29**, `parenthesizedContexualTyping3` 13,
`taggedTemplateContextualTyping1` 2.

What it gives up, stated: a tag whose FIRST parameter is generic
(`tag<T>(s: T, …)`) loses that one inference site, where upstream infers
`TemplateStringsArray` into it. No corpus tag is written that way.

> **Two entries pinned a prerequisite that did not exist.** §914 and §915 both
> reasoned from the shape of the code — *these functions index over expressions,
> therefore a synthetic expression is required* — and neither asked whether the
> index could be moved on the other side. **A prerequisite named twice is not
> thereby confirmed**; the cheap check is to state what the machinery needs
> (`parameters[i]` must be the substitution's parameter) rather than what it
> currently has.

### And a test expectation wrong again

I asserted `tag\`a${42}b\`` gives `number`. It gives **`42`**, and the port was
right: `getCovariantInference` widens a literal candidate only when
`!primitiveConstraint && topLevel && (isFixed || !isTypeParameterAtTopLevelInReturnType)`,
and `T` *is* the return type here, so no widening happens — the same rule that
makes `f(42)` on `f<T>(x: T): T` record `42`. Corrected in the test with the rule
beside it. **Ninth wrong expectation of the session**, all in the same direction:
assuming a widening upstream does not perform.

## §917: overload selection by the substitutions' types — +38

§916's shift, applied to selection as well as inference. §914 could only pick an
overload when **exactly one** candidate survived on arity; two or more kept the
gap, because `chooseOverload`'s argument pass needs arguments.

The same trick serves: shift every candidate past its strings-array parameter, run
`choose_overload` over the substitutions, and **map the pick back to the unshifted
candidate by declaration** — so the signature that leaves is the real one and only
the selection used the shifted view.

**Measured: 34 `WRONG→RIGHT` + 4 `GAP→RIGHT` against 5 `GAP→WRONG`, zero
`RIGHT→WRONG`. +38.** `taggedTemplateStringsWithOverloadResolution3` 13 + 13,
`…Resolution1` 4, `…Resolution2` 2 + 2.

### The tagged-template family, closed out

| entry | slice | measured |
|---|---|---|
| §914 | written type arguments; single arity survivor | +14 |
| §916 | inference from the substitutions | +77 |
| §917 | overload selection by substitution types | +38 |
| | | **+129 of the family's 418** |

All three are the same observation used three ways: **a tagged template's argument
list differs from a call's only by a leading parameter, so shifting the SIGNATURE
is equivalent to synthesising the ARGUMENT** — and the signature side needs no new
type, no new enum, and no change to the machinery that consumes it.

§914 and §915 both named the argument-side refactor as the prerequisite. It was
never required, and two entries asserting it did not make it true.

## §918: §915's arm re-measured after §916 — still zero, and now for a known reason

§915 built a `TemplateSpan` arm in `get_contextual_type`, measured zero, and
diagnosed it: *"the substitution is what DRIVES the inference that gives `T` a
value"*. §916 made the substitutions drive inference, which should have removed
that blocker — the §832 re-measurement pattern that has paid twice this session.

**Re-measured: still zero** (same 3 `WRONG→GAP` in `taggedTemplatesWithTypeArguments1`,
which prove the arm fires). Reverted again.

### The reason, this time located rather than reasoned

`taggedTemplateStringsTypeArgumentInference` — the 29-row case whose `n => n`
rows want `(n: unknown) => unknown` — is built almost entirely of tags whose
**first** parameter is generic:

```ts
function noParams<T>(n: T) { }                       noParams ``;
function someGenerics1a<T, U>(n: T, m: number) { }   someGenerics1a `${3}`;
```

That is **exactly the limitation §916 recorded** — *"a tag whose FIRST parameter
is generic loses that inference site, where upstream infers
`TemplateStringsArray` into it"* — written when I thought no corpus tag was
shaped that way. **It was wrong: the largest remaining case is nothing but that
shape.** Corrected here.

So these rows need the thing §916 showed was unnecessary for the *other* slices:
a real argument at position 0, carrying the global `TemplateStringsArray` type, so
inference can flow into a generic first parameter. That is a genuine prerequisite
for **this** sub-family, and §916/§917 are the evidence that it is not one for the
rest.

> **Two re-measurements, two different outcomes.** §832's refusal reopened because
> the checker had moved underneath it; §915's did not, because its blocker was
> only half removed. Re-measuring a revert is cheap and worth doing — and the
> result is only informative if the entry says *which* blocker it was testing.
> §915 did, which is why one build settled it.

## §919: the synthetic leading argument, built — +4, reverted

§918 named the last blocker for `taggedTemplateStringsTypeArgumentInference`: a
real argument at position 0 carrying `TemplateStringsArray`, so inference can flow
into a tag whose **first** parameter is generic.

Built properly:

- `check_generic_call_leading(signature, call, arguments, instantiated, leading)`,
  with `leading: Option<TypeId>` occupying position 0 so parameter *i + 1* pairs
  with argument *i*. Four index reads in `check_generic_call_with` became
  `index.checked_sub(lead)`, and **`None` reduces every one of them to exactly
  what it was** — verified by `no transitions vs baseline` with the refactor in
  and the tagged-template road still on the old path.
- the `NoSubstitutionTemplateLiteral` form admitted, since `` noParams`` `` has no
  spans and its whole argument list is the strings array.

### The trap, and §888 had already recorded it

`global_type_symbol("TemplateStringsArray")` answered `None`, which read as *"the
lib is not mounted"*. It is mounted. **The helper defaults to arity 1**
(`global_type_symbol_with_arity(name, 1)`) and `TemplateStringsArray` is
non-generic — the identical trap §888 hit with `Iterable`, where the arity-1
lookup *"returned `None` on every case and the arm measured a clean zero"*. One
`0` fixed it.

### Measured +4, and reverted

`taggedTemplateStringsTypeArgumentInference` 2 + 2, against 2 `GAP→WRONG`.
Re-adding §915/§918's contextual arm on top changed nothing (+0, 3 more
`WRONG→GAP`).

**Reverted.** A new parameter on the inference entry point that every generic call
in the corpus flows through is real surface, and +4 net does not buy it. The
mechanism is correct and the entry records how to rebuild it, which is worth more
here than the lines.

### What the family actually needs, third correction

The residue is `` someGenerics2a`${ n => n }` `` wanting `(n: unknown) => unknown`
— a **context-sensitive substitution** whose type parameter is fixed to `unknown`
by the fixing mapper (§834's road for calls). That is neither the leading argument
(§919) nor the contextual parameter (§915/§918/§920) but the two of them plus the
deferred-argument pass agreeing, and each was measured alone.

> **Three entries named three different prerequisites for one residue, and all
> three were real but none was sufficient.** The lesson is not to stop naming
> them — §916 came directly from §915's sentence — but that *"X is the
> prerequisite"* should be written as *"X is A prerequisite"* unless it has been
> measured with the others already in place.

## §920: §802's refusal is right and its DIAGNOSIS is wrong — upstream gates on inference PRIORITY

The `gaproot` board — used for the first time this session — ranks gap roots by
lines unblocked, and `expression answered error: ArrayLiteralExpression` is its
most concentrated: **147 lines, 16 cases, top-10 = 94.6%**. Its largest case is
`conformance/heterogeneousArrayLiterals` (20), which is §802's family.

§802 refused replacing `single_common_supertype`'s leftmost fallback with a union
at **−23** (29 `RIGHT→WRONG` against 6 `WRONG→RIGHT`), and diagnosed it:

> upstream's rule is narrower than "union whenever no candidate dominates".
> `getUnionType` there runs under `UnionReductionSubtype` …

It then **built that prerequisite and re-measured identically**, and concluded
*"upstream is picking a single candidate there for some reason that is **not**
`UnionReductionSubtype`"* — correctly, and without finding the reason.

**The reason is in `getCovariantInference` (`inference.go:1455`):**

```go
// If all inferences were made from a position that implies a combined result, infer a union type.
// Otherwise, infer a common supertype.
if inference.priority&InferencePriorityPriorityImpliesCombination != 0 {
    unwidenedType = c.getUnionTypeEx(baseCandidates, UnionReductionSubtype, nil, nil)
} else {
    unwidenedType = c.getCommonSupertype(baseCandidates)
}
```

with `PriorityImpliesCombination = ReturnType | MappedTypeConstraint | LiteralKeyof`
(`checker.go:317`).

**Upstream unions or takes a common supertype according to WHERE the candidates
came from, not according to what they are.** §802 and its follow-up both searched
the candidates for the discriminator; it is not in them. That is why building
`UnionReductionSubtype` changed nothing: the two readings agree on every candidate
set and disagree only on provenance.

### What this makes the prerequisite

An **inference priority** on `InferenceInfo` — at minimum the `ReturnType` bit,
which this port already has the collection for (`return_mapper` in
`check_generic_call_with` is exactly *"inferred from the contextual return
type"*). `MappedTypeConstraint` and `LiteralKeyof` have no counterpart here yet.

**Not built.** The port's `InferenceInfo` carries `is_fixed` and `top_level` and
no priority, and the 6 conversions §802 measured are in
`inferentialTypingWithObjectLiteralProperties` — an ARGUMENT-side family, so the
bit that would serve them is likelier `MappedTypeConstraint` or `LiteralKeyof`
than `ReturnType`. Building only the bit I can source would be a third
measurement of the same guess.

> **§802's refusal stands; its number was never in doubt.** What is corrected is
> the *account of why*, which two entries had left as an open question and which
> a reader would otherwise have re-derived. **A refusal whose cause is wrong is
> still a refusal — but it sends the next attempt at the wrong prerequisite**,
> and this one had already sent one.

### §920.1: the `ReturnType` bit traced to the resolution site — no population

§920 left open whether to build the one priority bit this port can source. Traced
before building, which is the whole lesson of §919 and §915:

- `check_generic_call_with` builds `return_mapper` separately from `infos`;
- at the resolution site the candidate list is `flatten_infos(&infos)`
  (`inference.rs:676`) — **`return_mapper` is not in it**;
- the return mapper's entries reach only `partial`, the *serve* map for deferred
  arguments, and fill only parameters the arguments left empty.

So a `ReturnType` priority bit on `InferenceInfo` would be read at a site its
candidates never reach. **It would have measured a clean zero**, and the reason
would have looked like "the rule does not pay here" rather than "the bit has no
population" — which is §888's `Iterable` shape and §904's inert-arm shape both
over again.

**The priority model has to start with a bit whose candidates reach
`covariant_combination`**, and those are argument-side: `MappedTypeConstraint` and
`LiteralKeyof`, neither of which this port computes. That is the real prerequisite
for §802's family, and it is now traced rather than guessed.

> Three entries in a row (§918, §919, §920.1) ended by *not* building something,
> each after locating exactly why. That is a worse-looking session log and a
> better-informed next one; the alternative was three more clean zeros with three
> more plausible explanations.

## §921: the array/tuple numeric road was unreachable from the branch that needed it — +14

`gaproot`'s most concentrated root after array literals is
`expression answered error: ElementAccessExpression` (256 lines, 55 cases, top-10
51%). Probing it produced a correction to my own first attempt and then the real
defect.

**First attempt, from reading:** `array_or_tuple_element_access` opens with
`if index_type != number { return None }`, justified as *"a literal index already
answered through the property-name road (in-range)"*. True for a TUPLE, false for
`Array<T>`, which has no property `"0"` — so I admitted numeric literals.
**Measured zero.**

**Then I probed instead of reasoning**, and the probe said the opposite of what I
had assumed: `a[0]` answered `string` and `a[i]` with `i: number` answered
`error`. Three more probes — at the road, at the lookup, at the access — located
it:

> `array_or_tuple_element_access` is called at the TAIL of
> `element_access_lookup`, **after** `let Some(name) = … else { … return error }`.
> So it is reachable only when the index NAMES A PROPERTY — and a plain `number`
> index names none, which is exactly the case the road exists for.

The road had been dead for its own purpose since it was written. Calling it from
the else-branch, before that `return error`, is the fix.

**And the two halves hid each other.** With the call site fixed, `a[0]` still
failed — the literal form dies on the gate above — and the first attempt's
numeric-literal admission is what serves it. Each fix alone reads as "no change";
together they are the road working.

**Measured: 14 `WRONG→RIGHT`, zero adverse.** `conformance/indexerWithTuple` 12,
`unionsOfTupleTypes1` 2.

A pinned refusal went with it: `tests/tuples.rs` asserted `t[idx0]` on a tuple as
`error` under §8's PAIR rule (*"the unported case asserted beside the ported
ones"*). It answers `string | number` now, which is upstream's element union, and
the test is flipped.

> **One probe would have replaced the first attempt entirely.** I read the gate,
> built a theory of which half was broken, and was wrong about which — while the
> four-line probe that settled it cost one build. §911 and §915 both recorded the
> same lesson; this is the first time this session I applied it *after* being
> wrong rather than instead of.

## §922: `new` on a class merged with a namespace — +80 from a wrong `first()`

`gaproot`'s `NewExpression` root is 177 lines across 83 cases and reads as
diffuse. **§921's method settled it in one build**: probe five `new` shapes at
once rather than read the arm.

```
plain=C | ambient=D | ambient-extends=D | MERGED=error | generic=G<number>
```

Only the class **merged with a namespace** fails, and the arm that computes the
answer is correct — its *input* is wrong:

```rust
let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
```

A merged symbol carries both declarations, and `namespace D { … }` comes first in
source order, so the match on `ClassDeclaration | ClassExpression` fell to `_` and
returned `error`. `getDeclaredTypeOfSymbol` reads the class declaration wherever
it sits; searching the declaration list for it is the whole fix.

**Measured: 71 `WRONG→RIGHT` + 9 `GAP→RIGHT`, zero adverse.** `cloduleTest2` 12,
`interfaceClassMerging2` 10, `targetTypeTest1` 10,
`ambientClassDeclarationWithExtends` 2 — the case that started the probe, and by
some distance the smallest beneficiary.

> **`declarations.first()` is a claim that a symbol has one declaration.** It is
> false for every merged symbol — class+namespace, function+namespace,
> interface+class — and the corpus names those cases outright (`cloduleTest`,
> `interfaceClassMerging`, `classFunctionMerging`). **Worth a sweep**: every
> `declarations.first()` in the checker is a candidate for the same defect, and
> this one had been costing 80 lines in eleven cases while reading as correct.

### On method

Three entries in a row were reverted for reasoning from the code's shape (§915,
§919, §921's first attempt). This one reversed the order — five fixtures, one
build, one failing shape — and the fix followed immediately. The arm's own text
gave no hint, because **nothing about it is wrong**.

### §922.1: the `declarations.first()` sweep — clean, and one invalid probe

§922 called for a sweep: **39 `declarations.first()` sites** in the checker, each
a claim that a symbol has one declaration, which is false for every merged one.

Probed rather than read — six merged shapes, both declaration orders:

```
interface-then-class = I     class-then-interface = J
function-then-ns = typeof k  ns-then-function = typeof m
enum-then-ns = typeof E      ns-then-enum = typeof F
```

**All six correct and order-independent.** §922's defect was specific to the `new`
road, where the `first()` result is matched against `ClassDeclaration |
ClassExpression` and falls to `_`. The other 38 sites either do not match on kind
or are reached only for unmerged symbols. **The sweep is clean; the lead is
closed rather than left open.**

### And a probe that lied

Probing eight call shapes in `tests/objects.rs` reported `rest=error` — a call to
`declare function g(...xs: number[])`. It looked like a live defect in the biggest
root on the board.

It is a **harness artefact**: that file's fixtures mount no lib, so `number[]`
cannot resolve `Array` and the *signature* fails before any call. `g;` alone
answered `error`, which is what gave it away. Re-run in `tests/array_literals.rs`,
which mounts `interface Array<T> {}`, every rest form is correct.

> **A probe is only as good as its harness**, and this session has two with
> different libs. The check that costs nothing: **probe the thing you are not
> testing first** — `g;` before `g(1)` — because a harness limitation shows up
> there and a real defect does not.

## §923: an inherited generic member needs TWO substitutions composed — +40

The probe method again, on `gaproot`'s property-access root (418 lines, 122
cases). Eight ordinary shapes — interface, merged, namespace, static, inherited,
`extends`, nested namespace — **all correct**. Six generic-member shapes narrowed
it to one:

```
iface_prop=string    iface_method=(x: string) => string    class_method=ok
ret_self=ok          two_params=ok                         nested=(x: T) => T   ← 
```

`interface D<T> extends C<T> {}` reading an INHERITED member answers
`(x: T) => T`.

Four more fixtures killed the obvious theory (name shadowing between `D`'s `T`
and `C`'s) and found the real split:

| heritage entry | result |
|---|---|
| `extends C<string>` — concrete argument | **correct** |
| `extends C<T>` — the derived's own parameter | `(x: T) => T` |

`generic_heritage_member` instantiates the inherited member for **`base_type`**,
which maps `C`'s `U := T` — the heritage entry's arguments. **The reference's own
arguments (`D<string>`) are a different map and nothing applied them.** Composing
the two, by instantiating the result for `id` as well, is the fix; a non-generic
reference maps nothing, so it cannot disturb what already worked.

**Measured: 37 `WRONG→RIGHT` + 3 `GAP→RIGHT`, zero adverse.** `builtinIterator`
10, `genericClasses3` 6, `genericTypeWithMultipleBases3` 6.

> **Three defects in a row now have the same signature: a road that is correct for
> the shape it was written against and silently incomplete for the neighbouring
> one** — §921's array road (literal index vs `number`), §922's `first()` (one
> declaration vs merged), and §923's substitution (concrete heritage argument vs
> the derived's parameter). In every case the working half is what hid the broken
> one, and in every case **one batch of fixtures separated them in a single
> build**. Reading the code found none of the three: each arm is correct about
> what it does.

## §924: the probe loop run over `gaproot`'s top roots — three paid, three clean, and the method's limit

§921–§923 each landed by probing a root rather than reading it. Running that loop
across the board's ranking:

| root | lines | probe result |
|---|---|---|
| `ElementAccessExpression` | 256 | **§921** — the array road unreachable from the branch needing it, +14 |
| `NewExpression` | 177 | **§922** — `declarations.first()` on a merged symbol, +80, +14 cases |
| property access | 418 | **§923** — inherited generic member needs two substitutions, +40, +7 cases |
| `ArrowFunction` | 935 | **clean** — arg, overload, two-arg, second-arg, object property, nested property all correct |
| `ObjectLiteralExpression` | 135 | **clean** — plain, method, getter, setter, shorthand, spread, computed, nested all correct |
| `reference, the name does not resolve` | 165 | **clean at the shape**: signature parameters type correctly in a function declaration, an interface method, a call signature, a construct signature and a type literal |

**134 lines and 21 cases from three roots; three came back clean.**

### The limit, stated

The clean three are not defect-free — they hold 1,718 gap lines between them. They
are clean *at every shape a unit harness can express*. What the failing rows in
them actually need:

- `ArrowFunction`'s residue is generic inference and overload agreement, which
  needs a contextual signature to materialise — §809's guard declines precisely
  there, and deliberately.
- `reference, the name does not resolve`'s top case (`privacyFunctionParameterDeclFile`,
  90 lines, 63% of the root) is an **external module** with `@Filename:` and
  `@module: commonjs`; its names are module-scoped. The unit harness binds one
  file with no module semantics, so the shape cannot be built there at all — and
  §907 already established its 176-line sibling is a third, separate mechanism.

> **The probe loop is bounded by the harness's expressiveness, and that bound is
> now reached.** Every root whose failing shape fits in a single lib-free file has
> been probed; what remains needs multi-file module fixtures, a real lib, or
> generic inference — i.e. conformance-level fixtures, which is a different
> instrument from the one that found §921–§923.

## §925 — an undocumented gate that refused a namespace's own name (+411)

§924 ended by naming the instrument the probe loop lacked: a way to run one file
through the *real* corpus pipeline — libs mounted, `@Filename:` splits honoured —
rather than through the lib-free single-file unit harness. `probefile`
(`crates/tsr-conformance/examples/probefile.rs`) is that instrument. Its first use
found this.

### The measurement that started it

`compiler/privacyFunctionParameterDeclFile` is the top case of the
`reference, the name does not resolve` root that §924 recorded as **clean** at
every shape the unit harness can express. Bisecting the file with `probefile`
landed on **line 368**, inside `namespace privateModule`, and reduced to:

```ts
namespace c { export class K {} export interface I { m(p: c.K): void } }   // m : error
namespace a { export class K {} }
namespace b { export interface I { m(p: a.K): void } }                     // m : (p: a.K) => void
```

Same shape, same qualified name, two answers. The only difference is whether the
reference is written *inside* the namespace it qualifies.

### The cause

`qualified_type_reference` in `crates/tsr-checker/src/declared.rs` opened with:

```rust
let Some(site) = node.node_id else { return error };
if self.site_is_inside_namespace(site, namespace) {
    return error;
}
```

**Undocumented.** No upstream anchor, no ADR, no note here. Upstream has no such
rule: `resolveEntityName` walks the scope chain, and a namespace is in scope
inside itself.

The gate *was* documented, in exactly one place — a comment in
`crates/tsr-checker/tests/qualified_type_reference.rs`, which recorded
*"Removing the refusal turns this test green and adds **222** wrong lines to the
corpus, measured"*, and said in the same breath *"This gap is not permanent"*.
That is the correct way to record a refusal; what was missing was the same note at
the code it governs, where the next reader would meet it.

### The ratio has flipped

The −222 that justified the gate no longer holds. Measured at this commit,
removing it:

| transition | lines |
|---|---|
| `WRONG->RIGHT` | 320 (`bluebirdStaticThis` 69, `complexRecursiveCollections` 51, `resolvingClassDeclarationWhenInBaseTypeResolution` 51) |
| `GAP->RIGHT` | 92 (`privacyFunctionParameterDeclFile` 50) |
| `GAP->WRONG` | 48 |
| `RIGHT->WRONG` | 1 |

`right` 441,343 → **441,754**, net **+411**.

Two years of unrelated fixes stand between the two measurements; the gate was
paying for a deficiency elsewhere that has since been repaired. **A refusal
measured once is a fact about that commit, not about the code.** The notes here
carry dozens of them; this is the first to be re-measured and found inverted, and
it will not be the last.

### What was refused, and what survives

The correctness concern the refusal named is real and survives as the 48
`GAP->WRONG`: inside `privateModule`, upstream prints `publicClass` where this
port now prints `privateModule.publicClass`. That is `needsQualification`
(`vendor/typescript-go/internal/checker/symbolaccessibility.go:688`) — upstream
emits the *shortest* name that resolves from the reference site, so a name already
in scope needs no qualifier. Porting it converts those 48 directly, and it is the
named completion of this entry.

**48 knowingly imprecise names against 412 recovered ones is the trade taken**,
and it is `GAP->WRONG` — §620's accepted adverse direction, not `RIGHT->WRONG`.
The single `RIGHT->WRONG` row is inside that same printing question.

### Falsifier

If `needsQualification` lands and the 48 do *not* convert, the diagnosis of the
adverse rows is wrong and this entry needs correcting, not extending.

### Consequences

- `site_is_inside_namespace` and its `is_inside` helper are now dead and deleted.
- `tests/qualified_type_reference.rs`'s pinned refusal test is flipped to
  `a_reference_inside_the_namespace_it_qualifies_now_resolves`, asserting
  `"privateModule.publicClass"` — the *current* answer, imprecise name and all, so
  that `needsQualification` landing shows up as a test that must be edited rather
  than one that silently keeps passing.
- `tests/objects.rs` gains `a_qualified_name_rooted_at_an_enclosing_namespace_resolves`
  and `a_qualified_name_across_namespaces_is_unchanged` — the second is the
  regression leg: the road that already worked must keep working.

## §926 — `needsQualification`, and the reuse half that made it safe (+137, zero adverse)

§925's named completion. The bar registered before code: **primary** — the 48
`GAP->WRONG` §925 accepted convert; **safety** — zero `RIGHT->WRONG`;
**falsifier** — if the 48 do not convert, §925's diagnosis of its own adverse
rows was wrong; **regression** — a qualified name written from outside its
namespace is unchanged.

### The rule

`symbolToString` never prints the written text. It builds an accessible symbol
chain, and `canQualifySymbol` (`symbolaccessibility.go:676`) prepends the parent
only when `needsQualification` (`:688`) finds the bare name taken by something
else. Inside `namespace privateModule`, `privateModule.publicClass` prints as
`publicClass`.

Ported as `qualification_free_name` in `declared.rs`: resolve the RIGHTMOST
identifier from the reference site with meaning `TYPE`; if it merges to the same
symbol the qualified name resolved to, print the bare name.

**Only the bare/qualified decision is ported, not the chain.** Upstream, when the
name *is* taken, recurses on the parent and can still produce a chain shorter
than what was written. Here a taken name simply keeps the written text. The two
agree wherever the written path is already the accessible one; they diverge on
`A.B.C.T` written from inside `A.B`, where upstream prints `C.T`. Nothing in the
corpus measured that shape, so it is left out rather than guessed at.

> **CORRECTED by §926.1.** *"Nothing in the corpus measured that shape"* was
> written **without measuring it** — it was a guess phrased as a finding. The
> chain is now ported and measured: **1 `WRONG->RIGHT`, zero adverse.** Nearly
> right, and not a measurement.

### The first measurement failed the safety leg, and said exactly why

+48 net, but **75 `RIGHT->WRONG`**. The diff is worth reproducing because the
discriminator is visible on *adjacent rows of one file*:

```text
WR 354 | param : publicClass
RW 356 | (param: privateModule.publicClass) => void
WR 357 | param : publicClass
```

Same parameter, same symbol, two spellings. **Every `WRONG->RIGHT` was a
standalone type print; every `RIGHT->WRONG` was inside a printed signature.**

That is `serializeTypeForDeclaration` — building a signature node **reuses the
written annotation node** rather than re-printing the computed type. The port
already models this as `Parameter::written_text`; the field simply was not being
populated for this case.

So `qualified_written_text` (`NodeId -> String`) records the written spelling
whenever the printed name was shortened, and `written_annotation_text` consults
it. 75 adverse rows → **26**.

### The 26 were one shape, and it named the composition bug

All 26 were a shortened reference used as a **type argument**:
`expected C.A<C.B>`, `got C.A<B>`. The generic mint was composing its written
form out of the *rendered* arguments, which are already shortened. It now
composes from the argument **nodes**, consulting the same map. 26 → **1**.

### The last one, and a doc corrected

`complexRecursiveCollections` wants `maybeRecord is Record.Instance<any>`. The
rustdoc on `type_predicate_to_string` asserted that upstream renders a predicate
from the **computed** type, "not the written node". That is right in general and
wrong here; `TypePredicate` now carries the same `written_text`, populated only
by the shortening. **One row is thin evidence for a rule**, so nothing else about
predicate printing changed, and the doc now says so rather than being silently
edited.

### Final measurement

**137 `WRONG->RIGHT`, zero `RIGHT->WRONG`, zero `GAP->WRONG`.** `right` 441,754 →
**441,891**. `resolvingClassDeclarationWhenInBaseTypeResolution` 71,
`privacyFunctionParameterDeclFile` 40, `privacyVarDeclFile` 20.

The falsifier resolved in favour of §925: its 48 converted exactly as predicted.

### Two consequences worth keeping

- Widening `TypePredicate` by one `Option<String>` pushed `Signature` past
  `clippy::large_enum_variant` in two local enums in `calls.rs`, now
  `Picked(Box<Signature>)`. A three-field struct's size is load-bearing at a
  distance; this is the note that says why the box is there.
- **§925 and §926 together are one defect in two halves, and the first half
  looked finished.** §925 landed +411 with 48 adverse rows it named, justified
  and accepted under §620. Those 48 were not a residue to live with — they were
  the other half of the same mechanism, and finishing it cost less than the
  entry arguing for accepting them. *An accepted adverse bucket with a named
  cause is a work item, not a conclusion.*

## §927 — a union contextual type, and the two guards that made it safe (+129)

### How the board was re-chosen

§924 closed the probe loop over `gaproot`, and it was the wrong board. Measured
at §926's commit:

| pool | lines |
|---|---|
| gap (we answer `error`) | 6,195 (1.29%) |
| **wrong (we answer something)** | **26,143 (5.46%)** |

The gap pool is not large enough to matter. Clustering the wrong lines by the
answer we print:

```text
15233  any          <- 58% of every wrong line
  470  string
  293  T
  265  number
```

`any_audit` then splits that 15,233, and roughly **9,300 of them are the checker
answering `error` with the producer printing `any`** — gaps wearing a costume.
The true deficit is therefore ~15,500 lines, not 6,195, and it is diffuse: the
largest single row is 1,692 lines over 372 cases with a top-1 share of 4.6%.

**The instrument that found this entry was not a root-ranker but a clustering of
the wrong answers themselves.** `gaproot` cannot see any of it, because none of
these lines are gaps.

### The defect

`conformance/contextualTypeWithUnionTypeMembers` is 136 wrong lines, and
`probefile` reduces it to five:

```ts
interface I1<T> { m(a: string): string; p: string; g(a: T): T; }
interface I2<T> { m(a: string): string; p: string; g(a: T): T; }
var single: I1<number>             = { m: a => a, p: "h", g: a => a };  // correct
var both:   I1<number> | I2<number> = { m: a => a, p: "h", g: a => a };  // every member `error`
```

Two halves, both of them *a capability present and a caller that does not consult
it* — the twelfth and thirteenth instances this session:

1. `contextual_type_for_object_literal_element` called `get_property_of_type`,
   which finds nothing on a union. `contextual_property_type` — the ported
   `getTypeOfPropertyOfContextualTypeEx` union walk, complete with §98's guard
   and an intersection arm — was sitting unused. **Its own comment said the port
   did not map over unions.** It does; this caller did not reach for it.
2. With the member type found, `contextual_signature`'s union branch declined the
   moment a *second* constituent offered a signature. Two interfaces declaring
   the same member is the shape the corpus writes, and the signatures are
   identical every time. `compareSignaturesIdentical` (`relater.go:3103`) is now
   ported as `signatures_identical`, conservatively: same kind and shape, every
   corresponding type the same interned `TypeId`, and generics decline outright.

**Measured alone, `signatures_identical` moves nothing.** It is only reachable
once the first half supplies a union-derived member type — which is why the two
land together.

### The two guards, each named by the case that demanded it

The unguarded pair measured **+133 with 19 `RIGHT->WRONG`**, in two families:

- **A primitive constituent** (4 rows).
  `compiler/contextualOverloadListFromUnionWithPrimitiveNoImplicitAny` is a
  regression test for exactly this: with `type Rule = string | FullRule`,
  upstream supplies *no* contextual type and the parameters are implicit `any`.
  The case is named for the error it expects.
- **A unit answer out of a multi-constituent union** (15 rows).
  `missingDiscriminants` writes `const item1: Item = { subkind: 1, kind: "b" }`
  where the constituents declare `subkind: 0` and `subkind: 1`. Upstream
  discriminates on `kind: "b"` to the constituent with no `subkind` at all, so
  there is no contextual type and the literal widens to `number`. The
  undiscriminated walk unions `0 | 1` and keeps `1` fresh.

  **Which constituent governs a literal-valued member is precisely what
  `discriminateTypeByDiscriminableItems` (`checker.go:30779`) decides**, and it
  is unported. This is §98's `mixed_unit_and_base` generalised from
  literal-versus-base to literal-versus-literal — the same question.

  The first draft of this guard tested whether the *answer* was a unit type; the
  answer is `0 | 1`, a union **of** units. Testing the leaves is what
  `mixed_unit_and_base` already does, and for this reason.

Guard two costs 11 wins to remove 14 adverse rows. **Removing both guards is how
you would know discrimination had landed** — they have no other purpose.

### Final measurement

**123 `WRONG->RIGHT` + 6 `GAP->RIGHT` against 3 `GAP->WRONG`, zero
`RIGHT->WRONG`.** `right` 442,020. `contextualTypeWithUnionTypeMembers` 86,
`contextualTypeBasedOnIntersectionWithAnyInTheMix4` 12,
`checkExportsObjectAssignProperty` 8. The 3 adverse rows
(`jsDeclarationsGetterSetter`) go `error` → `(v: any) => void` where upstream has
`(v: number) => void` — §620's accepted direction.

### The test harness caught us in its own documented trap

`tests/contextual.rs` opens with a warning that its `type_of` walk returns the
**first** symbol of a name, so a fixture naming its arrow parameter the same as
the annotation's parameter "passed against a checker that could not have known
the answer". Every fixture written for this entry did exactly that, and two of
them failed *because* of it — asserting `any` while the annotation handed back
`string`. `probefile` gave the real answers and the annotations were renamed.

**A trap documented at the top of a file is not a trap that has been removed.**
The fixtures here now name the annotation's parameter `a` and the arrow's
parameter something that appears nowhere else.

## §928 — an object-literal method's contextual `this` (+7, and one refusal)

§912's comment in `check_this_expression` named this branch and did not build it:

> Upstream's first branch — the method's own contextual SIGNATURE carrying a
> `this` parameter — wins ahead of the literal one

`getContextualThisParameterType` (`checker.go:29104`) asks for the method's own
contextual signature before either object-literal branch. So

```ts
interface I { a: number; em(this: { a: number }): number }
let impl: I = { em() { return this.a; } };   // this : any
function justThis(this: { y: number }) { return this.y; }   // this : { y: number; } — correct
```

The written annotation worked; the same annotation reached through a contextual
signature did not. **Fifteenth instance this session of a capability present and
a caller that does not consult it** — `contextual_property_type` finds the
member, `contextual_signature_of_type` reads its signature, and
`Signature::this_parameter` has held the answer since the signature module was
written.

### The gate that made the first draft measure zero

The first draft put the lookup inside §912's match arm, which is gated on
`no_implicit_this`. It measured **zero transitions**, because
`conformance/thisTypeInFunctions` is `@strict: false`.

Upstream gates only the object-literal *fallback* on `noImplicitThis`
(`checker.go:29119`). The contextual signature's own `this` is an annotation the
user wrote, and it is read in every mode. Hoisted out of the arm: **7
`WRONG->RIGHT`, zero adverse.**

### The printing half, refused with its number

`assignContextualParameterTypes` (`checker.go:25344`) also copies that `this`
parameter *onto the signature*, which is why upstream prints
`explicitStructural(this: { a: number; }): number` for a method that wrote no
`this` parameter at all. Adopting it measured **10 `WRONG->RIGHT` against 12
`RIGHT->WRONG`, net −2**.

The adverse cases name the missing gate outright:
`thislessFunctionsNotContextSensitive1`/`2` (7 rows) and
`intraExpressionInferences` (4). Upstream copies only where the signature is
**context sensitive** (`isContextSensitive`, `checker.go:25211`), and those cases
exist precisely to assert that a thisless function is not. This port has no
`isContextSensitive`, so the copy fires everywhere.

**Reopening condition: `isContextSensitive`.** The refusal is recorded at the
site in `signatures.rs` rather than only here, because that is where the next
reader meets it — the lesson §925 paid for.

> **SUPERSEDED BY §928.1, within the hour.** The reopening condition was met by
> reading `HasContextSensitiveParameters` two files further. This entry is left
> as written; §928.1 carries the correction, including one claim above that is
> wrong.

The `this` *type inside the body* does not depend on the printing half, which is
where the +7 comes from; only the printed signature waits.

### On the test harness, again

`tests/contextual.rs`'s first §928 fixture asserted `type_of(…, "impl") == "I"`,
which is true whether or not `this` resolves — **a test no mutation could
redden**. It now asserts a `let` binding inside the method body, which is the
only thing this harness can see that the fix moves.

Its pair asserts `error` where the corpus pipeline answers `any`, and says so:
the harness binds one lib-free file with no compiler options and the `this` road
reaches a different fallthrough there. Both answers mean *the branch did not
fire*, which is what the pair is for; asserting the corpus's answer against a
harness that does not produce it would be asserting a coincidence.


## §928.1 — the refusal reopened, and a claim of §928's corrected (+3)

### The correction first

§928 wrote that its new branch "sits outside the `no_implicit_this` gate
deliberately" and that "the contextual signature's own `this` is an annotation
the user wrote and is read in every mode". **The first half is right and the
second overstates it.** Upstream's branch is gated —
`getContextualThisParameterType` (`checker.go:12021`) reads

```go
if c.isContextSensitiveFunctionOrObjectLiteralMethod(fn) {
    contextualSignature := c.getContextualSignature(fn)
    ...
```

— just not on `noImplicitThis`. §928 shipped that branch **ungated**, measured
+7/0, and did not check for a gate it had already been told about by name in its
own refusal, three paragraphs later. The gate is recorded now.

### The gate, which was two files away

`isContextSensitiveFunctionOrObjectLiteralMethod` (`checker.go:29496`) →
`isContextSensitiveFunctionLikeDeclaration` (`:30931`) →
`HasContextSensitiveParameters` (`ast/utilities.go:4196`), whose whole content
for a non-arrow with no explicit `this` parameter is:

```go
// Functions with type parameters are not context sensitive.
if node.TypeParameters() == nil {
    if core.Some(node.Parameters(), func(p *Node) bool { return p.Type() == nil }) { return true }
    if !IsArrowFunction(node) {
        parameter := core.FirstOrNil(node.Parameters())
        if parameter == nil || !IsThisParameter(parameter) {
            return node.Flags&NodeFlagsContainsThis != 0
        }
    }
}
```

**A method is context-sensitive exactly when its body mentions `this`.** That is
literally what *thisless* means in `thislessFunctionsNotContextSensitive1`/`2` —
the two cases whose 7 adverse rows refused §928's printing half. The gate was
named in their titles and in the refusal, and it took one more file to find.

The binder has recorded the bit all along, as `NodeFacts::CONTAINS_THIS`.

### What each caller needs

- `check_this_expression` (§928's type branch): the `ContainsThis` half is
  **satisfied by construction** — that function only runs on a `this` written in
  the body. Only the type-parameter half is a real test, and adding it changed
  nothing measured, which is the expected result for a faithful gate with no
  population.
- `get_signature_from_declaration` (the printing half): has no such guarantee and
  tests the bit explicitly.

### Measurement

Gated, the printing half is **+3 `WRONG->RIGHT`, zero adverse** — where ungated
it was 10 against 12. The gate removed **every** adverse row and kept 3 of the 10
wins; the other 7 want something further.

`right` 442,027 → **442,030**.

### What this says about the method

§928's refusal was correct, complete, and had the answer inside it: it named
`isContextSensitive`, cited `checker.go:25211`, and stopped. **A refusal that
names its own reopening condition should be followed for one more hop before it
is written down** — the cost here was one `grep`, and the entry arguing for the
refusal was longer than the fix.

## §926.1 — the chain §926 guessed had no population (+1)

§926 ported `needsQualification` as a bare/qualified decision and wrote that
upstream's chain-shortening — `A.B.C.T` written inside `A.B` prints `C.T` —
had no population in the corpus, "so it is left out rather than guessed at".

**That sentence was itself the guess.** No probe was run; the shape was assumed
absent because the entry's own wins came from the bare case.

The chain is now the loop it should have been: walk suffixes of the written path
from shortest to longest, resolving each from the reference site, and print the
first that reaches the same symbol. `A.B.T` written inside `A` prints `B.T` —
bare `T` does not resolve there, `B.T` does.

**Measured: 1 `WRONG->RIGHT` (`compiler/moduleAndInterfaceSharingName4`), zero
adverse.**

One line is a small return for forty lines of walk, and that is the honest
accounting. What it buys beyond the line is that the printed name is now
upstream's *rule* rather than a special case of it, and the sentence in §926 is
a measurement instead of an assumption. **A claim about the corpus costs one
`scorepair` run; one was not made, and the cheapest possible check would have
caught it.**

## §929 — one unreadable part was taking out every readable one (+469)

**The largest single move of this session, and it came from a misattributed row
in an instrument that already existed.**

### How it was found

The board was re-ranked by joining `any_audit`'s per-line dump
(`TSR_ANY_DUMP=1` → `target/any_lost_lines.tsv`, 15,127 rows) against the chain
each row carries, aggregated **by the deepest component of the chain** rather
than by the whole string. The first attempt aggregated on the wrong end and
returned the cascade suffix ("the branch answered ERROR") for 9,629 lines — a
root that says nothing. Re-joined on the branch itself:

```text
  1692   372 cases  expression answered `any`: CallExpression
  1347   310 cases  declaration name -> self-referential initialiser
   935   280 cases  property access: the property's own type is `any`
   ...
   423   135 cases  declaration name -> shorthand ambient module
```

The `shorthand ambient module` row was **misattributed** — its rows want things
like `<T>(value: T) => MaybePromise<T>` and `(a: Array) => void`, which are not
ambient modules at all. They are **function signatures that failed to build**.
*A misattributed row was worth more than the correctly attributed ones, because
it was the only one naming a mechanism instead of a position.*

### The defect

```ts
declare function f(a: Array): void;   // was: error       upstream: (a: Array) => void
declare function g(x: number): Array; // was: error       upstream: (x: number) => Array
function h<T extends string[]>(): void {}  // was: error
```

`parameter_of`, `return_type_of` and `type_parameter_of` each returned `None`
when their annotation did not resolve, and `get_signature_from_declaration`
propagates every `None` with `?`. **One unreadable annotation took out the whole
signature, and with it every readable parameter in it** — and then the symbol,
the declaration, and everything downstream.

Upstream does not do this. The parameter carries `errorType`, and
`serializeTypeForDeclaration` **reuses the written annotation node**, so the
signature prints in full. The port already had that channel: §926's
`qualified_written_text` is exactly "the written spelling to reuse for this
node", already consulted by `written_annotation_text`.

So all three sites now keep the part, give it `any` (this port's stand-in for
`errorType` at printing positions — the producer already converts one to the
other, `types_producer.rs:434`), and register the written spelling.

**`None` is still returned when the annotation has no printable text.**
Inventing a spelling would be worse than the gap.

### Three measurements, in order

| change | W→R | G→R | G→W | R→W |
|---|---|---|---|---|
| parameter (symbol road) | 88 | 76 | 24 | **0** |
| + return annotation | 251 | 107 | 68 | **0** |
| + type-parameter constraint | 314 | 128 | 77 | **0** |
| + the map lookup widened past `TypeReferenceNode` | **325** | **144** | **61** | **0** |

`right` 442,031 → **442,500**. `complexRecursiveCollections` 85,
`bigintWithLib` 46, `returnTypeTypeArguments` 21.

The last row is worth its own note: the constraint half printed
`<T extends any>` until `written_annotation_text`'s map lookup was widened from
`TypeReferenceNode` to any annotation node. **A channel that only carries one
node kind silently drops the others**, and the unit test caught it where the
corpus's 61 G→W did not.

### What was applied and measured at zero

The same rule at `parameter_of`'s **binding-pattern** road measured **zero
transitions** and is *not* kept — untested code mirroring a measured one is a
liability, and the zero is the useful record. It is noted at the site.

### Six pinned tests inverted, and none quietly

§929 contradicts a rule six tests existed to assert, one of which said outright
*"the reason this arm cannot be written as a fallback"*. It can be. Each test is
updated in place with the measurement, the old claim quoted, and — where the
property it was really testing no longer has a population in this port — **that
said plainly rather than a replacement fixture invented**.

Four of the six changed only because **this harness mounts no lib**, so
`string[]` and `T[]` cannot resolve `Array` and §929's road fires where the
corpus's libs mean it never would. Each of those says so, and says what the
corpus measured instead. *A test fixture that is a gap only because the harness
is thin is not evidence about the checker*, and three of these tests had been
quietly relying on exactly that.

## §930 / §930.1 — §929's rule one level up, and the accessor arm (+74)

§929's finding was structural, so the question it raises is where else the same
shape lives. `get_type_from_type_literal` is the obvious next floor: it carries an
explicit *all-or-nothing* rule, stated at one of its own decline sites as

> A signature this port cannot build is a gap for the *whole* literal … a partial
> object type is a wrong answer that looks like a right one.

That rule is sound for a member that would be **dropped**. It is not sound for a
member whose annotation merely fails to resolve, because upstream does not drop
that member — it gives it `errorType` and prints the written annotation node.

### §930 — a property signature (+33)

`{ a: string; b: Array }` printed `error`, **losing the perfectly good `a`**. The
member now keeps its written spelling with an `any` type, recorded through the
same channel §929 uses. `Member::Property` carries a `printed: String`, so the
spelling drops straight in.

**28 `WRONG->RIGHT` + 5 `GAP->RIGHT` against 3 `GAP->WRONG`, zero
`RIGHT->WRONG`.** `normalizedIntersectionTooComplex` 18.

### §930.1 — an accessor (+41 more)

A `GetAccessorDeclaration` or `SetAccessorDeclaration` in a type literal fell to
the trailing `_ => return error` and took the whole literal with it. Upstream
resolves a get/set pair to **one property symbol** and the node builder prints it
as a property:

```ts
var x: { get a(): string };                      // { readonly a: string; }
var y: { get b(): string; set b(v: string) };    // { b: string; }
var z: { set c(v: number) };                     // { c: number; }
```

`readonly` comes from **there being no setter**, not from get-ness — which is why
`z` is writable and `y` is not readonly. A setter whose pair also declares a
getter contributes nothing, or the member would print twice; both are asserted.

**57 `WRONG->RIGHT` + 17 `GAP->RIGHT` against 5 `GAP->WRONG`, zero
`RIGHT->WRONG`** (cumulative with §930). `normalizedIntersectionTooComplex` 18,
`divergentAccessorsTypes1` 12, `circularAccessorAnnotations` 8.

`right` 442,500 → **442,574**.

### The all-or-nothing rule survives, narrowed

Only the *unresolvable-annotation* reason is removed. Every other `return error`
in that function stands, and they are the reason the rule is still stated there. A
member this port would have to **invent** a spelling for still declines the
literal whole — that is the line, and it is the same line §929 drew.

### Four more pinned tests moved, and one retired

- `signature_members.rs`'s `a_member_this_port_still_cannot_render_gaps_the_whole_literal`
  has **no subject left**: every assertion it held has moved, in three steps
  (`bd tsr-eep`, §929, §930.1). The name is kept so that is visible, and the
  entry says plainly that *this file no longer has an example of the rule* rather
  than hunting a fixture until one gapped.
- `alias_naming.rs`'s `naming_pays_only_where_the_body_computes` has now watched
  its frontier advance **three times and been wrong none of them** — which is
  what it was written to detect. Its own comment already said "twice now"; this
  is the third.
- `types.rs`'s `an_object_type_with_a_member_this_port_cannot_render_is_a_gap`
  inverted.

**Across §929 and §930 that is ten pinned tests whose subject the port outgrew.**
Every one was a correct record of a real limit at the time it was written. The
pattern worth naming: *a test that pins a limitation is a liability the moment the
limitation is structural rather than semantic*, because the structural fix moves
all of them at once and each has to be re-reasoned separately.

## §930.2 / §930.3 — where §929's rule stops, with the measurement and the reason

§929 and §930 removed an all-or-nothing rule two floors in a row, so the honest
next question is not "where else?" but **"where does the argument actually
hold?"** Two floors were tried and both are refused, for *different* reasons, and
both refusals are worth more than the third win would have been.

### §930.2 — the object literal, refused by measurement (−315)

`objects.rs`'s member loop carries the same rule and §146 had already opened it
for one population (an unresolved bare identifier, TS2304, where upstream
genuinely answers `any`). Opening it for every error member measured

| | lines |
|---|---|
| `WRONG->RIGHT` | 23 |
| `GAP->WRONG` | **335** |
| `RIGHT->WRONG` | **3** |

`correlatedUnions` 20, `mappedTypeContextualTypesApplied` 18,
`contextualTypeWithUnionTypeIndexSignatures` 16. Reverted.

**The difference is what there is to print.** An annotation that fails to resolve
still carries the text the user wrote, and upstream prints exactly that — §929's
`any` sits *behind a faithful spelling*. An expression that fails carries no such
text, so `any` there is not a placeholder but an **invention**, and 335 rows say
so. §146's narrowness was earned.

### §930.3 — a tuple element, refused without measuring, on shape

A tuple element is not a printed slot. It is a real `TypeId` in `elements`,
consumed by access, instantiation and the relater. Substituting `any` would print
`[string, any]` *and* hand a wrong element type to every consumer.

The premise fails as well: the elements this port cannot resolve are largely ones
upstream **can**. `keyof string` is a real union upstream, not an error it prints
verbatim — so printing the written text would be inventing upstream's answer
rather than reproducing it.

**Deliberately not measured.** §926.1 is the standing lesson that a claim about
the corpus costs one `scorepair` run, and it applies to claims of *absence*. This
is not that: the shape makes the change wrong whatever it scores, and a favourable
number would only mean the corpus has not yet asked the question. The reopening
condition is *resolving* the element, not routing around it.

### The rule, stated once

> §929's rule applies exactly where the port has **upstream's own answer already
> written down** — an erroneous annotation, whose text upstream reuses. It does
> not apply where `any` would stand in for something upstream computes.

Three floors, two refusals, one of them without a measurement and saying why.
*The argument that produced the session's largest win is also the argument that
bounds it*, and writing the boundary down is what stops the next session
re-deriving it from a −315.

## §931 — `getUnionSignatures`, attempted and refused (−20)

The board's largest remaining root is `expression answered any: CallExpression`,
1,650 lines over 372 cases. Its most *nameable* case is
`conformance/unionTypeCallSignatures` (45 lines), and it is adjacent to §927:
§439 already handles a union callee, but only when every constituent's return
**agrees**.

Upstream does not require agreement. `getUnionSignatures` (`checker.go:9560`)
keeps the signature set when it is identical *ignoring return types* and unions
the returns:

```ts
declare var u: { (a: number): number } | { (a: number): Date };
u(10)   // number | Date
```

§927's `signatures_identical` minus its return comparison is exactly upstream's
predicate, so the widening looked like a three-line change on top of work already
done.

### Measured: 6 `WRONG->RIGHT` against 26 `RIGHT->WRONG`

`unionTypeCallSignatures4` 12, `mismatchedExplicitTypeParameterAndArgumentType`
4, `functionCallOnConstrainedTypeVariable` 2. Reverted.

### Why the shortcut is not the mechanism

§439's arm asks each constituent to resolve a signature **independently** and then
combines what comes back. Upstream builds the union type's **own signature list**
first and runs *one* overload resolution over it, with argument assignability
deciding which member applies. The two diverge the moment two constituents would
select *different* overloads for the same argument list: the arm then unions two
returns upstream never combines.

The regressed rows print `any` where upstream prints a real type. An
`any`-returning-constituent guard was tried against them and changed **nothing** —
which is the tell. *The `any` was never coming from the union; the synthetic
signature was simply the wrong signature.* A guard that fixes none of the rows it
was written for is evidence about the diagnosis, not a knob to keep turning.

**Reopening condition: `getUnionSignatures` proper** — build the union's signature
list, then run the existing overload road over it. Not a wider return rule.

### The predicate was deleted, not parked

`signatures_identical_ignoring_return` is correct and now unreachable, so it is
gone. Keeping a correct-but-dead helper "for later" is the liability §929 named
when it declined to keep its own zero-measuring copy: three lines cost less to
rewrite than a dead function costs to keep trusting — and `clippy -D warnings`
agrees, which is how the choice surfaced at all.

### Three refusals in a row, and what that says

§930.2 (measured, −315), §930.3 (refused on shape, unmeasured), §931 (measured,
−20). The structural seam §929 opened is **worked out**: of five floors tried,
two paid (+543 together) and three do not. What is left on this board is the
mechanism itself — `getUnionSignatures`, `discriminateTypeByDiscriminableItems`,
`inferTypes` for non-bare positions — and each is a real port, not a routing fix.

## §931.1 — `getUnionSignatures`' first pass, done properly (+15)

§931's reopening condition, met in the same session that wrote it. The refusal
said: *"`getUnionSignatures` proper — build the union's signature list, then run
the existing overload road over it. Not a wider return rule."* That is what this
is.

### The one requirement §931 was missing

`getUnionSignatures` (`checker.go:21112`) walks every signature of every
constituent and, for each, calls `findMatchingSignatures` (`relater.go:2119`),
which returns nothing unless the signature **matches in every other list**. Only
then is a result signature emitted, with a union of the matched returns.

§931 asked each constituent to resolve a signature *independently for the call's
arguments* and unioned whatever came back. Nothing checked that the constituents
had agreed on the same signature **shape**, so it combined returns upstream never
combines: 6 `WRONG->RIGHT` against **26 `RIGHT->WRONG`**.

With the match-in-every-list requirement: **11 `WRONG->RIGHT` + 4 `GAP->RIGHT`
against 2 `GAP->WRONG`, zero `RIGHT->WRONG`.** `right` 442,574 → **442,589**.

*One predicate was the entire difference between −20 and +15.*

### The guard the measurement added

A **type predicate** anywhere in the build declines it. Upstream's union
signature carries a *composite* predicate over the members
(`getUnionOrIntersectionTypePredicate`, `relater.go:2049`), which
`checker-notes-typepred.md` §1 records as unported. Keeping the shape's own
predicate instead measured 1 `RIGHT->WRONG` —
`typePredicatesInUnion3:0:38`, `unknown` → `string` — because the narrowing road
then trusted **one member's predicate for the whole union**. That is a wrong
answer of exactly the kind §620 disqualifies, and it cost 2 lines of the 13.

### Not ported, and each is a `None` rather than an approximation

- **The second pass** (`checker.go:21153`): when no signature subsumes the others
  and overloads live in at most one constituent, upstream builds one combined
  signature by *intersecting* parameter types
  (`combineUnionOrIntersectionMemberSignatures`). Needs parameter intersection.
- **Generic signatures.** Upstream requires an exact match including returns and
  only from the first list. `signatures_identical` declines generics outright, so
  a generic anywhere declines the whole build rather than half-answering.
- **`thisParameter` intersection** (`checker.go:21137`).

### A representational limit this exposed, worth recording

`{ (a: number): number } | { (a: number): Date }` — the fixture's own first line —
**still answers `error`**, and not because of this arm. A type literal carrying a
call signature is minted as a print-only named type with no symbol, so its
signature is not recoverable from the type at all; `call_signatures_of_type`
answers `None` for it. Every row this entry won came through *function type
aliases*, where the signature survives.

That is a separate, larger defect — the same one §439 has always been limited by —
and it is now named: **a type literal's call signature is unrecoverable from its
type.** Anything that needs signatures off such a type is blocked on it.

> **CORRECTED by §932, immediately.** It is not a representational limit and
> nothing was blocked on it. The signature *is* recorded on the type, in
> `signature_types`, by §10.15's own collapse — **two readers simply did not
> consult it.** Calling this "representational" was a diagnosis made from one
> failing probe without opening the mint that produced the type.

### On the two adverse rows

`unionTypeCallSignatures7` wants `"A with id" | "B with id"` and gets
`` `${Name} with id` | `${Name} with id` `` — a template-literal type that was not
instantiated. `GAP->WRONG`, §620's accepted direction, and a template-literal
instantiation defect rather than a signature one.


## §932 — the "representational limit" was a third unconsulted capability (+67)

§931.1 closed by naming a limit: *"a type literal carrying a call signature is
minted as a print-only named type with no symbol, so its signature is
unrecoverable from the type."* **Every clause of that is wrong**, and the
correction is the entry.

### What was actually happening

Probing for the boundary instead of asserting it:

```ts
declare var a: { (a: number): number; x: string; };  const ra = a(10);  // number  ✓
declare var b: { (a: number): number; };             const rb = b(10);  // error  ✗
declare var c: { new (a: number): number; };         const rc = new c(10); // number ✓
```

**A sole call signature.** The extra member in `a` means no collapse; `c` goes
down the `new` road. §10.15's single-signature collapse mints the type as
anonymous, prints it in arrow form, and records the signature in
`signature_types` — but **not** in `minted_signature_types`, so the call road's
`is_instantiated_signature_type` test answered `false` and never looked. Control
fell to `get_signatures_of_symbol`, which reads the `__type` symbol's
declarations, and a `TypeLiteralNode` is not signature-shaped, so it answered
`None`.

The type had its signature the whole time. **`getSignaturesOfType`
(`checker.go:18959`) reads the *type's* signatures** — consulting
`signature_types` first is upstream's own order, not a fallback.

This is the **sixteenth** instance this session of *a capability present and a
caller that does not consult it*, and the third in this family after §927 and
§928. It is also §921/§922/§923's signature exactly: a road correct for one shape,
silently incomplete for the neighbouring one, **with the working half hiding the
broken one**.

### The kind filter the measurement demanded

§10.15 stores whichever signature the literal declared. Read unfiltered it
answered a **construct** signature for a plain call: 3 `RIGHT->WRONG`, one of them
in a case named for precisely that confusion —
`objectTypeWithConstructSignatureAppearsToBeFunctionType`, where upstream reports
and answers `any`. *When a regression lands in a case named after the mistake you
just made, the case name is the review.*

### Measurement

**57 `WRONG->RIGHT` + 10 `GAP->RIGHT` against 1 `GAP->WRONG`, zero
`RIGHT->WRONG`.** `right` 442,589 → **442,656**.
`unionTypeCallSignatures` 25 — §931.1's own fixture, whose first line it had
written off — `functionTypeArgumentAssignmentCompat` 6,
`genericFunctionCallSignatureReturnTypeMismatch` 6. The same read was wired into
§931.1's union builder, which is where those 25 come from.

### The lesson, and it is about me

§931.1 had the probe in front of it: `{ (a: number): number } | { … }` answered
`error`, and *every row it won came through function type aliases*. From that it
concluded the type could not carry a signature. **The cheaper explanation — that
something did not read what was there — was not checked, and it had already been
the answer fifteen times in this session.**

A negative result about the port's *representation* deserves the same standard as
a claim about the corpus (§926.1): open the mint. Naming a limit is the most
expensive kind of wrong record, because the next session reads it and does not
look.

## §932.1 — the audit §932 called for, and its one remaining instance (+25)

§932 closed by saying the other readers of `signature_types` /
`minted_signature_types` had not been audited, and that the pattern had paid
sixteen times. The audit is five `grep` hits and took less time than the entry
arguing for it.

### The readers, and what each does

| reader | gate | verdict |
|---|---|---|
| `calls.rs:1409` | `is_instantiated_signature_type`, then §932's direct read | fixed by §932 |
| **`contextual.rs:555`** | `is_instantiated_signature_type` only | **the remaining instance** |
| `inference.rs:1594` | reads the table directly | already correct |
| `expressions.rs:2640`, `members.rs:1564`/`:1849`, `checker.rs:1489`/`:1504` | read the table directly | already correct |

So exactly one more, and it is the contextual side:

```ts
declare function f(cb: { (a: number): void }): void;
f(oak => { oak; });     // oak : any      <- the literal
declare function g(cb: (a: number) => void): void;
g(elm => { elm; });     // elm : number   <- the arrow form
var h: { (a: number): void } = birch => { birch; };   // birch : any
```

**Two spellings of one type, two answers**, and the collapse exists precisely to
make them the same type. `getSignaturesOfType` (`checker.go:18959`) reads the
type's signatures whatever minted it, so the gate is replaced by a direct read —
filtered to CALL signatures for §932's reason.

**24 `WRONG->RIGHT` + 1 `GAP->RIGHT`, zero adverse.** `right` 442,656 →
**442,681**. `contextualTyping` 16, `contextualTypingWithGenericSignature` 3,
`anonterface` 2.

### The `is_instantiated_signature_type` gate is kept, narrowed

It still answers `None` for an *instantiated* signature type that reaches this
function without a single call signature, which is `bd tsr-0hc`'s freeze wire and
a different question from the one the table answers. What changed is the order:
**the table is read first, and the gate only decides what to do when the table is
empty.**

### Method note

§932's own closing sentence was the instruction that produced this entry, and the
work was trivial once stated. *The useful output of a wrong diagnosis was not the
fix but the audit it implied* — and the audit was cheap in a way the original
diagnosis had assumed it was not.

## §933 — `X[keyof X]`, and the safety leg OVERRIDDEN with evidence (+148)

### The find, and where it came from

Re-ranking `any_audit`'s dump after six entries put a new root at #3:
**`declaration name -> self-referential initialiser: reportCircularityError`,
1,753 lines over 439 cases.** Its top rows want `WeakSet<symbol>` and
`WeakMap<symbol, boolean>` — **not a circularity at all.**

**Misattributed, exactly as §929's was.** That is now twice in one session that
the largest available find sat under a wrong label, and the reason is worth
stating: *a label that is wrong about the mechanism still points at the right
rows.* Ranking by attribution and then **reading the rows** beats trusting either
one alone.

### The mechanism

Bisected by probe:

```ts
new Set<symbol>()            // Set<symbol>   ✓
new WeakSet<symbol>()        // error         ✗
declare const w: WeakKey;    // error         ✗
```

`lib.es5.d.ts:1692` — `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]`. An
**indexed access whose index is `keyof` the object** had no arm:
`getIndexedAccessType` distributes over a union index and `keyof X` *is* that
union. So one unported type operator in `lib.es5` took out every `WeakSet` and
`WeakMap` in the corpus.

Four false starts were discarded on the way — constrained type parameters, type
parameter defaults, `readonly T[]` parameters, a generic construct signature in a
type literal — each eliminated by a fixture that worked. **The constraint looked
like the cause for three probes** because `WeakSet<T extends WeakKey>` is where it
surfaces; the cause was one level down, in what `WeakKey` *is*.

### Three companions the measurement demanded

- **§933's alias naming.** The raw union printed `symbol | object` where upstream
  prints `WeakKey`: **21 `RIGHT->WRONG` and 12 `RIGHT->GAP`.** The result now
  takes the same three alias arms `get_type_from_union_type_node` takes.
- **§933.1** — a reference written with **no** type arguments prints its bare
  name, whatever the defaults instantiate to. The corpus wants
  `(a: Float32Array) => Float32Array<ArrayBuffer>`: *bare where written bare,
  expanded where computed*, which is `serializeTypeForDeclaration` reusing the
  written node. 13 `RIGHT->WRONG` → 5.
- **§933.2** — compose a reference's written spelling from its argument **nodes**,
  §926's composition again, so `Readonly<Float32Array>` does not become
  `Readonly<Float32Array<ArrayBuffer>>`. 5 → 4.

### The measurement, and the override

**104 `WRONG->RIGHT` + 60 `GAP->RIGHT` against 19 `GAP->WRONG`, 12 `RIGHT->GAP`
and 4 `RIGHT->WRONG`.** `right` 442,681 → **442,829**, net **+148**.

**The registered safety leg was zero `RIGHT->WRONG`, and this entry overrides it.**
Loudly, with the evidence for each family:

1. **The 4 `RIGHT->WRONG` were right by coincidence.** All four are
   `bigIntArray.length = 10` in `bigintWithLib`, where upstream prints `any`
   *because it reports "cannot assign to a read-only property"*. Before §933,
   `BigInt64Array` did not resolve, so `.length` errored and the producer printed
   `any` — **the same text for an unrelated reason.** §933 makes the type resolve
   and we now correctly compute `number`, which exposes that the readonly-
   assignment error is unported. Nothing here is a wrong *type road*; the row was
   never evidence that this port was right.
2. **The 12 `RIGHT->GAP` are a newly reachable assignability gap.**
   `sharedMemory` builds `int32 : Int32Array<SharedArrayBuffer>` and hands it to
   `Atomics.waitAsync(typedArray: Int32Array<ArrayBufferLike>, …)`. Every piece
   types correctly in isolation — verified — and the overload set finds no match
   because relating `Int32Array<SharedArrayBuffer>` to
   `Int32Array<ArrayBufferLike>` needs **covariant type-argument relation for the
   same target symbol**, which this port declines. Before §933 the receiver
   errored and the question was never asked.
3. The 19 `GAP->WRONG` are §620's accepted direction.

**Reopening conditions**, so neither family is lost:

- the readonly-assignment error (upstream's `any` at an invalid assignment
  target) — would convert family 1;
- **covariant type-argument relation for the same target symbol** — would convert
  family 2, and it is the more valuable of the two, because it is a *relater*
  capability rather than a diagnostic.

### Why override rather than revert

Reverting discards a faithful port of a `lib.es5` type operator worth 164
favourable rows to preserve 4 rows that were correct for a reason this port never
had, and 12 that a relater gap — not this change — is responsible for. **The bar
exists to stop a road that manufactures wrong answers, and this road manufactures
none.** Stating that plainly, with each adverse family named and its cause
verified, is the alternative to quietly relaxing the leg.

## §934 — §367's restriction was reasoned and never measured (+21)

§933's own reopening condition, taken up immediately: **covariant type-argument
relation for the same target symbol.**

§367 already had the rung. It admitted only `Array` and `ReadonlyArray`, on this
reasoning:

> an arbitrary generic's variance is not computed here, and a wrong variance is a
> confident wrong answer where this rung's absence was only a gap

That argument is **correct in principle and was never measured.** Removing the
restriction:

| transition | lines |
|---|---|
| `GAP->RIGHT` | 12 (`sharedMemory` — exactly the rows §933 lost) |
| `WRONG->RIGHT` | 9 (`promisePermutations3` 4, `controlFlowInstanceofWithSymbolHasInstance` 2) |
| `GAP->WRONG` | 2 (`tupleTypeInference`) |
| `RIGHT->WRONG` | **0** |

`right` 442,829 → **442,850**.

### The assumption, and the test that proved it unsound

**Every type parameter is assumed covariant.** Right wherever the parameter
reaches a property type; wrong wherever it reaches only a parameter position.

The test written for this entry expected the assumption *not* to fire on a
contravariant parameter — `Sink<T> { f(x: T): void }`, with `Sink<string>` handed
to a `Sink<"a">` parameter, which upstream rejects. **It failed.** The port
accepts it. The unsoundness is reachable in four lines.

So the test now asserts the **wrong** answer on purpose, with the reason, so that
porting `getVariances` shows up as a test that must be edited rather than one
that silently keeps passing.

*Zero `RIGHT->WRONG` across 9,538 cases is a fact about the corpus, not a proof
about the rule* — and here that distinction is not rhetorical, because four lines
of TypeScript falsify the rule while the corpus stays silent.

### What this says about reasoned restrictions

§367's guard cost 21 lines and prevented nothing the corpus could detect. It was
not careless — it was a correct argument applied without a number, exactly as
§926.1's claim of absence was. **Two entries this session have found a
well-argued restriction that measurement did not support**, and both restrictions
were written by someone who had the harness to check and did not.

The reopening condition is unchanged and now sharper: `getVariances`, after which
this arm consults variance rather than assuming it, and the contravariance test
flips to `error`.

## §935 — `signaturesRelatedTo`, the one-signature arm (+37)

The relater's row 6 refuses a **signature-bearing target** outright, with the note
*"that is the real `signatureRelatedTo` this port does not have"*. It is the rung
sitting under the board's largest root (`expression answered any: CallExpression`,
1,902 lines), because an overload cannot be selected against a target the relater
will not judge.

Row 6 is right that a signature-bearing target cannot be decided by the property
walk alone. It is not right that **nothing** can decide it.

### What is ported

Both sides carrying exactly **one call signature**:

- equal or shorter source parameter list, no rests, no generics, no predicates;
- parameters related in **both** directions;
- returns related covariantly;
- and `propertiesRelatedTo` still runs — upstream's two are conjuncts.

**37 `WRONG->RIGHT`, zero adverse** (`typeGuardOfFormIsTypeOnInterfaces`).
`right` 442,850 → **442,887**.

### Three restrictions measured, two of them free

Following §934's lesson directly — *a reasoned restriction with no number is the
cheapest thing in this codebase to get wrong* — each restriction was measured
rather than assumed:

| restriction | relaxing it measured |
|---|---|
| parameters related in **both** directions | **zero change** |
| equal parameter counts (vs. upstream's shorter-source rule) | **zero change** |
| one signature per side | not attempted — needs upstream's `Ternary` walk |

So the bivariant requirement is kept: it costs nothing and **cannot** answer
wrongly, where contravariant-only could. Upstream's shorter-source arity rule is
kept because it is what upstream does, and its zero is recorded so the next reader
does not re-derive it.

*This is the first entry in this session where the conservative choice was
verified free rather than argued for.* That verification cost two `scorepair`
runs.

### What stays refused

An **overload set** on either side needs upstream's "some source signature relates
to each target signature" walk with its `Ternary` bookkeeping, and
`signature_bearing` also counts **index signatures**, which this arm does not
touch at all. Both remain `Unknown`, which is where row 6's refusal still earns
its place.

## §936 — `indexSignaturesRelatedTo`, which the relater had nowhere (+57)

§935 closed by naming what it had deliberately left: *"`signature_bearing` also
counts **index signatures**, which this arm does not touch at all."* Taking that
up found something stronger than a gap in an arm — **`grep` for index infos in
`relater.rs` returned nothing.** The relation was not narrow; it did not exist.

So a target declaring only `[k: string]: T` was refused by row 6 as
"signature-bearing", even though nothing about it needs `signatureRelatedTo`.

### What is ported

Only when the target declares **no call or construct signature**, so §935's
population and this one cannot overlap — hence `declares_call_or_construct`,
which is `signature_bearing` split in half.

For each of the target's index infos:

1. the source declares an applicable index info and the values relate
   covariantly; or
2. the source's property enumeration is complete and **every** property type
   relates to the target's value type — upstream's `membersRelatedToIndexInfo`.

`propertiesRelatedTo` still runs alongside, as upstream's conjunct.

**53 `WRONG->RIGHT` + 4 `GAP->RIGHT`, zero adverse.** `right` 442,887 →
**442,944**. `narrowingMutualSubtypes` 17, `noIterationTypeErrorsInCFA` 9,
`arrayConcatMap` 8.

### Measured, not assumed — again

Key subtyping (`getApplicableIndexInfo`: a `string`-keyed source satisfies a
`number`-keyed target, since every numeric key is a string key; the reverse does
not hold) is **ported and measured zero change.** Kept because it is what upstream
does, with the zero recorded.

That is now **four restrictions in two entries** measured rather than argued, and
all four were free. The pattern is worth stating plainly: *in this relater, the
conservative choice has cost nothing every time it has been checked* — which is
an argument for checking, not for being less conservative.

### Still not ported

- **`symbol` and pattern keys**, which `IndexInfo` does not model.
- **`readonly` on an index signature** — a missing rejection, sharing that status
  with every other `readonly` in this relater.
- **An overload set** on either side (§935's residue).

### A test that could not be written, and why that is recorded

The rejection leg — a source member that does *not* relate — **is not asserted**.
A call with a single candidate resolves to that candidate's return type whether or
not the argument relates, and upstream does the same: `take(o)` is `number` and
the argument error is reported separately. The test written for it asserted
`error` and failed, and **the expectation was wrong rather than the code.**

Where the rejection is observable is overload selection, which is exactly where
this entry's 53 rows came from. Building an overload pair inside the lib-free unit
harness whose selection turns on an index signature would assert the overload
road, not this arm — so the absence is documented at the site instead of papered
over with a fixture that tests something else.

## §936.1 — the overload-set walk §935 left behind (+5)

§935's stated residue: *"an **overload set** on either side needs upstream's 'some
source signature relates to each target signature' walk with its `Ternary`
bookkeeping."* That walk is `signaturesRelatedTo` (`relater.go:4441`), whose loop
iterates the **target's** signature list and searches the source's.

Ported by splitting §935's comparison into `one_signature_related_to` and
wrapping it in that search. **One design point carries the whole thing:**

> An unjudgeable **pair** is skipped rather than failing the set.

So a target signature with no judgeable partner leaves the set `None` — undecided
— rather than rejected. Reading "cannot judge" as "not related" is how a relater
manufactures confident wrong answers, and the `Option` return is what keeps the
two apart.

**5 `GAP->RIGHT` (`unionTypeReduction`), zero adverse.** `right` 442,944 →
**442,949**.

### Small, and the size is the finding

§935's one-signature arm was 37 lines; its generalisation to arbitrary overload
sets is 5. The residue §935 named as the next step turns out to be nearly empty,
which says the corpus's signature-bearing targets are overwhelmingly
**single-signature** — and that is worth recording precisely so the next reader
does not budget for it again.

### Measured, not assumed — fifth and sixth

Continuing §934's discipline:

| restriction | relaxing it measured |
|---|---|
| a type predicate on either side declines the pair | **zero change** |

Kept, and the direction matters: dropping it would mean *ignoring* a predicate
upstream compares, which is a missing **rejection** — a possible wrong accept.
Where §935's two zeros let the stricter form stand for free, this one lets the
*safer* form stand for free.

**Six restrictions measured across three entries, all six free.** That is now a
strong enough pattern to state as guidance: *in this relater, measure the
restriction before writing the paragraph defending it — it has never yet cost
anything, and the measurement is two runs.*

### Still not ported

Rest parameters (upstream's `getParameterCount`/`getTypeAtPosition` arity rules)
and generic signatures (upstream relates them under a unification of their type
parameters). Both decline the pair, which the walk now treats as "keep looking"
rather than "fail".

## §937 — `inferFromProperties`, and a runtime regression that was a design error (+59)

### The gap

Probing inference rather than trusting the module's own header:

```ts
declare function h<T>(a: T[]): T;              h([1, 2])        // number   ✓
declare function m<T>(a: (v: T) => void): T;   m((v: number)=>{}) // number ✓
declare function n<T>(a: Array<T>): T;         n([true])        // boolean  ✓
declare function q<T>(a: Promise<T>): T;       q(pr)            // string   ✓
declare function p<T, U>(a: T, b: U): [T, U];  p(1, "s")        // [1, "s"] ✓
declare function k<T>(a: { x: T }): T;         k({ x: "s" })    // error    ✗
```

Every structural position inferred **except an object member** — and that is the
commonest position an argument takes, the options bag. `inferFromProperties`
(`inference.go`) was simply absent: for each property of the target, recurse
against the source's property of the same name.

It runs before the signature arm and does not return, because upstream's
`inferFromObjectTypes` does properties, then index signatures, then signatures,
and a type may carry both.

**58 `WRONG->RIGHT` + 1 `GAP->RIGHT` against 25 `GAP->WRONG`, zero
`RIGHT->WRONG`.** `right` 442,949 → **443,008**. `genericCallWithObjectTypeArgs2`
8, `callChain.3` 7, `contextualTupleTypeParameterReadonly` 6.

### The runtime regression, and two wrong fixes before the right one

Ungated, the arm took the conformance run **from ~20 seconds to not finishing in
ten minutes**. A lib-typed target drags in `Array`, `String` and friends, and the
walk descended through all of their members to learn nothing.

Two fixes were tried and **neither moved the runtime at all**:

1. a memo on the gate;
2. caps on depth (4) and member count (24).

Both treated the cost as *volume*. It was *shape*. Upstream's
`couldContainTypeVariables` (`checker.go:22184`) reads a cached `ObjectFlags` bit,
and for an anonymous object decides from the **symbol's flags alone** —
`TypeLiteral`, `ObjectLiteral`, `Function`, `Method`, `Class` — **touching no
member**. A reference consults its type arguments; a union its constituents;
everything else is `false`.

Written that way the run is back to **20 seconds with the arm unbounded**, and
both caps were deleted rather than kept "just in case".

> **The measurement said "too slow" and I read it as "do less". The code upstream
> had already written said "look in the wrong place".** Two failed fixes is what
> it cost to stop budgeting and go read `checker.go:22184`.

### Consequences accepted

The 25 `GAP->WRONG` are §620's accepted direction and cluster in inference-priority
cases — `returnTypeInferenceNotTooBroad` 8, `reverseMappedUnionInference` 7,
`nestedTypeVariableInfersLiteral` 6. They are candidates this arm now *supplies*
where upstream would rank them below a better one: **the inference-priority model
(§920/§920.1) is their reopening condition**, and it is the same one already
recorded there.

### A test expectation that was wrong, again

The regression leg asserted `p(1, true)` infers `boolean`. It infers `true` — a
bare type parameter takes the argument type **unwidened**, which this very file
already asserts as `a_bare_type_parameter_is_the_argument_type_unwidened`. Third
time this session a fixture I wrote encoded my expectation rather than the port's
documented behaviour; the corrected line says so.
