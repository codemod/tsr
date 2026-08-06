# Structural assignability — the correction, and the item that is actually left

## 1. The correction: structural comparison landed 387 commits ago

`STATUS.md` §4.3b was written at `c4a34e3` and opens:

> `relater.rs` compares object types only to themselves, and its own module doc
> says a `false` between two distinct object types means "comparison is narrow",
> not "unrelated".

**The first clause is false and has been since `e24b7ca`, "Compare object types
structurally, over base symbols (tsr-4sc)"** — 387 commits before the sentence
was written, and an ancestor of the commit that wrote it. At `c4a34e3`:

- `relater.rs` has `properties_related_to`, `property_names_of` and
  `collect_property_names` — 95 lines walking every property of the target, own
  and inherited over base symbols, and relating each covariantly;
- the module doc's own "What is ported" list ends with **"structural comparison
  of object types"**;
- `tests/relater.rs` asserts it in six of its eighteen tests, including
  `two_structurally_identical_interfaces_relate`,
  `an_inherited_property_is_a_requirement` and
  `mutually_recursive_interfaces_terminate`.

This was not a small error. It sized the board's top item at ~2,100 lines across
seven dependents and directed a session to build a thing that exists.

### Where the false premise came from

Not from nowhere, and the mechanism is worth recording because it is still live
in this crate. Three doc sites disagreed with the code:

| site | what it said | status |
|---|---|---|
| `lib.rs` crate doc | "Object types relate only to themselves — structural comparison is not ported"; also "No inference, and no overload resolution", "No intersection types", "Nothing calls it yet", and a heading "Why `checker_types` still reads 0%" | **frozen at the 0% day**; `bd tsr-7wkn` |
| `relater.rs::is_type_assignable_to` rustdoc | "object types … answer `false` because structural comparison is not ported" | the *module* doc above it was updated by `e24b7ca`; this **item** doc was not |
| `calls.rs::SELECTABLE` rustdoc | "structural comparison is narrow, not because they are unrelated" | accurate then and now — this is the one that was right |

An agent opening `lib.rs` first — the ordinary way to learn a crate — reads the
0%-day inventory and stops. The measurement that produced §4.3b never had to be
wrong about the corpus; it only had to be right about a doc that was.

> **A crate-level "what exists today" inventory is a claim with no test, and it
> decays fastest of anything in the tree** — it is the one doc that every item
> touches and no item owns. Either it carries the date and gradient it was true
> at, or the next sizing pass inherits it. This one had neither, so it now
> carries a `STALE` banner naming `STATUS.md` as the authority.

This is `docs/conventions.md`'s *"A prerequisite in your own doc comment is
checked the way a handover's is"* — with the prerequisite one level up: the
*crate's* self-description is a prerequisite of every sizing pass run against it,
and nothing checked it for 380 commits.

### What this does to the three refusals

`checker-notes-selectable.md`, the `namedcallee` remainder and §4.3b all rest on
the quoted sentence. They are **not thereby overturned** — their *numbers* stand
and their conclusion is still right — but their stated reason is wrong, and the
difference changes what gets built. The correct reason is §2.

## 2. What is actually left: `false` has no third answer

The relation compares object types. What it cannot do is **tell you whether its
`false` is an answer**. Six sites produce a `false` that does not mean "the
relation does not hold", read out of `relater.rs` at `c4a34e3`:

| site | line | why the `false` is not an answer |
|---|---|---|
| `property_names_of` → `None` | 424 | a base with type arguments, a non-identifier base, or no members — `base_symbols_of` cannot enumerate the target's inherited requirements |
| target property absent from source | 443 | upstream *skips* a target property that is `SymbolFlags::OPTIONAL`; optionality is not read, so `{x} -> {x, y?}` fails |
| either side has no members table | 234 | `has_members` is false for a function type or an index-signature-only type, so the pair never reaches the structural arm at all |
| the depth cap | 334 | `MAX_DEPTH` giving up is recorded as "not related" |
| a generic member's type | 428 | `bd tsr-4qx` — the member type read is the uninstantiated declaration |
| a signature-bearing type | — | call/construct/index signatures contribute nothing, which is *also* the one direction that can make a wrong **`true`** |

The last row is why this cannot be fixed by widening `SELECTABLE` with a flag
test: for signature-bearing object types the relation is unsound in **both**
directions at once, so no per-type flag predicate separates the trustworthy pairs
from the rest. The decision is a property of the **pair**, not of either type.

