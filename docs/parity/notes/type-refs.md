# Parity box `type-refs` (tsr-2zk.13) — notes

Judgment calls made by the type-refs box, with the numbers that decided them.
Upstream is `vendor/typescript-go` @ `5b1047d`. Baseline for every number
below: the frozen dumps at `0d996e8` (types 465,643 RIGHT / 1,523 GAP / 9,621
WRONG).

## 1. QUALIFIED port set (tsr-2zk.16.4)

### 1.1 What was ported

`Checker::resolve_entity_name_ex` (`crates/tsr-checker/src/declared.rs`) is
`resolveEntityName` (`checker.go:15772`) with `ignoreErrors = true` and the
alias steps the older `&self` `resolve_entity_name` never had:

- `resolveQualifiedName` (`:15828`) resolves the **left** with meaning
  `Namespace` and `dontResolveAlias = false`, so `import b = a.b`,
  `import * as React` and ES-import lefts are followed before their exports
  are read. Before this, the exports of the unresolved ALIAS symbol (always
  empty) were read, every alias-rooted `X.Y` missed, and the reference fell to
  the ANY-flagged unresolved mint.
- The right name goes through `getExportsOfSymbol` (a module follows `export =`
  with `dontResolveAlias = false`, then `export *`) and `getSymbol`'s meaning
  filter (an alias counts when its chain's flags meet the meaning; unknown =
  all). On a miss over an alias namespace, the exports of
  `resolveAlias(namespace)` are tried (`:15853`).
- The found symbol is walked along its alias chain until it carries the
  meaning (`:15821`).

The `&self` `resolve_entity_name` is kept unchanged: `constraints.rs:628`
(a `&self` fn) and `check.rs:7969` (a lifetime that does not admit `&mut`)
cannot take the `&mut` version. Its other callers would benefit; see the final
report's out-of-scope list.

### 1.2 The written-text mint is kept, except where upstream's type cannot print differently

`qualified_type_reference` still mints the written text for classes,
interfaces, namespace-rooted enums and generics, because routing them through
`getTypeReferenceType` loses the qualifier: the printer cannot yet qualify a
name from its symbol (NB-SYMBOL-CHAIN, ADR-0044 step 2). Measured routing
variants against the baseline, each on top of the alias walk:

| routed through the shared `get_type_reference_type` | R→W | gains |
|---|---:|---:|
| enums (all) | 4 | +4 |
| type aliases (all, argument-less) | 36 | +23 |
| generics (all) | 54 | +8 |
| argument-less classes/interfaces | 174 | +11 |

So three narrower arms were taken instead, each chosen so that no rendered
line can move where upstream's answer would print differently:

1. **Non-generic type alias whose declared type does not carry the alias.**
   Upstream's declared type is the body as built; only alias-accepting
   constructors attach the alias. A pre-existing body (`number`, `undefined`,
   an interface the body merely names) prints as itself, never `N.T`.
   "Carries" is read from this port's print-at-creation representation: an
   alias-attributed union, a type printing as the alias's own name, or a body
   that is directly an array, tuple or type-argument-bearing reference
   (`isDeferredTypeReferenceNode`, `:23236` — `type S = Container<string>` is
   a deferred reference carrying `S`; omitting this rule cost 16 R→W in
   `recursiveGenericUnionType1/2`). Converts `classFunctionMerging`,
   `conditionalTypeRelaxingConstraintAssignability`, `moduleVisibilityTest4`.
2. **Alias-rooted enum whose declared type prints, at the reference site,
   exactly as the mint.** The relater then sees the real enum instead of an
   OBJECT mint (`<foo.E1>0` assigned to `number` reported TS2322 in
   `commonJSImportNotAsPrimaryExpression`). Namespace-rooted enums keep the
   mint: routing them measured 3 R→W through a union's named-constituent guard.
