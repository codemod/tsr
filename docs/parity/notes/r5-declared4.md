# Lane notes: r5-declared4 (`declared.rs` follow-ons, fourth box)

Round-5 cloud lane on epic `tsr-2zk`. It succeeds r5-declared3
([`r5-declared3.md`](r5-declared3.md)) as the single owner of
`crates/tsr-checker/src/declared.rs`, and owns `instantiation_expressions.rs`
and `unique_symbols.rs` after r5-instexpr. Items, in order: `tsr-2zk.1115`
(defaulted type arguments in printed references; numeric-string names in type
literals), `tsr-2zk.1102` (the instantiation-expression cache ignores alias
frames), and the `tsr-2zk.1078` leftovers. Native anchors are
`vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go` unless
noted).

**Base.** The brief said to freeze after batch AT. The round was wrapped up
before AT landed, and the integrator asked for a measurement against the
current integration tip instead: `c75e4af5` (batch AP). Frozen dumps:

| dump | RIGHT | EMPTY_RIGHT | WRONG | EMPTY_WRONG | GAP |
|---|---|---|---|---|---|
| `diagverdictdump` | 5484 | 5596 | 1107 | 51 | |
| `verdictdump` | 549046 | | 6376 | | 878 |

The final gate re-checks everything on top of AT.

**Setup.** As in r5-operators3 §4: PyPI is blocked, so a stdlib-only stand-in
for `tomlkit.parse`/`inline_table`/`dumps` served `assemble.py`. It is kept
outside the repository and is not committed. One detail for the next box: the
stand-in must always write the `[[x]]` header of an array of tables, even when
the entry has only sub-tables. Dumps ran alone (`RAYON_NUM_THREADS=3`).

## 1. `tsr-2zk.1115`

### 1.1 (a) Printed arity of a type reference — WIP, not landed

**Status.** Implemented and measured. It is **not pushed as code** because
it costs three RIGHT lines (below). The patch is
[`r5-declared4-print-arity-WIP.diff`](r5-declared4-print-arity-WIP.diff),
taken against this branch's tip. Measured unfiltered on top of §1.2–§3 against
the frozen base `c75e4af5`: types **+42 WRONG→RIGHT and 3 RIGHT→WRONG**
(the 42 include the §1.2–§3 gains listed in §4), diagnostics no change.

The 3 losses all have the same shape:
- `generatorReturnExpressionIsChecked:0:0`
- `generatorTypeCheck64:0:11`
- `types.asyncGenerators.es2018.2:0:121`

Each is a function whose return annotation is written `Iterator<X>`. Native
prints the signature as `() => Iterator<X>` because serializeTypeForDeclaration
reuses the written return node. This port printed it right only because
§136's display arity made the *type* print short. With the type printing
native's full `Iterator<X, any, any>`, the signature printer's reuse of the
written return annotation does not fire for this reference. A probe shows
`function g(): Iterator<number>` printing `() => Iterator<number, any, any>`,
while `function* h(): Generator<number>` reuses `Generator<number>`. The
difference is `Iterator`'s esnext merge (`lib.es2025.iterator.d.ts`
redeclares it without defaults). **What is left:** find why
`written_annotation_text` (`signatures.rs`) or `node_reuse.rs` declines that
written reference, fix it there (those files are r5-printer3's and
r5-nodereuse's), then land this patch. The gains it measured (largest
first):
- `awaitUsingDeclarationsWithIteratorObject` 7;
- `usingDeclarationsWithIteratorObject` 7;
- `awaitUsingDeclarationsWithAsyncIteratorObject` 6;
- `destructuringAssignmentWithDefault2` 3;
- `builtinIteratorReturn` 2 per configuration (r5-nodereuse's handed-off
  leftover);
- `builtinIterator`, `innerTypeArgumentInference`,
  `recursiveGenericMethodCall`, `dependentDestructuredVariables` and
  `parserMissingLambdaOpenBrace1`, 2 each.


