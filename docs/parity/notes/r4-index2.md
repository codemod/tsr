# Lane notes: r4-index2 (tsr-2zk.905)

Judgment calls made by the `r4-index2` parity box, continuing `r4-index`
([r4-index.md](r4-index.md)). Numbers are measured with `verdictdump` /
`diagverdictdump` against the baseline frozen at `ce0af35` (r4-index head
`582545c` merged with integration head `c02dbbd`). Native behaviour is
reproduced with a `tsgo` built from the pinned submodule (`5b1047d`) by
`scripts/offline-cargo/build-tsgo.sh`. §1 is measured against the baseline
frozen at `ce0af35`; §2 and §3 against one re-frozen at `f270987`, after the
integration head `6417762` was merged.

## 1. TS2374 groups by key type over the symbol's `__index` member

**Forcing constraint.** `checkTypeForDuplicateIndexSignatures`
(`checker.go:4904`) reads `getIndexSymbol(getSymbolOfDeclaration(node))`,
the `__index` entry of `getMembersOfSymbol`, and files every signature with
exactly one typed parameter under each constituent of
`getTypeFromTypeNode(parameterType).Distributed()`. Every signature of a key
filed twice is reported with `typeToString(key)`. The port's
`check_duplicate_index_signatures` instead read the `string`/`number` keyword
written on one declaration. That missed three shapes and over-reported one:

| Shape | Witness | Port before |
|---|---|---|
| union and non-keyword keys (`string \| number`, `symbol`, `` `foo${string}` ``, `T`) | `indexSignatures1` (70-73, 88, 90) | 0 of 8 lines |
| merged declarations (`declare namespace` twice, one interface each) | `genericClassesRedeclaration` (3, 42) | 0 of 2 |
| parse-recovered signatures in a file with syntax errors | `optionalPropertiesSyntax` (31-34) | 0 of 4 (whole check skipped) |
| `static` and instance signatures of one class | `staticIndexSignatureAndNormalIndexSignature` | 2 extra |

Statics are bound into the class's exports, not its members, so they never
meet the instance signatures and are never checked against each other:
pinned `tsgo` on `class A { static [x: string]: number; static [y: string]:
number; }` reports nothing.

**The once-per-symbol flag is replaced by locality.** Native guards the
class/interface call with `links.indexSignaturesChecked`, set by whichever
declaration is checked first, and that one check reports on every merged
declaration. The port has no declared-type links to hang the flag on, and
adding a symbol set would be a new side table for a boolean. Instead each
declaration computes the symbol-wide grouping and reports only the
signatures it owns. The diagnostic set is identical whenever every
declaration is checked; it differs only for a declaration that is never
checked (a default-library interface): native, reached from the user's
merged declaration, would also report on the library side, which no
baseline shows. Falsifier: a corpus case whose baseline carries a TS2374
in `lib.*.d.ts`.

**The parse-error guard is dropped for this check.** Native runs it on
parse-recovered trees, and the recovered `[idx: number]?: any` /
`? [idx: number]: any` / `[idx?: number]: any` signatures each still have
one typed parameter. Measured: no loss in either dump.

**Declined.** A key the port could not compute (`error`) is not filed;
native would file `errorType` and print `any`. Class expressions are not
checked because `check.rs` does not dispatch this check from its
`ClassExpression` arm (native: `checkClassLikeDeclaration`, pinned `tsgo`
reports `const C = class { [x: number]: number; [y: number]: number; }`
twice). The function accepts a class expression; the one-line dispatch is
outside the owned files and is in
[r4-index2-class-expression-duplicate-index.diff](r4-index2-class-expression-duplicate-index.diff)
(its corpus effect is not measured).

Port convention record: no cache, side table, mapper or traversal is added;
the merged symbol's declarations are scanned syntactically and each key
type node is resolved by the existing `get_type_from_type_node`, once per
checked declaration that has at least two index signatures across the
merge.

