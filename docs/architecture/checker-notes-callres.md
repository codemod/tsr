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
  **[Corrected in §746: the second sentence was wrong about upstream —
  `hasTypePredicateOrNeverReturnType` reads `getReturnTypeFromAnnotation`,
  so an inferred `never` reads reachable upstream too. The first sentence
  stood; §746 ported the function.]**
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
  pre-gate; an inferred `never` return reads reachable. **[Corrected in
  §746: the inferred-`never` half is upstream's behaviour too; the
  pre-gate half is ported.]**
- The unreachable-assignment sentinel (`flow.go:226`/`:257`) is not asked.

## §746 — `getEffectsSignature` proper: the §127 syntactic pre-gate retired (+4 W→R / 1 R→W, `diagnostics` unchanged)

§744's residue, first item — and its note was WRONG about upstream, which
this section corrects: `hasTypePredicateOrNeverReturnType` (`flow.go:2211`)
reads `getReturnTypeFromAnnotation`, never the inferred return. A function
whose body only throws does not truncate flow at its callers upstream
either. What the pre-gate was actually missing was every callee shape
outside its two declaration forms: a variable annotated with an ALIAS of a
predicate function type (`const b: Test`), a class property, a `this`- or
`super`-based member, a property chain deeper than one, and every
non-statement call.

### Ported

