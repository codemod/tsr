# ADR-0011: `unsafe` is denied by default

- **Status:** accepted
- **Date:** 2026-08-03
- **Related:** [ADR-0001](0001-idiomatic-rewrite.md) (idiomatic rewrite, not
  transliteration), [ADR-0012](0012-ast-is-sync.md) (which this constrained)

## The forcing constraint

The reason to spend years porting a 300k-line compiler to Rust rather than to Zig
or C is memory safety. A port that reaches for `unsafe` whenever the borrow
checker is inconvenient has paid Rust's costs — the learning curve, the
compile times, the lifetime plumbing — and kept C's failure modes. At that point
the choice of language is no longer justified by anything.

This is not hypothetical for a compiler. The shape of the problem invites it: an
arena with self-referential data, a tree of interior-mutable side tables, and hot
paths where a bounds check is measurable. Each is a plausible-sounding reason to
write `unsafe`, and each is one that a slightly different design removes.

## The decision

**`unsafe_code = "deny"` at the workspace level.** Every use is an explicit,
justified exception.

The current set is three items, and it is meant to stay small enough to list here:

| Where | Why it is unavoidable |
|---|---|
| `crates/tsr-core/src/arena.rs` | A bump allocator hands out typed references into raw memory it manages itself. No safe abstraction expresses that; `bumpalo` and `oxc_allocator` both contain the same code. |
| `crates/tsr-parser/src/parsed_file.rs` — one `unsafe impl Send` | `self_cell` produces a type holding a pointer into its own storage, which suppresses the automatic `Send` derive even though every constituent is `Send`. |
| `benches/parse.rs`, `examples/alloc_profile.rs` | `GlobalAlloc` is an unsafe trait, and there is no safe way to observe allocation. Both are tooling; neither ships in a binary. |

### One exception removed, and one correction to this list

**Removed 2026-08-05** by the identity widening
([ADR-0034](0034-a-program-needs-one-identity-space.md)):
`crates/tsr-compiler/src/file.rs` held a second `unsafe impl Send`, on
`ProgramFile`, for the same `self_cell` reason as the row above it. Under
program-wide identity a file owns no arena, no node table and no bind result —
they all belong to the `Program` and outlive every file in it — so `ProgramFile`
is a plain borrowing struct, `self_cell` leaves it entirely, and the `Send`
impl is derived rather than asserted. `grep -rn "allow(unsafe_code)"` now
returns exactly the four sites the three rows above describe.

**And the list was one short.** That `ProgramFile` impl cited this ADR in its
`SAFETY` comment and was never added to the table, so from `2a0dc19` until now
the list read "three exceptions" while the tree held four. Recorded rather than
silently corrected, because the argument in "Consequences accepted" below —
*"if it grows past a handful, that is the signal that this ADR is being worked
around"* — depends on the count being real, and a list nobody re-derives from
the tree cannot carry that weight. The check is one `grep`; it is now run when
this list is touched.

`deny` rather than `forbid`: `forbid` cannot be lifted even by a documented
`#[allow]`, which would force the arena into a crate of its own to no benefit.
The cost of `deny` is that an exception is a one-line annotation — so the rule is
that an annotation without a `SAFETY` comment stating the invariant is a review
failure.

## What this ruled out, concretely

The policy is only worth having if it changes decisions. It has changed two:

**A `transmute` in generated code.** `SyntaxKind::from_u16` was
`transmute::<u16, Self>(value)` behind a bounds check, with a comment asserting
that discriminants are contiguous `0..COUNT`. That was true — and nothing enforced
it. A gap introduced upstream in `ast.json` would have turned a generated function
into undefined behaviour with no diagnostic anywhere. It is now an index into the
existing `ALL` array, with a `const` block proving `ALL` is in discriminant order,
so the invariant is a compile error rather than an assumption. Same code size,
same speed, and the failure mode became a build break.

**Writes into a node table's spare capacity.** `NodeTable::push` writes four
parallel vectors and is ~4.5% of a large parse; bypassing the length checks with
`set_len` and raw writes would recover part of it. Rejected. Four percent of the
parser, which is itself a small share of a compiler run, does not buy a hand-rolled
`Vec`.

## Alternatives

**Allow `unsafe` freely in "hot" code, with review.** Rejected: "hot" is not a
property anyone can define in advance, and the profile moves. This session alone
saw the hottest function change three times. A rule that depends on a moving
measurement is not a rule.

**`forbid` everywhere, with the arena in its own unaudited crate.** The same
`unsafe`, one crate further away, plus a dependency boundary that exists only to
satisfy a lint. Rejected as ceremony.

**Depend on `bumpalo` instead of writing an arena.** Genuinely tempting — it moves
the `unsafe` behind a widely-audited dependency. Rejected for now because the arena
is 230 lines and we need control over chunk growth and the `needs_drop` assertion
that keeps destructors out of the tree. Worth revisiting if the arena grows.

## Consequences accepted

- Some code is slower than it could be. The `NodeTable::push` example is the known
  case, and it is quantified rather than hand-waved.
- The exception list needs maintaining. If it grows past a handful, that is the
  signal that this ADR is being worked around rather than followed.
- `deny` can be silenced locally, so the policy relies on review noticing an
  unjustified `#[allow(unsafe_code)]`. The lint makes it visible; it does not make
  it impossible.

## How we would know this was wrong

- **A measured, user-visible regression that only `unsafe` can fix.** Not a
  microbenchmark — something a person waits for. Then the specific case gets an
  exception with the measurement attached, and the policy survives.
- **The exception list grows steadily.** Three items is a policy; fifteen is a
  fiction, and the honest response would be to say so rather than keep counting.
- **A dependency does the job better.** If `bumpalo` or a successor covers the
  arena's requirements, taking it removes the largest block of `unsafe` we own.
