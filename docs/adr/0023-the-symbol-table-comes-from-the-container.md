# ADR-0023 — Which symbol table a declaration goes into comes from the container, not from the declaration

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10`.

## Context

`tsr-binder` decides two things for every declaration: what [`SymbolFlags`] it
carries, and which symbol table it is filed in (`Destination::{Locals, Members,
Exports, GlobalExports}`). Both come out of one function, `classify`, keyed on the
**declaration's node kind**.

Upstream splits these. Flags are chosen by the caller, per declaration kind. The
table is chosen by `declareSymbolAndAddToSymbolTable`
(`internal/binder/binder.go:428-447`), which switches on **`b.container.Kind`** and
never looks at the node:

| `b.container.Kind` | table |
|---|---|
| `SourceFile`, `ModuleDeclaration` | that container's exports |
| `ClassDeclaration`, `ClassExpression` | the class symbol's members, or its exports if the declaration is `static` (`declareClassMember`, `binder.go:414`) |
| `EnumDeclaration` | the enum symbol's exports |
| `TypeLiteral`, `ObjectLiteralExpression`, `InterfaceDeclaration`, `JsxAttributes` | that symbol's members |
| every function-like, `TypeAliasDeclaration`, `MappedType` | `GetLocals(b.container)` |

The two models agree for most declarations because most declaration kinds only
ever occur in one kind of container. **Type parameters are the exception**: the
same `TypeParameterDeclaration` appears under a class, an interface, a function, a
method, a type alias, a mapped type, and every call/construct signature — and
upstream files it in a different table in each case.

`classify` gave it one answer, `Destination::Locals`, which `locals_owner`
resolves to the nearest *locals* container. A class is `IsContainer` **without**
`HasLocals` (`crates/tsr-binder/src/container.rs:53`, matching
`GetContainerFlags`, `binder.go:2553`), so a class is not one. The `T` of
`class A<T>` was therefore filed in the enclosing **file**, where it met the `T`
of `class B<T>` and merged with it.

### The forcing constraint

This was found from the other end. `bd tsr-y4u.18` recorded 4,464 TS2300
"duplicate identifier" diagnostics that we emit and upstream does not, and named
the unported `binder.go:208` assignment-declaration exemption as the likely
dominant cause. That hypothesis was wrong, and measurably so: the exemption tests
`SymbolFlagsAssignment`, which upstream sets only on JavaScript expando and
`this`-property declarations (`binder.go:1058`, `:1064`, `:1129`), while the
sample that motivated it (`compiler/abstractPropertyInConstructor`) is a `.ts`
file. The exemption is unreachable there.

Bucketing the over-reports by *position* — which is what
`examples/ts2300_classes.rs` does — could not see this, because a position tells
you where a collision was reported, not what declared the colliding names. A
second tool that buckets by the **declaring construct**
(`examples/ts2300_constructs.rs`) answered it in one run:

```
4350 TS2300 over-reports, by declaring construct:
  4132  TypeParameter        <- 95%
    66  TypeAliasDeclaration
    26  ExportSpecifier
     …