**Measured** against the frozen baseline: diagnostics RIGHT 4255 -> 4257,
EMPTY_RIGHT 4968 -> 4969 (`genericClassesRedeclaration`,
`optionalPropertiesSyntax`, `staticIndexSignatureAndNormalIndexSignature`;
`indexSignatures1` gains its 8 TS2374 lines and stays WRONG on other codes);
`checker_types` unchanged; both loss checks empty. Perf at 21 samples, median
child CPU new/old: domain-model 1.008, generic-imports 1.000.

## 2. TS7053 for an `any` key (tsr-2zk.905)

**Forcing constraint.** `getPropertyTypeForIndexType` enters its index-info
arm when `isTypeAssignableToKind(indexType, StringLike|NumberLike|ESSymbolLike)`
(`checker.go:27083`), which `any` satisfies. `getApplicableIndexInfo(objectType,
any)` then finds any info at all (`isApplicableIndexType` asks
`isTypeAssignableTo(any, key)`), so an `any` key misses only on a receiver
with **no** index info, and reaches the TS7053 report. The object-literal
shortcut at `:27135` answers only a `string`/`number` key (property union) or
a literal key (TS2339); an `any` key falls past it. r4-index's reporter
declined every `any` constituent and every object-literal receiver, so
`var emptyObj = {}; emptyObj[hi]` with `hi: any` was silent
(`noImplicitAnyIndexing:30`).

Pinned `tsgo` on `{}`, a class instance and `{ a: number }` receivers with an
`any` key: three TS7053 lines, text identical to this port's after the
change; a receiver with only a number index is silent in both.

**Kept declines.** An `error` key is still declined ([box protocol](../box-protocol.md) §3a: this port's `error`
is "not computed", not native's `errorType`). That leaves `newOperator:56`
(`new M.T[]`, a missing argument natively typed `errorType` and printed
`any`) open: the missing-expression producer would have to answer native's
`errorType` distinctly from "not computed".

Port convention record: no cache, side table or traversal; two predicates
in the existing reporter widen, and the `any`-key early exit reads the
`get_index_infos_of_type` answer the reporter already computes.

## 3. A tuple's number index (cluster 3; delivered as a diff)

**Forcing constraint.** `getTupleBaseType` (`checker.go:19206`) gives a tuple
target the base `Array<E>` (`ReadonlyArray<E>` when readonly), `E` the union
of its element type arguments with a variadic element contributing
`T[number]`; `resolveObjectTypeMembers` copies the base's `[n: number]: T`
into the tuple, so `getIndexInfosOfType([string, number])` is
`[n: number]: string | number`. `get_index_infos_of_type` fell through to
`Some(vec![])` for every tuple, a confident "no index". Native, pinned
`tsgo`: `[string, number]` is not assignable to `{ [n: number]: string }`
("'number' index signatures are incompatible"), and `[string, boolean?]`
is not assignable to `{ [n: number]: string | boolean }` (`undefined`).

**The owned half alone loses three cases.** With only the tuple arm,
`destructuringParameterDeclaration3ES5`, `...3ES6` and `...4` went RIGHT to
WRONG: `a5([1, 2, "string", false, true])` against `[any, any, [[any]]]`
reported TS2345 at the argument instead of TS2322 at `"string"`. Root
cause, reduced to `var q: [number] = "s"` (silent with the arm; TS2322
without it and natively): the relater's primitive-source arm
(`relater.rs`, `is_related_to`) sends any target with index infos through a
members-only proof (`has_members(target)`), which a tuple cannot satisfy,
and answers `Unknown`; a tuple with no index infos used to take the full
`is_related_to(apparent, target)` road, which compares `length` and the
element properties. The fix exempts tuple targets from that arm, so they
keep the road they had; the arm's other targets are unchanged.

Both hunks are in
[r4-index2-tuple-number-index.diff](r4-index2-tuple-number-index.diff)
(`relater.rs` is not owned). Measured on top of `b9dd8e2` against that
commit's dumps: types +11 RIGHT (`unionsOfTupleTypes1` 6 lines,
`controlFlowBindingPatternOrder` 4, one GAP in
`avoidNarrowingUsingConstVariableFromBindingElementWithLiteralInitializer`),
diagnostics unchanged, both loss checks empty. Perf at 21 samples, median
child CPU against the frozen baseline: domain-model 0.983, generic-imports
1.011. Callgrind on `generate_perf_project.py --modules 100`: 4,358,094,195
Ir without, 4,362,152,483 with (+0.09%); the tuple arm alone was
4,358,603,673 (+0.01%), so most of the rise is tuple targets of primitive
sources now taking the full relation road.

