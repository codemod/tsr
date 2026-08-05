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

## 10. The build: lookup only, and the prediction registered before the run

Built at `HEAD` of this section's commit pair. `bd tsr-6ph` item 4 and `bd
tsr-6j2`'s resolution are the inputs; nothing here re-derives them.

### The design taken, and the one rejected

Two ways to get the slice, and they are not equivalent:

- **(a) Producer guard.** Let `get_type_of_symbol` answer a module type, and have
  `types_producer::type_at_location` return `error` at the alias's own reference
  position. This is available — `tsr-6j2` established that the rendering path and
  the checker's internal type already disagree for a single node in at least
  seven places, one of them labelled a *baseline-writer* property in the file
  itself, quoting upstream's own workaround comment at
  `type_symbol_baseline.go:371`.
- **(b) Checker-side, lookup only.** Never build a printable type for the module
  symbol at all. Do the member lookup inside the property-access path, so
  `get_type_of_symbol` on the alias keeps answering `errorType`.

**(b) was taken.** Not because (a) is unfaithful — it is the more faithful of the
two, since ADR-0039 places positional rules exactly there — but because the
property this slice has to hold is *"the module object is never printed"*, and
under (b) that holds **by construction**: there is no type to print, so no future
consumer can print one. Under (a) it holds by a guard someone must remember, and
the thing being guarded is worth 1,192 wrong lines. `docs/conventions.md`,
*"prefer a control pinned by construction over one pinned by arithmetic"*, is the
same argument one level up.

**What would make (a) win, stated so this is revisitable:** a consumer that needs
the module object as a *value* rather than as a member source — `ns` passed as an
argument, spread, assigned, or reached through an aliased chain that this arm
cannot walk. (b) can serve only a member read. None of those exists in this port
today; the first one that lands is the falsifier for this choice, and the
migration is mechanical because `module_symbol_of_namespace_alias` is already the
seam.

(b) also touches no file outside `crates/tsr-checker/src/{symbols,members}.rs`,
so **the alias rows cannot move by accident** — any movement in them is a bug,
not a judgement call.

### What was built

| where | what |
|---|---|
| `symbols.rs` | `module_symbol_of_namespace_alias` — `getTargetOfNamespaceImport` (`checker.go:14628`) and `getTargetOfImportEqualsDeclaration`'s external-module-reference case (`checker.go:14439`), reduced to the module symbol, gated on `resolveExternalModuleSymbol` (`checker.go:15556`) leaving it unchanged |
| `symbols.rs` | `get_export_of_module` widened from private to `pub(crate)`; no behaviour change |
| `members.rs` | `module_member_type` — resolve the receiver identifier, require `ALIAS`, take the module symbol, read the export, `get_type_of_symbol` on **the member** |
| `members.rs` | one call at the top of `check_property_access_expression`, **before** `check_expression(receiver)`, because the receiver has no type to take |
| `tests/module_objects.rs` | 11 tests, six mutations measured |

`resolve_alias` is untouched and still answers `None` for both forms. That is
deliberate: it feeds `get_type_of_alias`, and a resolved answer there would print
the file path. This is a second, narrower entry point with a different consumer.

Every upstream anchor above was taken from `grep -n` on the declaration at
`5b1047d10`, not from a window or a briefing —
`getModuleSpecifierForImportOrExport` is at **`checker.go:15030`**, and the
number quoted in the first draft of the code comment (`:14608`) was wrong and
would have resolved.

### The mutations, and the two that did not bite

| # | mutation | reddens |
|---|---|---|
| 1 | `Node::NamespaceImport(_) => return None` | 4 tests |
| 2 | `Node::ImportEqualsDeclaration(_) => return None` | 2 tests |
| 3 | drop the `export =` gate | **nothing** |
| 4 | `get_export_of_module(…)?` → `.unwrap_or(module_symbol)` | 1 test |
| 5 | remove the `ALIAS` test | **nothing** |
| 6 | ignore the circularity frame's answer | **stack overflow, SIGABRT** |

1, 2 and 4 have pairwise disjoint red sets, so each is evidence about its own arm
rather than about the arm existing.

3 and 5 are recorded because a guard no mutation can make observable is
decoration. **3 cannot be discriminated by any fixture, and the reason is
structural**: TypeScript rejects *"An export assignment cannot be used in a module
with other exported elements"*, so a module carrying `export=` has nothing else in
its `exports` table and gated and ungated both miss. Both gates are kept, each
with the named edit that would make it bite, following the precedent of
`get_property_of_anonymous_symbol`'s flags gate.

6 is the requirement to make a cycle fire, and it did — but **not through the
guard the brief named**. `get_symbol_flags`'s `seenSymbols` (`checker.go:16368`)
closes the *alias-chain* cycle `tests/cross_file_aliases.rs` pins. The cycle this
arm makes reachable runs through variable **initialisers** — `a.ts`'s
`export var x = B.y` against `b.ts`'s `export var y = A.x` — and is closed by
`get_type_of_variable_or_parameter_or_property_worker`'s `resolutions` frame,
which predates this work. No new guard was ported. Two guards, two cycles, and
reading alone would have named the wrong one.

The cycle answers **`any`**, not `error`, and that is upstream's:
`reportCircularityError` (`checker.go:18822`) returns `errorType` only when the
declaration carries a type annotation, and `anyType` for a circular initialiser
without one. *"`errorType`, never `anyType`"* is a rule about answers this arm
invents, not about a computed upstream answer reached through it.

### The prediction, registered before any corpus run

**Population.** 1,618 assertion lines, from `receiver_gap.rs`'s AST-plus-
declarations walk; the independent blocker-walk read 1,590, 1.8% apart. That is
the ceiling, and note it already contains the ×2 for `a.b` rendering two lines.

**Rate.** I predict **1,050–1,350 converted**, i.e. 65–83% of the ceiling, and I
will call anything inside that a hit. The subtraction from 1,618 is three named
groups, and each is a `None` in the code:

1. **`import d from "./m"`** — not built. `getTargetOfImportClause` runs
   `resolveESModuleSymbol`'s synthetic-default arm, whose answer is a *cloned*
   module type, not the module symbol. The `import d from` alias row is 232 lines;
   the member accesses hanging off default imports are a subset of the 1,618 that
   this arm cannot reach at all.
2. **`export * from`** — `get_export_of_module` reads the binder's `exports`
   table directly, so a name arriving only through a star re-export misses.
3. **The member's own type gaps.** A converted access still answers `errorType`
   when the export is, say, an un-annotated function or an array. This is the
   dominant term and it is unmeasured; 65% is where I put it.

**Cascade.** I predict the gradient gains **more** than the rows lose, but by less
than the 1.66× the cross-file alias arm measured — call it **1.1–1.3×**. The
reason it should be smaller: `fa29e66`'s cascade came from *calls* to imported
names, and a member access is already counted twice inside the 1,618, so the
usual downstream line is inside the row rather than outside it.

**What must NOT move — the sharpest condition, and it has no test behind it at
corpus scale.** These are the revert conditions:

| row | must read |
|---|---:|
| `import * as ns` declaration name | **259** |
| `import a = require` declaration name | **433** |
| `import d from` declaration name | **232** |
| `typeof ns` answers | **648** |
| `typeof ns.x` answers | **83** |
| `import("m").W` answers | **461** |

If any of these moves *at all*, in either direction, the slice is wrong and comes
out — a move up means the module object became printable, a move down means
something else in the alias path changed. They are pinned by construction here:
no module type is created, and `resolve_alias` is byte-identical. `tests/
module_objects.rs`'s two receiver tests assert the same property on a fixture.

**How this could be right for the wrong reason.** The number going up is not a
confirmation on its own. The converted lines must be **property accesses whose
receiver is a namespace-shaped alias**. If the gain shows up as a broad
improvement across property accesses generally, or concentrated in one file, then
something else moved — most likely `get_export_of_module` becoming `pub(crate)`
having some effect I have not foreseen, which it should not, since no new caller
was added outside `module_member_type`. I want that scored as a **miss** even if
the total moves the right way. The second way to be right wrongly: if the
conversion lands near 1,618 exactly, that would mean groups 1–3 above are all
empty, which I do not believe, and I would want the split re-measured rather than
the prediction credited.

**Scoreboard note.** The last comparable prediction on this workstream was a good
prediction of the wrong quantity — 389 predicted against 428 the rows lost and 711
the gradient gained. So the number above is stated as *rows converted*, and the
cascade is quoted separately rather than folded in.
