# Round-4 merge of `main` (duplicated work)

Integration merged `origin/main` into `claude/beautiful-shannon-ar5gh0` after
batch F (2026-10-08). Main's local boxes and the round-4 cloud boxes had ported
four of the same things independently. Two copies of one native cache or check
cannot both stay: either they report twice or the code carries an unread table.
For each pair we kept one copy and deleted the other.

| Native operation | Round-4 copy | Main's copy | Kept |
|---|---|---|---|
| `resolveObjectTypeMembers`' call/construct signatures of an interface symbol (C1) | `PerfLinks::interface_signatures` (r4-perf, `r4-perf.md` §2): kind-slotted, admitted through `memo_frames`, publishes only decided lists | `Checker::interface_signatures` keyed `(symbol, kind)` (575cf602) | **main** |
| The `getTypeWithThisArgument` substitution's "does the member mention any minted `this`" walk (C5) | `mentions_type_parameter_where` + `is_minted_this_type_shape` | `mentions_this_type` / `is_minted_this_type` (aee0d31b) | **main** |
| `resolveObjectTypeMembers`' `resolvedProperties` names (C2) | `PerfLinks::structured_property_names` (r4-perf2, `r4-perf2.md` §3) with `StructuredNamesWalk` | `Checker::structured_property_names` (d67ff023) | **round 4** |
| `checkPropertyAccessibilityAtLocation`'s `isSuper` arm, TS2513 / TS2855 | `super_property_accessibility_error` in `super_expression.rs` (r4-classsyntax) | inline in `readonly_target.rs` (8f8d070c) | **main** |

Why the split:
- **Default to main's copy.** Main is the trunk, and its later commits build on
  its own versions. Keeping ours would make every later main merge conflict
  again.
- **C2 is the exception.** Git auto-merged the non-conflicting parts of C2 to
  round 4's shape: `collect_structured_property_names` takes a
  `StructuredNamesWalk`, and main's three-argument call does not compile
  against it. Keeping main's copy would have meant rewriting the walk, which
  is more change than a merge resolution should carry.
- **Ours had both TS2513/TS2855 arms.** If both ran, an inaccessible `super`
  member would be reported by whichever ran first, and the other would be
  dead. Only main's copy remains.

What we accepted: main's C1 memo has no receiver/alias-frame admission and does
not check that the list is decided. `r4-perf.md` §2 records why the round-4
copy had both. The conformance gate on this merge is the evidence that the
difference is not observable on the corpus.

How we would know this was wrong: the gate after this merge shows a loss in
cases whose interface signatures are read under an alias-evaluation frame, or
whose returns are provisional (`errorType`) during a circular resolution. In
that case, restore the round-4 publication rule in main's
`signature_candidates_of_interface_symbol` (admission through `memo_frames`,
publication through `signature_links_publishable` and the decided-returns
test).

## r4-perf3 merged after main (batch G)

r4-perf3 was built on the pre-main integration head, so it still edited the
round-4 interface-signature memo that the main merge had removed (C1 above). It
also had its own placeholder marker for `late_bound_members_of`, a set of
`(owner, static)` keys, `PerfLinks::late_bound_active`. Main had added a
counter for the same purpose.

How it was resolved:
- **C1:** main's memo stays. r4-perf3's `publication_mark` / `publishable_since`
  change to it is dropped with that memo. The other memos keep it.
- **Late-bound marker:** r4-perf3's keyed set is kept. Main's counter
  (`Checker::late_bound_active`) only fed main's C2 memo, which was not kept,
  so it is removed. The set is the stricter rule: it withholds publication only
  for an owner whose placeholder is live, not for every owner while any
  placeholder is live (`r4-perf3.md`).
