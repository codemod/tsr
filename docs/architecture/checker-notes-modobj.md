# A type for the module object: what the 3,539 actually is

Measured at `7602d6b` by `crates/tsr-conformance/examples/module_object.rs`, over
the full `types` corpus, in 54s wall.

`bd tsr-6ph` sizes the module-object half of the cross-file alias seam at
**3,539 seam-only lines** — gap lines every one of whose blocking leaves is a
cross-file alias, and whose worst leaf is a namespace-shaped form (`import * as
ns`, `import a = require(...)`, `export * as ns`). `bd tsr-4jk` says, from a
different measurement, that two of those three forms are **unbuildable**,
because upstream prints the local alias where this port can only print a path.

Nobody had reconciled the two. This page is that reconciliation, and it is the
answer to `docs/conventions.md`'s first ranking question — *"can this port spell
the answer?"* — asked at the baseline's right-hand side rather than at the code.

**Verdict: build nothing yet.** 46.7% of the 3,539 is unspellable outright, a
further 36.9% is unreachable or blocked below, and the **577 lines (16.3%)** that
survive both filters cannot be converted without simultaneously manufacturing
**at least 1,192 wrong lines** — unless a capability this port does not have
lands first. That capability is named in §6.

## 1. The instrument, and why it is comparable

`module_object.rs` reproduces `module_blocked.rs`'s seed classifier, span test
and blocker walk verbatim — an example cannot import another example — and adds
two channels: what upstream's answer text *is*, and what lies behind the
namespace target once the specifier is resolved. Everything routes through
`types_producer::assertions_for_case_with_ids`, so the compiler measured is the
one the gradient scores, with every bundled lib loaded. A probe that builds its
own per-file checker measures a different compiler and launders wrong answers
into gaps; that has happened five times here (`bd tsr-qj4`).

**Control C1 (arithmetic).** The seam-only population, bucketed by the worst leaf
beneath each line:

| worst leaf | lines | share |
|---|---:|---:|
| specifier names no file in the program | 636 | 12.3% |
| the file has no export of that name | 319 | 6.2% |
| **namespace-shaped target** | **3,539** | **68.7%** |
| target found, still `error` | 609 | 11.8% |
| target found, typed, wrong type | 39 | 0.8% |
| would convert — exactly upstream's type | 13 | 0.3% |
| **total seam-only** | **5,155** | |

3,539 to the line, against the figure `module_blocked` publishes and `tsr-6ph`
quotes. The two probes are walking the same population, so everything below is
comparable to `checker-notes-modblock.md`.

## 2. The split

A line is **spellable** when upstream's answer never names a module object, or
when every module object beneath it resolves through an `export = X`
(`resolveExternalModuleSymbol`, `vendor/typescript-go/internal/checker/checker.go:15556`)
to an ordinary named symbol.

| upstream's answer | what lies behind the target | lines | share |
|---|---|---:|---:|
| is the module object — `typeof ns` | `export =`, still `error` | 80 | 2.3% |
| is the module object — `typeof ns` | plain module object | **648** | 18.3% |
| is the module object — `typeof ns` | no target file | 347 | 9.8% |
| rooted at one — `typeof ns.x` | `export =`, still `error` | 10 | 0.3% |
| rooted at one — `typeof ns.x` | plain module object | **83** | 2.3% |
| rooted at one — `typeof ns.x` | no target file | 95 | 2.7% |
| an import type — `import("m").W` | `export =`, still `error` | 5 | 0.1% |
| an import type — `import("m").W` | plain module object | **461** | 13.0% |
| an import type — `import("m").W` | no target file | 20 | 0.6% |
| ordinary — `number`, `typeof C`, `() => void` | `export =`, still `error` | 211 | 6.0% |
| ordinary | plain module object | **577** | 16.3% |
| ordinary | no target file | 1,002 | 28.3% |

**Spellable 1,885 (53.3%), unspellable 1,654 (46.7%).**

That headline is still too generous, and the sub-rows say why:

- **1,002 of the spellable half (28.3% of the whole) have no target file.** The
  specifier names an ambient module, a package, or a path this probe's resolver
  does not model. The module-object seam does not reach them; they are the same
  work item as `module_blocked`'s own 636-line `NoSuchFile` bucket.
- **306 lines (8.6%) sit behind an `export = X` — and every single one of them
  still answers `error` today.** Zero converted. See §4.
