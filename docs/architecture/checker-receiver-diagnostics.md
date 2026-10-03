# Concrete receiver types in assignment diagnostics

The recursive import control for `tsr-6.48` assigned both `{ value: 1 }`
and `{ value: 'bad' }` to `A<number>`. TSR incorrectly reported both properties
against `T`. Pinned tsgo accepts the numeric assignment and reports only the
string assignment against `number`.

This is a correctness prerequisite for worker ownership under `tsr-1yb.3.2`.
It does not establish the full-project TSR/tsgo wall target of at most 0.50.

## Cause and native contract

The mapper was already correct: the declaration's property type and formal
parameter shared the same checker-local TypeId, and the receiver carried
`T -> number`. `elaborate_object_literal` instead fetched the declaration's
property symbol and read its raw type, bypassing receiver substitution.
Its early diagnostic also prevented the whole-object relation from correcting
that answer.

At native revision `5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
`internal/checker/relater.go:546` (`elaborateElement`) obtains both source and
target property types by indexed access on their respective concrete types.
`getBestMatchIndexedAccessTypeOrUndefined` at line 620 resolves the target
receiver. TSR now reads both members through `get_type_of_property_of_type`.
The source member includes its mutable-location widening; reading the fresh
initializer directly had also produced literal names where native reports
`string` or `number`.

Native `reportRelationError` at line 4751 generalizes literal source names for
targets that cannot have top-level singleton types, except `never`.
`typeCouldHaveTopLevelSingletonTypes` at line 1305 preserves literal, template,
union/intersection and constraint boundaries. The reporting helper follows
those flag/composite rules using the existing checker-local constraint resolver
and a local recursion path. It changes the displayed source after a negative
relation; it does not replace the semantic relation arguments or share types
between checkers.

## Verification

The reference is source `e8f6ff097d3acdff11ed3e825c668a8ac7f369f7`.
The pinned native binary SHA256 is
`118d4efb63e59af858d6d0f5a718a8d2809d9f5551303a62caa5be3d5364c7ce`.

| Control | Result |
| --- | --- |
| Recursive imported interfaces | Correct assignment accepted; one native TS2322; query remains `A<number>` |
| Defaulted/inherited and mapped members | Complete plain native diagnostics match |
| Mutable and const-asserted members | Concrete target and native source names match |
| `never`, boolean, literal union and template targets | Native generalization/preservation boundaries match |
| CLI versus private checkers at 1/2/3/4 workers | Complete output and query controls match; eligible file coverage preserved |
| Full 474,251 type-assertion rows | Byte-identical: 459,451 RIGHT, 2,195 GAP, 12,605 WRONG |
| Standard diagnostic pass set | 2,807 to 2,834; 27 gains, zero losses |
| Expanded diagnostics, including clean cases | 7,593 to 7,630 matches across 10,570 eligible cases; 37 gains, zero losses |
| Previously correct diagnostic positions/codes | Zero multiset losses in the expanded comparison |

The expanded diagnostic probe skips the same 1,874 configuration-varied,
known-divergence or unrecorded cases as the existing suite. It includes all
5,082 eligible cases expecting no diagnostics: clean matches rise from 4,786
to 4,796. Actual diagnostics fall from 24,563 to 24,507; 60 cases change.
Its comparison covers positions/codes, while the external native controls
compare complete plain message text and spans. Neither comparison is a claim
that every remaining corpus diagnostic message matches native.

The real Next.js workload drops from 123 diagnostics to 121, with no additions.
The removed errors concern `AsyncIterator<T>.next` in
`detached-execution-stream.ts` and `insights-dashboard.ts`. Loaded identities,
input contents and effective configuration are unchanged: 13,097 loaded,
1,341 checked and 13,560 parsed files. The checking loop is unchanged; this
report does not complete permanent cross-tool checked-identity telemetry
under `tsr-1yb.1.2`.

A fresh native replay is clean and includes both files whose errors were
removed. Native still loads 13,098 files: its one additional identity is
`node_modules/@lingui/conf/dist/index.d.ts`. Loaded-scope alignment remains
in `tsr-1yb.1.1`; native zero diagnostics versus TSR 121 also prevents an
equivalent-output throughput claim. Evidence is
`/tmp/tsr-648-app-native-comparison.json`.

Release checker/execute tests, focused conformance tests, Clippy, formatting
and whitespace checks pass. Re-run the principal controls with:

```sh
rtk proxy cargo test --release -p tsr-execute --test checker_ownership
rtk proxy cargo test --release -p tsr-checker -p tsr-execute
rtk proxy cargo run --release -p tsr-conformance --example verdictdump
rtk proxy cargo run --release -p tsr-conformance --example diagpass
```

## Remaining work and local evidence

`tsr-6.49` still blocks imported augmentation fidelity and worker readiness.
The broader receiver-consumer audit is `tsr-1yb.4.1.5`. Constraint/intersection
reporting controls and the broader display audit remain in `tsr-6.48.1`.
This change does not complete native expression-specific elaboration,
related-information or diagnostic-chain support.

Two extra native controls exposed existing gaps on both the reference and
candidate: the literal-union relation chain is absent (`tsr-6.55`), and a
declared-function call with a const-asserted bad argument lacks TS2345
(`tsr-6.56`). They are tracked separately rather than treated as native passes.

Local artifacts are `/tmp/tsr-648-native-controls.json`,
`/tmp/tsr-648-members-source.json`, `/tmp/tsr-648-members-after.tsv`,
`/tmp/tsr-648-diagpass-comparison.json`,
`/tmp/tsr-648-diagnostic-corpus-comparison.json`,
`/tmp/tsr-648-diagnostic-corpus-probe.rs`, `/tmp/tsr-648-app-comparison.json`
and `/tmp/tsr-648-gates.json`. Temporary corpus instrumentation was archived
and removed; the production source was restored and verified before further
measurements. These local paths are evidence pointers, not portable CI inputs.

## Timing check

Two independent rounds each use five alternating fresh-process pairs, with a
warm-up for each binary, on the unchanged Next.js inputs. Both disable
incremental/composite reuse and emit, and use the same effective options.
Builds and corpus probes finish before timing starts.

| Round | Reference median wall | Candidate median wall | Reference/candidate median user CPU |
| --- | --- | --- | --- |
| 1 | 4.526538 s | 4.563399 s | 3.450159 / 3.454582 s |
| 2 | 4.625858 s | 4.597490 s | 3.511401 / 3.497172 s |

The wall changes have opposite signs: +0.81% then -0.61%. They do not confirm a
throughput improvement or a consistent regression. Process RSS ranges overlap;
no memory improvement is established. This package is retained for corrected
checking and reporting, not as an accepted speed experiment. Its intentional
diagnostic change also means it is not an identical-output timing comparison.

The timed reference binary SHA256 is
`9dcf14e36f759bb324c897db7cfd655ede85b61986003a4459a4b6ef0faad5ff`;
the candidate is
`1d00f1fa46470c39f214cd771abf3eb75b3322a2f64cfe618fc597af357f97e0`.
All samples, CPU/system time, peak RSS and complete diagnostic fingerprints are
in `/tmp/tsr-648-members-paired.json`. Input hashes were rechecked after timing.
This is not evidence of the required comparable TSR/tsgo ratio of at most 0.50.