Hence `bd tsr-kmzf`: make the relation three-valued — `Related` / `NotRelated` /
`Unknown` — and thread Kleene logic through the composite arms (a source union is
`Related` when all constituents are, `NotRelated` when any is, `Unknown`
otherwise; a target union dually). Each of the six sites above becomes `Unknown`
rather than `NotRelated`. `flow.rs`'s two callers depend only on a positive and
map `Unknown` to `false` unchanged. `calls.rs` gains the ability to ask about a
**pair**, which is what the 303 lines need.

## 3. The registered KEEP/REVERT bar for `bd tsr-kmzf` — UNMEASURED

**Nothing below has been run.** No checker code was written for this item; this
section is a registration so that the build is scored against a bar chosen before
its numbers exist, per `docs/conventions.md`.

**Population.** The 303 object-parameter lines of `examples/selectable.rs`'s 492,
plus its 44 union lines and `namedcallee.rs`'s 48. Registered as *one* population
because all three are the same gate reading the same relation; if the build ships
a gate that admits object pairs but not union-of-object pairs, the population is
a mixture and the bar is void (`docs/conventions.md`, *"A bar registered against a population is void if the population is a mixture"*) — re-register first.

**Leg 1 — the counterfactual, before any checker code.** Re-run
`examples/selectable.rs` with the decidability predicate in place, modelled on
`examples/namedcallee.rs`, and compute the **at-risk column in the same pass**:
for every one of the 303, both (a) does the ternary *decide* the pair, and (b) if
it decides it, does the decision agree with the baseline. Keep if forecast
converts ≥ 150.

**Leg 2 — controls.** C1: every classified line answers `errorType` today, expect
0 exceptions. C2: buckets sum to the classified total. Report a firing control
rather than tuning it.

**Leg 3 — measured net.** `casedelta` over a `git stash`, net ≥ +100 lines. A net
of exactly 0 means the code did not run; check that before re-reading the
premise.

**Leg 4 — new wrong ≤ 10 lines, ABSOLUTE, not a ratio.**

The shape of leg 4 is the part that needed thinking, so the reasoning is here
rather than implied:

1. **A loss here is evidence of a wrong rule, not of a bad trade.** The only way
   this build manufactures a wrong line is by classifying a pair as *decided*
   when it is not, which promotes a different overload and yields a confident
   wrong return type. There is no mechanism by which a correct decidability
   predicate loses lines. `docs/conventions.md`, *"An absolute bar catches a wrong predicate that a ratio bar ships"*: when losses are proof of a
   bug, an absolute zero is the honest bar, because a ratio "prices something that
   has no price".
2. **A ratio would pass the bad design here, and that is not hypothetical.**
   *"Computing the at-risk column in the same pass, confirmed at scale"* records `bd tsr-4sa`, where a 646-convert / 377-wrong design scored 28×
   on a ratio and was caught only by an absolute of 40. This item has the same
   shape — it fires on a **position** (every overloaded call whose parameter is an
   object) rather than on a defect — so a single leaked `Unknown` family shows up
   as hundreds of lines at once, exactly as the `unique symbol` family did.
3. **Why 10 and not 0.** Zero would be the bar if the predicate could only add.
   It cannot: widening the gate also re-decides calls that today gap for an
   *unrelated* reason, and `docs/conventions.md`, *"A residual that passes the ratio leg is still evidence"*, requires reading the
   residual regardless. 10 is small enough that any systematic leak breaks it —
   the smallest leaked family measured on this board was 26 lines (ADR-0039's ceiling, `bd tsr-0opd`) — and large
   enough that a handful of one-off disagreements does not force a revert of a
   sound build. **The residual must be read even if the leg passes.**

**Falsifier — and it is the likely outcome.** If leg 1 forecasts **fewer than
150** of the 303 as decidable, the ternary buys the gate nothing and the item is
refused with that number. This is not a formality: the 303 are object parameters
in overload sets that *differ by* object parameter type, and such sets
characteristically differ by optional members and by signature-bearing shapes —
rows 2 and 6 of §2's table, the two that `Unknown` swallows most. It is entirely
possible that the relation is three-valued, correct, and still decides almost
none of the population. **Leg 1 is cheap and answers this before a line of
relater code is written; run it first.**

## 4. What this session did and did not do

Did: established that structural comparison exists (§1), located the three doc
sites that said otherwise and corrected two of them, filed `bd tsr-7wkn` for the
third, specified the real remaining item from the code (§2), and registered its
bar (§3).

Did **not**: write the counterfactual, touch `relater.rs`'s logic, or move the
gradient. The item is unstarted, and §3 is a registration, not a result.