- What is left is **577 lines, 16.3% of the 3,539**: an ordinary answer, a
  resolvable target, a plain module object. That is the whole actionable
  population, and it is a **ceiling** — each of those lines additionally needs
  the member lookup on the module object to produce upstream's exact type, which
  this probe does not mock.

## 3. Why the `import("m").W` rows are not ordinary

Folding them in would have read 66.9% spellable instead of 53.3%. They are
upstream's node builder printing a symbol that has **no accessible name at the
reference site** — the `privacy*` family, which exists to test exactly that. The
top spellable-looking cases before the correction were
`compiler/privacyImportParseErrors` (238), `privacyFunctionCannotNameParameterTypeDeclFile`
(236), `privacyCannotNameVarTypeDeclFile` (140),
`privacyFunctionCannotNameReturnTypeDeclFile` (108) — 722 lines of one family,
all answering `import("./privacy…_Widgets").Widget1` and variants.

This is a *second* rendering capability, not the module object. A module-object
type does not supply it. Classifying them as ordinary would have sized the item
by counting lines whose answer this port can no more print than `typeof ns`.

## 4. The `export =` sub-case, measured

`tsr-4jk` recorded this as the sub-case that does not wait, and as unmeasured:
`export = X` makes `resolveExternalModuleSymbol` return `X`, an ordinary named
symbol whose name is spellable. It is now measured, by asking this port for
`get_type_of_symbol` on the target module's `export=` symbol
(`INTERNAL_EXPORT_EQUALS`, `crates/tsr-binder/src/binder.rs:3527`) and comparing
`type_to_string` against upstream's text:

| namespace seeds | `export =` converts | `export =` typed but wrong | `export =` still `error` |
|---|---:|---:|---:|
| `import a = require(...)` | 0 | 0 | 146 |
| `import * as ns from` | 0 | 0 | 31 |
| `export * as ns from` | 0 | 0 | 0 |

**177 seeds, 306 lines, and not one of them types today.** The `export =` route
is spellable and is *not* available work: the port answers `error` for the
`export = X` alias symbol itself, so the module-object seam is necessary and not
sufficient for every line behind it. `tsr-4jk` guessed `typeof ClassB` and
`typeof Foo` in the `require` histogram were this shape; they may be, and the
port cannot type them either way.

## 5. Two controls, and what they caught

**C2, pinned by construction.** A module symbol's name in this port is the file
path with its extension stripped. So a namespace seed whose module symbol name
equals the local alias upstream prints is impossible — the expected value is
**0**, fixed by the binder before any code in the probe runs, and a non-zero
reading would mean the unspellability claim is wrong at its root. Measured: **0**.
The probe also prints one example per case, and they are unambiguous:

```
compiler/aliasAssignments   upstream `typeof moduleA`
                            this port would name the module `/aliasAssignments_moduleA`
compiler/aliasUsageInArray  upstream `typeof Backbone`
                            this port would name the module `/aliasUsageInArray_backbone`
```

Preferred over an arithmetic control for the reason `docs/conventions.md` gives:
every sum in this probe would still reconcile under a polarity inversion, and
this bucket would not.

**C3, pre-registered.** `tsr-4jk` says upstream's answer on the *declaration
name* of a namespace-shaped import is **always** the local alias. Predicted
before running: the exception count is ~0. Measured: **222 declaration-name
seeds on plain module objects, 17 exceptions (7.7%)**. So "always" is 92.3%, and
the record is corrected here.

The exceptions do not weaken the finding — they strengthen it. All 17 are
printed by the probe; 15 of them are *other unspellable forms*, not spellable
ones:

```
conformance/exportNamespace2       alias `a`                 -> typeof import("./a")
compiler/exportStarNotElided       alias `aliased`           -> typeof import("./data1")
compiler/es6ImportNameSpaceImport  alias `nameSpaceBinding2` -> typeof nameSpaceBinding
compiler/modulePreserve4           alias `g2`                -> typeof g1
conformance/esmModuleExports1      alias `Foo3`              -> typeof Foo
compiler/importInsideModule        alias `foo`               -> any
```

Only the two `any` rows are spellable. The rest print either an import type or
**a different alias's name than the one being declared** — and that last group is
the decisive evidence in this document, see below.

## 6. The falsifier that failed: this port has no route to the name

The build order hinges on whether this port can reach
`getNameOfSymbolAsWritten` / node-builder behaviour, which emits the shortest
accessible chain to a symbol *at the reference site*. Two independent facts say
it cannot, and the second closes the question against any cheap workaround:

