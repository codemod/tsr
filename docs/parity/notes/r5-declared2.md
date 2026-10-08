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
(e9d15e8 and two doc commits) and the frozen base is that merge, `9efedcf`:

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

Unfiltered, against the frozen base `9efedcf`:

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
