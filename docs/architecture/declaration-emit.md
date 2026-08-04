# Declaration emit (`tsr-declarations`)

**Status:** slice 4 of Phase 3.5 (`bd tsr-49v.6`), at **47.35%** on the byte-exact
emit gate and **67.76%** on the structural one. This is the artifact the phase
exists to ship: the first `.d.ts` this port has ever produced.

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
declaration transform calls **29 of them, at 39 call sites**; this port's
`SyntacticResolver` implements **8**, and the other 21 belong to features listed
under "known approximations" below. The trait is the seam; everything above it is a
port that does not know which implementation it has.

> **Corrected.** This section first said the transform calls 9 methods. That was a
> miscount of *this crate's trait*, presented as a measurement of `transform.go` —
> see the correction in
> [ADR-0022](../adr/0022-the-declaration-transform-is-a-port-around-a-resolver-seam.md#the-forcing-constraint),
> which also tabulates which 21 are missing and why.

The 8 implemented:

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

## The gates, and why there are two of them

```
dts_reachable_target   496/1162   42.69%    the population a checker-free emitter can aim at
dts_emit               161/340    47.35%    of which the text is byte-identical
dts_shape              618/912    67.76%    the declarations are right, ignoring types
```

`dts_emit` is the real gate: **byte-identical output for every unit of the case**,
not whitespace normalised and not comment-insensitive, because the text is the
product. Its denominator is the *reachable* set, because a case that draws a
`TS9xxx` has a `.d.ts` naming types written nowhere in its source. Counting those
as failures would bury the emitter's rate under 666 cases it was never in reach
of, and would make the number move whenever the *analysis* changed.

That exclusion is also its limitation, and it is why `dts_shape` exists. What
those 545 skipped cases need a checker for is the **types**; which declarations
survive and in what order is decidable from syntax — and visibility, elision and
ordering are precisely where this port approximates. `dts_shape` compares the kind
and name of every declaration, in order, recursing into namespace bodies and
stopping at class bodies. It judges **912** cases against `dts_emit`'s 340.

Its denominator depends on nothing but the corpus, unlike `dts_emit`'s, so a
regression there is always a regression in the transform.

### The two gates are complementary, and the mutation table proves it rather than
asserting it

Read the table at the end of this document by column. Four mutations move
`dts_emit` and leave `dts_shape` at exactly 618 — `declare`, initializers, enum
values, the optional `?`, all of which are type or text. The other four move both.
Neither number is a proxy for the other, which is the whole justification for
carrying two.

### Three denominators were wrong, in three different ways

Each was found by a different means, and the third only because a mutation was
run for an unrelated reason.

**1. An echoed input is not an output.** `dts_reachable_target` was published at
**575/1,289 (44.61%)**. It is **496/1,162 (42.69%)**.

Both suites read declaration output from the `//// [x.d.ts]` sections of the `.js`
baseline. A baseline echoes every **input** unit before the emitted files, so a
case with a `foo.d.ts` *input* — an ambient library, a `node_modules` stub —
carries a `//// [foo.d.ts]` section that upstream never emitted. 127 cases are
that shape. Reading the echo as output inflated the population and, in `dts_emit`,
made the emitter appear to produce files upstream did not: 215 failures reading
"emitted `x.d.ts`, upstream did not", every one of them the harness's fault.

The discriminator is exact rather than heuristic — a declaration section is output
iff no input unit has that name — and all three suites now apply it.

**2. The pairing was decided while emitting.** `dts_emit` was published at
**180/488 (36.89%)**. It is **160/340 (47.06%)**. The suite walked the source
units and looked for a matching baseline section as it went, so a unit whose
output happened to be empty fell through to a skip: improving the transform moved
cases between judged and skipped. The pairing is now computed from the baseline
alone, in `output_units`, before anything is emitted.

The rate went *up* by nine points, which is the uncomfortable direction for a
correction to move. It is not a loosening — the comparison is unchanged and still
byte-exact. The old denominator was inflated with unit/section pairs that were
never comparable.

**3. The suite short-circuited on the first failing unit.** Even with the pairing
fixed, a case whose *second* unit needs inference is a skip only if the first unit
has not already failed — so the denominator still depended on the output, one
level down. Removing `declare` moved 18 cases out of the skip bucket and into the
judged set (371 → 390).

This one was found by a mutation run for a different purpose, which is worth
recording: the mutation's *rate* was the thing being measured and the *denominator*
was the thing that turned out to be broken. Both suites now emit every unit and
decide every skip before comparing anything, and the denominator is fixed at 340
and 912 across all eight mutations below.

The same lesson as the round-trip suite, where the parse-cleanliness check had to
be hoisted ahead of all printing (`docs/architecture/printer.md`). Three times
now, in three suites: **anything that decides the denominator must run before the
component under test does.**

**[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md)'s first falsifier
still does not fire.** It asked whether "fewer than a few hundred" of the
`@declaration` cases are reachable. 496 is comfortably above, and it is 43% of the
population rather than a tail. The correction moves the number; it does not change
the decision.

### The checker is not confined to declaration emit

Worth stating here because Phase 5 inherits it. Emit as a whole reaches the
resolver from five more transformer packages:

| package | call sites | for |
|---|---:|---|
| `declarations` | 39 | all of the above |
| `moduletransforms` | 8 | referenced export containers, import declarations |
| `tstransforms` | 7 | decorator metadata, import elision |
| `estransforms` | 5 | |
| `jsxtransforms` | 4 | `GetJsxFactoryEntity` |
| `inliners` | 1 | const-enum `GetConstantValue` |

`emitter.go:112` fetches the resolver unconditionally. There is exactly one
checker-free escape and it does not apply here: at `:115`, *script* transforms fall
back to `binder.NewReferenceResolver` when import elision, JSX, `isolatedModules`
and decorator metadata are all off. Declaration emit has no such fallback, which is
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md)'s finding restated
from the driver's side.

