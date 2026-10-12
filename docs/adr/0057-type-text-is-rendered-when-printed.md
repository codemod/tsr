# ADR-0057: A type's text is rendered when it is printed, not when the type is minted

- **Status:** **proposed**, not built. Nothing below exists yet; the
  integrator decides from the measured gain. Lane r7-perf, issue
  `tsr-2zk.1277`. Measurements: [`r7-perf.md`](../parity/notes/r7-perf.md)
  §44.
- **Date:** 2026-10-12
- **Would supersede:** [ADR-0050](0050-mapped-symbol-types-are-read-lazily-and-mapped-prints-truncate.md)'s
  rejection of its alternative 1, and [ADR-0052](0052-print-time-decisions-are-plans-the-site-renderer-reads.md)'s
  "the baked text stays the site-free print". Their plans and renderers
  (`mapped_object_text`, `deferred_member_text_at`,
  `deferred_conditional_text`) are kept: they become the print-time
  renderers, unchanged.
- **Pinned upstream:** `vendor/typescript-go` @ `5b1047d`.

## The forcing constraint

Native has no type text. `typeToString` (`checker/printer.go`) runs the node
builder when something prints, and nothing else ever asks. This port computes
a type's printed form when the type is minted and stores it in the type
(`TypeData::Named`, `Anonymous`, `Union`, `Intersection`, `EnumLiteral`,
`types.rs`). Three costs follow, and every type pays them whether or not it
is ever printed:

1. **The text itself.** Every union formats its constituents
   (`create_union_with_text`), every reference its arguments
   (`type_reference_text`), every signature its parameters
   (`signature_to_string`), every resolved mapped object its members
   (`mapped_object_text`).
2. **The forcing.** A text that lists members needs the members' types, so a
   type literal resolves its member types when it is minted, and a mapped
   type instantiates every property's template to print it. Native resolves
   both lazily (`resolveStructuredTypeMembers`, `getTypeOfMappedSymbol`,
   `checker.go:20984`). r7-declared measured a lazy-`DeepReadonly`
   micro-benchmark (200 aliases) at 209 M Ir, 170 M of it the creation-time
   text and the members it forced (`r7-declared.md`, reported to the
   integrator on 2026-10-12). This could not be fixed in `declared.rs`.
   Native prints these types in full too, but only when something prints
   them.
3. **Identity.** The union key is the whole `TypeData` payload, text
   included (`TypeStore::intern_literal`). So the text is hashed and cloned
   at every intern, and two unions with equal constituents but different
   origin text are two types. Native keys `unionTypes` on constituents,
   origin and alias (`getUnionKey`, `checker.go:17508`), never on a
   string.

ADR-0050 rejected print-time text because "every `type_to_string` reads the
baked text through `&self`" and a mapped print needs `&mut`. ADR-0052 kept
the baked text and moved individual print-time decisions to plans. Both
worked around the constraint; this record removes it.

## Measured on domain-model (dm) and domain-model-large (dml)

Callgrind, `--singleThreaded`, profiling build of main `a8012982` (batch 82).
"Outermost text Ir" is the inclusive cost of every edge from a non-text
function into a checker function whose name ends in `_text`, `_texts`,
`_text_*` or `*to_string*`. It counts each text computation once, together
with whatever that computation forced. The union constructor's interning
is subtracted, because a lazy text still interns.

| | dm | dml |
|---|---:|---:|
| total Ir | 785,810,748 | 3,410,949,809 |
| outermost text Ir | 31,716,517 | 149,492,355 |
| of which union interning (kept) | 2,261,707 | 10,518,310 |
| **text work a lazy text skips (upper bound)** | **29.5 M (3.75%)** | **139.0 M (4.07%)** |
| largest producers: `create_union_with_text` / `written_type_text_flags` / `signature_to_string` / `mapped_object_text` / `type_to_string` / `type_reference_text` | 7.4 / 5.6 / 5.3 / 4.3 / 3.4 / 3.0 M | 34.9 / 27.4 / 24.0 / 20.6 / 16.3 / 13.8 M |
| `DeepReadonly` cost: bench minus its `FrozenModelNNN` aliases and their `Defaults` consts | 15.25 M (1.94%) | 67.8 M (1.99%) |
| of which inside text producers | 0.3 M | 1.7 M |

These numbers bound what the stages below can recover on the bench
projects:

- **Stages 1–2 (lazy text)** recover at most the text work, 3.75% of dm's
  instructions and 4.07% of dml's. dm and dml print almost nothing (one
  diagnostic on dm), so nearly all of it is never read.
- **Stage 3 (lazy members)** recovers at most the `DeepReadonly` cost,
  1.9% and 2.0%. On the bench projects this is not text. Removing the
  aliases saves 13.9 M on dm, and only 0.3 M of that is inside a text
  producer. The rest is member and name resolution that the aliases' eager
  declared types trigger (`check_type_alias_circularity` is 30.8 M
  inclusive on dm). Part of that, native pays too. The 170-of-209 M ratio is
  the micro-benchmark's, not the bench projects'.
