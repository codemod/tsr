# Shadowed type-parameter names and qualified reference identity

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 7332f284: 456,480/478,855 matching assertions. Tracked in tsr-6.38.

## How native names a type parameter in a `.types` line

The baseline writer calls `TypeToTypeNode` with the assertion node's parent as
the enclosing declaration and `GenerateNamesForShadowedTypeParams`
(`type_symbol_baseline.go:394`). Every type parameter then goes through
`typeParameterToName` (`nodebuilderimpl.go:1404`): a per-print by-id cache, a
by-text set of names already allocated in this print, and the shadow test
`typeParameterShadowsOtherTypeParameterInScope` (`:1396`), which resolves the
name at the enclosing declaration and asks whether it finds a *different* type
parameter. The first name that is neither allocated nor shadowed wins (`T`,
then `T_1`, `T_2`, ...). `enterNewScope` clones the naming state per signature
(`nodebuilderscopes.go:10`), so sibling overloads never see each other's names
while a nested signature sees its parents'.

This port bakes type text at creation and re-renders signatures, unions and
generic-reference arguments at an assertion site (`type_to_string_at`). Two older
mechanisms approximate the allocation: own-parameter renaming by instantiating a
signature with fresh `X_1` mints, and a render scope of the enclosing signature
renders' (name, symbol) pairs. They did not reach a *free* type parameter — one
the signature does not declare — so `function O<T>(t: A, t2: T)` with
`type A = <outer T>` printed `<T>(t: T, t2: T) => void` at `O` where native
prints `<T>(t: T_1, t2: T) => void`.

## What changed

**A declared type parameter now renders through the allocation at the site.**
`type_parameter_name_at` answers the render scope's allocation for the same
symbol (the by-id cache), otherwise skips names held by the render scope (the
by-text set) or shadowed at the assertion's parent. Overloads rendered as one
type literal now push their own scope per signature, mirroring `enterNewScope`.
Only types recorded in `type_parameter_symbols` take this path; the fresh rename
mints carry no symbol and keep their text.

Not modelled: the per-text next-name counter (it only matters when a candidate
was skipped by shadowing without being allocated), and names inside baked
composites that are not re-rendered at the site (tuples, conditional and mapped
type text). A port that cloned type parameters on instantiation as
`instantiateSignatureEx` does would be needed for the
`chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2` family
(`<S extends S_1>(cb: (x: S_1) => S)`): the port keeps the method's own `S` when
a receiver maps the class `T` to that same `S`, so the two occurrences are one
identity here and no print-time rule can tell them apart.

**Generic qualified references are cached by type-argument identity.** The
qualified-reference mint cache was keyed by printed text and target symbol, so
`Promise.Thenable<R>` written under the class's `R` and under an overload's
shadowing `R` became one type carrying the class's `R`. Renaming that overload
by instantiation then had nothing to substitute. Native `createTypeReferenceEx`
(`checker.go:25107`) keys instantiations by `getTypeListKey(typeArguments)`; the
generic half now lives in `qualified_generic_reference_types` keyed by text,
symbol and argument ids. This is a semantic fix as well as a printing one: the
conflated reference also carried the wrong argument into member reads.

## Two prerequisites the measurement exposed

The site rule turned two RIGHT rows wrong in
`contextualSignatureInstantiation2`: `(x) => f(g(x))` under `(r:U) => S` printed
`x : U_1`. The parameter's type was the *variable annotation's* `U`, not the
arrow's — a contextual-typing defect the old print had hidden. Native routes a
concise arrow body and a `return` to the same `getContextualReturnType`
(`checker.go:29358`, `:29665`), which reads the written return annotation first.
The concise-body arm now does too.

That fix in turn moved one RIGHT row in `conditionalTypes1`:
`<T>(value: Foo<T>): Baz<T> => value` printed `value : number | boolean`.
`getNarrowableTypeForReference` substitutes constraints only under a
non-generic contextual type, and the port's `Baz<T>` (`type Baz<T> = Foo<T>`,
`Foo` conditional) evaluated its body to `error` under the alias bindings and
was judged concrete. The same defect already affected the `return` form
(`function convert4<T>(value: Foo<T>): Baz<T> { return value; }`). An alias
reference whose body references another alias now follows that alias with the
bound arguments; upstream's `Baz<T>` is the instantiated `Foo<T>` and inherits
its generic flags (`getGenericObjectFlags`, `checker.go:24880`).

## Measurement