## What the round-trip gate could not see

Three defects in `tsr-printer` were found by pointing a byte comparison at it, all
three invisible to 11,726 round-trip cases because none of them is in the tree.
They are the strongest evidence in this repository for what a structural gate does
not cover.

(The percentages below were measured against the pre-correction `dts_emit`
denominator of 488 and are left as measured. They are evidence about the *size* of
each defect, and are not comparable to the 47.06% above.)

| Defect | Effect | `dts_emit` |
|---|---|---|
| `TextWriter::new` started `line_start: false`; upstream's `Clear` sets it `true` (`textwriter.go:27`), so `emitSourceFile`'s opening `writeLine` is a no-op on an empty buffer | **every printed file gained a leading blank line** | 1.07% → 12.50% |
| An empty bracketed list wrote a space, and then — half-corrected — nothing. Upstream writes a *line* for an empty multi-line list (`printer.go:4745`), so `interface I { }` is `{`, newline, `}` | `f( )`, then `interface I {}` | 15.18% → 23.91% |
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
distribution: the residue is *text*. It is filed as `bd tsr-49v.6.1`. That is also
[ADR-0022](../adr/0022-the-declaration-transform-is-a-port-around-a-resolver-seam.md)'s
second falsifier — if raising the printer's fidelity stops moving the number, the
reachable target was measuring something other than what the emitter can do.

## Two gates are not two test suites

The corpus exercises **96%** of `transform.rs`. The unit suite, on its own,
reached **51.59%** — and `docs/conventions.md` says submodule-dependent tests
*skip* rather than fail, so the unit suite alone is what a contributor sees before
pushing and what CI sees without the submodule. Measured with `cargo llvm-cov`:

| | corpus run | unit tests, before | unit tests, after |
|---|---:|---:|---:|
| `transform.rs` | 96.35% | 51.59% | **83.53%** |
| `type_builder.rs` | 93.21% | 78.91% | 82.65% |
| `enum_value.rs` | 83.02% | 44.74% | 61.40% |
| `modifiers.rs` | 95.79% | 86.44% | 89.83% |
| `tsr-dts/rules.rs` | 98.14% | 79.86% | 81.56% |
| `tsr-dts/visibility.rs` | 92.25% | 67.83% | 74.03% |

`tests/transform.rs` closed most of that gap, and it is not only a coverage
exercise: it found a real defect on its first run. `buildClassMembers` passes
`ignorePrivate: false` to `ensureType` (`transform.go:1933`), so the *property* a
private parameter property declares emits no type — while the constructor
parameter it came from keeps one, because `ensureParameter` passes `true`. This
port emitted the type in both places, leaking a private member's shape. It parses,
so nothing structural saw it; it is one corpus case, so the byte gate barely saw
it either.

## Mutations

Eight, applied to the code under test and re-measured over the corpus, against a
baseline of **160/340** and **618/912**. (The `dts_emit` baseline is 161 after the
private-parameter-property fix above, which landed later; the eight deltas are
otherwise unaffected.)

| Mutation | `dts_emit` | `dts_shape` |
|---|---:|---:|
| `declare` never added (`ensureModifierFlags` drops the `AMBIENT` addition) | **39** | 618 |
| Namespace bodies elided again (nested declarations read as invisible) | 130 | **546** |
| Initializers kept on every declaration (`ensureNoInitializer` neutered) | 130 | 618 |
| Visibility pass disabled — every declaration visible | 143 | 565 |
| Scope-fix marker never appended | 143 | 577 |
| Enum values not folded, initializers emitted as written | 155 | 618 |
| Side-effect imports elided | 152 | 610 |
| Optional-parameter `?` never added | 159 | 618 |
| — baseline — | 160/340 | 618/912 |

Every mutation moves at least one gate, and **the denominators do not move at
all** — 340 and 912 in all sixteen runs. That is the property the third
denominator correction above was made to establish, and it is checked here rather
than asserted.

The column of unchanged 618s is the useful half. `dts_shape` is blind to exactly
the four mutations that change types or text and sensitive to exactly the four
that change structure. A shape gate that moved under "initializers kept" would be
byte-comparing by accident, and would stop being able to judge the cases it exists
for.

Two mutations are worth their own line. `declare` never added takes `dts_emit`
from 160 to **39**: nearly every declaration in a `.d.ts` carries it, so this is
the single most load-bearing line in `modifiers.rs`. And the optional-`?` mutation
moves it by **one case** — a real rule the corpus barely exercises in the reachable
set, covered by `tests/analysis_agreement.rs` instead.

### Two mutations silently did nothing, twice

The first run of this table reported "declare never added" and "enum values
unfolded" as moving the rate by **zero**. Both were `str::replace` calls whose
target text no longer existed: `cargo fmt` had reformatted both sites after the
mutation strings were written. The code compiled, the corpus ran, and the result
was indistinguishable from a rule the gate cannot see.

The harness now asserts the target substring is present before writing, and fails
loudly if it is not. A mutation that cannot be shown to have applied is not
evidence of anything — and the failure mode is *silence*, which is the same shape
as the finding it was meant to produce.

An earlier attempt had a coarser version of the same problem: this crate's files
were untracked, so the `git checkout` meant to revert each mutation failed and
they accumulated. Every number here is from a run that restores from a copy and
verifies the restore.
