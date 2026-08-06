# Call and `new` resolution: an 18,314-line row that licenses 844 lines of work

Status: measured 2026-08-06 at `058b4a9` + this commit, over the 9,538-case
`.types` corpus population, from one pinned binary in an isolated worktree. The
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
R2 (spellability), conjunctive. **R1 fired at 27.4%. R2 did not fire, at 37.8%
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
3. **Only 15.1% of the row is a `calls.rs` defect** — 2,763 of 18,314 lines have
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

## 1. The row is 18,314 lines, and the 3,200 was a different unit

The assignment quotes `docs/architecture/checker-notes-recvgap.md` §4:

| terminal reason | assignment | this probe, whole-gradient |
|---|---:|---:|
| `… / VariableDeclaration / initialiser CallExpression` (BLOCK_SCOPED) | 1,124 | 952 + 1,643 |
| `expression answered error: CallExpression` | 791 | **11,739** |
| `… / VariableDeclaration / initialiser NewExpression` (FUNCTION_SCOPED) | 526 | 474 + 527 |
| **stated total** | **≈3,200** | **18,314** |

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

So the row's ceiling is **18,314 lines carrying the reason, plus 2,427 held
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
- They are not `VariableDeclaration`-only. 226 lines carry `PropertyAssignment`,
  `PropertyDeclaration` or `Parameter` initialisers. This is what control C2
  caught (§9).

## 2. Concentration (rule R3), and where the assignment's figure came from

Whole-gradient, per row. Unit: assertion lines.

| row | lines | cases | top-1 | top-10 | three largest cases |
|---|---:|---:|---:|---:|---|
| `… / initialiser CallExpression` | 4,864 | 818 | 14.0% | **33.1%** | `compiler/temporal` 681, `compiler/promiseType` 122, `compiler/promiseTypeStrictNull` 122 |
| `… / initialiser NewExpression` | 1,711 | 297 | 8.9% | **38.9%** | `compiler/typedArraysCrossAssignability01` 153, `conformance/parserRealSource11` 116, `compiler/duplicateLocalVariable1` 82 |
| `expression answered error: CallExpression` | 11,739 | 2,370 | 5.9% | **21.9%** | `compiler/temporal` 697, `conformance/parserRealSource11` 621, `compiler/genericDefaults` 242 |
| `expression answered error: NewExpression` (companion) | 1,985 | 672 | 4.7% | 20.9% | `conformance/parserRealSource7` 93, `conformance/parserRealSource11` 51, `conformance/unionTypeConstructSignatures` 44 |

**R3 does not fire.** The assignment's 90.2% and 68.4% top-10 figures are
properties of the *recvgap subset* — the lines those reasons hold down, which
concentrate because a few large files (`compiler/temporal`,
`conformance/parserRealSource*`) contain long receiver chains. The rows
themselves are corpus-wide: 818, 297 and 2,370 cases, top-10 between 21.9% and
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
reconciliation prints **479,060 = 292,217 right (61.00%) + 143,509 gap (29.96%) +
43,334 wrong (9.05%)**, identical to `receiver_gap.rs` at `7602d6b` — including
that page's unexplained 0.09pp difference against the committed `checker_types`
snapshot, which this probe reproduces and does not resolve (`open`).

## 4. R1 — the partition, and the bucket the rule was written on

Over the three assigned rows. Unit: assertion lines.

| bucket | lines | share | cases | top-10 |
|---|---:|---:|---:|---:|
| **callee HAS a type** — the prerequisite is met | **5,009** | **27.4%** | 977 | 14.7% |
| callee is `a.b`, receiver gaps | 5,158 | 28.2% | 622 | 55.5% |
| callee is `a.b`, member gaps | 3,210 | 17.5% | 652 | 37.6% |
| callee name resolves, symbol has no type | 3,111 | 17.0% | 615 | 25.5% |
| callee is another expression form, gapped | 1,074 | 5.9% | 397 | 19.2% |
| callee name does not resolve | 752 | 4.1% | 145 | 63.8% |
| **total** | **18,314** | | | |

