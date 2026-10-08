# r4-jsx — JSX element-type checks (`tsr-2zk.907`, under `tsr-2zk.8`)

Round-4 cloud lane notes. Continues `docs/parity/notes/jsx.md` (rounds 1–3);
section numbers there are not reused here. Native source is
`vendor/typescript-go` @ `5b1047d`, `internal/checker/jsx.go`.

## 1. The `JSX.ElementType` branch of `checkJsxOpeningLikeElementOrOpeningFragment`

**Forcing constraint.** `jsx.go:140-150`: when `getJsxElementTypeTypeAt`
answers a type, upstream does not run `checkJsxReturnAssignableToAppropriateBound`
at all. It relates the *tag's* type — `getStringLiteralType(tagName.Text())`
for an intrinsic tag, `checkExpression(tagName)` otherwise — to that type
and reports TS2786 on the tag name, chained over TS2789 `Its type '{0}' is not
a valid JSX element type.`. Until this round `check_jsx_component_bound`
declined every element in such a file (jsx.md §6), so intrinsic tags missing
from a literal `ElementType` never reported (`jsxElementTypeLiteral`: two
TS2786 missing).

**What is ported.** `check_jsx_element_type_constraint` and
`jsx_element_type_type_at` (`crates/tsr-checker/src/jsx_component.rs`).
The branch is taken from inside `check_jsx_component_bound` before its
intrinsic-tag decline, because upstream takes it for intrinsic tags too. The
diagnostic is `NewDiagnosticChain(TS2789, TS2786)` at the tag-name span; the
relation's own elaboration beneath the TS2789 head is not modelled (code,
span and both message texts are upstream's). Only a definite
`Ternary::NotRelated` reports.

**`instantiateAliasOrInterfaceWithDefaults` (`jsx.go:1032`).** With no
written arguments every type parameter takes its default. Reused
`Checker::instantiated_heritage_base(symbol, &[], location)`, which already
implements `fillMissingTypeArguments` over defaults (unknown/any fallback,
JS implicit-any) and `create_type_reference`, which serves both an interface
and a type alias (`getTypeAliasInstantiation`). Rejected: a JSX-local
default filler — a second copy of `fillMissingTypeArguments`. Where the
heritage helper declines (a required parameter with no default in a TS file),
the check is skipped; upstream would instantiate with `errorType`
arguments. No such shape exists in the corpus: the three cases that declare
`JSX.ElementType` (`jsxElementType`, `jsxElementTypeLiteral`,
`jsxElementTypeLiteralWithGeneric`) write none or `P = any`.

**`getJsxElementTypeSymbol`** is `getSymbol(exports, "ElementType",
SymbolFlagsType)`: the export is resolved through an alias and must carry a
type meaning; `jsx_type_symbol` does not filter by meaning, so the filter is
applied here.

**Measured.** `jsxElementTypeLiteral` WRONG → RIGHT; no other verdict moved
in either dump. No cache or side table is added: the constraint is
recomputed per element through `get_declared_type_of_symbol` and the
instantiation table that `create_type_reference` already owns.

**Remaining, not this lane's.** `jsxElementTypeLiteralWithGeneric` line 21:
`ElementType<any>` (a mapped type indexed by `keyof JSX.IntrinsicElements`
with a conditional over `P`, unioned with `React.ComponentType<P>`) is
instantiated, but `"ruhroh"` relates to it as `Related`. Upstream's
`any extends X ? K : never` yields `K`, so the type is
`keyof IntrinsicElements | ComponentType<any>`, which `"ruhroh"` does not
satisfy. The gap is in the mapped/indexed alias instantiation producer
(`declared.rs`, type-aliases lane `tsr-2zk.16.2` on `main`), not here; the
check declines on `Related`, so it costs a missing TS2786, never a false one.
`jsxElementType` line 91 waits on the same plus TS2769/TS2741 elaboration.

**How we would know this is wrong.** A false TS2786 "Its type …" in a file
with `JSX.ElementType` would mean either the default instantiation differs
from upstream's or the tag type does; compare `getJsxElementTypeTypeAt` and
`checkExpression(tagName)` first.
