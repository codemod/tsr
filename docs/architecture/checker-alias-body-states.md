# Alias-body cache answers and actual evaluation

This archive extends the [reference-state observer](checker-reference-states.md)
at Rust `3e2073fc90c1646b9c4284e9ab1c5739c3bfb99e`. It observes the separate
`alias_body_evaluations` table, first guard rejection, general body entry/return,
and publication. Native is pinned to
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`; corpus fixtures are pinned to
`4d4f005c8541e0255a9d8791205fdce326e462bc`.
The [JSON report](checker-alias-body-states.json) binds binaries, archive bytes,
raw receipt hashes and validation results. Production checking is unchanged.

The 75 added counters distinguish direct NonNullable delegates, direct ordinary
alias delegates and other admissions. Only the first alias-body admission
consumes a direct callsite tag; nested evaluation gets its own classification.
Counters reconcile independently for each actual checker owner and globally.
The existing 358 counters remain separate, giving 433 counters in total.
Alias-body answers never become answers in the reference-table classifier.

The original cache lookup executes once. A cached intrinsic error returns
`None` without entering the body. On a miss, conditional evaluation is tried
first. Its optional result is observed without changing evaluation order; that
delegate may itself use deeper caches. If it refuses, counters record the first
short-circuit reason or the actual general body entry. Publication and method
returns remain distinct: an evaluated general-body error is published and
returned as `None`, whereas a conditional delegate error is published and
returned as `Some(error)`. Counter values are calls, not distinct tuples or
allocation events.

| Actual Next.js observation | Default: four checkers | Single: one checker |
| --- | ---: | ---: |
| Direct NonNullable alias-body requests | 5,548 | 5,171 |
| Existing cached-error answers | 5,120 | 4,940 |
| Actual general body evaluations | 428 | 231 |
| Evaluated body errors | 279 | 150 |
| Successful evaluations/publications | 149 | 81 |
| Total `None` returns | 5,399 | 5,090 |
| First guard rejections | 0 | 0 |

Both enabled repetitions reproduce these non-clock counts in each mode. Cached
errors account for 94.8%/97.1% of the direct refusals. A new negative cache at
this boundary would largely duplicate existing reuse. These counts provide no
normal saved-wall ceiling, and the evaluated errors have not been attributed to
their deeper semantic origin. Correcting unsupported evaluation is separate
from adding reuse; the broader mapper/context tasks remain open.

Other admissions show genuine guard refusals: 808/599 non-alias requests and
102/56 conditional-body refusals in default/single mode. They also reuse
30,937/26,558 cached errors and 18,919/16,905 cached values. Conditional
delegates return 184/117 values. These are separately counted paths and cannot
all be treated as expensive general-body executions.

Seven focused Rust controls pass. They exercise actual bound first/repeat alias evaluation,
evaluated-error reuse, forced depth refusal followed by restored depth, arity
and non-alias refusal, same-spelled distinct aliases and ordered argument
tuples, and standard `NonNullable<T> = T & {}` direct admission. A separate
observer control exercises direct versus nested classification. The depth
control observes that Rust retains the error for the same tuple after depth
returns to normal; a different tuple still evaluates. It does not establish
natural-source recursion or native diagnostic equivalence.

An executed body-entry omission fails five controls; an executed cached-error
misclassification fails three. Original bytes are restored and all seven
controls pass again. The full enabled checker suite passes 1,507 tests with
three existing ignores, strict release checker/execute Clippy passes, and the
checker receipt readers pass 93 Python tests. Guarded replay rejects shared,
wrong-pin, dirty, existing-output and externally edited restore cases, matches
all ten hooked files, and restores a clean checkout.

The [native controls](native-alias-body-states-test.go), run alongside the
existing [native helper controls](native-reference-states-test.go), distinguish
persistent alias-table depth errors from the active mapper frame's lifetime.
All three selected native controls pass. They seed a minimal checker and declared alias links. They are private API
controls, not natural declaration binding or whole-project semantic parity.
The ordinary and probe producers' full type/display and diagnostic payloads
are compared with counters disabled; native public controls compare complete
diagnostics. Known same-spelled identity and recursive Flatten mismatches
remain failed gates rather than disappearing from the report.

With counters disabled, all 476,787 type assertion payloads and 10,570 complete
diagnostic-case payloads are byte-identical between ordinary and probe
producers, with zero previous RIGHT losses. Types retain 465,777 RIGHT, 9,563
WRONG and 1,447 GAP; diagnostics retain 3,569 RIGHT, 4,917 EMPTY_RIGHT, 1,919
WRONG and 165 EMPTY_WRONG. Changes from older snapshots belong to upstream
fidelity ports. The app preserves 1,397 direct checks, 14,051 loaded files and
three complete diagnostics in all 12 children. Public controls preserve Rust
outputs in 140 children; native diagnostics match 24 of 28 cases, retaining
both known failing families in both modes. Pool controls retain 30 children
and two deliberately rejected abort receipts.

All ten private hook files and three prior private target binaries are restored
with exact byte guards; isolated Rust, native, fixture and replay checkouts are
clean.

All builds and owned heavy checks run serially. App observations cover two
repetitions in default/single mode; they qualify the observer and scope, not a
production speedup. Host contention is uncontrolled. Observed input snapshots
do not establish coverage of all failed/transient resolution queries or all
lazy semantic forcing. The comparable whole-project TSR/native median target
of **0.50** remains unmet/unverified.

Replay requires a separate clean checkout at the exact Rust source pin, pinned
native libraries for building, and fresh receipt directories outside that
checkout. The inherited diagnostic allocation probe is macOS-only.

```sh
python3 docs/architecture/replay_checker_alias_body_states.py \
  --source /tmp/isolated-tsr-3e2073fc --output /tmp/alias-state-replay

# Use an external target directory and preserve an ordinary binary first.
TSR_MAPPER_PROFILE=1 cargo test --release --offline -p tsr-checker \
  --lib alias_body_state_probe::tests -- --test-threads=1

python3 docs/architecture/checker-alias-body-state-controls.py \
  --bindings /tmp/alias-state-bindings.json \
  --output /tmp/alias-state-controls --repeats 2

python3 docs/architecture/replay_checker_alias_body_states.py \
  --source /tmp/isolated-tsr-3e2073fc --output /tmp/alias-state-replay --restore
```

Bindings supply `normal`, `probe` and `native` paths, SHA-256 hashes and source
SHAs. The driver retains the existing exact native binary qualification. Add
`--project /path/to/tsconfig.json` for an application or `--pool-controls` for
admission/no-check/abort controls. Runtime hooks remain in isolated snapshots;
the archive patch includes all original reference/pool hooks and the new
alias-body observer. Replay checks every hooked byte before any restoration.