**R1 fires: 27.4% ≥ 25%.** The bucket is the one the rule names, printed
literally, not a quantity derived from it.

Per row the split is very uneven, and it is the `new` half that carries it:

| row | callee has a type |
|---|---:|
| `… / initialiser NewExpression` | 1,133 of 1,711 — **66.2%** |
| `expression answered error: NewExpression` (companion) | 1,220 of 1,985 — 61.5% |
| `… / initialiser CallExpression` | 1,404 of 4,864 — 28.9% |
| `expression answered error: CallExpression` | 2,472 of 11,739 — 21.1% |

A `new` expression's callee is a class name, which this port types; a call's
callee is very often `a.b`, which it does not. **If the item were split by
keyword rather than by row, `new` would be the one that looks buildable.** §5
says why it is not.

### 4.1 The next link, where the callee gaps

The callee's own `gap_reason`, top rows. Unit: blocking call nodes, not lines
(one node can block several lines).

| callee's reason | count |
|---|---:|
| `property access, the receiver is a gap: Identifier` | 3,014 |
| `property access, the receiver is a gap: PropertyAccessExpression` | 1,432 |
| `reference, the name does not resolve` | 945 |
| `property access, the property has no type` | 841 |
| `reference, symbol has no type: SymbolFlags(FUNCTION) / FunctionDeclaration / neither` | **797** |
| `property access, the receiver has no such property: Promise<boolean>` | 491 (4 cases) |
| `property access, the receiver is a gap: CallExpression` | 397 |
| `reference, symbol has no type: SymbolFlags(FUNCTION) / FunctionDeclaration / annotation TupleType` | 389 |

The largest three are property-access chains, which is
`docs/architecture/checker-notes-calls.md`'s finding — *"it is a property-access
problem, not a call problem"* — arriving from a different direction and one level
further out. That page measured it on **nodes** and got 56.4%; this page measures
it on **lines** and gets 45.7% (5,158 + 3,210 of 18,314). Two instruments,
different units, same conclusion.

The 797 under `FunctionDeclaration / neither` is a distinct mechanism and is
filed (§8): an unannotated `function f() { … }` has no return-type inference here,
so every call to it gaps.

## 5. R2 — the spellability gate, which is where the item dies

The test is on the **baseline's own right-hand side** for the blocked line, so no
answer of ours enters it. `Plain` means a bare name or keyword; `Structural`
means it contains `<`, `{`, `[`, `|`, `&`, `(` or `=>`; `Any` means literally
`any`.

Over the 5,009 admitted lines (4,943 of which have a comparable baseline RHS):

| shape | lines | share |
|---|---:|---:|
| Plain | 1,867 | **37.8%** |
| Structural | 2,339 | 47.3% |
| Any | 737 | 14.9% |

**R2 does not fire: 37.8% against a 70% threshold.**

### It does not fire for any sub-shape either

Splitting the admitted bucket by what kind of type the callee has — this split is
**not** where the rule lives; R1 stays on the bucket above:

| callee's type shape | admitted lines | share | plain-shaped |
|---|---:|---:|---:|
| an object type (`TypeData::Anonymous`) — `calls.rs`/`signatures.rs` own it | 2,763 | 55.2% | 844 of 2,716 = **31.1%** |
| a `Named` type — a lib constructor interface or a class | 1,435 | 28.6% | 722 of 1,423 = **50.7%** |
| `any` | 608 | 12.1% | 238 of 601 = **39.6%** |
| a union, a literal, a non-`any` intrinsic | 203 | 4.1% | 63 of 203 = **31.0%** |

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
| `… / initialiser CallExpression` | 4,864 | 1,480 | **1.30×** | 92 | 76.5% |
| `… / initialiser NewExpression` | 1,711 | 947 | **1.55×** | 97 | 56.2% |
| total | 6,575 | **2,427** | 1.37× | | |

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

