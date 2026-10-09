# r6-accessible — alias accessibility at print (`tsr-2zk.1136`)

Pinned tsgo `5b1047d`. Base frozen at `claude/beautiful-shannon-ar5gh0` @
`0180457` (batch BB; batch BD had not landed when the box started).
Native oracle: `scripts/offline-cargo/build-tsgo.sh`. PyPI is blocked, so a
stdlib-only stand-in for `assemble.py`'s three `tomlkit` calls sat on
`PYTHONPATH` outside the repository (`r5-operators3.md` §4).

## 0. What the port is

Native's type printer asks the accessibility walk in two places:

- the alias arm of `typeToTypeNodeHelper` (`nodebuilderimpl.go:3362`): a type
  with an alias prints `Alias<Args>` only when `IsTypeSymbolAccessible(alias,
  enclosingDeclaration)` holds (the `.types` baseline never sets
  `UseAliasDefinedOutsideCurrentScope`). Otherwise the type prints its
  structure;
- `serializeTypeName` (`nodebuilderimpl.go:436`), which the reuse visitor
  calls when a written entity name does not survive the move to the print
  site (`tryVisitTypeReference`, `nodecopy.go:442`). It requires
  `IsSymbolAccessible(symbol, enclosingDeclaration, meaning, false)` to be
  `Accessible`, or the reference is serialized from its type.

The walk behind both (`isSymbolAccessibleWorker`, `IsAnySymbolAccessible`,
`getAccessibleSymbolChain`, `getContainersOfSymbol`, `hasVisibleDeclarations`)
was already ported whole for the declaration emitter's tracker
(`crate::symbol_accessibility`, `r5-declemit3.md` §2). The port here is
`crates/tsr-checker/src/alias_accessibility.rs`. It asks that walk from the
printer and adds nothing to it. It does not touch
`getAccessibleSymbolChain`/`trySymbolTable` (`tsr-2zk.39`).

**The forcing fact.** `serialize_type_name` (node_reuse.rs) used
`has_visible_declarations` as its whole accessibility test. That is only the
second half of native's test. A non-exported top-level `type Id` of an
external module *is* visible to `hasVisibleDeclarations`: it is a
late-painted statement whose parent source file is visible
(`IsLateVisibilityPaintedStatement`). What refuses it in native is the first
half. From the importing file there is no accessible chain, and `Id` has no
container, so `isSymbolAccessibleWorker` answers `CannotBeNamed`. The port
named `Id` through the import-route namer and printed `Id<…>` where native
prints the structure.

## 1. The 35 item-1 lines, classified against native

Verdicts are frozen base → full stack (commit + the three diffs of §2).

| Case | Lines | Base | Stack | Cause of what remains |
|---|---|---|---|---|
| `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1` | `1:0 1:3 1:11` | WRONG | RIGHT | — (diff 1) |
| | `1:1 1:2 1:12` | WRONG | RIGHT | — (diff 3) |
| `…NoStrictNullChecks2` | `1:0 1:3 1:11` / `1:1 1:2 1:12` | WRONG | RIGHT | — (diffs 1 / 3) |
| `…NoStrictNullChecks3` | `1:0 1:3` | WRONG | RIGHT | — (diff 1) |
| | `1:1 1:2 1:11 1:12` | WRONG | WRONG | §3 (c): `originalArgs?: undefined` for native `never` |
| `declarationEmitInlinedDistributiveConditional` | `0:0 0:4 1:2 1:3 1:5` + 1 | WRONG | WRONG | §3 (a): `import("./internal").PublicKeys1` — chain naming, `.39` |
| | `0:1 0:12 1:1 1:8 1:9 1:11 1:12` | WRONG | WRONG | §3 (b): `PublicKeys2` is inaccessible (the gate now says so), but a conditional alias instance has no structure to print |
| `mappedTypeGenericInstantiationPreservesHomomorphism` | `1:0 1:1 1:2 1:4 1:5` | WRONG | WRONG | §3 (b): `PrivateMapped<T[any]>` is an opaque generic-alias mint |

That is 14 of the 35 converted. The stack also converts 10 lines outside
the list: `aliasOnMergedModuleInterface` ×2,
`declarationEmitUnnessesaryTypeReferenceNotAdded(target=es2015)` ×2,
`exportEqualErrorType` ×3 and `exportEqualMemberMissing` ×3. All 10 come
from diff 1. Each is a written reference to a name that is not accessible
at the print site, which the port was naming through the import route.

