# The `TypeReference` row, decomposed — §4.3's registered probe answered

Sixth session, measured at **`df13a69`** by the new
`crates/tsr-conformance/examples/typerefgap.rs` and **re-run unchanged at
`a4e3991`**, after the three-valued relater landed. The two runs differ in
exactly one number — the gap total the walk starts from, 86,642 → 86,545 — and in
nothing else: every bucket, every control, every head case is identical to the
line. The row is not sensitive to that build, which is what one would expect of a
population that turns out to be a single naming refusal.

`STATUS.md` §4.3 listed the row as unscorable:

> **3,855** — `TypeReference`, no further dependency — but **top-1 is 51.1%**
> (`resolvingClassDeclarationWhenInBaseTypeResolution`), so ~1,988 is one case
> and the real row is **~1,867 of unknown cause**. *Probe: split by case, then by
> why the reference resolves to nothing.*

Run. **The cause is not unknown and it is not a mixture.** The row is one
mechanism at **99.8%**, the top-1 case is not a special case, and removing it
changes the diagnosis by nothing at all.

```
lines want-any  share   arg-gaps  mechanism
 4002       37   0.9%       1063  qualified name, leftmost RESOLVES as a namespace
                                    — refused (resolveEntityName unported)
    8        0   0.0%          8  DOWNSTREAM — name unresolved, a type ARGUMENT gaps
```

| | lines | want-any | reachable |
|---|---:|---:|---:|
| the row | **4,010** | 37 (0.9%) | **3,973** |
| the top-1 case | 1,986 (49.5%) | 0 | 1,986 |
| **the row without it** | **2,024** | 37 | **1,987** |

`STATUS.md`'s 3,855 was measured at `b00738d`; the row now reads **4,010**. The
top-1 share is 49.5% against the published 51.1% — the row grew and the case did
not.

## 1. What defines the row, and why that had to be copied

The row is a `depend.rs` bucket. Its population is whatever `depend.rs`'s walk
puts in it, so `gaps`, `step` and `has_step_arm` are **copied verbatim** into the
new instrument rather than re-derived. A walk that drifts by one edge measures a
different population and reports it under the published number, which is
`bd tsr-qj4`'s failure mode arriving through a different door. The probe routes
through `types_producer::assertions_for_case_with_ids`, as that issue requires.

`no further dependency` at a `TypeReferenceNode` means `depend.rs`'s step
returned `None`. That happens when the name has no `node_id`, when
`resolve_name(.., TYPE)` fails, when the resolved symbol has no declarations, or
when its first declaration has no name — so the ending is **a property of the
walk as much as of the compiler**. The instrument therefore reports every
`TypeReferenceNode` root, not only the row's:

| ending | lines | share of the kind |
|---|---:|---:|
| `no further dependency` — **the §4.3 row** | 4,010 | 65.8% |
| `the dependency types — the root is here` | 1,939 | 31.8% |
| `cycle` | 142 | 2.3% |
| **all `TypeReferenceNode` roots** | **6,091** | |

That table is the guard against reading the row as the whole kind. It is not:
the other 2,081 lines hold the mechanisms the row does *not* contain, and they
are listed in §5.

> **The row is GONE at `d9a730b`.** A fresh `depend.rs` after the two
> qualified-naming builds shows no `TypeReference / no further dependency` row at
> all — it was 4,010 lines and ranked 5th at the start of that session. The
> sibling ending fell 1,939 → 1,563. This is the check §7 asks for, run in the
> direction that confirms rather than refutes: **a different instrument, one this
> page did not use to score itself, agreeing that the mechanism and the row were
> the same thing.** Removing the mechanism removed the row.

## 2. The mechanism split is a branch table, not a guess

Every bucket is a branch of `Checker::get_type_from_type_reference`
(`crates/tsr-checker/src/declared.rs`), the port of `getTypeReferenceType`
(`internal/checker/checker.go:23146`), taken in the checker's own order so that
a line is attributed to the branch that actually decided its answer:

- a **qualified** name whose leftmost identifier resolves as a namespace answers
  `errorType` **deliberately**. Upstream resolves it through `resolveQualifiedName`
  (`internal/checker/checker.go:15828`), which resolves the left as a namespace
  and returns nil when that fails;
- a name that does **not** resolve is not a gap by itself. Upstream mints a
  synthetic unresolved symbol and renders the written text
  (`getUnresolvedSymbolForEntityName`, `internal/checker/checker.go:23102`),
  ported at `d356450` for +4,645 lines;
