# r6-declared: `declared.rs`, round 6

Lane files: `declared.rs`, `instantiation_expressions.rs`, `unique_symbols.rs`.
Frozen base: `claude/beautiful-shannon-ar5gh0` `b18aec06` (main `17265fac`
plus bookkeeping). Vendor pinned at `5b1047d`.

## 0. Frozen base

Unfiltered, release build of `b18aec06`:
- `diagverdictdump`: RIGHT 5530, EMPTY_RIGHT 5596, WRONG 1063, EMPTY_WRONG 49
  (12,238 rows);
- `verdictdump`: RIGHT 549,853, WRONG 5,607, GAP 843 (556,303 rows).

Setup note: PyPI answers 403, so `assemble.py`'s three `tomlkit` calls ran on
a stdlib-only stand-in kept in the session scratchpad, not committed
(`r5-operators3.md` §4).

## 1. getObjectTypeInstantiation's referenced-parameter key

**Forcing constraint.** getObjectTypeInstantiation (checker.go:22304-22352)
computes an anonymous type's `outerTypeParameters` once per declaration. When
the type carries no alias type arguments (it is not the right-hand side of a
generic type alias) and its symbol is a `Method` or `TypeLiteral` (a type
literal, a mapped type, a function or constructor type node, a method), it
filters that list with isTypeParameterPossiblyReferenced (checker.go:22403).
An empty list answers the type itself, the uninstantiated literal. The port's
`type_literal_key` keyed every open alias-evaluation binding. So
`{ id?: number | string }` inside `type Column<T> = (keyof T extends never ?
{ id?: number | string } : { id: T }) & …`, evaluated under the frame
`T := T`, was a fresh literal, and its members printed as types
(`string | number`), where native keeps the written literal and prints
`number | string`. The written-text mint of a deferred conditional hid this.
r5-mapped6's typed conditional print exposed it (its four losses at
`controlFlowGenericTypes:298/300/301/303`, `r5-mapped6.md` §2).

**Port.** `type_literal_key` keeps only the bindings for which
`is_type_parameter_possibly_referenced` holds, for exactly native's node
kinds and only when `type_alias_host_for_type_node` names no generic alias.
`is_type_parameter_possibly_referenced` ports the pinned function:
- a symbol with other than one declaration answers `true`;
- walking from the node up to the parameter's declaration container, a
  `Block`, a conditional whose `extends` clause references the parameter, or
  running off the tree (the node is outside the parameter's scope) answers
  `true`;
- otherwise `containsReference`: an argument-less type reference whose
  identifier resolves (type meaning) to the parameter; a `typeof` whose first
  identifier resolves to a declaration inside the parameter's scope, or whose
  type arguments reference it (`this`, or an unresolvable shape, answers
  `true`); a method with a body and no return annotation answers `true`, and
  otherwise only its type parameters, parameters and return type are walked.

**The members follow the key.** The instance's members resolve under the
mapper of the parameters it keys on. With none, it is the declared literal,
resolved with no mapper at all. The port's member road reads the open frames
directly: `reused_annotation_text` (`node_reuse.rs`) declines while any frame
is open. So a literal keyed on no binding, but first built inside a frame,
still printed its member as a type: `C<"a">` for
`type C<T> = T extends string ? { id?: number | string } : never` printed
`{ id?: string | number; }`. `get_type_from_type_literal` now builds the
literal under exactly its key's bindings: one frame of the kept bindings, or
none. The caller's stack is restored afterwards. The native tsgo probe
(`--declaration`) prints `{ id?: number | string; }` for that alias and for
`W<boolean>`'s `inner`, and `{ id?: string | number | undefined; }` for a
literal that names the bound parameter.

**Divergence kept.** A bound symbol that is not a type parameter answers
`true` (the old, unfiltered key). No frame binds one today; native's
`this`-type arm has no counterpart in the frames.

**Checker port convention.**
- Native operation: `typeNodeLinks.outerTypeParameters`, written once per
  declaration by getObjectTypeInstantiation.
- Key identity and owner: `(declaration NodeId, type-parameter SymbolId) ->
  bool` in `InstantiationExpressionLinks::possibly_referenced`. That links
  struct is this lane's; `Checker`'s field list is main's, so the memo lives
  there rather than in a new field. It is a `RefCell` because
  `type_literal_key` takes `&self` at about twenty call sites in other lanes'
  files.
- Publication: written once per key, never invalidated. The answer is syntax
  plus name resolution, neither of which depends on a frame.
- Receiver/alias context: none. The frames decide which symbols are asked
  about, never the answer.
- Work boundary: one ancestor walk and at most one subtree walk per key. A
  subtree walk resolves a name only when the identifier's text equals the
  parameter's name.

**Measured** (unfiltered, both dumps, against §0): diagnostics and types
unchanged, zero losses, slowcases clean. Callgrind Ir (`tsr -p <project>
--singleThreaded --pretty false --noEmit`): domain-model 1,090,817,385 ->
1,090,253,218 (-0.05%), generic-imports 343,083,431 -> 343,055,471 (-0.01%). The port changes identities only
where a literal names none of the open bindings, and today's printers render
the merged identities the same way. Its effect is the diff it unblocks (§1.1).

Tests: `tests/r6_declared.rs`:
- `a_resolved_branch_naming_no_parameter_is_the_written_literal` fails on the
  base;
- `a_literal_naming_no_bound_parameter_is_the_written_literal` and
  `a_literal_naming_the_bound_parameter_is_instantiated` pin the neighbours,
  which already passed.

**Falsifier.** A literal whose port evaluation reads a frame through a path
`containsReference` does not see. That would be a dynamic-scope leak in the
alias road, for example a non-generic alias body evaluated under the caller's
frames. Such a literal would merge two different instances, and a print
would show the first instance's members.

### 1.1 r5-mapped6's conditional-typed-print diff, re-measured (sent to r6-mapped)

[`r5-mapped6-conditional-typed-print.diff`](r5-mapped6-conditional-typed-print.diff)
applies unchanged on top of §1's commit `f9339d0` (only the line offsets
move). Unfiltered against `f9339d0`:
- types: **+5 RIGHT, 0 losses**:
  - `complicatedIndexesOfIntersectionsAreInferencable:0:5`;
  - `simplifyingConditionalWithInteriorConditionalIsRelated:0:17/19`;
  - `wideningWithTopLevelTypeParameter:0:47`;
  - `mappedTypeAsClauses:0:106`.

  r5-mapped6's four held losses (`controlFlowGenericTypes:298/300/301/303`)
  are gone.
