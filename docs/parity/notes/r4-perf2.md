# Parity lane `r4-perf2` — member memo tables (round 4)

Lane issue `tsr-2zk.937` (parent `tsr-2zk.17`). Box protocol:
`docs/parity/box-protocol.md`. Pinned upstream: `vendor/typescript-go` @
`5b1047d`. Predecessor: [`r4-perf.md`](r4-perf.md) (C7, C9; its gates and
`memo_frames` admission rule are reused here unchanged). Release target
(CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r4-perf2.md` §N). Every number
names the source state and measurement it came from.

## §1 Method

- Source base `eec4b6b` (r4-perf's C7+C9 `cdc49ee` merged with the
  integration branch at `bc1e54e`). 4-vCPU cloud container, Linux 6.18,
  `RUSTUP_TOOLCHAIN=stable`.
- **Ir**: `valgrind --tool=callgrind` total instructions of the release `tsr`
  on `python3 scripts/generate_perf_project.py --modules 100`,
  `--singleThreaded --pretty false`. Deterministic; every variant's complete
  CLI output is `cmp`-identical to the base's.
- **Corpus**: both unfiltered dumps (`diagverdictdump`, 10,570 rows;
  `verdictdump`, 477,985 rows) against the base, frozen from a separate
  worktree build of `eec4b6b`.
- **Wall/CPU**: `scripts/whole_project_perf.py`, 21 samples, default
  scheduling, medians; against the previous binary (self-comparison) and
  against native tsgo built from the pinned submodule
  (`scripts/offline-cargo/build-tsgo.sh`).

Base Ir at `eec4b6b`: **3,404,376,675** (r4-perf measured 3,530,032,122 at
`cdc49ee`; the integration branch's merges account for the difference).

## §2 C3 — instantiated member types (`PerfLinks::reference_member_types`)

**Forcing measurement.** At `eec4b6b`, `instantiate_for_reference_with_this`
was 7.73% of inclusive Ir (263.2 M), about 52,000 requests on the
100-module project, each re-running `instantiate_type` for the receiver's
mapper.

**Native operation.** `getTypeOfInstantiatedSymbol` (`checker.go:16528`):
`instantiateSymbol` (`:20753`) gives each member of an instantiated
reference an instantiated symbol whose `links.resolvedType` is
`instantiateType(getTypeOfSymbol(target), mapper)`, computed once. Consumers
here: `get_type_of_property_with_this_argument` (the symbol road and the
tuple-through-`Array` arm) and `instantiate_for_reference`.

**Identity and owner.** Key `(receiver, declared, this_argument)`, all
`TypeId`s of the owning checker. `declared` stands for the target symbol
(`getTypeOfSymbol(links.target)`), `receiver` for the mapper (its
`type_reference_targets` arguments, or none for a non-generic class or
interface kept as `Named`), and `this_argument` for the mapper's polymorphic
this image. Value: `(target's polymorphic this at publication, result)`.
Private `Checker`, Program lifetime.

**Mapper mode is not in the key.** Print mode
(`identity_unmapped_type_parameters`) substitutes identity for foreign
parameters, so a print-mode answer is not the real instantiation; the
admission (`memo_frames`) never admits print mode or a mapped-template frame,
so neither reads nor publishes, and the key needs no mode bit. Adding the mode
to the key would let print mode publish answers no real consumer may read.

**Publication.** Absent = uncomputed. Published only when all hold:
1. admitted by `memo_frames` over the target symbol's declarations (r4-perf
   §2's rule: an alias-evaluation frame whose bound parameter's scope encloses
   a declaration keeps the old frame-sensitive computation); the computation
   runs with the admitted frames taken;
2. no resolution on the stack and no flow loop active
   (`signature_links_publishable`) — the brief's "never cache a member still
   being resolved": under an active resolution a circular read answers
   `errorType` somewhere inside the instantiation;
3. the answer is not `errorType` (an unresolved parameter list, arity
   mismatch, pending return, depth or count limit — all possibly
   provisional);
4. if a non-empty mapper was applied, the answer is not `declared` unchanged,
   unless `declared` is a leaf type (primitive, literal or enum, or a union of
   them). An unchanged object answer read `declared`'s current contents:
   `TypeStore::complete_object` fills a reserved object in place and
   `peek_property_type` skips edges not yet published, so "mentions no type
   parameter" can become false later. A leaf has no such content. An empty
   mapper answers `declared` whatever it holds and is always decided.
5. the target's polymorphic this did not change during the computation.

The this-type is minted lazily, so a hit is accepted only when the target's
current polymorphic this equals the published one.

**Work boundary.** The worker is `local_type_parameter_types_of` plus
`instantiate_type`; a hit is the admission test, one hash lookup and the
polymorphic-this read.

**Probe** (temporary counters, not committed; 100-module project): of about
52,000 requests, about 3% refused admission, 66% hits, 8% computed and published,
10% not decided (rule 4: an object unchanged after a real mapper; error
answers: 0), 12.5% computed under an active resolution. About half of the
last group repeat a key, so allowing publication under resolution would save
roughly another 1% Ir; the brief forbids it and this port has no cheap way to
see whether a circular read happened inside the computation (the cycle marks
live in `resolution.rs`'s private frames).

**Measured** (§1):

| | Ir | Δ |
|---|---:|---:|
| base `eec4b6b` | 3,404,376,675 | — |
| C3, unchanged answers never published (round 3's rule) | 3,331,940,339 | −2.13% |
| C3 as shipped (leaf exception, rule 4) | 3,330,344,380 | **−2.18%** |

`instantiate_for_reference_with_this` inclusive (at 3,329,809,578, a build of
the same source before formatting): 263.2 M → 189.2 M, of which
the worker is 164.7 M and `memo_frames` admission 40.6 M (§5).

**Corpus.** Both dumps `cmp`-identical to the base.

**Wall and CPU** (§1; C3 against the `eec4b6b` binary, 21 samples, medians):

| Project | wall | CPU |
|---|---:|---:|
| generic-imports | 96.8 / 98.4 ms (0.984) | 0.975 |
| domain-model | 204.7 / 206.1 ms (0.993) | 0.982 |
| domain-model-large | 725.1 / 757.5 ms (0.957) | 0.981 |

**How we would know it is wrong.** A §5 loss under the patch; or a member read
through a generic reference whose type differs between the first request
inside an alias evaluation and one outside it (the admission rule), or one
whose declared object type gains a type-parameter edge after the first read
(rule 4).

