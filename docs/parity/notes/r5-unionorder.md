# r5-unionorder: why TSR prints union members in a different order

Lane `r5-unionorder` (epic `tsr-2zk`), pinned vendor `5b1047d`. Base frozen
at `f5d0291`. Input: the `union-order` and `union-members-differ` rows of
[r5-typetriage](r5-typetriage.md).

## 1. The brief's premise, corrected

The brief says native prints union members in **type-id order**
(`sortedTypes` in `getUnionType`, `compareTypes` by id). That is
TypeScript's (Strada's) rule. **tsgo does not sort by id.** `insertType`
and `addTypesToUnion` order by `CompareTypes` (`internal/checker/utilities.go:415`):
sort-order flags (`getSortOrderFlags`, `:581`), then the type-name symbol's
name (`compareTypeNames`, `:589`), then per-kind data (symbol declaration
position, literal values, tuple shape, type-argument lists, mappers), and
only then the id (`:577`: "This results in type creation order for built-in
types"). The port already ports this comparator (`unions.rs`
`compare_types`).

So the hypotheses about intrinsic init order and literal mint order (a, b)
can only matter where every earlier key ties: two intrinsics of identical
flags (none print in the witnesses) or two types the comparator cannot tell
apart. **No witness was decided by the id tiebreak.** The type store's id
assignment and the intrinsic init order were not changed, and no ADR was
needed.

## 2. Classification of the witnesses

Measured at the base: 54 WRONG lines in 24 cases whose want and got hold the
same tokens and contain `|` (the triage's `union-order` rule; one line more
than the triage counted at `ccb48e7`). Classes:

- **(a)** intrinsic or init order; **(b)** mint order along another
  evaluation path; **(c)** a missing or misordered origin; **(d)** a TSR
  sort that differs from native's `CompareTypes`;
- **(e)** *not a sort at all*: native prints a **reused written type node**
  (`serializeTypeForDeclaration` / `serializeReturnTypeForSignature` reuse
  arms, `nodebuilderimpl.go:2181`, `:2023`; the pseudochecker's
  `typeFromTypeAssertion`, `pseudochecker/lookup.go:510`), so the printed
  order is the source order, and TSR prints from the type;
- **(f)** the reverse of (e): TSR bakes the written order into a type's
  text where native prints the type (sorted).

| witness (lines) | want | class | producer | owner |
|---|---|---|---|---|
| `unknownControlFlow:0:23` (1) | `ThisOrThatNode \| null \| undefined` | (c) | origin entries sorted by a heuristic key, rendered without `formatUnionTypes`' nullable tail | **this lane, fixed §3** |
| `typeInferenceLiteralUnion:0:15` (1) | `(T \| Primitive)[]` | (c) | same: alias `Primitive` keyed by its first member (`undefined`) | **fixed §3** |
| `generatorYieldContextualType:0:25,29,69` (3) | `T \| Directive` | (c) | same | **fixed §3** |
| `logicalOrOperatorWithEveryType:0:232,233,236,237` (4) | `{ a: string; } \| E` | (c) | `add_named_unions` did not count an enum as named | **this lane, fixed §4** |
| `baseClassImprovedMismatchErrors:0:7,12` (2) | `() => number \| string` | (e) | pseudochecker return from `return 10 as number \| string` (`PSEUDO-RETURN-TYPE-FROM-SINGLE-RETURN-EXPRESSION`) | signatures.rs `written_return` |
| `spreadObjectNoCircular1:0:4,5` (2) | `{ content: Foo \| Box; }` | (e) | object-literal property typed by `this as Foo \| Box`: reuse of the assertion node | printing.rs/objects.rs (r5-shapes) |
| `typePredicatesOptionalChaining3:0:2,22` (2) | `value is undefined \| null` | (e) | written predicate type reused | signatures.rs |
| `complexRecursiveCollections:1:24` (1) | `… is Collection.Keyed<any, any> \| Collection.Indexed<any>` | (e) | written predicate type reused | signatures.rs |
| `destructuringParameterDeclaration10(*)` (4) | `Record<"json" \| "jsonc" \| "json5", …>` | (e) | type-literal property annotation in a parameter (`TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE`) | declared.rs (r5-declared3) |
| `cannotIndexGenericWritingError:0:8` (1) | `{ [s: string]: number \| string; }` | (e) | written constraint node (`TYPE-PARAMETER-CONSTRAINT-NODE-REUSE`) | signatures.rs `type_parameter_of` |
| `objectFreeze`, `objectFreezeLiteralsDontWiden`, `objectFromEntries`, `contextualSignatureInObjectFreeze`, `circularContextualReturnType` (18) | `<T extends { [idx: string]: U \| null \| undefined \| object; }, …>` | (e) | lib `Object.freeze` overload's written constraint reused | signatures.rs `type_parameter_of` |
| `complicatedIndexesOfIntersectionsAreInferencable:0:5` (1) | `"initialValues" \| "validate" \| …` | (e) | written conditional/alias body | declared.rs |
| `declarationEmitMappedTypePropertyFromNumericStringKey:0:2-5` (4) | `{ [K in keyof T]: string \| T[K]; }` | (f) | mapped template text baked from the written `T[K] \| string` | mapped.rs (r5-mapped4) |
| `wideningWithTopLevelTypeParameter:0:47` (1) | `T extends undefined ? never : string \| T` | (f) | conditional branch text baked from the written node | declared.rs |
| `dependentDestructuredVariablesFromNestedPatterns:0:12,22,55` (3) | `[undefined, Error] \| [Awaited<T[K]>, undefined]` | (f) | mapped template text baked from the written tuple union | mapped.rs |
| `noInferUnionExcessPropertyCheck1:0:3` (1) | `(() => NoInfer<T>) \| NoInfer<T>` | (d) | `NoInfer<T>` is an alias reference here, a substitution type natively | **this lane, fixed §6** |
| `mappedTypeIndexedAccess:0:13` (1) | `{ key: "bar"; … } \| { key: "foo"; … }` | (d) | no `compareTypeMappers` arm for instantiated type literals | shipped as a diff, §5 |
| `genericRestParameters3:0:149` (1) | `[x: string, number] \| [x: string, ...rest: A]` | (d) | see §7 | open |
| `returnTagTypeGuard:0:40` (1) | `(val: boolean \| number) => void` | (e) | JSDoc `@param` type reused | JSDoc lane |

