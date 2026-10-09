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
| `logicalOrOperatorWithEveryType:0:232,233,236,237` (4) | `{ a: string; } \| E` | (c) | see §4.1 | open |
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
| `noInferUnionExcessPropertyCheck1:0:3` (1) | `(() => NoInfer<T>) \| NoInfer<T>` | (d) | see §4.2 | open |
| `mappedTypeIndexedAccess:0:13` (1) | `{ key: "bar"; … } \| { key: "foo"; … }` | (d) | see §4.3 | open |
| `genericRestParameters3:0:149` (1) | `[x: string, number] \| [x: string, ...rest: A]` | (d) | see §4.4 | open |
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

## 4. Open (d)/(c) items

Filled in as investigated.
