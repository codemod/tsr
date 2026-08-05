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