**What was wrong.** `declare const i: Iterator<string, undefined>` printed
`Iterator<string, undefined>`; native prints `Iterator<string, undefined,
any>`. §136 (printseam §6) had given a partially-written reference to a
default-library declaration a *display arity*, the written prefix
(`reference_display_arity`). The assumption was that native prints what was
written. It does not. getTypeFromClassOrInterfaceReference (`:23169`) builds
`createTypeReference(t, fillMissingTypeArguments(...))`. The type no longer
knows how many arguments were written, and typeReferenceToTypeNode prints
every argument.

**The one elision native makes** (`nodebuilderimpl.go:3084`): for a reference
to the global `Iterable`, `IterableIterator`, `AsyncIterable` or
`AsyncIterableIterator` (each resolved at arity 3, `checker.go:1088`-`1097`),
trailing arguments identical to their parameter's default are dropped. This
keeps `.d.ts` emit backwards compatible now that those interfaces have three
parameters. That is why `IterableIterator<number, any>` prints
`IterableIterator<number>`, and why `builtinIteratorReturn` was RIGHT under
§136 on some lines and WRONG on others: §136 matched native only where the
written prefix happened to equal the elided one.

**Port (in the WIP patch).**
- `reference_print_arity` is that rule. It is computed from the arguments
  alone, on the factory's miss path (`create_type_reference_with_display`),
  so it holds for every reference: written, inferred, or rebuilt by
  instantiation.
- The partially-written lib arm in `get_instantiated_type_reference` is
  removed, so such references take the same road as every other fill.
- `create_type_reference_with_display` no longer reads its `display`
  argument. It is kept only because `inference.rs` (not this lane's) still
  passes one, and it now forwards to `create_type_reference`. A follow-up can
  drop the parameter there.

**Native's `t.node` leg.** Native does not elide for a deferred reference
whose own node spells every argument. This port mints deferred references
(`deferred_alias_reference`) only for alias bodies, never for these four
interfaces, so that leg has no population here.

**Not changed: §933.2's written spelling** (`qualified_written_text` for a
partially-written reference). That is native's written-node reuse in
signatures (`serializeTypeForDeclaration`), not the type's own print. The
brief's measurement says it moves nothing on its own, and it still answers
the reuse sites correctly.

**How I would know it is wrong:** a corpus line that prints a non-Iterable
reference with fewer arguments than its target has parameters, where no
written node is being reused.

### 1.2 (b) Numeric names on merged type-literal members

`var a: { "1": number; 1.0: string; }` prints `{ 1: number; }` natively.
`PropertyExcludes` is empty, so the two declarations merge into one symbol
named `"1"`, typed from its first declaration. getPropertyNameNodeForSymbol
(`nodebuilderimpl.go:2434`) quotes the name only when *every* declaration is
string-named. The numeric declaration therefore makes it the numeric literal
`1`.

The type-literal duplicate arm (`build_type_literal`) kept the first
declaration's printed name. It now re-spells that name as the numeric literal
when a later merged declaration is numeric-named. An identifier-named later
declaration changes nothing, because a quoted first declaration whose text is
an identifier already printed bare.