- diagnostics unchanged; slowcases clean on both dumps.
- Ir: domain-model 1,090,253,218 -> 1,092,009,836 (+0.16%, the same +0.15%
  r5-mapped6 §2 measured), generic-imports +0.002%.

It touches `declared.rs` and `node_reuse.rs`, so it stays a diff. It was sent
to r6-mapped as the round-6 brief asks; the integrator orders it after
`f9339d0`.

## 2. `tsr-2zk.1123`: ramdaToolsNoInfinite2's 54 MB type text

**Forcing constraint.** With r5-relater7's binder diff
([`r5-relater7-binder-declare-module-imports.diff`](r5-relater7-binder-declare-module-imports.diff),
main's `binder.rs`, so it stays a diff), ramdaToolsNoInfinite2's imports
resolve. The dump then aborted: `type_reference_text` formatted a 54,525,882-byte
argument list (2.5 GiB peak, `memory allocation … failed`). Instrumented, the
port was at instantiation **depth ~90 with a count of ~4,600**; the texts
doubled at every level (`Overwrite`, `Required`, `Naked`, `Length`,
`__Reverse`). The native tsgo probe checks the same file in 0.4 s with
**48,750 instantiations, no TS2589**. Native's two bounds (depth 100, count
5,000,000, checker.go:22111) are never reached, so bounding the port's count
would not be faithful. The port recursed where native does not.

The cause is `__Reverse`:

```ts
type __Reverse<L, LO, I = IterationOf<'0'>> = {
    0: __Reverse<L, Prepend<LO, L[Pos<I>]>, Next<I>>;
    1: LO;
}[Extends<Pos<I>, Length<L>>];
```

Native's getIndexedAccessType reads the one property the index names
(getPropertyTypeForIndexType -> getTypeOfSymbol). An anonymous type's members
are resolved on demand, so arm 0 is instantiated only while the index selects
it, and the recursion ends when `Extends<…>` turns to 1. The port's
indexed-access road built the object literal first, both arms included, so
arm 0 recursed at every level until the depth guard. Each level's arguments
contain the previous level's, so the texts doubled.

**Port.** `indexed_type_literal_member` (declared.rs), first in the
`IndexedAccessTypeNode` arm: for a written type literal whose members are all
plain property signatures (written annotation, no `?`, static name) and an
index that evaluates to a string or number literal naming exactly one of
them, it answers that member's annotation type and builds nothing else. Every
other shape declines to the eager road unchanged: index signatures, methods,
accessors, optional members, and generic or union indexes.

