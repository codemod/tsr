# r5-declemit3 — the declaration-emit SymbolTracker, round 5

Lane notes for round-5 box `r5-declemit3` (`bd tsr-2zk.1024`, covering
`tsr-2zk.1000` and `tsr-2zk.1001`), continuing r5-declemit2
(`docs/parity/notes/r5-declemit2.md` §4) and building on r5-modules' TS2883
tracker hook (`b24a56d`, `docs/parity/notes/r5-modules.md` §6). Pinned
upstream: `vendor/typescript-go` @ `5b1047d`.

Box baseline: the integration head `a1e453d` with
`claude/beautiful-shannon-ar5gh0-r5-modules` (`b24a56d`) merged, as the brief
asked; diagnostics 5,314 RIGHT / 5,576 EMPTY_RIGHT over 12,238 judged keys
(plain and configured).

## 1. TS4113/TS4114 for late-bound member names (`tsr-2zk.1001`)

`checkMemberForOverrideModifier` (`checker.go:4729`) looks the member up as
`getPropertyOfType(thisType, symbol.Name)` where `symbol` is
`getSymbolOfDeclaration(member)` — the **late-bound** symbol. A computed name
`lateBindMember` binds (`[foo]` for a `unique symbol` const, `[prop]` for a
literal-typed const) is therefore found under its bound name in the class's
own type and in the base type.

The port took the binder symbol's name, which is `__computed` for every
computed member, so neither lookup found anything and the member returned
before any report: no TS4113 for `override [foo]() {}` over a base without
it, no TS4114 for an unmarked `[prop]()` that overrides one. The name now
comes from `late_bound_members_of` (the same table `getPropertyOfType`
answers from here, and the one r4-heritage's `issueMemberSpecificError` fix
already reads), keyed by the member's declaration. A computed name that is
not late-bindable keeps `__computed` and finds no property, as natively.

The table is read only when `hasLateBindableName` holds — an entity-name
expression whose type is a string literal, number literal or `unique symbol`
(`TypeFlagsStringOrNumberLiteralOrUnique`). `late_bound_members_of` also
names a computed member keyed by a *plain* `symbol` entity (`[sym]` with
`sym: symbol`), because the printer shows those as index-info components;
native never late-binds them, so the member keeps `__computed`. The first
version read the table unconditionally and invented a TS4114 in
`overrideLateBindableIndexSignature1(noimplicitoverride=true)` (RIGHT →
WRONG on the full run); the gate removed it.

The TS4113 spelling suggestion stays on the bound name; for a unique-symbol
name native's `__@foo@N` never matches a candidate either.

Measured: +3 diagnostics rows (`override21`,
`overrideLateBindableName1` × 2 `noImplicitOverride` variants).

## 2. The SymbolTracker's `TrackSymbol` arms (`tsr-2zk.1000`)

### 2.1 `IsSymbolAccessible`, ported whole

`crates/tsr-checker/src/symbol_accessibility.rs` ports
`symbolaccessibility.go` as one unit, as methods of the
`DeclarationEmitResolver` (native reaches `hasVisibleDeclarations` through
`GetEmitResolver()`, and that resolver already owns the visibility links):
`isSymbolAccessibleWorker`, `IsAnySymbolAccessible`, `getContainersOfSymbol`,
`getWithAlternativeContainers`, `getAlternativeContainingModules`,
`getAliasForSymbolInContainer`, `getAccessibleSymbolChainEx` with its
per-call visited-table map, `trySymbolTable`, `getCandidateListForSymbol`,
`isAccessible`, `canQualifySymbol`, `needsQualification` and
`someSymbolTableInScope`, plus `IsTypeSymbolAccessible` /
`IsValueSymbolAccessible` (no alias painting: `hasVisibleDeclarations` now
takes `shouldComputeAliasToMakeVisible`, and only paints when it is set, as
`addVisibleAlias` does).