## 2. The commit and the diffs, in apply order

**Commit** (this lane's files): `alias_accessibility.rs` (the two questions,
plus `prints_accessible_alias` for the alias arm), its `mod` line, and
`tests/alias_accessibility.rs`. The tests are four two-file controls taken
from the native baselines' shapes:
- a non-exported alias of another module cannot be named;
- an exported one can, through its module (`allowModules`);
- an imported one can;
- the non-exported alias is accessible in its own file.

The commit alone changes no output: nothing calls it until the diffs land.

Each diff applies alone on the commit (`git apply --check`). All
measurements are against the frozen base, unfiltered.

| # | Diff | Files | Types | Diagnostics |
|---|---|---|---|---|
| 1 | `r6-accessible-serialize-type-name.diff` | node_reuse.rs (unowned) | **+18 / −0** | 0 changed |
| 2 | `r6-accessible-alias-arm.diff` | checker.rs (main) | 0 / −0 on top of 1 | 0 changed |
| 3 | `r6-accessible-mapped-object-site.diff` | checker.rs (main), mapped.rs (unowned), declared.rs (r6-declared) | **+6 / −0** on top of 1+2 | 0 changed |
| | all three | | **+24 / −0** (types RIGHT 549,881 → 549,905) | 0 changed |

1. **`serializeTypeName`'s gate.** `serialize_type_name` asks
   `is_symbol_accessible_at(symbol, site, meaning)` in place of
   `has_visible_declarations`. This covers NoStrictNullChecks1–3's
   signature lines and the 10 outside lines.
2. **The alias arm's gate.** The `alias_of` arm of `type_to_string_at` runs
   only when `IsTypeSymbolAccessible` holds. It is neutral on today's corpus,
   because no `alias_of` type is printed where its alias is inaccessible. It
   is native's operation, though, and diff 3 relies on the same gate (below).
3. **A resolved mapped object prints its members at the site.**
   `createTypeNodeFromObjectType` → `createTypeNodesFromResolvedType`
   (`nodebuilderimpl.go:2690`, `:2627`): `mapped_object_text` gains a site,
   so each slot goes through `type_to_string_at`. This is how the expanded
   `Id<…>` inside `{ useTestQuery: () => … }` reaches diff 1's structure.
   The arm runs only when the type has no alias, or its alias is not
   accessible at the site (`prints_accessible_alias`).
   - **Measured first without that gate: +24 / −222.** Every loss was an
     alias-named mapped object (`Record<string, any>`, `BB`, `HTML`) that
     native names because the alias *is* accessible.
   - Generic alias instances carry the alias in `type_reference_targets`.
     Gating on those alone left −24.
   - The non-generic §909 mint (`type BB = { [x in …]: number }`, declared.rs)
     dropped the alias: its only record was the baked name.
   - The declared.rs hunk writes `alias_of` there, as
     `getTypeFromMappedTypeNode` gives the type `getAliasForTypeNode`
     (`checker.go:24255`, `:23711`; ADR-0045 rule 2). With it, −24 became 0.

### Ownership and work boundaries (checker port convention)

- **Native operation**: `isSymbolAccessibleWorker(symbol, enclosing, meaning,
  false, true)`'s verdict, read by the alias arm and by `serializeTypeName`.
- **Key identity and owner**: **no new cache.** Native caches the chains,
  never the verdict. Each query builds a `DeclarationEmitResolver`, whose
  `AccessibilityCache` is keyed as native's chain cache (symbol,
  external-aliasing flag, first scope location, meaning) and is dropped
  after the query.
  - A verdict cache keyed (alias symbol × enclosing declaration × meaning)
    on the `Checker` was built first and measured identical (+24 / −0).
  - It was removed because it needs a field in main's `checker.rs`, and
    there is no cost to pay down: the walk runs only when a type is
    printed. That is never on the bench projects' clean runs (Ir below).
  - What would bring it back: a profile in which `is_symbol_accessible_at`
    shows above noise, e.g. a project printing many diagnostics with
    aliased types.
- **Publication**: none. Alias resolution forced by the walk is the
  checker's own memo. With `shouldComputeAliasesToMakeVisible = false`,
  `hasVisibleDeclarations` paints nothing, so a short-lived resolver answers
  what native's long-lived `EmitResolver` would.
