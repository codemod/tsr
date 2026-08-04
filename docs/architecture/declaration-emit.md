# Declaration emit (`tsr-declarations`)

**Status:** slice 4 of Phase 3.5 (`bd tsr-49v.6`), at **36.89%** on the byte-exact
emit gate. This is the artifact the phase exists to ship: the first `.d.ts` this
port has ever produced.

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.
**Upstream counterpart:** `internal/transformers/declarations/` (4,160 lines).

The decision to port this rather than write it against the spec — and why that is
not in tension with [ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md),
which decided the opposite for `tsr-dts` — lives in
[ADR-0022](../adr/0022-the-declaration-transform-is-a-port-around-a-resolver-seam.md)
and is not restated here.

## Where it sits

```
  source tree ──► tsr-declarations ──► declaration tree ──► tsr-printer ──► .d.ts
                         │
                         └── EmitResolver ─┬─ SyntacticResolver (Phase 3.5)
                                           └─ CheckerResolver   (Phase 4)
```

`tsr_dts::analyze` says which declarations would need inference. `tsr_printer`
writes a tree. This builds the tree in between: bodies gone, initializers gone,
`declare` added, invisible declarations dropped.

## The seam, which is the whole design

Upstream's transform reaches the checker through one Go interface,
`printer.EmitResolver` (`internal/printer/emitresolver.go:77`, 44 members). The
declaration transform calls **9** of them. Those 9 are the trait; everything above
it is a port that does not know which implementation it has.

| Method | `SyntacticResolver`'s answer |
|---|---|
| `IsDeclarationVisible` | reachability from the exports, top-level only |
| `IsOptionalParameter` | a `?` token or an initializer |
| `IsLiteralConstDeclaration` | a `const`/`readonly` declaration, **unannotated**, whose initializer is a primitive literal |
| `CreateLiteralConstValue` | the literal, re-spelled: `0x1` → `1`, `` `1` `` → `"1"` |
| `CreateTypeOfDeclaration` | the initializer's *shape*, rewritten |
| `GetEnumMemberValue` | a constant-expression fold, including auto-numbering |
| `GetEffectiveDeclarationFlags` | the modifier list |
| `CreateReturnTypeOfSignatureDeclaration` | **refused** |
| `IsImplementationOfOverload` | **refused** (`false`) |

A refusal emits `any`, exactly as upstream's `ensureType` does when its node
builder returns nothing — and records the position in
`DeclarationEmit::inference_required`, so "the emitter guessed" is counted rather
than silent.

## The gate

```
dts_reachable_target   496/1162   42.69%    the population
dts_emit               180/488    36.89%    of which we reproduce byte for byte
```

A pass is **byte-identical output for every unit of the case**. Not whitespace
normalised, not comment-insensitive: the text is the product.

The denominator is the *reachable* set — cases where nothing needs inference —
because a case that draws a `TS9xxx` has a `.d.ts` naming types written nowhere in
its source. Counting those as failures would bury the emitter's rate under 666
cases it was never in reach of, and would make this number move whenever the
*analysis* changed. The stacking is the point: `dts_reachable_target` says what
could be aimed at, `dts_emit` says what was hit.

### The denominator was wrong, and by more than a rounding

`dts_reachable_target` was published at **575/1,289 (44.61%)**. It is **496/1,162
(42.69%)**.

Both suites read declaration output from the `//// [x.d.ts]` sections of the `.js`
baseline. A baseline echoes every **input** unit before the emitted files, so a
case with a `foo.d.ts` *input* — an ambient library, a `node_modules` stub —
carries a `//// [foo.d.ts]` section that upstream never emitted. 127 cases are
that shape. Reading the echo as output inflated the population and, in `dts_emit`,
made the emitter appear to produce files upstream did not: 215 failures reading
"emitted `x.d.ts`, upstream did not", every one of them the harness's fault.