**Why not re-point the printer's naming pieces.** `Checker::symbol_chain`,
`needs_qualification` and `is_shadowed_at` (`checker.rs`) each answer one
printer question, and the conflated `needs_qualification` OR is load-bearing
for 6,850 `.types` lines (`checker-notes-sitename.md` §8). Re-pointing them
is measured printer work in another lane's file. The tracker needs only the
verdict, so the walk is written whole and called from the tracker alone.
**What would change this:** a declaration-emit node builder; the printer's
`lookupSymbolChain` would then call this port, and the pieces would go.

**Port facts the walk depends on.**

- The binder gives an exported declaration a *local* symbol carrying only
  `ExportValue` with `export_symbol` set (`binder.rs`, `declareModuleMember`),
  exactly as native. `trackComputedName`'s fallback resolves the name at its
  own location and gets that local, whose `parent` is unset, so
  `getContainersOfSymbol` finds the file but `getAliasForSymbolInContainer`
  finds no export that is the *same reference* — and the result is
  `CannotBeNamed` from the file. That is why an exported `Foo` and a
  non-exported one both report TS4023 in
  `declarationEmitComputedPropertyNameSymbol1/2`.
- The port has no `globalThis` symbol (binder `merge_into_globals`).
  `trySymbolTable`'s globals arm and the `typeof globalThis` serialization
  both reduce to one question — does a table in scope before `c.globals`
  hold a `globalThis` of value meaning — which
  `is_global_this_accessible` answers by name.

**Declines** (each only removes a route or a container, listed in the
module docs): `getExportsOfSymbol` reads raw exports plus `export *`, not
late-bound statics; `getAlternativeContainingModules` reads statement-level
specifiers only and has no all-files fallback (the checker holds no program
file list); the JavaScript `exports.A = class {}` container arm.
`symbolToStringEx` for the error names is the accessible chain's names, else
the declaration name; a module name is the quoted module path without its
leading `/` (`"type"`, `"bug"`), which is native's for the root-relative test
files.

Checker port convention record: in the `AccessibilityCache` doc comment
(native `accessibleChainCache`, `extendedContainersByFile`,
`symbolTableAliasCache`; owned by the resolver, one declaration-diagnostics
run; final on first publication; no receiver context beyond the scope
location in the key; alias resolution and export tables are the expensive
work).

### 2.2 The walk over the inferred type

`DeclarationEmitResolver::track_type_symbols` (`symbol_access.rs`) is the
structured walk r5-declemit2 §4 asked for. It follows what native's
`typeToTypeNode` would write:

