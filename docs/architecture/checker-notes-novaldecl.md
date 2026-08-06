# `resolves, but the symbol has no value declaration` — the row is aliases

Fifth session. `depend.rs` at HEAD ranks this root at **2,404 lines, want-any
687 (28.6%), 447 cases** — the largest gap root nobody had opened.
`examples/novaldecl.rs` opens it.

## The split

`get_type_of_symbol`'s flag dispatch is **complete** — accessor, variable /
property, function / class / enum / module, enum member, alias, export marker
— so these lines are not a missing arm in that switch. Classifying instead by
the symbol's shape, over 900 classified lines:

| shape | lines | want-any |
|---|---:|---:|
| `ALIAS`, `ImportEqualsDeclaration` | 399 | 115 |
| `ALIAS`, `ImportSpecifier` | 214 | 45 |
| `ALIAS`, `ImportClause` | 130 | 21 |
| `ALIAS`, `NamespaceImport` | 73 | 13 |
| `INTERFACE` used in value position | 35 | 26 |
| `ALIAS`, `NamespaceExportDeclaration` | 24 | 1 |
| `TYPE_ALIAS` in value position | 9 | 9 |
| the rest (multi-declaration merges) | 16 | 4 |

**840 of 900 are import aliases.** The row has one owner.

## And that owner already refused it, in writing, for a reason that still holds

`resolve_alias` (`symbols.rs`) returns `None` for the import-clause,
namespace-import and `export =` forms, and its comment says why:

> Every other alias form … reaches its target through module resolution and
> then prints the **alias's own** name rather than the target's. `bd tsr-4jk`
> has the measurement: `import * as ns from "./m"` records `>ns : typeof ns`,
> so resolving it in this port would print `typeof <the stripped file path>` —
> a wrong line where a gap stands.

So the largest unopened root is **not a resolution problem, it is a naming
problem**, and it is the same family as the qualified-naming build `STATUS.md`
§5 refused at 2.7 wrong per right. Resolving these symbols is easy; printing
them is the item, and printing them wrong converts 840 gaps into 840 wrong
lines.

`bd tsr-4jk` carries it. Nobody should re-open this row expecting an alias
arm — the arm exists and the refusal is downstream of it.

## Control note, recorded rather than smoothed

C1 (every classified line answers `errorType` today) read **39 of 900**, not
0. The probe reaches the symbol with its own `resolve_name` call, which is not
byte-for-byte the route `types_producer` takes, so ~4.3% of what it classifies
is not the line the producer gapped. The shape of the finding — 93% aliases —
is far outside that margin, but the per-bucket counts carry it and should be
read as ±4%. A control that fires is reported, not tuned.