The discriminator is exact rather than heuristic — a declaration section is output
iff no input unit has that name — and both suites now apply it.

**[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md)'s first falsifier
still does not fire.** It asked whether "fewer than a few hundred" of the
`@declaration` cases are reachable. 496 is comfortably above, and it is 43% of the
population rather than a tail. The correction moves the number; it does not change
the decision.

## What the round-trip gate could not see

Three defects in `tsr-printer` were found by pointing a byte comparison at it, all
three invisible to 11,726 round-trip cases because none of them is in the tree.
They are the strongest evidence in this repository for what a structural gate does
not cover.

| Defect | Effect | `dts_emit` |
|---|---|---|
| `TextWriter::new` started `line_start: false`; upstream's `Clear` sets it `true` (`textwriter.go:27`), so `emitSourceFile`'s opening `writeLine` is a no-op on an empty buffer | **every printed file gained a leading blank line** | 1.07% → 12.50% |
| An empty bracketed list wrote a space, and then — half-corrected — nothing. Upstream writes a *line* for an empty multi-line list (`printer.go:4744`), so `interface I { }` is `{`, newline, `}` | `f( )`, then `interface I {}` | 15.18% → 23.91% |
| Named imports and exports wrote their braces by hand instead of going through `emit_list`, so `LFNoSpaceIfEmpty` never applied | `export {  };` — the most common line in a `.d.ts` | 29.92% (+10 cases) |

The middle row is worth reading twice: the first attempt at the fix was *also*
wrong, in the opposite direction, and both spellings parse. Only the byte
comparison distinguished three behaviours that a tree comparison calls identical.

A fourth was in the separator guard, whose one non-character rule — a digit
followed by `.` continues a numeric literal — is a fact about **tokens**.
`typeof c1.foo` ends its first token in a digit too, and printed as
`typeof c1 .foo`. The guard now consults whether the last write *was* a literal.

## Findings that changed the transform

### Visibility is only defined at the top level

`tsr_dts::visibility` walks the file's own statement list and nothing else, so a
declaration inside a namespace body is absent from the set — not because it is
invisible, but because the set has no opinion. Reading that absence as "not
visible" elided the body of **every namespace in the corpus** and printed
`declare namespace M {}` for all of them. That parses. Nothing but a byte
comparison was ever going to catch it.

### An ambient namespace needs no scope marker, and the flag that says so is never set

`transform.go:1846` reads `input.Flags & NodeFlagsAmbient` to decide that a
`declare namespace` needs no `export {}` marker — everything in it is exported
already. Ported faithfully, the check moved the corpus by **exactly zero**,
because this parser does not set `NodeFlags::AMBIENT` on a `declare`d declaration.
The transform tests the `declare` *modifier* as well, and says so; the parser gap
is `bd tsr-qc3` and is not confined to this — anything asking "am I in an ambient
context?" gets the wrong answer today, and the checker will ask constantly.

### A side-effect import binds no name, so reachability has nothing to say about it

`import "./polyfill";` is unreachable from every export by construction, and
eliding it changes what an importer of the `.d.ts` loads. Upstream keeps it for
the same reason (`transform.go:2474`). This is the general shape of the risk in
using reachability as a visibility stand-in: the approximation is silent about
things it was never asked about, and silence reads as "no".

### An enum member without an initializer emits its ordinal

`enum E { A, B }` emits `enum E { A = 0, B = 1 }`, and `C = A | B` over constant
siblings emits `C = 3`. Upstream rewrites *every* member's initializer to the
checker's constant and drops it entirely when there is none, so keeping the source
form is not a formatting difference. The fold is in `enum_value.rs` and covers the
same constant-expression grammar `tsr_dts`'s `TS9020` rule already recognises —
which is the right amount, because a member outside that grammar is either
reported (so the case is excluded) or genuinely non-constant (so upstream drops
its value too).

### A `const` declaration is not a const assertion

