# Circular generic defaults

`tsr-6.63` is a correctness prerequisite for concrete-member reuse
(`tsr-1yb.4.2.1`). Frozen main `f0b32ab8` aborts with stack overflow for:

```ts
interface MC_Base<T> { base:T }
interface MC_Derived<T=keyof MC_Derived> extends MC_Base<T> { own:number }
declare const x:MC_Derived;
const a:number=x.own; type MC_Keys=keyof MC_Derived;
```

Pinned tsgo `5b1047d10d32e7d5b446be4de56b126ff42f82bb` returns
`input.ts(2,24): error TS2716: Type parameter 'T' has a circular default.`
The exact fixture, options and all six control inputs are retained in
[the receipt](checker-default-circularity.json). The earlier public fixture
also remains in `checker-native-member-completion-fixtures.py`.

The original trace repeats a bare generic reference through its `keyof`
default before default-resolution state is published. The cycle does not
require heritage traversal. Native `getResolvedTypeParameterDefault` owns a
parameter-local slot: publish resolving before evaluating the default, mark
only the re-entered slot circular, and prevent its outer evaluation from
replacing the circular result. `checkTypeParameter` checks the written default
first, which determines the re-entered parameter in a mutual cycle.

The Rust state belongs to one private Checker and its Program lifetime. Its
key is the private parameter TypeId, effective outer alias bindings by symbol
identity, and mapped-template mode. Uncomputed, resolving, circular, completed
absence, completed value and unsupported evaluation remain distinct. A fresh
instantiated parameter resolves its target outside unrelated caller alias
frames before applying the saved target mapper. Defaults still undergo the
existing caller substitution and forward-reference rules. The expensive
worker is default-node evaluation or target-default instantiation.

The first candidate passed focused native controls but lost one previously
RIGHT assertion in `ramdaToolsNoInfinite2`. Its default evaluated to named
unresolved `Prev<N>`; the payload-free unsupported state erased that value to
the intrinsic gap. The final implementation propagates the evaluated unresolved
TypeId and removes its cache entry, preserving the named result without claiming
completed reuse. Probe off/on produced identical complete case payloads. An
additional red test exposed unrelated ambient bindings changing a captured
mapper's target default; target resolution now preserves the mapper boundary.

Verification on the final ordinary release source:

- Six full CLI controls match pinned native complete diagnostics and ordered
  loaded-file scope, including valid defaults, mutual cycles and repeated queries.
- The complete 493-row Ramda output returns to the baseline payload.
- All 477,652 unfiltered type rows are byte-for-byte identical to baseline;
  no previously RIGHT type or diagnostic cases are lost across 10,570 diagnostic
  cases. One already-WRONG diagnostic case gains its expected TS2716.
- 1,500 checker tests pass, with three existing ignores. Strict Clippy and the
  upstream-anchor check pass. Review was targeted manual; no independent agent
  review was performed.

The receipt preserves the rejected candidate, red/green checks, failed initial
build and the disproved stable-reparse-TypeId test assumption. Builds, probes and
small correctness-control timings are excluded from performance qualification.
This adds no concrete-member cache and makes no speed claim. Existing growth
and mapper-context audits retain key-cost/storage obligations. The whole-project
TSR/tsgo median wall target of 0.50 remains unverified.
