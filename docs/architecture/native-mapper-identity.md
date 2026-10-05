# Native mapper identity and composition controls

`tsr-1yb.4.1.2.1` characterizes native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` before more substitution consumers or
cache representations are ported. The current Rust inventory is frozen at
`0ba36938`; this unit changes neither production Rust nor canonical vendor.
The parent `tsr-1yb.4.1.2` remains open for its wider inventory, native work counts
on representative projects, and implementation handoff.

The next contract slice is `tsr-1yb.4.1.2.2`: reconcile current Rust substitution
consumers with these executable lifetime distinctions before extending reuse.
Allocation attribution remains with `tsr-1yb.16.1`; this audit does not choose a
representation from equal answers alone. Relation identity is separately split
into native executable controls (`tsr-1yb.4.1.4.1`) and their dependent Rust
key/context handoff (`tsr-1yb.4.1.4.2`). Keeping the native characterization ready
before a Rust cache candidate avoids making the proposed representation its own
oracle. The dependent handoff still requires publication and measured worker
cost evidence before production retention; no speed benefit follows from these
task definitions.

The forcing constraint is concrete: equal substitution answers are not native
mapper identity. Mapper callbacks can change their answers, and inference reads
invalidate active caches. Flattening all these forms into an interned vector
would silently change ownership, ordering or lifetime. These controls make those
distinctions executable; they do not select a production representation.

## Identity and lifetime decisions

All native anchors below are under `vendor/typescript-go/internal/` at the pin.

| Native operation | Exact identity or behavior | Owner and reuse boundary | Executable evidence |
|---|---|---|---|
| `checker/mapper.go:40`, `newTypeMapper`; `:124`, simple lookup; `:154`, array lookup | Exact `*Type` source pointer; ordered pairs, first duplicate source wins. One source selects the simple factory. Reordering both distinct pairs can preserve answers while changing an ordered list key. | New mapper object per factory call; no structural mapper equality contract. The array factory retains caller-owned source/image slices, without cloning. Treating those slices as immutable requires a new ownership proof. | `OrderedPairs`, `ShadowedAndCheckerLocal` |
| `checker/checker.go:22171`, `findActiveMapper` | Exact `*TypeMapper` pointer equality, searching the active stack from the end. Equal answers and equal mapper kind are insufficient. | Private checker and this active stack; no cross-checker or process-wide mapper identity. | `OrderedPairs` directly checks an equal-answer mapper misses active lookup. |
| `checker/mapper.go:65`, merge; `:72`, prepend; `:79`, append; `:260`, merged map | `m2.Map(m1.Map(t))`. Prepend and append have different precedence; reversed composition changes answers. | The composed object retains both mapper objects. | `CompositionAndReceiver` |
| `checker/mapper.go:47`, combine; `:54`, composite helper; `:286`, composite map | If the first mapper changes the type, instantiate inside its image under the second mapper; otherwise directly apply the second. `T -> Box<U>` then `U -> string` reaches `Box<string>`; a merged direct map leaves `Box<U>`. | Composite captures the checker and both mappers; recursive instantiation uses the checker's private caches. | `CompositionAndReceiver` compares direct merge with recursive composition. |
| `checker/mapper.go:135`, `MapsThisOnly` | The source parameter's `isThisType`, not its printed name. A receiver source maps independently of ordinary generic parameters. | Receiver parameter identity stays in the same private checker; member realization is a separate boundary. | `CompositionAndReceiver`; public `Derived.next().own` |
| `checker/mapper.go:214`, deferred; `:240`, function map | Matching deferred callbacks run on each lookup; arbitrary function callbacks are live behavior. A stable mapper pointer need not mean a stable answer. | Captured callback state and caller-owned inputs must remain local to their actual lifetime. No callback-result memo is provided by these factories. | `DeferredAnswersAreLive` |
| `checker/mapper.go:303`, inference factory; `:312`, inference map; `checker/inference.go:1273`, context creation | Fixing and non-fixing mappers retain a specific checker/context. First fixing read processes intra-expression sites, clears unfixed answers, fixes that parameter, then resolves it. | Context identity and fixing state matter; parameter identity alone does not identify an inference answer. | `InferenceFixingAndInvalidation`; two public calls infer independent string/number answers. |
| `checker/inference.go:1317`, `getInferredType`; `:1401`, active invalidation; `:1642`, clear | Newly resolved inference answers clear active mapper caches. Clearing cached inference resets only unfixed parameters. | Same non-fixing mapper can answer differently after candidate changes; fixed answers survive the clear. | `InferenceFixingAndInvalidation` directly observes cache clearing and changing/fixed answers. |
| `checker/checker.go:22104`, `instantiateTypeWithAlias`; `:22146`, push; `:22161`, pop | Active entry key writes original type identity and requested alias. A returned nested worker result is cached when that mapper was already active. An unseen mapper is pushed and popped; its top-level result is not retained in this cache. Pop clears the backing map. No unfinished-result placeholder is inserted by this mechanism. | Private active call stack only; depth/count refusal still precedes lookup. Reuse ends at pop or inference invalidation. | `ActiveReuseAndPublication`: six requests execute four workers over two top-level rounds; three recursive requests execute two workers before a returned inner answer is reused. |
| `checker/checker.go:17405`, key builder; `:17570`, object key; `:17603`, conditional key | Ordered local numeric type IDs; alias presence, symbol identity and ordered alias arguments. Object key also includes single-signature mode; conditional key includes `forConstraint`. | Keys are meaningful only within the actual owning checker/target/root. Two independently created private checkers can mint identical numeric IDs and hashes for different pointers. | `InstantiationKeyContext`, `ShadowedAndCheckerLocal` |
| `checker/checker.go:25107`, `createTypeReferenceEx` | Target-owned cache keyed by ordered effective type arguments. | Persistent within that target in the private checker. A returned reference handle can still have lazy members. | `CompositionAndReceiver` checks repeat reference reuse and changed arguments. |
| `checker/checker.go:22304`, object instantiation; `:22485`, conditional instantiation | Map outer parameters to effective ordered arguments; include alias and operation-specific mode. The target/root table is filled after the factory/conditional calculation returns. Mapper pointer is not this persistent key. | Private target/root tables, separate from active mapper reuse. Publication of a handle does not prove all lazy members are forced. | Key-builder controls plus source inspection; recursive mapped/conditional CLI output controls. Their private factory counts/publication states are **not directly traced**. |

The key controls establish tested distinctions, not collision freedom for xxh3.
The internal tests allocate minimal checker state and bind its
`couldContainTypeVariables` worker as `NewChecker` does at
`checker/checker.go:1253`. They exercise the real private methods, but do not
exercise full `NewChecker` initialization, real intra-expression site ordering,
candidate acceptance/priority or every mapper family. Backreference,
array-to-single, recursive conditional-root WIP and lazy member publication need
the parent contract's broader controls. The public CLI children use normal
compiler/checker initialization and establish only observable semantics.

## Current Rust consumers and ownership

At `0ba36938`, `inference.rs:5132` accepts `&[(TypeId, TypeId)]`, separate parameter
IDs and name labels. It checks the first exact source match, completes pending
signature returns, checks for mentioned parameters, then applies refusal/depth
guards and the recursive worker. It has no native active-mapper stack equivalent.
The `identity_unmapped_type_parameters` mode also changes behavior. Neither
printed names nor this vector alone proves equivalence to native mapper objects.

| Current consumer or table | Current key/state | Required boundary before extending reuse |
|---|---|---|
| `inference.rs:1996`, live contextual mapper; `:2074`, live return mapper; `checker.rs:509`, active inference contexts | Node-keyed inference state; produced substitution vectors, fixing state and live reads | Own inference context and parameter identities. Re-reading a live mapper must not reuse a stale answer. |
| `checker.rs:239`, instantiations; `declared.rs:5162`, reference creation; `checker.rs:774`, alias-body evaluations | `(SymbolId, Vec<TypeId>)` with ordered arguments | Existing checker/binder/store owner; preserve alias, receiver and deferred-reference context. Similar tuple shapes do not authorize merging the tables. |
| `checker.rs:727`, instantiated objects; `mapped.rs:766`, mapped instantiation | `(TypeId, Vec<(TypeId, TypeId)>)`; mapped branch first installs `error`, then overwrites with its result | Provisional/error state is not automatically a reusable completed value. Its publication protocol needs reconciliation with native target/root rules. |
| `declared.rs:1752`, type-literal instantiation | Same object key; reserves a named handle before rebuilding members | Recursive handle reservation differs from native active-cache completion. Preserve existing lazy/active refusal guards until their own contract is proved. |
| `inference.rs:5362`, anonymous properties | Object/vector key, populated after successful member/index rebuild | Completed member image belongs to the checker and original source identity. Receiver/alias context must remain explicit. |
| `inference.rs:5439`, signature instantiation; `checker.rs:1068–1071`, signature tables | `(TypeId, ordered pairs)` and minted signature -> composed pairs | Fresh signature parameters, declaration-owned lazy returns and stored composition must survive. Existing images are recursively instantiated when extending the signature mapper. |

The companion JSON contains exact lexical call-site lines and source hashes for
**all 16 Rust files** matching `instantiate_type(` under `crates/`: the 14
production modules `assignment_declarations`, `calls`, `contextual`, `declared`,
`expressions`, `flow`, `indexed`, `inference`, `mapped`, `members`, `signatures`,
`spreads`, `union_signatures`, `variances`, plus the lazy-return integration test
and `tsr-conformance/examples/infergen.rs`. This is a reproducible direct consumer
inventory, not a dynamic call graph. Internal tests/definition lines are retained
and labeled as lexical matches; indirect callers of those consumers are not
additional independently characterized mapper contracts.

The exact mutation boundary for follow-up is the existing `Checker` and its
associated type store/binder, inference context and target/root operation. No
raw `TypeId` may escape into a process-global key. This unit supplies evidence
to those owners; it adds no immutable interner, shared cache or new production
substitution API. Mapper key allocation attribution remains `tsr-1yb.16.1`; its
earlier requested-byte observations did not justify a representation change.

## Qualified controls and reproduction

[`native-mapper-identity-test.go`](native-mapper-identity-test.go) is copied as
`internal/checker/mapper_identity_test.go` into a fresh archive of the pin. Seven
focused tests pass five repetitions, race detection, the complete affected
checker package and vet. Three temporary mutations are rejected by their intended
assertions: printed-name lookup, reversed merged composition and omitted pop
clearing. The archive is restored byte-for-byte afterward; canonical vendor is
unchanged. Compilation/setup failures during fixture development are not semantic
control evidence.

The 21 existing trace-reader controls, formatting, Python syntax, 3,395 native
anchors and section citations pass. The issue-ID gate retains the 190 historical
missing records tracked in `tsr-10`. Production Rust is unchanged, so this unit
does not rerun or claim a new full Rust corpus score.

[`native-mapper-identity.ts`](native-mapper-identity.ts) has positive controls
for ordered pairs, nested composition, same-name nominal types, shadowed method
parameters, independent inference contexts, polymorphic `this`, recursive mapped
objects and readonly recursive conditional inference. The harness appends nine
deliberately wrong assignments. Every negative control produces TS2322 at its
explicit assignment (including the private-brand diagnostic chain).

[`native-mapper-identity-controls.py`](native-mapper-identity-controls.py) reuses
the already qualified baseline/probe binaries from
[`worker-lifecycle-qualification.json`](worker-lifecycle-qualification.json).
It refuses other binary hashes, checks native source anchors against the exact
Git pin, reconstructs the committed lifecycle patch and checks all six probe
files. Three negative guard controls reject a wrong binary, modified native
anchor and modified probe source. These guards bind known artifacts, not
universal build attestation.
Four cases (positive/negative, default/one checker) each run showConfig, baseline,
probe-off, probe-on and repeat: **20 actual fresh compiler children**, eight
accepted worker traces. Complete stdout, stderr, status and loaded order agree
across observers/repeats and worker modes. Positive status is zero; negative
no-emit status is one. Default constructs four checkers, single constructs one;
only one performs the fixture's full-file worker. Those are lifecycle counts,
not mapper instantiation counts.

```sh
# First archive the pin and build/qualify the baseline and lifecycle probe as
# documented in native-worker-activity.md; keep canonical vendor unchanged.
cp -f docs/architecture/native-mapper-identity-test.go "$TASK_NATIVE_COPY/internal/checker/mapper_identity_test.go"
(cd "$TASK_NATIVE_COPY" && go test -p 1 ./internal/checker -run '^TestMapperIdentity' -count=5)
(cd "$TASK_NATIVE_COPY" && go test -race -p 1 ./internal/checker -run '^TestMapperIdentity' -count=1)
(cd "$TASK_NATIVE_COPY" && go test -p 1 ./internal/checker && go vet ./internal/checker)
python3 docs/architecture/native-mapper-identity-controls.py \
  --native-baseline "$TASK_NATIVE_BASELINE" --native-probe "$TASK_NATIVE_PROBE" \
  --native-source "$TASK_NATIVE_COPY" --probe-source "$TASK_NATIVE_PROBE_SOURCE" \
  --output "$TASK_MAPPER_OUTPUT"
```

The public helper deliberately accepts the previously qualified binary hashes;
a different-platform rebuild requires fresh lifecycle qualification rather than
relaxing its guards. Use a task-owned `GOCACHE` when sandbox permissions require
it. The JSON summary records helper, fixture, Go test, source, binary, raw receipt
and test-log fingerprints, supervising PIDs/start/outcomes and wall/CPU/RSS.
Local raw receipts are under `/tmp/tsr-mapper-identity-u0kgq5ex/public-qualified/`.
They are local replay evidence, not durable CI artifacts. Probe timing is separate
from ordinary timing; these tiny controls do not support a throughput conclusion.

Observer output equality does not prove full forcing, complete inputs or native
memory admission. The reader's work/input/provenance/admission/target acceptance
flags remain false. TSR output was not compared in this native characterization;
the previously documented private-brand and readonly Flatten fidelity residues
remain tracked in `tsr-6.55` and `tsr-6.58`. No previously RIGHT Rust assertion can
change from this test/document-only unit, and no Rust corpus delta is claimed.
The comparable whole-project TSR/tsgo median wall ratio <=0.50 remains unverified.

The rejected alternatives now have falsifiers: printed-name and reversed-merge
mutations fail the private assertions; wider active-cache lifetime changes worker
counts; borrowed arrays and changing inference violate frozen-vector assumptions.
A future representation can win only after it preserves these semantics and
demonstrates lower measured cost on complete equivalent work. Passing the controls
alone is a correctness prerequisite, not a speed win.