- `get_effects_signature` (`flow.go:2047`): a statement-level call types
  its callee through `get_type_of_dotted_name`; any other call through
  `check_expression` (upstream's `checkNonNullExpression`); `super` callees
  decline. Call signatures of the apparent type: a lone non-generic
  signature is the answer, otherwise a set with SOME predicate-or-`never`
  member resolves the call. The `[Symbol.hasInstance]` binary arm is not
  ported (this binder mints no `instanceof` flow-call nodes).
- `get_type_of_dotted_name` (`:2122`) and `get_explicit_type_of_symbol`
  (`:2154`): identifiers through `resolve_name` + merged symbol,
  `this`/`super` through the checker's own expressions, property access
  through `get_property_of_type` on the base's EXPLICIT type,
  parentheses. Functions/methods/classes/namespaces answer their type; a
  variable or property only with an annotation
  (`isDeclarationWithExplicitTypeAnnotation`). Not ported: the
  mapped-symbol origin arm, the `for..of` arm, the related-info diagnostic.
- `has_type_predicate_or_never_return`: a predicate, or an annotated
  return whose type is `never` (the annotation's TYPE, so `type N = never`
  counts, as upstream's `getReturnTypeFromAnnotation` does).
- Both consumers — `get_type_at_flow_call` and the reachability walk's
  call arm — enter through it. `callee_declares_asserts` is deleted.

### Measured (full scorepair pair, baseline accepted on clean §745 HEAD)

```
checker_types  right 435,743 → 435,746 (+4 W→R: assertionTypePredicates1 3,
               controlFlowFunctionLikeCircular1 1; 1 R→W: the same case's
               `b() : boolean` → `any`, file _8 row 7)
diagnostics    casequery --list identical
```

### The R→W, and why it is the port's eager predicate and not this arm

`controlFlowFunctionLikeCircular_8`: `b();` then `type First = typeof arg;
type Test = (arg: unknown) => arg is First; const b: Test = whatever`.
Upstream resolves a function TYPE's predicate lazily
(`getTypePredicateOfSignature`), so `b`'s type is fully resolved before
anything asks `First`; `First`'s `typeof arg` walk then re-enters `b()`'s
effects signature, which re-asks `First`, and the cycle breaks THERE —
TS2456, `First : any`, `b : (arg: unknown) => arg is any`, `b() : boolean`.
This port resolves the predicate WITH the signature (`Signature::predicate`'s
own doc records the choice), so `b`'s resolution itself reaches the walk,
the walk re-enters `b`, and the cycle breaks one level higher: `b : any`,
`b() : any`. Before this build the line was right only because nothing
probed `b` mid-resolution. The four `b : …` rows in the same file were
already wrong for the same reason. Taken at 4:1 with the cause named; the
fix is lazy predicate resolution, which is a signatures-module change, not
a flow one.

**Falsifier:** any R→W outside a circularity fixture on a callee this
port types differently mid-walk than at its own check — that would mean
`get_type_of_dotted_name` is not the read-only probe upstream's is.

### Residue

- Lazy `getTypePredicateOfSignature` (the R→W above).
- `getExplicitThisType` proper: a `this`-parameter WITHOUT an annotation
  should decline; `check_this_expression` answers it.
- The unreachable-assignment sentinel (`flow.go:226`/`:257`) is not asked.
- `neverReturningFunctions1` still fails on `ThisType<…>` contextual `this`.

## §747 — the unreachable-assignment sentinel and the `for..in` non-null arm (+1 W→R, ZERO adverse)

§744's third residue item, plus the one arm of `getTypeAtFlowAssignment`
the port had left at `None`.

- `flow.go:226` and `:256`: an assignment the walk cannot reach — direct
  match or a left-hand part of a dotted reference — answers
  `unreachableNeverType`. The old comment at the dotted arm argued the
  declared type was equivalent because the exit converts the sentinel;
  §744 showed that argument is right at a read and wrong at a join, and
  the same reasoning applies here. Both arms now ask
  `is_reachable_flow_node` first.
- `flow.go:267`: `for (const _ in ref)` acts as a non-null on `ref` — a
  `VariableDeclaration` whose grandparent is a `ForInStatement` whose
  expression matches the reference answers the antecedent's type through
  `get_non_nullable_type`. The `optionalChainContainsReference` half is
  deferred to §748: this port HAS a helper of that name (§51.2), but it
  walks `?.` tokens with a sticky flag rather than upstream's
  `IsOptionalChain` flag walk, and the two disagree on `a.b?.c` against
  reference `a`; `finalizeEvolvingArrayType` is a no-op on this port's
  representation (evolving arrays live in `state.array_elements`, §736).
  CORRECTED before commit: the first draft of this note said no such
  helper existed.

### Measured (scorepair over the §745 baseline, so cumulative with §746)

```
checker_types  right 435,746 → 435,747 (+1 W→R: forInStrictNullChecksNoError;
               G→W 0, R→W 0 beyond §746's one)
diagnostics    casequery --list identical to §746's
```

The sentinel arms measured zero on their own — no corpus line sits on a
join fed by an unreachable assignment that this port also narrows — and
are landed for the same reason §745 was: the declared-type stand-in was
the exact shape §744 had to replace at the call arm.

## §748 — `NodeFlags::OPTIONAL_CHAIN` set by the parser; `optionalChainContainsReference` becomes the flag walk; the binder's chain flow goes live (+1 case / +17 W→R, ZERO adverse)

§747's deferred half, and the thing underneath it.

### The forcing constraint

`flow.go:1851`'s `optionalChainContainsReference` loops `for
ast.IsOptionalChain(source)`, and `ast.IsOptionalChain` is a NODE FLAG test
(`ast/utilities.go:344`): `NodeFlagsOptionalChain` on a property access,
element access, call, or non-null expression. This parser recorded the `?.`
token on the node but never set the flag (`crates/tsr-binder/src/narrowing.rs`
carried an "inert today" note and `tsr-y4u.7` recorded the gap). So:

- the checker's §51.2 helper of that name walked `?.` TOKENS with a sticky
  bit, which is not the same predicate — it answers `true` for `a.b?.c`
  against reference `a` (upstream stops at `a.b`, which is not a chain
  link), and it cannot see a `NonNullExpression` link at all;
- every optional-chain path in the binder (`bind_optional_chain_flow` and
  below, `binder.rs:2259`) was unreachable — `a?.b` got the flow graph of
  `a.b` — and the checker had grown a family of syntactic stand-ins
  (SS150/SS151/SS154, §51.4) over a graph that lacked the `?.` branch.

### What landed

1. **Parser** (`crates/tsr-parser/src/expression.rs`): every member-rest
   constructor in the call/member loop stamps the flag exactly as
   `parser.go:5399-5521` does — `questionDotToken != nil ||
   tryReparseOptionalChain(expression)` for property/element access and
   call; `tag.Flags & OptionalChain` for a tagged template.
   `try_reparse_optional_chain` is ported whole, including the retroactive
   stamp on a run of non-null expressions over a chain (`a?.b!.c` makes
   `a?.b!` a link only once `.c` follows). The decorator loop
   (`parse_decorator_expression`) has no `?.` arm and is left alone: no
   chain can begin there.
2. **Checker** (`flow.rs`): `optional_chain_contains_reference` is now the
   flag walk, with `is_optional_chain` (`ast.IsOptionalChain`) beside it.
   The §747 `for..in` arm gets its second disjunct, and its answer no
   longer carries the antecedent's `incomplete` — upstream's
   `FlowType{t: …}` does not (corrected here).
3. **Binder**: nothing changed in code; the ported chain-flow functions
   became reachable. The three "inert" comments are updated to say so.

### Measured

```
scorepair over the §747 baseline (435,747):
  right 435,747 → 435,764   +17 W→R, ZERO adverse
    controlFlowOptionalChain 16, nonNullableTypes1 1
coverage (all sixteen suites):
  checker_types 6,088 → 6,089
  diagnostics   2,600 → 2,600 by casequery; controlFlowOptionalChain's
                FAIL reason 9 unexpected / 6 missing → 5 / 2
  every other suite identical (parser_typescript 5,031, binder_symbols
                8,497, printer_round_trip 11,805 unmoved)
```

The binder's chain flow going live cost nothing measurable — the
checker's stand-ins and the real graph agree everywhere the corpus looks.

**Snapshot drift, recorded rather than claimed.** The committed
`diagnostics.snap` read 2,599 and was last written at §744; the §747 tree
already scores 2,600 by casequery, so the +1 in this landing's snapshot
diff belongs to the §745–§747 window, not to §748. A `casequery
diagnostics --list` from a §744 worktree names it:
`compiler/forInStrictNullChecksNoError`, FAIL at §744, PASS at §747 — the
very case §747's `for..in` arm converted in `checker_types`, so §747's
"`diagnostics` unchanged" was wrong (its comparison ran against §746's
list, which this run shows was itself not re-taken). Corrected in §7.

### Follow-ups named, not built

- **TS2779** (`check.rs`, §181's decline): the flag now exists, so the
  "optional property access as assignment target" arm can be ported over
  it instead of declining. `spine_has_optional_chain` walks through
  parentheses, which the flag does not; the two differ exactly there.
- The checker's other `?.` stand-ins (`spells_question_dot_chain` at the
  equality arm, SS151) still key on the token. Each is a candidate for
  the flag, one measured landing at a time.
- `optionalChainContainsReference`'s remaining upstream call sites not yet
  in this port: the predicate arm (`flow.go:324`), the equality
  containment pair (`:490`/`:492`, `narrowTypeByOptionalChainContainment`),
  `instanceof` (`:814`), the switch pair (`:1073`/`:1077`), and
  `hasMatchingArgument` (`:1888`).

## §749 — `hasMatchingArgument` whole, the `this is T` predicate argument, the two remaining chain strips, and the branch label's `UnionReductionSubtype` (+2 cases / +50 W→R / +9 G→R, ZERO adverse)

§748's follow-up list, taken in order of upstream distance.

### What was wrong

`narrow_type_by_call_expression` gated the predicate road on *"some
argument IS the reference"*. Upstream's gate is `hasMatchingArgument`
(`flow.go:1886`): an argument that is, CONTAINS (`isFoo(x.y)` for `x`), or
optionally chains onto the reference — or a callee that is a property
access whose receiver is or contains it. That last clause is the `this is
T` road: `b.isLeader()` narrows `b`, and `getTypePredicateArgument`
(`flow.go:2451`) answers the RECEIVER of the invoked access for a
`TypePredicateKindThis`. This port's predicate struct spells the `this`
form as `parameter_name: None`, and the arm returned `t` for it — so every
`this`-predicate method call narrowed nothing. `typeGuardFunctionOfFormThis`
was 41 lines from passing on that alone.

### What landed

- `has_matching_argument`, `is_or_contains_matching_reference`,
  `skip_parentheses`, `every_type`, `is_nullable_type` — ported by name.
  `is_nullable_type` is `hasTypeFacts(t, IsUndefinedOrNull)` upstream;
  this port's facts carry `IS_UNDEFINED` but no `IS_NULL`, so it reads
  the `NULLABLE | ANY` flags instead (`any`'s facts are `All`; `unknown`
  masks the bit out). Recorded as a stand-in, not a transcription.
- `get_type_predicate_argument`'s `this` half, inline in the call arm.
- `narrow_type_by_type_predicate` (`flow.go:315`): the matching arm, and
  the chain strip (`:324`) — `isFoo(o?.x)` proves `o` non-null on the true
  branch when the predicate type cannot be `undefined`, and on the false
  branch when every constituent of it is nullable. **Not ported**: the
  `getDiscriminantPropertyAccess` → `narrowTypeByDiscriminant` road,
  which this port has never factored out of its arms (each arm carries
  its own inline discriminant test). Named residue.
- The `assumeTrue || !isCallChain(call)` guard: a `?.()` call answers no
  predicate on the false branch.
- `instanceof` (`flow.go:814`): `o?.x instanceof C` strips the base.
- **The branch label's subtype reduction** (`flow.go:1298`). The first
  pair read 2 R→W in the same case: after `if (b.isLeader()) … else if
  (b.isFollower()) …` the join of `LeadGuard | FollowerGuard |
  GuardInterface` printed all three where upstream prints
  `GuardInterface`. Upstream's branch label sets `subtypeReduction` when
  an antecedent is not a SUBSET of the initial type and hands the list to
  `getUnionType(…, UnionReductionSubtype)`; this port's loop label already
  did exactly that through the decidability-gated
  `union_with_subtype_reduction` (`checker-notes-assign.md` §9), and the
  branch label ran only the declared-heritage slice (SS203) — which
  cannot see that a class is a structural subtype of an interface that
  merely `extends` its base. Mirrored; the heritage slice stays as the
  fallback when the gated reducer declines. The 2 R→W became 2 W→R.

### Measured

```
scorepair over the §748 baseline (435,764):
  first cut (no branch-label change)  +45 W→R / +9 G→R / 2 R→W
  landed                              +50 W→R / +9 G→R / 0 adverse
    typeGuardFunctionOfFormThis 32+9, typePredicatesOptionalChaining2 4,
    assertionTypePredicates1 3, tail 11
coverage: checker_types 6,089 → 6,091; diagnostics 2,600 unmoved;
          every other suite identical
```

### Residue

- The discriminant road inside `narrowTypeByTypePredicate` (above).
- `is_nullable_type` as a flag test; an `IS_NULL` fact would retire it.
- The port still runs the `hasOwnProperty` arm BEFORE the predicate arm
  (upstream: after). No corpus line distinguishes the orders; noted so a
  future disagreement is recognisable.

## §750 — the comparable relation, `isDiscriminantProperty`, and the `getDiscriminantPropertyAccess`/`narrowTypeByDiscriminant` pair (ZERO movement, ZERO adverse — a fidelity swap)

STATUS §4.-4's first three steps, in its order.

### 1. `Relation::Comparable` (`relater.rs`)

Three comparable-specific rules the port's shapes can reach, each anchored:
the simple arms tried REVERSED first unless the target is `never`
(`relater.go:181`/`:2661`); a source union needs only SOME constituent
(`:2870`); the assignable-only simple arms are shared (`:261`).
`is_type_comparable_to` / `are_types_comparable` (`:162`/`:166`). Five
unit tests in `tests/relater.rs`, each naming the mutation that reddens it.
Upstream's further carve-outs (type parameters `:3435`, template literals
`:3574`, the intersection-into-primitive hoist `:2886`, mapped-type
modifiers `:3973`) sit on shapes this relater does not decide; gaps, not
divergences. Corpus: no consumer yet, no transitions.

### 2. `is_discriminant_property` (`relater.go:1087`)

Computed from constituents rather than read off a synthetic union property
— this port builds no `createUnionOrIntersectionProperty` symbol and has no
`CheckFlags`. The three conditions are transcribed: some constituent has
the property, the types are non-uniform across those that do, at least one
is a literal type (`isLiteralType`, `checker.go:25393`, ported), and the
type is not generic (reduced to "not a type parameter"). Not transcribed:
the private/protected mismatch rule and `isPatternLiteralType`.

### 3. The pair (`flow.go:1436`, `:725`)

`get_discriminant_property_access` — candidate arm for an ACCESS whose
receiver is the reference only; the binding-pattern pseudo-reference arm
and the two `const` alias arms are not ported and decline.
`narrow_type_by_discriminant` — `filterType` by hand (the predicate needs
`&mut self`) over `are_types_comparable(narrowedPropType, discriminantType)`;
a constituent the relater declines in BOTH directions declines the WHOLE
narrowing, per `checker-notes-assign.md` §2's rule on acting on negatives.

**The first pair read 2 R→W** — `discriminatedUnionTypes2` f30,
`{ tag: true } | { tag: false } | { [x: string]: string }` under `if
(foo.tag)`. Upstream's `getTypeOfPropertyOfType(union, "tag")` is the
SYNTHETIC union property's type, which includes an index-signature
constituent's value type (`WritePartial`, `checker.go:21530`); this port's
§49 helper declines on any constituent lacking the name. Added
`union_property_type_for_discriminant` with the three upstream arms
(property / applicable index signature / `undefined` for an object literal)
and `None` for the `ReadPartial` case. 2 R→W → 0.

### Consumers

- The predicate arm (`flow.go:327`) — consumed, and DORMANT here: `tsc
  5.5.4` on `if (isCircleKind(o.kind)) o` does not narrow `o` (TS2339 on
  `o.r`), because `hasMatchingArgument` walks the REFERENCE for the
  argument, not the argument for the reference. The road fires only for
  the binding-pattern pseudo-reference (`getNarrowedTypeOfSymbol`), which
  this port lacks. Recorded so the next reader does not "fix" §749's
  `has_matching_argument` into the wrong direction.
- The truthiness arm (`flow.go:434`) — SWAPPED from §51.3's inline
  `filter_union_by_member_truthiness`. **Zero transitions over the corpus**:
  the pair reproduces the inline arm exactly. The inline function stays
  for the §84 sibling-of-pattern arm, which is the pseudo-reference road's
  stand-in.

### Measured

```
scorepair over the §749 baseline (435,823):
  step 1 alone         no transitions
  steps 1-3 + swap     first pair 2 R→W (discriminatedUnionTypes2), then
                       no transitions after the union-property fix
coverage               below (§7 row)
```

### Residue

- The equality (`:496`), typeof (`:623`) and switch (`:1229`) arms still
  run their inline forms; each is one swap-and-measure landing, and the
  equality one is where `comparable_ternary`'s enum arms (§666) must be
  carried by the relater's simple arms first (`relater.go:266-270`, the
  `number → enum` pair — not ported, the relater comment "there are no
  enum types in this crate" is stale).
- `is_nullable_type` / `is_discriminant_property` as flag computations
  rather than fact/CheckFlags reads.

## §751 — the relater's enum simple arms, on this port's member model (+25 W→R / +16 G→R, ZERO adverse)

§750's residue and §4.-4's precondition for the equality-arm swap.

### The model gap

Upstream's enum member is a literal type carrying `NumberLiteral |
EnumLiteral` (or `StringLiteral | EnumLiteral`) with its value on the type;
its simple arms compare `source.AsLiteralType().value`. This port mints a
member as a `Named` type flagged `ENUM` (§55, `declared.rs`) and keeps the
value only as the KEY it was interned under (`enum_value_types[(enum symbol,
"n:1" | "s:x")]`). So the arms are transcribed against two adapters:
`enum_member_value(member)` reads the key back (fresh→regular twin first,
then a linear scan of the map — enums are small and the relater's simple
arms are the sole caller), and `plain_literal_key(literal)` spells a plain
literal the same way, so the two compare directly.

### The arms (`relater.rs`, `is_simple_type_related_to`)

Placed BEFORE the `NumberLike → number` arm — that arm's `ENUM` bit
(upstream's `TypeFlagsNumberLike` includes `Enum`) would otherwise relate a
STRING-valued member to `number`, which the test
`a_string_enum_member_is_a_string_and_not_a_number` reddens on reorder.

- `:219`/`:225` — an enum literal relates to the PLAIN literal of its value.
- `NumberLike → number` / `StringLike → string` — by the member's actual
  domain (`n:` / `s:`), decided both ways.
- `:236-243` — two members of one enum relate only by identity; members of
  two enums with different names are decided unrelated; the same name
  (`isEnumTypeRelatedTo`, merged declarations) is `None`, explicitly.
- `:266-270`, assignable and comparable only — `number` relates to a numeric
  member, and a non-enum numeric literal to the member holding its value.
  The enum's UNION type reaches these through the composite dispatch, one
  member at a time, exactly as upstream's `typeRelatedToSomeType` does.
- `FLAG_DECIDABLE` gains `ENUM`: a non-firing arm on a member is now an
  answer. Its doc comment is rewritten; the old one listed the enum arms
  among the unported.

### Measured

```
scorepair over the §750 baseline (435,823):
  right 435,823 → 435,864   +25 W→R / +16 G→R, ZERO adverse
    enumAssignabilityInInheritance 19, enumLiteralTypes1 8,
    enumLiteralTypes2 8, typeAliases 4, genericCallWithGenericSignatureArguments3 2
coverage: checker_types 6,091 unmoved (gradient 91.01 → 91.02);
          diagnostics 2,600 unmoved; every other suite identical
```

### The harness finding

`tests/relater.rs`'s bare harness (parse → bind → `Checker::new`, no
program) resolves `let a: E.A` to `errorType`: the qualified type-reference
road resolves its namespace root only once the enum's declared type exists,
and nothing in the harness asks for it first. The corpus pipeline never
meets that order (the probe prints `E.A`), so the tests read the member
through `const a = E.A`'s initializer instead — `declaration_type`, which
documents why. A single-member enum's access spells the ENUM (§55.1's
`enum_access_spelling`), so the string fixture carries two members.

### Residue

- `comparable_ternary`'s enum half (§666) is now redundant with the relater
  and can go when the equality arm is swapped.
- `n:{n}` formatting: `MemberValue::Num` prints through `Display`; a
  fractional or negative member value compares against the literal's
  printed text, which is the same spelling for the values the corpus holds
  but is not a proven identity.

## §752 — the equality arm onto the discriminant pair (+56 W→R / +9 G→R, ZERO adverse)

§4.-4's second swap, and the first one that moved. §750 built the pair
(`getDiscriminantPropertyAccess` / `narrowTypeByDiscriminant`) and swapped the
TRUTHINESS arm onto it at zero movement; §751 gave the relater its enum simple
arms, which is what the equality arm's inner `narrowTypeByEquality` needed
before the swap could be anything but a downgrade.

### What was there, and why it was replaced whole

§51.1 (`checker-notes-narrow.md`) carried its own inline test: find an operand
that is a `PropertyAccessExpression` whose receiver `is_matching_reference`,
take the identifier name, check the other operand types to a UNIT, then filter
with `filter_union_by_member_literal` over `comparable_ternary`. Upstream has
no such function. It has `flow.go:496`-`:503` — two calls to
`getDiscriminantPropertyAccess`, one per operand order, each feeding
`narrowTypeByDiscriminantProperty`.

The inline form is not a smaller version of that; it is a different predicate
that happens to agree on the common case. Three things it could not do:

- **Element access.** `u["kind"] === "a"` is an `ElementAccessExpression`, so
  the match failed and the union was left whole.
  `getAccessedPropertyName` (`flow.go:1727`) takes both forms, and
  `compiler/discriminantElementAccessCheck` is the corpus case that wants it.
- **The receiver's nullable strip.** `narrowTypeByDiscriminant` removes
  `undefined`/`null` from the RECEIVER when the access is an optional chain or
  a non-null assertion, before reading the property.
  `conformance/controlFlowOptionalChain2` wants that.
- **A non-UNIT comparand.** The inline test required
  `literal_type.intersects(UNIT)`; the pair hands the property type to
  `narrowTypeByEquality`, which decides for itself.

It also gated on nothing resembling `isDiscriminantProperty`, so it would
narrow on a uniform property where upstream declines. That direction cost
nothing measurable here, but it is the reason the swap is a *replacement* and
not an *addition*: running both would keep the looser predicate alive.

### The port

`narrow_type_by_discriminant_property` (`flow.go:702`), and at the arm, the two
operand orders in upstream's order — `leftAccess` then `rightAccess`, the OTHER
operand as the value.

The inner closure passes `chain_strips_nullable: false`. Upstream's
`narrowTypeByEquality` has no such parameter; it is this port's SS151 addition,
correct at the TOP-level equality arm where the reference itself is the chain.
Inside the pair the strip has already happened, on the receiver, in
`narrow_type_by_discriminant` — passing it again would strip twice.

**The key-property fast path (`:703`-`:719`) is not ported.** It needs
`getKeyPropertyName` and `getConstituentTypeForKeyType`, and this port interns
no key-property index for a union. (**§757 refuses it outright and corrects
this paragraph**: the path is gated at TEN constituents, so upstream itself
takes the general road for every union the corpus's discriminated shapes have.
Nothing is lost here.) Declining it is upstream's own behaviour for
every union whose `keyPropertyName` is empty; for a union that has one, the
general filter reaches the same constituent by comparability instead of by
lookup. What is actually given up is the `removeType` shortcut on the negative
`===` branch and the O(1) — an optimisation with one behavioural edge, not a
mechanism the arm depends on.

`filter_union_by_member_literal` stays: the §50 sibling-pattern arm is its
other caller, and that arm is a pseudo-reference road the pair does not serve.

### Measured

```
scorepair over the §751 baseline (435,864):
  right 435,864 → 435,929   +56 W→R / +9 G→R, ZERO adverse
    staticAnonymousTypeNotReferencingTypeParameter 15,
    controlFlowOptionalChain2 12+2, discriminantElementAccessCheck 10+4,
    typeGuardIntersectionTypes 2
coverage: checker_types 6,091 → 6,092 (63.86% → 63.87%),
          gradient 91.02% → 91.04%; diagnostics 2,600 unmoved;
          every other suite identical
```

**A note on the baseline.** `target/verdict_baseline.tsv` was found at §750,
not §751 — the previous session landed without `--accept`. A control run on the
clean §751 tree reproduced §751's published matrix exactly (+25 W→R / +16 G→R),
which both confirmed §751's numbers independently and stopped §752 from being
measured against a stale reference and reported as +81. `conventions.md`
records this failure mode; it is the second time it has been met.

### How you would know this was wrong

A case that narrowed under §51.1's looser predicate and stops narrowing under
`isDiscriminantProperty` would show as R→W or R→G. None did at this corpus. If
one appears after a later change makes `is_discriminant_property` stricter, the
suspect is the `has_literal`/`non_uniform` pair, not this arm.

### Residue

- `comparable_ternary`'s enum half (§666) is now redundant with the relater in
  the EQUALITY road but still live in the §50 sibling arm and the switch arm;
  it can go when those two are swapped.
- The typeof arm (`flow.go:623`) and the switch arm (`:1229`) are the two swaps
  left in §4.-4's road, in that order.
- The key-property fast path, above, remains unported.

## §753 — the typeof arm's discriminant half (+4 W→R, ZERO adverse)

§4.-4's third swap. Unlike §752 this one is an ADDITION, not a replacement:
the port had no inline discriminant test at the typeof arm at all, so nothing
was removed and the measurement is the mechanism's own.

### The three halves of `narrowTypeByTypeof`

`flow.go:614` has one dispatch and three outcomes, and this port acquired them
in the wrong order over three separate sessions:

| half | upstream | ported at |
|---|---|---|
| the target IS the reference | `:632` | the SS-era arm |
| the target reaches it through a `?.` chain | `:621`-`:623` | SS152 |
| the target is a discriminant property access | `:624`-`:629` | **§753** |

The comment above the arm claimed the second and third were both unported
(`bd tsr-q9g`) — the second had in fact landed at SS152 and the comment was
never corrected. It is corrected now.

### The composition bug the swap also fixed

Upstream's chain half **assigns** and falls through:

```go
if c.strictNullChecks && c.optionalChainContainsReference(...) && ... {
    t = c.getAdjustedTypeWithFacts(t, TypeFactsNEUndefinedOrNull)
}
propertyAccess := c.getDiscriminantPropertyAccess(f, target, t)
```

This port **returned**. So `typeof o?.kind === "string"` stripped the
`undefined` and stopped, where upstream strips it and THEN filters the union by
the discriminant. This is the same composition §51.5 had already restored at
the value-equality arm (`flow.go:491`), met a second time at a second arm — a
sign worth noting: an early `return` where upstream assigns is a *shape* this
port has now got wrong twice, and the remaining arms should be read for it
rather than waited on.

`compiler/narrowingTypeofDiscriminant` holds both forms as `f1` (plain access)
and `f2` (chain), which is why the case moves by 4 and not by 2.

### The port

The inner narrowing is `narrowTypeByLiteralExpression` — the SAME typeof filter
the matching-reference half uses, applied to the PROPERTY type instead of the
reference's. That is the whole content of the half; everything else is the pair,
already built at §750.

### Measured

```
scorepair over the §752 baseline (435,929):
  right 435,929 → 435,933   +4 W→R, ZERO adverse
    narrowingTypeofDiscriminant 4
coverage: checker_types 6,092 unmoved (the case has other defects),
          gradient 91.04% unmoved at this rounding;
          diagnostics 2,600 unmoved; every other suite identical
```

**+0 cases is the honest headline.** The four lines are all in one case, and
that case does not convert — `casequery` would name what else blocks it. This
is §4.-3's tail behaving exactly as §4.-3 says it does, and the swap is worth
landing anyway because it is upstream's shape and because the composition bug
it removes is not confined to the case that exposed it.

### How you would know this was wrong

The chain half now runs `is_discriminant_property` on a type it has already
stripped. If a later change makes the strip lossy, a `typeof o?.kind` guard
would start declining where it used to narrow — visible as R→W on
`narrowingTypeofDiscriminant`'s `f2` lines specifically.

### Residue

- The SWITCH arm (`flow.go:1229`) is the last of §4.-4's five, and the only one
  left where an inline test still stands (`narrow_union_by_member_switch`).
- `comparable_ternary`'s enum half (§666) survives in the §50 sibling arm and
  the switch arm; the switch swap is what retires it.

## §754 — the switch arm onto the pair, and the compose bug's third sighting (+60 W→R / +16 G→R, ZERO adverse)

The last of §4.-4's five swaps, and the largest. `getTypeAtSwitchClause`
(`flow.go:1059`) now has all four of upstream's arms.

### Two changes, not one

**The swap.** §51's inline test — a `PropertyAccessExpression` whose receiver
is the reference, then `narrow_union_by_member_switch` — is replaced by
`getDiscriminantPropertyAccess` + `narrowTypeByDiscriminant` with
`narrowTypeBySwitchOnDiscriminant` as the inner narrowing
(`flow.go:1083`-`:1086`). The gain is the same shape §752 measured at the
equality arm: element access, the receiver's nullable strip, and the
`isDiscriminantProperty` gate in place of a looser ad-hoc one.

**The compose bug, for the third time.** Upstream's default arm ASSIGNS the
optional-chain containment result and falls through to the discriminant road:

```go
if c.strictNullChecks { ... t = c.narrowTypeBySwitchOptionalChainContainment(...) }
access := c.getDiscriminantPropertyAccess(f, expr, t)
if access != nil { t = c.narrowTypeBySwitchOnDiscriminantProperty(t, access, data) }
```

This port had the containment in an `else if` chain that RETURNED. So
`switch (o?.kind)` stripped the `undefined` and then declined to discriminate.

**That is the third arm with this exact shape** — §51.5 found it at the
value-equality arm, §753 at the typeof arm, §754 here. §753's note said the
remaining arms should be READ for it rather than waited on; they were, and it
was there. The pattern is worth stating plainly for whoever ports the next
narrowing arm: **upstream's narrowing arms accumulate into `t` and fall
through; a port that returns early from any intermediate step silently drops
every later step.** Three of the four arms that could have had this defect did
have it.

### Placement

The discriminant road moved from BEFORE the matching-reference arm (where §51
had put it) into the DEFAULT arm, where upstream has it. That reordering is
behaviour-neutral in practice — a property access whose receiver is the
reference is not itself the reference — but the port now reads in upstream's
order, which is what makes drift trackable.

### Not ported

- `narrowTypeBySwitchOnDiscriminantProperty`'s key-property fast path
  (`flow.go:1232`-`:1245`), for the reason §752 records — **and refused
  outright at §757**, which reads the ≥10-constituent gate that makes it a
  large-union index rather than a narrowing rule.
- The second containment variant, `typeof`-of-a-chain (`flow.go:1077`-`:1080`),
  whose clause check is `!(never || the string literal "undefined")` rather
  than `!(undefined || never)`. It declines here; it is a separate small item.

`narrow_union_by_member_switch` stays: §50.1's sibling-pattern arm is its other
caller, exactly as `filter_union_by_member_literal` survived §752.

### Measured

```
scorepair over the §753 baseline (435,933):
  right 435,933 → 436,009   +60 W→R / +16 G→R, ZERO adverse
    typeGuardNarrowsIndexedAccessOfKnownProperty1 27+12,
    discriminantElementAccessCheck 12+4, controlFlowOptionalChain 9
coverage: checker_types 6,092 → 6,093 (63.87% → 63.88%),
          gradient 91.04% → 91.05%; diagnostics 2,600 unmoved;
          every other suite identical
```

`typeGuardNarrowsIndexedAccessOfKnownProperty1` at 39 lines is the largest
single-case movement of the four landings, and it is an ELEMENT-access case —
the road §51's inline test could not reach at all.

### The road, closed

§4.-4 is complete. Five arms, four landings, one shared pair:

| arm | landed | movement |
|---|---|---|
| type predicate | §750 | new road, 0 |
| truthiness | §750 | 0 |
| equality | §752 | +65 |
| typeof | §753 | +4 |
| switch | §754 | +76 |

**+145 right lines and +2 cases for the subsystem**, against §4.-4's own
forecast that it would need five landings to pay for itself. It needed four,
and the falsifier it set (fewer than ~20 cases blocked on discriminant shapes)
was never triggered.

### Residue

- `comparable_ternary`'s enum half (§666) is now redundant everywhere the pair
  runs, but §50/§50.1's sibling-pattern arms still use `comparable_ternary`
  through `filter_union_by_member_literal` and `narrow_union_by_member_switch`.
  Retiring it means porting the pseudo-reference road
  (`getCandidateDiscriminantPropertyAccess`'s binding-pattern and alias arms,
  `flow.go:1457`), which is the natural successor item to this road.
- `getKeyPropertyName` / `getConstituentTypeForKeyType` remain unported, now
  wanted by two call sites rather than one. **Closed at §757: refused, with the
  ≥10-constituent gate as the number.**

## §755 — the const-alias discriminant (`const k = u.kind`) (+32 W→R / +14 G→R, ZERO adverse)

§754's named successor, and the first item on the PSEUDO-REFERENCE road. It is
also the cheapest thing on that road, which is why it went first.

### The shape

`getDiscriminantPropertyAccess` (`flow.go:1436`) does two things: find a
CANDIDATE access, then check the property is a discriminant. §750 ported the
second half properly and inlined a one-arm version of the first. §755 splits
`getCandidateDiscriminantPropertyAccess` (`:1457`) out to match upstream, which
is what let a second arm be added beside the first rather than bolted onto the
caller.

Upstream has three arms; two are now ported:

- **The access arm** (`:1468`-`:1472`) — an access whose receiver is the
  reference. What §750 had.
- **The alias arm** (`:1473`-`:1482`, first half) — an identifier bound to a
  CONST whose initializer is an access on the reference. `const k = u.kind`,
  then `if (k === "a")` narrows `u`.

The alias arm returns the **initializer**, not the identifier. That is the
detail that makes it cheap: everything downstream —
`getAccessedPropertyName`, `isDiscriminantProperty`, `narrowTypeByDiscriminant`
— receives an ordinary access expression and needs no new arm. All four of
§4.-4's swapped arms picked the alias up for free, which is why one 40-line
change moves 46 lines.

### The annotation guard is load-bearing

`getCandidateVariableDeclarationInitializer` (`flow.go:1495`) returns the
initializer only of an **unannotated** declaration. `const k: "a" | "b" =
u.kind` is typed by its annotation, not by the access, so treating `k` as an
alias for `u.kind` would narrow on a relationship the annotation may have
broken. The test asserts the annotated form does NOT narrow, and removing the
`declaration.r#type.is_some()` check reddens exactly that assertion — verified,
not assumed.

`isConstantVariable` does the other half: a `let` alias can be reassigned
between the alias and the guard, so it is not an alias.

### Still not ported

- The binding-pattern/function pseudo-reference arm (`:1459`-`:1467`). This
  port models that road through `state.discriminant_pattern` (§50) rather than
  by letting a binding pattern stand as the reference, so the arm has no
  counterpart to test against. Retiring `comparable_ternary` still waits on
  reconciling those two models — §754's residue stands.
- The `const { kind: x } = obj` half (`:1483`-`:1489`). It returns a BINDING
  ELEMENT as the candidate, so it needs `getAccessedPropertyName`'s
  binding-element arm (`flow.go:1727`), which belongs to the same
  pseudo-reference road. This is the next cheapest item on it.

### Measured

```
scorepair over the §754 baseline (436,009):
  right 436,009 → 436,055   +32 W→R / +14 G→R, ZERO adverse
    controlFlowAliasing2 26+14, controlFlowAliasing 6
coverage: checker_types 6,093 unmoved, gradient 91.05% → 91.06%;
          diagnostics 2,600 unmoved; every other suite identical
```

Both cases are named for the mechanism, which is the cleanest attribution any
landing in this block got. **+0 cases**: `controlFlowAliasing2` moves 40 of its
lines and still does not pass, so the rest of it is blocked elsewhere —
`casequery` would name it, and that is the natural probe for whoever takes the
binding-element half.

### How you would know this was wrong

An alias narrowing where the alias and the guard straddle an assignment to the
receiver would be a wrong answer, not a missing one. `isConstantVariable` only
proves the ALIAS is constant, not the receiver; upstream relies on the flow
walk to catch the receiver's assignment, and this port's
`an_assignment_to_the_receiver_resets_the_property_narrowing` test covers the
non-alias form. If that test's alias twin ever reds, this arm is the suspect.

## §756 — the destructuring alias (`const { kind } = u`), and two predicates that were wrong (+51 W→R / +14 G→R, ZERO adverse)

§755's named next item, and it needed two fidelity repairs first. Both repairs
measured **ZERO** on their own; the +65 is the arm's.

### The two predicates

**`combined_node_flags` was missing `GetRootDeclaration`.** Upstream's
`getCombinedFlags` (`ast/utilities.go:1181`) *begins* by walking a binding
element out through its pattern to the owning declaration (`:1173`). This port
started at the node itself, so a destructured `const { kind } = u` never saw
the CONST flag — the binding element's parent is the pattern, not the
declaration list. The walk existed in the port already, privately, in
`unused.rs`; the shared helper simply did not have it.

**`is_constant_variable` reimplemented the flag lookup by hand, and got it
wrong three ways.** Upstream is
`symbol.Flags&Variable != 0 && getDeclarationNodeFlagsFromSymbol(symbol)&Constant != 0`
(`utilities.go:1040`), where the lookup is `getCombinedNodeFlags` on the value
declaration. This port walked exactly one parent, demanded a
`VariableDeclarationList`, and tested `NodeFlags::CONST`. So it answered
`false` for every destructured const, missed `using` (upstream's `Constant` is
`CONST | USING`, and this port's `NodeFlags::CONSTANT` already spelled that),
and never checked the symbol was a VARIABLE at all.

`is_constant_variable` has five call sites. **Fixing it moved nothing** — the
full corpus reported `no transitions vs baseline`. That is worth recording
rather than skipping: it is the control that makes the arm's +65 attributable
to the arm, and it says the other four call sites were not being reached with
destructured consts.

### The arm

`flow.go:1483`-`:1489`. Where §755's half matched an access's RECEIVER against
the reference, this one matches the variable declaration's RHS **whole** — the
reference is `u` in `const { kind } = u`, not the receiver of anything. The
candidate returned is the binding **element**, not an access, which is why this
half needs `getAccessedPropertyName`'s binding-element arm and §755's did not.

`getDestructuringPropertyName` (`flow.go:1792`) is ported for its OBJECT-pattern
arm: the name is `getBindingElementPropertyName` = `PropertyNameOrName()`
(`utilities.go:1081`), so the shorthand `{ kind }` and the renamed
`{ kind: k }` both answer `kind` — **the property name discriminates, not the
local name**. The ARRAY-pattern arm (`:1800`), which answers the element's
index, is left with the parameter arm on the pseudo-reference road.

### Measured

```
scorepair over the §755 baseline (436,055):
  the two predicate fixes alone:  no transitions vs baseline
  + the binding-element arm:      right 436,055 → 436,120
                                  +51 W→R / +14 G→R, ZERO adverse
    controlFlowAliasing 25, controlFlowAliasing2 26+14
coverage: checker_types 6,093 → 6,094 (63.88% → 63.89%),
          gradient 91.06% → 91.08%; diagnostics 2,600 unmoved;
          every other suite identical
```

### The pseudo-reference road, as it now stands

One arm left: the binding-pattern/function pseudo-reference arm
(`flow.go:1459`-`:1467`). It is **not** a transcription job. Upstream lets a
binding pattern or arrow function stand as `f.reference` and narrows a
pseudo-reference in `getNarrowedTypeOfSymbol`; this port carries
`state.discriminant_pattern` (§50) and its own sibling test instead. The two
models have to be reconciled before the arm has anything to be checked
against, and that reconciliation is also what retiring `comparable_ternary`
waits on — §50/§50.1's arms are its last callers.

**So the cheap half of this road is now spent.** §755 and §756 took +111 right
lines and +1 case between them for about eighty lines of code; what remains is
a model-reconciliation task and should be priced as one, not as a third alias
arm.

### How you would know this was wrong

`is_constant_variable` now answers `true` for `using` declarations and for
destructured consts at four call sites that never saw them before. The corpus
says none of those four is reached differently today. If a later landing makes
one of them reachable — anything that widens what counts as a reference — the
suspects are `flow.rs:1131` and `:1157`, not this arm.

## §757 — the key-property fast path, REFUSED with a number, and a correction to §752/§754

§752 and §754 each declined `narrowTypeByDiscriminantProperty`'s and
`narrowTypeBySwitchOnDiscriminantProperty`'s key-property fast path
(`getKeyPropertyName` / `getConstituentTypeForKeyType`), and each described it
as "an optimisation with one behavioural edge". **That description was too
generous, and it invited a future session to port it.** It is read now, and it
is refused with a number.

### The number

`computeKeyPropertyNameAndMap` (`relater.go:1139`) answers
`InternalSymbolNameMissing` — i.e. the fast path does not run — unless **all**
of these hold:

- the union has **at least 10 constituents** (`:1141`);
- at least 10 of them are object or instantiable-non-primitive (`:1141`);
- every constituent's key property is a **literal** type, or the map is
  abandoned entirely (`:1182`-`:1184`);
- at least **10** constituents have a UNIQUE key, and those are at least
  **50%** of the union (`:1200`).

So `getKeyPropertyName` returns `""` for every union of fewer than ten
constituents, and `narrowTypeByDiscriminantProperty` falls through to
`narrowTypeByDiscriminant` — **which is exactly what this port already does, at
every call site, for every union the corpus's discriminated shapes actually
have.** It is a lookup table built to keep large unions off an O(n) walk, not a
narrowing rule.

### What this port gives up by refusing it

Two things, both bounded:

- **Performance on 10+-constituent discriminated unions.** This port walks and
  compares where upstream would index. It has not been measured as a problem
  and there is no benchmark asking the question.
- **One behavioural edge**: on the NEGATIVE `===` branch, upstream's fast path
  does `removeType(t, candidate)` when the candidate's key property is a unit
  type (`flow.go:712`-`:714`), where the general filter re-tests every
  constituent for comparability. These agree whenever the key property really
  is a unique unit discriminant — which the ≥10/50% gate has already proved
  before the shortcut is reachable.

### The correction

§752's and §754's sections say the fast path is "not ported" and give the
reason as the missing helpers. That is true but incomplete: **a reader could
come away thinking a case is being lost.** None is. The right summary, and the
one those two sections should be read with, is: *upstream takes the general
path this port takes, for every union smaller than ten constituents; the fast
path is a large-union index and porting it is a performance task with a
benchmark attached, not a conformance task.*

**Refused. Do not port it as part of the discriminant subsystem.** Reopen it
only with a profile showing the general walk is hot on a real program — and
note that this port's `union_property_type_for_discriminant` and
`discriminant_keeps` would each need the same index to benefit, so the item is
larger than the two functions it names.


## §758 — `narrowTypeByOptionality`, and the §85 divergence it exposed (+7 W→R, ZERO adverse)

Acting on §754's own instruction — *read the remaining narrowing arms rather
than wait for them* — a sweep of upstream's `narrowTypeBy*` list against the
port found one function missing **entirely**, and porting it surfaced a latent
bug in a different subsystem.

### The missing arm

`narrowType` (`flow.go:377`) has a branch BEFORE its kind dispatch
(`:378`-`:381`): for `a?.b`'s chain root and for `a ?? b`'s left operand,
upstream emulates a synthetic `a !== null && a !== undefined` condition and
calls `narrowTypeByOptionality` (`:415`). This port had **no such branch and no
such function** — those expressions fell through to the ordinary truthiness
road.

The distinction is real: truthiness also removes `""`, `0` and `false`, and the
nullish operators do not. Both halves of the function are ported — the matching
reference, and the discriminant property through the §750 pair.

### The divergence it exposed

The first measurement was **+1 W→R against 1 R→W** — a net zero with an
adverse, which is a refusal under this project's rules. The adverse was
`compiler/nonNullableTypes1:0:32`: `obj` in `if (obj?.x === "hello")` went from
`NonNullable<T>` to `T & {}`.

Cause: the port's `get_type_with_facts` spells the `NonNullable<T>` utility for
`TRUTHY` only, and the intersection mint `T & {}` for the NE-family — §85.1's
explicit claim. **Upstream does not split them.**
`getAdjustedTypeWithFacts` (`checker.go:31159`) maps surviving constituents
through `getGlobalNonNullableTypeInstantiation` for `NEUndefinedOrNull` **and**
`Truthy`. §758 was the first thing to route a type parameter through the
NE-family at that position, so the divergence had been latent.

Corrected for TYPE PARAMETERS only. Measured **alone**, on a clean tree without
the new arm: **+6 W→R, ZERO adverse** (`unknownControlFlow` 4,
`nonNullableTypes1` 2). `unknown` is untouched and keeps the filter road —
`narrowingTruthyObject`'s 15 R→G still gates it, and that half of §85.1 stands.
`checker-notes-narrow.md` §85.1 is half-superseded in place.

**This is the second time this session that an "adverse transition" was a
correct change meeting a pre-existing bug rather than a defect in the change.**
The habit worth keeping: when a faithful transcription produces exactly one
adverse, read the adverse before pricing the transcription.

### Measured

```
scorepair over the §756 baseline (436,120):
  the §85 correction alone:   +6 W→R, ZERO adverse
  + narrowTypeByOptionality:  right 436,120 → 436,127
                              +7 W→R, ZERO adverse
    unknownControlFlow 4, nonNullableTypes1 2,
    nullishCoalescingOperator11 1
coverage: checker_types 6,094 → 6,095 (63.89% → 63.90%),
          gradient 91.08% unmoved at this rounding;
          diagnostics 2,600 unmoved; every other suite identical
```

### What the unit tests do and do not pin

The type-parameter spelling has a real test that reddens on revert. **The
optionality arm does not, and its test says so.** The arm's only
corpus-visible effect is the `??` left operand narrowed inside the RIGHT
operand to its NULLISH part rather than its falsy part
(`nullishCoalescingOperator11`, 1 line), and `type_of_last_expression` cannot
see a sub-expression. A fixture that would expose it needs overload resolution
the port currently answers `error` for. The corpus case is the guard; the test
pins the chain-root answers the arm must not break, and is labelled as a
fidelity pin rather than a behaviour test.

That is worth stating rather than quietly shipping a green test: **a test that
passes both with and without the change is not a regression guard**, and this
file should not imply otherwise.

### Residue

- `getAdjustedTypeWithFacts`'s OTHER extra — recombining `unknown` into
  `unknownUnionType` before filtering — is still unported, and is still
  described accurately at `flow.rs`'s `narrow_type_by_call_expression` note.
- The port still has one `get_type_with_facts` where upstream distinguishes
  adjusted from unadjusted. §85's mint machinery sits inside the unadjusted
  one, which is why the two kept drifting; separating them is the real fix and
  is not attempted here.

## §759 — `narrowTypeByBooleanComparison`, a whole arm that was missing (+13 W→R / +1 G→R, ZERO adverse)

The §758 sweep's second finding. Comparing upstream's `narrowTypeBy*` list
against this port's function names, five had no counterpart:
`narrowTypeByConstructor`, `narrowTypeByBooleanComparison`,
`narrowTypeByPrivateIdentifierInInExpression`, `narrowTypeByInstanceof`,
`narrowTypeByOptionalChainContainment`. Three of those turned out to be
**ported inline under different names** (instanceof at §83, the containments at
§51.4 and SS154). Two were genuinely absent. This is one of them.

### What it does

`flow.go:510`-`:515`, the last two arms of the equality dispatch:

```go
if ast.IsBooleanLiteral(right) && !ast.IsAccessExpression(left) {
    return c.narrowTypeByBooleanComparison(f, t, left, right, operator, assumeTrue)
}
```

and the function itself (`:806`) is three lines: fold the literal's polarity
and the operator's negation into `assumeTrue`, then **re-enter `narrowType` on
the other operand**. So any condition that narrows on its own keeps narrowing
when it is compared to a boolean literal — `isA(x) === true` narrows exactly as
`isA(x)` does, and `isA(x) === false` as `!isA(x)`.

Without it, `if (isA(x) !== true)` narrowed nothing at all.

### Two details that are easy to get wrong

**The fold is a three-way XOR**, transcribed rather than simplified:

```
assumeTrue = (assumeTrue != isTrueKeyword) != (operator is an equality operator)
```

Four combinations, and getting one backwards is this arm's failure mode — so
the test asserts all four plus a loose operator, and flipping either sign in
the expression reddens it. That was checked, not assumed.

**The other operand must not be an access expression** (`!IsAccessExpression`).
An access beside a boolean literal is a DISCRIMINANT comparison — `o.flag ===
true` — and has already been offered to the §752 pair earlier in the dispatch.
Without that guard this arm would take it back and narrow the wrong thing.

**And the operand must not be the matching reference**: `x === true` is
answered by `narrowTypeByEquality` at `:483`, before this arm ever runs, and
filters by comparability. The port's order already matched upstream's here, but
the first draft of the test asserted the wrong shape and was corrected against
`compiler/narrowByBooleanComparison` rather than against intuition.

### Measured

```
scorepair over the §758 baseline (436,127):
  right 436,127 → 436,141   +13 W→R / +1 G→R, ZERO adverse
    narrowByBooleanComparison 13+1
coverage: checker_types 6,095 unmoved, gradient 91.08% unmoved at this
          rounding; diagnostics 2,600 unmoved; every other suite identical
```

Every moved line is in the one case named for the mechanism, and that case does
not convert — so this is a +0-case landing, recorded as such.

### Residue

`narrowTypeByConstructor` (`flow.go:760`) is the other genuinely-absent arm:
`x.constructor === C`. It needs `isMatchingConstructorReference`
(`getReferenceCandidate` on a `.constructor` access whose receiver is the
reference) and `getNarrowedTypeOfSymbol`'s class-instance handling. Not
attempted here; it is the next item on this sweep and is bigger than this one.

## §760 — `narrowTypeByConstructor`, and two symbol-vs-type-road traps (+65 W→R, ZERO adverse, +3 cases)

The §758 sweep's last genuinely-absent arm, and the largest single landing of
this block. `x.constructor === C` keeps the constituents CONSTRUCTED BY `C`.

### Sized before it was written

The sweep named the arm; `casequery` priced it. All five
`typeGuardConstructor*` fixtures were failing, with line deficits of 4, 40, 6,
12 and 11 — about 73 lines available. That is why this arm was worth writing
where §759's (one case, 14 lines) was borderline. **Price the fixtures before
transcribing the function**; the two arms came out of the same sweep and are an
order of magnitude apart in value.

### The two traps

The first draft measured **+13 and left 52 lines on the table** — the two CLASS
fixtures did not move at all while the primitive ones did. Both causes were the
same mistake in different clothes: **reaching for a symbol where this port
keeps a type.**

- **`prototype` is synthetic here.** §117 slice 2 (`members.rs:956`) mints a
  class's static `.prototype` **on the type road only**
  (`get_type_of_property_of_type`); there is no symbol for it. The draft used
  `get_property_of_type(...).map(get_type_of_symbol)`, which answers `None` for
  exactly the class case the arm is most wanted for. The §83 instanceof arm
  takes the symbol road and survives only because it has an
  erased-construct-return fallback that `narrowTypeByConstructor` does not.
- **A class's constructor type is `Anonymous`, not `Named`.** The
  `isFunctionType || isConstructorType` guard was transcribed through
  `signature_candidates_of_named_type`, which — as its name says — answers only
  for `Named` types. So the guard rejected every class. A class's static side
  IS a constructor type upstream, and the guard now says so.

Neither was visible from the corpus number alone; both were found by writing
the unit test, watching the class assertion fail, and bisecting the guards.
**A +13 that should have been +65 looks exactly like a +13.**

### `isConstructedBy` and the ObjectFlags gap

`isConstructedBy` (`flow.go:793`) checks **symbol identity, not structure**,
when either side is a class: two classes with identical members are the same
type structurally, but `instanceOfA.constructor === B` is false. Upstream tests
`ObjectFlagsClass`; this port carries no `ObjectFlags`, so `class_symbol_of`
stands in — a `Named` type whose owning symbol has `SymbolFlags::CLASS`. That
is the same question asked of the data this port does keep, and
`typeGuardConstructorDerivedClass` (12 lines, a base/derived pair that must NOT
collapse) is what would catch it being wrong.

### Inequality does not narrow

`flow.go:762` declines unless the operator is `==`/`===` in the true branch or
`!=`/`!==` in the false one — `x.constructor !== C` does not prove the
constituent is not a SUBCLASS of `C`. Asserted in the test.

### Measured

```
scorepair over the §759 baseline (436,141):
  first draft (symbol road):  +13 W→R  — both class fixtures unmoved
  corrected (type road):      right 436,141 → 436,206
                              +65 W→R, ZERO adverse
    typeGuardConstructorClassAndNumber 40,
    typeGuardConstructorDerivedClass 12,
    typeGuardConstructorNarrowAny 6, PrimitiveTypes 6,
    NarrowPrimitivesInUnion 1
coverage: checker_types 6,095 → 6,098 (63.90% → 63.93%),
          gradient 91.08% → 91.09%; diagnostics 2,600 unmoved;
          every other suite identical
```

**+3 cases**, the largest case movement of this block.

### The harness limit, again

The unit test asserts the CLASS road only. The primitive road needs `String`
and `Number` from the lib and this harness builds no program — the same
limitation §751 recorded for `let a: E.A`. The test says so rather than
quietly covering half the arm, and the corpus carries the primitive road
(`typeGuardConstructorPrimitiveTypes`, `typeGuardConstructorNarrowAny`).

### The sweep, closed

Of the five upstream `narrowTypeBy*` functions with no counterpart NAME here:
three were ported inline under other names (instanceof §83, the two
containments §51.4/SS154), and two were genuinely absent — §759 and §760. The
sweep is spent, and it was worth **+79 right lines and +3 cases** for two
functions totalling about 150 lines.

## §761 — the switch typeof-chain containment, §754's residue closed (+2 W→R, ZERO adverse)

`flow.go:1077`-`:1080`. `getTypeAtSwitchClause`'s default arm has **two**
optional-chain containments, not one, and §754 ported only the direct form and
recorded the other as residue. This closes it.

### Why it is a separate arm and not a flag

The two variants differ in their CLAUSE CHECK, and the difference is not
cosmetic:

| variant | expression | clause check |
|---|---|---|
| direct (`:1074`) | `switch (o?.kind)` | no clause type is nullish |
| typeof (`:1078`) | `switch (typeof o?.x)` | no clause type is `never` or the **string literal `"undefined"`** |

Under `switch (typeof o?.x)` every clause type is a string literal, so the
direct form's nullish test can never fire — reusing it would strip the base
unconditionally, including for `case "undefined":` where the chain is *not*
proved defined. The test asserts exactly that case, and swapping in
`switch_clause_range_covers_nullish` reddens it. Checked, not assumed.

### Measured

```
scorepair over the §760 baseline (436,206):
  right 436,206 → 436,208   +2 W→R, ZERO adverse
    controlFlowOptionalChain 2
coverage: checker_types 6,098 unmoved, gradient 91.09% unmoved;
          diagnostics 2,600 unmoved; every other suite identical
```

Two lines. It is landed because it is upstream's shape and because the residue
note that named it would otherwise stay on the page indefinitely — but it is
also a fair illustration of where this seam now is: **the arms are ported, and
what is left of them is worth single digits.**

### `getTypeAtSwitchClause`, complete

All four arms and both containments are now ported. The one thing this port
does not have is the pseudo-reference road (§50's `discriminant_pattern` model
versus upstream's binding-pattern-as-reference), which is unchanged by this and
remains the subsystem's last open item.

## §762 — the pseudo-reference road, reconciled (+2 W→R, ZERO adverse, +1 case, −226 lines)

The discriminant subsystem's last open item, carried as residue since §754 and
priced as a "model reconciliation, not a transcription". It was the second
thing, but the reconciliation turned out to be one function.

### The two models were isomorphic all along

Upstream makes the binding pattern **the reference**: `getNarrowedTypeOfSymbol`
(`checker.go:13723`) calls `getFlowTypeOfReferenceEx(pattern, …)`, and
`getCandidateDiscriminantPropertyAccess`'s first arm (`flow.go:1459`) fires on
`IsBindingPattern(f.reference)`.

This port already had that walk — `narrow_destructured_parent` (§50) — but kept
the LOCATION as the reference and carried the pattern beside it in
`state.discriminant_pattern`. The same information, spelled differently. So
there was nothing to reconcile in the flow model at all: the join is one arm in
`get_candidate_discriminant_property_access` that reads
`state.discriminant_pattern` where upstream reads `f.reference`.

**The residue note was right that this was the open item and wrong about why it
was hard.** It was priced as a subsystem because the two models *looked*
different in the STATUS summary. Reading both put the estimate at one function.

### What made it cheap was already built

The arm returns the **declaration** — a binding element or a parameter — and
everything downstream needs `getAccessedPropertyName` to name it:

- the **binding-element** arm (`getDestructuringPropertyName`, `flow.go:1792`)
  landed at §756 for the `const { kind } = u` alias;
- the **parameter** arm (`flow.go:1735`-`:1737`, the parameter's INDEX in its
  list) is added here, and it is exactly §50.3's tuple-by-position rule moved
  from an inline site onto upstream's function.

So §756 had already paid most of this bill without either section knowing.

### Three inline arms deleted, not rewritten

With the candidate arm in place, §50 (equality), §84 (truthiness) and §50.1
(switch) each became a worse-gated duplicate of the pair, and each was removed
whole. Their four helpers went with them:

```
sibling_member_of_pattern            40 lines
narrow_union_by_member_switch        79
filter_union_by_member_truthiness    60
filter_union_by_member_literal       47
                                    226 lines deleted
```

Each removal was measured on its own and each was a **wash** — the pair answers
everything the inline arms answered, and answers it through
`isDiscriminantProperty` and the comparable relation rather than through
`comparable_ternary`.

One behavioural improvement rides along that the inline form could not have:
`sibling_member_of_pattern` returned the identifier's OWN text, so
`const { kind: k } = u` discriminated on `k`. `getBindingElementPropertyName` is
`PropertyNameOrName()`, so the pair discriminates on `kind` — §756's lesson,
now applied to the pseudo-reference road as well.

### Measured

```
scorepair over the §761 baseline (436,208):
  the candidate arm alone:            +2 W→R, ZERO adverse
  + the equality site removed:        unchanged (wash)
  + truthiness and switch removed:    unchanged (wash)
  + the four helpers deleted:         unchanged
  right 436,208 → 436,210
    dependentDestructuredVariablesWithExport 2
coverage: checker_types 6,098 → 6,099 (63.93% → 63.94%),
          gradient 91.09% unmoved at this rounding;
          diagnostics 2,600 unmoved; every other suite identical
```

**+1 case and −226 lines.** The line count is the result worth quoting: this
landing is mostly a deletion, and the deletion is the point — four bespoke
filters replaced by the one upstream road that now serves all five arms.

### `comparable_ternary` is NOT retired — correcting the expectation

§754 and §755 both said retiring `comparable_ternary` waits on this road. That
was too optimistic and is corrected here: the §50 family was **four of its
callers, not all of them**. Four remain:

```
narrow_type_by_switch_on_discriminant   flow.rs:4032, :4080
narrow_type_by_equality (§52's filter)  flow.rs:6767, :6794
```

Those are the SWITCH clause filter and the equality comparable-filter — both on
the ordinary reference road, both unrelated to pseudo-references. Retiring the
stand-in means moving those two onto `Relation::Comparable`, which is its own
item and is not blocked by anything now.

### Residue

- The four `comparable_ternary` call sites above.
- `getNarrowedTypeOfSymbol`'s own guards (`checker.go:13751`-`:13772`) — the
  `>= 2` element count, the root-initializer circularity check, the
  `isSomeSymbolAssigned` parameter test — are still whatever §50 made them;
  this landing changed the candidate road, not the entry conditions.

## §763 — `comparable_ternary` becomes `areTypesComparable` (+10 W→R, ZERO adverse, +1 case)

§762's residue, and the last piece of the discriminant subsystem's scaffolding.
Ten lines of new code; the rest is deletion.

### What it was

`comparable_ternary` was a **stand-in** written before this port had a
comparable relation: a hand-rolled table over a hardcoded `simple` flag set —
identity, an enum-member/literal lookup (§666's `enum_member_matches_literal`),
literal-vs-own-base, and same-base-primitive — returning `None` for anything
outside those domains. §750 gave the relater `Relation::Comparable` and §751
gave it the enum arms, at which point the stand-in was strictly weaker than the
thing it stood in for, and only inertia kept it.

### What it is

`areTypesComparable` is `isTypeComparableTo(a, b) || isTypeComparableTo(b, a)`,
so the body is now that query in both directions with the same Kleene contract
the callers already expect — identical in shape to `discriminant_keeps`, which
has run this way since §750:

```rust
forward == Related                     -> Some(true)
backward == Related                    -> Some(true)
both NotRelated                        -> Some(false)
anything else                          -> None   (decline the narrowing)
```

`None` stays "decline the whole narrowing" for the same reason it always was: a
dropped constituent is a confident wrong answer, and this relater's negatives
are decidable only on the domains `checker-notes-assign.md` §2 lists.

### Measured

```
scorepair over the §762 baseline (436,210):
  right 436,210 → 436,220   +10 W→R, ZERO adverse
    equalityWithIntersectionTypes01 10
coverage: checker_types 6,099 → 6,100 (63.94% → 63.95%),
          gradient 91.09% → 91.10%; diagnostics 2,600 unmoved;
          every other suite identical
```

The moved case is the tell: `equalityWithIntersectionTypes01` is an
INTERSECTION comparison, and intersections were never in the stand-in's `simple`
flag set — it declined them all. The relater has structural walks; the stand-in
had a flag test.

### `enum_member_matches_literal` deleted

§666's enum lookup was `comparable_ternary`'s only caller and went with it — 19
lines. §751's residue predicted exactly this ("`comparable_ternary`'s enum half
is now redundant with the relater and can go when the equality arm is
swapped"); it took until the last caller moved rather than the first, but the
prediction held.

### The subsystem, closed

Between §750 and §763 the discriminant road went from five arms with four
bespoke inline filters and a hand-rolled comparability table to **one pair
(`getDiscriminantPropertyAccess` / `narrowTypeByDiscriminant`) serving all five
arms over the real relation**. The scaffolding removed along the way:

```
§762  sibling_member_of_pattern, narrow_union_by_member_switch,
      filter_union_by_member_truthiness, filter_union_by_member_literal   226
§763  enum_member_matches_literal, comparable_ternary's table              ~80
                                                                    ~306 lines
```

### Residue

None for this road. `Relation::Comparable` is now the only comparability
answer in `flow.rs`, and the remaining unported narrowing items are the
`getNarrowedTypeOfSymbol` entry guards §762 listed.

## §764 — two `getNarrowedTypeOfSymbol` entry guards, one worth +4 and one worth 0 (ZERO adverse)

§762 changed the pseudo-reference CANDIDATE road and said plainly that it left
the ENTRY guards (`checker.go:13751`-`:13775`) as §50 had made them. Two of
those are ported here, and they are worth recording separately because **they
measure completely differently and only one of them is a conformance item.**

### The `never` guard — +4

`checker.go:13775`: when the pseudo-reference walk narrows the PARENT to
`never`, the element is `never`. Upstream answers that directly rather than
projecting a binding element out of an empty union. This port fell through to
the projection, which cannot produce the right answer from nothing.

**Measured alone: +4 W→R, ZERO adverse** — `arrayDestructuringInSwitch2` 2,
`dependentDestructuredVariables` 2.

### The `isSomeSymbolAssigned` guard — 0, and kept anyway

`checker.go:13772`: `!(IsParameterDeclaration(root) && isSomeSymbolAssigned(root))`.
Once any symbol the parameter's pattern binds is reassigned, the siblings stop
being projections of a single parent value, and discriminating them against
each other stops being sound.

**Measured alone: ZERO.** The corpus does not contain the shape. It is kept
because it prevents a **wrong answer, not a missing one** — the failure mode is
`f({ kind, v }: A | B) { kind = "b"; if (kind === "a") { v } }` answering
`string` where `v` is `string | number`. A unit test pins it, and removing the
guard reddens that test; the test says in its own doc comment that it is the
only thing pinning the guard, because the corpus number cannot.

`isSomeSymbolAssignedWorker` (`checker.go:31475`) recurses over a pattern's
elements, so the guard asks about EVERY symbol the root's name binds rather
than the one being read — assigning `kind` is what withdraws the narrowing for
`v`. That is the detail a simplified version would get wrong.

### Why the split measurement matters

Landed together these read as "+4, zero adverse", which would have credited the
soundness guard with movement it did not produce and hidden that it is
unfalsifiable by the corpus. Splitting cost one extra run. **A guard that
measures zero needs a test or it is indistinguishable from dead code**, and
that is the rule this section is really for.

### Measured

```
scorepair over the §763 baseline (436,220):
  the `never` guard alone:        +4 W→R, ZERO adverse
  + isSomeSymbolAssigned:         unchanged
  right 436,220 → 436,224
    arrayDestructuringInSwitch2 2, dependentDestructuredVariables 2
coverage: checker_types 6,100 unmoved, gradient 91.10% unmoved;
          diagnostics 2,600 unmoved; every other suite identical
```

### Residue — the guards still unported

- `GetRootDeclaration` on the declaration (`:13752`): this port takes ONE hop
  from the pattern to its holder, so a NESTED pattern
  (`const { a: { b, c } } = x`) finds a `BindingElement` where it wants a
  parameter or variable declaration, and declines. Upstream walks to the root.
- The root-initializer circularity check (`:13755`-`:13759`).
- `mapType(parentType, getBaseConstraintOrType)` (`:13768`): a type PARAMETER
  constrained to a union does not reach this road here, only a written union
  does.

None of the three is a soundness gap — each declines rather than mis-narrows.

## §765 — `GetRootDeclaration` at the entry, so nested destructuring reaches the road (ZERO movement, ZERO adverse)

§764's residue, first item. It measures **zero** and is landed on the strength
of a test, under the rule §764 wrote down.

### The bug

`getNarrowedTypeOfSymbol` uses two different nodes and this port conflated
them:

- **`parent := declaration.Parent.Parent`** (`checker.go:13760`) — the holder
  of the IMMEDIATE pattern, which is what `getTypeForBindingElementParent`
  reads the parent type from.
- **`rootDeclaration := GetRootDeclaration(declaration)`** (`:13752`) — the
  declaration owning the WHOLE destructuring, which is what the const/parameter
  test at `:13761` and the `isSomeSymbolAssigned` guard at `:13772` are asked
  about.

For a flat pattern these are the same node, which is why one node served both
for as long as it did. For a NESTED one — `const { p: { kind, v } } = x` — the
immediate holder is a `BindingElement`, which is neither a parameter nor a
variable declaration, so the const-like test fell to its `_ => return None` arm
and **the entire pseudo-reference road was unreachable for nested
destructuring**.

Both uses are now correct: `holder` still reads the parent type, `root` takes
the const test and the assignment guard.

### Zero, and landed anyway

```
scorepair over the §764 baseline (436,224):
  no transitions vs baseline
```

The corpus holds no nested discriminated destructuring, so it cannot see this
either way. §764's rule applies — **a change that measures zero needs a test or
it is indistinguishable from dead code** — and the test
(`a_nested_destructuring_still_reaches_the_discriminant_road`) reddens to
`string | number` when `root` is put back to `holder`. Checked, not assumed.

This is the third change in two sections whose value is invisible to the
corpus. That is itself worth noticing: **the entry guards are a region where
the corpus has stopped being the measuring instrument**, and sections here
should be expected to land on tests rather than on scores.

### Residue

Two entry guards remain, both declining rather than mis-narrowing:

- the root-initializer circularity check (`:13755`-`:13759`), which needs
  `IsNodeDescendantOf` and `getControlFlowContainer` agreement;
- `mapType(parentType, getBaseConstraintOrType)` (`:13768`), so a type
  PARAMETER constrained to a union reaches the road where only a written union
  does today.

## §766 — the base-constraint map at the entry (+16 W→R, ZERO adverse)

§765's residue, and the last of `getNarrowedTypeOfSymbol`'s three entry
guards that this port could reach. It is the largest of the three, and it was
listed last in §764's residue — which is worth noting, because the ordering
there was written from how each guard LOOKED, not from any measurement.

### The guard

`checker.go:13768`: `parentTypeConstraint = mapType(parentType, getBaseConstraintOrType)`,
and the union test at `:13772` is on the CONSTRAINT.

This port tested the written type. A destructured parameter typed
`T extends A | B` has a written type that is a TYPE PARAMETER, not a union, so
the test declined — **every generic destructuring missed the road**, however
plainly discriminated its constraint was.

`conformance/dependentDestructuredVariables` is full of them: 16 lines.

### The three entry guards, priced

```
§764  the `never` result            +4
§765  GetRootDeclaration             0   (nested patterns; test-only)
§766  the base-constraint map      +16
```

**+20 lines from three guards on one function**, none of which changes what the
road DOES — each only changes whether the road is reached. That is the shape of
this region: the pseudo-reference mechanism was built at §50 and correct, and
what was missing for a dozen sections was the entry conditions letting real
programs in.

### Measured

```
scorepair over the §765 baseline (436,224):
  right 436,224 → 436,240   +16 W→R, ZERO adverse
    dependentDestructuredVariables 16
coverage: checker_types 6,100 unmoved, gradient 91.10% unmoved at this
          rounding; diagnostics 2,600 unmoved; every other suite identical
```

+0 cases: all 16 lines are in one case that does not convert.

### `getNarrowedTypeOfSymbol`'s entry, complete for this port

One guard is left unported and it is not reachable here: the root-initializer
circularity check (`:13755`-`:13759`), which needs `IsNodeDescendantOf` on the
root initializer AND `getControlFlowContainer` agreement between the
declaration and the location. It is a CIRCULARITY guard — it stops a
declaration whose own initializer contains the location from recursing — and
this port has its own circularity protection on the type road, so porting it
without measuring what it would break is not obviously right. Left with the
reason recorded rather than as a bare TODO.

## §767 — `depend.rs` gets five step arms, and the board's second-largest root turns out to have been INVISIBLE (instrument only, ZERO checker change)

No checker code. `scorepair` reads `no transitions vs baseline`, and the walk
population is unchanged at **8,009 gap lines** — only ATTRIBUTION moved.

### Why this was worth a section

§766's handoff re-ran `depend.rs` and the board's top rows read:

```
CallExpression            1191  14.9%   the dependency types — the root is here
ArrowFunction              993  12.4%   NO STEP ARM for this kind — not a finding
PropertyAccessExpression   736   9.2%   the dependency types — the root is here
ObjectLiteralExpression    430   5.4%   NO STEP ARM for this kind — not a finding
...
MappedType 256, ArrayLiteralExpression 214, FunctionExpression 212 — all NO STEP ARM
```

**Roughly 2,100 lines — 26% of the board — were the instrument, not the port.**
`has_step_arm` is doing exactly its job there (§ its own doc comment: without
it, "no further dependency" conflates a real finding with a probe that cannot
walk), but a bucket that says nothing about the compiler still occupies the
rank a real row wants. A session that ranks by raw line count picks
`ArrowFunction` and finds nothing to port.

### The arms

Five, all on the `BinaryExpression` arm's shape — follow the first CONSTITUENT
that gaps; nothing gapping means the arm refused and the root really is here:

- `ObjectLiteralExpression` → each property's VALUE (a shorthand's value is its
  own name, which the identifier arm then resolves);
- `ArrayLiteralExpression` → each element;
- `ArrowFunction` → parameters, then the CONCISE body expression;
- `FunctionExpression` → parameters;
- `ParameterDeclaration` → the annotation, else the initializer.

### What it found

```
CallExpression   1218  15.2%   the dependency types — the root is here
Parameter        1059  13.2%   no further dependency         249 cases
                                 top case: conformance/contextuallyTypedIife
```

**The second-largest root on the gap board is an un-annotated PARAMETER with no
annotation and no initializer — i.e. CONTEXTUAL PARAMETER TYPING — and it was
completely invisible before**, hidden inside `ArrowFunction`'s "not a finding".
1,059 lines across 249 cases.

That is a nameable subsystem with a named head case, sitting at #2, that no
previous board could point at. It is the single most useful output of this
session's instrument work, and it cost five match arms.

The residual `NO STEP ARM` bucket is now **`MappedType` 272 (3.4%)** and
nothing else above 1%. It is left because a mapped type is a TYPE node and this
probe's `type_id_at_location` is an expression road; giving it an arm is a
different piece of work, recorded rather than guessed at.

### The freeze that held

`depend.rs`'s C1 is that its walked population matches the gap suite's.
**8,009 before and after** — the arms re-attribute, they do not widen the walk.
If a later arm changes that total, it has changed what is being counted and
nothing it prints is comparable to this board.

### How you would know this was wrong

An arm that follows a constituent which is NOT actually the dependency would
move lines to a root that cannot fix them. The check is the one §4.-2g used:
print the row before working it — which was done here rather than promised.
`contextuallyTypedIife` is **immediately-invoked function expressions**:
`(jake => { })("build")`, `((a, b, c) => { })("foo", 101, false)`, plus the
default/optional/rest parameter variants. So the root is un-annotated
parameters whose type comes from the CALL's arguments, which is precisely what
the arm claims. (An earlier draft of this paragraph guessed "callbacks passed
to generic functions" — that is a different and narrower population, and
reading the fixture is what corrected it.)

The row is 249 cases wide, so the head case is not the whole of it; the next
session should print more of the row before sizing the subsystem. But the arm
is attributing to the right KIND of root.

## §768 — the IIFE contextual-parameter arm (+73 W→R / +6 G→R, ZERO adverse)

**The session's largest landing, and §767 is why it exists.**

### The chain from instrument to arm

§766 handed off with a re-ranked board. §767 noticed a quarter of that board
was the instrument, added five step arms, and the second-largest root turned
out to be an un-annotated PARAMETER — 1,059 lines, 249 cases, head case
`contextuallyTypedIife`. Reading that fixture showed immediately-invoked
function expressions. This section ports the arm that types them.

Neither §767 nor this would have happened from the board as it stood: the row
was inside `ArrowFunction / NO STEP ARM — not a finding`, which is a bucket
that by construction says nothing about the compiler. **The instrument work
paid for itself, in one section, at roughly 16× its own size.**

### The arm

`checker.go:29463`-`:29484`, and it runs BEFORE the contextual-signature road:
an IIFE's parameters are typed from the ARGUMENTS of the call that invokes
them, widened. `(jake => { })("build")` types `jake` as `string` — nothing in
that program has a signature to be contextual from.

`GetImmediatelyInvokedFunctionExpression` (`ast/utilities.go:1853`) looks
through any number of parentheses, which is what the fixture's *"Lots of
Irritating Superfluous Parentheses"* block (`((((function (y) { }))))("-")`)
exists to check, and the test covers it.

Three cases past the arguments (`:29477`-`:29480`), transcribed rather than
collapsed:

- an argument at the parameter's index → its widened type;
- no argument but an initializer → `None`, so the ordinary road runs and the
  initializer types it;
- neither → `undefined`.

### Not ported, and it declines rather than guessing

A REST parameter needs `getSpreadArgumentType` (`:29468`). The positional arm
would hand `((...numbers) => …)(5, 6, 7)` the type of argument 0, which is
wrong — so it returns `None` and takes the implicit `any`. The test asserts
that decline, so the gap is pinned as a gap.

**§769 ported that arm, measured it at 21:9 adverse, and reverted it.** The
arm was correct; what broke was that a tuple in this port inherits no
`Array<T>` members, so `noNumbers.some(…)` went RIGHT→GAP the moment the
parameter stopped being `any[]`. STATUS §5 carries the record and names the
prerequisite — tuple apparent type — which is not this arm's to fix.

### A doc correction this landing forced

`get_contextually_typed_parameter_type`'s own doc listed *"An immediately
invoked function expression"* under **"Not ported, each answering `None`"**,
with a population figure (28 of 925). That bullet is now false and has been
rewritten to name the rest-parameter half as the remaining gap, and to say it
was corrected when the arm landed. **A "not ported" list is a liability the
moment one of its entries ships**, and this one would have gone on telling the
next reader the mechanism was absent.

### Measured

```
scorepair over the §766 baseline (436,240):
  right 436,240 → 436,319   +73 W→R / +6 G→R, ZERO adverse
    contextuallyTypedIifeStrict 30, contextuallyTypedIife 16+6,
    destructuringArrayBindingPatternAndAssignment3 13
coverage: checker_types 6,100 unmoved, gradient 91.10% → 91.12%;
          diagnostics 2,600 unmoved; every other suite identical
```

**+0 cases on a +79-line landing.** The three head cases each keep other
defects; `destructuringArrayBindingPatternAndAssignment3` moving 13 lines is
the interesting one, because it is not an IIFE fixture — IIFE parameter types
feed destructuring there, which is the sort of second-order reach a row's own
name does not advertise.

### How you would know this was wrong

The widening is load-bearing: `((n) => n)(101)` is `number`, not `101`. If a
later change makes IIFE parameters keep literal types, `contextuallyTypedIife`
will move backwards and the `get_widened_literal_type` call here is the
suspect.

## §770 — a tuple inherits `Array<T>`'s members (+44 W→R / +43 G→R against 3 G→W, 29:1, +2 cases)

§769's named prerequisite, landed. It is also §769's vindication: the refused
arm was correct and this is what it was waiting on.

### The gap

Upstream's tuple is a REFERENCE to a target synthesised by
`createNormalizedTupleType` (`checker.go:24148`) whose base is
`Array<union of the element types>` — `ReadonlyArray` when readonly — so
`.some`, `.every`, `.indexOf`, `.map` and the rest resolve through the ordinary
base-member road.

This port mints a tuple as a bare `Named` object carrying an element list
(`create_tuple_type`, `declared.rs:1775`) with **no base at all**. It answered
its numeric indices (§8) and nothing else. Every array method on every tuple in
the corpus was a gap.

### `length` is excluded, and the exclusion is measured

A tuple's own `length` is its element COUNT — a literal for a plain tuple (§117
slice 4), a UNION of possible lengths when elements are optional, which §117
declines. Upstream's tuple target declares its own `length` rather than
inheriting `Array`'s `number`.

The first cut did not exclude it and measured **2 R→W** on `tupleTypes`'
`declare const b1: readonly [number?]`, whose baseline wants `0 | 1` and got
`number`. That is the shape this project keeps warning about: a deliberate
decline replaced by a confident wrong answer. Excluding `length` removed both,
and left **no RIGHT→anything transition at all**.

### Measured

```
scorepair over the §768 baseline (436,319):
  first cut:                 +87 against 2 R→W + 3 G→W
  with `length` excluded:    right 436,319 → 436,406
                             +44 W→R / +43 G→R against 3 G→W — 29:1
    mapOnTupleTypes01 33+32, mapOnTupleTypes02 5+3, thisTypeInTuples 3+7
coverage: checker_types 6,100 → 6,102 (63.95% → 63.98%),
          gradient 91.12% → 91.14%; diagnostics 2,600 unmoved;
          every other suite identical
```

**No RIGHT→WRONG and no RIGHT→GAP.** The three adverse are GAP→WRONG — lines
that had no answer now have a wrong one, on cases that were already failing.

### What those three are, and why they are not a defect in this arm

**This paragraph was WRONG and §772 corrects it.** It said the three were a
`this`-type threading gap. They are not — this port threads `this` correctly,
and §772 shows `t.slice` printing
`{ (start?: number, end?: number): (string | number)[]; (): [number, string]; }`
RIGHT, with the `this` resolved to the tuple. The three are two *different*
causes, neither of them this-types, and neither a defect in the fallback. See
§772 for what they actually are; the claim that they are strictly better than
the gap they replaced stands.

### What the unit test does and does not pin

`a_tuple_length_is_its_element_count` **does not redden when §770 is
reverted**, and says so. This harness builds no program, so
`global_type_symbol("Array")` is `None` and the fallback never fires in it at
all. §770's guard is the CORPUS — which is the right instrument here, unlike
§764/§765's entry guards where it was blind. The `length` exclusion's guard is
the measured 2 R→W.

### §769 is now unblocked

The IIFE rest arm can be re-applied as STATUS §5 describes — roughly twenty
lines — and should be re-measured against this baseline rather than trusted
from its old number.

### Residue

- The 3 G→W above, whose causes §772 names correctly (merged-interface
  overload ORDER, and a union receiver's member signatures). **Not** this-type
  threading, which works.
- `ReadonlyArray` is selected by the tuple's own readonly flag, which is right,
  but no corpus line distinguished the two here.

## §771 — §769 re-applied, now that its prerequisite exists (+43 W→R, ZERO adverse)

The same arm §769 wrote, measured at **21:9 adverse**, and reverted. §770
removed the blocker. Re-measured here: **+43 W→R, ZERO adverse.**

```
§769 (against the §768 baseline):   +21 W→R  vs  4 R→G + 5 R→W   — refused
§770 (the prerequisite):            +87      vs  3 G→W           — landed
§771 (the same arm, re-applied):    +43 W→R  vs  0               — landed
```

Nothing about the arm changed. What changed is that a tuple now inherits
`Array<T>`'s members, so giving an IIFE rest parameter its correct tuple type
no longer takes `noNumbers.some(…)` from RIGHT to GAP.

### Why this is worth its own section rather than a footnote

**The refusal was the productive step.** §769 could have been forced through —
it was net-positive at +12 — or approximated as `number[]` to dodge the
adverse. Either would have banked a smaller number and buried the real defect:
that this port's tuples had no members at all. Instead the refusal named the
prerequisite, §770 fixed it for the whole corpus (+87, and two cases that had
nothing to do with IIFEs), and the arm then landed at nearly double its
original score with no cost.

The arithmetic across the three sections is **+151 right lines and +2 cases**,
against §769-forced's +12 with nine adverse transitions carried forward.

### Residue

A SPREAD argument (`f(...xs)`) still declines — the variadic legs at `:29504`
and `:29528` are not ported, and the test asserts that decline so it stays
pinned as a gap.

### Measured

```
scorepair over the §770 baseline (436,406):
  right 436,406 → 436,449   +43 W→R, ZERO adverse
    contextuallyTypedIife 19, contextuallyTypedIifeStrict 19,
    emitDefaultParametersFunctionExpressionES6 2
```

§768's test asserted `any` for the rest case and described the arm as
declining. That assertion is now false and has been updated to
`[number, number]`, with the decline moved onto the spread-argument form that
genuinely still declines — the same liability §768 itself flagged about
"not ported" lists, met one section later on a test.

## §772 — correcting §770's residue: the three adverse are NOT a this-type gap

Documentation only, no code. §770 landed +87 against 3 GAP→WRONG and attributed
those three to "a `this`-type threading gap". **That attribution was wrong**,
and it was wrong in the expensive direction: it would have sent the next
session into `this`-type machinery that already works.

### What the port actually does

`verdictdump` on `thisTypeInTuples`, at §771:

```
:0:7   RIGHT  t.slice : { (start?: number, end?: number): (string | number)[]; (): [number, string]; }
:0:9   RIGHT  slice   : { (start?: number, end?: number): (string | number)[]; (): [number, string]; }
```

The fixture augments `interface Array<T> { slice(): this; }`, and the port
resolves that `this` to `[number, string]` — the TUPLE — and prints the whole
merged member correctly. §164's call-site rule (`calls.rs:703`: a signature
returning a minted this-type answers the RECEIVER) is doing its job, and §770's
`Array<union>` fallback does not break it.

### The two real causes

**(a) Merged-interface overload ORDER — `thisTypeInTuples:0:5`/`:0:6`.**
The failing lines are `let a = t.slice();` — the ZERO-argument call. Both
overloads are applicable to it (`slice(start?, end?)` has all-optional
parameters), so the answer is decided entirely by which candidate is tried
first. Upstream answers `[number, string]`, i.e. it picks `(): this`; this port
answers `(string | number)[]`, i.e. lib's.

Note the print order is the OPPOSITE of the apparent resolution order, and both
ports agree on the print: the member prints lib's overload first (`:0:7` above
is RIGHT) while upstream ANSWERS as if the augmentation's were chosen. The
baseline is unambiguous — `let a = t.slice();` gives `a : [number, string]`
while `t.slice(1)` gives `(string | number)[]`, so the zero-argument call takes
`(): this` and the one-argument call takes lib's.

**The MECHANISM behind that is not established, and this note does not claim
one.** A first draft asserted "TypeScript's merged-interface rule: a later
declaration's overloads precede earlier ones for resolution". That was an
inference from the observed answer, not a reading, and the upstream code points
against it: signature order is `symbol.Declarations` order
(`getSignaturesOfSymbol`, `checker.go:19806`) and the binder appends with a
plain `core.AppendIfUnique` (`binder.go:2519`) — no reordering anywhere on that
path. So "later declarations come first" is unsupported, and it has been
withdrawn rather than left standing.

What IS established: upstream picks `(): this` for the zero-argument call and
this port picks lib's `(start?, end?)`, both signatures are applicable to zero
arguments, and the difference is therefore in candidate selection rather than
in this-types.

**Narrowed one step further, by reading rather than inferring.**
`chooseOverload` (`checker.go:9025`) is plainly first-applicable-wins: it walks
`s.candidates` in order, skips any failing `hasCorrectTypeArgumentArity` or
`hasCorrectArity`, and returns the first that is applicable (`:9040`-onwards).
There is no preference for fewer parameters, no arity tie-break, no second
ranking pass. So the whole question reduces to **the ORDER of `s.candidates`**.

That suggested a claim — *"upstream's candidate order cannot be its print
order"* — and **the claim was tested and REFUTED.**

The probe: reverse the candidate list in `get_signatures_of_symbol`
(`signatures.rs:243`, which mirrors `getSignaturesOfSymbol` exactly — the same
declarations order, the same overload-implementation skip) and run the corpus.

```
right 436,449 → 430,453   −5,996 lines
```

Declaration order is overwhelmingly the correct general rule, and a global
reordering is catastrophic. So `thisTypeInTuples`'s two lines are a **LOCAL
exception**, not evidence of a systematic ordering difference, and the
inference above is withdrawn along with the two before it.

**Where that leaves the item.** Three things are now established by reading or
measurement rather than inference:

1. `chooseOverload` is first-applicable-wins over `s.candidates` — read.
2. This port's `get_signatures_of_symbol` mirrors upstream's rule exactly —
   read, both sides.
3. Global candidate order is declaration order and must stay that way —
   measured at −5,996.

Which means the cause is something that makes lib's `slice(start?, end?)`
*inapplicable* or unreachable for this particular zero-argument call on a
TUPLE, rather than anything about ordering. That is a much smaller search than
the one this section started with, and it is genuinely open — three hypotheses
have now died here, and the next one should be measured before it is written
down.

**(b) A UNION receiver's member signatures — `sliceResultCast:0:3`.**
`declare var x: [number, string] | [number, string, string]; x.slice` should
print the UNION of the per-constituent signatures:
`((start?: number, end?: number) => (string | number)[]) | ((…) => (string | number)[])`.
This port collapses to one. That is union-property signature projection, not
overloads and not this-types.

### Why record a three-line correction at all

Because a wrong cause in a residue note is worse than no note: §770's reader
would have opened `this`-type threading, found it working, and had to
re-derive the real answer. The project's own rule — *correct the record when a
number turns out to be wrong, and say it was corrected* — applies to
attributions as much as to numbers.

**And the correction needed correcting, twice.** This section's first draft
replaced §770's wrong cause with a wrong MECHANISM ("later declarations come
first"), inferred from the answer rather than read from the source. Its second
draft replaced that with a wrong INFERENCE ("candidate order cannot be print
order"), which a one-run probe then refuted at −5,996 lines.

Three plausible-sounding causes died in one section, and the pattern in all
three is identical: each was written the moment it *explained* the observation,
without being tested against anything else. The two that were merely read
against the source survived — `chooseOverload`'s rule, and the port's mirroring
of `getSignaturesOfSymbol`. **The failure mode is not ignorance, it is
fluency**, and the corrective is cheap: one corpus run refuted in forty seconds
what three paragraphs had asserted.

Neither cause is cheap, and neither is a defect in §770's fallback. Both are
now named precisely enough to be picked up or refused on their merits.


## §774 — the board re-measured for the 95% goal, three candidate levers priced, two refused

Measurement only, no code. Run because the session goal became a NUMBER
(95% gradient), and a number-goal has to be planned against the ceiling and the
levers rather than worked item by item.

### What 95% requires, and whether it is reachable

`ceiling.rs` at this tree:

```
gradient                    436,449 / 478,855  (91.14%)
UNREACHABLE (ADR-0038), attributed        0  (0.00 points)
  not attributed: case has no .errors.txt 408  (0.09 points)  <- the blind spot
REACHABLE CEILING           reachable denominator 478,855 of 478,855
```

**Nothing is structurally unreachable.** 95% is therefore a volume problem, not
a bound problem — but the shape of the volume matters:

```
95% of 478,855                = 454,912 right      (+18,463 from today)
the ENTIRE gap                =   7,957 lines
closing all of it             → 444,406 = 92.8%
```

**Closing every gap line in the corpus does not reach 95%.** The remaining
~10,500 must come from converting WRONG lines, of which there are ~29,900. Any
plan that ranks only gap rows is planning for at most +1.7 points.

### The gap is a list, not a tail

`gaproot.rs`, 7,957 lines over **117 distinct root rows**:

```
 rows   lines   share
    1    1152  14.48%
   10    4416  55.50%
   30    6497  81.65%
  100    7928  99.64%
```

Thirty rows cover 82%. That is a materially different picture from §4.-3's "long
tail", which was measured in CASES; in LINES the gap is concentrated. The two
framings are both right and answer different questions, and a line-denominated
goal should use this one.

### §773's recommendation, WITHDRAWN

§773 named *"property access, the property has no type" at 259 lines* as the
best-evidenced unworked item. **It is not an item at all.** `gaproot.rs` runs
with `PROPERTY_DECLARATION_EDGE = true` and DESCENDS through that refusal to the
property's own declaration; `property-declaration` appears in `EDGES TAKEN` at
240 steps. A one-step split figure is not a root figure, and `depend.rs`'s
property-access table is a one-step split. Withdrawn before anything was built
on it — the fifth wrong call in this block, and the fourth caught by checking
rather than by consequence.

### Lever 1: design P (qualified-name printing) — REFUSED, third re-size

`qualnamep.rs` at this tree:

```
STRICT   CONVERTS 0    CHURN 0     AT RISK 6     net  -6
LOOSE    CONVERTS 52   CHURN 112   AT RISK 762   net -710
CP7  right lines already printing a dotted namespace name  20,055
```

Design P has now been sized three times: **36 → 23 → 0**. It is not shrinking
because it was mis-measured; CP7 says why — design W landed and its 20,055 right
lines ARE design P's former population. **The design is spent.** It should not
be re-proposed without a fresh CP7.

### Lever 2: the class/enum alias leaf — REFUSED, re-measured

`get_type_of_alias`'s §145 arm excludes `NAMESPACE | CLASS | ENUM` leaves, with
§156's recorded refusal (58:34, then 33:34 over §157) and the reason *"classes
keep the gap until that print road exists"* — the print road being lever 1.

Lever 1 is dead, so that prerequisite will not arrive. The §769→§771 lesson says
a refused arm can turn clean when the tree moves, so it was re-measured rather
than assumed:

```
admit CLASS and ENUM leaves:
  +6 G→R, +6 W→R   against  26 G→W
  12 gained : 26 lost
```

**The refusal STANDS**, now on a third independent measurement, and its stated
prerequisite is void. `collisionExportsRequireAndInternalModuleAliasInGlobalFile`
alone contributes 12 of the 26. Anyone reopening this needs a different reason
than "the tree has moved" — that was tested.

### Where that leaves the 95% goal

Two of the three levers the board still advertised are spent. What is left is
ordinary volume across concentrated rows, and the wrong-line side must carry
roughly 57% of the distance. The rows with the highest `own` share — work that
converts directly rather than unblocking something downstream — are:

```
                                                  lines   own  cases
alias RHS TypeReference resolved with arguments     125   125     31
declaration name, symbol has no type: BINDING…      132   128     43
annotation TypeReference qualified name             109   101     24
declaration name, symbol has no type: FUNCTION…     100   100     60
expression answered error: CallExpression          1152   538    247
expression answered error: ArrowFunction           1101   464    207
```

The first four are 100%-own rows: nothing downstream depends on them, so their
`lines` figure is their conversion, not a ceiling.

## §776 — the generic object-rest `Omit` mint, with the prerequisite §775 named (+12 G→R / +10 W→R against 2 G→W, ZERO RIGHT→anything)

§775, refused hours earlier at 22:12, landed after its own prerequisite was
built. Third time this pattern has run (§769→§770→§771, and now this), and the
first time the prerequisite was small enough to build in the same session.

### What §775 could not do

`getRestType`'s generic branch mints `Omit<source, omitKeyType>`, and
`omitKeyType` is NOT just the bound names: `unspreadableToRestKeys`
(`checker.go:17806`-`:17818`) adds every property that cannot be spread — a
method or accessor declared in a class, and a `private` or `protected` member.

§775 omitted only the bound names and took
`compiler/destructuringUnspreadableIntoRest` **30 RIGHT→WRONG**. Its variant 3
tried to DECLINE whenever the source had an unspreadable member and could not
see them: private and protected members are `Member::Property` in this port,
and `Member` carries no visibility.

### The prerequisite

`is_spreadable_property` over SYMBOLS rather than over `Member`
(`checker.go:17830`, joined with `getRestType`'s own private/protected test at
`:17808`): a property is spreadable unless it carries `private`/`protected`, or
is a METHOD/ACCESSOR whose declaration's parent is class-like.

That needed modifier access for class-member kinds, which
`member_declaration_has_modifier` supplies — deliberately separate from
`merged_export_spaces.rs`'s `declaration_has_modifier`, which matches
top-level kinds (interface, class, enum, function, module, `var`) and answers
`false` for every member kind. Two disjoint node sets, two questions; merging
them would make each caller carry the other's arms.

**Constructor parameter properties are why `ParameterDeclaration` is in the
list** — `destructuringUnspreadableIntoRest` declares all five of its
members that way.

### An ordering bug the unit test caught

Upstream tests `omitKeyType.flags & Never` at `:17820`, **before**
`getGlobalOmitSymbol` at `:17823`. A first draft had them the other way round,
so `{ ...r } = obj` over a source with nothing to omit answered `error` in a
lib-less program instead of the source. The corpus could not see it — every
corpus program has a lib — and the unit test could, which is the reverse of the
usual split on this page.

### Measured

```
scorepair over the §771 baseline (436,449):
  §775 variant 1 (bound names only)        +22  against 30 R→W + 2 G→W   -8
  §775 variant 2 (excluding `this`)        +22  against 10 R→W + 2 G→W  +12
  §775 variant 3 (decline on non-Property) +15  against 10 R→W           +5
  §776 (the unspreadable half, ported)     right 436,449 → 436,471
                                           +12 G→R / +10 W→R against 2 G→W
                                           ZERO RIGHT→WRONG, ZERO RIGHT→GAP
    genericObjectRest 4, genericObjectSpreadResultInSwitch 2+4,
    genericIsNeverEmptyObject 2, objectRestNegative 3, restInvalidArgumentType 2
coverage: checker_types 6,102 unmoved, gradient 91.14% → 91.15%
```

### The 2 G→W are a different unported mechanism

`narrowingDestructuring:0:76`/`:0:77` want `{ a: string; }` — the NARROWED
concrete type — and get `Omit<T, "kind">`. Upstream narrows the source by
control flow first, at which point it is no longer generic and the concrete
branch runs. That is `getFlowTypeOfDestructuring` (`checker.go:17743`), which
`destructure.rs`'s own module doc already names as unported. Not a defect in
this arm; it is an older gap becoming visible now that the generic branch
answers at all.

### Residue

- `getFlowTypeOfDestructuring`, above.
- `isGenericIndexType(omitKeyType)` (`:17813`): a COMPUTED key declines earlier,
  at `binding_element_property_name`.
- `isGenericObjectType` is reduced to a bare TYPE PARAMETER; a mapped or
  indexed-access source keeps the gap.

## §777 — the WRONG-side board for the 95% goal, and `any_audit` is BROKEN (measurement only)

§774 established that 95% needs ~10,500 lines from the WRONG bucket, because
the entire gap is 7,957. This is the wrong side, measured — and the first
instrument reached for it turned out to be untrustworthy, which is the more
important half of this section.

### `wrongflip.rs` — trustworthy, and it names one dominant cause

```
WRONG (ported, defect)      29,815 lines
  we answered `any`, upstream did not      18,112  (60.75%)
  same shape, different text                6,670  (22.37%)
  different shape — a wrong type            4,062  (13.62%)
  upstream expanded it, we printed a name     546   (1.83%)
  upstream NAMED it, we printed a structure   362   (1.21%)
  error leaked into a printed type             63   (0.21%)

propagation split:  ROOT 4,113 (13.80%) — everything else is a symptom
```

Two facts worth carrying. **Over-answering `any` is 61% of the entire wrong
bucket** — one cause, bigger than everything else combined. And **only 13.8% of
wrong lines are ROOTs**; the rest propagate, so the conversion-per-fix ratio on
this side is better than the line counts suggest and worse than they promise,
depending on which root a fix reaches.

Naming failures are **908 lines (3.05%)**, which independently corroborates
§774's refusal of design P: there is no naming lever left to pull.

### `any_audit.rs` — its CONTROL FAILS, and its rows must not be used

The obvious next step was `any_audit`, which splits printed `any` by the rule
that minted it. It runs, prints a full ranked table, and then **aborts on its
own control**:

```
CONTROLS (must read zero):
  UNCLASSIFIED, banked      6360
  UNCLASSIFIED, lost       13903
  DISAGREEMENT (both)      20121
assert_eq!(… , 0, "the classifier no longer mirrors
                   `types_producer::type_at_location`;
                   no row above is trustworthy")
```

The assertion's message is the finding: **no row above is trustworthy.** The
control exists precisely because "a classifier that has drifted produces a tidy
table either way", and it has drifted by 40,384 lines.

**I read its rows before reading its exit code** and quoted
`CHECKER/… unannotated Parameter, container CONTEXTUALISABLE` at 1,042 + 778
lines as "~1,820 lines from contextual parameter typing, the biggest actionable
lever". **That number is withdrawn.** The row may be right, but this probe
cannot establish it, and the total it sits inside is the only part corroborated
elsewhere (`wrongflip`'s 18,112, a separate probe that did not crash).

`any_audit` needs repairing before the `any` bucket can be split at all. Until
then the 18,112 is a known total with an unknown composition.

### A stale doc found on the way

`contextual.rs`'s module doc carries a reachability table:

```
| callee                     | count     | reachable                |
| one non-generic signature  | 548 (59%) | yes — this module        |
| generic                    | 183 (20%) | no: needs inference      |
| overload set               | 102 (11%) | no: needs the full resolveCall |
```

The generic row is **no longer true**: `contextual_type_for_argument` carries
"Iteration 4 arm (a)" — a single generic candidate's parameter type flows as-is,
with the fixing-mapper and returnMapper guards — and `inference.rs` is 2,519
lines with `check_generic_call`, `instantiate_type` and
`is_context_sensitive_argument`. The same doc's opening line, *"reduced to the
three corpus arms"*, is stale too: `get_contextual_type` now dispatches
seventeen.

Not corrected in this section because correcting it properly means re-measuring
the table's four rows, which is its own piece of work. **Recorded here so the
next reader does not plan against it**, and so that whoever re-measures knows
the numbers are pre-inference.

### Where this leaves the 95% route

- The gap side is 7,957 lines over 117 rows, top 30 = 82% (§774).
- The wrong side is 29,815, of which `any` over-answering is 18,112 —
  **composition unknown until `any_audit` is repaired**.
- Naming is spent (908 lines, design P refused three times).
- 13.8% of wrong lines are roots.

The honest next step for the goal is **repair `any_audit`**, because without it
the largest single population in the corpus cannot be ranked, and every plan
that touches it is planning blind.

## §779 — an ambient `declare var` is `any`, not the auto road's `undefined` (+41 W→R, ZERO adverse, +3 cases)

Found by working the WRONG board §777 established as the trustworthy one.
`wrongflip`'s W2 (exact substitutions on ROOT lines) ranked `any -> undefined`
at 43 lines with **69.8% in one case** — the most concentrated row on that
board, and small enough to read whole.

### The rule

Upstream's `autoType` **is `any`** — `c.autoType = c.newIntrinsicTypeEx(TypeFlagsAny, "any", …)`
(`checker.go:976`). A reference whose flow type is still auto goes through
`convertAutoToAny` (`checker.go:11182`) and prints `any`.

A `declare var a;` **can never be assigned**, so its flow type never leaves
auto, and upstream prints `any` at every reference. This port's auto road
answers its INITIAL `undefined` instead — correct for a `let x;` read before
its first assignment, wrong for an ambient declaration that has none.

`conformance/jsxEsprimaFbTestSuite` is thirty `declare var` lines
(`declare var 日本語; declare var AbC_def; declare var x; declare var a;` …) and
every one of them printed `undefined`.

### The arm

`is_auto_typed_declaration` excludes a declaration whose `VariableStatement`
carries `declare`. Everything else about the auto road is untouched, and the
test pins both sides of the distinction: `let b;` before assignment is still
`undefined`, `let c; c = 1;` is still `number`.

### Measured

```
scorepair over the §776 baseline (436,471):
  right 436,471 → 436,512   +41 W→R, ZERO adverse
    jsxEsprimaFbTestSuite 30, tsxReactEmit3 7, tsxExternalModuleEmit2 4
coverage: checker_types 6,102 → 6,105 (63.98% → 64.01%),
          gradient 91.15% → 91.16%; diagnostics 2,600 unmoved
```

**+3 cases, and the port crosses 64%.**

### What this does not do

It does not port `convertAutoToAny`. The general rule — *a flow type that is
still auto at the reference is `any`* — would also cover a non-ambient
declaration that happens to have no assignments anywhere, and this arm reaches
only the ambient case, where "no assignments" is guaranteed by the grammar
rather than by a walk. The general form needs the port's auto road to
distinguish `autoType` from `undefinedType`, which it currently conflates by
using `undefined` as the initial; that is a model change and is not attempted
here.

Worth noting for the 95% campaign: this row was 43 lines on a board whose
largest ROOT substitution is 96. **There is no large lever left on either
side** — §774 showed the gap is 117 rows with the top 30 at 82%, and §777 shows
the wrong side's ROOT bucket is 4,113 lines spread thinner still. The route is
many arms of this size.

## §780 — repairing `any_audit`'s classifier: 40,384 → 1,656 (instrument only, no checker change)

§777 found `any_audit` failing its own control and §778 corrected §6 for having
told readers to use it anyway. This is the repair. `scorepair` reads
`no transitions vs baseline` — nothing about the checker moved.

```
control total (UNCLASSIFIED banked + lost + DISAGREEMENT)
  before   40,384
  after     1,656     — 96% of the drift closed
```

### The classifier had stopped mirroring the producer in three specific ways

`classify` claims to mirror `type_at_location`'s branch order. It did — but
mirroring a branch means mirroring **both of its exits**, and three of the
producer's exits were missing.

**1. SS183's error exit (`types_producer.rs:510`).** The right side of a
property access prints `any` when the ACCESS ITSELF computes to `error`, not
only when it computes to `any`. The classifier tested `any` alone, so every
such name became a disagreement. Worth ~5,000.

**2. The producer's error→`any` conversions.** Several producer branches
(`:434`, `:467`, `:617`, `:630`) answer `any` where the checker held `errorType`
— *"upstream holds `errorType` at"* those positions and the baseline records
`any`. A classifier branch that answered `error` therefore **explains** the
printed `any` rather than contradicting it. Calling that a disagreement buried
a real origin under the control. Worth ~15,900, the largest of the three.

**3. The rendering (`types_producer.rs:1355`).** The producer prints through
`type_to_string_at(id, reference)`, which is reference-aware; the classifier
used the context-free `type_to_string`. **Measured at zero** on this corpus and
kept anyway, because the two printers can differ and a classifier that renders
differently from the producer is drift waiting to happen.

### What was deliberately NOT done

`verdict` could have asked `type_at_location` for its own answer, which would
satisfy the assertion **tautologically** — the control would then catch nothing,
which is worse than failing loudly. §778 ruled that out in advance and this
repair respects it: each fix copies a specific branch exit of the producer, and
a branch that reaches a genuine non-`any`, non-`error` type **still disagrees**.

### The 1,656 that remain, and a hypothesis marked as one

```
  264  Identifier  parent=VariableDeclaration
  126  Identifier  parent=BinaryExpression
   94  Identifier  parent=BindingElement
   56  Identifier  parent=Parameter
```

Their reasons are things like *"declaration name -> implicit `any`: unannotated
Parameter, container CONTEXTUALISABLE"* on a line the producer printed `any` —
the classifier's branch and the producer's are the **same code**
(`get_type_of_symbol` on the parent's symbol), and yet they answer differently.

**Hypothesis, untested:** the port's contextual parameter typing is
ORDER-DEPENDENT. The producer walks and renders every line of a file; the
classifier re-asks afterwards, by which time the enclosing call has been checked
and a contextual type exists that did not exist at render time. If so this is a
checker defect worth ~1,656 lines, not a classifier one.

It is written down as a hypothesis because today has killed five plausible
causes that were written the moment they explained the observation (§772, §773,
§777). **The next person on this should test it before believing it** — the
cheap test is to render a file twice and compare, or to classify in the
producer's own walk order.

### Status of the instrument

Still failing, still refusing to publish, and that is correct — 1,656 lines of
drift is 1,656 too many for a table that ranks the corpus's largest population.
But the residue is now small enough to characterise, which it was not at 40,384.
A debug view (`TSR_ANY_DEBUG=1`) prints the remaining disagreements by
node/parent kind and the first ten raw, which is how the four rows above were
obtained.

## §781 — the hypothesis tested: `type_at_location` is ORDER-DEPENDENT, 670 of 757 (instrument only)

§780 recorded a hypothesis and said in its own text *"test it before believing
it"*. This is the test, and it took one env-gated probe.

### The test

On every line the classifier still disagreed about, call the **producer's own
function** — `types_producer::type_at_location`, the same function whose
recorded answer the suite scores — a second time, and compare with what it
recorded during the render walk.

```
REPLAY of the producer on DISAGREEMENT lines:
  producer answers the SAME as it recorded    87
  producer answers DIFFERENTLY               670    (88%)
```

**`type_at_location` is not a pure function of the node.** Asked twice about the
same node in the same program, it answers differently 88% of the time on this
population. The classifier was innocent for those 670: it was not drifting from
the producer, it was asking a producer that had changed its mind.

### What this establishes, and what it does not

**Established**: on lines the producer printed `any` and the classifier's branch
answered something else, re-asking the producer gives a different answer 670
times out of 757.

**NOT established**: that the checker is 88% order-dependent in general. This
population is pre-filtered twice — to lines printing `any`, and to lines where a
branch disagreed — which is exactly the population most likely to be
order-sensitive. The corpus scores are stable because they come from ONE
consistent walk; nothing here says a scored number is wrong.

**What it does mean** is that a type in this port can depend on what has been
checked before it. Upstream memoizes deterministically; `getTypeOfSymbol` on a
symbol answers the same thing whenever it is asked. Somewhere this port's
caching lets a later question see state an earlier one did not — the contextual
parameter road is the natural suspect, since a parameter's type genuinely
differs before and after its enclosing call is checked, but **that attribution
is not tested and is not claimed.**

### The consequence for the instrument

`any_audit`'s control demands zero, and 670 of its remaining 757 are not
classifier drift at all. **So the control as written cannot reach zero while the
checker is order-dependent** — it is measuring two different things under one
number: classifier fidelity (what it was built for) and checker determinism
(what it accidentally detects).

Not changed here. Splitting the control means deciding which of the two the
probe is for, and that decision should be made by whoever repairs the
determinism, not by the session that found it. The replay is left behind
`TSR_ANY_REPLAY=1` so the split can be measured the moment it is wanted.

### Why this is worth more than the 670 lines

A checker whose answers depend on question order is one whose measurements are
conditional on the walk. Every instrument on this project reads
`type_at_location`; §767's step arms, §774's roots, §777's wrong board and
§780's own repair all assume it answers the same thing twice. **It does not**,
and that is now on the page with a number rather than an assumption.

The immediate work it implies is not a conversion arm: it is finding which cache
or which road lets the second answer differ, and the replay probe is the tool
for it — filter to one case, print both answers, and the divergent node names
itself.

## §782 — order-dependence costs REAL LINES: 72 where the second answer is the right one (instrument only)

§781 established that `type_at_location` answers differently when asked twice.
The obvious next question is whether either answer is better, and it has a
number.

```
REPLAY of the producer on DISAGREEMENT lines:
  producer answers the SAME as it recorded      87
  producer answers DIFFERENTLY                 670
  ...of which the REPLAY matches the baseline   72
```

### It cuts both ways, and that is the point

```
compiler/awaitedTypeJQuery:0:17   rendered=any   replay=null   want=null   <- replay RIGHT
compiler/anyInferenceAnonymousFunctions:0:45  rendered=any  replay=T  want=any  <- render right
compiler/capturedLetConstInLoop1:0:10  rendered=any  replay=never  want=any     <- render right
```

So this is not "the second answer is better". It is that **the checker holds
two different answers for the same node and the walk decides which one the
corpus sees** — and on **72 lines the one it does not see is the baseline's**.

### What that does and does not license

**Established**: on 72 lines in this sample, the information needed for the
RIGHT answer already exists in the checker; the render walk asks too early and
records a different one.

**NOT established**: that a determinism fix converts 72 lines. Making the
checker deterministic means choosing which answer becomes canonical, and
nothing here says the replay answer wins — on `anyInferenceAnonymousFunctions`
and `capturedLetConstInLoop1` the rendered one is correct and the replay is
not. A fix that simply preferred the later answer would convert 72 and break an
unknown number of the other 598.

**And 72 is a floor on a narrow sample.** The population is pre-filtered twice
— to lines printing `any`, and to lines where a classifier branch disagreed. It
is a keyhole. The corpus-wide cost of order-dependence is unmeasured and this
number must not be quoted as it.

### Why it is worth recording anyway

Because it converts §781 from an instrument curiosity into a checker defect
with a cost — **and §783 then measured that cost corpus-wide and found this
section's framing too optimistic by two orders of magnitude. Read §783 before
acting on anything here.** The honest way to size it was always to widen the
keyhole; §783 did, and the answer is 170 lines lost against 16,597 saved.

### Residue

- The wide replay measurement above.
- Which answer should be canonical, which is the actual design question and is
  untouched.

## §783 — the wide replay: walk order is worth +16,427, not a defect to be fixed (instrument only)

§782 measured order-dependence through a keyhole — `any` lines that a
classifier branch disagreed about — found 72 lines where the replay was right,
and said in its own text that the corpus-wide figure was unmeasured and **must
not be quoted as this number**. This measures it, and the caution was
warranted: the keyhole was misleading in the OPTIMISTIC direction.

```
WIDE REPLAY over EVERY aligned line:
  aligned lines replayed                    474,229
  producer answers DIFFERENTLY               29,676   (6.3%)
  ...replay RIGHT where rendered was wrong      170   <- lines lost to walk order
  ...rendered RIGHT where replay is wrong    16,597   <- lines the walk order SAVED
```

### The finding, which reverses the expected sign

Order-dependence is real and large: **29,676 aligned lines — 6.3% of the corpus
— answer differently when asked a second time.** But the walk's order is
overwhelmingly *beneficial*. It saves 16,597 lines and costs 170, a ratio of
about **98 to 1**, worth **+16,427 net**.

§782 called this "a correctness item with its own conversion" and estimated the
conversion from a keyhole. The conversion is **at most 170 lines**, and the
naive repair — making the checker answer from a cold, order-independent state —
would **lose roughly 16,597**.

### What that means for the determinism work

It is not a line-conversion opportunity and should not be scheduled as one.
What the walk order is doing is supplying context that a cold ask does not
have: a parameter typed after its enclosing call is checked, a reference typed
after its declaration's initialiser. Upstream gets the same effect from
deterministic memoisation that happens to be populated in a valid order;
this port gets it from the order alone.

So the real statement is: **this port's answers are correct largely BECAUSE of
the walk order, and that dependency is undocumented and unenforced.** The risk
is not the 170 lines; it is that any future change to walk order — a new probe,
a reordered check, a parallel walk — silently moves 16,597 lines. That is worth
a guard, not a repair.

### The instrument consequence, restated

`any_audit`'s control still cannot reach zero, and now the reason is precise:
it is detecting a property the port RELIES ON. Splitting the control into
classifier-fidelity and checker-determinism halves is the right move — and the
determinism half should be reported as a standing figure to watch for movement,
not as a defect count to drive to zero.

### What is still not established

Which of the 29,676 divergences are legitimate context-dependence and which are
genuine cache bugs. This probe cannot tell them apart — it only knows the two
answers differ and which one the baseline preferred. A cache bug and a
correctly-context-sensitive answer look identical here.


## §784 — the unresolved-identifier gate's missing JS half, and its CommonJS exception

### The gate, and the sentence in it that was never code

`checkIdentifier`'s unresolved arm in this port (`expressions.rs`, the §31
gate) answers `any` rather than `errorType` for a name that does not resolve.
The reasoning is not upstream's — upstream answers `errorType` and reports
TS2304 — it is a hedge about *this port*: our binding roads are incomplete, so
a confident `errorType` would sometimes be a lie about a name upstream can see
and we cannot.

The hedge has a structural escape. When the file carries import machinery, the
port *does* have known gaps that could explain the miss, so it stops hedging
and answers `errorType` — see [`Checker::file_has_import_machinery`]. The
comment that introduced that escape names a **second** case of the same kind:
"a JS/JSX file resolves through machinery with known port gaps — both stay
honest gaps." That sentence was never a condition. The gate tested only for ES
import/export declarations.

### The forcing measurement

Ranking the baseline's wrong lines by case:

```
awk -F'\t' '$2=="WRONG"{split($1,a,":"); c[a[1]]++} END{for(k in c) print c[k], k}' \
  target/verdict_baseline.tsv | sort -rn | head
```

puts `compiler/parsingDeepParenthensizedExpression` first at **325 wrong
lines** — the single most concentrated block in the corpus. It is an
`allowJs` `.js` fixture (`@fileName: a.js`) whose `f`, `l`, `b` and `o` are
undeclared; upstream reports TS2304 on each and the oracle records `error`.
The port printed `any` on all 325. So the missing half of the gate's own
comment was worth more than any other single wrong-line cluster on the board.

### Why the naive arm cost 11 RIGHT→GAP

Adding `|| self.in_js_file(id)` alone measured:

```
WRONG->RIGHT: 198   RIGHT->GAP: 11   WRONG->GAP: 32
```

Net still positive (+187) but with 11 regressions — and every one of them
(`ensureNoCrashExportAssignmentDefinePropertyPotentialMerge`,
`requireAssertsFromTypescript`, `commonJSImportClassTypeReference`, …) in a
file that binds its names through **CommonJS**: `require(…)`, `module.exports`,
`exports.x`.

That is not a counterexample to the argument; it is the argument. The
ES-declaration escape exists because a file whose names arrive through module
machinery this port only partly has is a file where an unresolved name may be
the *port's* miss. A CommonJS file is exactly that file.
`file_has_import_machinery` cannot see it, because it looks for
`ImportDeclaration | ImportEqualsDeclaration | ExportDeclaration` and a
CommonJS file has none of the three.

### The shape that landed

A sibling predicate, [`Checker::file_has_commonjs_machinery`], asking the same
question over the other module system: does the file reference `require`,
`module` or `exports`? Same walk, same per-root cache, same "can an unresolved
name here be our own gap?" semantics. The gate becomes

```rust
|| (self.in_js_file(id) && !self.file_has_commonjs_machinery(id))
```

and measures **198 WRONG→RIGHT with zero adverse of any kind** — the 11
RIGHT→GAP and the 32 WRONG→GAP both disappear, because both were the same
population.

### The alternative rejected

Broadening `file_has_import_machinery` itself to also match those three
identifiers would have been one predicate instead of two. Rejected: it would
silently change the answer for **TypeScript** files that happen to name a
variable `module` or `exports`, which is a different question with a different
(and unmeasured) answer. The two predicates share a meaning, not a call site.

### How we would know this is wrong

By name, not by shape: `file_has_commonjs_machinery` matches any identifier
spelled `require`/`module`/`exports` anywhere in the file, including a local
variable that has nothing to do with modules. That over-matches toward the
*conservative* answer (`any`, the pre-§784 behaviour), so its failure mode is
lost conversions rather than new wrong lines. If a later measurement shows a
JS case still printing `any` where the oracle wants `error`, this predicate is
the first thing to narrow — to a `require(…)` CALL and a `module.exports` /
`exports.x` ASSIGNMENT, rather than a bare name.

## §785 — `Record<K, V>` with a primitive key carries an index signature

### The gap, named in the module's own doc for a long time

`index_signatures.rs`'s module doc has listed "index signatures on a **class**
and on a mapped type" as unported. `Record<K, V>` is `{ [P in K]: V }`, a
mapped type, so it has been in that gap the whole time.

Upstream reaches the answer in `resolveMappedTypeMembers` (`checker.go`): it
walks `getLowerBoundOfKeyType(constraintType)` and, for each key, either makes
a PROPERTY (when the key is usable as a property name) or an **index info**
(when it is not — which is exactly `string`, `number`, `symbol` and the pattern
types).

### Why the property road did not already cover it

§45 special-cased `Record<string, V>` in `Checker::record_string_value`
(`members.rs:315`), consulted from the PROPERTY access road. That was the right
call for `r.k`, and it never fires for `m[i]` — an element access does not ask
for a property by name. So a `Record` element access fell through
`element_access_lookup` to `errorType`, printed `any`.

Verified by probe against a control rather than assumed: in the lib-less
harness, `interface R { [k: number]: string }` answers `string` for `m[i]`
while `Record<number, string>` answers `error`. The plain index signature works;
the mapped spelling does not.

### The shape

A sibling of `record_string_value` in `index_signatures.rs`, consulted from
`get_index_infos_of_type` ahead of the `TypeData::Named` members road, keyed on
`type_reference_targets` and the global `Record` symbol, restricted to
`arguments[0] == string || arguments[0] == number`.

Because it lands in `get_index_infos_of_type` rather than in the element-access
road, it serves every consumer of index infos at once — element access, the
apparent-type twin, and the relater.

### Why `string` and `number` only

`Record<"a" | "b", V>` must produce PROPERTIES. Handing back an index signature
for it would make `r.c` answer `V` where upstream errors — a confident wrong
answer in place of a missing one, which is the trade this project refuses
everywhere. The restriction is upstream's own line: an index info is what you
get for a key *not usable as a property name*.

### The measurement

```
TOTAL 474243  right 436910  gap 7798  wrong 29535
GAP->RIGHT:   74   objectSpreadRepeatedComplexity 59, controlFlowComputedPropertyNames 10, discriminantNarrowingCouldBeCircular 3
WRONG->RIGHT: 126  compiler/temporal 116, controlFlowComputedPropertyNames 4, useBeforeDeclaration_destructuring 3
```

**+200, zero adverse of any kind**, +1 case, gradient 91.20% → 91.24%.

Two things in that are worth reading rather than skimming:

- **The `temporal` 116 is a prediction that held.** STATUS §4.-5 split that
  case's 339 wrong lines and attributed **117** of them to the port answering
  `any`. This arm converts 116 of them. That is the first time on this board a
  cause-split predicted an arm's size before it was measured, and it is the
  argument for splitting a case before pricing it.
- **74 of the 200 are GAP→RIGHT, in cases that are not about `Record` at all**
  (`objectSpreadRepeatedComplexity` 59). Index infos are consumed by the spread
  and relater roads too, so an index signature that did not exist was failing
  those independently. The arm is wider than its name.

### The two risks, recorded before the run and settled by it

1. *Relater movement.* `get_index_infos_of_type` is consulted by assignability,
   so this could have moved relations adversely. It did not — zero R→W, zero
   R→G.
2. *The early return.* The arm returns ahead of the `TypeData::Named` members
   road, which is safe only while a `Record` reference carries no members table
   of its own. That holds **today** precisely because mapped-type member
   resolution is unported. **If mapped members are ever ported, this early
   return becomes a shadow and must move below the `Named` road.**

### How we would know this is wrong

The arm is keyed on the global `Record` symbol, so a corpus file declaring its
own unrelated `Record` in global scope would get an index signature it should
not have. Nothing in the measurement shows that, and the failure would be
W→R-shaped noise rather than a regression, but it is the falsifier: the general
fix is to port `resolveMappedTypeMembers` and delete both this and §45's
`record_string_value`.

## §786 — the deferred indexed access, on the EXPRESSION road

### The rule

`getIndexedAccessType` (`checker.go`) does not resolve when
`isGenericObjectType(objectType) || isGenericIndexType(indexType)`. It builds an
`IndexedAccessType` that prints as written and is resolved only at
instantiation. `x[k]` with `x: T, k: K extends keyof T` is `T[K]`, not `any`.

The ANNOTATION half has been ported since §619–§626: `declared.rs`'s
`IndexedAccessTypeNode` arm mints `T[K]` through the §31 mint, with §625's
concrete/generic asymmetry already measured there. This is its EXPRESSION twin,
minting the same way so the two spellings cannot print differently.

### Where it goes, and the zero-movement lesson

Placed at the tail of `element_access_lookup`, the arm measured **zero
movement**. That is not a small miss; it is the wrong reading of the function.
`element_access_lookup` short-circuits:

```rust
let Some(name) = self.property_name_from_index(index_type) else {
    if let Some(info) = self.get_applicable_index_info(...) { return ...; }
    return error;          //  <-- the whole generic family exits HERE
};
```

A generic index names no property, so `property_name_from_index` yields `None`
and the function returns from *that* branch, never reaching its own tail. The
arm belongs in the branch.

STATUS §4.-5 had recorded "no generic-object arm anywhere in that chain, so the
`any` is the final `error`" — the chain reading was right and **the exit point
named in it was wrong**, which is why that entry was written as *the absence is
verified, the conversion is not*.

### The gate is `keyof` of THIS object

Deferring on "the index is generic" measured **6 RIGHT→WRONG**, all in one
fixture (`mappedTypeRelationships.types:107-123`):

```ts
function f6<T, U extends T, K extends keyof U>(x: T, y: U, k: K) {
    x[k] = y[k];
}
>y[k] : U[K]      // defers
>x[k] : any       // does NOT defer
```

`K` indexes `U`, and `U extends T` makes `keyof T` a **subset** of `keyof U`,
not the reverse — so `x[k]` is upstream's reported error, printed `any`, which
this port already spelled `errorType`. The arm therefore requires the index's
constraint (or the index itself) to be the deferred `keyof` mint **of this
object**.

That relation is decided **by name**, not by the relater, and the reason is
structural rather than lazy: the thing being compared is a §35 deferred `keyof
X` mint, a named `TypeFlags::ANY` type the relater cannot relate to anything.
Deciding by name is narrower than upstream — it declines a `keyof` reached
through an alias — and narrower is the correct direction here, because
declining leaves the `any` this road already printed while a wrong defer prints
a confident type.

### `deferred_keyof_types`

§35 mints `keyof T` as a plain unresolved named type; nothing in its flags says
it is generic, and `unresolved_types` also holds §31's mints for names that
simply did not resolve. A side set (ADR-0003, not a `TypeData` widening)
records which mints are `keyof`. Without it, `isGenericIndexType` would have to
be recovered from printed text, which is not sound.

### The measurement

```
TOTAL 474243  right 436995  gap 7787  wrong 29461
GAP->RIGHT:    8   typeGuardOfFormTypeOfFunction 4, asyncFunctionReturnType 3, ...
WRONG->RIGHT: 77   mappedTypeRelationships 46, keyofAndIndexedAccessErrors 11, quickinfoTypeAtReturnPositionsInaccurate 6
GAP->WRONG:    3 ⚠
```

**+85, zero RIGHT→WRONG, zero RIGHT→GAP.** Gradient 91.24% → 91.26%.

### The three GAP→WRONG, named

This arm fires **only where the road already answered `error`**, so its failure
direction is gap→wrong and never right→wrong — §620's own stated reason for
accepting that direction on the annotation road. All three are a *different*
unported road becoming visible, not this arm answering wrongly:

| line | oracle | port | the road that is actually missing |
|---|---|---|---|
| `asyncFunctionReturnType:0:111` | `Promise<Awaited<TObj[K]>>` | `Promise<TObj[K]>` | `Awaited<T>` is unported; the `TObj[K]` inside is correct |
| `typeVariableTypeGuards:0:87` | `NonNullable<T>[K]` | `T[K]` | the object was not NARROWED — flow narrowing of a type parameter |
| `typeGuardOfFormTypeOfFunction:0:84` | `Function.apply`'s signature | `any` | not an indexed access at all; walk-order sensitivity (§783) |

Each is worth more than its line: the first two say the deferred print is being
built correctly and the *context* around it is what is missing, which is the
best evidence available that the mint itself is right.

### How we would know this is wrong

The by-name relation is the falsifier. If a corpus case defers where upstream
resolves — a `keyof` reached through an alias whose print happens to match, or
two distinct type parameters printing the same name in different scopes — this
gate is what let it through. The general fix is a real `isGenericIndexType`
over a real `keyof` type, which needs `keyof` to stop being a printed mint.

## §787 — `Array`/`ReadonlyArray` are one inference target, and two larger fixes that did NOT pay

### What landed

`infer_from_types_within` compared two reference targets by identity. A
`readonly T[]` parameter is a `ReadonlyArray<T>` reference; an array-literal
argument is an `Array<number>` reference; the identity test refused, `T`
collected no candidate, and the call answered `errorType` — printed `any`.

Upstream infers argument-wise across that pair (`inferFromObjectTypes`,
`inference.go`): a mutable array IS a readonly one and their single type
argument occupies the same slot. `Checker::is_array_like_pair` admits exactly
the two globals, in either order.

Restricted to those two rather than any structurally-compatible pair, because
inference that admits a target it cannot justify produces a **candidate**, and a
wrong candidate is a confident wrong answer rather than a missing one.

Measured **+4** (3 GAP→RIGHT, 1 WRONG→RIGHT) against 1 GAP→WRONG, zero
RIGHT→WRONG, zero RIGHT→GAP.

### Why the target was `compiler/setMethods`, and why it did not move

The §4.-5 board put `setMethods` at 180 wrong lines against 37 right, with
**177 of the 180 the port answering `any`** where the oracle wants `Set<number>`
— the most concentrated `any` block left on the board after §784–§786.

`Set : SetConstructor` is RIGHT in that case, so the lib IS loaded and this was
never a lib-resolution problem. A probe ladder against the real
`SetConstructor` shape isolated **three** independent blockers, and the honest
outcome is that only the first is worth having:

| probe | before | after the landed arm |
|---|---|---|
| `new C<T>(v: readonly T[])` | `error` | `S<number>` |
| `new C<T>(v: readonly T[] \| null)` | `error` | `error` |
| `new C<T = any>(v: T[])` | `S<any>` | `S<any>` |
| `new C<T = any>(v?: readonly T[] \| null)` **(the real shape)** | `S<any>` | `S<any>` |

### Refused (a): widening the union strike-out — ZERO movement

The union arm strikes a target constituent the source is assignable to, guarded
by `!parameters.contains(&c)` — which excludes a NAKED `T` and nothing else. So
`readonly T[] | null` was struck whole (an `Array<number>` IS assignable to
`ReadonlyArray<T>`) and `T` collected nothing.

Upstream cannot reach that state: `inferToMultipleTypes` (`inference.go:700`)
matches constituents IDENTICALLY in its first pass, and a constituent carrying
an uninferred parameter is identical to nothing. Striking on assignability is
this port's approximation and is sound only where the constituent is closed.

Narrowing the strike to skip any constituent that MENTIONS an inference
parameter fixed the probe (`readonly T[] | null` began inferring) and measured
**exactly zero corpus transitions**. The shape is real and the fix is right;
the corpus does not reach it independently of blocker (b), which sits in front
of it on every case that would have shown it.

### Refused (b): §44's all-defaulted shortcut — −4 net, with RIGHT→WRONG

`get_signature_of_named_type` (`signatures.rs`) instantiates an ALL-DEFAULTED
generic signature with its own defaults and strips the type parameters (§44).
For `new <T = any>(values?: readonly T[] | null): Set<T>` that answers
`Set<any>` and **preempts the inference road entirely** — which is why every
probe carrying a default stayed `S<any>` no matter what inference could do.

Declining the shortcut when the signature's VALUE parameters mention a type
parameter fixed all four defaulted probes. On the corpus it measured
**2 RIGHT→WRONG (`genericDefaults`) and 2 RIGHT→GAP
(`contextualTypesNegatedTypeLikeConstraintInGenericMappedType1/3`)** for no
compensating conversion — net −4. Reverted.

**Why it went backwards is the useful part**: the shortcut is not merely a
fallback, it is load-bearing for cases where inference then produces a *worse*
answer than the default. Removing it exposes the inference road's own gaps on
cases that were passing by accident.

### What actually blocks `setMethods`, named

The real `SetConstructor` has **two** generic construct signatures
(`readonly T[] | null` and `Iterable<T> | null`). With the shortcut declined,
the probe with two overloads answered `error` rather than `S<number>` — so past
(a) and (b) the case lands on **generic overload selection**, which is
`get_signature_of_named_type`'s own already-recorded refusal: *"A generic
candidate (926 lines). That is `inferTypes`, the largest gate in `callgate.rs`'s
own split."*

So `setMethods`'s 180 lines are **not** reachable by any local inference fix.
They are downstream of generic overload selection, and the prerequisite has a
name and a size already on the page. Recorded here so the next session does not
re-derive the ladder.

### How we would know the landed half is wrong

`is_array_like_pair` resolves `Array` and `ReadonlyArray` through
`global_type_symbol`. A corpus file declaring its own global `ReadonlyArray`
would make the pair admit an unrelated reference. The 1 GAP→WRONG
(`jsxGenericComponentWithSpreadingResultOfGenericFunction:0:13`) is the line to
re-read first if this is ever suspected.

## §788 — generic construct overloads are ALTERNATIVES, not agreements

### The rule that was wrong

The `new` road's generic-candidate walk (`expressions.rs`) answered only when
**every** candidate produced the same return:

```rust
match agreed {
    None => agreed = Some(answer),
    Some(t) if t == answer => {}
    Some(_) => { ok = false; break; }
}
```

That is right for a set that is really one signature seen several ways —
`DateConstructor`'s four construct signatures all return `Date`, which is the
shape the walk was built for — and **wrong for a genuine overload set**, where
the candidates are alternatives and only one is meant to fit.

`SetConstructor` is the head case:

```ts
new <T = any>(values?: readonly T[] | null): Set<T>;
new <T = any>(iterable?: Iterable<T> | null): Set<T>;
```

`new Set([0, 1, 2])` fits the first and not the second. The second answered
`errorType`, `ok` went false, and the whole call declined to a gap.

### What upstream does

`chooseOverload` (`checker.go`) walks the candidates **in order** and takes the
first one the arguments fit. It never consults the rest, and it never requires
the candidates to agree about anything.

### What landed, and what stayed refused

The arm is the first-applicable rule restricted to what this port can decide
without the relater:

- **arity accepts the argument count** — `Checker::arity_accepts`, which is
  `hasCorrectArity` reduced to the minimum (leading non-optional, non-rest
  parameters) and maximum (unbounded with a rest);
- **the generic resolution does not answer `errorType`**.

Assignability-ranked selection among several *fitting* candidates stays the
already-recorded 926-line refusal. This arm does not need it: where several
candidates fit, upstream's answer is the first, and so is this one.

It runs **only where the agreement walk already declined**, so its failure
direction is gap→wrong and it can take no right line away.

### The measurement

```
TOTAL 474243  right 437264  gap 7708  wrong 29271
GAP->RIGHT:    69  intlNumberFormatES2023 38, intlNumberFormatES2020 17, localesObjectArgument 12
WRONG->RIGHT: 196  compiler/setMethods 126, intlNumberFormatES2023 27, overloadResolutionConstructors 17
GAP->WRONG:     6 ⚠  zero RIGHT->WRONG, zero RIGHT->GAP
```

**+265** — the largest arm of the session, and it closes the case §787 had just
finished refusing.

### §787's refusal was right about the blocker and wrong about its size

§787 measured three blockers in front of `setMethods` and named the third as
*"generic overload selection, already refused at 926 lines"*, concluding that
those 180 lines were **not reachable by any local fix**.

The blocker was correctly identified. The conclusion did not follow: *selecting
among fitting candidates by assignability* is the 926-line item, but *taking the
first candidate that fits at all* needs none of it, and that is what upstream
does. 126 of `setMethods`'s 180 lines converted on the smaller rule.

**The lesson is about how a refusal gets sized.** §787 priced the whole road
from the hardest thing on it. A refusal should name the cheapest rule that would
answer the case, not the most complete one — otherwise it refuses work that was
never required. This entry is the counterexample to its own predecessor, one
commit later.

### The CALL twin was tried and reverted

The same arm on `resolve_call_signature`'s named-callee road measured **+36
more** but introduced **1 RIGHT→WRONG** (`parseErrorDoubleCommaInCall`) and 4
further GAP→WRONG. The reason is structural and worth recording: on the call
road, returning `None` from that block is not the end — the caller has its own
generic road (`calls.rs:646`, `check_generic_call` on the resolved signature)
that handles these better. The arm **short-circuited a better road**, which is
the one thing an additive fallback must not do.

The `new` road has no such successor, which is why the identical rule is safe
there and not here. **Re-attempt the call twin only as a last resort after that
caller's road, not inside this block.**

### How we would know this is wrong

`arity_accepts` decides fit without looking at the argument *types*, so a
candidate whose arity matches but whose parameters the arguments do not satisfy
will be picked when an later candidate was meant. The 6 GAP→WRONG
(`intlNumberFormatES2020/ES2023`, `typesWithSpecializedConstructSignatures`) are
where to look first; the general fix is the assignability ranking that remains
refused.

## §789 — an object literal in a MIXED-ARITY overloaded call keeps its contextual type

### The road, and why three earlier readings missed it

`symbols.rs`'s §56.3 supplies the contextual member root for an object-literal
argument. It resolves the call's signature and reads the parameter at this
index. For an overload set, `resolve_call_signature` answers the **first**
candidate without consulting arity — so a mixed-arity set handed this road the
wrong parameter list entirely.

```ts
declare function g(x: number, o: { u: "a" | "b" }): void;
declare function g(o: { u: "a" | "b" }): void;
g({ u: "a" });
```

The road looked for `u` on the first overload's `x: number`, found nothing, and
left the literal to widen: `{ u: string; }` where the oracle records
`{ u: "a"; }`.

This family had **three** readings on this page before this one, and the first
two were wrong:

1. *a missing `isLiteralOfContextualType`* — refuted, it is ported at
   `signatures.rs:2190`;
2. *"the contextual type is not reaching the object literal through an
   overloaded call"* — right in outline, too broad to act on;
3. *"the SS114 arity discriminator in `contextual.rs` does not fire"* —
   **also wrong, and instructively so**: instrumenting
   `contextual_type_for_argument_resolving` printed nothing on any probe rung
   INCLUDING the ones that already worked, which should have said immediately
   that the function was not on this road at all. It is not. Object-literal
   member context comes from `symbols.rs` §56.3, and `contextual.rs`'s
   discriminator governs a different question.

The lesson is the cheap one: when instrumentation is silent on a case that
**works**, the instrument is in the wrong place — that is a stronger signal
than silence on a case that fails, and it arrives for free.

### The probe ladder that made it cheap

| probe | answer |
|---|---|
| single signature | `{ u: "a"; }` ✅ |
| plain `string` property (control) | `{ u: string; }` ✅ |
| method, single signature | `{ u: "a"; }` ✅ |
| two overloads, **same arity** | `{ u: "a"; }` ✅ |
| two overloads, **mixed arity**, function | `{ u: string; }` ❌ |
| two overloads, **mixed arity**, method | `{ u: string; }` ❌ |

Same-arity sets already work through §70's agreement path. **Only mixed-arity
fails** — which turns "contextual typing through overloads" into "consult
arity", a rule this port already had in `Checker::arity_accepts` (§788, one
commit earlier).

### The rule, and why it is stricter than §788's

The resolved candidate is kept only when its arity accepts the call; otherwise
the **unique** arity-accepting candidate supplies the context, and a tie
declines.

§788's `new` road takes the *first* fit. This road demands *uniqueness*, and the
asymmetry is deliberate: §788 supplies an **answer**, where being wrong shows up
directly as a wrong line; this supplies a **contextual type**, where being wrong
silently retypes the argument and can cascade. Declining merely leaves the
widening that was already there, so the conservative direction is free.

### The measurement

```
TOTAL 474243  right 437289  gap 7708  wrong 29246
WRONG->RIGHT: 25  arrayToLocaleStringES2020 13, arrayToLocaleStringES2015 12
```

**+25, zero adverse transitions of any kind.** Gradient 91.31% → 91.32%.

### What it did NOT convert, and what that means

`compiler/temporal`'s 78 contextual-literal lines did **not** move.
`startOfMoonMission.until(endOfMoonMission, { largestUnit: "hour" })` is a
mixed-arity overload set too, so something else gates it — the argument is at
index 1 rather than 0, and the receiver is a generic class instance. **The
contextual-literal family is therefore still open past this arm**, and it should
not be priced at the 480 the §4.-5 census gave it: that number is the whole
widening family across every cause, and this arm converted 25 of it.

### How we would know this is wrong

`arity_accepts` decides fit without looking at argument types, so a mixed-arity
set whose unique arity match is nevertheless the wrong overload will supply a
wrong context. Nothing in the measurement shows it (zero adverse), and the
uniqueness requirement makes it rarer than §788's first-fit rule, but it is the
same falsifier: the general fix is assignability-ranked selection, still refused.

## §791 — variadic tuple normalisation for a generic alias, and what its +2 proves

### The rule

`type TV0<T extends unknown[]> = [string, ...T]` instantiated as `TV0<[boolean]>`
is `[string, boolean]` upstream — the rest element is **spliced**, not
substituted. This port printed the alias reference.

### The obvious route, and why it is the wrong one

`instantiate_type`'s **Arm 6** (`inference.rs:1417`) substitutes a tuple
element-wise through `tuple_element_lists` and re-mints with
`create_tuple_type(elements, readonly)`. A §40 *print-only* variadic has no
element-list entry — that is what "print-only" means — so Arm 6 answers
`errorType`.

Teaching Arm 6 to splice would need per-element rest-ness in the tuple
representation: `create_tuple_type` takes a flat `Vec<TypeId>`, and
`tuple_rest_tails` maps a whole tuple to a NODE rather than its elements. That
is a type-model change, and it is what STATUS §5's §790 entry refused as a
subsystem.

**None of it is needed.** §40's structural road *already* splices a rest over a
concrete tuple — that is how `excessivelyLargeTupleSpread` works. All it lacked
was the type parameter being concrete. So this arm:

1. records the NODE behind every print-only variadic mint
   (`variadic_tuple_nodes`, ADR-0003);
2. at an alias instantiation, binds the alias's type parameters to the arguments
   with §91's own `alias_evaluation_bindings` frame;
3. **re-resolves the recorded node**.

Nothing substitutes anything. The existing splice runs, now over concrete
arguments.

### The guard that the first run required

A variadic body can reference its own alias, and the re-resolve re-enters this
road: the first corpus run **overflowed the stack**. Two fixes, both kept:

- a syntactic gate — only a tuple body carrying a rest element is considered, so
  arbitrary alias bodies are never resolved eagerly just to find out;
- `variadic_alias_in_progress`, a per-symbol sentinel around the re-resolve.

### The measurement, and the estimate it corrects

```
WRONG->RIGHT: 2  conformance/variadicTuples1 2
```

**+2, zero adverse of any kind.** That number is the finding.

STATUS §5's §790 entry priced this subsystem at roughly **780 lines** —
`variadicTuples1` 281, `strictBindCallApply1` 204, `genericRestParameters1` 161,
`variadicTuples2` 138 — on the reasoning that all four are "the same road".
They are the same *feature*; they are not the same *position*. This arm
normalises variadic tuples in **annotation** positions and converts 2 lines,
because 224 of `variadicTuples1`'s 281 wrong lines are the port answering `any`
in **expression** positions: array literals with spreads, `as const`, and
`bind`-shaped signatures.

**The correction is the reusable part.** §790 counted four cases that share a
language feature and called the total one subsystem's worth. A population
sharing a feature is not a population sharing a fix — the expression half needs
spread-element typing in array literals and `as const` normalisation, which are
different builds from this one. The remaining variadic estimate should be
re-derived per POSITION, not per feature, and until that is done it has no
number.

### How we would know this is wrong

The re-resolve runs §40's structural road a second time for the same node under
a binding frame. If a future change makes that road stateful in a way the frame
does not cover, an instantiation could see a stale splice. The sentinel prevents
recursion, not staleness; the `a_non_tuple_argument_declines_rather_than_guessing`
test pins the one case where declining is the right answer.

## §792 — a tuple spread inside `as const` splices, and the per-POSITION re-derivation that found it

### The rule

`[1, ...t] as const` with `t: [boolean]` is `readonly [1, boolean]` upstream.
`check_const_assertion` (`assertions.rs`) declined any spread outright and
answered `errorType`.

### The refusal's stated reason did not cover the case

§105's comment gave it as: *"spreads, holes, and object elements decline the
operand whole — readonly MEMBERS are slice 2, behind the value-spelling
carriage."*

That reason is real for holes and object elements. It is **not a reason for a
tuple spread**, which needs no member machinery at all — only the operand's
element list, which this port already keeps in `tuple_element_lists`. The
refusal had bundled three unrelated element kinds under one justification, and
the justification only fitted two of them.

**A refusal's stated reason has to be re-read against the case in hand rather
than inherited from the sentence it appears in.** That is the same failure shape
as §787 (priced from the hardest problem) and §790 (priced from the widest
feature), arriving a third way.

### Where the case came from

§790 refused variadic tuples as a subsystem at ~780 lines. §791 built the
ANNOTATION half for **+2** and corrected that estimate: the four cases in it
share a language feature, not a fix, and the number had to be re-derived **per
position**. Doing that put the expression half in two places — array literals
and `as const` — and a five-rung probe found the literal road already correct
(`[1, ...t, 2]` answers the widened array upstream does) with `as const` the one
that failed.

Worth **+80**, against §791's +2 on the annotation half.

### Upstream's cap, transcribed

The first measurement was +78 net with **2 RIGHT→WRONG** in
`compiler/excessivelyLargeTupleSpread` — a case that exists to exercise exactly
this bound. Upstream:

```go
if len(spreadTypes)+len(n.types) >= 10_000 {
    // Expression produces a tuple type that is too large to represent
    c.error(c.currentNode, message)
    return false
}
```
(`checker.go:23379`)

With the cap the same measurement is **+80 and zero adverse of any kind**. The
port would otherwise have built the giant tuple, which is the failure mode a
"faithful in the common case" splice invites: the bound is part of the rule, not
an implementation detail of upstream's.

### The restriction

Only operands that HAVE an element list splice. A spread of an array, or of
§40's print-only variadic, still declines — there is nothing to splice and
inventing a length would be a confident wrong answer where a gap belongs.

### How we would know this is wrong

The splice reads `tuple_element_lists` and drops the operand's optional mask
(`tuple_optional_masks`). A spread of a tuple with optional elements —
`[a?, b?]` — therefore splices as if every element were required. No corpus line
shows it, and the const-assertion position makes it rare, but it is the first
thing to check if a `readonly [...]` line disagrees about optionality.

## §793 — `[...T]` as a parameter: two gaps that each measured ZERO alone

### The rule

```ts
declare function f<T extends unknown[]>(t: [...T]): T;
f([1, 2]);   // T := [number, number]
```

This port answered `errorType`.

### Why it needed two fixes, and why neither could be measured on its own

**(1) Inference.** `[...T]` is built as a §40 print-only variadic — a named type
with no element list and no reference target — so every arm of
`infer_from_types_within` missed it and `T` collected no candidate at all.

**(2) Contextual typing.** Every arm of `array_literal_tuple_context_kind` reads
an ANNOTATION: a type assertion, an annotated declaration, an assignment target.
A **call argument** has none of those, so the literal stayed `number[]` however
the parameter was spelled.

Landing (1) alone made the call answer `number[]` where it had answered a gap —
a confident wrong answer replacing an honest one — and the corpus reported it
correctly as **zero transitions**. Landing both is **+22**
(`variadicTuples1` 12, `variadicTuples2` 6, `restTupleElements1` 3), zero
adverse.

**This is the failure mode that the per-arm measurement discipline is blind to.**
A fix whose partner is missing reads as "no effect" and looks refusable; §793's
first half would have been written off as a dead end on its own number. The
signal that it was not is that the unit probe MOVED (`error` → `number[]`) while
the corpus did not — a mechanism that works but produces the wrong answer, which
is a missing partner rather than a wrong idea. **A probe that moves and a corpus
that does not is the shape of an incomplete chain, not of a useless arm.**

### What is admitted, and what is not

Only a SINGLE rest over a name resolving to one of this inference's own
parameters. `[...T]` is upstream's idiom for *"this parameter is the whole
tuple"*. Anything with a leading or trailing element — `[string, ...T]`, or two
rests — needs the source SPLIT across positions, which is tuple-splitting
machinery this port does not have. Those keep declining, and
`a_variadic_with_a_leading_element_still_declines` pins it.

### The running per-POSITION tally

§790 refused variadic tuples as one ~780-line subsystem. Re-derived per
position, after §791's correction:

| position | arm | lines |
|---|---|---:|
| annotation (alias instantiation) | §791 | +2 |
| expression — array literal | probed, already correct | 0 |
| expression — `as const` | §792 | +80 |
| call argument — `[...T]` parameter | §793 | +22 |

**+104 so far against an estimate of ~780**, and the remaining
`variadicTuples1` residue is now mixed shapes rather than one road. The estimate
was wrong in its number and right that the feature is large; what it got wrong
was treating "shares a feature" as "shares a fix", which four separate arms at
four separate positions have now demonstrated.

### How we would know this is wrong

The contextual half fires for ANY call argument whose parameter type is a
recorded variadic node, including one this port built for a shape it cannot
infer through (`[string, ...T]`). There the literal now mints a tuple where it
previously widened, and the inference half still declines — so the argument's
own printed line changes while the call stays a gap. No corpus line shows a
regression from it, but that is the interaction to check first.

## §795 — the IIFE spread-argument legs, and a refusal cashed twice

### The rule

```ts
declare const t1: [number, boolean, string];
(function (a, b, c) {})(...t1);   // a: number, b: boolean, c: string
(function (...x) {})(...t1);      // x: [number, boolean, string]
```

`getSpreadArgumentType`'s legs at `checker.go:29504` and `:29528`. §771 declined
every spread argument outright and recorded these as its residue.

### The chain this closes, which is the reason to write refusals properly

| | |
|---|---|
| **§769** | wrote the IIFE REST arm, measured **21:9 adverse**, reverted it |
| **§770** | landed the prerequisite §769 named |
| **§771** | re-applied §769's arm, exactly as its refusal predicted |
| **§795** | the residue §771 itself recorded |

§769's arm was **correct** and its measurement was **bad**, and the entry
separated those two facts instead of collapsing them into "does not work". What
broke was downstream: a tuple inherited no `Array<T>` members then, so giving a
parameter its true tuple type took `noNumbers.some(…)` from RIGHT to GAP. The
refusal named the prerequisite in one sentence — *a tuple's apparent type must
include the members of `Array<union of its elements>`* — and said the arm would
afterwards be "a ~20-line re-application of code this entry describes".

Both halves came true. **A refusal that names its prerequisite precisely enough
is a work item, not a dead end**, and this one was cashed twice by later
sessions that only had to re-read it.

That it was re-checkable at all is why this session re-tested it: the probe was
four lines (`t.some`, `t.map`, `t.length` on a tuple), it confirmed §770 had
landed the prerequisite, and the arm followed. **Re-testing a refusal's stated
prerequisite is cheaper than re-deriving the refusal.**

### What is admitted

Exactly one spread over a CONCRETE tuple. A spread mixed with plain arguments
needs the position arithmetic `getSpreadArgumentType` does across several
sources; a spread of an array has no element to land on. Both keep declining,
and `a_spread_of_an_array_still_declines` pins the second.

### The measurement

```
WRONG->RIGHT: 16  conformance/restTuplesFromContextualTypes 16
```

**+16, zero adverse of any kind.**

### How we would know this is wrong

The rest leg takes `elements[index..]` as a tuple without consulting the
operand's optional mask, so a spread of `[a, b?]` into a rest parameter reports
both elements as required. The same gap is noted in §792's splice; if a
`readonly [...]` or parameter line ever disagrees about optionality, these two
arms are where it comes from.

## §797 — the `const` type-parameter argument context, and a refusal NARROWED rather than lifted or kept

### The rule

```ts
declare function f<const T>(x: T): T;
f(["b", "c"]);              // readonly ["b", "c"], not string[]
f(["a", ["b", "c"]]);       // readonly ["a", readonly ["b", "c"]]
```

`isConstTypeVariable` (`checker.go`) makes an argument position const exactly as
`as const` does, all the way down through nested literals.

### Why `is_const_context` could not answer it

That function walks the parent chain **syntactically** — parens, array
literals, spreads, property assignments, template spans — looking for a const
assertion. Const-ness here depends on the callee's **resolved signature**, which
is not in the tree. `array_literal_argument_of_const_type_parameter` asks it at
the same seam §793 used for the tuple-context arm, behind the
`resolving_signature_calls` re-entry guard, and climbs the same carriers so the
context reaches nested literals (that climb is the difference between **+8** and
**+14**).

### The decline, narrowed

§33 refused **every** call through a const-marked signature, because this port's
inference widens where upstream keeps literals — **70 GAP→WRONG** when written.
STATUS §5's §796 entry re-measured that at **42**, built the arm above, and
showed the 42 are *all one shape*:

```ts
declare function test1<const T>(create: () => T): T;
test1(() => ['a']);   // readonly ["a"]
```

There const-ness must cross a **function boundary** into the arrow's return
before the literal is reached, and nothing in this port carries a const context
across one.

So the decline now asks for exactly that shape — a parameter whose type has call
signatures — and every other const-marked call goes through inference with its
arguments correctly in const context.

| what was tried | result |
|---|---|
| lift the decline entirely | 42 GAP→WRONG against 8 WRONG→RIGHT |
| keep it whole | those 14 lines unreachable |
| **narrow it to the callback shape** | **+14, zero adverse** |

**A refusal narrowed to its actual cause beats both lifting it and keeping it.**
That is only available once the cause is measured rather than assumed — §33
stated a reason, §796 tested that reason and found it applied to a *subset*, and
this arm is the subset's complement.

### A residue, recorded rather than closed

Upstream distinguishes the LITERAL's line from the CALL's: `['a', ['b', 'c']]`
is `["a", ["b", "c"]]` — a tuple, **not** readonly — while the call is
`readonly ["a", readonly ["b", "c"]]`. The readonly comes from `getWidenedType`
over the const type variable, not from the literal itself. This port's const arm
mints the readonly tuple **at the literal**, so the CALL lines are right and the
LITERAL lines stay wrong. That is why a case carrying both converts about half.

Fixing it needs the readonly to move from the literal to the inference site,
which is the same const-type-variable machinery the callback shape needs. **Both
residues are one prerequisite**, and that is the next thing to build here.

### How we would know this is wrong

The arm fires on any array literal reaching a call argument through the climbed
carriers, without checking WHICH parameter it lands on. A signature with one
`const` type parameter and one ordinary parameter will put a const context on
both arguments. No corpus line shows it; the fix is to resolve the argument's
own parameter, which `contextual_type_for_argument` already does for other
questions.

## §798 — the `readonly` belongs at the INFERENCE site, not the literal

### The distinction upstream makes, and §797 did not

```ts
declare function f1<const T>(x: T): T;
const x12 = f1(['a', ['b', 'c']]);
>x12 : readonly ["a", readonly ["b", "c"]]     // the CALL
>['a', ['b', 'c']] : ["a", ["b", "c"]]         // the LITERAL — no readonly
```

The `readonly` comes from `getWidenedType` over the **const type variable**, not
from `checkArrayLiteral`. §797 minted it at the literal, which made every CALL
line right and every LITERAL line wrong — and said so, as a recorded residue.

### The move

Two edits, which only work together:

1. `array_literals.rs`'s const arm mints a **plain** tuple when the const
   context came from a const TYPE PARAMETER (`create_tuple_type(elements,
   !const_argument)`). An `as const` assertion is the other case and keeps the
   readonly there — `is_const_context` answers that one, and the two roads stay
   separate.
2. `inference.rs`'s resolution loop applies `Checker::readonly_tuple_image` to a
   candidate whose type parameter `is_const` — recursively, because the readonly
   goes all the way down.

Either alone is a regression: (1) without (2) drops the readonly from the call
lines §797 had just won; (2) without (1) double-applies it.

### The measurement

```
WRONG->RIGHT: 43  typeParameterConstModifiers 29, jsdocTemplateTag6 14
```

**+43, zero adverse**, on top of §797's +14 in the same case.

**The 14 lines in `conformance/jsdocTemplateTag6` were not predicted.** That
case is about JSDoc `@template` and has nothing to do with the shape this was
built for; it benefits because its templates carry const-marked parameters and
the same resolution loop serves them. Placing a rule at the site upstream places
it at reaches callers this port had not enumerated — which is the argument for
matching upstream's *location* and not only its *effect*.

### Why §797 got it wrong, and why that was still the right order

§797 had the readonly at the literal because that is where `as const` puts it,
and the first measurement (+14, zero adverse) confirmed nothing was broken. The
residue was visible only by reading the baseline's LITERAL lines next to its CALL
lines — the corpus score cannot distinguish "right for the right reason" from
"right for a reason that will not generalise".

Landing §797 first was still correct: it was clean, it was measured, and it made
the residue precise enough to fix in one step. **A clean arm with a recorded
residue is a better intermediate state than an unbuilt correct one.**

### How we would know this is wrong

`readonly_tuple_image` rebuilds every nested tuple readonly, including one that
arrived from somewhere other than the const literal — a tuple-typed variable
passed through a const parameter is now re-minted readonly whether or not
upstream would. No corpus line shows it, and upstream's `getWidenedType` over a
const type variable does the same thing, but a case where a const parameter
receives an already-readonly or explicitly-mutable tuple is where to look.

## §799 — the OBJECT half of the const road, and four attempts on one arm

### The rule

`f({ a: 1, b: 'x' })` on `<const T>(x: T)` is
`{ readonly a: 1; readonly b: "x"; }` upstream — the same `getWidenedType` over
a const type variable that §798 applied to tuples.

### Four attempts, and what each one cost

| attempt | change | result |
|---|---|---|
| 1 | reuse `const_context` on the object road | **−4** (readonly at the literal — the mistake §798 had just fixed for arrays) |
| 2 | split into `const_parameter_context` + `regular_members` | **0** (literal right, call not readonly) |
| 3 | build the object arm of `readonly_tuple_image` | **0** — *and this measurement was WRONG* |
| 4 | both halves together | **+39, zero adverse** |

**Attempt 3's zero was a stale build.** The arm and the split flag were both in
the tree, the probe was run against a binary that predated one of them, and the
entry written from it named a third blocker that does not exist. STATUS §5
carried that wrong conclusion for one commit.

**The lesson is narrow and mechanical**: when a measurement contradicts a probe
that just passed, rebuild before theorising. Three of this session's findings
came from trusting a zero (§793's incomplete chain, §796's inert half, §799's
attempt 2) and one came from a zero that was simply false. The two are
indistinguishable without a rebuild, and the rebuild is thirty seconds.

### What landed

- `objects.rs` gets `const_parameter_context` as a **second** flag.
  `const_context` means *"readonly regular members"* at seven call sites and
  only the regular-members half applies here; conflating them is attempt 1's −4.
- `readonly_tuple_image` gets an object arm: `spread_members_of` → mark every
  `Member::Property` readonly → `render_object_type` → re-mint on the same
  members symbol.

Flat only. `Member::Property` carries its type as printed TEXT, so a nested
object cannot be re-minted the way a nested tuple can (tuple elements are
`TypeId`s). A member list carrying a signature or an index declines whole rather
than marking half of it.

```
WRONG->RIGHT: 39  typeParameterConstModifiers 19, jsdocTemplateTag6 17,
                  typeParameterConstModifiersWithIntersection 3
```

**17 of the 39 are `jsdocTemplateTag6` again** — the same unenumerated caller
§798 picked up, for the same reason: the rule sits where upstream puts it.

### The residue, pinned in a test rather than described

Upstream records `{ readonly a: 1; readonly b: "x"; }`; this port keeps the
`readonly` and **widens the members**. The literal itself is right —
`check_object_literal` retains `{ a: 1; }` — and the re-mint throws that away,
because `spread_members_of` reads each member's type from the SYMBOL table
(`get_type_of_symbol`, the declared and widened type) rather than from the
literal's retained members.

`an_object_argument_gets_the_readonly_at_the_call` asserts the half-answer **as
it is**, with the cause in its doc comment. When the re-mint learns to carry the
literal's members, that assertion is what changes — which is a more useful
marker than a sentence in a notes file, because it fails when someone fixes it.

## §800 — the literal's own members, and a residue that reported its own repair

### The residue §799 pinned

§799 landed the `readonly` on an object inferred through a `const` type
parameter and **widened the members**: `{ readonly a: number; }` where upstream
records `{ readonly a: 1; }`. The cause was in the re-mint —
`readonly_tuple_image`'s object arm rebuilt through `spread_members_of`, which
reads each member's type from the `__object` SYMBOL via `get_type_of_symbol`,
i.e. the declared and widened type, rather than from what the literal had
actually printed.

The literal itself was right the whole time. `check_object_literal` retains
`{ a: 1; }` under a const context; nothing downstream remembered it.

### The fix

`object_literal_members` (ADR-0003) records the `Vec<Member>` a literal printed,
keyed by its minted type, beside the `fresh_object_literal_types` and
`object_literal_index_infos` tables that already sit there. The re-mint prefers
it and falls back to `spread_members_of` for everything else.

`Member` gains `#[derive(Clone)]`, which it did not have — the only reason it
did not is that nothing had needed to keep a member list past the mint.

### The measurement, and why this landed anyway

```
no transitions vs baseline
```

**Zero corpus movement.** No baseline line happened to depend on the
distinction: §799's 39 conversions were all cases where the `readonly` alone was
the difference.

What says the fix landed is the **test**. §799 pinned its residue as an
assertion of the half-answer —
`an_object_argument_gets_the_readonly_at_the_call`, asserting
`{ readonly a: number; }` — with the cause in its doc comment. §800 turned that
test red by making the port correct, and the assertion was then updated to
upstream's real answer.

**A residue pinned as an assertion reports its own repair. A residue described
in prose does not.** This is the first time in the session that technique paid,
and it paid in the case where the corpus was silent — which is exactly where a
prose note would have gone unread. The corpus is the arbiter of *value*; it is
not an arbiter of *correctness*, and a change that is right with zero
transitions still belongs in the tree if something else can witness it.

### How we would know this is wrong

The table is written at every object-literal mint, not only const ones, so it
grows with the literal count. It is read only by `readonly_tuple_image` today.
If a future reader uses it as a general member source, note that it holds the
members **as printed at mint time** — a literal whose members were later
narrowed or instantiated is not reflected, which is precisely the property that
makes it right here and would make it wrong there.

## §801 — a tuple target infers element-wise, found through a case it does not fix

### The gap

```ts
declare function f<T>(x: [T, T]): T;
f(t);   // t: [number, number]  →  number
```

answered `errorType`. A tuple is not a type REFERENCE in this port —
`create_tuple_type` mints a named type and records its elements in
`tuple_element_lists`, with no entry in `type_reference_targets` — so
`infer_from_types_within`'s reference arm never saw a tuple/tuple pair and no
candidate was collected at all.

Equal length only. A mismatch is upstream's variadic arithmetic, a rest element
absorbing several positions, which this port does not have; pairing positionally
across a mismatch would match the wrong source element to the wrong parameter.

### How it was found, which is the transferable part

The probe was `jsdocTemplateTag6`'s `f4<const T>(x: [T, T])` called with
`[[1, "x"], [2, "y"]]`. Printing the type of **every subexpression** showed:

```
CALL   error
OUTER  [[1, "x"], [2, "y"]]
INNER  [1, "x"]
  ELEM 1
  ELEM "x"
```

Everything inside was already right — the nested tuples, every literal, the
const context from §797–§800. Only the combination failed.

**"Everything inside is correct, the combination is not" localises to the
combining step, not to another arm of what is already working.** Four arms of
const-context work preceded this, and the natural next guess was a fifth. The
subexpression dump said inference instead, in one run.

### And it fixed a different family

```
WRONG->RIGHT: 13  strictOptionalProperties1 4, typeInferenceWithTupleType 4, tupleTypes 3
```

**Zero of the 13 are in `jsdocTemplateTag6`.** The case that exposed the gap
needs the variadic arithmetic above and is untouched. This is the third time
this session a rule placed where upstream places it paid out somewhere
unenumerated (§785's index infos, §798/§799's `jsdocTemplateTag6`) — and the
first time the *source* case got nothing.

### The residue, pinned as an assertion

`f<T>(x: [T, T])` with `[number, string]` should infer `string | number`; this
port answers `number`. The element-wise walk is right — both positions DO
contribute candidates, which is why the call resolves at all — and what does not
union them is the candidate-COMBINATION step (`getCovariantInference`), the same
rule every other multi-candidate position uses.

`differing_positions_do_not_yet_union` asserts the half-answer. §800 showed why
this is worth doing: when the combination rule lands, that test turns red and
reports its own repair, where a prose note would not.