1. **The printer takes no context.** `Checker::type_to_string`
   (`crates/tsr-checker/src/checker.rs:305`) has signature
   `fn type_to_string(&self, id: TypeId) -> String`. There is no enclosing node,
   so the rendered form is fixed at type creation — `TypeData::Named` carries the
   string. `docs/conventions.md`'s worked example predicted this; it is now
   checked rather than assumed.
2. **The right name is not a property of the declaration either.**
   `compiler/es6ImportNameSpaceImport` declares `nameSpaceBinding2` and upstream
   prints `typeof nameSpaceBinding`; `compiler/modulePreserve4` declares `g2` and
   prints `typeof g1`; `conformance/esmModuleExports1` declares `Foo3` and prints
   `typeof Foo`. **So even "name the module type after the alias that declared
   it" produces a wrong line.** The name is chosen by an accessibility search
   over everything in scope, and nothing short of that search reproduces it.

That kills the obvious workaround before it is built. It is the same mechanism
`Checker::resolve_alias` already gaps `import a = foo.bar.baz` for
(`compiler/aliasBug.types`, `>booz : typeof booz`) — one limitation, now found in
its fourth place.

## 7. Why the 577 are not available work *either*, yet

This is the part that makes the item a stop rather than a smaller build.

To convert the 577, `get_type_of_symbol` must return a real type for the module
symbol, so that `ns.foo` has a receiver and the member lookup runs. The moment it
does, the lines that assert the module object itself stop being gaps and start
printing:

| line group | today | after a naive module-object type |
|---|---|---|
| 577 ordinary answers behind a plain module object | gap | converted (ceiling) |
| 648 `typeof ns` behind a plain module object | gap | **wrong** — `typeof /0` |
| 83 `typeof ns.x` behind a plain module object | gap | **wrong** |
| 461 `import("m").W` behind a plain module object | gap | **wrong** |

**577 converted against 1,192 manufactured wrong.** That is the trade ADR-0038
and ADR-0039 exist to refuse, and it is 2.1 wrong lines per converted line.

The escape is not impossible, and it is worth stating precisely so that the next
reader does not have to re-derive it: the positions that print the module object
are **syntactically identifiable** — the identifier that *is* the alias, versus
the member access hanging off it. An arm that hands the checker a module-object
type for lookup while returning `errorType` for the alias's own reference
position would take the 577 without the 1,192. That is "port the position, not
the arm" (`docs/conventions.md`), and it is the same ordering constraint the
accessor work hit, where case 4 had to gap *before* the implicit-`any` arm.

Whether that separation is available depends on whether the rendering path and
the checker's internal `type_of_expression` can disagree for one node. Nobody has
established that, and it is the single question that decides this item. It is
filed as `bd tsr-6j2`.

## 8. What the numbers do not say

- **1,002 lines have no resolvable target file.** They are counted spellable
  because their answer text is ordinary, and nothing else about them is claimed.
  Do not read that bucket as available work.
- **577 is a ceiling, not a conversion.** Unlike `module_blocked`'s member forms,
  this probe does not mock the answer — there is no module-object type to ask.
  Each of those lines additionally needs the member lookup to produce upstream's
  exact text.
- **A row counts the lines that name a defect, not the lines downstream.** The
  measured cascade on the sibling arm (`fa29e66^..fa29e66`) was 1.66×, and it is
  a property of the form: an imported name is imported in order to be used, so a
  module object's cascade is plausibly higher than a re-export's. That cuts both
  ways here — it multiplies the 577 *and* the 1,192.
- **This is a corpus measurement, not the gradient.** No gradient number is
  quoted anywhere on this page.

## 9. Consequence for the board

`tsr-6ph`'s 3,539 should not be quoted as a population of available work. The
figure to carry forward is:

```
3,539  seam-only lines waiting on a module object
1,654  unspellable — building them makes wrong lines        (46.7%)
1,002  spellable answer, no resolvable target file          (28.3%)
  306  behind an `export = X` that this port cannot type    ( 8.6%)
  577  actionable CEILING, and only behind bd tsr-6j2       (16.3%)
```

`tsr-4jk`'s 692 is unchanged and now has a mechanism check behind it as well as a
histogram: no per-declaration naming scheme reproduces upstream's answer, because
upstream sometimes prints a *different* alias's name.

