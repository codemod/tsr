# Lane notes: r6-names2 (tsr-2zk.1142)

Round-6 box on epic `tsr-2zk`, continuing r6-names (`tsr-2zk.1133`,
[`r6-names.md`](r6-names.md)). Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Owned files: `crates/tsr-checker/src/name_slots.rs`,
`crates/tsr-checker/src/export_specifier_names.rs`,
`crates/tsr-binder/src/scope_change.rs`. The binder walk (`lib.rs`),
`symbols.rs`, `check.rs` and `checker.rs` are main's, so every hook into them
ships as a measured diff in this directory.

## §0 Base and instruments

- **Base.** Batch BJ (r6-names) had not landed on
  `claude/beautiful-shannon-ar5gh0` when this box started, so the frozen base
  is that branch's tip `23e1689` with `claude/beautiful-shannon-ar5gh0-r6-names`
  (`a725c95`) merged into it (a clean merge, commit on this branch) and
  r6-names' diffs 1–6 applied in `r6-names.md` §3's order, uncommitted. That
  is the tree batch BJ lands with diff 7 held. At the base: diagnostics 5,596
  RIGHT / 5,603 EMPTY_RIGHT / 997 WRONG / 42 EMPTY_WRONG; types 549,976 RIGHT /
  827 GAP / 5,500 WRONG.
- **Oracle.** Native tsgo from `scripts/offline-cargo/build-tsgo.sh`.
- **Setup.** PyPI is blocked; `assemble.py`'s three `tomlkit` calls ran on a
  stdlib-only stand-in kept in the session scratchpad, outside the repository
  (r5-operators3 §4).
- **Ir.** `valgrind --tool=callgrind`, `tsr -p tsconfig.json
  --singleThreaded --pretty false` on `benches/projects/{domain-model,
  generic-imports}`, release build alone. Base: domain-model
  1,091,736,111 / 1,091,780,839, generic-imports 343,052,879 / 343,047,703
  (two runs each). The bar is ±0.08% (about ±0.87 M on domain-model).

## §1 `useOuterVariableScopeInParameter` without an options slot on `BindResult`

r6-names' diff 7 (`r6-names-parameter-scope.diff`, `r6-names.md` §11) ported
`resolveNameHelper`'s `useOuterVariableScopeInParameter` arm into the binder
walk for every caller, with the two options `requiresScopeChange` reads
published into a `OnceLock` on `BindResult`. It converts 5 diagnostics cases
and 38 type lines with 0 lost, and was held on Ir: the `OnceLock` field moved
`lookup_scoped`/`merged_symbol` codegen at identical call counts.

**Forcing constraint.** The arm's answer depends on compiler options the
binder does not hold, and the walk has 111 direct `BindResult::resolve_name`
callers in the checker besides the wrappers. The options must reach the walk
without a `BindResult` field (the integrator's bar) and without editing
those callers (a dozen other lanes' files).

**What makes it possible.** Native reads the options in exactly three places,
the *triggers* of `requiresScopeChangeWorker` (`nameresolver.go:376-399`):
a static property (`!GetEmitStandardClassFields()`), `??` or an optional
chain (`target < ES2020`), and an object rest binding (`target < ES2017`).
A parameter list with no trigger answers `false` under every option set, and
the arm reads the options only after the body-containment test passes, which
on domain-model happens on no lookup at all (the cold function is entered
from the inline `IsParameterDeclaration` test and returns early). So:

- The walk takes `Option<ScopeChangeOptions>` as a **parameter**.
  `requiresScopeChange` is evaluated three-valued: a trigger read without
  options is *unknown*, and `core.Some`/`ForEachChild` become "true as soon as
  one item is, else unknown if one was, else false". Unknown declines the arm
  (the body's variable stays the answer, this binder's earlier behaviour);
  with options passed, there is no unknown and the answer is native's.
- `resolve_name`, `resolve_name_excluding` and the binder's
  `resolve_name_with_export_alias` pass `None`. The new
  `BindResult::resolve_name_with_export_alias_in_scope` takes the options, and
  the checker's `Checker::resolve_name_with_export_alias` (`symbols.rs`, the
  road identifiers resolve through) passes them, built at the call from its
  own `language_version` and `standard_class_fields` fields: the
  "Checker-held value read at the call site". No Checker field is added.

So every road gives native's answer for a parameter list without a trigger,
and the checker's wrapper road gives it for every parameter list.

**Variants measured** (domain-model / generic-imports Ir, two runs each,
against the base above; dumps against the frozen base):

| Variant | domain-model | vs base | generic-imports | Dumps |
|---|---|---|---|---|
| A. r6-names diff 7 as shipped (`OnceLock` on `BindResult`) | 1,093,471,739 / 1,093,347,050 | +0.15% | 343,092,213 / 343,080,098 | +5 cases, +38 types, 0 lost |
| B. options passed only by the checker wrapper; other roads skip the arm | 1,092,238,299 / 1,092,161,955 | +0.04% | 343,082,268 / 343,084,350 | +4 cases, +38 types, 0 lost |
| **C. B plus the three-valued arm on every road (shipped)** | **1,092,589,101 / 1,092,487,282** | **+0.065–0.078%** | **343,063,677 / 343,066,432** | **+5 cases, +38 types, 0 lost** |

B loses `optionalParamReferencingOtherParams2` against A: TS2304 on `b` is
reported through the wrapper road, but flow's TS2454 resolves `b` through
`BindResult::resolve_name`, which in B still finds the body's `var b`, so the
case reports both. Two roads answering one name differently is not native's
model, so B was refused on fidelity as well as the case. C answers that road
too (no trigger in `(x = a, y = b)`) and reproduces A's dumps exactly (same
verdict for every case and every aligned type line).

