# Type-parameter constraint node reuse (`tsr-2zk.16.69`)

## Source-bound investigation

Source: `c8185606e3b972d59d345b6e45d789586d993af8`.
Native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
No checker implementation changes or case conversions in this investigation.
The existing visitor is sufficient for the reproduced constraint; its caller
and stored representation are outside this worker's ownership.

Native binary SHA-256:
`0121c87324737d04a4d0728f05b8ca73d25002234d672cdffc13dcdec3bf9b1c`.
TSR CLI SHA-256:
`50fd451ef234b67b97b7458a1024a5832fa09058ec51affd6e4b519634470aff`.
Built with the repository's Rust 1.96.0 toolchain, release profile.

## Actual pinned operation

`typeParameterToDeclaration` (`nodebuilderimpl.go:1611`) obtains
`getConstraintOfTypeParameter(parameter)` and the written constraint from
`getConstraintDeclaration` (`checker.go:29132`). The latter walks the
parameter symbol's declarations in order and returns the first type-parameter
constraint node.

`typeToTypeNodeHelperWithPossibleReusableTypeNode`
(`nodebuilderimpl.go:1597`) reuses that node only when the builder is not
actively expanding and `getTypeFromTypeNode(node, false) == constraint`.
Successful `tryReuseExistingNodeHelper` is followed by
`checkTypeExpandability`; otherwise the constraint is serialized from its type.
This gate is **strict identity**, not the error-type charity of
`pseudoTypeEquivalentToType` used for parameter/return annotations.

`typeParameterToDeclarationWithConstraint` (`nodebuilderimpl.go:1329`) is
not the reuse decision. It temporarily clears
`FlagsWriteTypeParametersInQualifiedName`, builds modifiers and the parameter
name, serializes the default from its type, restores flags, and assembles the
provided constraint node into the declaration.

The existing-node visitor inserts keyword `any` into an untyped parameter
with no initializer (`nodecopy.go:660`), including a rest parameter. Its
semantic type can still be `any[]`; the reused node prints `...args: any`.

## Direct native and actual TSR controls

Ran both actual CLIs with `--noEmit --pretty false` on:

```typescript
type Keys = "a" | "b";
declare function aliases<K extends Keys>(): K;
declare function rest<T extends (...args) => void>(): T;
declare function callable<T extends { (...args): void }>(): T;
const a: never = aliases;
const b: never = rest;
const c: never = callable;
```

Both emit TS7019 at `(3,34)` and `(4,40)`:
`Rest parameter 'args' implicitly has an 'any[]' type.`
Both then emit TS2322 at `(5,7)`, `(6,7)`, `(7,7)`, in that order.
The first message agrees:
`Type '<K extends Keys>() => K' is not assignable to type 'never'.`
Native's remaining messages are:

```text
Type '<T extends (...args: any) => void>() => T' is not assignable to type 'never'.
Type '<T extends { (...args: any): void; }>() => T' is not assignable to type 'never'.
```

TSR substitutes `...args: any[]` in both messages. The actual corpus
`probefile` pipeline reproduces the same difference on the ad-hoc control and
on `compiler/declFileRestParametersOfFunctionAndFunctionType`.

An additional throwaway Rust executable used the actual parsed/bound `Checker`
on a function returning `<K extends Keys, T extends (...args) => void>() => T`.
Its existing `Signature::written_return`, rendered by
`site_free_annotation_text`, produced exactly:

```text
<K extends Keys, T extends (...args: any) => void>() => T
```

Replacing the current type with the intrinsic `number` refused reuse. Thus the
owned visitor is runnable and already preserves the alias and inserts native
`any`; top-level constraint dispatch is the missing prerequisite.

## Continued native controls: aliases, defaults, receiver and normalization

Additional actual native/TSR CLI runs used `--noEmit --pretty false` on the
following independent controls. These further isolate the parent cutover; they
are not implementation conversions.

```typescript
type Keys = "a" | "b";
declare function written(): <K extends Keys = Keys>(value: K) => K;
const writtenResult: never = written();
declare function nested(): <T>(value: <T extends Keys = Keys>(x: T) => T) => T;
const nestedResult: never = nested();
interface Receiver { method(): <T extends this = this>(x: T) => T; }
declare const receiver: Receiver;
const receiverResult: never = receiver.method();
declare function callable(): <F extends { (...args): void }>(f: F) => F;
const callableResult: never = callable();
declare function returnedDefault<T extends Keys = Keys>(): T;
const serializedDefault: never = returnedDefault;
```

