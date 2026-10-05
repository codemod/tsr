# Reference-table answers, reservations and delegate returns

This archive extends the [actual checker-pool observer](checker-mapper-pool.md) at frozen Rust `96520fe712cdbd3ef76f8a73b7778a285aaa33bd`. It qualifies observation of the existing reference table, without choosing a production cache or changing its keys. Native comparisons use pinned `5b1047d10d32e7d5b446be4de56b126ff42f82bb` and the previously qualified worker-activity binary. The [machine-readable report](checker-reference-states.json) binds source, binaries, hooks, readers, fixtures, raw receipts and every original call site.

All 19 reference-table get/insert sites are observed: empty references, deferred identity mapping, the ordinary factory, early string/identity mapping, keywords, conditional/body/template/normalized mapping, named reservation and mapped-sequence replacement, `NonNullable`, and borrowed index/member consumers. The macros evaluate original key/value operands once and obtain every semantic answer from the original map. The existing pool hooks now account for its four-field result, including JavaScript syntax diagnostics. Runtime hooks stay in an isolated checkout; the production tree gains only this archive.

The 258 added counters separate four request routes, 19 table sites and eight optional delegate branches. There are 358 counters including the existing pool observer. Actual default/single owners reconcile with the post-join aggregate, and repeated non-clock counts match in each mode. Errors mean the returned value equals that checker's intrinsic error ID. An error argument inside a named reference is not itself an error answer.

A named insertion marks an explicit reservation before subsequent display/target metadata and mapped capture/sequence work. A reserved answer requires an overlapping request with the same checker, symbol, ordered argument IDs and reserved result ID. The guard closes at the method return; that closure does **not** establish member completion. A framed answer can match the currently executing request. A same-key request count also includes delegation between the ordinary and `NonNullable` methods, so neither count alone establishes recursive worker work. The [native API tests](native-reference-states.md) separately demonstrate handles cached before member resolution and different target, alias and active-mapper error lifetimes.

An optional delegate counter records entry and its `None`, intrinsic-error or value return. Delegates can use deeper caches; these counters do not count expensive inner evaluations. In particular, `evaluate_alias_body` at frozen `declared.rs:7575` checks `alias_body_evaluations` before body work, and returns `None` for a cached intrinsic error. A refusal at the outer `NonNullable` method can therefore reuse an existing inner refusal.

| Actual app observation | Default: four checkers | Single: one checker |
| --- | ---: | ---: |
| Files actually checked | 1,397 | 1,397 |
| Ordinary reference lookups | 609,073 | 523,024 |
| Named reservations opened/closed | 49,508 | 35,513 |
| Answered named reservations | 0 | 0 |
| Reference-table error answers | 0 | 0 |
| `NonNullable` outer lookups | 14,140 | 12,497 |
| `NonNullable` outer misses/body delegate calls | 5,447 | 5,072 |
| Body delegate refusals/error method returns | 5,305 | 4,996 |
| Successful `NonNullable` publications | 142 | 76 |

These are calls within checker owners, not distinct tuples, allocation savings or proof of duplicated expensive work. String-mapping delegates illustrate the distinction: 609,220 default calls yield only eight admitted values; 609,212 return `None`. The separate string-mapping cache remains in place. Follow-up **`tsr-1yb.16.1.2.2.1`** owns inner alias-body cache answers, actual evaluations and refusal reasons, including depth/context behavior, before any new negative cache or speed claim.

Seven focused tests pass. Five exercise actual bound Rust factory/table paths: first/repeat named references, error arguments, explicitly seeded error/later-value answers, `NonNullable` refusal with no publication, and early string mapping with its existing internal reuse. Two exercise the observer protocol directly, including exact owner/ordered tuple/result matching, nested and late-bound requests, optional returns and an unreturned guard. Explicit seeds and constructed observer frames are not natural source-level error recovery or recursive publication proofs. An executed reservation-counter omission and an executed error-answer misclassification each fail two tests; original module bytes are restored after both.

The enabled full checker suite passes 1,500 tests with three existing ignores; strict release checker/execute Clippy and formatting pass. The existing and new checker receipt readers pass 85 Python tests, including 13 new reader mutations. Guarded replay refuses the shared repository, wrong revision, dirty checkout, existing output and externally edited restore bytes; a positive replay matches every qualified hook byte and restores a clean checkout.

The app's 12 compiler children preserve complete diagnostics, loaded scope and direct checked identities/order with counters off/on, twice per mode. The 140 public children preserve ordinary/off/on/repeat Rust outputs. Native full-diagnostic comparisons match 24 of 28 cases; `same-spelled-identity-negative` and `flatten-negative` remain failed in both modes, linked to fidelity work `tsr-1yb.6.68` and `tsr-1yb.6.58`. The 30 pool-control children match outputs, including six native comparisons; two deliberate aborts refuse completed receipts. App/public runs observe no answered reservations or reference-map error answers, so their absence cannot substitute for natural integration controls.

With counters disabled, all 476,787 type assertions and 10,570 complete diagnostic cases produce byte-identical ordinary/probe payloads. There are 465,685 RIGHT type assertions; diagnostic counts are 3,462 RIGHT, 4,894 EMPTY_RIGHT, 2,026 WRONG and 188 EMPTY_WRONG. No previous RIGHT result is lost. Differences from older source snapshots belong to intervening fidelity ports, not this observer. Native full type/display parity and complete-project work equivalence are not newly established here.

The local receipts retain failures: the first ordinary build lacked the vendored libraries; exact setup refused an outdated pool tuple and writer marker; the first string-mapping assertion incorrectly expected plain `string`; initial Clippy required `#[must_use]` on the request guard. Corrected stages are separate records. All nine runtime hook files and three prior private target binaries are restored; isolated Rust, native, fixture and replay copies are clean. Builds and owned heavy checks run serially. Two app repetitions describe observer overhead only; they provide no ordinary saved-wall ceiling or confirmed speed improvement. The comparable TSR/native median target of **0.50** remains unmet/unverified, and the broader mapper/context gates remain open.

Replay requires a separate, clean checkout at the exact Rust pin, with the pinned native libraries available for building. The diagnostic allocator inherits the existing macOS-only `malloc_size` probe. Choose a fresh receipt directory outside the checkout:

```sh
python3 docs/architecture/replay_checker_reference_states.py \
  --source /tmp/isolated-tsr-96520fe7 --output /tmp/reference-state-replay

# In that isolated checkout, with an external Cargo target directory:
TSR_MAPPER_PROFILE=1 cargo test --release --offline -p tsr-checker \
  --lib reference_state_probe::tests -- --test-threads=1

python3 docs/architecture/checker-reference-state-controls.py \
  --bindings /tmp/reference-state-bindings.json \
  --output /tmp/reference-state-controls --repeats 2

python3 docs/architecture/replay_checker_reference_states.py \
  --source /tmp/isolated-tsr-96520fe7 --output /tmp/reference-state-replay --restore
```

The bindings file supplies `normal`, `probe` and `native` records with actual binary path, SHA-256 and source SHA. Build ordinary and probe binaries outside measurements and preserve both. The driver validates exact pins and the qualified native binary hash, clears inherited compiler/probe controls, persists complete receipts and rejects changed output/scope or counter reconciliation. Use `--project /path/to/tsconfig.json` for app comparisons, or `--pool-controls` for admission/no-check/abort controls. Keep private project contents in local receipts.
