# r6-relater — variance reliability, held lifts, TS2321 (`tsr-2zk.1124`, `.1065`)

Lane under epic `tsr-2zk`, round 6, successor to r5-relater8
([branch `claude/beautiful-shannon-ar5gh0-r5-relater8`], its notes
`r5-relater8.md` and diffs `r5-relater8-wip-variance.diff`,
`r5-relater8-wip-conditional.diff` live on that branch only). Owns
`relater.rs`, `relation_cache.rs`, `variances.rs`, `identity.rs`,
`index_access_reports.rs`, `assignreport.rs`, the tests
`crates/tsr-checker/tests/relater8_arms.rs` and this file. Native anchors are
`vendor/typescript-go` @ `5b1047d`; every test expectation was checked
against a native tsgo built from that submodule
(`scripts/offline-cargo/build-tsgo.sh`).

## 0. Frozen base

`b18aec06` (`claude/beautiful-shannon-ar5gh0`: main `17265fac` plus round-6
bookkeeping). r5-relater7's tip `68bd970`, which r5-relater8's WIP was built
on, is an ancestor; the variance diff applied to it with offsets only.

- `diagverdictdump`: RIGHT 5530, EMPTY_RIGHT 5596, WRONG 1063, EMPTY_WRONG 49.
- `verdictdump`: RIGHT 549853, WRONG 5607, GAP 843.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,079,039; domain-model 1,091,485,686.

Every `Ir` below uses that command, and each run's complete CLI output is
compared with the base's (`cmp`). Loss checks are `box-protocol.md` §5's, on
`cut -f1,2`, unfiltered; slowcases runs on both dumps.

## 1. `object` meets an object target through its apparent `{}` (`.1124` a)

r5-relater8 §1, split out of its variance diff (the
`non_primitive_source_related_to` hunks alone).

**Forcing constraint.** structuredTypeRelatedToWorker replaces a source by
its apparent type before the structural arm (relater.go:3762); the apparent
type of `object` is the empty object type (getApparentType). The structural
arm (relater.go:3864) then relates `{}` to the target's properties,
signatures and index infos. The port took only the definite negative (a
required target property `{}` cannot supply) and answered every other
`object -> T` pair `Unknown`, which hid isSourceIntersectionNeedingExtraCheck
(relater.go:3243): for `x: { a?: string }`, `y: T & { a: boolean }`, `T
extends object`, the optional-property pass finds `boolean -> string`
only after `object -> { a?: string }` relates (`intersectionPropertyCheck`
10:3).

**Ported** (`Relater::non_primitive_source_related_to`): the three conjuncts
over `{}`, stopping on the first False. `sourceIsPrimitive` is false for
`object`, so `{ [x: string]: any }` keeps its shortcut, and `{ [x: string]:
number }` fails (`{}` is not an object literal, so its index is not
inferable). A generic mapped target keeps its own arm; a qualified alias
mint's flags are not evidence of an object.

**Alternative.** Leaving every non-negative pair `Unknown` (the old rule):
correct but silent wherever the answer is consumed as a decision, as above.

**Stated gap.** Targets without a member table stay `Unknown`: a tuple, and a
resolved non-generic mapped type (`Record<string, unknown>`). Both are False
natively.

**Measured** against §0, both loss checks empty, slowcases clean:
- diagnostics: `intersectionPropertyCheck` WRONG → RIGHT;
- types unchanged;
- `Ir`: generic-imports 343,079,039 → 343,082,196 (+0.001%); domain-model
  1,091,485,686 → 1,090,903,686 (−0.05%). CLI output identical.

**Falsifier.** An `object -> T` pair where native's `{}` walk differs from
`properties_related_to(emptyObject, T)`; likeliest a target property the
port reads differently through `Object`'s members.

Test: `tests/relater8_arms.rs`
`the_non_primitive_object_meets_a_target_through_its_apparent_empty_type`.
