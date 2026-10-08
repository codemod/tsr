# r4-unions — lane notes

Round-4 lane for union reduction and union ordering (`tsr-2zk.13.2`,
`tsr-2zk.16.86`, `tsr-2zk.16.89`). Native reference: `vendor/typescript-go`
@ `5b1047d`. Every change below was reproduced against the pinned source
before it was ported; each section names the native function, what TSR did
instead, and the cases it converts.

## 1. `CompareTypes`' intersection arm (`tsr-2zk.16.89`)

**Native.** `CompareTypes` (`internal/checker/utilities.go:415`) orders two
intersections by `compareTypeLists(t1.Types(), t2.Types())` (`:504`), after
sort flags and names tie. An intersection's list is in source order
(`addTypeToIntersection`'s `orderedSet`, `checker.go:26258`), so
`(T & F) | T & string` is printed `(T & string) | (T & F)`: `T` ties, and
`string`'s flag bit sorts below `Object`'s.

**TSR before.** `compare_types` (`crates/tsr-checker/src/unions.rs`) had no
intersection arm. Two unaliased intersections compared equal on every key
and fell to the type-id tiebreak, which is this port's creation order.

**Change.** The arm, comparing `TypeData::Intersection::types` with the
existing `compare_type_lists`. No new state. An aliased intersection still
compares by its alias name first, as `getTypeNameSymbol` does.

**Measured.** Unfiltered types dump: 14 lines gained, 0 lost; diagnostics
unchanged. Converts `compiler/narrowingTypeofFunction`; lines in
`conformance/intersectionNarrowing` (4), `conformance/unknownControlFlow` (7),
`conformance/mappedTypes4` (1).

## 2. `reduceVoidUndefined` under subtype reduction (`tsr-2zk.16.86`)

**Native.** `getUnionTypeWorker` (`checker.go:25653`) calls
`removeRedundantLiteralTypes(typeSet, includes, unionReduction&Subtype != 0)`
whenever the set holds both `void` and `undefined` (`:25675`), and the third
argument enables the clause `reduceVoidUndefined && flags&Undefined != 0 &&
includes&Void != 0` (`:25848`). So under `UnionReductionSubtype` — the
conditional expression, `||`, `??` — `undefined` disappears beside `void`:
`required ? console.log('x') : undefined` is `void`.

**TSR before.** `remove_redundant_literal_types` is the literal-reduction
call and correctly omits the clause. `union_with_subtype_reduction` never
applied it, and `is_subtype_reduction_free` (`expressions.rs`) declared
`void` reduction-free, so `check_conditional_expression` never reached the
subtype path for that pair and printed `void | undefined`.

**Change.** The clause at the top of `union_with_subtype_reduction`, after
the literal pass and before the `removeSubtypes` walk (native order), and
`void` removed from `is_subtype_reduction_free`'s safe set. That function's
contract is "literal and subtype reduction agree", which is false for
`void` beside `undefined`. Both changes are inside lane-specific functions;
the second sits in the hub file `expressions.rs`, and the issue names it as
the TSR site to change.

**Rejected.** Adding the clause to `remove_redundant_literal_types` behind a
flag. That is upstream's shape, but every literal caller would carry a dead
parameter, and the subtype entry point is the only caller that passes
`true`.

**Measured.** 25 lines gained against the frozen baseline (11 from this
commit), 0 lost; diagnostics unchanged. Converts
`compiler/truthinessCallExpressionCoercion1`.

## 3. `removeSubtypes`' class-derivation gate reads `ObjectFlagsClass` (`tsr-2zk.16.86`)

**Native.** `removeSubtypes` (`checker.go:25934`) removes a source related to
a target under `strictSubtypeRelation` unless both `getTargetType(...)`
carry `ObjectFlagsClass` and the source does not derive from the target
(`:26011`). `ObjectFlagsClass` is set on a class's declared instance type
only.

**TSR before.** `is_class_instance` (`unions.rs`) answered true for any
`Named` or `Anonymous` type whose symbol is a class. That includes the
constructor type `typeof C`, an `Anonymous` type carrying the class symbol,
and the polymorphic `this`, a `TYPE_PARAMETER` minted as `Named` with the
class as its members symbol. So `[Alpha, Beta]` kept both constructors
(neither derives from the other), and `b ? this.c : this.self` kept
`this | C`.

**Change.** `is_class_instance` requires `TypeFlags::OBJECT` and excludes
`Anonymous`. The relation then decides, as it does natively.

**Measured.** 12 lines gained, 0 lost; diagnostics unchanged. Converts
`conformance/constructorTagOnClassConstructor` and
`compiler/abstractClassUnionInstantiation` (8 lines), plus the target lines of
`conformance/typeRelationships`.

## 4. `emptyTypeLiteralType` sorts after declared object types (`tsr-2zk.16.89`)

**Native.** `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
(`checker.go:22933`) answers the shared `emptyTypeLiteralType` for a
member-less, unaliased literal (`:22939`). That type's symbol is created
without declarations (`:1024`). `CompareTypes`' object arm orders by
`compareSymbols`, and `compareSymbolsWorker` (`utilities.go:366`) puts a
symbol with declarations before one without. So `{} | { b: number }` prints
`{ b: number; } | {}`.

**TSR before.** A written `{}` is minted per node, carrying its own `__type`
symbol with a declaration, so `compare_type_symbols` ordered it by source
position and printed `{}` first.

**Change.** `compare_type_symbols` treats a type that
`is_unaliased_empty_type_literal` identifies as having no declaration
position. That predicate already exists as the port's completed identity
for `emptyTypeLiteralType` (`declared.rs`). It is read-only, and no new
state is added. Merging the per-node mints into one shared type would be
the faithful representation. It is outside this lane's files and would also
change interning, so it is not attempted here.

**Measured.** 12 lines gained, 0 lost; diagnostics unchanged. Converts
`conformance/spreadUnion2` (11 lines), plus one line in
`conformance/unknownControlFlow`. Perf median CPU ratio 0.993 / 0.997 (21
samples).