Nothing was built in `crates/tsr-checker/src/symbols.rs` or `resolution.rs` for
this item. That is the finding.

## 10. Ambient modules — the slice the seventh session found, sized, and its bar

*(A previous §10 described the lookup-only build `3baeb70`, reverted at
`a618e3a` for 2.5 wrong per right; it was removed with the code. This section
is a different slice with a different counterfactual, and it exists because the
naming half — the thing that killed both earlier designs — has since shipped at
`c91314c` for the forms it could reach.)*

### 10.1 What changed since the refusal

`c91314c` (sixth session) named a module object at the reference site — the
alias-arm of `getAccessibleSymbolChain` — for `import * as ns`,
`import a = require(...)` and `export * as ns`, refusing when ≥2 distinct
aliases are in scope. That build's residue is what this section sizes: the
seam population is now **1,691** (was 3,539 at the refusal), the unspellable
half **439** (was 1,654).

### 10.2 The counterfactual, one pass, three columns

`examples/module_object.rs` gained the alias-search forecast: for every
unspellable line, does the *name upstream printed* resolve at the *line's own
site* to a namespace-shaped alias of the *line's own blocking module* — with
the module's identity split three ways: a program file, an **ambient**
`declare module "x"`, or nothing at all. Measured at `d098d66`:

```
  263  REACHABLE name — but module UNRESOLVABLE          stays gap either way
   69  REACHABLE, unique in-scope name [AMBIENT]         the build's conversions
   52  REACHABLE, ambiguous [file module]                stays gap (c91314c's refusal)
    7  REACHABLE, ambiguous [AMBIENT] (1) + unresolvable (6)
    5  REACHABLE, unique [file module]                   other causes, not this item
   19  MISS (5 not namespace-shaped + 14 name absent)
   16  import(...) wanted, alias IN SCOPE                the would-wrong ceiling's larger half
    8  import(...) wanted, no alias in scope             refusal holds
```

And the member half, mocked exactly as §1's instrument mocks everything:
adding ambient resolution to the probe's own replay moved the seam leaves
`WouldConvert` **14 → 42 (+28)** and `WouldBeWrong` **39 → 64 (+25)**;
247 `specifier names no file` lines re-resolved, of which 98 land on
`target found, still error` — downstream gaps, converted by nobody today.

**Two probe defects were found and fixed before any number above was read**,
both the same lesson: this binder stores an ambient module's name *unquoted*
(`module_name`, `crates/tsr-binder/src/binder.rs:4091`) where upstream keys the
quoted string. A quoted-name test can never fire here, and it read a false
zero twice — once in this probe's first ambient tag, once in `qualnamep.rs`'s
`bd tsr-xpb8` split (where the corrected test *still* reads 0, so that
conclusion survives its own defect).

### 10.3 The item: `tryFindAmbientModule`, and nothing else

`resolveExternalModule` (`checker.go:15149`) consults
`tryFindAmbientModule` (`checker.go:15533`) **before** file resolution:
a non-relative specifier is first looked up among the globals under its quoted
name with `SymbolFlagsValueModule` meaning, merged. This port's
`resolve_external_module_name` (`crates/tsr-checker/src/symbols.rs:939`) goes
straight to the `ModuleHost` and so answers `None` for every
`declare module "x"` in the corpus.

The build is that fallback, checker-side, plus one gate-widening: the naming
interception in `type_to_string_at` tests `is_module_symbol` (a `SourceFile`
declaration), which an ambient module fails, so the same interception must
also accept `is_ambient_module` — otherwise a resolved ambient module prints
its baked name, which is the exact failure mode that killed `3baeb70`.

**One deliberate divergence, stated with its reason**: upstream distinguishes
ambient modules from ordinary globals by the quotes in the symbol name. This
binder's naming is unquoted and is load-bearing for its other consumers, so
the checker distinguishes by *declaration shape* (`is_ambient_module`,
`checker.rs:708`) instead. Same selection, different key.

**What this slice does NOT do**: the `/.lib/<file>` reference-path mapping
(upstream `harnessutil.go:39`), which is why 263 reachable lines stay
unresolvable — `react.d.ts` is simply never loaded by this harness. That is a
harness-fidelity item, measurable separately, and follows this one.

### 10.4 The bar, registered before any checker code

