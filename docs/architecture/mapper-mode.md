# Print and semantic instantiation context qualification

`tsr-1yb.4.1.2.3.1` delivers executable controls on frozen Rust
`ed40c6c8ed0ddbfb0bdab03dcaec42b025e63e2e` against native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The [receipt](mapper-mode.json) binds sources, the proposal, ordinary binaries,
complete corpus outputs, native/public controls, timings and restoration.

**The mode-key representation is rejected by its preset whole-CLI preservation
gate.** The underlying cache-context repair `tsr-1yb.4.1.2.3` stays open.
Canonical checker and vendor code remain unchanged. No runtime speedup or native
2× result is claimed.

## Defect and bounded proposal

`inference.rs::rename_own_type_parameters_for_print` temporarily enables
`identity_unmapped_type_parameters` and restores it after substitution. That
mode preserves otherwise-unmapped parameters for printing. Persistent
`instantiated_objects` and `instantiated_signatures` keys contain only the source
TypeId and ordered source/image pairs, so restoring the flag leaves a successful
print-created answer visible to later semantic requests.

The object control maps only T in `{value:T;other:U}`. Cold semantic substitution
refuses. Print substitution succeeds; the same semantic request then improperly
returns that cached handle. The signature control reproduces this for a callable
capturing T/U with no own generic parameters. Both controls exercise the production
instantiation boundaries in the existing inference test module, and both fail on
the frozen baseline. The repaired controls pass print-first and semantic-first
orders, plus restoration to semantic mode. Omitting the mode from the proposed
key makes both fail again.

Native's real `instantiateType` retains an unmapped parameter, maps a matching
parameter and releases the active mapper. The [Go control](mapper-mode-native-test.go)
passes three repetitions; its checker/mapper source hashes were reverified.
This does not authorize removing every Rust missing-inference refusal. Those
entries can represent unsupported work. The candidate preserves the current
refusal contract; it does not prove every partial map is a native-supported
semantic request.

The [proposal](mapper-mode-proposal.patch) introduces one private key and
constructor: source TypeId, owned ordered substitutions, and the captured print
mode. All four object/signature producers use it. Separate tables, source/image
identity and duplicate precedence remain intact. Type-literal reservation/error
replacement, mapped error reservation/completion and signature successful
publication remain in their existing order. The helper captures the mode before
recursive printing can temporarily change it. Existing checker-local snapshot
and rollback consumers continue cloning the same tables.

This is one context repair, not a complete mapper equivalence contract.
Parameter/name, fixing, alias and other operation contexts still require the
broader `tsr-1yb.4.1.2` contract before reuse expands.

## Correctness qualification

- Both new production-boundary tests fail on the fresh baseline, pass with the
  partition and fail with a deliberate constant-mode mutation. The mutation is
  restored exactly before final builds.
- The release checker suite has 1,492 passing tests across 101 result blocks,
  with three existing ignored tests. Strict release library/test Clippy passes.
- Formatting initially found only indentation in the inserted test block. Only
  that block was formatted; the final full archive formatting check passes.
  Final regressions, Clippy, ordinary binaries and full corpora were requalified.
- Complete type output is byte-identical: 475,538 assertions, 464,198 RIGHT,
  1,602 GAP and 9,738 WRONG, with zero previous RIGHT losses. All 10,570
  diagnostic-case outputs are also byte-identical. These are complete payload
  comparisons, not equality of aggregate counts.
- The [public fixture](mapper-mode.ts) and [bound runner](mapper-mode-controls.py)
  complete 12 baseline/candidate/native children across positive/error variants
  and default/single modes. Complete diagnostics match; the negative has four
  native TS2322 assignments. Loaded order stays equal across each tool's modes
  and between the Rust baseline and candidate.
- The existing eight-family mapper runner completes another 64 children for
  each Rust binary. Both match native on 15/16 variants. The unchanged nominal
  negative lacks native's nested private-brand diagnostic detail (`tsr-6.68`).