- **Receiver/alias context**: only the enclosing declaration (the
  printer's `reference`, as native's `ctx.enclosingDeclaration` is the
  baseline node's parent). Alias arguments never enter the question.
- **Expensive work boundary**: one chain walk per printed alias or refused
  entity name. A refusal's error-name strings are computed by the shared
  worker and discarded. Diff 3's member print is the creation-time print's
  own walk, with the same truncation budget, run at the site.
- **`alias_of` writer (diff 3)**: key `TypeId` of the §909 mint, value (alias
  symbol, no arguments). Written once at creation, never mutated. Readers:
  the printer, `unions.rs`' alias-first ordering, declared.rs' re-mint
  checks and `symbol_access.rs`. All are unchanged on the corpus (0 lines
  moved beyond the +6).

### Gates (full stack)

- Both dumps unfiltered against the frozen base: diagnostics 0 changed,
  types +24 / −0, no missing keys.
- `slowcases`: clean on both dumps.
- `cargo test --workspace --release`: 356 test binaries pass.
- `cargo clippy`: nothing in touched files. Stable 1.97 flags pre-existing
  code in `templates.rs`, `signatures.rs`, `enum_initializer.rs`,
  `unique_symbols.rs`, `printing.rs`, `members.rs`, `index_signatures.rs` and
  `tsr-dts/tests/accessibility.rs`.
- `cargo fmt`: clean.
- Ir, release `tsr --singleThreaded --pretty false`, callgrind. CLI output
  is `cmp`-identical on both projects:

| Project | Base | Stack | Δ |
|---|---:|---:|---:|
| domain-model | 1,091,167,905 | 1,091,171,984 | +0.0004% |
| generic-imports | 343,082,704 | 343,072,711 | −0.003% |

## 3. What remains of item 1, with causes

**(a) An accessible alias of another module is named through an import type
— `.39`.** Native prints `import("./internal").PublicKeys1<keyof Obj>`
(`lookupSymbolChain` → the module container → `getSpecifierForModuleSymbol`).
The gate answers *accessible* here (`allowModules`), which is right. The
spelling needs the chain printer, and on top of it a site render of the
**generic** mapped type `{ [K in …]: Obj[K]; }`, whose constraint text is
baked when it is minted. Six lines.

**(b) An inaccessible generic alias whose instance has no structure —
ADR-0045 (`tsr-2zk.16.2`, main) and declared.rs.** Both `PublicKeys2<keyof
Obj>` (a deferred conditional) and `PrivateMapped<T[any]>` (a homomorphic
mapped type over a non-type-parameter) are opaque mints:
`Named("Alias<Args>")` with a `type_reference_targets` entry and no
conditional or mapped parts. Native's fallbacks are:
- `keyof Obj extends infer T ? T extends keyof Obj ? … : never : never` —
  `conditionalTypeToTypeNode`'s distributive arm over a non-type-parameter
  check type;
- `T[any] extends infer T_1 ? { [K in keyof T_1]: T_1[K]; } : never` —
  `createMappedTypeNodeFromType`'s homomorphic-instantiation arm.

`prints_accessible_alias` already answers `false` at these sites. The
missing piece is the instance's structure (ADR-0045 rule 4: the declared type
stops minting `Name<Params>`). Twelve lines.

**(c) Not accessibility: optional `never` becomes `undefined` without
`strictNullChecks`.** Three lines `NoStrictNullChecks3:1:1/1:2/1:12`, plus
`1:11` which shares the cause (four in all). Native probe, with
`strictNullChecks: false`:

```ts
type Id<T> = { [K in keyof T]: T[K] } & {};
declare function h<D>(d: D): () => Id<{ a?: never; d?: D }>;
const q: number = h(1)();   // native: '{ a?: never; d?: number; }'
                            // TSR:    '{ a?: undefined; d?: number; }'
```

The result is right without the `& {}`, right for an inline
`{ a?: never; d?: D } & {}`, and right for `M<…> & {}` where `M` is a
separate alias. It is wrong only for an inline mapped node inside an
intersection alias body, instantiated under an outer mapper: declared.rs's
alias-body evaluation. None of `mapped.rs`' optionality sites fire, because
all are gated on `strict_null_checks`. Owner: r6-declared.