**A cycle keeps the eager road.** `limitDeepInstantiations`' `type Foo<T, B> =
{ "true": Foo<T, Foo<T, B>> }[T]` re-enters its own instantiation key: the
selected member's first argument is the instance being computed. Native
recurses to the depth guard and caches errorType for that key, which
collapses the recursion (27,369 instantiations, TS2589). This port's alias
road declines at the guard, answering the named reference rather than
errorType (`r5-spans.md` §2.3). So a lazily read member that re-enters never
collapses. The first draft OOMed at 6 GiB, and a draft that handed only the
re-entrant read back went from 30 MiB to 975 MiB (slowcases `SLOWER`). The
arm now records the literal's publication state:
- `lazy_member_reads`: keys whose selected member is resolving;
- `eager_indexed_literals`: literal nodes that re-entered. Such a node takes
  the eager road from then on, whose reserved literal identity closed the
  cycle before this arm existed.

A self-re-entrant literal is a static property of its alias. Reading its outer
levels lazily would add one eager chain per level.

**`keyof T` is one identity.** The deferred `keyof T` mint
(`get_type_from_type_node_worker`'s keyof arm) made a new named type per
evaluation. Native's getIndexType caches one index type per generic type
(`resolvedIndexType`). The eager literal had hidden this: its member was
cached inside the cached literal. Read lazily, `x is { a: keyof T }["a"]`
evaluated `keyof T` twice, and pseudoReturnTypeMatchesPredicate's identity
test failed (`type_predicates::predicates_retain_resolved_mapped_and_indexed_types`).
The mint is now memoized per `(operand type, printed text)`.

**Checker port convention.**
- Native operations: getPropertyTypeForIndexType -> getTypeOfSymbol, with an
  anonymous type's lazy members; getIndexType's `resolvedIndexType`.
- Keys, owner and publication, all in `InstantiationExpressionLinks` (this
  lane's links struct; `Checker`'s fields are main's):
  - `lazy_member_reads`: `TypeLiteralKey`, inserted before and removed after
    the member read;
  - `eager_indexed_literals`: the literal `NodeId`, inserted once and never
    removed;
  - `deferred_keyof_mints`: `(operand TypeId, text) -> TypeId`, written once
    when minted.
- Receiver/alias context: the open alias frames, through `type_literal_key`
  (§1).
- Work boundary: one annotation evaluation per selected read, instead of the
  whole literal.

**Divergence kept.** At the guard, native answers errorType and reports
TS2589. Here a re-entrant cycle takes the eager road, as on the base. That
waits on the alias road answering errorType at the guard, which r5-spans §2.3
showed is not safe until this port stops reaching depth 100 where native does
not.

Tests: `tests/r6_declared.rs`
`an_indexed_type_literal_resolves_only_the_selected_member` (`Count<3>` is
`3`; an unresolvable sibling does not poison `["a"]`). It passes on the base
too. A doubling fixture that OOMs the base does not exercise the arm in a
lib-less unit test: its index evaluates to `error` there. The falsifier is
the conformance case itself: ramdaToolsNoInfinite2 with the binder diff
OOMs without this arm.

**Measured** (unfiltered, both dumps, against §0): diagnostics and types
unchanged, zero losses, slowcases clean on both dumps
(`limitDeepInstantiations` 45 ms / 30 MiB at the base; the rejected
hand-back draft took it to 863 ms / 973 MiB). Ir against §1's `f9339d0`:
domain-model 1,090,253,218 -> 1,089,528,215 (-0.07%), generic-imports
343,055,471 -> 343,061,332 (+0.002%). With the binder diff on top,
ramdaToolsNoInfinite2 completes (5.8 s, 249 MiB) instead of aborting; §2.1
has that diff's unfiltered numbers.

### 2.1 The binder diff's `any` prints

Re-measured on top of `91e85aa`, unfiltered: r5-relater7's binder diff now
completes. Against §0:
- diagnostics +3 cases: `moduleAugmentationInAmbientModule1`,
  `moduleAugmentationInAmbientModule5`, `ramdaToolsNoInfinite2`;
- types +23 lines;
- 11 type lines lost.

The losses fall in three groups:

**`privacyImportParseErrors:0:564/569/594/599`: ported (this commit).**
`var v: m`, where `m` is an unexported `import m = require("glo_M2_public")`,
is now a local of its `declare module`. Native's resolveName tests an alias
by `getSymbolFlags(resolveAlias(s)) & meaning`. A target with no type
meaning fails that test, and so does the unknown symbol of an unresolved
target. resolveTypeReferenceName then mints getUnresolvedSymbolForEntityName,
which prints the written name. The ImportEquals arm of
`get_type_from_type_reference` answered `error` (`any`) instead. It now
answers `unresolved_type_reference` for a resolved target without a type
meaning, and for an unresolved `require(…)` target.

The unresolved qualified target (`import a = x.c`) keeps `error`. Its written
print is native's too, and it gains `importDeclWithClassModifiers:0:9`, but
it loses `:0:3`. On that line the alias declaration `b`, merged with `var b:
a`, prints the var's type where native prints the alias's `any`, which is a
merge print outside this lane.

Measured without the binder diff: types +1 (`exportEqualsProperty2:0:1`),
zero losses, slowcases clean. Ir domain-model 1,089,529,559, generic-imports
343,057,790 (flat against `91e85aa`). With the binder diff, all four lines
are RIGHT.

**`moduleAugmentationImportsAndExports3:3:4`: diff, not landed.** `B` is an
ES import inside `declare module "./f1"`. Native also fails to resolve it
(TS2667, TS2307: the loader does not collect imports inside an augmentation,
references.go:48-70, and the port's collector agrees). Native prints `B` from
the unresolved symbol and types `b` as `any`. Native prints the written name
for any unfindable import too: the tsgo probe prints `X` for `import { X }
from "./missing"; let v: X`. The port's ES-import road answers `error` when
the module is unfindable. Answering the written name there (the ES-import
half of
[`r6-declared-unresolved-alias-written-name.diff`](r6-declared-unresolved-alias-written-name.diff))
gains 23 lines, but loses `asyncAwaitIsolatedModules_es2017`/`_es5`/`_es6`
(diagnostics). The cause is the unresolved mint's typing, not its print:
- native's unresolved reference is errorType, and
  checkAsyncFunctionReturnType returns on `isErrorType(returnType)`;
- the port's `check_async_function_return_type` (`expressions.rs`, not this
  lane's) tests `== error`, so TS1064 fires on the mint.

The diff's `expressions.rs` half asks `is_error`, which counts the unresolved
mint, as native asks `isErrorType`.

The whole diff (`declared.rs` ES-import road plus `expressions.rs`), measured
unfiltered on `861e945`:
- without the binder diff: **types +21, zero losses**:
  - `asyncAwaitIsolatedModules_es2017`/`_es5`/`_es6`, 5 lines each;
  - `decoratorMetadataTypeOnlyImport:0:1`;
  - `isolatedDeclarationErrorTypes1:0:9`;
  - `unusedInvalidTypeArguments:2:1`;
  - `defaultExportsCannotMerge01/02/03:1:3`.

  Diagnostics unchanged and slowcases clean.
- with the binder diff: `moduleAugmentationImportsAndExports3:3:4` is RIGHT,
  so the binder diff's remaining losses are only ramda's six lines below.

`expressions.rs` is not this lane's file, so the integrator lands it.

**`ramdaToolsNoInfinite2:0:448/449/485/490/491/492`: other lanes.**
- 485 and 490-492 print a defaulted argument native elides (`_Drop<…, "->">`,
  `List<any>`). That is `tsr-2zk.1115`'s print arity (§5).
- 448 and 449 are `ONonNullable<…>`, a renamed import
  (`NonNullable as ONonNullable`) that the reference road refuses to print
  under its local name (the §158 per-site naming wall).

**Slow.** `ramdaToolsNoInfinite2` is RIGHT and EMPTY_RIGHT with the binder
diff, but it takes 25 s in the diagnostics dump and 5.8 s in the types dump.
At the base it took 0.24 s and 0.17 s, and native takes 0.4 s. slowcases
flags it SLOWER, so the binder diff stays held on time as well (§2.2).

### 2.2 The binder diff's time: one conditional, one registry walk

With the binder diff, `ramdaToolsNoInfinite2` took 31 s through the CLI. gdb
stack samples put every one of them in `mentions_type_parameter_inner`,
called from `conditional_extends_instantiations`. That function found the
type parameters a conditional's extends type mentions by running one full
graph walk per registered type parameter, the whole registry, on every
conditional evaluation. Native computes getPermissiveInstantiation and
getRestrictiveInstantiation once per type and caches them on the type
(`permissiveInstantiation`, `restrictiveInstantiation`).

Ported:
- `conditional_extends_instantiations` memoizes its `(permissive,
  restrictive)` pair, or `None`, per extends `TypeId`.
  `InstantiationExpressionLinks::conditional_extends` is written once per
  type and never invalidated.
- The mentioned set is found by order-preserving bisection over the
  candidates (`collect_mentioned_type_parameters`). One walk asks a whole
  slice, and a slice nobody mentions is dropped, so the cost is O(k log n)
  walks for k mentioned parameters of n candidates. The answer is exactly the
  per-candidate filter's, in the same order. `mentions_type_parameter`'s
  walker is `inference.rs`'s, and asking a slice is its public form.

CLI on the case with the binder diff: 31.4 s -> 2.5 s (bisection) -> 2.1 s
(memo); native takes 0.4 s. Dumps with the binder diff: diagnostics 26 s ->
1.7 s, types 5.4 s -> 1.2 s. slowcases still flags it SLOWER against the base
(0.24 s / 0.17 s): the base never resolved the case's imports, so it did not
evaluate the aliases at all. What is left is spread over the eager alias
road, with no single hot spot.

Without the binder diff, unfiltered: zero losses, slowcases clean, gains
unchanged (`exportEqualsProperty2:0:1`, §2.1). Ir against §2.1's commit:
domain-model 1,089,529,559 -> 1,089,344,728 (-0.02%), generic-imports
343,057,790 -> 343,074,440 (+0.005%).

Also in this commit, at r6-relater's request:
`ConditionalInferenceNode::declaration` is `pub(crate)`, for
`conditionalTypeAssignabilityWhenDeferred` 41/65. No behaviour change.

## 3. A namespace-rooted qualified enum reference answers the enum

**Forcing constraint.** r5-relater7 §1 found `First.E`, written in a type
position with `First` a namespace, minted as an OBJECT-flagged `Named` type.
The qualified road's enum arm (QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE,
getTypeReferenceType's declared type) was scoped to an alias-rooted name. A
namespace-rooted enum kept §41's mint, so `Abc.Nope.a -> First.E` never
reached isEnumTypeRelatedTo, and `z = "x"` with `z: First.E` was silent.

**Port.** The arm admits any root. It still answers the enum's declared type
only where that type prints as the written text does at the site, so no
rendered line moves. The earlier scoping recorded 3 R→W from a union's
named-constituent guard (`boolean | X.Foo` printing `any`); those lines do not
move now.

**Measured** (unfiltered, both dumps, against §0; this commit alone on
`e100e74`):
- diagnostics **+2 cases**: `enumLiteralAssignableToEnumInsideUnion` and
  `enumAssignmentCompat7`, WRONG -> RIGHT;
- types unchanged, zero losses, slowcases clean;
- Ir domain-model 1,089,344,728 -> 1,089,283,795, generic-imports
  343,074,440 -> 343,062,841 (both within noise).

**`enumAssignmentCompat3` stays WRONG on one row.** Line 70, `abc = secondCd`:
`Cd.E` is `{ c, d }`, whose `c` is 0, where `First.E.c` is 2. Native's
isEnumTypeRelatedTo walks the source's members and reports "Each
declaration of 'E.c' differs in its value". The port relates the pair. That
is the enum-relation arm (`relater.rs`, r6-relater's lane). The other 11
rows report now.

The conformance cases are the falsifier. A unit test cannot show it, since
the print is the same before and after.

## 4. `conditionalTypesExcessProperties`: `Something<A>` is the intersection

**Forcing constraint.** Native reports TS2322 at both assignments in

```ts
type Something<T> = { test: string } & (T extends object ? { arg: T } : { arg?: undefined });
function testFunc2<A extends object>(a: A, sa: Something<A>) {
    sa = { test: 'hi', arg: a };
}
```

because `Something<A>` is `{ test: string } & (A extends object ? … : …)`.
The conditional is deferred: its check type is a type parameter. Traced with
a temporary relation print, the port's `Something<A>` was a member-less
`Named` mint, related as Related with no walk. `instantiate_intersection_alias`
gave up because `instantiate_type` (`inference.rs`) answers `error` for the
deferred conditional constituent.

**Port.** `conditional_constituent_instantiation`: when `instantiate_type`
declines on a CONDITIONAL constituent that records its node
(`ConditionalInferenceNode`), the constituent is instantiated as
getConditionalTypeInstantiation does. Its node is evaluated under the
captured bindings plus the alias's parameters bound to the arguments, with
instantiateTypeWithAlias' depth and count guard. A check type that stays
generic gives the deferred conditional. `Something<A>` is now an INTERSECTION
of `{ test: string }` and that conditional.

**Not converted yet; two pieces remain.**
- The relation to a deferred conditional target answers `Unknown`. Even
  `sa = { arg: a }` against an inline `A extends object ? { arg: A } :
  { arg?: undefined }` is silent, where native's conditional-target arm
  (isDistributionDependent, then both branches) answers False. That is
  `relater.rs`, r6-relater's lane.
- The constituent under frames keeps the written text (`T extends object ?
  …`) rather than `A extends object ? …`. That is the conditional typed print,
  r5-mapped6's held diff (§1.1), which waits on lazy branch text
  (`tsr-2zk.1135`).

**Measured** (unfiltered, against §0, this change alone on `b81a7f1`): no
verdict moves, zero losses, slowcases clean. Ir domain-model 1,089,283,795 ->
1,089,348,302, generic-imports 343,062,841 -> 343,061,841 (noise). The
falsifier is the case itself once the relater arm lands. Until then a
`Something<A>` that still related as Related would show the old name mint
again.

## 5. `tsr-2zk.1115`(a): the printed arity of a type reference (r5-declared4's WIP, landed)

The port is r5-declared4 §1.1's patch
([`r5-declared4-print-arity-WIP.diff`](r5-declared4-print-arity-WIP.diff)).
§136's written-arity display is retired. A reference prints every argument
after fillMissingTypeArguments, except typeReferenceToTypeNode's one elision
(`nodebuilderimpl.go:3084`): trailing default-identical arguments of the
global `Iterable`, `IterableIterator`, `AsyncIterable` and
`AsyncIterableIterator` are dropped.

**Depends on r6-printer's `12d9d73`**
(`r6-printer-global-augmentation-visible.diff`, `node_reuse.rs`).
`is_declaration_visible` tested IsExternalModuleAugmentation only for
string-named modules, so lib's `declare global { interface Iterator … }` was
invisible, and the written `Iterator<X>` return was not reused. Without that
diff this commit loses r5-declared4's three `Iterator<X>` lines; the
integrator lands that diff first.

**Ir.** The WIP cost domain-model +0.225% and generic-imports +0.058%
(integrator: +0.22%/+0.06%) over the printer diff alone. Callgrind
(inclusive, generic-imports) put the cost in `reference_print_arity`, which
looked up the four globals (`global_type_symbol_with_arity` and
`merged_symbol`) on every reference before knowing whether the target was one
of them. Native resolves them once, at checker creation (checker.go:1088-1097).
Ported:
- the targets are resolved once per checker
  (`InstantiationExpressionLinks::iterable_elision_targets`, written on first
  use, never invalidated);
- a target whose name is not one of the four answers first, since each
  global's merged symbol carries its name. The global lookup itself costs
  about 10k Ir per checker, and generic-imports runs one checker per file.

A probe with the elision loop disabled moved nothing, so the loop is not the
cost. Result, over the printer diff alone: domain-model 1,089,345,350 ->
1,089,441,153 (+0.009%), generic-imports 343,069,605 -> 343,069,170
(-0.0001%).

**Measured** unfiltered, with the printer diff applied under this commit, on
`55cbd3f`:
- types **+37 RIGHT, zero losses**;
  - `usingDeclarationsWithIteratorObject` 7,
    `awaitUsingDeclarationsWithIteratorObject` 7,
    `awaitUsingDeclarationsWithAsyncIteratorObject` 6;
  - `destructuringAssignmentWithDefault2` 3;
  - `builtinIteratorReturn` 2 per configuration, `builtinIterator`,
    `innerTypeArgumentInference`, `recursiveGenericMethodCall`,
    `dependentDestructuredVariables` and `parserMissingLambdaOpenBrace1`,
    2 each;
- diagnostics unchanged, slowcases clean.

It also covers ramdaToolsNoInfinite2 485/490-492's defaulted-argument prints
under the binder diff (§2.1), which r5-declared4 attributed to this arity.
Those lines are not re-measured here.

Left: `inference.rs` still passes `display` to
`create_type_reference_with_display`, whose argument is now unread
(r5-declared4 §5). The main lane can drop it; no behaviour change.