Both CLIs match TS2322 messages at `(3,7)`, `(5,7)`, and `(12,7)`:

```text
Type '<K extends Keys = Keys>(value: K) => K' is not assignable to type 'never'.
Type '<T>(value: <T extends Keys = Keys>(x: T) => T) => T' is not assignable to type 'never'.
Type '<T extends Keys = Keys>() => T' is not assignable to type 'never'.
```

At `(8,7)`, native prints
`Type '<T extends this = Receiver>(x: T) => T' is not assignable to type 'never'.`
TSR prints `extends Receiver = Receiver` instead. At `(9,44)`, both emit TS7019;
at `(10,7)`, native prints `...args: any` and TSR prints `...args: any[]`.
The diagnostics retain this order. The actual corpus pipeline also shows the
receiver distinction: the method declaration prints the written
`() => <T extends this = this>(x: T) => T`, whereas the `receiver.method`
reference currently prints `extends Receiver = Receiver` in TSR. Native's
reference message retains constraint `this` and serializes only the default.
`nodecopy.go:552` explicitly preserves `ThisTypeNode`; replacing it based on
a guessed receiver equivalence would contradict the pin.

A separate namespace-local alias control:

```typescript
namespace N {
    export type Keys = "c";
    export function factory(): <T extends Keys = Keys>(x: T) => T { throw 0; }
}
function shadow() { type Keys = number; const result: never = N.factory(); }
```

Native keeps constraint `Keys` but serializes default as `"c"` in the returned
signature's TS2322 message; TSR prints `"c"` for both. Controls with an outer
type parameter named `Keys`, or a nested generic named `T`, match both CLIs.
Written-node defaults and semantically serialized defaults are different paths;
copying all defaults/constraints as text is not a faithful cutover.

A further strict-identity countercontrol:

```typescript
declare function ordinary<T extends any = any>(): T;
declare function missing<T extends Missing = Missing>(): T;
const a: never = ordinary;
const b: never = missing;
```

Both emit TS2304 at `(2,36)` and `(2,46)`, followed by TS2322 at `(3,7)` and
`(4,7)`. Native prints `<T extends unknown = any>() => T` for `ordinary`; TSR
prints `<T extends any = any>() => T`. Both retain
`<T extends Missing = Missing>() => T` for `missing`.
`getConstraintFromTypeParameter` (`checker.go:17071`) normalizes a non-error
`any` constraint to `unknown` (or `stringNumberSymbolType` for mapped keys)
before the node builder tests identity. The error-type constraint is not
normalized. Therefore an unconditional annotation-reuse rule would introduce
an incorrect `extends any` result even with a working visitor.

Native `nodecopy.go:560` visits type-parameter names, constraints and defaults;
its function/mapped-type wrapper (`nodecopy.go:835`) enters and exits
`enterNewScope`. Name allocation and fake-scope ownership remain the shared
renderer contract, not a new visitor-local naming cache. The existing
`instantiate_signature` (`inference.rs`) currently clears `written_constraint`
when a constraint image changes. Parent must coordinate any required native
receiver/mapper treatment with that owner; this worker did not edit it.

## Owned visitor regression controls

`crates/tsr-checker/tests/constraint_node_reuse_native.rs` now exercises actual
parsed/bound signatures and the existing retained-node renderer. Three release
tests pass, with no ignored tests or corpus exceptions:

- Written alias constraints plus both function-type and callable-object
  constraints insert native `...args: any` rather than semantic `any[]`.
- A reused generic node preserves `Public.Keys` in both its written constraint
  and written default. Native's separately serialized default can instead be
  `Keys`; this deliberately prevents conflating the two paths.
- Nested generic declarations sharing the spelling `T` preserve each written
  scope and the alias constraint/default.

Each control also replaces the current semantic type with intrinsic `number`
and requires reuse refusal. The tests protect visitor behavior and an actual
semantic-image boundary; they do not claim the blocked top-level constraint
cutover is implemented. `cargo test --release -p tsr-checker --test
constraint_node_reuse_native` reports 3 passed; `rustfmt --check` on the owned
test file succeeds. The existing throwaway actual visitor executable also
passes after the test addition. Compiler source and CLI binary identities are
unchanged, so the earlier full-corpus zero-loss and performance receipts remain
applicable without attributing test-only additions to a speed change.

Additional actual native/TSR accessibility controls:

- A nested generic constraint `typeof key` and parameter named `key` agree in
  both CLIs, including the returned-signature path. No fake-scope change is
  justified by that control.
- An unexported namespace-local `Keys` remains written in both the constraint
  and default through references, including a function-local shadowing type
  alias. Both CLIs agree.
- An exported single-literal namespace alias remains written as constraint
  `Keys` in native but has serialized default `"public"`; TSR's diagnostic
  prints `"public"` in both fields. This is the previously identified shared
  semantic-serialization path, not evidence to relax visitor accessibility.
- An explicitly qualified union alias constraint `Public.Keys` remains written
  in native; its semantic default prints `Keys`. TSR's diagnostic prints
  `Public.Keys` in both fields. Top-level, function-local value shadowing, and
  sibling namespace controls reproduce the same result.

No new cache, traversal, production helper, or shared metadata change was added.
`node_reuse.rs` remains unchanged because the directly exercised visitor
behaviors already match the pin. The cluster remains in progress with the
parent prerequisite; no next cluster was taken.

## Target-shaped operator continuation

The parent reports ownership of its per-signature display helper in
`printing.rs`; this worker leaves that helper, signature metadata, and source
constraint/default pins untouched. The next actual native/TSR controls placed
these target constraint shapes inside a written returned generic function:

```typescript
declare function source(): <T extends number[] & { [s: string]: number | string }>() => T;
const result: never = source;
declare function keySource(): <K extends keyof { sum: [a: number, b: number]; concat: [a: string, b: string, c: string] }>() => K;
const keyResult: never = keySource;
declare function indexSource(): <K extends { key: number | string }["key"]>() => K;
const indexResult: never = indexSource;
```

Both actual CLIs agree on all TS2322 messages, at `(2,7)`, `(4,7)`, `(6,7)`,
in order. Both preserve `number | string` inside the intersection's index
signature, `keyof` with its tuple-member literal, and the indexed-access
constraint. The actual corpus pipeline shows the same written forms while
printing the standalone `key` property type as `string | number`. Thus written
node preservation does not alter semantic ordering globally.

A fourth owned regression test covers these three operators plus the existing
unrelated-image refusal boundary. `cargo test --release -p tsr-checker --test
constraint_node_reuse_native` now reports **4 passed, 0 failed, 0 ignored,
0 filtered out**; owned-file `rustfmt --check` passes. CLI binary SHA-256 values
remain identical to the full-corpus/performance receipts above. This test-only
continuation does not claim a compiler fix or new target conversions.

A valid conditional alias with constrained `infer` also matches the actual
CLIs on its returned generic signature. Direct conditional-return and invalid
`infer`-outside-conditional probes encountered unsupported semantic production
and missing TS1338 checking before a reusable node reached this visitor. Those
paths are outside this worker's owned constraint visitor and were not turned
into expected-empty tests or special-case visitor changes. No next cluster was
claimed; the same 16-case constraint cluster awaits parent dispatch.

## Parenthesized target continuation

Actual native/TSR CLI controls with written constraints `keyof (Obj)`,
`(Obj)["key"]`, and `(1)` agree, including nested parentheses on simple operands.
A unique-symbol property inside a constraint also agrees on the source and
returned-signature controls; these runs do not justify changing the visitor's
scope gate. The pinned simple operand helper is
`tryVisitSimpleTypeNode` (`nodecopy.go:452`); unique-symbol scope checking is
`nodecopy.go:595`. No new naming/accessibility policy was inferred from syntax.

A fifth owned visitor regression retains the three parenthesized forms and
requires refusal after an unrelated semantic replacement. Release test result:
**5 passed, 0 failed, 0 ignored, 0 filtered out**; formatting check passes.

Re-ran actual target smoke for both `typeParameterConstraints1` and
`declFileRestParametersOfFunctionAndFunctionType`: **39 RIGHT, 4 WRONG**.
The remaining four lines are exactly top-level `extends any` versus native
`extends unknown`, `extends 1` versus native `extends (1)`, and the two callable
constraint `...args: any[]` versus native `...args: any` forms. The owned visitor
controls for parentheses and callable insertion pass; parent semantic
normalization/constraint display dispatch is still required. No production
source or CLI binary changed, so the full paired no-loss/missing-ID and native
performance receipts remain unchanged. The investigation still claims no
whole-case conversions or completed release gate.

## Source-symbol query accessibility continuation

Actual native/TSR CLI runs on the following control agree on TS2322 messages at
`(3,7)`, `(4,50)`, and `(5,55)`, in source order:

```typescript
const keys = { a: 1, b: 2 };
declare function source(): <K extends keyof typeof keys>(key: K) => K;
const original: never = source;
function shadow() { const keys = { c: 3 }; const view: never = source; }
namespace Other { export const keys = { d: 4 }; const view: never = source; }
```

The actual corpus pipeline preserves `keyof typeof keys` at the original
site, and uses `keyof typeof globalThis.keys` at both shadowed sites. The
source object has keys `a,b`; the two shadows have keys `c` and `d`. These are
distinct source symbols, not printed-text-equivalent inputs. The owned
`track_existing_entity_name` / `serialize_type_name` query path retains the
original source identity and qualifies its fallback name at the print site.

A sixth integration regression exercises that exact site-aware visitor path
via `written_annotation_text_at` on the retained return node. The real parsed
and bound Checker emits all three expected forms. Release suite result:
**6 passed, 0 failed, 0 ignored, 0 filtered out**. No production helper,
cache, shared field or visibility change was introduced. A standalone rustc
probe initially mixed CLI/test dependency profiles and failed to compile; it
provided no behavioral evidence and was replaced by the coherent Cargo target.

Compiler source/binary identity remains unchanged. Prior full unfiltered
zero-RIGHT/EMPTY_RIGHT-loss and zero-missing-ID receipts, all-16-target blocked
count, and unverified performance receipts therefore remain unchanged. This
continuation guards a real accessibility boundary but does not claim a cluster
conversion or a completed release goal.

## Property-constraint continuation (`spreadObjectOrFalsy`)

Actual native/TSR CLI controls place the target's constraint inside a written
returned generic function:

```typescript
declare function source(): <T extends {}, A extends { z: (T | undefined) & T }>(a: A) => T;
const result: never = source;
declare function nested(): <T extends {}, A extends { z: (T | undefined) & T; readonly q?: T | null }>(a: A) => T;
const nestedResult: never = nested;
```

Both emit identical TS2322 messages at `(2,7)` and `(4,7)`, in order. The
constraint preserves `(T | undefined) & T` and the optional annotation
`readonly q?: T | null`. The actual corpus pipeline separately renders `z` as
semantic `T` and `q` as `T | null | undefined`. This verifies that node reuse
preserves written structure without rewriting the underlying semantic types.
The owned path is `reused_type_members` / `reused_type_member` followed by
`emit_reused_type`; the existing native recovery boundary remains unchanged.

Added a seventh owned regression for the intersection/property annotation
boundary and unrelated semantic-image refusal. Release suite result:
**7 passed, 0 failed, 0 ignored, 0 filtered out**; formatting check passes.
Actual target smoke remains **46 RIGHT, 1 WRONG, 0 GAP** for
`conformance/spreadObjectOrFalsy`: the top-level signature still prints
`A extends { z: T; }` rather than the written constraint. Parent dispatch is
required; no production visitor defect was demonstrated.

No compiler source, binary, shared metadata, or visibility edits occurred.
Checked native/TSR SHA-256 values still match the recorded full receipts.
The prior unfiltered pair remains zero verdict changes, prior-RIGHT/EMPTY_RIGHT
losses, missing IDs, and new IDs; native performance remains unverified rather
than accepted. No blanket fresh-TypeId guard, text-equivalence gate, or cache
was introduced, and no other cluster was claimed.

## Diagnostic context versus site-aware local alias recovery

Actual native/TSR CLI controls returned a generic function from a factory with
a function-local alias `type Keys = "a" | "b"`. Native TS2322 at `(6,7)` and
`(7,47)` preserves `() => <K extends Keys = Keys>(key: K) => K`; TSR expands
both fields to `"a" | "b"`. A separate local object alias control agrees in
both diagnostics at `(13,7)` with
`() => <K extends keyof Shape, V extends Shape["key"]>(value: V) => K`.
The actual corpus pipeline instead renders that object constraint structurally,
including `keyof { key: string; }` and `{ key: string; }["key"]`.

Native `typeToString` (`printer.go:177`) supplies
`TypeFormatFlagsAllowUniqueESSymbolType | TypeFormatFlagsUseAliasDefinedOutsideCurrentScope`.
Its diagnostic invocation can have no enclosing declaration. The flag affects
`symbolToTypeNode` (`nodebuilderimpl.go:644`) and alias serialization
(`nodebuilderimpl.go:3362`); it is not equivalent to granting all site-aware
visitor references access. The existing port's site-free visitor declines
scope-local names, while the site-aware visitor checks source/site symbol
identity. Removing that decline globally would conflate diagnostic and corpus
contexts and is not a faithful owned-only fix.