Forecast components, each measured above: **69** naming conversions (unique
in-scope ambient alias) + **28** member conversions (mocked exact-text) = 97.
The naming 69 is a *root-name* match, not a whole-line match — `typeof ns.x`
lines match on `ns` and can still miss on `.x` — so the floor discounts it.

- **Leg 1 — net ≥ +60.** Rule: ~60% of the 97-line two-component forecast,
  the discount owned entirely by the root-vs-whole-line gap in the naming
  column.
- **Leg 2 — the mechanism's own new wrong ≤ 82**, with global `Δwrong`
  reported beside it, per the post-W rule. Rule: 2× the counterfactual's own
  would-wrong ceiling of 41 (25 mocked member `WouldBeWrong` + ≤16
  `import(...)`-wanted lines with an alias in scope).
- **Leg 3 — cases regressed == 0.**
- **Leg 4 — lost (RIGHT→WRONG, by `verdictdump`) ≤ 5.** Not 0, because
  ambient-first ordering is upstream's and this port resolves files first
  today: a case with both a file and an ambient module under one specifier
  can flip. ≤ 5 because the probe saw no such case; a larger reading is
  diagnosis, not trade.

Falsifiers, in the order to open the drawers:

1. **New wrong far above 82, concentrated in member reads** → the member arm
   is wider than `mock_target`'s three name forms (import specifier, default,
   export specifier); re-take the mock with the missing form before touching
   the build.
2. **Any lost line** → grep the losing case for a file and a
   `declare module` sharing one specifier; the ambient-first ordering is the
   suspect, and it is upstream's ordering, so the fix direction is not
   obvious — stop and measure.
3. **Net far below 60** → check that the namespace-shaped `resolve_alias`
   arms actually route through `resolve_external_module_name` rather than a
   second copy of resolution the build did not touch.

### 10.5 Scored against the bar — built at the commit carrying this section

Measured over the full corpus, `verdictdump.rs` at both ends of a
path-limited stash, before = `b3aeda1` exactly (`347,530 / 81,828 / 39,557`,
the handoff triple):

```
right  347,530 -> 347,658   +128
gap     81,828 ->  81,355   −473
wrong   39,557 ->  39,902   +345     GAP→WRONG 345, RIGHT→WRONG 0
cases    2,750 ->   2,750   regressed 0, finished 0
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +60 | **+128** | pass |
| 2 | mechanism-own new wrong ≤ 82, global Δwrong beside it | **~15 own · +345 global** | pass — see below |
| 3 | cases regressed == 0 | **0** | pass |
| 4 | lost (RIGHT→WRONG) ≤ 5 | **0** | pass |

**Leg 2's split is arithmetic over the two dumps, not a judgement call.** Of
the 345 GAP→WRONG lines:

- **216** are byte-identical to the baseline once `import("…").` qualifiers
  are stripped from the want — the type is computed correctly and only the
  **import-type naming form** is missing. Owner: `bd tsr-xpb8`'s specifier
  half — and for an *ambient* container the specifier is exact and free
  (`nodebuilderimpl.go:1260`), which makes this bucket the next slice's
  measured population rather than an accepted loss.
- **112** are identical once a dotted qualifier is stripped
  (`im_private_mi_public.c_public` wanted, `c_public` printed) — the
  **container-alias chain**: `symbol_chain` declines at a module container
  while the alias half it needs (`module_name_at`) already exists.
- **17** other: ~15 are the mechanism's own — `typesVersions` selection
  inside ambient module names (6, `typesVersions.ambientModules`), one
  shorthand-merge line (`ambientShorthand_merging` wants `any`), the
  `typesVersionsDeclarationEmit` signature pair — plus tsx tag-shape lines
  owned elsewhere.

The registered falsifier 1 named the right drawer with the wrong label: the
counterfactual's would-wrong ceiling (41) was measured over the seam-only
population, and the 345 arrived through **type-position references** the
probe never walked. The ceiling was a ceiling on the population it was
measured on — §4.1's rule, met again from the other side.

**What was NOT done, deliberately**: no positional refusal was added to stop
the 328 named-owner lines printing bare names. The port's shipped design
already prints a bare name when the chain stops at a *file* module container
(the `qualnamep.rs` FILE-half's 932-line family predates this build);
refusing only for ambient containers would fork one mechanism's behaviour by
container kind, and refusing for both requires knowing an accessible alias
exists for the *symbol itself* — which is precisely the next mechanism, not a
guard to improvise here. The 328 sit in the wrong bucket with their owners
named, and §10.6 prices converting them.