3. **Alias-rooted generic** references take `get_type_reference_type` (arity
   window, defaults). Namespace-rooted generics keep §42 v2's mint (54 R→W).

The alias-rooted scoping of 2 and 3 is a population gate, not an upstream
rule: it confines the new road to references that were unresolved before this
change, leaving every previously measured line on its measured road. It should
disappear when NB-SYMBOL-CHAIN lands, at which point all four variants above
should be re-measured.

### 1.3 Two declines that keep a gap rather than a wrong diagnostic

`alias_rooted_reference_declines` returns an alias-rooted reference to the
unresolved mint it answered before, in two situations where the symbol it now
reaches is answered by machinery this port lacks:

- **Generic heritage.** `interface S<T> extends D<T> { a?: string }` then
  `const y: S<number> = {}` reports a false TS2322 **in the baseline binary
  too, with or without a namespace** (reproduced with `tsr -p` on a two-line
  file). Resolving `React.HTMLAttributes<HTMLElement>` surfaced it in
  `reactTagNameComponentWithPropsNoOOM2` (diagnostic EMPTY_RIGHT→EMPTY_WRONG).
  The bug belongs to the relater / base-type lanes.
- **Module augmentation.** The binder does not run `mergeModuleAugmentation`
  (`checker.go:1407`; `crates/tsr-binder/src/binder.rs` lists it as unported),
  so an augmented module's export table is incomplete. Only augmentations in
  the reference's own file are detected (both witnesses,
  `moduleAugmentationDoes{Interface,Namespace}MergeOfReexport`, have that
  shape); elsewhere this road matches the ES-import road, which ignores
  augmentations too.

Falsifier for each: delete the arm once its owner lands; the named case must
stay EMPTY_RIGHT.

### 1.4 The §605 cycle gate moved before resolution

`circular4` (`export type T = ns2.nested.T` across two files importing each
other) must answer `error`. The gate sat on the *miss* branch; the alias walk
now resolves the name, so the gate runs before the resolution result is used.
Without the move: 2 R→W.

### 1.5 Checker port convention

- **Native operation:** `resolveEntityName` / `resolveQualifiedName` /
  `getSymbol` / `getExportsOfSymbol` @ `5b1047d`; consumer
  `getTypeFromTypeReference` via `qualified_type_reference`.
- **Identity and owner:** no new cache or side table. The existing
  `qualified_reference_types` key `(text, resolved symbol)` is unchanged;
  `resolved` may now be a symbol reached through an alias, which is upstream's
  identity too.
- **Publication:** none added. `resolve_alias` is recomputed per call (as
  before; upstream memoises `aliasTarget`).
- **Work boundary:** per alias-rooted qualified reference, one alias walk for
  the left, one `resolve_alias` for the decline check, and (for a module
  target only) a scan of the reference file's top-level statements. Neither
  bench project contains a namespace import or a qualified annotation, so the
  perf self-ratio cannot reach this code; measured ratios 0.98–1.05 against an
  identical-binary control of 1.02–1.03 at 21 samples.

### 1.6 Result

Types: 465,762 RIGHT / 1,457 GAP / 9,568 WRONG (+119 lines, 0 R→W; +13 cases
fully RIGHT). Diagnostics: 8 WRONG→RIGHT, 0 losses.

Remaining in the set: `externalModuleReferenceDoubleUnderscore1` (`basics`
root identifier and `typeof basics.TimeUnit` print `any`: the types producer's
root-identifier rule, `types_producer.rs`, not owned);
`contextuallyTypedJsxChildren2` (`NoInfer` evaluation); `trackedSymbolsNoCrash`
(`Extract<ast.Node, …>` over the namespace-rooted alias mint — needs arm 1 of
§1.2 for alias bodies that carry the alias, i.e. NB-SYMBOL-CHAIN);
TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL and IMPORT-EQUALS-ALIAS-TYPEREF-TARGET
(the identifier arm's §157/§491 roads) not started.