C's residual, per function (callgrind, run 1 against base run 1):
`BindResult::resolve_name` +0.30 M (the inline `IsParameterDeclaration` test
on each `locals` hit, which every road now has), the wrapper road's walk
instance +0.18 M, `Checker::resolve_name_with_export_alias` +0.16 M (building
and passing the options), `use_outer_variable_scope_in_parameter` +0.09 M.
Nothing else moves by more than 0.07 M. That is work the arm does, not
layout: A's `lookup_scoped`/`merged_symbol` shift is gone.

Measured with C: diagnostics 5,596 → 5,601 RIGHT (`functionLikeInParameterInitializer`,
`optionalParamReferencingOtherParams2`, `parameterInitializersForwardReferencing`,
`parameterInitializersForwardReferencing1`, `parameterInitializersForwardReferencing1_es6`),
types 549,976 → 550,014 RIGHT (+38), 0 lost on either dump; `slowcases` clean
on both.

Native probe (`--strict`), both targets, matched by the port with the diff
and pinned in `r6_names2_parameter_scope.rs`:

```text
function strange(x = a, y = b) { var b = ""; return y; }   TS2304 b (both)
export function bar(func = () => foo) { let foo = "in"; }  TS2304 foo (both)
export function f1(p = outer) { var outer: number = 2; }   nothing (both)
export function nullish(a = c ?? 1) { var c ...; }         es2015 TS2373 c; es2022 TS2304 c
export function plain(a = d) { var d = 1; return a; }      TS2304 d (both)
```

**Rejected alternatives.**
- *A process-global registry keyed by `SymbolStoreIdentity`*, read on the
  cold path only. Exact on every road with no hot-path cost, but a hidden
  global owner across independent programs is against the worker-ownership
  contract (`docs/architecture/threading.md`), and stale or interleaved
  registrations would answer silently wrong.
- *A thread-local set by the checker.* A checker built on one thread and
  another program's checker built later on the same thread would read the
  wrong options.
- *Wrapping `Checker::binder` in a type that shadows the resolve entry
  points.* Every `self.binder.symbols()`-style read would then borrow `self`
  through `Deref`, breaking hundreds of sites that rely on `&'a BindResult`.
- *An options parameter on `resolve_name` itself*: 111 checker call sites in
  a dozen lanes' files, for an answer that only differs on trigger lists.
- *An atomic in `BindResult`'s padding* (`max_depth: u32` leaves four bytes):
  probably layout-neutral, but it is still options on `BindResult`, which
  the dispatch ruled out. Not built.

**Accepted limitation, with its falsifier.** A road other than the checker's
wrapper declines the arm for a parameter list *with* a trigger: under
`target >= ES2020` (or standard class fields) `function f(a = c ?? 1) { var c }`
resolves `c` to the body's variable there, where native continues outward.
The probe's `nullish` row at es2022 still reports native's diagnostics
through the wrapper road, and nothing in the corpus moves between A and C. The falsifier is a case where A and C differ: C reproduces A on both
dumps, so there is none in the corpus. If one appears, the next step is to
pass the options on that road (its call site), not a `BindResult` field.