**Not converted:** `numericStringNamedPropertyEquivalence` 0:10/0:11 are the
same rule on the object-literal road (`objects.rs`, r5-printer3's), so they
are not touched here.

## 2. `tsr-2zk.1102`: the instantiation-expression cache and alias frames

`InstantiationExpressionLinks::types` was keyed `(node, expression type)`.
Upstream's key is the same, but upstream computes a node once, over the
declared type parameters, and maps that result through the alias's mapper.
This port evaluates an alias body under a frame (`alias_evaluation_bindings`)
instead, so the type arguments under the node resolve to the frame's
bindings. With `type F<T> = typeof f<T>`, evaluating `F<number>` and then
`F<string>` returned `(x: number) => number` twice.

**Port.** The key gains the open frames, `flattened_alias_bindings()`, the
same flattening `type_literal_key` uses, so the two caches partition alike.
The convention record is in the module header of
`instantiation_expressions.rs`. In summary:
- **Owner:** the Checker.
- **Key:** node, expression type, and the open frames.
- **Publication:** unchanged (absent, active, completed).
- **Receiver/alias context:** the frames are now in the key.
- **Work boundary:** one signature instantiation per key. No open frame means
  an empty vector, with no allocation.

**Reports.** A computation under an open frame stands in for upstream's
mapper over the single frame-free computation, so it parks no reports. The
frame-free computation is the one that reports, as upstream's does. Without
this, a frame-bound evaluation could report a constraint failure that native,
working over the generic `T`, never does.

Unit test: `instantiation_expressions_are_keyed_on_the_alias_frame`. It fails
with the frame half of the key removed (`left: "(x: number) => number"`).

r5-declared3 §5.1's narrowed decline (`typeof C<T>` constituents keep the
alias mint) is unchanged. Keying the cache was not enough on its own to
restore that TS2352, as r5-declared3 measured; the missing piece is still
references that carry the instantiated structure.

## 3. The `tsr-2zk.1078` leftovers

### 3.1 `singletonLabeledTuple:0:17`

`type AliasRest = [...p: number[]]`. The tuple has a single Rest element (not
Variadic: the operand has an array element type node), so natively it is a
deferred reference carrying the alias. `AliasRest extends [unknown]` is
`false`.

The alias tuple road (§79.1) minted the name `AliasRest` and copied the
structural tuple's side tables onto it. This port's structural answer for a
one-rest tuple is the array normalization `number[]`, which has no tuple side
tables, so the name carried no identity at all and the conditional deferred.
When the structural answer is a reference without tuple tables, the name now
takes that reference's identity (target, arguments, member owner) through
`deferred_alias_reference`. That is the same road a class or interface
reference written as an alias body takes. The printed name is unchanged.

**Stated divergence.** Native's target is the tuple `[...number[]]` and
this port's is the array `number[]`. Both relate the same way to a
fixed-length tuple target. That is the existing §959 normalization, not
something this change introduces.

### 3.2 `namedTupleMembersErrors` 11/12

`type RecusiveRestUnlabeled = [string, ...RecusiveRestUnlabeled]`: native
reports TS2456 at both aliases (20,13 and 21,13), and both declared types are
`any`. The rest operand is a reference with no array element type node, so
the element is Variadic. getTypeFromArrayOrTupleTypeNode (`:24121`) then
**never defers the tuple**: it resolves every element now, the operand
re-enters getDeclaredTypeOfTypeAlias, and pushTypeResolution reports the
cycle.

`native_resolves_lazily` (the `enter_deferred` boundaries used while a
declared type resolves) treated every deferrable tuple as lazy, including
variadic ones. So the re-entry answered the §29 name placeholder, and the
alias printed `[string, ...RecusiveRestUnlabeled]`. The tuple arm now excludes
tuples with a variadic element, as native's condition does. Both TS2456 are
reported and both lines print `any`.

The case stays WRONG on diagnostics: it still misses five parser-grammar
diagnostics (TS5085/5086/5087/17019/2574) and reports two extra TS1110.

**Note for r5-errorsplit.** report_type_alias_circularity answers the port's
gap (`error`), and the writer's rewrite credits it as `any`. Switching that
producer to `native_error` (ADR-0048) is the contract owner's per-producer
step and is not made here.

## 4. Measured

Unfiltered, the branch (§1.2, §2, §3.1, §3.2; **not** the §1.1 WIP patch)
against the frozen base `c75e4af5`:

| | base | after |
|---|---|---|
| diagnostics RIGHT + EMPTY_RIGHT | 11080 | 11081 (+1) |
| type lines RIGHT | 549046 | 549051 (+5) |
| losses (diag / types / base-RIGHT keys missing) | | 0 / 0 / 0 |

