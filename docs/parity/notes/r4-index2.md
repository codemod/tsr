# Lane notes: r4-index2 (tsr-2zk.905)

Judgment calls made by the `r4-index2` parity box, continuing `r4-index`
([r4-index.md](r4-index.md)). Numbers are measured with `verdictdump` /
`diagverdictdump` against the baseline frozen at `ce0af35` (r4-index head
`582545c` merged with integration head `c02dbbd`). Native behaviour is
reproduced with a `tsgo` built from the pinned submodule (`5b1047d`) by
`scripts/offline-cargo/build-tsgo.sh`.

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
