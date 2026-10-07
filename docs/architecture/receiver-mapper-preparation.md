# Receiver mapper preparation: two restored rejections

Neither preparation-cache variant meets the required four-checker benefit
gate. Production code and the ordinary release binary are restored exactly to
`5dd3bad84d12991e1ba169d2d5687321e1989740`. This experiment closes
`tsr-1yb.16.3.9`; it does not implement the full member builder or establish the
complete-work TSR/tsgo wall ratio target of 0.50.

The [receipt](receiver-mapper-preparation.json) records binary/source patch
hashes, all paired resource samples, input fingerprints, fixture hashes and
restoration. Rejected patches are archival experiments, absent from runtime:
[V1](receiver-mapper-preparation-v1-rejected.patch),
[V2](receiver-mapper-preparation-v2-rejected.patch). Apply each separately to
the pinned TSR baseline. Do not layer them.

## What can be reused at this boundary

Native `5b1047d10d32e7d5b446be4de56b126ff42f82bb` resolves the concrete reference
mapper in `resolveTypeReferenceMembers` / `resolveObjectTypeMembers` and retains
it through `instantiateSymbol`. Instantiated member read and write types have
separate lazy links. TSR instead prepares the mapper in
`members.rs::instantiate_for_reference_with_this` at the consumer seam.

The [private probe](receiver-mapper-preparation-probe.patch) records successful
preparations per checker, keyed by exact receiver, declared input and original
`this` TypeIds. It compares subsequent mapper fingerprints and result TypeIds
with the first observation of each tuple. These are private checker identities,
not printed names, member symbols or completed native member-table keys.

| Workload | Checkers | Preparations | Distinct tuples | Later mapper fingerprints differ | Later result IDs differ |
| --- | ---: | ---: | ---: | ---: | ---: |
| Generated 400 modules | 4 | 237,548 | 48,740 | 12,002 | 10,800 |
| Generated 400 modules | 1 | 237,236 | 45,981 | 2 | 0 |
| Local API | 4 | 1,984,240 | 95,689 | 20,074 | 15,440 |
| Local API | 1 | 823,349 | 36,669 | 13,921 | 10,760 |

The probe preserves full ordinary CLI output in baseline/disabled/enabled
comparisons. A TypeId difference alone does not prove a different printed or
semantic type. Changing mapper observations still prevent using this tuple as
an unqualified completed-result key. Member completion, alias/origin and
receiver ownership remain subject to the
[existing contract](checker-member-cache-contract.md), with implementation
owned by `tsr-1yb.33.1` / `tsr-1yb.4.2.1` and receiver repair by `tsr-6.69.2`.

Probe intervals include instrumentation and nested work. Preparation excludes
temporary destruction; the substitution interval includes the probe's hash/map
accounting. They cannot be treated as exclusive CPU shares or saved-wall bounds.
The raw local report remains at `/tmp/tsr-member-reuse-evidence/locate.json`;
private application paths/configuration and diagnostic bodies are not exported.

## Candidates and measured gates

V1 retains a prepared mapper in a checker-local `Arc`, keyed by receiver and
original `this`. Each hit revalidates the target symbol, ordered actual
arguments, forced declared parameter identities and current formal `this`.
Parameter declarations are still forced in their original order. Empty/failed
or changed metadata cannot bypass ordinary computation. It retains no member
type result, so lazy signature-return admission, type-variable walking,
instantiation limits and alias presentation still run through `instantiate_type`.
The immutable AST supplies parameter-name strings. A changed parameter identity
and temporary failure/recovery control passes, alongside the other 20 focused
member tests and all 188 checker library tests.

V2 retains only nonempty generic receiver mappers. Empty and nongeneric maps
avoid persistent cache entries. Its ordinary CLI output remains identical;
additional semantic gates were not run after its performance veto.

Each comparison uses one warmup and five alternating fresh-process pairs with
`--noEmit --incremental false --composite false --pretty false --checkers N`.
Both builds use identical effective config, ordered loaded identities and stable
physical input snapshots. CPU/RSS comes from the existing `wait4` helper.
Builds, input hashing and setup stay outside child timings; no sampler runs.

| Variant | Checkers | Baseline median | Candidate median | Change | Peak RSS ratio |
| --- | ---: | ---: | ---: | ---: | ---: |
| V1 | 4 | 2.296532 s | 2.309296 s | 12.8 ms slower | 1.0119 |
| V1 | 1 | 4.349689 s | 4.176086 s | 173.6 ms faster | 1.0330 |
| V2 | 4 | 1.755328 s | 1.753469 s | 1.9 ms faster | 1.0136 |

The predeclared retention gate requires at least 20 ms median benefit in both
four- and single-checker generated comparisons, RSS ratio at most 1.05, no real
API wall regression, and full fidelity before retention. V1's single-checker
observation cannot override its four-checker failure. V2 also fails that gate.
The loop stops after its two variants. Full type/diagnostic corpora, V2
single-checker confirmation and candidate API performance were not run after
the four-checker vetoes. No corpus, native-equivalence or speed certificate is
claimed for either rejected patch.

The host is not isolated. Baseline medians change between batches, so compare
each candidate only with its paired baseline. The fixture emits the same single
intentional TS2322 for all samples. The local API's existing four ioredis parser
errors remain unchanged. These observations do not qualify equivalent complete
native work or identify the worker-tail cause.

## Replay

Generate the same 403-file workload using the tracked generator:

```sh
rtk proxy python3 scripts/generate_perf_project.py --modules 400 --out /tmp/tsr-member-reuse-400
```

Build baseline and each separately applied patch in isolated source/target
directories, freeze the ordinary binaries, and run the existing TSR/TSR harness:

```sh
rtk proxy python3 scripts/dependency_parse_perf.py \
  --baseline /tmp/baseline-tsr --candidate /tmp/candidate-tsr \
  --baseline-source 5dd3bad84d12991e1ba169d2d5687321e1989740 \
  --candidate-source 5dd3bad84d12991e1ba169d2d5687321e1989740+v2 \
  --project /tmp/tsr-member-reuse-400 --output /tmp/receiver-replay \
  --samples 5 --checkers 4
```

The harness's optional checker count applies the same setting to both builds;
omitting it preserves its previous behavior. A one-pair V2 replay validates
that option and the same input/output guards, separately from the five-pair
decision samples. The receipt records its result.

The remaining work is to qualify and reuse the actual completed member images
and instantiated symbol links through their existing ownership tasks. This
experiment supplies no reason to repeat string borrowing alone or to add a
second reference-identity cache.