**Verdict on "diffuse vs systemic".** The triage was right that the class
is diffuse: 37 of 54 lines are node reuse (e) or its reverse (f), which
no union sort can produce. One cause *was* systemic, the origin entry order
(c), and it is fixed below.

## 3. Fix: the origin's entries are `CompareTypes`-ordered and printed through `formatUnionTypes`

**Native.** `getUnionTypeWorker` (`checker.go:25705`): when an input
union is *named* (`addNamedUnions`, `:25824`: an alias, which an enum's
declared type also carries, `:23899`; or a non-union origin), the origin is
`newUnionType(reducedTypes)` after each named union is placed with
`insertType`. `reducedTypes` are the typeSet members not inside a named
union, already in `CompareTypes` order, so every entry is in
`CompareTypes` order. An **unnamed** union input is not an entry: its
members are typeSet members. The node builder prints the origin through
`formatUnionTypes` (`printer.go:383`), which collapses `false | true` to
`boolean` and moves `null` then `undefined` to the end.

**What TSR did.** `union_type_worker` built entries from the *inputs*,
kept an unnamed union (`boolean`) as one entry, and sorted with a key
"nullable entries last, else the first member's sort bits", followed by
`compare_types`. The key was fitted to two baselines (`boolean | E`,
`MyEnum | undefined`) and reproduces them, but it orders an alias by its
first member: `type Primitive = undefined | null | …` sorted before `T`,
and `undefined` before `null` (the comparator's `1 << 2` vs `1 << 3`).
The site renderer (`checker.rs`, the §97 origin arm) printed entries in
stored order without the nullable tail.

**Change.**
- `unions.rs` `union_type_worker`: an entry is a named union (alias or
  enum symbol) or an origin entry; an unnamed union contributes its
  members. Entries are sorted by `compare_types` alone.
- `unions.rs` `build_origin_union`: the baked text is
  `format_union_types(entries)`.
- `checker.rs` origin arm of `type_to_string_at_worker`: iterates
  `union_print_parts(entries)` (the existing `formatUnionTypes` plan)
  instead of the raw entries. This is a three-line edit inside a hub
  function, needed so the two renderers agree; without it 24 RIGHT lines
  (`MyEnum | undefined`, `Thing | null`, …) became `undefined | MyEnum`.

`boolean | E` and `MyEnum | undefined` still hold, now for native's
reason: `false`/`true` sort below a union, and the printer moves nullables.

**Measured** (both dumps unfiltered against the frozen base):
types +9 (8 WRONG→RIGHT: `unknownControlFlow:0:23`,
`typeInferenceLiteralUnion:0:15`, `generatorYieldContextualType:0:25,29,69`,
`signatureCombiningRestParameters1:0:4,13`; 1 GAP→RIGHT:
`literalTypes2:0:183`, `number | boolean` that declined before because a
`boolean` entry was an object-less union outside the slice gate), zero
losses; diagnostics unchanged. Perf (median child CPU vs the base binary):
domain-model 1.011 (21 samples), generic-imports 0.990 (41 samples).
`slowcases` clean on both dumps.

**How this would be shown wrong.** A union whose origin native keeps and
whose entries tie under `CompareTypes` until the id fallback would print in
TSR's mint order. None is known.

## 4. Fix: an enum's declared type is a named union

**Native.** `addNamedUnions` (`checker.go:25828`) keeps a union whose
`alias != nil`, and `getDeclaredTypeOfEnum` builds the enum's union with
`&TypeAlias{symbol: symbol}` (`checker.go:23899`). So `a7 || a6`
(`{a: string}` and enum `E`, subtype-reduced by `||`) gets the origin
`[{ a: string; }, E]` in `CompareTypes` order, an object (`1 << 20`) before
a union (`1 << 27`).

**What TSR did.** `add_named_unions` (`unions.rs`) admitted only a symbol
with `SymbolFlags::TYPE_ALIAS`, so an enum's union was not named in the
subtype-reduction road (`subtype_union_from_sorted_list`) and in the
aliased road of `union_type_worker`. No origin was built and the flat
members printed with the enum collapse first: `E | { a: string; }`.

**Change.** `add_named_unions` admits any union this port names by a
symbol. The only producers of a symbol-named union are
`get_named_union_type`'s callers: type aliases and the enum declaration
(`declared.rs`, `ENUM_LITERAL`), so this is `t.alias != nil`.

**Measured** (against the base after §3): types +4
(`logicalOrOperatorWithEveryType:0:232,233,236,237`), zero losses;
diagnostics unchanged; perf 0.982 (domain-model) and 0.981
(generic-imports), 21 samples; `slowcases` clean.

## 5. Shipped diff: `compareTypeMappers` for instantiated type literals

`r5-unionorder-object-mapper.diff` (in this directory). Not applied: it
writes in `declared.rs` (r5-declared3) and `inference.rs` (main).

**Native.** Two instantiations of one anonymous object type have the same
symbol, so `CompareTypes` reaches the non-reference object arm
(`utilities.go:478`) and orders them by `compareTypeMappers(t1.mapper,
t2.mapper)` (`:683`): a nil mapper after a non-nil one; two array mappers
by their source lists, then their target lists. `Pairs<FooBar>[keyof FooBar]`
distributes over `"bar" | "foo"`; the two member objects differ only in the
mapper `TKey -> "bar"` / `TKey -> "foo"`, so `"bar"` prints first
(`mappedTypeIndexedAccess.types:50`).

**What TSR does.** Its instantiated type literals are `Named` images keyed in
`instantiated_objects` by `(source, mapper)`, but nothing maps a result back
to its mapper. Two images of one literal tie on symbol position and fall to
the id, which is mint order (`foo` first, from `keyof FooBar`'s declaration
order).

**The diff.**
- `checker.rs`: a new field `instantiated_object_mappers: TypeId ->
  (source, mapper)`, the inverse of `instantiated_objects` for a freshly
  minted object. Checker port convention: native operation
  `compareTypeMappers` over `ObjectType.mapper`; key is the minted result
  `TypeId`, owned by the producer that minted it; written once, at mint
  (the reserved image in `declared.rs`, the minted image in
  `inference.rs`), never updated; no receiver or alias context; no work
  beyond a hash insert at mint and a lookup on a comparator tie.
  `mapped.rs`'s `instantiated_objects` insert is deliberately not
  recorded: its worker can answer an existing type, which must not gain a
  mapper.
- `unions.rs` `compare_types`: after the symbol arm, two `Named` objects
  compare their recorded mappers as above.

**Measured** (types and diagnostics unfiltered, against the base after
§4): types +1 (`mappedTypeIndexedAccess:0:13`; the case flips, 19/19
RIGHT), zero losses; diagnostics unchanged; perf 0.919 / 0.93, 0.978 (21,
41, 41 samples; noise). A first experiment with a linear scan of
`instantiated_objects` measured the same +1.

## 6. Fix: `NoInfer<T>` sorts as a substitution type

**Native.** `getNoInferType` (`checker.go:27394`) returns a substitution
type. `CompareTypes` sorts it by `TypeFlagsSubstitution` (`1 << 24`), which
is after an object's `1 << 20`; a substitution has no type-name symbol;
two of them compare base types, then constraints (`utilities.go:558`).
`NoInfer<T> | (() => NoInfer<T>)` therefore prints
`(() => NoInfer<T>) | NoInfer<T>` (`noInferUnionExcessPropertyCheck1.types:12`).

**What TSR did.** `NoInfer<T>` is an alias reference
(`declared.rs` `no_infer_base_type`'s doc comment says why), which sorts
under the name `NoInfer` ahead of an unnamed function type.

**Change.** `compare_types` (`unions.rs`) treats a `NoInfer` reference as
native's substitution: `SUBSTITUTION` sort bits, then the bases, then the
id. The lookup is gated on `OBJECT` so a comparison of non-objects pays
nothing.

**Measured** (against the base after §4): types +1
(`noInferUnionExcessPropertyCheck1:0:3`), zero losses; diagnostics
unchanged; perf 0.986 (domain-model), 0.994 (generic-imports), 21 samples;
`slowcases` clean.

## 7. Open

Filled in as investigated.