```

Two `T`s in unrelated classes merging into one symbol is a **resolution** defect,
not merely a noisy diagnostic. The diagnostic is how it surfaced.

## Decision

Special-case `TypeParameterDeclaration` in `classify`, choosing its destination
from the parent node the way upstream chooses from `b.container`. Do **not**
convert the binder to upstream's container-keyed model.

## Alternatives

**Convert `Destination` to be container-keyed, as upstream is.** This is the
faithful change and it is the one to make if this class of bug recurs. It was
rejected *now* because it is not a local edit: our `self.container` cursor is the
nearest `HAS_LOCALS` container, whereas upstream's `b.container` is the nearest
`IS_CONTAINER` (`bindContainer`, `binder.go:1495-1502` — upstream assigns
`b.container = node` for any container and creates `locals` only when
`HasLocals`). Making `self.container` follow upstream changes the meaning of every
one of its ~40 read sites, including `locals_owner` and the `table_owner` of every
`Members`/`Exports` declaration. That is a refactor with a wide blast radius, and
it would have been measured in the same breath as a 4,132-diagnostic behaviour
change — two variables at once, against a suite that was the only instrument.

**Fix only `TypeParameterExcludes`.** `Type & ^TypeParameter`
(`symbolflags.go:72`) means two type parameters of the same name never collide *in
the binder* at all, so correcting it alone silences all 4,132 diagnostics. It was
rejected as a standalone fix precisely because it works: it would have hidden the
table defect behind a green number while `class A<T>` and `class B<T>` went on
sharing a symbol. It is applied here as well, but as a separate correction with
its own justification (below), not as the fix for this.

## Consequences

- `classify` now answers on two different keys — node kind for everything, plus
  parent kind for one node kind. That is a genuine wart, and it is the cost of not
  doing the refactor. The special case is written to make the divergence loud: it
  cites `binder.go:429` and enumerates upstream's container cases.
- Only the symbol-owning containers are enumerated. Everything else falls through
  to `Destination::Locals`, which is already upstream's `GetLocals(b.container)`
  branch and was never wrong for type parameters.
- `enum E<T> {}` is invalid syntax that the parser still produces a node for.
  Upstream's switch would route it to the enum's exports, so that case is listed
  rather than left to fall through to the enclosing scope.
- **Not fixed:** `declareClassMember`'s `IsStatic` split (`binder.go:415`). Type
  parameters are never static, so it cannot be reached from this path; any future
  caller routed through here would need it.

## How we would know this was wrong

- The 4,132 figure is a diagnostic count over the corpus, not a case count. If
  `examples/ts2300_constructs.rs` attributes a diagnostic to the wrong construct —
  it picks the smallest declaration span containing the diagnostic span — the
  bucket is wrong and so is the conclusion drawn from it. The independent check is
  the probe: `class A<T> { } class B<T> { }` produced one symbol with two
  declarations before the change and two symbols after
  (`examples/ts2300_probe.rs`).
- If a second declaration kind turns out to need the same treatment, the
  special-case model has failed and the container-keyed refactor should be done
  instead (`bd tsr-y4u.22`). `TypeAliasDeclaration` (63 remaining over-reports) is
  the first place to look.

## Follow-ups

Filed rather than done, so that none of this is documented as though it were built:
`bd tsr-y4u.19` (the remaining 190 over-reports, bucketed), `tsr-y4u.20` (the
`excludes()` constants that are too *narrow*), `tsr-y4u.21` (the stale RSS gate
figure, see `docs/architecture/performance.md`), `tsr-y4u.22` (the container-keyed
refactor), `tsr-y4u.23` (the three still-unported blocks of `declareSymbol`,
including the `binder.go:208` exemption this ADR shows is not the cause of the
over-reports).

## The `excludes()` corrections made alongside

`SymbolFlags::excludes()` derives upstream's parallel `…Excludes` constants from
the declaration's own flags. The rationale in its doc comment still holds — a new
symbol kind that forgets its excludes silently permits every redeclaration — but
the derivation had drifted from the constants it claims to reproduce, in a
direction no gate could see: **an excludes value that is too broad produces a
diagnostic upstream does not have**, and until the `diagnostics` suite existed
nothing compared our diagnostics to upstream's at all.

Four were wrong (`symbolflags.go:59-72`):

| | upstream | was |
|---|---|---|
| `PropertyExcludes` | `Value & ^(Property\|Accessor)` | `PROPERTY` |
| `MethodExcludes` | `Value & ^Method` | `PROPERTY` |
| `Get`/`SetAccessorExcludes` | `Value & ^(other accessor \| Property)` | missing the `Property` term |
| `TypeParameterExcludes` | `Type & ^TypeParameter` | `TYPE` |
| `ClassExcludes` | `(Value\|Type) & ^(ValueModule\|Interface\|Function)` | missing the `Function` term |

`PropertyExcludes` was inverted outright: a property does not exclude another
property. Two properties of the same name in one table merge, and the *checker*
decides what to say — `TS1117` for an object literal, `TS2717`/`TS2687` for a
member. Reading it as `PROPERTY` made every such duplicate a binder `TS2300`.

**Left alone, deliberately:** the constants where our derivation is *looser* than
upstream — `ValueModuleExcludes`, `RegularEnumExcludes`, `ConstEnumExcludes`
(`excludes()` returns `empty()` for any module, and does not separate regular from
const enums). Correcting those makes the binder report *more*, which is where
regressions live, and none of them appear in the measured over-report buckets.
They are tracked separately rather than folded into a change whose whole
justification is removing false positives.

## Measured effect

`binder_symbols` **8,278/8,449 (97.98%) before and after**, at every step — the
constraint this work was held to. Each change was measured on its own; over-report
counts are diagnostics, not cases:

| | TS2300 over-reports | `diagnostics` |
|---|---|---|
| `0e17464` | 4,723 | 79/5,488 |
| `PropertyExcludes` | 4,545 | 77/5,488 |
| type-parameter table | 350 | 77/5,488 |
| remaining `excludes()` | **286** | 77/5,488 |

**`diagnostics` did not improve, and went down by two.** Both facts are real and
neither is a reason to revert:

- The −2 is `PropertyExcludes`. Two cases passed only because our binder's
  `TS2300` happened to sit exactly where the *checker's* `TS2300` belongs. Moving
  the diagnostic to the component that owns it is correct; we cannot yet emit it,
  so those cases now fail as under-reports instead of passing by coincidence.
- The other 4,437 removed over-reports bought nothing on that suite because the
  cases carrying them also miss thousands of checker diagnostics, and a case fails
  once. That was the point: `bd tsr-y4u.18` argued these had to go *before* the
  checker lands, because a binder false positive cannot be removed by adding a
  checker — it would persist underneath the checker's output and corrupt the suite
  exactly when it becomes the checker's gate.

The honest summary is that this is a correctness change with no headline number
attached to it, and the number it does have moved the wrong way by two.
