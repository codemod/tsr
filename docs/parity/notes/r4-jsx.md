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

## 2. TS2604: `resolveJsxOpeningLikeElement`'s no-signature arm

**Forcing constraint.** `jsx.go:566-581`: for a value tag whose apparent
type is not `errorType`, `getUninstantiatedJsxSignaturesOfType` (`jsx.go:898`)
— construct signatures of the apparent type, else call signatures, else for a
union `getUnionSignatures` over each constituent's list — is empty and the tag
is not an untyped call (`isUntypedFunctionCall`, `checker.go:9933`, with
construct count 0): TS2604 `JSX element type '{0}' does not have any construct
or call signatures.` on the tag name. Four cases missed exactly that
(`jsxViaImport`, `tsxDynamicTagName7` — a `this` tag —, `tsxElementResolution8`,
`tsxUnionTypeComponent2`).

**What is ported.** `check_jsx_signatureless_tag` and
`jsx_uninstantiated_signatures_are_empty` (`jsx_component.rs`), entered from
`check_jsx_component_bound` before the `ElementType` branch, as upstream
resolves the signature before choosing a bound. The `string` and
string-literal arms of `getUninstantiatedJsxSignaturesOfType` are not this
function's (`anySignature`; `check_jsx_string_literal_tag`). For a union the
port answers "empty" only when some constituent's list is empty, which is
`getUnionSignatures`' first `return nil`; when every constituent has
signatures, whether they combine is the matching half of `getUnionSignatures`
and the port declines. A string or string-literal constituent declines (its
arm can report TS2339 itself). Any unresolved list (`None`) declines.
`isUntypedFunctionCall`'s signature-less arm is re-stated locally rather than
called: `Checker::is_untyped_signatureless_call` is private to `calls.rs`
(calls lane); making it `pub(crate)` and calling it is the integrator's
one-line follow-up.

**Why round 1's version was reverted, and what changed.** Round 1
(`1da6e97`, reverted `6eb0a11`; `jsx.md` §8) reported a false TS2604 on
`reactSFCAndFunctionResolvable`. Root cause, reproduced in a five-line file:
a *qualified* reference to a type alias with no written arguments (`R.SFD`,
`React.SFC`) is minted in `declared.rs`'s QUALIFIED-TYPEREF arm as
`new_named(OBJECT, "R.SFD", Some(alias_symbol))` — a named type whose member
table is the **alias** symbol. `signature_candidates_of_interface_symbol`
reads call/construct members only from interface and type-literal
declarations, so it answers a *complete, empty* list for it. The same file
reports a false TS2349 on `E()` for `declare const E: R.SFD` — the gap is not
JSX's. The unqualified alias (`type S = R.StatelessComponent<{}>`) resolves.

**Accepted decline.** `jsx_uninstantiated_signatures_are_empty` answers
`None` for a named type whose member symbol is a `TYPE_ALIAS`. Upstream has no
such type (an alias's declared type is its body), so its empty list is not
evidence; this mirrors no upstream rule and exists only until the producer
is fixed. Two faithful fixes, neither in this lane's files:

1. `signatures.rs` `signature_candidates_of_named_type`: answer `None` for an
   alias-symbol member table (delivered as `r4-jsx-alias-mint-signatures.diff`,
   measured in the round-4 report), after which this decline is dead code;
2. `declared.rs`: route the argument-less qualified alias reference through
   `getTypeReferenceType` (blocked there on NB-SYMBOL-CHAIN printing).

**Measured.** WRONG → RIGHT: the four cases above. No other case's output
changed in either dump.

**How we would know this is wrong.** A false TS2604 means an empty list this
port treats as complete is not upstream's; compare `signatures_of_type_kind`
for the tag's apparent type against `getSignaturesOfType` before touching
this rule.

**The producer patch, measured** (`r4-jsx-alias-mint-signatures.diff`,
applied on top of this commit, full dumps against the frozen baseline): both
loss checks empty; diagnostics verdicts unchanged; five false diagnostics
removed from WRONG cases — TS2349 ×2 in `jsdocCallbackAndType`, TS2349 ×2 in
`typeTagNoErasure`, TS2345 ×1 in `coAndContraVariantInferences6`. It is
`signatures.rs` (not owned), so it is routed through the report.
