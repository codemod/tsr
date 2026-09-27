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