**Convention record.** Pinned operation `useOuterVariableScopeInParameter` /
`requiresScopeChange` (`binder/nameresolver.go:346-399`). Native caches the
per-function answer (`GetRequiresScopeChangeCache`); this port computes it on
demand after a body variable was found through a parameter, which no lookup
on domain-model reaches. No cache, side table or publication state: the
options are a per-call value from the checker's options-derived fields, set
once in `apply_compiler_options` before any lookup.

**Diff:** [`r6-names2-parameter-scope.diff`](r6-names2-parameter-scope.diff)
(binder `lib.rs`, checker `symbols.rs`, the test, and the `dead_code`
removal in `scope_change.rs`). It replaces r6-names' diff 7, which is
deleted from this directory (its measurements stay in `r6-names.md` §11 and
in variant A above). It applies on the base above (after r6-names' diffs
1–6) and alone on the tip; the owned half (`scope_change.rs`'s three-valued
worker) is committed on this branch.

## §2 Batch BJ's unattributed residuals, against the pre-BJ tip

`r6-names.md` §12 attributed the r6-names stack's cost against its own base
(`b18aec06`) with diff 7 applied, and left two items open:
`check_node_worker` +0.66 M "not attributed to any new call" and
`value_reference_slot` +0.25 M. Re-measured here against the right pair:
the pre-BJ tip `23e1689` (release `tsr`, built alone) and batch BJ as it
lands (§0's base: r6-names merged, diffs 1–6, no diff 7).

| Build | domain-model Ir (two runs) | generic-imports |
|---|---|---|
| pre-BJ `23e1689` | 1,091,369,859 / 1,091,265,219 | 343,073,434 / 343,051,263 |
| BJ (diffs 1–6) | 1,091,736,111 / 1,091,780,839 | 343,052,879 / 343,047,703 |
| BJ + `r6-names2-value-slot-inline.diff` | 1,090,893,161 / 1,091,511,794 | 343,051,569 / 343,076,814 |

BJ costs domain-model +0.37–0.52 M (+0.03–0.05%), inside the ±0.08% bar;
r6-names' +0.19–0.25% was mostly diff 7 (§1). Per function, BJ against
pre-BJ, identical in both run pairs (so not noise):

- `check_node_worker` **+0.055 M**, not +0.66 M. The +0.66 M was measured
  in the diff-7 build, whose `BindResult` layout change also moved code in
  this function; against the real pre-BJ tip what remains is the
  export-specifier tail's call site (diff 6). Real, but 0.005%: no action.
- `value_reference_slot` **+0.255 M**, real: one out-of-line call per
  identifier the allow-list's other arms do not take.
- `check_value_identifier` +0.11 M, the region walk on the reporting paths
  (§12's figure, unchanged).

Everything else that moves is the allocator (`_int_malloc`, `_int_free`,
`unlink_chunk`, ±0.05–0.12 M between runs of the same binary), which is
also why the last row's two totals are 0.6 M apart.

**The cheaper placement.** r6-names proposed inlining
`value_reference_slot` into `check.rs`'s allow-list (main's file). The same
placement is reachable from the owned file: `#[inline(always)]` on the
function folds its four-variant match into `is_value_reference`'s fallback
arm. Plain `#[inline]` was measured first and not taken (profile identical
to BJ). With `#[inline(always)]` the function disappears from the profile and
no caller grows (`check_node_worker`, the only one that inlines
`is_value_reference`, reads the same +0.055 M), so the 0.255 M is gone. The
attribute carries an `allow(clippy::inline_always)` with that reason; it is
the repository's first.

It ships as a diff although `name_slots.rs` is owned, because the lines it
touches are next to the `allow(dead_code)` line that r6-names' diff 1
removes; committing it now would make batch BJ's merge conflict. Apply it
after diff 1. Codegen only: no dump can change, and none was re-run for it.

**Correction.** The commit that landed this section (`04b961c`) also
committed, by accident, the base's working-tree copy of r6-names' diffs 1–6
in `check.rs` and `export_specifier_names.rs` and their six tests (the base
is applied uncommitted, §0, and the commit staged everything). That made
the branch carry main-file hooks unreviewed, and `name_slots.rs`'s half was
missing, so diffs 1–6 no longer applied on it. The next commit restores those
files to `7b0a4d8` exactly; nothing in §1–§2 depended on the slip.