Full `scorepair` against 7332f284, and again after rebasing against a fresh
baseline at 5206b571 (landed as 8ed14e53, 457321/478,855, 6999 complete
cases): **+55 WRONG-to-RIGHT, zero RIGHT losses, zero GAP transitions**; 16 already-WRONG rows change and stay wrong (12 move closer:
bluebird/ipromise rows whose remaining defect is an unresolved
`Promise.Inspection<R>` mint or `Windows.Foundation` qualification). The
46 bluebirdStaticThis rows are the reference-identity fix; the rest are the site
rule. Rejected order: landing the site rule without the two prerequisites lost
two (then one) RIGHT rows.

How this would be shown wrong: a corpus row where the parent-anchored shadow
test renames a parameter native prints plain (the old anchors differ for
computed property names, where the older own-parameter rename anchors at the
signature declaration), or a reference whose argument ids differ while native
shares the instantiation.

Remaining under tsr-6.38: unresolved generic mints (`Promise.Inspection<R>`)
are text-only and cannot be renamed by instantiation (native keys error types by
alias symbol and argument ids); type-parameter cloning for receiver
instantiation; by-text allocation inside baked composites.

## Retained overload sets reopen at a shadowing site

Higher-order instantiation keeps two forms of an overload set: semantic
signatures for later member reads and a baked spelling that may contain a
print-only rename. `alias_named_signature_types` originally made that spelling
an unconditional terminal. That preserved a correct mint-time rename, but also
prevented `typeParameterToName` from allocating again when the retained set was
printed inside a different generic signature. In
`declarationEmitHigherOrderRetainedGenerics`, both overload siblings therefore
printed `R2`/`B` where native prints `R2_1`/`B_1`, and the nested returned
signature printed `R1` where native prints `R1_1`.

The multi-signature renderer now evaluates the site allocation even for these
alias-marked sets. It uses the rebuilt text only if at least one overload
parameter was actually renamed at this site. Otherwise it falls through to the
baked spelling. This guard is necessary: always rebuilding gained the three
target assertions but lost 16 RIGHT assertions, because a neutral site exposed
the unrenamed semantic signatures. With the guard, the full run gains the same
three and loses none. Each overload still pushes and truncates its own render
scope, so sibling allocations do not leak; nested slots see only the current
overload's allocation.

The focused control reads the pinned native fixture and checks all three
distinguishing facts: both siblings allocate `_1`, the nested retained signature
also allocates `_1`, and no `_2` appears from scope leakage. One assertion in the
fixture remains wrong: the enclosing single signature prints `F_1` where native
prints `F`. That is the older single-signature rename path, not the retained
overload reopening, and is left for a separately measured change.

## Print-scoped allocations reopen inferred parameter unions

The bounded continuation in production revision `7942ce47` adds the native
per-print by-id allocation to the older render-scope approximation. Private
Checker state stores completed `(TypeId, name)` pairs from this TypeStore;
absent entries are unallocated, with no provisional or failed-name cache.
The root `type_to_string_at` call clears names on success or refusal. Each
signature truncates names to its inherited depth, matching
`cloneNodeBuilderContext` (`nodebuilderscopes.go:10`): nested slots share names,
but sibling signatures cannot consume one another's names. The existing
signature substitution remains a print-only clone, not a semantic mapper.

An inferred, unnamed union containing declared parameter identities now renders
those identities at the site instead of repeating its baked `T | T` spelling.
Both renderers consume the same constituent display plan from
`format_union_types`; boolean collapse and final null/undefined ordering remain
unchanged. The formatter extraction alone has byte-identical full assertion
and diagnostic dumps against `359a2789`. The rejected first dynamic attempt
bypassed that plan and lost 534 previously RIGHT lines; it was not adopted.

The corrected isolated lane gains eight WRONG-to-RIGHT assertions, with no
RIGHT losses, GAP transitions or population changes in 474,251 aligned records.
Two already-WRONG overload rows respell `target: T` to native `target: T_1`, but
their header and conditional value remain unresolved. All 10,570 diagnostic
records are byte-identical. Coordinator-generated pinned-native strict/es2015
control output is byte-identical to the worker output. Reserved suffixes,
constraints/defaults, sibling signatures, nullable/boolean unions and repeated
warm/reversed serialization have focused tests; semantic TypeIds and payloads
remain unchanged across serialization.

Allocation scans only names in this print and resolves lexical names at the
current site. Union rendering walks existing constituents under the existing
active-composite guard without forcing members or adding semantic reuse.
Work attribution remains under `tsr-1yb.11.1`; this is not a speed claim. The
native per-text next-name counter, baked composite/method-object presentation,
receiver parameter cloning and retained intersection semantics remain within
`tsr-6.38`. Evidence, including the rejected candidate and formatter checkpoint,
is retained in `target/parallel-wave44/shadowed-names-evidence.tar.gz`.
