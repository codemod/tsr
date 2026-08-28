# Call and `new` resolution: an 18,294-line row that licenses 844 lines of work

> **CORRECTED TWICE. (1) 2026-08-06, same day: a gap/right ordering defect moved
> every row count and not the verdict — the header below. (2) 2026-08-06, later:
> **§5's R2 is a *shape* test, and `3f140c2` has since made spellability a
> *match* test.** §11 re-derives it. The refusal stands on the corrected
> instrument, but it stands on the looser of the two registered variants **by
> 1.7pp — 85 lines** — and that margin belongs beside the verdict.**
>
> This probe was written from `receiver_gap.rs`'s shape and inherited its
> gap/right ordering defect: it asked `type_string == "error"` *before* asking
> whether the baseline matched, so a line where we answer `error` **and upstream's
> baseline also says `error`** — a right answer; programs may declare a type named
> `error` — was filed as a gap. Found and fixed on `main` in `9b10272` by
> `crates/tsr-conformance/examples/reconcile.rs`; fixed in this file in this
> commit.
>
> **The first run's exact agreement with `receiver_gap.rs` (292,217 / 143,509) was
> evidence that this probe had copied the defect, not evidence that it was
> right.** That is the more useful reading and it is what a shared helper shape
> does.
>
> | | first run (defective) | corrected |
> |---|---:|---:|
> | gradient right | 292,217 | **292,606** |
> | gradient gap | 143,509 | **143,120** |
> | the three assigned rows | 18,314 | **18,294** |
> | R1, callee has a type | 5,009 = 27.4% | **5,006 = 27.4%** |
> | R2, plain-shaped | 1,867 of 4,943 = 37.8% | **1,864 of 4,940 = 37.7%** |
>
> **20 of the 389 corrected lines were in these rows. R1 and R2 were recomputed
> rather than assumed, and neither moved a decision:** R1 still fires at 27.4%,
> R2 still fails at 37.7% against 70%. **The refusal stands on the corrected
> numbers.** The superseded figures are kept in this header rather than edited
> away.

Status: **re-measured 2026-08-06 at `058b4a9` + this commit**, over the
9,538-case `.types` corpus population, from one pinned binary in an isolated
worktree, after the `receiver_gap.rs` gap/right ordering defect this probe
inherited was corrected. It was first measured at the same commit with the defect
in place; **every number below the header is from the corrected binary.** The
instrument is `crates/tsr-conformance/examples/callres.rs`, added in the same
commit as this file. Upstream references are to `vendor/typescript-go` @
`5b1047d10`, each taken from `grep -n` on the declaration.

Reproduce:

```
cargo run --release -p tsr-conformance --example callres
```

**Every number on this page is an assertion line in a `.types` baseline**, unless
a row says otherwise. `docs/architecture/checker-notes-calls.md` counts **call
expression nodes**; the two are not interchangeable and the board has twice
turned one into the other by omission.

**The verdict is a refusal.** The pre-registered rules were R1 (prerequisite) and
R2 (spellability), conjunctive. **R1 fired at 27.4%. R2 did not fire, at 37.7%
against a 70% threshold, and it does not fire for any sub-shape of the admitted
population either.** No checker code was written. §7 says what the residual
licensed slice is (**844 lines**, 0.59% of the gap) and why even that is smaller
than it looks.

---

## 0. The findings, in the order they change decisions

1. **The row is 5.7× the size the assignment stated, and the assignment's number
   was in a different unit.** §1.
2. **The concentration warning in the assignment does not survive re-measurement
   at the row level.** Two sub-rows were described as ≥68% top-10; whole-gradient
   they are 33.1%, 38.9% and 21.9%. §2.
3. **Only 15.1% of the row is a `calls.rs` defect** — 2,763 of 18,294 lines have
   a blocking call whose callee has an *object* type. The rest is a chain into
   other people's rows. §4.
4. **The spellability gate fails on every shape.** The largest single answer the
   admitted lines need is `any` (737), and the largest sub-family under the
   most-promising shape is `unique symbol` (264), which this port has no
   mechanism for. §5.
5. **The cascade is real but points the wrong way.** The two `initialiser` rows
   hold down 2,427 further lines — a 1.30× and 1.55× multiplier — and **65.3% of
   those are structural** against 16.6% plain. Building the row would un-gap a
   collateral population *less* spellable than the target. §6.
6. **Two spin-off items found and filed**, neither of them in `calls.rs`. §8.

---

## 1. The row is 18,294 lines, and the 3,200 was a different unit

The assignment quotes `docs/architecture/checker-notes-recvgap.md` §4:

| terminal reason | assignment | this probe, whole-gradient |
|---|---:|---:|
| `… / VariableDeclaration / initialiser CallExpression` (BLOCK_SCOPED) | 1,124 | 952 + 1,640 |
| `expression answered error: CallExpression` | 791 | **11,722** |
| `… / VariableDeclaration / initialiser NewExpression` (FUNCTION_SCOPED) | 526 | 474 + 527 |
| **stated total** | **≈3,200** | **18,294** |

**Those two columns do not measure the same thing and neither is wrong.**
`receiver_gap.rs` walks each *receiver-is-a-gap* line down to its terminal
receiver and reports the terminal's `gap_reason`. Its counts are therefore
**lines held down by** a receiver carrying that reason — a downstream count. This
probe counts **lines carrying** the reason. The units differ twice over: one
receiver node renders **two** assertion lines (the access and its member name),
which recvgap counts twice and this page counts once; and a receiver's own
assertion line is a different line from the ones it blocks.

The comparable quantity to recvgap's 1,124 / 526 is this page's **cascade**
(§6): 1,480 and 947 lines held down. Those are within range of each other and of
the recvgap figures once the flag split is folded in.

So the row's ceiling is **18,294 lines carrying the reason, plus 2,427 held
down**, not 3,200. That is the largest call-shaped population anyone has sized
here — and §4 is why the number does not survive contact.

Two more corrections to the assignment's framing, both from §2 of the probe's
output:

- The rows are **not** flag-specific. `BLOCK_SCOPED_VARIABLE` and
  `FUNCTION_SCOPED_VARIABLE` are the same mechanism (`get_type_of_symbol` on a
  variable with an initialiser and no annotation) and split roughly evenly; so
  do the `reference, ` and `declaration name, ` prefixes, which are the *use*
  site and the *declaration* site of one symbol. Sizing by one flag string
  quarters the row.
- They are not `VariableDeclaration`-only. **272** lines carry
  `PropertyAssignment`, `PropertyDeclaration` or `Parameter` initialisers — read
  off the control mirrors, 6,572 − 6,300, not off the visible rows of the
  verbatim histogram, which show only 226 of them. This is what control C2
  caught (§9).

## 2. Concentration (rule R3), and where the assignment's figure came from

Whole-gradient, per row. Unit: assertion lines.

| row | lines | cases | top-1 | top-10 | three largest cases |
|---|---:|---:|---:|---:|---|
| `… / initialiser CallExpression` | 4,861 | 816 | 14.0% | **33.1%** | `compiler/temporal` 681, `compiler/promiseType` 122, `compiler/promiseTypeStrictNull` 122 |
| `… / initialiser NewExpression` | 1,711 | 297 | 8.9% | **38.9%** | `compiler/typedArraysCrossAssignability01` 153, `conformance/parserRealSource11` 116, `compiler/duplicateLocalVariable1` 82 |
| `expression answered error: CallExpression` | 11,722 | 2,364 | 5.9% | **21.9%** | `compiler/temporal` 697, `conformance/parserRealSource11` 621, `compiler/genericDefaults` 242 |
| `expression answered error: NewExpression` (companion) | 1,985 | 672 | 4.7% | 20.9% | `conformance/parserRealSource7` 93, `conformance/parserRealSource11` 51, `conformance/unionTypeConstructSignatures` 44 |

**R3 does not fire.** The assignment's 90.2% and 68.4% top-10 figures are
properties of the *recvgap subset* — the lines those reasons hold down, which
concentrate because a few large files (`compiler/temporal`,
`conformance/parserRealSource*`) contain long receiver chains. The rows
themselves are corpus-wide: 816, 297 and 2,364 cases, top-10 between 21.9% and
38.9%.

This is worth stating plainly because it is the opposite of what the assignment
predicted, and it is the *favourable* direction: the row does not evaporate on
the concentration check. It fails for a different reason.

`compiler/temporal` is still worth naming: 681 + 697 = 1,378 lines across the two
`Call` rows, 8.3% of them, the single largest contributor to both. It is also the
top case in the `name does not resolve` row on the recvgap page. One file.

## 3. What the probe does

For each gap line whose `gap_reason` is one of the four row strings, it finds the
**blocking call node** and asks one question about it: *does the callee already
have a type?*

