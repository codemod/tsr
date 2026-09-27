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
