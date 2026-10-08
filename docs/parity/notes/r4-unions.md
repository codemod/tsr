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