**16.6% plain against 37.8% for the target.** The lines this row holds down are
*less* spellable than the row itself — they are member reads off generic lib
types (`Promise<T>`, `Set<T>`, the typed arrays), which is exactly the family the
module-object item went negative on. So the cascade does not rescue the item; it
makes the case worse, and it would have been invisible to a sizing method that
counted only the row.

## 7. What is actually licensed, and why I refused it anyway

The intersection of both gates is: **the callee has an object type** (2,716 lines
with a comparable RHS) **and the baseline answer is plain-shaped** (844).

**844 lines. 0.59% of the 143,509-line gap.** Against an 18,314-line row and a
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
3. **The `any` bucket is a trap.** 608 admitted lines have an `any` callee, whose
   upstream answer is `any` (`resolveUntypedCall`, `checker.go:9902`). Porting it
   is faithful and it would move 608 lines from `gap` to `right` — and every one
   of those lines answers `any`, which is the column
   `docs/architecture/checker-notes-rank.md` §6 already flags as unaudited
   (21,685 lines banked on `any`, computed and defaulted not separated). Adding
   608 more to it before that audit exists would make the audit harder, for
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
| C1′ the mirror, kinds agreeing | 6,575 | — |
| C2 where the reason says `/ VariableDeclaration /`, the blocking node's parent is not one | **0** | an initialiser's parent *is* its declaration — a property of the subject |
| C2′ the mirror | 6,303 | — |
| C3 an `expression answered error:` line whose node does not carry the row's kind | **0** | `gap_reason` prints that kind from the same node table |
| C4 a downstream line that is itself in one of these rows | **0** | the reason prefixes are disjoint in `gap_reason` |
| C5 a reason matching more than one row test | **0** | `/ initialiser X` and `expression answered error: X` cannot co-occur |
| A1 rows == partition | 18,314 == 18,314, difference 0 | arithmetic |

**C2 fired, at 272, on the first run, and the defect was in the control.** It was
written unconditionally — *"the blocking node's parent is a `VariableDeclaration`"*
— and these rows also carry `PropertyAssignment`, `PropertyDeclaration` and
`Parameter` initialisers. The fix was to read the declaration kind out of the
reason string rather than to loosen the test. The 226 non-variable lines this
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
  over 18,314.
- **The navigation to the blocking node is wrong.** C1 or C2 would be non-zero.
  C2 already caught one error of exactly this class.
- **R2's `Plain` arm is a shape test and can be fooled.** It already is, by
  `unique symbol` (264 lines, §5). If someone finds a second such family the
  37.8% is an over-estimate again, and the refusal only gets stronger — but a
  family running the *other* way (a `Structural` RHS this port can in fact build,
  say a tuple or a union of literals) would weaken it. The verbatim RHS
  histograms are printed so this can be checked rather than argued.
- **The `callee has a type` bucket is a ceiling on `calls.rs`'s ownership, not a
  measurement of it.** 5,009 lines reach `resolve_call_signature` and it says
  `None`; *why* is not split here. If most of them stop at the
  `TypeData::Anonymous` destructure (that is, they are the `Named` shape) then
  the item is §8.1 and not a `calls.rs` item at all. That split is one more
  counter in this same probe, and it is filed rather than done (`open`,
  `bd tsr-klm`).
- **The cascade is measured only for the two `initialiser` rows.** The
  `expression answered error:` rows — 11,739 lines, the largest of the three —
  have no cascade figure here, because a call expression's downstream is not
  reachable by the receiver-chain walk this probe reuses. If those rows hold down
  a large spellable population, §6's verdict is understated in the favourable
  direction (`open`, `bd tsr-trf`).
- **The 0.09pp gradient difference against the committed `checker_types`
  snapshot** is reproduced here and still unexplained (`open`, inherited from
  `checker-notes-recvgap.md` §8). Nothing on this page is a share of that
  denominator, so no figure here moves if it is resolved — but the absolute
  right-count should not be quoted as the gradient's.