Found by `tests/analysis_agreement.rs` on its first run, and it is the reason that
test exists. `const b = [1, 2]` has type `number[]`; `const o = { a: 1 }` has type
`{ a: number }`. Only `as const` produces `readonly` members and literal element
types — `compiler/isolatedDeclarationsLiterals` pairs the two spellings of one
object literal and its baseline reads `readonly one: 1` against `one: number`.

Both crates encoded the same conflation, so they *agreed while both being wrong*.
The emitter is corrected here; `tsr_dts` is `bd tsr-49v.2.6`, because changing it
moves `isolated_declarations` and `dts_reachable_target` and needs its own
measurement. The test asserts the disagreement rather than papering over it, so
fixing the analysis makes the test fail and say so.

## Known approximations

Each is a place where upstream consults the checker or the `Program` and this
crate does not. They are the expected source of divergence.

| Area | Upstream | Here |
|---|---|---|
| Visibility | `IsDeclarationVisible` | reachability from the exports, top-level only; no declaration merging |
| Overload sets | `IsImplementationOfOverload` | never elided, so an implementation signature is emitted alongside its overloads |
| Return types | `CreateReturnTypeOfSignatureDeclaration` | refused; the case is `TS9007`/`TS9008` and out of the target |
| `strictNullChecks` | a compiler option | absent. `null` widens to `any`, which is the corpus default and every baseline currently in the target |
| Module specifiers | re-emitted from the original source text | re-quoted with double quotes, so `require('x')` becomes `require("x")` |
| Comments | preserved in `.d.ts` output | dropped — the printer emits none |
| `module.exports =`, expando functions, JS/JSDoc declarations | `transformCommonJSExport`, `transformExpandoAssignment`, `visitThisPropertyAssignments` | **absent, not stubbed.** Each needs the `Program` or JSDoc types |

## What is left, by shape

Of 308 failures at 36.89%:

| | |
|---:|---|
| ~130 | text differences with a specific cause — statement ordering, destructured parameter properties, merged declarations |
| 25 | `any` where the emitter refused |
| 18 | comments, which the printer does not emit |
| 14 | quote style on module specifiers |
| ~10 | blank lines and other spacing |

None of these is a rule that needs a checker, which is the useful thing about the
distribution: the residue is *text*. That is also
[ADR-0022](../adr/0022-the-declaration-transform-is-a-port-around-a-resolver-seam.md)'s
second falsifier — if raising the printer's fidelity stops moving the number, the
reachable target was measuring something other than what the emitter can do.

## Mutations

Five, applied to the code under test and re-measured over the corpus, against a
baseline of **180/488**.

| Mutation | Rate |
|---|---|
| `declare` never added (`ensureModifierFlags` drops the `AMBIENT` addition) | **59/488** |
| Initializers kept on every declaration (`ensureNoInitializer` neutered) | 150/488 |
| Visibility pass disabled — every declaration visible | 163/488 |
| Enum values not folded, initializers emitted as written | 175/488 |
| Optional-parameter `?` never added | 179/488 |
| — baseline — | 180/488 |

All five move it, which was not a given: an exact-match gate is a poor gradient
until it is nearly satisfied, and `isolated_declarations` sat at 5/15 for a while
with four mutations moving it by zero. At 36.89% this one already discriminates.

The last row is the honest one to keep: **one case**. The optional-`?` rule is
real — dropping an initializer without adding the `?` changes the signature — and
the corpus barely exercises it in the reachable set. A mutation that moves the
rate by one is a rule the gate can *technically* see and would not protect. It is
covered by `tests/analysis_agreement.rs` instead.

The first three of these were run twice. The first pass was wrong: this crate's
files are untracked in git, so the `git checkout` meant to revert each mutation
failed silently and they accumulated — the "restored" baseline read 49/488. Every
number above is from the second pass, which restores from a copy. A mutation
harness that cannot revert is measuring the sum of everything it has done so far.