- arguments on a non-generic type answer `errorType` per `checkNoTypeArguments`
  (`internal/checker/checker.go:23157`);
- an argument count unequal to the parameter count needs
  `fillMissingTypeArguments` and the arity check at
  `internal/checker/checker.go:23189`.

### The downstream bucket is 8 lines, and that is the surprise

`docs/conventions.md` requires the *"every part is handled, something downstream
gapped"* bucket to be counted **first**, because it was 77% of the object-literal
row. Here it is **0.2%** — eight lines in one case
(`compiler/ramdaToolsNoInfinite2`), all of them an unresolvable name whose type
argument gaps.

The reason is structural rather than lucky: `depend.rs`'s step *already walks out
of* the downstream shapes. A reference whose declaration exists and gaps
continues the chain and roots somewhere else; a reference whose declaration types
ends at `the dependency types — the root is here`. What survives into
`no further dependency` is precisely the set the walk cannot step out of, and
that set turns out to be almost exactly one refusal.

### The `arg-gaps` column does not split ownership, and that was measured

1,063 of the 4,002 qualified lines also have a written type argument that gaps —
`minutus.inez<minutus.inez<sagitta.stolzmanni, dammermani.melanops>, …>`. A
column like that invites the assumption that some other item owns part of the
row. It does not, and the instrument classifies the arguments rather than
asserting it:

```
 2052   98.5%  qualified name, leftmost RESOLVES as a namespace — the same refusal
   11    0.5%  not a TypeReference: ArrayType
    9    0.4%  not a TypeReference: IndexedAccessType
    6    0.3%  DOWNSTREAM — name resolves, generic, arity ok, a type ARGUMENT gaps
    2    0.1%  not a TypeReference: TypeQuery
    1              arguments written on a non-generic type (checkNoTypeArguments)
    1          not a TypeReference: ConditionalType
    1          not a TypeReference: TypeOperator
```

The gapping arguments are the same mechanism at 98.5%. Nothing in the row is
waiting on a second owner.

## 3. The controls, all pinned before the run

| control | pinned by | expected | read |
|---|---|---:|---:|
| C1 construction — every classified root answers `errorType` | the definition of a gap root | 0 | **0** |
| C2 arithmetic — buckets sum to classified | partition | 4,010 | **4,010** |
| **C3 upstream** — `unresolved name, every argument types` | `getUnresolvedSymbolForEntityName` (`checker.go:23102`) renders the written name, so such a reference cannot answer `errorType` at all | **0** | **0** |
| **C4 upstream** — `qualified, root unresolved, every argument types` | `resolveQualifiedName` (`checker.go:15828`) returns nil when the left is not a namespace, so upstream falls through to the same unresolved path and prints the dotted text | **0** | **0** |
| **C5 corpus** — top-1 case lines in any unresolved bucket | the case declares 29 namespaces and uses 28 distinct dotted roots; set subtraction leaves **none** undeclared, so no root in it can fail to resolve | **0** | **0** |
| C6 construction — a gap line whose reference computes a type | C1 | 0 | **0** |

C3, C4 and C5 are the ones that matter, and they are the kind
`docs/conventions.md` demands after the `removeSubtypes` partition error: their
expected values come from **upstream's control flow and the corpus source**, not
from arithmetic over my own partition. An arithmetic control cannot see a wrong
partition, because a wrong partition still partitions. If C3 or C4 had read
non-zero, the mechanism map would have been wrong rather than the counts merely
misplaced — the `d356450` arm would not be reached at this position.

C5 was verified against the case source before the probe ran:
`tests/cases/compiler/resolvingClassDeclarationWhenInBaseTypeResolution.ts`
declares `namespace lavali` at line 257 and 28 others; every dotted root used in
the file is one of them.

## 4. The head cases, and why the top-1 split changes nothing

| lines | share | case | the namespace |
|---:|---:|---|---|
| 1,986 | 49.5% | `compiler/resolvingClassDeclarationWhenInBaseTypeResolution` | 29 user namespaces in-file |
| 513 | 12.8% | `compiler/underscoreTest1` | `Underscore.Static` |
| 480 | 12.0% | `compiler/temporal` | **`Temporal.*` — a lib namespace** |
| 67 | 1.7% | `compiler/enumAssignmentCompat3` | |
| 62 | 1.5% | `compiler/complexNarrowingWithAny` | |
| 51 | 1.3% | `compiler/dynamicNames` | |
| 51 | 1.3% | `compiler/overload1` | |
| 39 | 1.0% | `conformance/enumLiteralTypes1` | |
| 39 | 1.0% | `conformance/enumLiteralTypes2` | |