| Shape | Native | Here |
|---|---|---|
| union / intersection without alias | each constituent | same |
| object literal, spread, type literal written as `{ … }` | `createTypeNodesFromResolvedType` → `addPropertyToElementList` | the captured `anonymous_properties` image (its `origin` is the property's declaring symbol), else the declared members |
| late-bound (`unique symbol`) property name | `trackComputedName` → `TrackSymbol(…, Value)` | same, through §2.1 |
| a type-literal alias | written by name if `IsTypeSymbolAccessible`, else expanded | same; the alias is the `TypeAliasDeclaration` holding the literal |
| `unique symbol` | `typeof sym` if value-accessible, else `ReportInaccessibleUniqueSymbolError` | same, for a type minted from a written `unique symbol` |
| `typeof globalThis` | `TrackSymbol(globalThisSymbol, …, Value)` | `is_global_this_accessible` |

**The printer's decision is read off its text.** Whether a type-literal type
prints structurally or as an alias name is the printer's choice; this port
records no alias for a non-generic type-literal alias (`alias_of` holds only
deferred references), so the walk expands a literal whose printed text opens
with `{` and treats any other as the alias it names. This reads the port's
own decision rather than guessing native's.

**Not followed** (reports missed, never invented): named references
(`TrackSymbol` on a class, interface or non-literal alias — the bulk of
native's calls), signatures, index signatures, mapped types, a unique symbol
minted for a `Symbol()` call (no recorded symbol, `tsr-2zk.1005`), and every
declaration kind other than a variable declaration (property declarations
and return types are still declined by the walk in `tsr_dts`).

The class-expression arm r5-declemit2 built (`report_private_properties`) is
still reached first and separately: it reads the class's static and instance
property lists, which this walk does not visit (a class symbol is a named
reference here). Folding it in waits for the class arm of this walk.

**Message selection.** `handleSymbolAccessibilityError` in `tsr_dts` picks
`getVariableDeclarationTypeVisibilityDiagnosticMessage`'s three variants by
`ErrorModuleName` and `CannotBeNamed` (TS4023 / TS4024 / TS4025), at the
declaration's name. Other contexts keep the private-name messages and
decline a result that names a module.

**Measured** (full unfiltered run against the box baseline, both dumps):
diagnostics +5 rows (`declarationEmitComputedPropertyNameSymbol1`,
`declarationEmitComputedPropertyNameSymbol2`,
`declarationEmitReadonlyComputedProperty` — TS4023;
`globalThisDeclarationEmit` — TS4025;
`declarationEmitExpressionWithNonlocalPrivateUniqueSymbol` — TS2527), with
the message text checked against the baselines; zero RIGHT/EMPTY_RIGHT
losses; `.types` lines unchanged (543,727 RIGHT). Median child CPU against
the baseline binary: domain-model 1.036 at 21 samples, 0.976 at 41;
generic-imports 0.990 at 21; diagnostics match. The walk only calls
`IsSymbolAccessible` for a late-bound `unique symbol` name, a type-literal
alias, a written `unique symbol` or `typeof globalThis`, so the common
inferred type costs one pass over its captured properties.

## 3. Remaining clusters, with what each needs

Measured over the box baseline's WRONG rows: every remaining
declaration-family or TS4xxx miss, by producer.

| Code | Cases | Needs | Home |
|---|---|---|---|
| 2883 | 4 (`declarationEmitCommonJsModuleReferencedType`, `…ObjectAssignedDefaultExport`, `…ReexportedSymlinkReference3`, `…UsingTypeAlias1`) | the module-object route and symlinked package specifiers (r5-modules §6) | r5-modules |
| 2527 `this` | 1 (`declarationFiles`, 4 diagnostics) | inferred types of class **property declarations** and **method return types** asked by the `tsr_dts` walk (`ensure_type` asks only variables), the `this` type under `FlagsInObjectTypeLiteral`, and array element types walked through `Array<…>` type arguments | this lane, next |
| 4032 | 1 (`declarationEmitExpandoPropertyPrivateName`) | the expando-function arm of the transform (`GetPropertiesOfContainerFunction`) in `tsr_dts`, **and** `TrackSymbol` on named references (an interface `I` local to `a.ts`) | this lane; the named-reference arm is the risky half (§3.1) |
| 4118 | 1 (`declarationEmitMappedTypeTemplateTypeofSymbol`, 2 diagnostics) | resolved members of a mapped type over `typeof sym`: the port keeps `{ [TKey in unique symbol]: true; }` with no member list, so `addPropertyToElementList`'s no-declaration arm (`ReportNonSerializableProperty`) has nothing to visit | `mapped.rs` (not this lane) |
| 4023 (JS) | 1 (`jsDeclarationsTypeReassignmentFromDeclaration2`) | declaration emit over JavaScript files, which `tsr_dts` does not walk | — |
| 4105 | 1 (3 diagnostics) | `checkIndexedAccessIndexType` (`checker.go:8220`), absent from the port along with its TS2536 arm | main's `indexed.rs` |
| 4109 / 4110 | 2 | `getTypeArguments`' circularity reports (`checker.go:21924`) | main's type-reference code |
| 4111 | 1 | `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11362`) | main's `members.rs` |

### 3.1 Why named references are not tracked yet

Native calls `TrackSymbol` for every symbol `lookupSymbolChain` names —
every class, interface, alias and `typeof` reference in every inferred type.
With §2.1 in place the walk could do the same, but the measured reward is one
case (TS4032, which also needs the expando arm), while the arm would ask
`IsSymbolAccessible` on every named reference in every declaration-emitting
test, where any infidelity in §2.1's declines (no all-files containing-module
fallback, no late-bound statics) turns an EMPTY_RIGHT row WRONG. It should be
built together with the expando arm and measured on its own.