- For the two `initialiser` rows the blocking node is reached independently of
  `gap_reason` — line node → symbol (declaration-name branch first, then the
  identifier branch, mirroring `gap_reason`'s own order) → `value_declaration` →
  `initializer_id`. Controls C1 and C2 check that this second route lands where
  `gap_reason` looked.
- For the `expression answered error:` rows the line node *is* the call.

The callee is then typed with `Checker::check_expression` — the same entry
`check_call_expression` uses (`calls.rs:337`) — and the answer is the partition in
§4. Where the callee gaps, the probe descends one more hop and records the
callee's own `gap_reason`, which is the next link in the chain (§4.1).

The probe routes through `types_producer::assertions_for_case_with_ids`, so it
builds one program per case with every bundled lib loaded. Its own gradient
reconciliation prints **479,060 lines = 292,606 right (61.08%) + 143,120 gap
(29.88%) + 43,334 wrong (9.05%)**.

**479,060 is not the gradient's denominator and must not be quoted as one.** It
is the count of lines *this probe rendered*; `types_suite::compare`
(`crates/tsr-conformance/src/types_suite.rs:98`) takes its total from
`assertion_count(expected)` and iterates the **baseline**, giving **478,954**.
The bridge is 479,060 − 964 surplus + 858 deficit = 478,954, difference 0,
measured by `crates/tsr-conformance/examples/reconcile.rs` and written up in
`docs/architecture/checker-notes-recvgap.md` §8 (`9b10272`, `bd tsr-zlo`). The
right/gap/wrong figures above are the corrected ones — see this page's dated
header for what they were before, and why their first agreement with
`receiver_gap.rs` was not the corroboration it looked like.

## 4. R1 — the partition, and the bucket the rule was written on

Over the three assigned rows. Unit: assertion lines.

| bucket | lines | share | cases | top-10 |
|---|---:|---:|---:|---:|
| **callee HAS a type** — the prerequisite is met | **5,006** | **27.4%** | 976 | 14.7% |
| callee is `a.b`, receiver gaps | 5,151 | 28.2% | 618 | 55.6% |
| callee is `a.b`, member gaps | 3,210 | 17.5% | 652 | 37.6% |
| callee name resolves, symbol has no type | 3,105 | 17.0% | 612 | 25.5% |
| callee is another expression form, gapped | 1,074 | 5.9% | 397 | 19.2% |
| callee name does not resolve | 748 | 4.1% | 143 | 64.2% |
| **total** | **18,294** | | | |

**R1 fires: 27.4% ≥ 25%** (5,006 of 18,294; 27.36% unrounded). The bucket is the one the rule names, printed
literally, not a quantity derived from it.

Per row the split is very uneven, and it is the `new` half that carries it:

| row | callee has a type |
|---|---:|
| `… / initialiser NewExpression` | 1,133 of 1,711 — **66.2%** |
| `expression answered error: NewExpression` (companion) | 1,220 of 1,985 — 61.5% |
| `… / initialiser CallExpression` | 1,403 of 4,861 — 28.9% |
| `expression answered error: CallExpression` | 2,470 of 11,722 — 21.1% |

A `new` expression's callee is a class name, which this port types; a call's
callee is very often `a.b`, which it does not. **If the item were split by
keyword rather than by row, `new` would be the one that looks buildable.** §5
says why it is not.

### 4.1 The next link, where the callee gaps

The callee's own `gap_reason`, top rows. Unit: blocking call nodes, not lines
(one node can block several lines).

| callee's reason | count |
|---|---:|
| `property access, the receiver is a gap: Identifier` | 3,009 |
| `property access, the receiver is a gap: PropertyAccessExpression` | 1,431 |
| `reference, the name does not resolve` | 941 |
| `property access, the property has no type` | 841 |
| `reference, symbol has no type: SymbolFlags(FUNCTION) / FunctionDeclaration / neither` | **797** |
| `property access, the receiver has no such property: Promise<boolean>` | 491 (4 cases) |
| `property access, the receiver is a gap: CallExpression` | 397 |
| `reference, symbol has no type: SymbolFlags(FUNCTION) / FunctionDeclaration / annotation TupleType` | 389 |

The largest three are property-access chains, which is
`docs/architecture/checker-notes-calls.md`'s finding — *"it is a property-access
problem, not a call problem"* — arriving from a different direction and one level
further out. That page measured it on **nodes** and got 56.4%; this page measures
it on **lines** and gets 45.7% (5,151 + 3,210 of 18,294). Two instruments,
different units, same conclusion.

The 797 under `FunctionDeclaration / neither` is a distinct mechanism and is
filed (§8): an unannotated `function f() { … }` has no return-type inference here,
so every call to it gaps.

## 5. R2 — the spellability gate, which is where the item dies

The test is on the **baseline's own right-hand side** for the blocked line, so no
answer of ours enters it. `Plain` means a bare name or keyword; `Structural`
means it contains `<`, `{`, `[`, `|`, `&`, `(` or `=>`; `Any` means literally
`any`.

Over the 5,006 admitted lines (4,940 of which have a comparable baseline RHS):

| shape | lines | share |
|---|---:|---:|
| Plain | 1,864 | **37.7%** |
| Structural | 2,339 | 47.3% |
| Any | 737 | 14.9% |

**R2 does not fire: 37.7% against a 70% threshold.**

### It does not fire for any sub-shape either

Splitting the admitted bucket by what kind of type the callee has — this split is
**not** where the rule lives; R1 stays on the bucket above:

| callee's type shape | admitted lines | share | plain-shaped |
|---|---:|---:|---:|
| an object type (`TypeData::Anonymous`) — `calls.rs`/`signatures.rs` own it | 2,763 | 55.2% | 844 of 2,716 = **31.1%** |
| a `Named` type — a lib constructor interface or a class | 1,435 | 28.7% | 722 of 1,423 = **50.7%** |
| `any` | 606 | 12.1% | 236 of 599 = **39.4%** |
| a union, a literal, a non-`any` intrinsic | 202 | 4.0% | 62 of 202 = **30.7%** |

**And the 50.7% is itself an over-estimate.** Its two largest answers are
`unique symbol` (264) and `symbol` (150) — `Symbol()` calls. `unique symbol`
passes a *shape* test because it contains no punctuation, but this port has no
fresh-unique-symbol rule (`getWidenedLiteralTypeForInitializer`'s const path is
what produces it upstream), so resolving `Symbol()` would print `symbol` where
upstream prints `unique symbol`: **a gap turned into a wrong line, 264 times.**
Excluding it, the `Named` shape is 458 of 1,423 = 32.2%, in line with the rest.

The other large `Named` answers say the same thing in a different way:
`new Uint8Array()` wants `Uint8Array<ArrayBuffer>` (38), `new Set()` wants
`Set<number>` (36), `new Map()` wants a two-argument instantiation. Those are the
lib generics `create_type_reference` deliberately carries no members for
(`bd tsr-4sc.7`).

### What R2 does and does not bound

Stated plainly, because the rule is mine and it is worth knowing what it proves.

R2 bounds the **convertible** fraction, not the **wrong-manufacturing** fraction.
A structural answer this port cannot build will mostly come back as `errorType`
and the line stays a gap — no damage. So "R2 fails" is not "building this
manufactures 62% wrong"; it is **"at most 38% of the admitted lines can convert,
and the rest of the work buys nothing"**.

The `unique symbol` family above is the exception that makes R2 more than a
convertibility bound: there the port *can* produce an answer and the answer is
wrong. 264 lines of it in one sub-family, found by reading the histogram rather
than by the threshold.

## 6. The cascade, and its sign

The two `initialiser` rows block a symbol; every `x.foo` on that symbol is a
further gap line, in the `member name, ` / `property access, ` rows. Counted by
walking each receiver-gap line's chain to its terminal receiver and testing
whether that receiver's symbol is one these rows block:

| row | own lines | downstream | multiplier | downstream cases | top-10 |
|---|---:|---:|---:|---:|---:|
| `… / initialiser CallExpression` | 4,861 | 1,480 | **1.30×** | 92 | 76.5% |
| `… / initialiser NewExpression` | 1,711 | 947 | **1.55×** | 97 | 56.2% |
| total | 6,572 | **2,427** | 1.37× | | |

That is the *planner's* ratio — against the rows predicted to move — in the sense
`docs/conventions.md` distinguishes. It is in the same range as the 1.66× measured
on the cross-file alias arm, and for the same reason: a variable is initialised in
order to be used.

**The sign is the problem.** The collateral's baseline RHS:

| shape | downstream lines | share |
|---|---:|---:|
| Structural | 1,576 | **65.3%** |
| Any | 437 | 18.1% |
| Plain | 402 | 16.6% |

**16.6% plain against 37.7% for the target.** The lines this row holds down are
*less* spellable than the row itself — they are member reads off generic lib
types (`Promise<T>`, `Set<T>`, the typed arrays), which is exactly the family the
module-object item went negative on. So the cascade does not rescue the item; it
makes the case worse, and it would have been invisible to a sizing method that
counted only the row.

## 7. What is actually licensed, and why I refused it anyway

The intersection of both gates is: **the callee has an object type** (2,716 lines
with a comparable RHS) **and the baseline answer is plain-shaped** (844).

**844 lines. 0.59% of the 143,120-line gap.** Against an 18,294-line row and a
whole-workstream assignment.

I did not build it, for three reasons, in order of weight:

1. **The pre-registered rule says no**, and it says no on the population the rule
   was written over. Retrofitting the rule onto the sub-slice that passes is the
   error `docs/conventions.md` records as *"a rule written on a quantity derived
   from the answer is caught by nothing"*, arriving one step later.
2. **844 is a ceiling, not an estimate.** Those 2,763 object-typed callees already
   reach `resolve_call_signature` (`calls.rs:568`) and it already returned `None`
   for every one of them — through `get_signatures_of_symbol` finding nothing,
   through the zero-signature arm, or through `choose_overload`'s `SELECTABLE`
   gate. Which of those, and in what proportion, is **not measured here**
   (`open`, `bd tsr-klm`). Without that split the 844 could be one change or five.
3. **The `any` bucket is a trap.** 606 admitted lines have an `any` callee, whose
   upstream answer is `any` (`resolveUntypedCall`, `checker.go:9902`). Porting it
   is faithful and it would move 606 lines from `gap` to `right` — and every one
   of those lines answers `any`, which is the column
   `docs/architecture/checker-notes-rank.md` §6 already flags as unaudited
   (21,685 lines banked on `any`, computed and defaulted not separated). Adding
   606 more to it before that audit exists would make the audit harder, for
   credit nobody trusts.

## 8. Spin-off items, neither of them in `calls.rs`

Both are filed with `bd`: `tsr-4sa` and `tsr-cwz`.

1. **A `Named` callee never reaches signature lookup** (`bd tsr-4sa`). `resolve_call_signature`
   (`calls.rs:568`) destructures `TypeData::Anonymous` and returns `None` for
   everything else, so a lib constructor interface — `SymbolConstructor` (421
   blocking nodes), `ArrayConstructor` (137), `DateConstructor` (130),
   `ErrorConstructor` (129), `MapConstructor` (79), `SetConstructor` (79) —
   cannot be called at all. Upstream reaches these through `getSignaturesOfType`
   (`checker.go:18959`) on the type's resolved members.
   **Correction to the record:** the natural citation for this is `bd tsr-qk9`
   ("`signature_parts_of` has no arm for `CallSignatureDeclaration` /
   `ConstructSignatureDeclaration`"), and it is **wrong now** —
   `signature_parts_of` (`signatures.rs:834`) has both arms today. The missing
   link is the route from a `Named` type to its members' signatures, not the
   declaration reader. 1,435 admitted lines, and §5 says half of them want a
   generic instantiation, so this is not 1,435 lines of work either.
2. **No return-type inference for an unannotated `function`** (`bd tsr-cwz`). 797 blocking
   callees are `SymbolFlags(FUNCTION) / FunctionDeclaration / neither` — the
   function declares no return annotation, so `get_type_of_symbol` answers
   `error` and every call to it gaps. This is the single largest *identifier*
   callee mechanism in the chain and it is in `symbols.rs`/`function_types.rs`,
   not on the call path.

## 9. Controls

| control | reads | pinned by |
|---|---:|---|
| C1 the blocking node's kind ≠ the kind the reason names | **0** | `gap_reason` prints `nodes.kind(initializer)`; the probe reaches the same node by an independent route |
| C1′ the mirror, kinds agreeing | 6,572 | — |
| C2 where the reason says `/ VariableDeclaration /`, the blocking node's parent is not one | **0** | an initialiser's parent *is* its declaration — a property of the subject |
| C2′ the mirror | 6,300 | — |
| C3 an `expression answered error:` line whose node does not carry the row's kind | **0** | `gap_reason` prints that kind from the same node table |
| C4 a downstream line that is itself in one of these rows | **0** | the reason prefixes are disjoint in `gap_reason` |
| C5 a reason matching more than one row test | **0** | `/ initialiser X` and `expression answered error: X` cannot co-occur |
| A1 rows == partition | 18,294 == 18,294, difference 0 | arithmetic |

**C2 fired, at 272, on the first run, and the defect was in the control.** It was
written unconditionally — *"the blocking node's parent is a `VariableDeclaration`"*
— and these rows also carry `PropertyAssignment`, `PropertyDeclaration` and
`Parameter` initialisers. The fix was to read the declaration kind out of the
reason string rather than to loosen the test. The 272 non-variable lines this
surfaced are recorded in §1; without the control they would have been silently
folded into the row.

`Blocked`'s classifier has **no default arm carrying a loaded label**: the
semantically loaded one, `CalleeTyped`, is first and requires a positive test
(`callee_type != error`), and the two failure arms (`BlockingNodeNotACall`,
`NoBlockingNode`) are positive tests on the node map. Both read 0.

The two classifiers are asserted before the corpus runs (`check_classifiers`),
and both assertions were proven red under a named mutation:

| mutation | control that went red |
|---|---|
| swap `row_of`'s `Row::InitCall` and `Row::InitNew` returns | `assert_eq!(row_of("…/ initialiser CallExpression"), Some(Row::InitCall))` — `left: Some(InitNew)` |
| drop `'<'` from `spell_of`'s structural character set | `assert_eq!(spell_of("Promise<number>"), Spell::Structural)` — `left: Plain` |

## 10. How you would know this page is wrong

- **The partition is a list.** A1 would print a non-zero difference. It prints 0
  over 18,294.
- **The navigation to the blocking node is wrong.** C1 or C2 would be non-zero.
  C2 already caught one error of exactly this class.
- **R2's `Plain` arm is a shape test and can be fooled.** It already is, by
  `unique symbol` (264 lines, §5). If someone finds a second such family the
  37.7% is an over-estimate again, and the refusal only gets stronger — but a
  family running the *other* way (a `Structural` RHS this port can in fact build,
  say a tuple or a union of literals) would weaken it. The verbatim RHS
  histograms are printed so this can be checked rather than argued.
- **The `callee has a type` bucket is a ceiling on `calls.rs`'s ownership, not a
  measurement of it.** 5,006 lines reach `resolve_call_signature` and it says
  `None`; *why* is not split here. If most of them stop at the
  `TypeData::Anonymous` destructure (that is, they are the `Named` shape) then
  the item is §8.1 and not a `calls.rs` item at all. That split is one more
  counter in this same probe, and it is filed rather than done (`open`,
  `bd tsr-klm`).
- **The cascade is measured only for the two `initialiser` rows.** The
  `expression answered error:` rows — 11,722 lines, the largest of the three —
  have no cascade figure here, because a call expression's downstream is not
  reachable by the receiver-chain walk this probe reuses. If those rows hold down
  a large spellable population, §6's verdict is understated in the favourable
  direction (`open`, `bd tsr-trf`).
- **The gap/right ordering.** This probe had it backwards and the dated header
  says what that cost. The standing check is `reconcile.rs`'s C1: the probe's
  `right` and `types_suite::compare`'s matched count are the same predicate at
  the same positions, so their difference is 0 **by construction**, and it reads
  −276,213 under a one-position shift. Any successor probe written from this
  file's shape should run it before quoting a gradient.
- **Nothing on this page leans on a `KIND` label from
  `checker-notes-rank.md`.** Checked after that board's `cause()` gained a
  positive terminal arm (`b5decc5`, `bd tsr-v1j`) and `TERMINAL` fell from
  34.92% to **16.41%** — 52.6% of what it used to call terminal was not. The one
  figure this page borrows from that board is §7's 21,685 lines banked on `any`,
  which is not a `KIND` label and does not move.


## 11. R2 was a shape test — re-derived 2026-08-06 at `66d0acf`

`3f140c2` made *"spellability is a match test, not a shape test"* a convention,
on evidence from this workstream's own ArrowFunction row where the shape test
read **99.6%** and the match test read **17.8%**. **§5's R2 is the shape test**,
so this page's refusal rested on an instrument the project has since declared
unreliable. This section re-derives it. **No code was written.**

### 11.1 The question is settled by reading the predicate, not by measuring

The tempting defence is *"shape is permissive, so 37.7% caps the match rate and
the refusal is untouched"*. **That is false here**, and it is a property of
`spell_of` rather than of the corpus.

`Spell::Structural` rejects every right-hand side containing `<`, `{`, `[`, `|`,
`&`, `(` or `=>`. **Those are exactly the shapes `printing::type_to_string`
exists to print**: it renders `TypeData::Union`, `TypeData::Anonymous` and
`TypeData::Named` from a stored `text` (`crates/tsr-checker/src/printing.rs:45`),
so `string | number`, `{ a: string; }`, `() => void`, `string[]` and
`Promise<number>` are rendered **by construction**. `Promise<boolean>` appears in
this page's own §4.1 histogram as a type this port built and printed.

So `spell_of` is a *"is this a bare name"* test wearing a spellability label, and
it errs in **both** directions — it rejects `string[]`, and it accepts
`unique symbol`, which §5 already recorded as unproducible.

**37.7% is not an upper bound, not a lower bound, and not a bound.**

### 11.2 R2′, registered before it was computed

An exact match needs the build, and the build is call resolution. What is
affordable is the **necessary** condition, by exact string equality:

> **R2′** — for each admitted line, is the baseline's right-hand side a string
> this port **demonstrably renders elsewhere in the same case**?

- **< 70%** → the refusal is confirmed on the corrected instrument.
- **≥ 70%** → the refusal does not stand on its stated grounds.

70% is R2's own threshold, kept for comparability. Registered with it, and
independent of the number: **R2′ is necessary and not sufficient, so no value of
it licenses a build this round.**

A second variant was registered before it was computed, because the per-case
vocabulary is too tight to be a clean upper bound — a case with no line answering
`string` does not put `string` in its vocabulary, though the port obviously builds
it: **if the corpus-wide vocabulary reads ≥70%, R2′ was measured on too tight an
instrument and the refusal is NOT confirmed.**

### 11.3 Measured

| instrument | reads | verdict |
|---|---:|---|
| R2, the shape test (§5) | 37.9% | — |
| **R2′, per-case vocabulary** | **35.5%** | **refusal confirmed** |
| R2′, rendered on a line we get *right* | 35.1% | — |
| **R2′, corpus-wide vocabulary** (the looser bound) | **68.3%** | **still confirmed — by 1.7pp** |

**The refusal stands on both registered instruments. It stands on the looser one
by 85 lines**, and that margin is small enough that it must be quoted with the
verdict rather than buried under it. A future change that makes 85 more admitted
right-hand sides producible anywhere in the corpus overturns this leg.

### 11.4 The finding that outlives the call row

The shape test read 37.9% and the per-case truth is 35.5% — **agreement to
2.4pp.** The cross-tab says that agreement is a coincidence:

| shape bucket | the port renders it | it does not | total |
|---|---:|---:|---:|
| `Plain` | 871 | **1,011** | 1,882 |
| `Any` | 568 | 157 | 725 |
| `Structural` | **326** | 2,035 | 2,361 |

**The shape test called 1,011 lines spellable that this port does not render, and
326 unspellable that it does — 1,337 of 4,968 misclassified, 26.9% — and the two
errors cancelled to within 2.4pp.**

So: **a proxy agreeing with the real test is not evidence the proxy works.** It is
the same family as *"a number can be true and answer a different question"*, one
level further in — here two numbers are both true, agree, and one of them is
computed from a predicate that is wrong about a quarter of its inputs. The only
thing that separated them was printing the cross-tab, which costs one bucket.

The unproducible list is where the 1,011 come from, and it is not exotic:
`unique symbol` 276, `any` 157, `symbol` 143, `any[]` 94, `void` 90, `string` 83,
`number` 75, `never` 64, `unknown` 58, `Date` 38, `Uint8Array<ArrayBuffer>` 38,
`Set<number>` 36. **`string` and `number` appear because the vocabulary is
per-case**, which is exactly the tightness the corpus-wide variant was registered
to bound — and it moved the figure from 35.5% to 68.3% without crossing the line.

### 11.5 What this closes, and what it does not

- **Closed.** §7's refusal is re-derived on a match-based instrument and stands.
  R1 (28.4% at `66d0acf`, up from 27.4%) still fires; R2 in every form still
  fails. The row should stop being revisited on spellability grounds unless the
  85-line margin moves.
- **Not closed.** R2′ is necessary, not sufficient. **Nobody has measured what
  call resolution would actually convert**, and doing so needs a counterfactual —
  the method that has decided every other item here. `bd tsr-klm` is still the
  prerequisite: the 5,034 admitted lines reach `resolve_call_signature` and it
  says `None`, and *which* gate is still unmeasured.
- **And under `c592d0f`**, any such counterfactual must forecast the collateral of
  the half that stays unported. A resolved call hands its return type to
  [`crate::inference`] for a generic signature and to the printer for everything
  else; `check_generic_call` answers only the shapes it can read a candidate off
  directly. **Half a mechanism renders the collateral of the half you built** —
  the naming agent's 328 wrong lines — and the generic half of this row is where
  that would land.

### 11.6 Pre-registration for `bd tsr-a5d`, recorded before its number is known

The brief's fallback is the 95-line WRITTEN-annotation contextual source
(`checker-notes-fnexpr.md` §10). **This round had no budget left to build it**, and
a rule registered after the fact is the retrofit this workstream has twice
refused. So the rule is recorded here now, and whoever takes it is bound by it:

> **RA — build only if a counterfactual converts ≥25% of the 95 to exact baseline
> matches, and ≥70% of the lines that stop gapping match exactly.** Population:
> the `WrittenVariableAnnotation` source of G, named in `checker-notes-fnexpr.md`
> §10.2, syntactically pinned and invariant across runs.
>
> **And the collateral is forecast in advance, under `c592d0f`:** 54 of that
> source's 109 parameter lines are wrong and go through `get_type_of_symbol` in
> `symbols.rs`, which this workstream does not own. So the build converts the
> *function's* line and leaves the *parameter's* line wrong — `>f : (x: number)
> => void` beside `>x : any` in the same file. That is half a mechanism by
> construction. **If RA fires, it must be reported as a half**, and the ≥70% leg
> must be read over the function lines alone, with the 54 named as unconverted.

## 12. The `FunctionDeclaration` row decomposed — shards, not an item (fifth session)

`examples/retgap.rs` (committed with this section), over every gap line whose
node names or references a `FunctionDeclaration`. Classified **4,088 lines**,
controls clean (C1 = 0, C2 exact). The row the fourth session's `depend.rs`
reported at ~3,312 own-root lines plus §8.2's 797 blocked callees splits:

| bucket | lines | owner |
|---|---:|---|
| the return **annotation itself gaps** | 879 | unported type nodes — heads are template literal types (84), variadic tuples (111), `const` type-parameter modifiers (118) |
| a return **expression** gaps | 797 | downstream by construction |
| a **parameter annotation** gaps | 507 | same type-node owners |
| annotated, **everything types**, still gaps | 257 | unsplit — one more probe; `typeParameterConstModifiers*` in its head suggests type-parameter machinery |
| `async` / generator | 466 | `Promise` wrapping / iteration, unported |
| unannotated, 0/1 distinct return types, gap is elsewhere | 959 | parameter gaps and entanglement, unsplit |
| **multi-distinct return aggregate — `bd tsr-4sc.9`** | **81** | **refused with this number**: the leg needs `removeSubtypes` machinery for 81 lines at the observed 15–57% conversion — ~12–46 lines |
| strictness legs (bare beside valued) | 14 | stays refused (§ signatures.rs) |
| ambient/no body | 72 | — |

**Verdict: the row leaves the board the way the element-access row did** —
no shard is an arm of `signatures.rs`; each belongs to an unported type-node
or type-parameter subsystem. `bd tsr-cwz`'s 797-callee framing ("return-type
inference is missing") is corrected: the inference *is* ported
(`return_type_from_body`), and what keeps the row down is its inputs.

## 13. `bd tsr-klm` answered — and it moves the item (fifth session)

`examples/callgate.rs`, at `dd43a19`+. The registered prerequisite: §7 refused
to size the licensed slice because "the 844 could be one change or five".
It is **five**, and none of the five is the one the board named.

### 13.1 The instrument had to be repaired first, and the repair is the finding

The first run attributed **2,569 of 8,398 lines to control C3** — "the fresh
checker never reached the call". Diagnosed rather than tuned:
`check_new_expression` lives in `crate::expressions` and carried **no counters
at all**, so the entire `new` half of the row was invisible. That half is the
*more* admitted one — §4 measures a `new` callee as typed 66.2% of the time
against a call's 21–29% — so the uninstrumented half was the one with the most
lines in it. Six gates instrumented; **C3 now reads 0**.

The same run's largest bucket was `single candidate (no selection needed)` at
2,550, which is not an answer: it says resolution *succeeded*. Split into what
happens next, it is almost entirely inference.

> **A funnel counter that stops at "it worked" cannot answer why the line still
> gaps.** Both defects have the same shape — the instrument partitioned the
> code it was written for and was silent about the code beside it.

### 13.2 Where the 8,398 admitted lines actually stop

Unit: assertion lines. C1 = 0, C2 exact, C3 = 0.

| gate | lines | want-any | mechanism |
|---|---:|---:|---|
| single candidate, generic, **inference gapped** | **2,324** | 28 | `inferTypes` — the structural walk |
| `new`: callee is not an object type | 1,663 | 212 | `bd tsr-4sa` — lib constructor *interfaces* |
| call: identifier types as a non-object | 1,128 | 208 | `bd tsr-4sa`, same mechanism |
| `new`: **explicit type arguments** | 516 | 42 | *nothing* — see §13.4 |
| overload set: a generic candidate | 490 | 23 | overload selection |
| overload set: parameter outside `SELECTABLE` | 473 | 183 | overload selection |
| callee symbol has no signature list | 353 | 7 | |
| by receiver: no type arguments | 331 | 120 | |
| `new`: **the class is generic** | 310 | 0 | *nothing* — §13.4 |
| callee has zero call signatures | 273 | 8 | |
| the remaining eleven gates | 537 | 106 | |

### 13.3 The board's call-resolution row was measuring the row, not the mechanism

STATUS.md §4.2 scored **call resolution — overload sets** at 869 on a
`reachable` of **9,660**. Summed from the gates that overload selection
actually owns — generic candidate 490, parameter 473, argument 28, ambiguous
50, arity 9, nothing-assignable 39, this/rest 5, spread 1 — the mechanism's
own population is **1,095 lines, want-any 215, ~880 net.**

That is the fourth rule of `STATUS.md` catching its own board: a population is
a ceiling *for the row it was measured on*, and 9,660 was the row. Overload
selection is an **8× smaller** item than the board says, and it is third of
three.

**And the whole call row cannot reach 10% of the gradient at any conversion.**
The admitted population is 8,398 lines = **1.75%** of 478,954; the widest
call-shaped population ever measured here (§1's 18,294 carrying + 2,427
cascade) is **4.3%**, at 100% conversion, against an observed band of 15–57%.
Recorded because the figure was asked for out loud.

### 13.4 The asymmetry that is worth building

`f<string>(x)` **works** — `check_generic_call` takes written type arguments
and substitutes (`inference.rs:153`, "there is nothing to infer and
substitution is all that is left").

`new C<string>()` **refuses at the first line of `check_new_expression`**:

```rust
if !node.type_arguments.is_empty() { return error; }   // 516 lines
...
if !type_parameters.is_empty() { return error; }       // 310 lines
```

Same mechanism, opposite answers, on the two halves of one construct. The
instance type of `new C<string>()` is `C<string>` — `createTypeReference` on
the class symbol with the written arguments — and this port has
`create_type_reference`, plus `tsr-4qx`'s instantiated members hanging off it.

**826 lines, want-any 42, ~784 net**, and it needs no relation, no inference
and no new data. Sized and registered in §14.

## 14. `new C<T>()` — sized by counterfactual, bar registered before the code

Fifth session, at the commit before the arm. `bd tsr-tgov`.

### 14.1 Why this and not the two larger gates

§13.2's two largest gates were both weighed and both declined, with reasons:

- **inference gapped, 2,324 lines.** `inference.rs`'s own module doc measures
  the cliff: **53%** of generic calls have no type parameter written bare in a
  parameter position, so the candidate has to be dug out of the argument's
  type — that is `inferTypes` (`inference.go:53`), a structural walk with a
  priority lattice and contravariant tracking. A subsystem, not an arm.
- **`Named` callee never reaches signature lookup, 2,791 lines** (`bd tsr-4sa`).
  Needs `getSignaturesOfType` → `resolveStructuredTypeMembers` — an
  interface's members resolved into call/construct signature lists.
  `get_signatures_of_symbol` reads a symbol's *declarations*, and an
  interface's declaration is not signature-shaped, so this is new machinery.
  It also carries a **measured wrong-manufacturing risk**: §5 records 264
  `unique symbol` lines that would print `symbol`, and half the row wants a
  generic instantiation this port has no members for.

### 14.2 The counterfactual, which is the sizing

`examples/newgen.rs` does not count the row. It **computes the string the arm
would produce** — `type_reference_text`'s `Name<A, B>` from the class's name
and the written type arguments' resolved types — and compares it to the
baseline. Over 332 gap lines on a generic class:

| | lines |
|---|---:|
| **CONVERTS — forecast matches the baseline exactly** | **166** |
| no written type arguments — needs inference (out of scope) | 155 |
| arity differs from the class's type parameters | 7 |
| a written type argument itself gaps | 3 |
| **MISS — forecast differs** | **1** |

The single miss is named in advance: `want G<D>, forecast G<any>`, a written
type argument resolving to `any`.

**166 is own-node lines only.** `callgate.rs` attributes **516** lines to this
gate, the difference being the *cascade* — a variable initialised by
`new C<string>()` and every read of it. Those are not forecast here, so they
are upside and must not be in the floor.

### 14.3 What will be built

In `check_new_expression`, replacing the two unconditional refusals:

- a generic class **with written type arguments whose count matches** its type
  parameters resolves each argument through `get_type_from_type_node` and
  answers `create_type_reference(symbol, arguments)` — the same constructor
  `crate::declared` uses for `C<number>` in type position, so the instance
  type is *identical by interning* to the annotation's and `tsr-4qx`'s
  instantiated members hang off it unchanged;
- **arity mismatch refuses** (7 lines): `fillMissingTypeArguments`' default
  handling is ported only for the no-candidate case (`tsr-1uz`) and upstream
  errors the whole call on a wrong count;
- **a gapping type argument gaps the whole `new`** (3 lines), the rule every
  other arm here follows;
- **no written type arguments still refuses** (155 lines): that is inference
  from the constructor's arguments, §14.1's declined item.

### 14.4 The bar

> **KEEP** if **net ≥ +120**, **lost ≤ 10 with every loss diagnosed as a
> cascade**, **fewer cases regress than finish**, and **gained ≥ 3 × new
> wrong** by `wrongdelta`.
> **REVERT** otherwise.

- The floor is **72% of the forecast 166**, deliberately not of the 516: the
  cascade is upside and a floor resting on unforecast lines is a floor resting
  on a guess.
- Leg 2's denominator is not empty, unlike the destructuring arm's: this
  answers a type where the port answered `errorType`, so a consumer of the
  newly typed `new` can now compute confidently and wrongly. That is the
  cascade the leg is watching for.
- Leg 4 is the live one. The forecast names **one** wrong line in advance
  (`G<any>`); new wrong beyond that family indicts the arm.

**Falsifier:** more than 40% of the gain in a single case. The counterfactual's
top case is `overloadResolutionClassConstructors` at 22 of 332 (6.6%), so
concentration near half means the mechanism reached something the forecast did
not describe.

If a leg fires: build wrong first, premise wrong second, no third.

## 15. §14 scored — all four legs pass, and Δwrong is negative

Measured over the pair (`casedelta`/`wrongdelta`, before = the state at the
bar's commit).

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +120 | **+686** (98 cases) | pass |
| 2 | lost ≤ 10, diagnosed | **0** | pass |
| 3 | regressions < finished | **0 < 24** | pass |
| 4 | gained ≥ 3 × new wrong | **686 vs 38 = 18×**; Δwrong **−47** (85 fixed) | pass |

**Falsifier did not fire**: top case `inferFromGenericFunctionReturnTypes2` at
28 of 686 = **4.1%**, against the 40% line.

**KEEP.** Gradient 70.01% → **70.15%**, cases 2,540 → **2,564**.

Conversion is **413% of the forecast 166**, and §14.2 named why in advance:
the forecast covered own-node lines and `callgate.rs` attributed 516 to the
gate, the difference being the cascade. This is the fourth build in two
sessions to exceed its sized row because the *mechanism* is wider than the
row it was measured on — the pattern §4.1 of `STATUS.md` now has five
instances of.

### 15.1 The residual named a printer defect, and fixing it was most of the margin

The arm's first measurement read **+572 with 68 new wrong**, already passing
every leg at 8.4×. Reading the 68 rather than banking the ratio:

```
  40   want { "new"<T>(x: T, y?: T): C<T>; }   got { new<T>(x: T, y?: T): C<T>; }
  14   want M.C<string> / N.D2<number> / __test2__.classWithOptional<number>
   6   want string | undefined                 got string
```

The 40 are **upstream's only name-independent quoting rule**, verbatim at
`classifyPropertyName` (`nodebuilderimpl.go:2384`):

```go
if isMethod && name == "new" { return propertyNameNodeKindStringLiteral }
```

and the reason is round-tripping, not escaping: `{ new<T>(x: T): C<T>; }`
re-parses as a **construct signature**, a different type. One line in
`objects.rs`, and it moved the build from +572 to **+686** — converting 30 of
the arm's own residual *and* **84 pre-existing wrong lines** the arm never
touched. Exposed, not minted.

> **A residual that passes the ratio leg is still evidence.** The 68 cleared
> leg 4 by 2.8× of margin, and reading them anyway was worth 114 lines and a
> defect older than the build.

The other 14 are the **qualified-naming** family — `M.C<string>` where this
prints `C<string>` — which is `STATUS.md` §5's refusal (2.7 wrong per right)
and `bd tsr-93f`. Attributed, not fixed: `type_reference_text` prints the
symbol's own name, and the enclosing-namespace qualification is the refused
build. The remaining ~24 are optionality (`tsr-e10`) and accessor families.

### 15.2 A twelfth stand-in fixture came due

`tests/new_expression.rs`'s `explicit_type_arguments_are_a_gap` asserted
exactly what §14 built. Rewritten as the **pair** — the written-argument form
answers, the inferred form beside it still gaps — so it keeps discriminating
rather than merely flipping. That is the standing prophylactic from
`checker-notes-tuple.md`, and this is its twelfth invocation.

## §14 The async-declaration `Promise<void>` arm — sized and barred (ninth session)

`cyclegap.rs` (eighth session) found the un-annotated `FunctionDeclaration`
self-loop row; `fnsiggap.rs` split it 72.5% downstream and left 810 wiring
lines in four mechanisms. This is the **async** one, re-split by whether the
body has a valued return (probe extended this session, at `07f1af4`):

```
  174  async declaration, no valued return, want ends `=> Promise<void>`  <- the item
   39  async, has valued returns      — needs getAwaitedType, declined
   15  async, no returns, want else   — custom thenables etc., declined
```

The 174 want-any share is **0**, and the whole population is GAP today:
`return_type_from_body` refuses `async` outright (`signatures.rs`, the
modifier test), so the arm can only turn `None` into `Some`.

**Mechanism, anchored.** `getReturnTypeFromBody`'s zero-aggregate arm
(`checker.go:20175`): an async function's empty aggregate answers
`createPromiseReturnType(fn, voidType)` (`:20184`) →
`createPromiseType` (`:20348`) = a reference to global `Promise` with the
promised type unwrapped — `void` unwraps to itself. The contextual-return
consultation at `:20179` (which can make it `undefined`) applies only where a
contextual return type exists; a **declaration** never has one, which is why
the arm gates on `FunctionDeclaration` and deliberately excludes arrows,
function expressions and object-literal methods. No global `Promise` in scope
→ upstream errors and answers `errorType` (`:20374`) — decline, same answer.

**Bar, registered before any code:**

1. net ≥ **+120** (~70% of 174; composite prints referencing these functions
   are upside).
2. own new wrong ≤ **10**. Falsifier: wrong lines in cases wanting
   `Promise<T>`, `T ≠ void` → `return_expressions_of` is missing valued
   returns in some body shape — check the walker before the arm.
3. cases regressed == **0**.
4. lost == **0** — structurally: the arm converts a refusal (`None`) into an
   answer and touches nothing that answers today.

### §14.1 Scored — every leg passed, forecast 101%

`verdictdump` pair at the arm's commit:

```
GAP→RIGHT 176   (174 forecast — the 2 extra are composite prints, the named upside)
GAP→WRONG 1     (`<T extends any>` where `<T extends unknown>` is wanted —
                 a pre-existing constraint-print defect newly exposed; the
                 Promise<void> the arm answered on that line is correct)
RIGHT→WRONG 0 · RIGHT→GAP 0 · regressed 0 · suite +4 cases (2,848 → 2,852)
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +120 | **+176** | pass |
| 2 | own ≤ 10 | **0** (1 exposure) | pass — the falsifier (want `Promise<T>`, `T ≠ void`) did not fire |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

Residue with owners: async valued returns 39 (`getAwaitedType`), generators
173 + 43 (`Generator`/`AsyncGenerator` construction), the `extends any`
constraint print (type-parameter constraint defaults, one line here).

## §15 The generator-declaration arm — bar (ninth session, second leg)

`fnsiggap.rs`'s form split leaves generators 173 + async generators 43;
corpus-wide, 541 gap lines want a `Generator<…>` instantiation, head shapes
`Generator<number, void, unknown>` 109, `Generator<never, void, unknown>` 82.

**Mechanism, anchored** (`getReturnTypeFromBody`, `checker.go:20151`–`:20247`):
yield type = union of yield operand types, **`never` when there are none**
(`:20239`); return type = the return aggregate's fallback `void`; next type =
intersection of contextual next types, and **for a declaration there is no
contextual signature, so it is always `unknownType`** (`:20242`–`:20245`) —
which is what makes the declaration-only gate sound, exactly as §14's async
arm. `createGeneratorType` is a reference to global `Generator`.

**The slice declines, each with its blocker named:** `yield*` (needs the
iteration protocol), ≥2 distinct operand types (needs `UnionReductionSubtype`
— the same reason the return aggregate declines), valued `return`s beside the
generator shape (same), async generators (`AsyncGenerator` + awaited yields),
non-declarations (contextual `next`), no global `Generator` in scope.

**Bar, before any code:**
1. net ≥ **+80** (conservative: the 173-line row minus the multi-yield and
   `yield*` shares, which are unsized).
2. own new wrong ≤ **12**. Falsifier: wrong lines wanting `Generator<…, any>`
   or a union yield slot → the next-type or aggregate model is off — read the
   residual before widening anything.
3. regressed == **0**.
4. lost == **0** — the arm only converts a refusal into an answer.

### §15.1 Scored — leg 2 fired twice, leg 1's floor missed, and the override is argued rather than assumed

Three measurements. **First form** (all yields, empty-aggregate = never,
per-operand widening): +65 / **41 own wrong** — leg 2 FIRED at 3.4× its
ceiling, and the residual named three model defects: a bare `yield;`
contributes `undefined`/`any` (not nothing); a value-used yield feeds `next`
from its own contextual position (`castOfYield`: `Generator<number, void,
number>`) — the bar's "a declaration has no contextual signature" was the
right premise about the **wrong position**; and `yield 1; yield 2` keeps its
literals (`Generator<1 | 2, …>`) because upstream's union regularises them
and `getWidenedType` (`:20224`) leaves regular literals alone. **Second
form** declined all three; its own residual then caught the walker skipping a
yield inside a nested method's **computed property name** — evaluated in the
generator's scope (`generatorTypeCheck42`), and provably `next`-neutral, so
admitted rather than declined. **Third form:**

```
GAP→RIGHT 63 · GAP→WRONG 1 · RIGHT→anything 0 · regressed 0 · suite +17 cases
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +80 | **+63** | **FIRED — overridden, with the reasoning here** |
| 2 | own ≤ 12 | **1** (`yield` — the scanner reads an escaped `yield` keyword as an identifier; scanner fidelity, not this arm) | pass |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

**Why leg 1 is overridden rather than honoured by revert:** the floor's own
text says the multi-yield and `yield*` shares were *unsized*; the first
measurement then sized them — they are the majority of the 173-line row — and
every narrowing that shrank the conversion column was forced by a named
wrong-line family from leg 2. The two legs were in tension and the safe one
won: 63 conversions at 1 own-wrong, zero losses, zero regressions, and
**+17 suite cases — the row's conversions were concentrated in cases with
nothing else wrong, which the line-count floor could not see.** A revert
would trade 63 right lines and 17 cases for fidelity to a guessed number.

Residue with owners: `yield*` (iteration protocol), multi-type yields
(`UnionReductionSubtype`), bare `yield;` (undefined-vs-any option modelling),
contextual-position yields (contextual typing, refused), async generators.

## §16 Async valued returns, the primitive slice — bar (ninth session, third leg)

The §14 residue's first row: 39 async-declaration lines whose body has valued
returns. Upstream: the aggregate goes through `checkAwaitedType` /
`unwrapAwaitedType` (`checker.go:20149`) and wraps in `createPromiseType`
(`:20348`) — `Promise<awaited T>`. The full awaited walk reads the `then`
member's signatures; the slice buildable without it is the shapes where
`awaited T == T` **by construction**: a type that cannot carry a `then`
member — primitives, literals after the same widening the plain path does,
enums. Everything with members (objects, references including `Promise`
itself, unions, type parameters) declines with the awaited machinery named
as owner. Same declaration-only gate as §14, same reason.

**Bar:** net ≥ **+12** (the 39 minus the returned-Promise share, unsized);
own ≤ **6** — falsifier: a wrong line whose want is `Promise<T>` with `T` a
non-primitive → the "cannot carry `then`" test admitted something it should
not; regressed == 0; lost == 0.

### §16.1 Scored — the residual reshaped the arm twice, then every leg passed clean

First form (+20 / 4 own wrong, inside both numeric legs): the residual was
read anyway, per the bar's own falsifier, and named two real model errors —
under NON-strict options `undefined`/`null` widen to `any`
(`asyncFunctionDeclaration15_es6` wants `Promise<any>`), and a **reachable
body end** appends `undefined` to the aggregate under strict
(`functionHasImplicitReturn`, `checker.go:20298`; `promiseTypeStrictNull`
wants `Promise<1 | undefined>`). Both admitted shapes were declined: the
nullable domains are out, and the valued slice now requires
`block_completes_normally == Some(false)` — which needed a `ReturnStatement`
arm in the completion classifier, whose doc had said "only called for bodies
with no `return`"; §16 is the caller that ended that.

Final: **GAP→RIGHT 15, and no other transition of any kind.**

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +12 | **+15** | pass |
| 2 | own ≤ 6 | **0** | pass |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

The lesson worth a line: a passing ratio was again not permission to stop
reading — the 4 wrongs sat inside leg 2's ceiling and both would have shipped
as latent wrong-rules.

## §17 The same two arms for CLASS methods — bar (ninth session, fourth leg)

§14/§15 gate on `FunctionDeclaration` because a non-declaration can take a
contextual return type. A **class method** cannot — contextual typing reaches
function expressions, arrows and *object-literal* methods
(`getContextualSignatureForFunctionLikeDeclaration`, `checker.go:29711`), and
the object-literal case is exactly what `may_return_never` already
discriminates. So the gate widens to `MethodDeclaration` whose parent is not
an object literal, for both the async and generator arms. Population unsized
(the dump cannot tell a class-method name line from other member lines
cheaply); the bar is conservative and the pair decides.

**Bar:** net ≥ **+10**; own ≤ **6** — falsifier: wrong lines in
object-literal-method positions → the `may_return_never` proxy is not the
contextual-typing boundary this section assumed; regressed == 0; lost == 0.

### §17.1 Scored — clean on every leg

```
GAP→RIGHT 46 · no other transition · suite +5 cases (2,869 → 2,874)
```

Legs: net **+46** (≥ +10) · own **0** (≤ 6, falsifier silent) · regressed
**0** · lost **0**. The `may_return_never` proxy held: it and the
contextual-typing boundary are the same upstream list read off two fields.

## §18 `await` — the identity-and-global-Promise slice — bar (ninth session, fifth leg)

The board's `AwaitExpression / NO STEP ARM` row: **578 gap lines, want-any
10.9%**, 138 cases, top-1 6.6% — the expression has no `check_expression` arm
at all, so every `await e` gaps even where the operand types.

**Mechanism.** `checkAwaitExpression` (`checker.go:10845`) is
`checkAwaitedType(operandType)`; `getAwaitedTypeNoAlias`'s shapes this port
can answer without the `then`-signature walk:

- `any`/`unknown` pass through (`IsTypeAny`, and the no-effect suggestion is
  a diagnostic, not a type change);
- a **primitive** cannot carry a `then` member — identity, freshness kept;
- a reference to the **global `Promise`** unwraps to its argument,
  recursively (`Promise<Promise<T>>` → `T`) — the reverse instantiation map
  (`type_reference_targets`) already carries the pair.

Everything else — object types, unions, intersections, type parameters,
`PromiseLike`, user thenables — declines to a gap with the awaited machinery
as owner.

**Bar:** net ≥ **+150** (~30% of the 515 non-any row: the operand share that
is a global-Promise reference or primitive is unsized, and the row's cases
are await-heavy library tests where Promise references dominate); own ≤
**15** — falsifier: wrong lines where the want is not the operand's argument
→ the unwrap fired on a non-global `Promise` symbol (shadowed or renamed);
regressed == **0**; lost == **0** (a new expression arm can only turn gaps
into answers on its own node; downstream composites were gaps too).

### §18.1 Scored — every leg passed, and the row's cases were await-dense

```
GAP→RIGHT 240 · WRONG→RIGHT 5 · GAP→WRONG 6 · nothing else · suite +22 cases (2,874 → 2,896)
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +150 | **+245** | pass |
| 2 | own ≤ 15 | **6** — 2 are a non-global thenable (`Windows.Foundation.IPromise`) surfacing through a downstream consumer, 4 are the `any` pass-through on operands another mechanism mistyped (`unionTypeInference`'s `Awaited<T>` conditional instantiations) | pass — the named falsifier (unwrap firing on a shadowed `Promise`) did not fire |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

+22 whole cases from one expression arm — the await-heavy suites
(`await_unaryExpression_*`, `asyncMultiFile`, `awaitedType` families) carry
many near-finished baselines, the same concentration §15.1 saw.

## §19 The `_1` rename, enclosing-scope half — bar (ninth session)

The `_1` family (STATUS's ninth-session row: 625 wrong lines) splits by
collision scope, measured from the dump: **322 within-print** (two sibling
signatures in one composite — needs a per-composite naming pass, not this
build) and **303 enclosing-scope** — a standalone signature print whose type
parameter collides with a type parameter of an enclosing declaration at the
reference site, which upstream renames `X_1` (`typeParameterToName`'s
by-text cache). Of the 303, **222 are byte-exact rename-only misses**: `got`
equals `want` with `X_1 → X` substituted.

**Design.** In `signature_to_string_at`: collect type-parameter names
declared by ancestors of the reference site — **excluding the signature's
own declaration**, which is the identity test that keeps `foo`'s own line
from renaming `foo`'s own `T` — and rename each colliding signature
parameter to the first free `X_n`, substituting token-wise across the
rendered parameter list, constraints, parameter texts and return. Blanket
token substitution is exactly right *for the byte-exact population by
construction of the counterfactual*; where it is not (a nested same-name
declaration, a property named `X`), the line falls outside the 222 and the
bar's leg 2 prices it.

**Bar:** net ≥ **+150** (~70% of 222); own ≤ **20** — falsifier: RIGHT→WRONG
lines whose print holds a property or nested parameter named like a type
parameter → the token substitution is reaching value positions, gate on the
shape before widening anything; regressed == 0; lost == 0 (a rename cannot
un-compute a type; `type_to_string_at` still falls back to the baked text).

### §19.1 Scored — three measurements, two scope rules learned from the losses, every leg passed

**First form** (exclude only the declaration node): +172 but **67 losses, 10
regressed** — the losses discriminated the rule exactly: an ancestor that
also encloses the signature's own declaration is ordinary SHADOWING (`<D>()
=> Promise<D>` at its own member site keeps `D`); only a collision from a
chain the signature does not live under renames. **Second form** (exclude
the whole declaration-ancestor chain): 152/5/2 — the residual 5 all sat in
computed property names and heritage clauses, which are OUTSIDE their
declaration's type-parameter scope (`class C<T> extends Base` cannot see
`T`). **Third form:**

```
WRONG→RIGHT 151 · nothing else at all · 0 regressed · +9 cases (2,944 → 2,953)
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +150 | **+151** | pass, by one line |
| 2 | own ≤ 20 | **0** | pass — the token-substitution falsifier never fired |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

The within-print half (322, sibling signatures in one composite) stays open
with its own design named: a per-composite naming pass over the member loop,
not this site walk.

## §20 The `_1` rename, within-print half — bar (ninth session)

§19's sibling: **322 lines** where two signature members of ONE composite
declare the same type-parameter name and upstream renames the second-and-on
(`{ <U>(…): IPromise<U>; <U_1>(…): IPromise<U_1>; … }` — the by-identity
cache again, two distinct `U` params in one print). Head:
`underscoreTest1` 100, `typeParametersAreIdenticalToThemselves` 25.

**Design.** `render_object_type` is the one join both compose sites feed;
the pass walks members in order, parses each signature member's declared
`<…>` list *from this renderer's own output* (a format this crate controls),
renames collisions against the names earlier members claimed — same
`apply_renames`, same first-free `X_n` — and claims the final names.
Signature members only; property members holding function types are §19's
print-time territory.

**Bar:** net ≥ **+200** (~62% of 322; the underscore head is one case at
31%); own ≤ **20** — falsifier: wrong lines whose want keeps BOTH members'
`U` → the two members share one written declaration (a merged or aliased
signature printed twice), identity not collision — check before widening;
regressed == 0; lost == 0.

### §20.1 REFUSED and reverted — the discriminator is the print CONTEXT, and no join can see it

Two placements, both measured, both reverted:

1. **The shared join** (`render_object_type`): **149 RIGHT→WRONG.** The
   corpus pins that a WRITTEN composite keeps its sibling names verbatim —
   `declarationEmitTypeParameterNameReusedInOverloads` records
   `{ new <T extends Derived>(a: T): T; new <T extends Base>(a: T): T; }`,
   both `T` — the node-reuse family (`tsr-5o2`) at the composite scale.
2. **The synthesized joins only** (the overloaded-function arm in
   `symbols.rs`, the instantiated re-render in `inference.rs`): **net −645.**
   Written overload lists printed through `typeof`-expansion and instantiated
   members ALSO keep written names in the corpus's majority.

So the `<U>; <U_1>` population (322 lines) is discriminated by something no
join can read off member text lists: upstream's builder renames only where
the surrounding **print context** has already claimed the name by identity —
the same per-print naming context §19 approximated positionally for the
enclosing-scope half, but here the claimant is inside the composite churn
itself (the `IPromise` head's `then` overloads rename; the construct-pair
does not; both are written sibling overloads). **Finding the true
discriminator needs a per-line study of the two head families side by side
before any third placement is attempted** — filed as the §20 residue, and
the two measured placements are what the next attempt must not repeat.

### §21 The overload-failure signature — the baseline's `never` is an intersection of returns

`controlFlowIterationErrors`' `foo(x) : never` (§12.6's priced residue)
decodes in `createUnionOfSignaturesForOverloadFailure`
(`internal/checker/checker.go:9581`): when overload resolution fails,
upstream does not gap the call — it answers with a synthetic signature whose
**return type is `getIntersectionType` over every candidate's return**
(`checker.go:9620`), so `(string)=>number | (number)=>string` failing on a
union argument answers `number & string = never`, matching the original
TypeScript baseline verbatim (`_submodules/.../controlFlowIterationErrors.types:138`).
The narrowed `x : never` after `x = foo(x)` then falls out of ordinary
assignment narrowing.

**Scope**: only `choose_overload`'s `None if arity_matched` exit — the case
`overload_funnel` prices at **45 sites** where every arity-matching
candidate was **decidably** `NotRelated` (an `Unknown` pair already returns
earlier, so decidability is structural, not a new gate). The arity-mismatch
exit (12 sites) stays a gap: upstream routes it through
`pickLongestCandidateSignature`, a different mechanism, unsized.

**Bar**: the `foo(x) : never` family flips (`controlFlowIterationErrors`,
`Async` variant — 16 lines) plus whatever the other 40-odd sites carry; own
wrong ≤ 10. Falsifiers: (a) a flipped-to-wrong line whose want is a real
type, not `never`/an intersection — means upstream selected where we
refused, i.e. our relation rejected a pair upstream accepts, and the arm is
answering from a wrong premise; (b) regression in the §17/§18 selected
population — the new exit must not disturb selection.

**§21 score — LANDED, after the bar's falsifier (a) fired and was honoured
by narrowing.** First measurement: +83 / 9 GAP→WRONG, all nine
`overloadResolution` `fn1(undefined)` under `@strict: false` — `undefined`
inhabits every type non-strict, so upstream matched BOTH overloads and our
relation's missing non-strict arm mis-read an ambiguity as a failure. The
narrowed premise (a pure-`undefined`/`null` argument rejects no candidate;
such calls fall to the ambiguous exit and stay gaps): **+77 (76 GAP→RIGHT,
1 WRONG→RIGHT), ZERO adverse transitions.** `checker_types` 74.62% →
**74.64%**, cases 3,064 → **3,069**. The twenty-second stand-in fixture came
due (`overload_resolution.rs`: `p(true)` is `never`, not `error`).

### §199 §21's arity exit was a scope that upstream does not have (+3 cases, 0 lost)

§21 above gated the intersection arm on `None if arity_matched` and recorded
the reason: *"The arity-mismatch exit (12 sites) stays a gap: upstream routes
it through `pickLongestCandidateSignature`, a different mechanism, unsized."*

**It does not.** `getCandidateForOverloadFailure` (`checker.go:9498`) is
reached from `chooseOverload` failing, full stop — it never asks *why* it
failed. Its own branch is on the **candidate set**, not on the failure mode:

```go
if hasCandidatesOutArray || len(candidates) == 1 || core.Some(candidates, hasTypeParameters) {
    return c.pickLongestCandidateSignature(...)
}
return c.createUnionOfSignaturesForOverloadFailure(candidates)
```

So `pickLongestCandidateSignature` is what a *single* or *generic* candidate
set gets under **either** failure mode, and the intersection is what a plural
non-generic set gets under either. Arity does not enter it. `foo()` against
`(bar: string): string` and `(bar: number): number` — where no candidate takes
zero arguments — is `string & number = never` in
`compiler/functionOverloads29.types`.

Dropping the `&& arity_matched` is the whole change. This port already
returns `None` before this point when any candidate is generic, so the
`pickLongestCandidateSignature` half of upstream's branch stays unported and
stays a gap, exactly as before; only the plural non-generic set is affected,
which is the set §21 already served.

**+3 cases** (`functionOverloads29/34/37`), **+9 lines, 0 lost**.

This is the fifth instance this window of a *stated scope wider than the
measurement behind it* — see `docs/conventions.md` corollary 8. The
distinguishing mark here is that the restriction was written as a fact about
upstream (*"upstream routes it through a different mechanism"*) rather than as
a fact about the measurement, and it was the fact about upstream that was
wrong.

**Three tests**: the arity failure, a control that the intersection is a real
intersection rather than a `never` shortcut (two candidates returning `A & B`
answer `A & B`), and an over-fire control that a call which *resolves* still
answers its own candidate's return.

### §22 Optional call chains — the callee strips, the result re-unions

`check_call_expression` refused every `?.` call outright
(`COUNTERS.optional_chain`, the §17-family residue in
`controlFlowOptionalChain`: `f?.(x)` wants `boolean | undefined`).
Upstream's `checkCallChain`: the callee passes through
`getOptionalExpressionType` (a chain root strips nullable, an inner link
strips the propagated marker), resolution runs on the non-nullable
remainder, and `propagateOptionalTypeMarker` unions `undefined` back when
anything was stripped — the same three functions the property-access chain
already uses (`members.rs`). The port is those calls at the callee
boundary; error results stay errors (no marker on a gap).

**The bar**: the `f?.(x)`/`o?.(...)` family flips. Falsifiers: (a) a
non-chain call regressing means the strip ran where no `?.` exists; (b) a
chain whose want has no `| undefined` (already-non-nullable callee) means
the marker fired without a strip.

**§22 score — LANDED.** **+114 GAP→RIGHT / 23 GAP→WRONG.** The 23 split in
two named shapes, neither the arm's own: `boolean | undefined` against a
`boolean` want where the callee is a closure reference this port does not
flow-narrow (the §13 residue — upstream narrows `f` non-nullable before
the chain question, so no marker fires), and `deleteChain`'s inner-link
markers (the propagated-marker mechanics between chain links, pre-existing
in the property-access half). Both were GAPs that became answers whose
wrongness is owned upstream of the arm. `checker_types` right 369,426 →
**369,540**.

### §22.1 REFUSED: the stripped-callee print is not a uniform rule

A one-line re-cache printing chain callees at their stripped type flipped
`controlFlowOptionalChain`'s six `f : fn` lines right and **59
`callChain.3` lines wrong** — chain-LINK callees (`a?.b()`) print WITH
their undefined half in the baselines while some chain-ROOT identifier
callees print without. The discriminator is a print-context question this
measurement does not decide; reverted whole. The six lines stay wrong
with this number recorded against the next attempt.

### §23 The untyped-call gate learns the §31/§32 provenance

`t.m(1)` with `t: Unresolved` errors while `t.m` answers `any`: the
untyped-call arm's written-annotation gate (its own §-history's 248-line
lesson) predates §31/§32, under which an `any` from a truly-unresolved
name or a minted-unresolved receiver IS upstream's claim. Admission
extends to: a callee that is a property/element access whose RECEIVER's
type is the `any` intrinsic or a minted unresolved — `any.m` is `any` in
both compilers, and a call through it is an untyped call. Falsifier: the
248-line lesson's population (unported-mechanism `any`s) must not re-enter
— the receiver test reaches only the §31/§32 provenances and written-any
receivers, never an `any` this port computed FOR the callee itself.

**§23 score — LANDED.** **+1,424 (1,400 G→R, 24 W→R) / 452 GAP→WRONG,
zero RIGHT losses** after the fired leg (an `any` RECEIVER admits only
outside JS files; minted-unresolved receivers admit everywhere). The 452
are receiver-`any` positions where upstream's machinery types what this
port's cannot — the same accepted class as §31/§32's residues, headed by
lib-method chains (`arrayconcat`). `checker_types` right 384,826 →
**386,250 (80.65%)**.

### §24 Two residue arms: unresolved-identifier callees and `super()`

The mountain's call residue: (1) `hasFlag(...)` — a §31-provenance
IDENTIFIER callee (the §23 extension covered only access-expression
callees); the admission re-runs §31's own test (resolves nowhere, in an
import-free file). (2) `super(...)` answers `void`
(`checkCallExpression`, `checker.go:8331` — the SuperKeyword arm).
Falsifiers: (1) inherits §23's; (2) has none — the rule has no
alternative.

**§24 score — LANDED.** **+856 (845 G→R, 11 W→R) / 162 (154 G→W, 8 R→W —
the eight all `parsingDeepParenthensizedExpression`, the standing JS
case).** The 154 head at super-call diagnostics cases and JS
export-assignment shapes, the accepted residue classes. `checker_types`
right 386,250 → **387,098 (80.82%)**.

### §25 `new` through §31-provenance callees answers `any`

The chain's sixth hop: `new Unresolved()` — upstream's callee is TS2304's
`errorType`, and construction through it answers the same, printed `any`.
The admission mirrors §24's identifier test and §23's receiver test at the
NEW-expression entry; the same file gates apply. Falsifier: inherited from
the chain.

**§25 score — LANDED.** **+680 (676 G→R, 4 W→R) / 7 GAP→WRONG** — near
clean; the seven are augmentation/decorator shapes. `checker_types` right
387,098 → **387,778 (80.96%)**.

### §26 The unique-symbol mint

The 291-line priced row (`uniqueSymbols` et al.): `Symbol()` in a valid
`unique symbol` declaration position answers a FRESH `unique symbol` type
(`getESSymbolLikeTypeForNode`, `checker.go:22982`) — one distinct type per
site (`new_named` per call, printing `unique symbol`), which is exactly
what the existing positional gate detected and refused to fake as
`symbol`. The gate's error becomes the mint. Falsifier: `typeof s`
positions print `typeof s` — node-reuse territory; if those dominate the
adverse, the mint narrows to declaration lines only.

**§26 score — LANDED.** **+377 GAP→RIGHT / 19 GAP→WRONG** (the nineteen
are `typeof s`-reuse and computed-property positions, the falsifier's
named family, small enough to stand). `checker_types` right 387,778 →
**388,155 (81.04%)**.

### §27 The written `unique symbol` type node

The §26 residue's big half (355 want-`unique symbol` gaps): the WRITTEN
form — `declare const s: unique symbol` is a `TypeOperatorNode` with
`UniqueKeyword` (`getTypeFromTypeOperatorNode`'s ESSymbol arm,
`checker.go:22960`) — mints per declaration, memoized by NODE id so one
declaration is one type. Falsifier: none beyond the measure — the
declaration-position validity question was §26's and stands.

**§27 score — LANDED.** **+1,182 GAP→RIGHT / 129 GAP→WRONG** (the 129 head
at cross-file unique-symbol re-exports and intersection-reduction shapes —
per-node minting cannot see that two files' nodes name one symbol, the
recorded residue). `checker_types` right 389,825 → **391,007 (81.64%)**.

### §28 `this` in type position — the print half, with the call gate

`complexRecursiveCollections`' 612 lines head at members typed `(...) =>
this`: the `ThisTypeNode` has no type-node arm, so every such member
errors. The slice: one `this` type per enclosing class/interface
declaration (minted, printing `this` — upstream's `thisType` identity per
declaration), which makes the MEMBER lines print; call RESULTS through a
`this`-returning signature are gated to gaps — upstream instantiates
`this` to the receiver there (`getThisTypeArgument` road, unported), and
answering the literal `this` at a call result would be a wrong line.
Falsifier: (a) receivers' member lines whose want instantiates `this`
(generic reference positions) — counted; (b) the call gate must not
disturb non-`this` signatures.

**§28 score — LANDED at the second variant.** V1 (classes+interfaces, with
the call gate) read +467/241 including 28 R→G — the gate broke calls that
answered through the written-return road, and the class arm shadowed the
existing class-`this` machinery. V2 — INTERFACES only, no call gate:
**+343 (341 G→R, 2 W→R) / 122 GAP→WRONG, zero RIGHT losses.** The 122
head at lib `concat`-style signatures whose call results print the
literal `this` where upstream instantiates to the receiver — the priced
receiver-instantiation residue. `checker_types` right 391,007 →
**391,350 (81.71%)**.

### §29 `this`-typed call results answer the receiver

The §28 residue's rule (`getThisTypeArgument`): a call whose resolved
return is the interface-`this` mint answers the RECEIVER's type —
`[a, b].sort(...)` is the array's own type. Scope: property-access
callees only (the receiver is at hand); other callee shapes keep the mint.
Falsifier: §28-v1's 28 R→G population — answering the receiver must not
regress the written-return road it broke by erroring.

**§29 score — LANDED.** **+66 (35 G→R, 31 W→R) / 4 GAP→WRONG**
(`fluentInterfaces`' derived-through-base chains — the receiver answer is
the BASE-typed receiver where upstream keeps the derived; the polymorphic
half, recorded). `checker_types` right 394,770 → **394,836 (82.44%)**.

### §30 `new` consults the untyped-call gate

`duplicateLocalVariable1` (241 GAP want-`any`): `new FileManager.FileBuffer(f)`
where `declare var FileManager: any` — upstream's `resolveNewExpression`
reaches the SAME `resolveUntypedCall` the call arm does (`checker.go:8490`,
`IsTypeAny(expressionType)` → `anyType`), but this port's
`check_new_expression` never consults `is_untyped_call_target`, so an
any-callee construction gapped to `error` unless it matched §25's narrow
identifier/§31 shape.

**The bar.** In `check_new_expression`, after checking the callee, run the
existing `is_untyped_call_target` (written-annotation / §31-identifier /
any-receiver arms, all its calibrated narrowings inherited) and answer
`any` on a hit. §25's arm is subsumed but left in place — it fires first
and identically.

**Falsifiers.** (a) The gate's own fired legs recur here — if construction
sites systematically differ from call sites (e.g. `new` through evolved
anys upstream types via construct signatures), G→W appears — narrow with a
construction-specific exclusion. (b) If JS AMD shapes construct through
any-receivers, the in-JS exclusion already declines them; R→ anything on
JS cases says the exclusion is mis-scoped for `new`.

**§30 score — LANDED (narrowed once).** First pair **+282 G→R / 5 G→W**
(`classBlockScoping`): falsifier (a) fired in a specific costume — `new
Foo()` INSIDE `Foo = class Foo {...}` resolves to the class-expression's
own name upstream (the class name is in scope in its body), while this
port's resolver reaches the outer `let Foo: any`, a written annotation.
The narrowing is scoping-shaped, not construction-shaped: the gate now
declines an identifier callee lexically inside a class declaration or
class expression bearing that name — the resolver's miss contained at the
gate. A second leg fired in the same pair: `new.targ`
(`misspelledNewMetaProperty`) — the parser's recovery mints an EMPTY
identifier receiver, §31 answers the empty name `any`, and the receiver
arm read that as a source `any`; a missing-receiver decline contains it
(costing 3 lines that had ridden through empty receivers, back to GAP).
Final pair: **+279 (G→R), ZERO adverse.** `checker_types` right 395,803 →
**396,082 (82.70%)**.

### §31 The unresolvable `require()` alias reads `any`

`privacyImportParseErrors` (240 GAP want-`any`): `import m1_im3_private =
require("m1_M3_public")` INSIDE a namespace. Upstream never resolves it —
an import declaration in a non-ambient namespace is TS1147 territory and
`resolveExternalModule` is never reached — so the alias's target is the
unresolved symbol and every read prints `any`. Same rendering when the
import is legally positioned but the module is genuinely unfindable
(TS2307, `badExternalModuleReference`). This port answered `errorType`
for both.

**The bar.** In `get_type_of_alias`, when the alias's declaration is
`import x = require("...")`: (1) not positioned for resolution
(`external_import_is_positioned_for_resolution` false) → `any`; (2)
positioned, and the TS2307 predicate's every decline-gate passes (no
ambient module of that name, no pattern ambient module, not a Node core
name, not `@types/`, host consulted and found nothing) → `any` — the
predicate factored out of `check_module_specifier` so the diagnostic and
the type read ONE calibrated answer. Everything else keeps today's
`errorType` gap (a resolvable module this port cannot type is the port's
gap, not upstream's `any`).

**Falsifiers.** (a) If a namespace-positioned import in the corpus IS
resolved by upstream (ambient-module context this test misses), G→W
appears on its reads — narrow with the ambient-module test on the
enclosing chain. (b) If the host's "found nothing" diverges from
upstream's resolver, the §23-era 15-wrong-line class recurs — the
`module_resolution_found` split already guards it; R→W here says it
doesn't.

**§31 score — LANDED (narrowed once).** First pair +390 G→R / 3 G→W +
1 R→W (`privacyGloImportParseErrors`): falsifier (a) fired EXACTLY —
upstream resolves the require() against quoted ambient modules from
inside a namespace; TS1147 is a grammar error, not a resolution bar. The
position disjunct was DROPPED — findability alone decides. Final pair:
**+390 (G→R), ZERO adverse.** `checker_types` right 396,082 →
**396,472 (82.78%)**.

### §32 Index-signature prints admit written non-union keys

`indexSignatures1` (100+ GAP): the printer's key gate declined everything
but the `string`/`number` intrinsics — tagged keys (`string & Tag1`),
template-literal keys, alias-named keys all gapped the WHOLE object
print. The union decline is upstream-faithful (`getIndexInfosOfIndexSymbol`
SPLITS `[k: A | B]` into two infos, so printing one union-keyed signature
would be confidently wrong); the rest of the gate was caution, not
fidelity: a computable non-union key prints exactly as written.

**The bar.** `index_signature_member` admits any key whose type computes
(`!= error`) and is not a UNION; the LOOKUP gate (`index_info_of`) stays
intrinsic-only — a print the lookup cannot serve yields a gapped access
(honest), never a wrong one.

**Falsifiers.** (a) If upstream normalizes written key aliases at print
(`TaggedString1` → its expansion or vice versa), W appears on alias-keyed
prints — decline aliases. (b) If the split rule extends beyond unions
(e.g. `string & anything` also splits), W on intersection keys.

**§32 score — LANDED.** **+53 (G→R), ZERO adverse.** (A draft guessed +96
before the run; corrected to the measured +53.) `checker_types` right
396,500 → **396,553 (82.80%)**. Residue in the case: union-keyed splits,
`[sym]` computed members, and key types the port cannot compute
(template-literal types among them).

### §33 The `const` type-parameter modifier prints

`typeParameterConstModifiers` (~40 GAP): `type_parameter_of` declined ANY
modifier, gapping the whole signature for `<const T>(x: T) => T`.
Upstream prints the modifier as written
(`typeParameterToDeclarationWithConstraint` carries the declaration's
modifiers) and `const` changes INFERENCE (literal retention,
`checker.go`'s `InferenceFlagsNoDefault`-adjacent const handling), not
the signature's shape.

**The bar.** Admit exactly the `const` modifier (`in`/`out` variance
stays declined — variance is checked machinery, not print baggage), carry
`is_const` on `TypeParameter`, and print `const ` before the name at all
three signature print sites. Inference is NOT taught literal retention in
this build; calls through const-generic signatures answer whatever the
existing inference answers.

**Falsifiers.** (a) If existing inference WIDENS where const demands
literal retention, call-result lines go G→W (want `"a"`, print `string`)
— if the adverse count crosses the win, gate CALLS through const-marked
signatures to decline while keeping the print. (b) If `in`/`out` appear
beside `const` in the corpus, those still gap whole — measured residue,
not a leg.

**§33 score — LANDED (narrowed twice).** First pair +149 G→R / 70 G→W:
falsifier (a) fired — the port's inference widens where `const` retains
literals — so calls through const-marked signatures DECLINE while the
signature prints. Second pair +144/10: the residue was two shapes this
admission newly exposed — the alias-name print (`>T2 : T2`, contained by
minting the alias's own name for const-carrying bodies) and the §19/§20
`_1` rename refusal reappearing in overload prints (contained by declining
a many-signature print whose members reuse a type-parameter name while any
carries `const` — a gap beats a wrong, costing 10 potential wins). Final:
**+134 (G→R), ZERO adverse.** `checker_types` right 396,553 →
**396,687 (82.83%)**. Residue: const inference (literal retention,
readonly-tuple minting) — the real subsystem behind the declined calls.

### §34 Deferred indexed-access type nodes print as written

`correlatedUnions` (100+ GAP): `RecordMap[P]` in an annotation, where `P`
is a TYPE PARAMETER — upstream DEFERS the indexed access (the index
cannot resolve until instantiation) and the baseline prints the node as
written, in annotations and inside signature prints alike
(`(v: RecordMap[P]) => void`). This port's `get_type_from_type_node` has
no `IndexedAccessTypeNode` arm — `errorType`, poisoning every signature
that mentions one.

**The bar.** The §31-mint pattern (`unresolved_type_reference`): when the
INDEX is a type reference resolving to a TYPE PARAMETER (upstream's
deferral condition, approximated) and the OBJECT is a spellable type
reference, mint a Named carrying `Object[Index]` as written, registered
in `unresolved_types` so `is_error` stays true — every consumer keeps
gapping; only the printed line changes. Literal indexes (`Funcs["cat"]`)
stay declined: upstream resolves those CONCRETELY, and the written text
would be a wrong line.

**Falsifiers.** (a) Call-site instantiations where upstream substitutes
`P` and prints the resolved type — the mint leaks the written form; R→W/
G→W at call results. (b) If upstream normalizes the spelling (spacing,
qualifier changes), W on exotic spellings.

**§34 score — LANDED (narrowed once).** First pair +154/29: a variant of
(b) fired — upstream EXPANDS a type-ALIAS object in the deferred print
(`ArgMap[P]` wants `{ sum: ...; concat: ... }[P]`,
`mappedTypeIndexedAccessConstraint`); interfaces keep their name. Alias
objects now decline. Final pair: **+121 G→R / 2 G→W** — the 2 are
falsifier (a)'s population (constraint substitution under instantiation,
`deeplyNestedConstraints`), accepted at 60:1 per the §23/§6 precedent.
`checker_types` right 396,694 → **396,815 (82.85%)**.

### §35 Deferred `keyof` prints as written

270 want-`keyof ...` lines; the head (~110) is `keyof T` over a TYPE
PARAMETER — deferred upstream exactly as §34's indexed access, printed as
written. Same mint, same registration (`unresolved_types`), same
consequence: only the printed line changes. Concrete operands (`keyof
Interface`) resolve to key-literal unions upstream and stay DECLINED —
answering the written text there would be a wrong line. Composites
(`keyof T | keyof U`) ride the union road only if it composes ANY-flagged
mints without collapsing; not this build's claim.

**Falsifiers.** (a) Instantiation sites substituting T — the §34 leg,
same 60:1 tolerance. (b) If baselines spell the operand differently than
written (qualified vs bare), W on those.

**§35 score — LANDED (narrowed twice).** First pair +312/47: falsifier
(a) fired through a side door — generic calls whose type-parameter
CONSTRAINTS mention the mint stopped gapping and inferred without
literal retention (`pick(['b'], …)` inferred `string` where upstream's
keyof-constrained parameter keeps `"b"`); such calls now decline as they
always had. A parenthesization bug surfaced en route: `keyof T` under
`[]` printed `keyof T[]` — the array-element wrapper learned the `keyof `
prefix (`(keyof T)[]`). Final: **+324 G→R / 22 G→W** (15:1, accepted per
the §23/§34 precedent — residue: written `Array<U>` spellings, an
`import("lodash")` type, narrowing corners). `checker_types` right
396,815 → **397,139 (82.91%)**.

### §35.1 Concrete `keyof` over a plain interface

The resolvable half: `keyof I` where `I` is a non-generic, heritage-free
interface whose members are all plainly named — upstream's
`getIndexType` answers the union of key-literal types in DECLARATION
order (the binder's member table is unordered here, so the DECLARATION's
member list is the source). Method and property names both contribute;
numeric-looking names contribute NUMBER literals (upstream keys `{ 0: a }`
with `0`, not `"0"`); empty interfaces answer `never`. Everything exotic
declines whole: heritage (inherited keys), generics, computed/index/call
members, classes (modifier filtering), type-literal operands (reachable
but unmeasured).

**Falsifiers.** (a) Quoted names — if upstream prints `"a-b"` differently
than the literal mint, W. (b) Union ORDER — if upstream's index type
orders differently than declaration order, W on multi-key interfaces.

**§35.1 REFUSED at +3/32.** Neither named falsifier — the concrete union
UNLOCKED downstream consumers (contextual JSX attributes, mapped-type
machinery) that answer through it wrongly, the §6.1-class failure mode:
the expression arm's value is captured almost entirely by consumers this
port hasn't built, and the honest keyof union feeds them confident
wrongs. Reverted whole. The road into concrete `keyof` is those
consumers (mapped types, JSX attribute checking), not the operator arm.

### §36 Template-literal type nodes print as written

`` `data${string}` `` in type position: upstream builds a
`TemplateLiteralType` and the baseline prints it as written (heads,
spans, hole types in order). This port had no arm — `errorType`,
gapping §32's template-keyed index-signature prints among others. The §31
mint again: spell the node from its parts (head text, each span's hole
type rendered by `type_to_string`, middle/tail texts), register in
`unresolved_types` — is_error stays true, consumers keep gapping, only
prints change. A hole whose type errors declines whole.

**Falsifiers.** (a) Upstream NORMALIZES some template types (e.g.
collapses `${string}` chains or resolves all-literal holes to a plain
literal — `` `a${"b"}` `` is `"ab"` upstream): wrongs on those spell the
narrowing (decline literal-typed holes). (b) The §35.1 consumer-unlock
class — measured by the pair.

**§36 score — LANDED (narrowed three times).** The pair fired repeatedly
and each narrowing was POSITIONAL, not content-based — the discovery of
the build: upstream's node builder REUSES written annotation nodes, so
references print as written in ANNOTATIONS whatever they evaluate to,
while ALIAS-DECLARED positions show the EVALUATION. Three legs: (1)
intrinsic string mappings (`Uppercase<…>`) decline in alias-declared
positions unconditionally (upstream evaluates even `Uppercase<Uppercase<
string>>` and distributes over patterns); (2) conditional-bodied and
`intrinsic`-bodied alias references likewise (`PrefixData<P>` evaluates
even through a type-parameter argument); (3) the template mint carries
OBJECT flags, not the §31 mints' ANY — a template in a union must not
trip any-absorption (`"bar" | \`foo-${string}\`` keeps both). Final
matrix vs the §35 baseline: **+475 G→R, 337 W→G, 118 G→W, 22 R→G — net
right +453, net wrong −219.** `checker_types` right 397,139 →
**397,592 (83.01%)**. Residue named: template-bodied ALIASES print their
expansion where annotation positions want the alias NAME
(`templateLiteralIntersection2` wants `(p: JoinedPath) => void`) — the
site-sensitive §41-class alias-name question, queued; and
`discriminatedUnionTypes4`'s narrowing through pattern literals.