**100% of the top-1 case is the qualified-namespace refusal**, and so is 99.8%
of everything else. §4.3's instinct to separate the case was right in general —
`bd tsr-jle` was 10,000-of-11,004 one case — and here it buys nothing: the row
with and without it is the same finding at half the size.

Three cases are 74.3% of the row, which is a concentration worth stating; but
concentration is not the ceiling argument here, because want-any is **0.9%**
rather than the 59% or 79% that has refused other rows. The lines are reachable;
they are behind a refusal.

`compiler/temporal` is worth its own line: `Temporal` is a **lib** namespace
(`@lib: esnext.temporal`), not a user one. The mechanism is identical, so the
480 lines are not a separate item, but they do say the fix has to work through
lib symbols and not only through in-file `namespace` blocks.

## 5. What the row does *not* contain

The other endings hold the mechanisms one would expect a `TypeReference` row to
be made of, and none of them is in the §4.3 row:

| mechanism | `root is here` | `cycle` | total |
|---|---:|---:|---:|
| DOWNSTREAM — generic, arity ok, a type ARGUMENT gaps | 635 | 6 | 641 |
| generic, FEWER arguments than parameters (`fillMissingTypeArguments`) | 478 | — | 478 |
| qualified name, leftmost resolves — the same refusal | 471 | 84 | 555 |
| DOWNSTREAM — non-generic, the DECLARED TYPE gaps | 198 | 6 | 204 |
| arguments on a non-generic type (`checkNoTypeArguments`) | 155 | 52 | 207 |
| generic, MORE arguments than parameters | 2 | — | 2 |

Read across the whole kind, the qualified-namespace refusal is
**4,002 + 471 + 84 = 4,557 of 6,091 = 74.8%** of every `TypeReferenceNode` gap
root in the corpus.