Converted, by section:
- **§1.2:** `numericStringNamedPropertyEquivalence:0:7`.
- **§3.1:** `singletonLabeledTuple:0:17` and `contextualTypeWithTuple:0:89`.
  The second converts the whole diagnostics case: `type test1 =
  [...number[]]` now carries the array identity, so `fixed1 = test1 & {
  length: 2 }` relates.
- **§3.2:** `namedTupleMembersErrors:0:11`/`0:12`. TS2456 is reported at both
  aliases, but the case stays WRONG on its parser diagnostics.
- **§2:** no corpus line moves. It is pinned by its unit test.

Checks:
- **`slowcases`:** clean on both dump pairs (only the KNOWN_SLOW cases, none
  slower).
- **`divergentcost`:** clean. recursiveConditionalCrash3 is the same watchdog
  OOM in base and after (≈36–37 s, 3,018–3,020 MiB).
- **Ir** (callgrind, `--singleThreaded --pretty false`, base binary vs this
  branch): generic-imports 342,942,573 → 342,924,128 (−0.005%), domain-model
  1,117,973,887 → 1,118,046,608 (+0.007%).
- **Median child CPU** vs the base binary, 21 samples: domain-model 0.964,
  generic-imports 0.956; `diagnostics_match: true` on both.
- **Tests and lint:** `tsr-checker` tests pass. Clippy reports nothing in
  code touched here. The stable toolchain flags pre-existing code, including
  `unique_symbols.rs:103` `match_same_arms`, which is reported and not fixed.

## 5. Follow-ons routed by the integrator (not started: round wrap-up)

- Namespace-rooted qualified enum references minted as OBJECT Named instead of
  the enum (`enumAssignmentCompat3` 12 lines,
  `enumLiteralAssignableToEnumInsideUnion`; r5-relater7 §1, §6).
- `conditionalTypesExcessProperties`: the `Something<A>` alias image answers
  Related with no structural walk.
- `tsr-2zk.1123`: bounded conditional-alias evaluation (instantiationDepth and
  instantiationCount, TS2589). With r5-relater7's binder diff applied,
  ramdaToolsNoInfinite2 builds a 54 MB type text. Also in scope: the `any`
  prints in privacyImportParseErrors and
  moduleAugmentationImportsAndExports3. Read r5-spans.md §2.3 first.
- `inference.rs`: once §1.1 lands, drop the `display` argument at its one
  `create_type_reference_with_display` call. No behaviour change.

## 6. Diffs received after wrap-up (not landed this round)

r5-mapped6 sent its `declared.rs` diffs at 07:42 UTC, after the integrator's
wrap-up ("do not start any new item"). They are on
`claude/beautiful-shannon-ar5gh0-r5-mapped6` (head `89bc82d`), in
`docs/parity/notes/`. Each assumes batch AR's tree (`28648eb` + r5-mapped5
`0a31b0c` + its six non-route diffs). The route diff also needs `mapped.rs`
from r5-mapped6 `d43ccd8` (`evaluate_mapped_type_node`), so none of them
could be measured against this lane's base `c75e4af5` in time. The next
`declared.rs` owner lands them measured, in this order (r5-mapped6's own
numbers):

1. `r5-mapped6-declared-route.diff`: types +85, diagnostics +1
   (`bigintIndex`), zero losses. It updates one expectation in
   `tests/index_signature_members.rs`.
2. `r5-mapped6-keyof-reduced-alias-body.diff`: +1, zero losses.
3. `r5-mapped6-keyof-late-bound-keys.diff`: +3, zero losses.
4. `r5-mapped6-conditional-typed-print.diff` (`.16.71`): **held** at +5/−4
   (`controlFlowGenericTypes` 298/300/301/303). It waits on native's narrower
   instantiation key: getObjectTypeInstantiation keys on the possibly
   referenced parameters (isTypeParameterPossiblyReferenced), while
   `type_literal_key` keys every open binding. That is r5-declared3 §1.3's
   refused-for-now item, and it is this file's to port.