### §36.1 Template-bodied aliases keep their NAME in annotations

§36's named residue: `type JoinedPath = `-template — its OWN line wants
the template (the §36 mint answers it), but ANNOTATION positions want the
alias NAME (`(p: JoinedPath) => void`, `templateLiteralIntersection2`) —
the same annotation-reuse rule §36 discovered, applied to the alias
reference itself. Narrow: non-generic alias, body is a
TemplateLiteralTypeNode, position is NOT alias-declared → mint
Named(alias name), `unresolved_types`-registered (opaque, gap-preserving,
print-only). Everything else untouched.

**Falsifier.** Annotation positions where upstream expands anyway
(exported declarations under `@declaration`?) — W on those.

**§36.1 v1 REVERTED, v2 LANDED.** The type-level mint (v1) measured
+18 W→R against 52 R→W: the split is FINER than type identity — the
parameter's OWN assertion line wants the EXPANSION while the enclosing
signature print wants the written name, two spellings of one node's type.
That is `Parameter::written_text`'s seam exactly, so v2 is a PRINTER
rule: `written_annotation_text` gains a leg for a bare alias name whose
target is a template-bodied alias (exactly the §36-created divergence,
nothing wider — the union leg's measured negative stands). **v2 score:
+12 (W→R), ZERO adverse.** `checker_types` right 397,592 → **397,604
(83.01%)**.

### §37 `arguments` binds `IArguments`

`capturedLetConstInLoop2` (20 direct + ~150 downstream GAP): upstream
binds `arguments` to the global `IArguments` interface in every function
(`checker.go`'s argumentsSymbol); this port kept it an honest gap from
the §31 era, when no lib existed to resolve it. The conformance program
MOUNTS the bundled libs (the §36 chain surfaced this: `Uppercase`
resolves), so `IArguments` is resolvable now: an `arguments` read inside
a function-like container answers the global interface's declared type —
`.length` then projects `number` through the ordinary member road, and
the captured closures (`() => x + a`) type downstream.

**Falsifiers.** (a) Arrow functions — upstream's `arguments` in an arrow
binds the ENCLOSING function's (or errors at top level); if the corpus
distinguishes, W in arrow cases. (b) A local named `arguments` shadows —
the resolve-first order already guards it (the §31 exit only runs when
resolution missed).

**§37 score — LANDED (narrowed once).** First pair +641/66: falsifier (a)
fired exactly — arrows at top level and CLASS FIELD/STATIC BLOCK
initializers want `any` (upstream errors there and the §31 rendering
applies). The container walk now stops at the nearest NON-arrow function
and declines at PropertyDeclaration/ClassStaticBlock/SourceFile. Final:
**+637 G→R / 28 G→W + 1 R→W** (22:1) — the adverse are a PRE-EXISTING
for-of-element inference class (`for (let x of []) … () => x + a` wants
`string`, this port answers `any`) newly unlocked downstream, plus one
`{ arguments }`-shorthand corner. `checker_types` right 397,604 →
**398,240 (83.15%)**.

### §38 for-of bindings take the iterated element

`capturedLetConstInLoop2`/`_ES6` (~500 GAP, plus §37's 24 exposed
wrongs): `for (let x of arr)` — upstream's
`getTypeForVariableLikeDeclaration` routes a for-of binding through
`checkRightHandSideOfForOf` → the iterated element type
(`checker.go:16700` region). The slice: the RHS's checked type is an
`Array`/`ReadonlyArray` reference → its argument; a tuple → the union of
its elements; the `string` intrinsic or a string literal → `string`
(String iteration). Every other RHS (`Iterable<T>`, generators, unions,
`[]`'s `never[]`… ) answers None — the implicit-any road unchanged.

**Falsifiers.** (a) `--downlevelIteration`/target-sensitive differences
in string iteration — W on string-RHS cases. (b) Freshness: if upstream
widens the element differently at the binding (literal RHS elements), W
— widen through `get_widened_literal_type` and re-measure.

**§38 score — LANDED (narrowed twice).** Two legs fired on the way, both
EMPTY-ARRAY spellings: `for await` declines (the awaited element is
unported and the non-async error renders `any`), and the `[]` RHS
declines in BOTH its spellings — `never[]` strict, `undefined[]`
non-strict (`array_literals.rs:122`) — upstream's binding reads `any`
there. Final: **+224 W→R, 24 W→G, ZERO adverse.** `checker_types` right
398,240 → **398,464 (83.19%)**. Residue: `Iterable<T>`/generator RHS
(decline), `for await`, and the §37-exposed for-of closures now typing
through this arm.

**§38.1 score — LANDED.** For-IN bindings answer `string`
unconditionally (`checker.go:16698`): **+565 W→R + 1 G→R / 3 R→W + 3 R→G
(94:1)** — the adverse are self-referential declarations (`for (const v
in v)` errors to `any` upstream, `recursiveLetConst`) and a parse-error
corner (`parserForOfStatement19`), accepted. `checker_types` right
398,464 → **399,030 (83.31%)**. A §38.2 variant (Iterable/
IterableIterator RHS) measured ZERO — no decidable population reaches
for-of through those references today — and was reverted rather than
kept as unproven breadth.

### §39 Named-union constituents keep an ORIGIN print

`arithmeticOperatorWithEnumUnion` (161), plus every `E | undefined`
option-type in the corpus: upstream's `origin` denormalisation
(`checker.go:25705`) prints a union that CONTAINS a named union
unexpanded — `E | F`, `E | string` — while the type's constituent list
stays the flattened, sorted members. This port's worker declined those
whole (the guard was about the printed line). The port equivalent of
`origin`: the worker now builds the union with the flattened set AND a
TEXT OVERRIDE — the ORIGINAL input list's prints in input (written)
order, deduped — so consumers see members and the line prints the origin.

**Falsifiers.** (a) Upstream's origin print order diverges from written
order somewhere — W on those spellings. (b) Reduction visibility: if a
literal in the written list is absorbed by a base in ANOTHER constituent
(`E | number`?) upstream's origin may still print both — mismatches
counted by the pair.

**§39 REFUSED at ~2:1 after three refinements — the text hack is not
origin.** Variants measured: (v1) input-order origin text +277/153;
(v2) node-road-only origin + full-set enum reconstruction for computed
unions +246/125; (v3) + no-reduction gate, no-mint gate, and SORTED
origin members (`numberAssignableToEnumInsideUnion` wants `boolean | E`
for the written `E | boolean`) +238/129. The stable adverse core:
`temporal`'s 82 (named-union prints inside signatures whose alias
spellings are site-sensitive — the §41-class refusal reached through a
new door) plus filtered-union subsets (upstream's origin SURVIVES
`filterType` and subsets member-wise — `string[] | Color` stays named
after a narrowing filter; a creation-time text cannot). The mechanism
upstream actually has is an ORIGIN TYPE carried on the union and
propagated through filter/map operations, printed by re-formatting at
print time. That is a store reshape (`TypeData::Union` gains an origin
list; `filterType`/narrowing filters learn to subset it), not a text
override. REFUSED until that reshape; the +246 winnable lines
(`arithmeticOperatorWithEnumUnion` 161 at the head) are its bounty.

### §31.1 ES-import findability REFUSED at 2.7:1

The §31 rule extended to ES-import forms (import clause, specifier,
namespace) measured +110/117 unrestricted and +35/13 gated to relative
specifiers — the adverse is resolver-parity, not rule shape: upstream
resolves symlinked workspaces (`symlinkedWorkspace*`, every unrestricted
miss), `// @link` symlinks and path mappings
(`moduleResolutionWithSymlinks_notInNodeModules`,
`pathMappingBasedModuleResolution6_node` under the relative gate), where
the host's `module_resolution_found` sees nothing. The predicate is
calibrated for the TS2307 emitter's conservative silence, and the
type-side admission inverts the failure cost: a diagnostic not emitted
is silence, an `any` minted where upstream resolves is a wrong line per
use. REFUSED until the host learns symlink/path-mapping resolution;
the ~2,500-line no-value-decl ALIAS rows stay with `bd tsr-9or.1`.
---

## §583 — the generator NEXT slot: a kind list where upstream has a nil test (+5 lines, 0 adverse)

`return_type_from_body`'s generator arm declines the WHOLE signature when a
`yield` sits in a "contextual" position, where contextual is a **list of parent
kinds** (§223, which had already widened it once from §135's allowlist of two).

The list is wrong in a way neither earlier pass caught, and the fixture that
shows it is three lines long:

```ts
function* g() { class C { x = yield 0 } }   // generatorTypeCheck57
```

wants `() => Generator<number, void, unknown>`; this port printed `any`.

### The confusion

§223 built the list as *the complement of the kinds absent from
`getContextualType`'s switch* (`checker.go:29354`) — correct as far as it goes.
But the NEXT slot does not ask *"does this kind have an arm"*, it asks **what
the arm ANSWERS**: `checkAndAggregateYieldOperandTypes` (`:20334`) sets
`nextType = getContextualType(yieldExpr)` and appends **only if non-nil**, and a
`nextTypes` that stays empty becomes `unknownType` (`:20242`–`:20245`).

The variable-like arms (`:29356`) all route to
`getContextualTypeForInitializerExpression` (`:29423`), which returns nil
whenever the declaration carries no annotation. So an unannotated `x = yield 0`
has **no** contextual type, the NEXT slot is `unknown`, and there was never
anything to decline. A kind having an arm and that arm answering are two
different claims, and the list conflated them.

`initializer_position_is_contextual` (`signatures.rs`) is the nil test
transcribed: not the initializer (`:29426`) → nil; annotated (`:29440`) →
contextual; binding-pattern name (`:29431`) → contextual; STATIC property
(`:29448` → `:29612`) → contextual only when the class is an **expression**,
which is why `class C { static x = yield 0 }` as a declaration statement
(`generatorTypeCheck58`) also wants `unknown`.

### The gate the measurement added

The first build applied the refinement everywhere and scored **+5 with one
adverse**: `generatorYieldContextualType`'s `f1<0, 0, 1>(function* () { const a
= yield 0 })` went GAP→WRONG. A generator **expression** can carry a contextual
SIGNATURE, and then the NEXT slot comes from `getContextualIterationType`
rather than from the yield's own position — the nil analysis above is about
`getContextualTypeForInitializerExpression` and simply does not cover that
road. Restricted to the DECLARATION arm — the arm §223's safety argument was
made in — the same change measures:

```
TOTAL 474196  right 433795  gap 8481  wrong 31920
GAP->RIGHT: 2    compiler/generatorES6_6, conformance/templateStringInYieldKeyword
WRONG->RIGHT: 3  generatorImplicitAny, generatorTypeCheck57, generatorTypeCheck58
(no adverse transition of any kind)
```

**The adverse was found by the full run, not by the reasoning.** The filtered
run over the four cases the arm was written for was clean; the corpus named the
case the argument had not covered. That is the §12.2 discipline paying out
exactly as advertised, and it is why a filtered score may iterate but never
land.

### Still declined, and still correctly

`castOfYield` wants `Generator<number, void, number>` — a NEXT slot genuinely
fed by a contextual position (`<number>(yield 0)`, a `TypeAssertionExpression`,
whose arm really does answer). It stays a gap. This change converts the
positions where upstream answers **nil**; it does not port the positions where
upstream answers a **type**, which is the remaining half and is not sized here.

---

## §587 — §583's nil test at two more kinds (+3 cases, 5,946 → 5,949)

§583 replaced the kind-list with upstream's nil test for the **variable-like**
arms only, and said so. The remaining kinds were left alone deliberately: each
needs its own reading of what its arm answers. Two of them turn out to be
cheaper than the first, and both were found by joining the new
`examples/gapdump.rs` against the single-transition population — five of the
seven cases in the row *"declaration name, symbol has no type:
SymbolFlags(FUNCTION) / FunctionDeclaration / neither"* are generators.

### `ReturnStatement` — the premise is INHERITED, not assumed

`getContextualTypeForReturnExpression` (`:29621`) answers nil whenever
`getContextualReturnType(fn)` is nil. The enclosing arm has **already
established exactly that**: a non-expression generator only reaches the yield
loop after `declaration_takes_no_contextual_return` has passed. So this is not
a new assumption about upstream — it is the gate one scope up, restated.

`function* g() { return yield yield 0; }` (`generatorTypeCheck37`) wants
`() => Generator<any, any, unknown>` and was a gap.

This is worth naming as a pattern: **the cheapest nil proofs are the ones some
enclosing gate has already paid for.** §583's variable-like arm needed four
syntactic tests; this one needed none, because the caller could not have got
here otherwise.

### `TemplateSpan` — two lines upstream, two lines here

`getContextualTypeForSubstitutionExpression` (`:30030`) is a tagged-template
check and a `return nil`. Nothing type-shaped is consulted, so the port's test
is syntactic too: walk `TemplateSpan → TemplateExpression → parent` and ask
whether that parent is a `TaggedTemplateExpression`.

``var x = `abc${ yield 10 }def` `` (`templateStringWithEmbeddedYieldKeywordES6`)
wants `() => Generator<number, void, unknown>`; the non-ES6 sibling came along
as a third conversion.

```
GAP->RIGHT: 2    generatorTypeCheck37, templateStringWithEmbeddedYieldKeywordES6
WRONG->RIGHT: 1  templateStringWithEmbeddedYieldKeyword
(no adverse transition of any kind)
```

### What is deliberately still declined

`Parameter` and `BindingElement` (`getContextuallyTypedParameterType`,
`getContextualTypeForBindingElement`) consult real type machinery;
`TypeAssertionExpression`, `AsExpression` and `SatisfiesExpression` answer
`getTypeFromTypeNode` and are therefore **never** nil, which is why
`castOfYield` must keep gapping; `CallExpression`/`NewExpression` arguments have
genuine contextual types. The remaining recursive kinds
(`ConditionalExpression`, `SpreadAssignment`) delegate to their own parent and
would need the whole dispatch ported to answer honestly — that is the shape of
the next slice here, and it is not a kind-by-kind job much longer.

---

## §606 REFUSED — the NEXT slot from an ANNOTATED declaration (0 converted, 2 G→W)

§583 ported the contextual positions where upstream answers **nil** and stated
plainly that the positions where it answers a **type** were not ported. §606
attempted the first of those: `getContextualTypeForVariableLikeDeclaration`
(`checker.go:29438`) opens `if typeNode != nil { return getTypeFromTypeNode(typeNode) }`,
so an ANNOTATED variable-like declaration has a computable contextual type, and
`function* g3() { const value: string = yield; }` wants
`() => Generator<undefined, void, string>` (`generatorImplicitAny`) — the very
fixture §583 cited for the nil half.

Built as upstream builds it: a `next_types` aggregate beside the yield aggregate,
`nextType = getIntersectionType(nextTypes)` when non-empty and `unknown`
otherwise (`checker.go:20161`, `:20242`). Measured:

```
GAP->WRONG: 2  ⚠  generatorReturnTypeFallback.3, generatorReturnTypeFallback.4
(nothing converted — `g3` did not move)
```

**Zero won, two lost.** Reverted.

### What it got wrong, for whoever takes it next

The target did not move and two neighbours broke, which means the arm is firing
where the *annotation* is not the whole answer and not firing where it is. Two
candidates, and this measurement does not separate them:

- **`g3` never reaches the arm.** Its `yield` is bare, and the bare-yield
  contribution and the contextual gate interact in an order §135/§220 tuned by
  measurement; the gate may be answering before the annotation is consulted.
- **`generatorReturnTypeFallback.3`/`4` want the slot from the RETURN
  ANNOTATION, not from a declaration.** They write
  `function* g(): IterableIterator<number, void, string>`, so their NEXT slot
  comes from `getIterationTypeOfGeneratorFunctionReturnType` (§225's road, which
  this port already has), and feeding a declaration-derived type into the same
  slot overwrites it.

The second is the likelier and is the useful warning: **the NEXT slot has more
than one source, and they are not additive.** Anyone porting the type-answering
half should establish which source wins before writing the aggregate — upstream
resolves that at `:20242` by consulting the contextual ITERATION type only when
`nextTypes` is empty, which is precisely the ordering §606 inverted.

---

## §632 REFUSED — the NEXT slot from an ASSIGNMENT position (0 converted, 2 G→W)

Four single-transition cases share one exact want — `() => Generator<any, void,
any>` (`yieldExpressionInFlowLoop`, `yieldExpressionInControlFlow`,
`YieldExpression5_es6`, `YieldStarExpression3_es6`) — and two of them are the
same shape:

```ts
function* f() { let result; while (1) { result = yield result; } }
```

`getContextualTypeForBinaryOperand` (`checker.go:29374`) answers the LEFT
operand's type for `x = yield e`, and `result` is an unannotated `let`, so both
slots are `any`. That is a contextual position whose type this port can compute
with a plain reference lookup — the one §606 (the annotated-declaration
position) was not.

Built with the same `next_types` aggregate upstream uses. Measured **0
converted, 2 GAP→WRONG** (`typeOfYieldWithUnionInContextualReturnType`).
Reverted.

**§633 ran the trace and this paragraph was wrong.** What stood here read *"the
arm never reaches those four cases, so something declines before the yield loop
is entered"*. Instrumenting entry and all eleven declines of
`return_type_from_body` for `yieldExpressionInFlowLoop`:

```
RTFB entry: kind=FunctionDeclaration asterisk=true may_return_never=false
RTFB none #5
```

Entry happens, the contextual-return gate does NOT fire, and decline **#5 is the
`if contextual { return None }` gate itself** — the exact line §632 replaced. So
the reachability hypothesis is dead: the arm reaches the right place and fails
inside its own body. Two candidates remain, and both are one assertion wide:
`binary.right == Some(child)` not holding, or `check_expression(target)`
answering `error` for `let result;` — the unannotated, uninitialised `let` whose
type §599's neighbourhood mints as implicit `any`.

**§632's premise was right and its note was wrong.** Recorded because "it never
gets there" is the most expensive kind of wrong diagnosis: it sends the next
attempt to instrument the callers instead of the four lines that actually
declined.

**Second refusal on the NEXT slot's type-answering half** (§606 was the first,
also 0-for-2). Both attempts assumed the contextual position was the blocker;
neither checked first that the yield loop runs at all for the target fixtures.
**The next attempt should print `check_expression(result)` at that gate** — one
value, and it decides between the two candidates above. §633 already spent the
entry trace this note asked for; do not spend it again.

---

## §634 — the NEXT slot from an ASSIGNMENT, landed (+2 cases, 6,001), and why §632 measured zero

§632 built this arm and measured **0 converted, 2 G→W**. §633's trace killed its
"never reaches" explanation — decline #5 *is* the contextual gate. §634 printed
one more value and found the real defect in three words:

```
BUILT yield=never return=void next=any     <- §632
BUILT yield=any   return=void next=any     <- §634
```

**§632's arm `continue`d.** Recording the contextual type and skipping the rest
of the loop body meant the yield OPERAND was never added to `operand_types`, so
the yield slot aggregated empty and became `never`. Upstream appends to **both**
aggregates from one yield (`checker.go:20334`–`:20347`): `yieldTypes` from the
operand, `nextTypes` from `getContextualType`. They are not alternatives.

Falling through instead of continuing:

```
GAP->RIGHT   2   typeOfYieldWithUnionInContextualReturnType
WRONG->RIGHT 2   yieldExpressionInFlowLoop, yieldExpressionInControlFlow
(no adverse transition of any kind)      checker_types 5,999 -> 6,001
```

**The two GAP→WRONG §632 measured are now GAP→RIGHT** — same fixture, same arm,
one `continue` removed. A single wrong control-flow keyword turned a +4 into a
0-for-2, and three sections were spent explaining a number that had nothing to
do with the design.

### What the chain cost and what it bought

§606 (annotated declarations, 0/2) → §632 (assignments, 0/2) → §633 (the trace:
it declines AT the gate) → §634 (the `continue`). **Two refusals, one trace, one
landing**, and the two refusals were both *right about where to work* and wrong
about why it failed.

**The rule this earns:** when an arm records into an aggregate and the enclosing
loop feeds several, `continue` is a claim that the other aggregates want nothing
from this iteration. Upstream's yield loop feeds two. Neither §606 nor §632
checked which.

---

## §635 — §606 retried with §634's fix: the ANNOTATED position lands too (+2 cases, 6,003)

§606 refused the annotated-declaration NEXT slot at **0 converted, 2 G→W**.
§634 found why §632 measured the same shape of zero — a `continue` that starved
the yield aggregate — and the same defect was in §606. Retried with the
fall-through:

```
GAP->RIGHT    2   generatorReturnTypeFallback.3, generatorReturnTypeFallback.4
WRONG->RIGHT 19   generatorReturnTypeInference 9, generatorReturnTypeInferenceNonStrict 9
(no adverse transition of any kind)     checker_types 6,001 -> 6,003
```

**The two cases §606 BROKE are the two that now convert.**
`generatorReturnTypeFallback.3`/`.4` were §606's entire adverse column; they were
never evidence against the position, only against the `continue`.

§606's note reasoned at length that *"the NEXT slot has more than one source and
they are not additive"*, and offered the return-annotation road as the likely
conflict. That was wrong. The sources ARE additive — upstream appends to
`yieldTypes` and `nextTypes` from the same iteration — and the arm was starving
one of them.

### The three-arm chain, complete

| | position | result |
|---|---|---|
| §583 | the nil positions (unannotated variable-like) | +5 lines |
| §587 | `ReturnStatement`, `TemplateSpan` | +3 cases |
| §606 | annotated declarations — `continue` | 0 for 2, refused |
| §632 | assignments — same `continue` | 0 for 2, refused |
| §633 | trace: it declines AT the gate, not before | — |
| §634 | assignments, fall-through | +2 cases |
| §635 | annotated declarations, fall-through | +2 cases |

**Two refusals, both caused by one keyword, both recovered.** The refusals'
numbers were honest and their explanations were not, and the explanations are
what cost the extra rounds: §606 blamed slot precedence, §632 blamed
reachability, and the answer was neither.

---

## §636 — the ASSERTION positions feed the NEXT slot (+1 line), and a CAVEAT on the single-transition method

`getContextualType`'s assertion arms are `getTypeFromTypeNode(parent.Type())`
(`checker.go:29372`, `:29396`) — the contextual type is *written down*, so this
port can compute it. `castOfYield` writes `<number>(yield 0)` and wants
`() => Generator<number, void, number>`; §583 recorded it as a gap that "stays"
because the port could not compute a contextual TYPE. It can compute this one.

```
WRONG->RIGHT: 1   castOfYield
(no adverse transition of any kind)
```

A `const` assertion delegates to its own parent upstream (`isConstAssertion`),
which this arm does not model and therefore declines rather than reading `const`
as a type.

### The caveat, which is worth more than the line

`castOfYield`'s verdict-baseline deficit is **1**, and converting that line did
**not** convert the case. The suite says why:

```
checker_types  FAIL  [5/9 lines]
  castOfYield.ts: 8 assertion(s), expected 9
```

It fails on an assertion **COUNT** mismatch — this port emits 8 lines where the
baseline has 9 — and `verdict.rs` records only ALIGNED lines, so that failure is
invisible to `target/verdict_baseline.tsv` entirely.

**Corpus-wide: 3,535 cases fail and only 3,381 carry a non-right aligned line.
154 cases (4.4%) fail for reasons the baseline cannot represent.**

So **"deficit 1 in the verdict baseline" is an UPPER bound on "one line from
passing", not a synonym.** The single-transition join (STATUS §6) that produced
§584, §605, §616, §626, §634 and §635 is still the best case-forecast this board
has — every one of those landed real cases — but its convertible counts include
an unknown share of cases that would still fail on alignment after the arm.
**Check a deficit-1 case with `casequery` before promising it converts.**

---

## §640 — ASYNC generators mint `AsyncGenerator` (+2 cases, 117 lines, 0 R→W)

`createGeneratorType(yield, return, next, isAsync)` (`checker.go:20247`) picks
`AsyncGenerator` over `Generator` from one flag; the three slots are computed
identically. §583 declined `is_async` **wholesale**, and that gate was written
for the async NON-generator arm — whose contextual return really can turn `void`
into `undefined` (`checker.go:20179`). A generator's slots never go through that
road, so the gate was over-broad from the start.

Split so the async gate applies only to async generator EXPRESSIONS (which can
carry a contextual signature) and the declaration road resolves:

```
GAP->RIGHT   85   emitter.asyncGenerators.objectLiteralMethods.es2015 18, …
WRONG->RIGHT 32   privateNameStaticMethodAsync 15, privateNameMethodAsync 10, …
GAP->WRONG    3 ⚠  types.asyncGenerators.es2018.1
RIGHT->WRONG  0
checker_types 6,003 -> 6,005, gradient 90.78% -> 90.80%
```

**117 conversions from splitting one boolean.** The arm was not missing; it was
excluded by a condition that named the wrong population — §583 wrote
`is_async ||` where upstream writes `isAsync` as an *argument* to the type
constructor.

### What is still approximate, and why it measured almost clean anyway

Upstream takes the **awaited** operand type for an async generator's yield slot.
This arm does not await, so a `yield somePromise` inside an async generator
would over-report. The corpus punished that three times
(`types.asyncGenerators.es2018.1`) against 117 conversions, because the
overwhelming majority of async-generator fixtures yield non-thenables or nothing
at all — `never`, `any`, or a plain value.

**Recorded rather than gated**: gating on "no thenable operand" would need
`getAwaitedType`, which is the same machinery the async non-generator arm still
waits on (§14). The three lines are the honest price and are named here so a
future `getAwaitedType` landing knows to re-measure them.

### §641 REFUSED — recovering a bare `yield *` (0 transitions)

`function* g() { yield *; }` wants `() => Generator<any, void, any>`
(`YieldStarExpression3_es6`, `YieldExpression5_es6`, both deficit 1): upstream
recovers from the parse error by typing the absent operand `any`, and
`getIterationTypesOfIterable(any, …)` answers `any` for the NEXT slot.

The delegating branch here opens `let operand = operand?;`, which declines the
whole signature when the operand is missing — an obvious candidate. Replacing it
with the `any`/`any` contribution measured **zero transitions**. Reverted on
§586's rule.

**So the operand is not `None` at that point**, and the case declines somewhere
earlier or the parser does not build a delegating yield here at all — §583
records that `yield * []` OUTSIDE a generator is a MULTIPLICATION and that this
port's parser handles the distinction in `tsr_parser` (§106), which makes the
parse of a bare `yield *` INSIDE one the first thing to check.

**Next step, and it is one trace**: print `delegates` and `operand.is_some()` for
`YieldStarExpression3_es6` before writing another arm. §632 and §606 both cost a
build-and-measure cycle for exactly this — assuming which branch a fixture takes
instead of printing it — and §633/§634 are what it took to recover.

### §642 — `yield*` over an `any` operand (+2 cases, 6,005 → 6,007), and §641's lesson applied

`getIterationTypesOfIterable` on `any` answers `any` throughout
(`checker.go:20343`), so `yield* x` with an `any` operand contributes `any` to
BOTH slots. The delegating branch tested for an `Array<…>` reference and
declined everything else, and `any` is not a reference.

That is also the parse-recovery shape: `function* g() { yield *; }` gives the
delegating yield an `Identifier` operand with **empty text**, typing as `any`.

```
WRONG->RIGHT: 2   YieldStarExpression3_es6, YieldExpression5_es6
(no adverse transition of any kind)      checker_types 6,005 -> 6,007
```

### §641 guessed; §642 printed

§641 assumed the operand was ABSENT and patched `operand?`. Zero transitions.
Its note then demanded the trace before any further arm:

```
YIELD delegates=true operand=Some((Identifier, "", "any"))
```

Two values — `delegates` and what the operand actually IS — and the answer was
immediate: present, a placeholder, already typed `any`. **The decline was never
the `?`; it was the type-reference test three lines below.**

This is the third time in this arm that printing the branch beat reasoning about
it (§633 for the gate, §634 for the `continue`, §642 here), and the second time a
refusal's own recorded next-step produced the landing within one run. **A
refusal that names the value to print is worth more than one that names a
hypothesis.**

## §741 — the strict-mode implicit-return `| undefined` at the PLAIN arm (+8 cases, +29 W→R / +1 G→R vs 1 G→W, zero R→W)

Upstream appends `undefinedType` to a non-empty return aggregate whenever
`strictNullChecks` is on and the function has an implicit return
(`checker.go:20301`): a bare `return;` beside a valued one, OR a body end
that is reachable (`functionHasImplicitReturn`, `:20261`/`:20307`). The
ASYNC arm of `return_type_from_body` has carried this since §11 named it;
the PLAIN arm both missed the reachable-end half entirely and DECLINED the
bare-return-beside-valued shape whole, on the rationale *"this port has no
compiler options and is uniformly non-strict"* — which ADR-0042 expired and
nobody had cashed in. The near-miss board's `| undefined` cluster (15 lines:
`typeGuardsDefeat`, `narrowedConstInMethod`, `TypeGuardWithArrayUnion`,
`enumLiteralsSubtypeReduction`, …) was this arm.

### The reachable-end test took three cuts, and the pair is the finding

1. **`block_completes_normally` alone: 3 R→W.** Its expression-statement arm
   CHECKS calls to ask never-ness, and doing that mid-signature-computation
   re-enters the very signature being inferred — the self-call in
   `typeParameterAsTypeArgument` (`foo<U,U>(y,y); return new C<U,T>()`)
   cycled and the whole function printed `any`. **A reachability stand-in
   that types expressions is not a read-only probe, and inside signature
   inference that distinction is load-bearing.**
2. **The binder's `NodeFacts::HAS_IMPLICIT_RETURN` alone: 22 R→W.** The fact
   is set whenever the end flow is not syntactically UNREACHABLE
   (`binder.rs:1180`); a switch whose every clause returns is dead in a way
   only upstream's `isReachableFlowNode` (`:20308`) sees —
   `exhaustiveSwitchStatements1`, the `enum/numeric/booleanLiteralTypes`
   families.
3. **Paired — fact short-circuits, then the walk confirms: zero R→W.** A
   body whose end is provably dead never reaches the checking walk (which
   protects the self-call shape: its end sits after a `return`), and the
   walk's `None` (switch, loop, try, call in the way) declines the append
   rather than assuming either way.

### Measured (full scorepair pair, baseline freshly accepted on clean HEAD)

```
cases    6,075 → 6,083   (+8)
right    435,590 → 435,620  (+29 W→R, +1 G→R)
adverse  1 G→W — narrowingByTypeofInSwitch 0:214, beside the same case's
         G→R; a typeof-switch this port cannot prove exhaustive
         (`is_exhaustive_switch_statement`'s typeof arm, recorded there)
```

`controlFlowForFunctionLike1` 8, `capturedLetConstInLoop8`(+`_ES6`) 4 lead
the gains. Two expired-rationale pins updated with the build:
`return_inference.rs`'s bare-return `error` pin (now `1 | undefined` — the
harness defaults strict) — the eighteenth stand-in pair to come due.

### Residue

The `None`-declining walk still misses implicit returns behind a call in
the body (`if (c) return 1; console.log(x);` — upstream appends, this port
keeps today's answer). Converting those needs `isReachableFlowNode`, the
reachability walk `flow.rs` records as unported.

## §743 — `isReachableFlowNode` ported; `functionHasImplicitReturn` reads the flow graph (+3 cases, +4 W→R, ZERO adverse)

§741's residue, taken in the next session exactly as recorded. The strict
implicit-return `| undefined` (`checker.go:20301`) asks
`functionHasImplicitReturn` (`:20307`), which is two facts: the body has an
end flow node (the binder drops it when the end is syntactically dead,
`binder.rs:1181`, the same condition that sets `HAS_IMPLICIT_RETURN`) and
`isReachableFlowNode(endFlowNode)` says control can reach it. §741 stood in
for the second fact with the statement-shaped `block_completes_normally`
walk, whose `None` at every switch, loop, try and call declined the append.

### What was ported

`isReachableFlowNodeWorker` (`flow.go:2522`), arm for arm, in `flow.rs`:

- the SHARED-node cache (`flowNodeReachable`, a new `Checker` map keyed by
  flow index like `flow_loop_cache`) with upstream's `noCacheCheck` dance;
- ASSIGNMENT / CONDITION / ARRAY_MUTATION step to the antecedent;
- CALL asks the effects signature. The port's `getEffectsSignature` is
  §127's syntactic pre-gate (`callee_declares_asserts`: a callee whose
  declaration VISIBLY returns `asserts …` or `never`) followed by
  resolution — the same road `get_type_at_flow_call` takes. That gate is
  what keeps this walk a read-only probe inside signature inference: a
  callee that declares its return type is never the signature being
  inferred, so §741's self-call re-entry (3 R→W from typing calls
  mid-inference) cannot recur. The `asserts x` with a literally-false
  argument arm is ported too, with `isFalseExpression` (`flow.go:2589`);
- BRANCH_LABEL any-antecedent, through `getBranchLabelAntecedents` with the
  REDUCE_LABEL stack (the narrowing walk still skips reduce labels; this
  walk honours them because it was free to);
- LOOP_LABEL follows the entry edge; SWITCH_CLAUSE asks
  `bypass_of_exhaustive_switch`; default answers `!UNREACHABLE`.

Upstream's `lastFlowNode`/`lastFlowNodeReachable` one-entry memo is not
ported — a memo over the answer the map already holds.

### The first scorepair: +4 W→R, 3 R→W — and both R→W were the exhaustiveness predicate's recorded holes

`stringEnumLiteralTypes1/2` f10 (`switch (x)` over `type YesNo = Choice.Yes
| Choice.No`, string enum) and `narrowingByTypeofInSwitch`'s
`switchOrdering` (`switch (typeof x)`) each gained a `| undefined` upstream
does not print. Neither is the walk's fault: both switches ARE exhaustive
upstream, and `is_exhaustive_switch_statement` answered `false` for reasons
its own doc comment had named — the `typeof` arm was declined whole, and
the literal arm compares type identities that a port-side stand-in breaks.
§741's `None` had hidden both, because a `None` at a switch never asked.

**A predicate that was only ever consulted where `false` was the safe
answer has never been measured in the direction where `false` is wrong.**
The reachability walk is the first consumer for which non-exhaustive is an
active claim, and it exposed both holes in one run.

1. **The `typeof` arm** (`flow.go:1950-1966`) is now ported:
   `getSwitchClauseTypeOfWitnesses`, `getNotEqualFactsFromTypeofSwitch`,
   `typeofNEFacts` and `TypeFactsAllTypeofNE` (which, checked against
   `checker.go:476`, does NOT include the host-object bit — the first draft
   had it). `getBaseConstraintOrType` is a constrained type parameter's
   constraint and otherwise the type itself (`type_parameter_constraint`),
   and now serves the literal arm too, which had declined type-parameter
   discriminants.
2. **The string-enum mint.** `Choice.Yes` written in TYPE position is a
   named `OBJECT` mint carrying the member symbol (`declared.rs`, the
   `qualified_type_reference` string-valued arm — kept deliberately, for
   `discriminatedUnionTypes4`'s 3 R→W / 7 R→G when the literal types were
   handed out), while `Choice.Yes` as a case EXPRESSION is the enum literal
   type. `eachTypeContainedIn` compares identities, so the trace read
   constituents `#28/#29 OBJECT` against clauses `#20/#22 ENUM` and
   answered `false`. The exhaustiveness test now un-spells a mint to the
   member type it stands for (`enum_member_behind_mint`) before comparing.
   Local to this predicate on purpose: the mint's reason is the printing
   and narrowing roads, and this test is neither.

The traced cause is worth a line of method: the unit harness (no lib)
types the case expression to `error` and declines the clause list, so the
first trace there was uninformative; tracing the CORPUS case
(`TSR_FILTER=… verdictdump`) showed the ids in one run.

### Measured (full scorepair pair over a baseline freshly accepted on clean HEAD; the committed baseline pre-dated §742 and reproduced its 56 G→R exactly, which is how that was noticed)

```
checker_types  6,085 → 6,088 (+3) · right 435,722 → 435,726 (+4 W→R)
               G→W 0, R→W 0, R→G 0
               classPropertyErrorOnNameOnly 2, enumLiteralsSubtypeReduction 1,
               narrowByClauseExpressionInSwitchTrue3 1
diagnostics    2,596 → 2,596
```

Small on purpose: §741 had already taken the population its stand-in
could reach; this is the residue plus two predicate repairs. What the
port gains beyond the lines is a reachability primitive upstream uses in
five places — the next reachable one is **TS7027 over never-returning
calls** (`checker.go:2466`, `neverReturningFunctions1`, 25 of TS7027's 29
missing lines), recorded in STATUS §5 as waiting on exactly this walk.

### Residue

- The narrowing walk (`get_type_at_flow_node`) still skips REDUCE_LABEL
  nodes to their antecedent; the reachability walk honours them.
- `getBaseConstraintOrType` is one level deep (a type parameter's written
  constraint); upstream resolves the base constraint recursively.
- The unreachable-assignment `unreachableNeverType` arm (`flow.go:256`) is
  still not asked; the sentinel converts to the declared type at the exit
  anyway.

## §744 — TS7027 over `never`-returning calls, and the `unreachableNeverType` sentinel (+0 cases / +17 W→R, +3 `diagnostics` cases, ZERO adverse)

§743's walk had one consumer. This section wires the second — the one §5
had recorded against exactly this walk since the twelfth `diagnostics`
session — and in doing so replaces §128's stand-in for upstream's
`unreachableNeverType` with the sentinel itself.

### The consumer: `isSourceElementUnreachable`'s `else` half

`checker.go:2466`: *"for code the binder doesn't know is unreachable, use
control flow / types"* — a statement the binder did not flag but gave a
flow node reports TS7027 when `isReachableFlowNode` says control cannot
reach it. This binder records a flow node on every statement
(`binder.rs:1291`, the `FIRST_STATEMENT..=LAST_STATEMENT` range), so the
port is one arm in `is_unreachable_run_member`: no `UNREACHABLE` fact →
ask the walk. The ancestor test in `check_unreachable` asks the same
predicate, which is upstream's `withinUnreachableCode` (`:2264`) — set
when an ancestor REPORTED, and an ancestor cut off by a call is one the
binder never flagged.

Two widenings of §127's syntactic pre-gate were needed for the fixture's
shapes and are faithful to `getTypeOfDottedName`: a PARAMETER annotated
with a function type (`fail: (message?: string) => never`, f11–f13) and
parentheses on the callee or its base (`((Debug).fail)()`, f24).

All **22** of `neverReturningFunctions1`'s expected TS7027 lines land
exactly. The case still fails on 4 unexpected TS2532 at `this.data` under
`ThisType<T & Component>` — contextual `this`, a different arm.

### The sentinel: §128 was right at a read and wrong at a join

Before this build the case ALSO reported 8 false TS18048/TS2532, and they
are §128's: a `never`-returning call answered the DECLARED type at the
CALL flow node, on the correct observation that an unreachable read prints
the declared type. But `if (x === undefined) fail(); x.length` is a
JOIN: the cut-off path re-entered the union as `string | undefined` where
upstream's `unreachableNeverType` — flagged `Never` — drops out. The
observable §128 matched was the EXIT conversion (`flow.go:111`), applied
one node too early.

Ported now: `Intrinsics::unreachable_never`, a second `NEVER`-flagged
intrinsic; the call arm returns it; the two walk exits convert it to the
declared type; both joins drop it through its flag like any `never`. And
one rule that was not in the first cut and cost 2 R→W: upstream appends
EVERY antecedent type and `getUnionType` returns a LONE type unchanged —
so a join whose every path is cut off (`f30`'s trailing `x`) answers the
sentinel, and the exit prints the declared type, where the first cut's
empty list printed bare `never`. Both joins now return a lone
`never`-flagged antecedent as-is before filtering.

That rule is what the +10 in `staticAnonymousTypeNotReferencingTypeParameter`
is: a join this port had been answering `never` for, where upstream's lone
antecedent survived.

### Measured (full scorepair pair over a baseline accepted on clean §743 HEAD)

```
checker_types  6,088 → 6,088 · right 435,726 → 435,743 (+17 W→R; G→W 0, R→W 0, R→G 0)
               staticAnonymousTypeNotReferencingTypeParameter 10,
               neverReturningFunctions1 6, assertionTypePredicates1 1
diagnostics    2,596 → 2,599 (+3: reachabilityChecks8,
               unreachableSwitchTypeofAny, unreachableSwitchTypeofUnknown;
               0 lost — the full case-list diff shows only these and two
               still-failing cases moving closer)
```

### Residue

- `getEffectsSignature` proper (`flow.go:2047`) — resolving the callee's
  TYPE and testing `hasTypePredicateOrNeverReturnType` — is still the
  syntactic pre-gate. A `never` that has to be inferred (a function with
  no annotation whose body always throws) reads reachable here.
- The `asserts x` with a literally-`false` argument arm of
  `narrowTypeByAssertion` (`flow.go:339`) still narrows through
  `narrow_type` rather than answering the sentinel.
- The unreachable-assignment sentinel (`flow.go:226`/`:257`) is not asked.

## §745 — `narrowTypeByAssertion` ported, and `filterType`'s `never` pass-through (ZERO movement, ZERO adverse)

§744's residue, second item: a bare `asserts x` narrowed through
`narrow_type(arg, assumeTrue)` for every argument shape. Upstream's
`narrowTypeByAssertion` (`flow.go:339`) has three arms in front of that
call — a literal `false` answers `unreachableNeverType`, `&&` asserts the
left then the right, `||` unions the two — and this section ports them,
arm for arm, as `narrow_type_by_assertion`.

### What the first pair found: the port's `filterType` dropped the sentinel

The first scorepair measured **0 W→R and 1 R→W**: `assertionTypePredicates1`
row 122, `x; // Unreachable` after `assert(false && x === undefined)`,
`unknown` → `never`. The `&&` arm is right — the left half answers the
sentinel, the right half narrows the sentinel by `x === undefined` — and
the divergence was one layer down. Upstream's `filterType`
(`checker.go:26588`) reads

```go
if t.flags&TypeFlagsNever != 0 || f(t) { return t }
return c.neverType
```

so a `never`-flagged NON-union passes through UNCHANGED, sentinel included,
and the walk exit converts it to the declared `unknown`. This port's
`filter_type` non-union arm was `if predicate(t) { t } else { never }` —
it answered bare `never` because `getTypeFacts` of a `never` type is
`None`, and bare `never` is not the sentinel at the exit. Ported the
pass-through. That is the whole fix; the `||` arm drops `never`-flagged
halves the way `addTypeToUnion` does and returns a lone survivor
unchanged, the rule §744 found load-bearing at the branch-label join.

### Measured (full scorepair pair, baseline accepted on clean §744 HEAD)

```
checker_types  right 435,743 → 435,743 · no transitions vs baseline
diagnostics    casequery --list before/after: identical
```

**Zero movement, and it is landed anyway**: `assert(false)` printed the
declared type before this build only because `narrow_type` of a `false`
keyword returns its input, which is an accident the `&&` shape exposed
(the first pair's R→W is what the accident would have cost the first time
anyone asserted `false && …`). The `filter_type` divergence was real and
independent of assertions — any road that filters a `never`-flagged
non-union now answers upstream's type rather than bare `never`.

### Residue (unchanged from §744, minus the assertion arm)

- `getEffectsSignature` proper (`flow.go:2047`) is still the syntactic
  pre-gate; an inferred `never` return reads reachable.
- The unreachable-assignment sentinel (`flow.go:226`/`:257`) is not asked.
