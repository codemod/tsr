# Lane notes: r5-declared2 (`declared.rs` follow-ons, second box)

Round-5 cloud lane on epic `tsr-2zk`, successor to r5-declared
([`r5-declared.md`](r5-declared.md)) as the single owner of
`crates/tsr-checker/src/declared.rs`. Items, in order: `tsr-2zk.1061` (the
held intersection-alias diff), `tsr-2zk.1042` (tuple aliases that print a
name where native prints the structure), `tsr-2zk.1043` (genericDefaults),
`tsr-2zk.1059` (renamed ES import specifiers on the alias road). Native
anchors are `vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go`
unless noted).

**Base.** Integration head `df42dc4` (batch W) did not carry r5-declared's
`e9d15e8` (`qualified_declared_type_twin`, `.979`), although batch W's
bookkeeping lists it; only its isEnumTypeRelatedTo diff landed (`1aa95e7`).
This lane's first commit merges `claude/beautiful-shannon-ar5gh0-r5-declared`
(e9d15e8 and two doc commits) and the frozen base is that merge, `4d875db`:

| dump | RIGHT | EMPTY_RIGHT | WRONG | EMPTY_WRONG | GAP |
|---|---|---|---|---|---|
| `diagverdictdump` | 5373 | 5584 | 1218 | 63 | |
| `verdictdump` | 544662 | | 6906 | | 965 |

**Setup.** As r5-operators3 §4 and r5-declared recorded: PyPI is blocked, so a
stdlib-only stand-in for `tomlkit.parse`/`inline_table`/`dumps`, kept outside
the repository, assembled the vendored crates. Dumps ran alone
(`RAYON_NUM_THREADS=3`), never beside a build.

## 1. `tsr-2zk.1061` — getTypeAliasInstantiation over intersection bodies

Lands [`r5-declared-intersection-alias.diff`](r5-declared-intersection-alias.diff)
(r5-declared §2; its reasoning is not repeated here) with the two blockers
that held it root-caused and fixed, plus one regression its unit test found.

### 1.1 `keyof Example4<'x', 'y'>` was `error` (keyofIntersection:0:17)

r5-declared §2 recorded this as "resolves to `any` when first instantiated
inside another alias's declared computation". The position was a red
herring: the same line alone in a file prints `error` too. The cause is the
`keyof` arm for concrete operands (§730, `get_type_from_type_node`'s first
TypeOperator arm). It sends an OBJECT-flagged operand to
`resolved_keyof_type` and everything else to `keys_of`, which has no
intersection arm. Before the diff `Example4<"x", "y">` was an OBJECT name
mint; after it the reference is the alias-carrying INTERSECTION native
builds, so it fell to `keys_of` and answered `error`.