- **Wall, estimated and not measured.**
  - The checks run in parallel, so a uniform 3.75% cut to check-phase work
    is about 2–4 ms of dm's ~60 ms check phase and ~105 ms wall (2–4%).
  - On dml, 4.07% of its ~303 ms check phase is about 12 ms (~3% of
    ~360 ms).
  - Stage 3 would add at most about as much again.

  Stage 1 is the first point where this estimate can be checked.

## The proposal

Staged, with no flag day. Each stage compiles beside the previous one, and
lanes migrate as they touch code.

### Stage 0: one reader seam (no behaviour change)

Every read of a type's text goes through the checker:

- `Checker::type_to_string(id)` for the site-free print;
- `type_to_string_at` for the site print, as today.

The store-less `printing::type_to_string(&Type)` (13 calls) and the direct
pattern reads of the text fields (19 matches of `Named { text` and the
like) move behind `TypeStore::text(id)`, which still returns the baked
string. The current readers are:

- 703 calls of the `type_to_string` family in `tsr-checker`;
- 61 in `tsr-conformance`;
- one each in `tsr-compiler` and `tsr-execute`.

They already read through the checker and need no change at this stage.
This is the "about 577 sites" r7-declared counted, recounted on
`a8012982`.

Readers that **decide** on a text (12 comparisons or prefix tests in six
files, for example `declared.rs` and `unions.rs`) are listed in the stage-0
commit. Each becomes a test on type data or on a plan (ADR-0052), never on
a printed string. Native never compares type strings to decide anything.

### Stage 1: store-renderable text becomes lazy

Some texts are a pure function of the store: a union's or intersection's
text from its constituents and symbol, a named reference's from its symbol
and arguments, an enum literal's from its member. These variants keep a
`TypeText`, which is `Baked(String)` or `Deferred`. A deferred text is
rendered on first read into a `TypeStore` side table (`FxHashMap<TypeId,
String>`, written once, never invalidated, because a type's site-free
print is a function of an immutable type).

The union key becomes the constituents, symbol and origin. The origin is
held as a `TypeId` or a plan, not as a string. That identity change is the
stage's only parity risk, and the exact oracle measures it.

### Stage 2: checker-renderable text becomes lazy

Some texts need the checker:

- a type literal's or mapped object's members;
- signatures, whose parameter and return types may be unresolved;
- the reuse decisions of ADR-0052.

Those mints record a plan (ADR-0052's shape: inputs, never results), and
`type_to_string` renders it. `type_to_string` becomes `&mut self`. Its
`&self` callers are migrated module by module, each as a mechanical commit
in the owning lane. A `&self` caller that cannot become `&mut` (a reader
holding a store borrow) reads `TypeStore::text`, which answers `Deferred`
texts only once they have been rendered. That reader is listed and
removed before stage 2 closes. `mapped_object_text` and `TruncationBudget`
become the mapped renderer unchanged. Under a shared budget they become
exactly native's single-budget print (ADR-0050's second consequence goes
away).

### Stage 3: members resolve lazily

With no text forcing them, a type literal's member types and a mapped
type's property types resolve on first read, as `resolveStructuredTypeMembers`
and `getTypeOfMappedSymbol` do. ADR-0050's `Slot::Mapped` is the existing
pattern. Stage 3 extends it to type-literal slots. This is where the
micro-benchmark's 170 M is.

### Conventions entry (added to `docs/conventions.md` when stage 0 lands)

> **A type's text is read, never stored and never compared.** Read it through
> `Checker::type_to_string` / `type_to_string_at`; never through a
> `TypeData` field, and never build a type's text at its mint. A decision
> that native makes in the node builder is a plan the renderer reads
> (ADR-0052), not a string test. ADR-0057.

## Alternatives

- **Keep the baked text and plan individual decisions** (ADR-0052, today).
  It works decision by decision, but leaves costs 1–3 above in place for
  every type. It wins if stage 1 does not recover what the table predicts.
- **Make the text lazy without the `&mut` migration** (stage 1 only). It
  recovers the union, reference and literal share (about 1% of dm's Ir:
  `create_union_with_text` without its interning plus
  `type_reference_text`), but not the forcing. That is a valid stopping
  point if stage 2's migration is judged too large.
- **A memoised `&self` renderer with interior mutability over the whole
  checker.** Refused. The renderer instantiates types, which mutates every
  checker table. Making those `RefCell`s would be the cross-cutting change
  ADR-0050 declined, in a worse form.

## Consequences if adopted

- `type_to_string` becomes `&mut self`. That is about 700 call sites, each
  migrated by its lane; the integrator schedules the last flip, as for
  ADR-0056's field.
- The union key changes from text to origin. Every `.types` line that prints
  an origin is in the exact oracle and must stay exact.
- Printing gets slower per print: each site-free print is rendered once,
  then read from the side table. Corpus `.types` runs print every type, so
  their cost moves from mint to print. That shift is the expected cost, not
  a regression.

## How we would know this was wrong

- Stage 1 recovers less than the union and reference text share on dm
  (`create_union_with_text` minus interning, plus `type_reference_text`:
  about 8 M Ir). The table's attribution is then wrong.
- After stage 2, dm's and dml's Ir fall by less than half the 3.75% and
  4.07% upper bounds. Most text would then turn out to be read anyway.
- Any exact-oracle loss from the union key change. Some reader then depended
  on text identity, and stage 0's list missed it.