The 478-line arity bucket is the only shard here that looks like an independent
item, and it is not this row's: it sits in `the dependency types — the root is
here`, and `fillMissingTypeArguments`' default fallback already landed at
`385fb60`, so what is left is the *substituting* half — a default that references
an earlier parameter, which `declared.rs` refuses on record.

## 6. The verdict: the row is not a new board item

**It converts into exactly one already-refused item, and it re-sizes it.**

`STATUS.md` §5 carries *"qualified naming build | 1,318 | 90.7% accurate on
target row; counterfactual lost 3,202 lines, regressed 753 cases"*. That refusal
is about the **printing** half — `type_to_string_at`'s symbol chain, measured in
`checker-notes-nameres.md` §49 — and its population was the 1,318 lines that were
*already wrong*. This row is the **resolution** half: 4,002 lines that are
currently a **gap**, where `get_type_from_type_reference` refuses to resolve
`M.I` at all. `bd tsr-awa` names the distinction itself — *"the 331 `import a =
b.c` lines are NOT part of it … they need qualified-name RESOLUTION, a different
item"*. That item has now been sized for the first time: **4,002 own-root gap
lines, 3,973 reachable.**

So the row does not dissolve into other owners' items the way the object-literal,
`FunctionDeclaration` and element-access rows did last session. It **collapses
into one owner**, and the honest scoring consequence is the same in one respect:
it is not a row to build against. It is evidence about an item that already has a
refusal.

### Two things have changed since that refusal, and both are checkable

1. **The prerequisite the refusal named has landed.**
   `checker-notes-nameres.md` §51 named the blocking defect exactly —
   *"`resolve_name` cannot see a namespace's exports"* (`nameresolver.go:104`'s
   `KindModuleDeclaration` arm) — and closed with an instruction:
   *"Nobody should build the chain again until (1) is fixed and the
   counterfactual re-run. That is the order: fix `resolve_name`, measure the
   corpus, then re-run this variant. The measurement costs two `casedelta`
   runs."* `resolve_name` gained that arm at **`3b7fa44`** (`bd tsr-56r`,
   +4,319), and `crates/tsr-binder/src/lib.rs` now carries the namespace-exports
   branch anchored to `nameresolver.go:104`–`:146`. **The refusal's own stated
   returning condition is met and the re-run has not been done.**

2. **The resolution half's data already exists — grepped, not assumed.**
   `docs/conventions.md`'s rule is *"before sizing an item on 'X is unported',
   grep for X"*. `Symbol::exports` is a public `SymbolTable` on
   `crates/tsr-binder/src/symbol.rs`, and `BindResult::resolve_name` already
   reads it. Walking `M.I.J` is a lookup per segment against data the binder
   already files. `declared.rs`'s comment says *"`resolveEntityName` … the binder
   does not expose yet"* — that comment is now **stale**, and it is the sixth
   instance of the stale-prerequisite failure `docs/conventions.md` records.

### The candidate design, marked unmeasured

Resolving `M.I` gets the type; **printing** it still needs a qualifier, and that
is the half that lost 3,202 lines. There is a third option the counterfactual
never tested, and it did not exist when the counterfactual was run: reuse the
**written** entity name for a reference the source spelled dotted, the same
mechanism `ff49871` built as `Parameter::written_text` / `Signature::written_return`
for `typeof`. The qualifier then comes from the source text rather than from a
computed `getSymbolChain`, so it **cannot fire on a line that did not write a
dotted name** — which is the entire blast radius that refused the chain.

This is a hypothesis with no number attached. It is written here because
`checker-notes-nameres.md` §52 lists a third untested variant as the obvious
next thing and this is a fourth; **nobody should build it before the
counterfactual is re-run**, on that page's own instruction and this project's
rule that a mechanism firing on a position rather than on a defect must have its
at-risk column computed in the same pass.

### CORRECTION, 2026-08-07, same session — it was measured, and it was built

**The paragraph above is superseded and its instruction is spent.** It is left
standing rather than edited away, per §8 of `STATUS.md`, because the reasoning it
contains turned out to be the load-bearing part.

The counterfactual was re-run the same session (`examples/qualname.rs`,
`docs/architecture/checker-notes-qualname.md`). It found the refusal had priced
**the wrong design**: the 3,202-line loss belongs to the design that qualifies
*every* printed name, and it had been quoted against a different design's
population. The mechanism this section describes was arrived at independently,
from a different instrument, and named **design W**:

| | converts | would-be-wrong | at risk | built |
|---|---:|---:|---:|---|
| **W** — reuse the written entity name | 1,770 | 99 | **0** | `8e28971`, **+3,590** |
| **P** — the computed symbol chain | 2,990 | 450 | 14 | `a57a04b`, **+2,973** |

**The sentence that mattered is the blast-radius one** — *"it cannot fire on a
line that did not write a dotted name"*. That is exactly why W's at-risk column
measured **0**, and it was written here before anyone measured it. Design P, the
printing half this section correctly separated out, then landed too, once it was
built against upstream's `needsQualification` stop conditions rather than
qualifying everything.

**Do not read the paragraph above as a live instruction.** Both halves are built.
What is still open is the residue, filed: alias naming 20 lines, enum narrowing 6,
and design P's outer symbol chain.

## 7. How you would know this page is wrong

- **The mechanism attribution is an artefact of the branch order.** It is not:
  the qualified branch is taken before arguments are looked at, and the argument
  classification in §2 shows the arguments are the same mechanism anyway. If a
  future `get_type_from_type_reference` reorders its branches, this table has to
  be re-run rather than carried.
- **The row is a `depend.rs` bucket and `depend.rs` is wrong about it.** The walk
  is copied, so a defect in the walk reproduces here rather than being caught.
  The check that would separate them is the §1 ending table: if the
  `no further dependency` share of `TypeReferenceNode` roots moves without any
  checker change, the walk is the thing that moved.
- **The want-any share is understated.** It is 0.9% and it is taken the same way
  every other probe on this board takes it — the baseline's answer text being
  exactly `any`. ADR-0038's ceiling is not a stable population (§2 of
  `STATUS.md`), so 3,973 is a ceiling on a ceiling.
- **The re-run of the refused counterfactual comes back negative again.**
  ~~That is the outcome this page cannot predict~~ — **RESOLVED, and it did NOT
  fire.** The re-run came back *positive*, at +3,590 (design W) and +2,973
  (design P). Recorded here rather than left silent, because **a falsifier that
  resolves and is not written down is worse than none**: the next reader cannot
  tell it was ever tested, and an untested falsifier and a passed one look
  identical on the page. The 4,002 lines were quotable as blocked when this was
  written; they are now converted.