Public fixtures observe supported captured/shadowed generic semantics. They do
not independently trace the internal cache-mode sequence, every lazy worker,
physical library bytes or complete cross-tool input equivalence. No checker-backed
emit or general native print-protocol coverage is established here.

The simplification pass found no additional change: one constructor already
replaces the repeated key construction, and the two test setups express different
production shapes. Targeted manual review excluded concurrent diagnostic work,
as the repository maps agent reviews to sequential work. Key storage/hashing and
extra mode entries require measurement; the review does not infer their wall cost.

## Preset whole-CLI gate and rejection

Before any application timing, this correctness-prerequisite policy was frozen:
for each of two independent five-pair rounds, default and single mode must each
have at most 20 ms median wall regression and at most 5% growth in median peak RSS.
No optimization speedup was required or claimed. The full native ratio target
remains TSR/tsgo <=0.50 with equivalent required work.

| Round | Mode | Baseline median | Candidate median | Candidate wall change | Median peak-RSS change | Gate |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 1 | default | 3.157631 s | 3.131270 s | -26.36 ms | +0.18% | pass |
| 1 | single | 4.953276 s | 4.938104 s | -15.17 ms | -2.37% | pass |
| 2 | single | 4.986469 s | 4.961988 s | -24.48 ms | +1.83% | pass |
| 2 | default | 3.130287 s | 3.163069 s | +32.78 ms | +1.10% | **reject** |

All 40 timed fresh compiler processes and ten preflights complete. Builds and
tests finish before sampling, role order alternates, and round two reverses mode
order. Incremental/composite reuse is disabled; the filesystem is warmed.
Complete 124 diagnostics, 14,050 ordered loaded files, 1,397 reported checks and
14,746 reported parses remain unchanged. Options, compiler binaries and observed
input bytes remain stable. The unchanged driver covers prior observed query
paths plus current loaded/config/library bytes; unobserved queries and transient
changes are excluded. Ordinary binaries do not prove actual default worker
admission or all equivalent native semantic work.

Ranges overlap, and these observations do not establish that the new key caused
the regression. They nevertheless fail the preset retention rule. Averaging
rounds, relaxing the threshold, or keeping only the passing modes would change
the decision after sampling. The representation is therefore not retained.

All four owned private runtime files are restored byte-for-byte to frozen main,
and the three ordinary target binaries to their baseline hashes. Candidate
binaries and exact source remain archived for reproduction. The canonical runtime
was never edited. The next action is current mapper-key attribution
`tsr-1yb.16.1`: separate construction/hashing/storage cost from actual worker
executions before selecting another context-safe representation. Wider reuse and
`tsr-1yb.4.3` remain gated by the unresolved repair and contract.

## Reproduction boundaries

Use an owned clean archive at the frozen Rust/native pins. The
[regression snippet](mapper-mode-regressions.rs) belongs inside the existing
`inference.rs::tests` module after its imports. Appending it to an otherwise clean
baseline produces the two red tests. Separately, applying the full proposal to a
clean baseline supplies both the runtime change and the formatted tests:

```sh
cargo test --release --offline -p tsr-checker --lib print_created_
cargo test --release --offline -p tsr-checker
cargo clippy --release --offline -p tsr-checker --lib --tests -- -D warnings
cargo fmt --all -- --check
```

For the native control, copy the existing `native-mapper-identity-test.go` helpers
and the new mode control into `internal/checker` in an owned native archive; run
`go test ./internal/checker -run '^TestMapperModeUnmappedIdentity$' -count=3` with
the pinned toolchain. The canonical vendor remains untouched.

The public runner accepts a build-binding JSON containing baseline/candidate
`source`, absolute binary `path` and `sha256`, plus the qualified native binary:

```sh
python3 -B docs/architecture/mapper-mode-controls.py   --bindings /absolute/path/build-bindings.json   --native-binary /absolute/path/native-off   --output /absolute/path/fresh-controls
```

The receipt names all archived raw outputs and hashes. Binding supplied source
labels to recorded binaries is evidence for this build, not universal attestation
of arbitrary caller-provided labels. Frozen correctness/timing results do not
qualify later concurrent source changes.