This measured root needs the parent-owned diagnostic/signature display helper
to select the native context, not a text-equivalence test, blanket fresh-ID
rule, or an unconditional accessibility relaxation in `node_reuse.rs`.
No new production helper or regression was added for the failing diagnostic
path because its contextual caller remains reserved to the parent. Existing
seven visitor regressions pass; implementation, full paired verdict/ID receipts,
and performance observations remain unchanged. The cluster stays in progress;
these additional controls do not claim conversions.

## Next measured cluster: existing entity-name tracking (`tsr-2zk.16.75`)

Claimed the next P1 existing-node tracking cluster after the constraint cluster's
parent dispatch prerequisite. Fresh target verdicts for
`compiler/controlFlowForFunctionLike1` contain eight wrong lines: returned inner
function views retain captured `typeof a` where native serializes `number` or
`string | number`. Actual native and TSR CLI runs with `--strict --noEmit
--pretty false` on the native case agree diagnostically; positioned type
assertions expose the display difference.

Tested an owned candidate in `declared_inside_reused_node`: stop treating an
outer captured parameter as signature-local at the nearest function boundary.
Actual target smoke corrected inner views but introduced formerly-RIGHT losses
on outer signature views, which must keep their parameter's written `typeof a`.
The target still contained eight wrong lines. The candidate was rejected and
reverted; no regressing implementation or exception test is committed.

Native `enterSignatureScope` / `enterNewScope` (`nodebuilderscopes.go:53`) own
**dynamic rendered signature** parameter scope. AST ancestry alone cannot tell
whether the current render includes the outer signature or only its returned
inner signature. Required shared contract: provide the active render signature
or its parameter-symbol scope to `ReuseContext` at visitor entry. The parent
owns signature rendering and shared state; this worker did not edit them.
Reusing a captured parameter solely because its declaration is an ancestor is
not sufficient, but replacing it solely because an inner function intervenes
is also wrong.

After reverting and rebuilding, actual target smoke reproduces the baseline
with **0 verdict changes and 0 missing IDs**. The CLI SHA-256 is again exactly
`50fd451ef234b67b97b7458a1024a5832fa09058ec51affd6e4b519634470aff`.
Seven owned regression tests pass after the revert. The unchanged binary
retains the earlier full zero-loss/missing-ID and unverified performance
receipts. Neither cluster is closed; the measured candidate is a rejected
experiment, not an implemented fix or claimed speed win.

## Serialized parent prerequisite

Parent owns `signatures.rs`; this worker did not modify it.
`TypeParameter::written_constraint` currently stores `Option<String>`.
`type_parameter_of` uses `written_annotation_text` plus a tuple-reference
fallback. `signature_to_string_at_worker` and the site-free signature renderer
copy that string directly. They neither retain a constraint node/type pair nor
run the existing-node visitor at the eventual print site.

Required parent contract: retain the written node and its original semantic
identity, migrate all `TypeParameter` constructors/instantiations/printers, and
apply the strict native constraint gate before site-aware visitor rendering.
Do not extend the annotation error-charity gate to constraints. Do not add an
unused renderer or relax a syntax heuristic in this worker: neither routes the
actual top-level constraint through the native operation.

### Ownership and work boundaries for that cutover

- **Native operation/consumer:** the helper and declaration builder above;
  consumed while signatures' type parameters are serialized.
- **Identity/owner:** the written constraint node and the constraint's semantic
  `TypeId`, belonging to the same Program/Checker stores. Printed text is not
  identity. Instantiated constraints must be compared with the original node's
  type, not accepted merely because a string was copied with the signature.
- **Publication:** no new cache is justified. A retained node/type pair is a
  reuse candidate, not a completed print or successful access result. The
  visitor completes success or refuses at the consumer's site; active expansion
  must retain the native fresh-serialization path. A provisional constraint
  resolution must not be published as completed reuse.
- **Context:** preserve print-site alias accessibility and type-parameter naming;
  mapper images must not reuse a moved constraint by accident. The existing
  visitor already tracks alias frames and reference-site name identity.
  `WrittenAnnotation`'s annotation charity is not the constraint identity rule.