**Representation choices.** An optional element's `undefined` is added to
the union because this port keeps it in `tuple_optional_masks` rather than
in the element type argument (`addOptionality` in
`getTypeFromTupleTypeNode`); `variadic_tuple_index_union` does the same. The
print-only shapes (`tuple_rest_tails`, `variadic_tuple_nodes`) answer `None`
(gap), not "no index". No cache or side table: the existing tuple tables are
read per call.

**Not lifted.** The TS7053/TS7015 reporter still declines tuple receivers
(`index_infos_are_declared`); no corpus TS7015/TS7053 line on a tuple
receiver is missing at this baseline, so lifting it could only add risk.

## 4. TS2411/TS2413 misses that remain are outside the owned files

Every remaining TS2411/TS2413 miss in the corpus (frozen baseline, 14
lines in 9 cases) was reproduced against pinned `tsgo` and traced to a producer
this lane does not own. None is a defect in `checkIndexConstraints`' port.

| Case (lines) | Native | Blocker in this port |
|---|---|---|
| `enumIsNotASubtypeOfAnythingButNumber` (95), `unionSubtypeIfEveryConstituentTypeIsSubtype` (110, 111) | `string \| number` and `number` not assignable to `typeof f`, `f` a function merged with a namespace | `relate_ternary(string, typeof f)` answers `Unknown` (`var z: typeof f = "s"` reports nothing; a plain function's `typeof h` does). Relater, main's lanes. |
| `indexTypeCheck` (22) | `Orange` not assignable to `Yellow` through `string -> Red` (index-signature-only interface) | `var r: Red = "x"` reports nothing: primitive source against an index-signature-only target answers `Unknown`. Relater. |
| `objectTypeHidingMembersOfExtendedObject` (10) | a user `interface Object` augmentation; `@skipDefaultLibCheck: false` makes native check `lib.es5.d.ts`'s `Object` first, and `links.interfaceChecked` reports the user's `data` member from there | `check_interface_heritage_conformance` (`heritage_conformance.rs`) runs the interface arm only from the symbol's first interface declaration, which is the unchecked library one. Fix shape (prototyped here, not measured): run from every interface declaration and keep only the error nodes the declaration being checked owns; it needs the caller's first-declaration gate lifted, so nothing is committed. |
| `interfaceExtendingClass2` (11) | parse-recovered interface member still checked | `check_heritage_conformance` returns early on `file_has_parse_errors`, before `check_index_constraints` is reached. |
| `interfaceExtendsObjectIntersectionErrors` (33 x2, 34, 35) | `interface I14 extends TCX` where `type TCX = typeof CX` | the interface's base walk does not follow an alias of `typeof Class` (`members.rs` `base_symbols_of`), so property names are not enumerated. |
| `indexSignatureInOtherFile`, `indexSignatureInOtherFile1` (2 x2 each) | `class Test extends Array1`, `Array1` a `var` of a construct-signature type, inherits `[Symbol.iterator]` / `[Symbol.unscopables]` | `get_property_names_of_type(Test)` answers `None` for a class whose base is a value expression (`members.rs`); the port also reports a false TS2415 there (heritage). |

**Considered and not committed.** For an interface with an instantiated
base (`interface I5 extends G<number> { [k: string]: string }`),
`get_property_of_type` misses every inherited name because
`base_symbols_of` refuses a base with type arguments, so the loop in
`check_index_constraints_of_type` skips them. An owned-file fallback that
takes the declaring symbol from the uninstantiated base walk (the value
still comes from `get_type_of_property_of_type`) matches native on direct
reproduction (`len`, `[Symbol.iterator]` of `Array1<number>`), but no corpus
case has such a missing line, so it was left out rather than shipped
unmeasured against a case.