**Port.** getIndexTypeEx (`:26684`) distributes over an intersection
(`:26693`, the union of the constituents' keys) and answers
`string | number | symbol` for `never` (`:26701`). Both arms already exist in
`resolved_keyof_type_worker`; the gate now admits INTERSECTION and NEVER
operands as well as OBJECT ones.

The second `keyof` arm (written operands that mention a type parameter)
admitted only operands whose *type* is an alias reference. An
intersection-bodied alias that reduces to one constituent answers that
constituent with no alias (getIntersectionTypeEx, `:26127`), so
`keyof Wrapped<T>` with `type Wrapped<T> = T & unknown` lost its `keyof T`
(`union_key_element_access::generic_alias_keys_follow_substituted_bodies`).
The arm now also admits a reference whose *written name* resolves to an
intersection-bodied alias. That is native's own reading: getIndexType runs
on whatever the reference resolved to, alias or not.

### 1.2 `typeof Class<T>` constituents (aliasInstantiationExpressionGenericIntersectionNoCrash2)

`type Wat<T> = ClassAlias<T> & FnAlias<T>` with `ClassAlias<T> = typeof
Class<T>`. Native relates `Wat<number>` to `Wat<string>` structurally
(relater.go:3389 probes alias variance for object and conditional types only)
and reports TS2352 through the `ClassAlias` constituent, which it prints as
`{ new (): Class<string>; prototype: Class<any>; }`. This port answers an
instantiation expression in a type query as `error`
(`get_type_from_type_query_node`'s first line), so `ClassAlias<T>` is a
member-less name mint, the constituent relation is `Unknown`, and the
diff's structural intersection relation reported nothing.

**Decline, with the missing piece named.** `instantiate_intersection_alias`
keeps the alias's own name mint (whose relation reads the alias's arguments,
as before the diff) when a constituent of the declared intersection is an
alias reference whose body does not evaluate (`evaluate_alias_body` answers
`None`). It waits for `tsr-2zk.1006` (r5-instexpr, instantiation
expressions). When `typeof Class<T>` has a real type, the decline stops
firing for that shape and the relation is native's. It is not a special
case of the test: any intersection over a constituent this port cannot
build has no structural relation to offer.

- **What would change it:** `.1006` landing. Re-measure with the decline
  removed; the TS2352 should then come from the structural road.
- **How I would know it is wrong:** a case whose intersection constituent
  fails to evaluate for a reason native shares (a genuinely erroneous body)
  and whose native print is the structure. None appeared in the full run
  (zero losses, below).

### 1.3 Not changed: self-referential bodies

r5-declared's stated divergence stands (`type LinkedList<T> = T & { next:
LinkedList<T> }` keeps the name mint). The fix is in the frame-bound member
read (`members.rs`, main's file); nothing in this item needed it.

### 1.4 Measured

Unfiltered, against the frozen base `4d875db`:

| | base | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10957 | 10960 (+3) |
| type lines RIGHT | 544662 | 544715 (+53) |
| losses (diag / types) | | 0 / 0 |

Cases converted: `jsxChildWrongType`, `parenthesisDoesNotBlockAliasSymbolCreation`,
`spyComparisonChecking`. Type gains, by case: `typeVariableConstraintIntersections`
19, `propTypeValidatorInference` 6, `declarationEmitGenericTypeParamerSerialization2` 4,
`intersectionsAndEmptyObjects` 4, `ramdaToolsNoInfinite2` 3,
`recursiveTypeRelations`, `spyComparisonChecking`, `numericStringLiteralTypes`
2 each, and one each in `contextuallyTypedByDiscriminableUnion2`,
`declarationEmitClassMixinLocalClassDeclaration`,
`declarationEmitMappedTypePreservesTypeParameterConstraint`,
`declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1`–`3`,
`mappedTypeGenericIndexedAccess`, `nonNullableAndObjectIntersections` (both
configurations), `templateLiteralTypes6`, `unknownType1`.

Ir (callgrind, `--singleThreaded --pretty false`, base binary vs this
commit): generic-imports 342,890,644 → 342,886,851 (−0.001%); domain-model
1,194,676,690 → 1,193,835,226 (−0.07%). Median child CPU vs the base binary,
21 samples: generic-imports 1.023, domain-model 1.020;
`diagnostics_match: true` on both. Workspace tests pass (the regression in
`union_key_element_access` is §1.1's second paragraph); clippy reports
nothing in `declared.rs` (its pre-existing findings are in
`enum_initializer.rs`, `index_signatures.rs`, `signatures.rs`,
`templates.rs` and `tsr-dts` tests).

## 2. `tsr-2zk.1042` — when a tuple alias keeps its name

### 2.1 The native rule

getTypeFromArrayOrTupleTypeNode (`:24115`) has two outcomes for a tuple
written as an alias's body:

- **no variadic element** (`:24121`, isDeferredTypeReferenceNode answers true
  because getAliasSymbolForTypeNode finds the alias): a *deferred* type
  reference, and createDeferredTypeReference (`:25121`) attaches
  getAliasForTypeNode's alias. It prints the alias name (`T06`, `T04`, `Foo<T, U>`);
- **a variadic element**: createNormalizedTupleTypeEx, which takes no alias.
  It prints the structure.

An element is variadic when getTupleElementFlags (`:24709`) says so: a rest
(or a `...`-labelled member) whose operand has no array element type node
(getArrayElementTypeNode, `:24185`: an array type node, parentheses, or a
one-element tuple over such a rest). `...string[]` and `...[...string[]]` are
Rest; `...T`, `...Array<string>`, `...Numbers`, `...[a: string]`, `...any`,
`...number` and `...(T extends 0 ? [c] : [])` are Variadic.

The port approximated this split twice. §956/§959 (non-generic aliases)
named a rest-bearing body unless "a rest's operand is a written reference" or
"normalization produced an element list"; §957 (generic aliases) printed the
structure for *any* rest. Both are replaced by `is_variadic_tuple_element`,
which is getTupleElementFlags' test. The old proxies agreed on most of the
corpus; they disagreed on `[item: any, ...any]` and `[any, ...remainder:
any]` (`namedTupleMembersErrors`, named before, structural natively) and on
`[...any]` (`variadicTuples1` `AnyArr`).

This parser builds a labelled rest as `RestTypeNode(NamedTupleMember(T))`
rather than native's `NamedTupleMember` with a `...` token (§956's note), so
`rest_element_operand` unwraps that nesting before getArrayElementTypeNode's
test.

### 2.2 Instantiation follows the same split

- **References to a variadic alias** (the existing §40 road in
  `create_type_reference_with_display`) are now gated on a variadic element
  rather than on any rest, so a non-variadic generic rest tuple
  (`type U<T> = [T, ...T[]]`) keeps its alias on reference (`U<0>`), as a
  deferred reference instantiates with its alias (getObjectTypeInstantiation).
  No corpus line moved either way; the gate is the native one.
- **A rest over a deferred conditional** (`...(T extends 0 ? [fourth: "c"] :
  [])`, `partiallyNamedTuples` `AddMixedConditional`). instantiateType maps
  the conditional and normalizes around its result. The print-only
  conditional mint cannot be instantiated by `instantiate_type`, so when that
  answers `error` the road evaluates the alias body under the alias's
  arguments (§92's `evaluate_alias_body`, the port's existing frame-bound
  instantiation) and takes the normalized tuple or array it builds.
- **An alias declared as a reference to a normalized-tuple alias**
  (`type V30<A> = Tup3<A, string[], number[]>` over `type Tup3<…> = [...T, ...U,
  ...V]`, `variadicTuples2` V30–V52). getTypeFromTypeAliasReference
  instantiates Tup3's declared type with V30's alias, and
  getObjectTypeInstantiation re-creates a non-deferred reference through
  createNormalizedTypeReference, dropping the alias. So V30 declares the
  structure, and so does every reference to V30.
  `alias_declares_normalized_tuple` follows a chain of such references
  (bounded at 8) to a variadic tuple body.

### 2.3 Variadic primitives are rests of `errorType`

TupleNormalizer.normalize (`:23374`): a variadic operand that is not any,
not generic, not a tuple and not array-like becomes a rest of `errorType`.
`[...string]` records `any[]` (`restTupleElements1` T08) and
`[first: string, ...rest: number]` records `[first: string, ...rest: any[]]`.
The old comment (§959) called this "inventing that error's recovery"; it is
native's own branch. The operand becomes ADR-0048's `native_error` (upstream's
`errorType` identity), which `normalize_variadic_tuple` already treats as an
any-flagged variadic. Only primitive operands are decided here, since they
are never array-like; an object operand keeps the old spelling until the
port answers isArrayLikeType for it.

### 2.4 `keyof` of a tuple carries its origin

getLiteralTypeFromProperties (`:26723`) attaches `keyof T` as the key union's
origin when the operand is a Reference, and a tuple is one. The tuple arm of
`resolved_keyof_type_worker` returned the bare union, so
`ToAnonymousTuple<[boolean, number]>`'s template printed the expanded keys
(`partiallyNamedTuples` 16–22). It now attaches the origin, as the property
arm already did.

### 2.5 Not converted

- `singletonLabeledTuple:0:17` (`AliasRest extends [unknown]` with
  `type AliasRest = [...p: number[]]`). The alias is a name over an array
  normalization; the conditional sees a member-less name and defers. Native
  relates the deferred reference's target. Needs the named copy to carry the
  array's identity; not attempted.
- `namedTupleMembersErrors` 8 (`[first: string, rest: ...string[]?]`, a
  JSDoc-style postfix `...`), 11/12 (self-referential variadic rests, which
  native reports as circular and declares `any`). The parser and
  `circular_alias.rs` own those.
- `restTupleElements1` 9/76/79/91 and `variadicTuples2` 200+ are inference
  and indexed-access lines, not alias naming.

### 2.6 Measured

Unfiltered, against §1's commit (whose dumps are this item's base; both are
zero-loss against the frozen base `4d875db`):

| | before | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 10960 | 10960 |
| type lines RIGHT | 544715 | 544738 (+23) |
| losses (diag / types) | | 0 / 0 |

Gains: `variadicTuples2` 9 (V30–V52), `partiallyNamedTuples` 9 (16–18,
20–22, 26–28), `namedTupleMembersErrors` 3 (2, 3, 10), `restTupleElements1` 1
(8), `variadicTuples1` 1 (`AnyArr`). Ir vs the base binary: generic-imports
342,895,768 → 342,868,204 (−0.008%); domain-model 1,194,512,792 →
1,194,001,918 (−0.04%). Median child CPU, 21 samples: generic-imports 0.982,
domain-model 1.027; `diagnostics_match: true`. Tests pass; no clippy finding
in touched code. Unit tests: `crates/tsr-checker/tests/r5_declared2.rs`.

## 3. `tsr-2zk.1043` — genericDefaults `[A, any, C]`: held diff (`inference.rs`)

`f14<A>()` with `declare function f14<T, U = V, V = C>(…): [T, U, V]` records
`[A, any, C]`; this port answered `error` (17 lines, `genericDefaults`
579–897). The brief placed the cause in getDefaultFromTypeParameter; it is
not there. `get_default_from_type_parameter` already takes the first
declaration with a default, and `f12<A>()` (`U = T`, a backward reference)
was already right.

**Cause.** The explicit-type-argument arm of the generic call road
(`inference.rs`, the `written_type_arguments` branch) builds its mapper one
position at a time, so when `U`'s default `V` is instantiated, `V` is not in
the mapper yet; `instantiate_type` refuses an unmapped parameter and the
call answers `error`. fillMissingTypeArguments (`:21954`) first maps every
unfilled position to errorType and only then instantiates each default
against that mapper, so an invalid forward reference becomes errorType
(TS2744 is its diagnostic, reported elsewhere).

**Port.** [`r5-declared2-fill-missing-forward.diff`](r5-declared2-fill-missing-forward.diff)
pre-fills the mapper with ADR-0048's `native_error` (upstream's errorType,
printed `any` inside a type) and overwrites each filled slot in order.
`inference.rs` is main's file, so this ships as a diff.

Measured unfiltered on top of §2's commit: type lines +17 (all
`genericDefaults`), diagnostics unchanged, zero losses on both dumps.

The same pre-fill already exists for type references (`declared.rs` §136's
fill, which uses `any` for the definite forward outcome) and for heritage
bases (`signatures.rs`, which uses the gap and declines). Those two are
unchanged here: neither moved a line in this run.

## 4. `tsr-2zk.1059` — renamed ES import specifiers on the alias road

`import { type "<A>" as typeA } from "./m"` with `export { type someType as
"<A>" }` and `type someType = "someType"`: native's `const importTest: typeA`
is `"someType"`, and the two literal initializers report TS2322
(`arbitraryModuleNamespaceIdentifiers_module`, 10 module configurations). The
port answered `any`.

§491's alias road in `get_type_from_type_reference` admits only an
*unrenamed* import specifier, because the target's declared type prints the
target's own name and native prints the local one (the §158 per-site naming
wall). That reason does not apply to a target whose declared type prints no
name at all. resolveTypeReferenceName calls resolveAlias whatever the local
name is, so the type is the same; only a name-carrying print would differ.

**Port.** A renamed import specifier whose fully resolved target is a
non-generic type alias declaring a primitive or literal type (not a union,
not enum-like) answers that declared type in regular form. Two details:

- resolveAlias follows the whole chain; this port's `resolve_alias` is one
  hop, and the export here is itself an alias (`export { type someType as
  "<A>" }`), so the road uses `resolve_alias_fully` (§501).
- The §29 park (`deferred_since`) is honoured as in the unrenamed arm.

Everything else a renamed specifier names (classes, interfaces, enums,
aliased object types, generic aliases) keeps the old answer until printing
can name the local alias (`tsr-e2u`).

**Measured** unfiltered, against §2's commit (the §3 diff not applied):
diagnostics RIGHT + EMPTY_RIGHT 10960 → 10970 (+10: every configuration of
`arbitraryModuleNamespaceIdentifiers_module`), type lines 544738 → 544758
(+20, two per configuration), zero losses on both dumps. Ir vs the base
binary: generic-imports 342,895,758 → 342,856,319 (−0.01%); domain-model
1,194,733,782 → 1,194,028,986 (−0.06%). Median child CPU: generic-imports
0.982 (21 samples); domain-model 1.036 at 21 samples, 0.954 at 41;
`diagnostics_match: true`. No unit test: the shape needs two files and a
module resolution, which the corpus case exercises and the checker's unit
harness does not.

## 5. Not done: `.1034`(e)

Operands for a conditional root whose only generic part is the extends type
(r5-declared §1.4) were not attempted in this session.

## 6. Folded diffs from other lanes (integrator request)

Two measured `declared.rs` diffs written by other boxes, applied as written:

- r5-errorsplit4's [`r5-errorsplit4-default-declared.diff`](r5-errorsplit4-default-declared.diff)
  (on its branch): `get_resolved_type_parameter_default` withholds
  publication only for the port's gap (`is_gap`), so an unresolved default
  is cached once, as getResolvedTypeParameterDefault does (`:22007`), rather
  than minted anew per read (ADR-0048).
- r5-typetriage's [`r5-typetriage-enum-member-ascii-escape.diff`](r5-typetriage-enum-member-ascii-escape.diff)
  (on its branch): the two `(typeof E)[…]` enum-member spellings use
  `quote_ascii` (nodebuilderimpl.go:3276–3278 has no NoAsciiEscaping).

Measured together, unfiltered, on top of §4's commit: type lines +1
(`enumWithUnicodeEscape1:0:1`), diagnostics unchanged, zero losses on both
dumps. Ir vs the base binary: generic-imports −0.007%, domain-model +0.01%.
Tests pass.