- **Expensive work:** retain IDs during signature construction; perform the
  existing annotation-subtree walk only when the constraint is printed. Existing
  node-cached semantic resolution and site renderer handle refused subnodes.
  No duplicate cache or rendered-string side table is proposed. Expansion-state
  and type-parameter-renaming equivalence remain parent prerequisites, not
  measured optimization claims.

## Fresh whole-case and legacy receipts

Ran unfiltered `verdictdump`, `diagverdictdump`, and `casedelta` from the source
above. Also ran both actual `Suite::judge` implementations over every discovered
case in a throwaway executable, preserving skipped and unsupported outcomes.
That whole-case oracle is not a filtered line tally.

| Surface | Fresh result |
| --- | --- |
| Discovered whole-case oracle IDs | 12,444 per suite |
| Types whole-case oracle | 8,054 RIGHT; 1,484 WRONG; 2,906 SKIPPED |
| Diagnostics whole-case oracle | 4,224 RIGHT; 1,278 WRONG; 6,942 SKIPPED |
| Judged types cases | 8,054 / 9,538 = 84.44% |
| Judged diagnostics cases | 4,224 / 5,502 = 76.77% |
| Types positional matched / expected | 469,839 / 478,855 |
| Types aligned legacy verdicts | 469,839 RIGHT; 7,175 WRONG; 964 GAP |
| Diagnostics legacy verdicts | 4,224 RIGHT; 4,968 EMPTY_RIGHT; 1,278 WRONG; 100 EMPTY_WRONG |

All 16 named cases remain whole-case WRONG. Their aligned lines are below;
these include other unresolved operations in the same cases, not a prediction
that constraint reuse alone converts every line.

| Case | RIGHT | WRONG | GAP |
| --- | ---: | ---: | ---: |
| compiler/cannotIndexGenericWritingError | 21 | 1 | 0 |
| compiler/circularContextualReturnType | 7 | 4 | 0 |
| compiler/contextualSignatureInObjectFreeze | 5 | 2 | 0 |
| compiler/correlatedUnions | 452 | 19 | 14 |
| compiler/declFileRestParametersOfFunctionAndFunctionType | 15 | 2 | 0 |
| compiler/divideAndConquerIntersections | 83 | 5 | 2 |
| compiler/genericFunctionsAndConditionalInference | 77 | 9 | 0 |
| compiler/inlinedAliasAssignableToConstraintSameAsAlias | 10 | 1 | 0 |
| compiler/mappedTypeIndexedAccessConstraint | 105 | 50 | 0 |
| compiler/objectFreeze | 44 | 24 | 0 |
| compiler/objectFreezeLiteralsDontWiden | 3 | 16 | 0 |
| compiler/objectFromEntries | 62 | 15 | 0 |
| compiler/styledComponentsInstantiaionLimitNotReached | 118 | 3 | 0 |
| compiler/typeParameterConstraints1 | 24 | 2 | 0 |
| conformance/noUncheckedIndexedAccess | 256 | 8 | 0 |
| conformance/spreadObjectOrFalsy | 46 | 1 | 0 |
| **Total** | **1,512** | **162** | **16** |

Repeated both legacy dumps unfiltered on the unchanged implementation:
**0 verdict changes, 0 RIGHT/EMPTY_RIGHT losses, 0 missing IDs, 0 new IDs**.
The aligned types denominator is 477,978. Some raw printed type strings contain
newlines/tabs; row parsing recognizes actual verdict columns rather than
mistaking continuation text for extra IDs. No source/binary changes occurred
between the paired runs.

## Native performance receipt: release gate not met

Ran `scripts/whole_project_perf.py` against the pinned native binary, nine
fresh-process samples per tool plus the harness warmups, after builds and corpus
processes completed. Both project scopes were unchanged; no reduced fixture.

| Project | Observed median wall TSR/tsgo | Scope/options/diagnostics match | Verified wall ratio |
| --- | ---: | --- | --- |
| benches/projects/domain-model | 0.996843 | true / true / true | null |
| benches/projects/generic-imports | 0.943428 | true / true / true | null |

Both reports retain `complete_input_equivalence_verified: false`,
`actual_checked_work_verified: false`, `work_comparable: false`, and
`target_verified: false`. These are observed CLI timings, **not verified
complete-equivalent-work performance**. The requested verified <=0.50 release
gate and 99.9% parity are not established. The parent must retain these missing
measurement prerequisites alongside the shared constraint-dispatch prerequisite
in Beads; this investigation does not close `tsr-2zk.16.69`.
