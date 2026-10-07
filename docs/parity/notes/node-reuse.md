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

## Existing-name partial query recovery continuation (`tsr-2zk.16.75`)

Fresh filtered whole-target assertion run includes all 11 named cases:
**653 RIGHT, 83 WRONG, 5 GAP** across 741 aligned lines. All 11 remain blocked;
the 88 non-RIGHT lines include other unresolved operations and are not a
conversion forecast for this root.

The native `declarationEmitPartialNodeReuseTypeOf` case depends on module import
routes, return-object written annotation retention, and private-value fallback.
Those return/alias producers remain parent-owned. An actual namespace control
isolates the visitor's fallback without fabricating module imports:

```typescript
namespace Values {
    export const imported = "imported";
    export const other = "other";
    const privateValue = "private";
    export function source(): { foo: typeof imported; bar: typeof privateValue; baz: typeof other } { throw 0; }
}
const result: never = Values.source;
function shadow() { const imported = 1; const other = false; const result: never = Values.source; }
```

Native TS2322 at `(7,7)` and `(8,68)` serializes the return object as
`{ foo: "imported"; bar: "private"; baz: "other"; }`; TSR diagnostics retain
`typeof imported`, `typeof privateValue`, and `typeof other`. The actual TSR
corpus pipeline's **site-aware** `Values.source` references outside the namespace
correctly print the three literal fallbacks, while the declaration inside the
namespace preserves written `typeof` nodes. Thus the site-aware visitor's
refusal/fallback is runnable and correct here; the diagnostic caller still
selects a different context. Unconditionally expanding all `typeof` nodes would
lose the declaration's written view.

Required parent contract remains explicit diagnostic versus site-aware builder
context, with dynamic signature scope where applicable. Native
`tryVisitTypeQuery` (`nodecopy.go:400`) first tracks the source entity, then
serializes its name on failure; recovery can serialize the actual type when no
name is available. Export/import route naming is delegated to the shared
symbol namer rather than recreated by this worker. No production visitor edit
is justified by this control, and no import-route fallback or text-equivalence
cache was added.

Seven owned tests pass. Native/TSR CLI hashes still match the pinned full
receipts; full zero-RIGHT/EMPTY_RIGHT-loss/no-missing-ID pair and performance
verification remain unchanged. Both claimed clusters remain unresolved on
shared contracts; this investigation does not report a completed fix.

## Predicate qualification continuation (`tsr-2zk.16.75`)

Actual target smoke for `formatToPartsFractionalSecond` remains
**13 RIGHT, 4 WRONG, 0 GAP**. Actual native/TSR CLIs agree diagnostically on
the native source. Its type disagreements include one nested predicate
parameter using `DateTimeFormatPart` instead of `Intl.DateTimeFormatPart`, plus
registry type qualification. These require the target's library signature
instantiation/display context; diagnostics alone do not prove type parity.

An independent actual control defines a namespace `Types.Part`, a generic
source accepting `(value: Part) => value is S`, and a non-predicate counterpart.
At external and function-local-shadowed corpus sites, both nested forms
correctly qualify `Types.Part`; actual native/TSR CLI diagnostics agree. No
owned visitor accessibility defect is reproduced by this control. A direct
`guard(value: Part): value is Part` uses the parent-owned
`signature_to_string_at` predicate early return and its baked signature; that
path is not an entity-name visitor change this worker may make.

The remaining exact target prerequisite is parent library-signature
instantiation/dispatch context and registry type naming, not a guessed global
predicate accessibility relaxation. No production edit or test asserting an
unsupported empty result was added. Seven owned tests pass. Source/binary
identities and prior full no-loss/missing-ID/performance receipts remain
unchanged; `.75` still has 11 blocked targets and no conversion claim.

## Written mapper/source-identity continuation (`tsr-2zk.16.75`)

Actual native/TSR CLI and corpus controls exercised the owned annotation gate's
alias-frame and source-identity boundary, not printed-text equivalence:

```typescript
type Box<T> = { value: T };
type Holder<T> = { fn: (value: Box<T>) => Box<T>; mixed: (value: T | string) => T | string };
declare const original: Holder<number>;
const result: never = original.fn;
const mixed: never = original.mixed;
type Same<T> = { fn: <T>(value: Box<T>) => Box<T> };
declare const same: Same<number>;
const sameResult: never = same.fn;
```

Both actual CLIs agree; the real corpus pipeline renders the moved reference
as `Box<number>`, the moved union as `string | number`, and the distinct nested
signature's own `T` unchanged. Another actual control with
`Query<T> = { fn: (value: T) => typeof value; nested: <U extends T>(value: U) => U }`
instantiated at `number` prints `(value: number) => number` and
`<U extends number>(value: U) => U` in both CLIs and the corpus. A conditional
alias branch with `(value: T[]) => T[]` likewise produces mapped `number[]`.

These controls exercise `reuse_annotation`, its alias-frame refusal, and
existing signature substitution feeding the visitor. No stale written mapper
reference or owned production failure was demonstrated. A blanket fresh-TypeId
refusal or syntax-based alias guard is not justified. No new cache, shared
metadata change, production helper, or duplicate passing-path regression was
added. Seven owned tests pass; unchanged compiler identities retain the full
no-loss/missing-ID and unverified native performance receipts. Parent context
contracts identified above remain unresolved; no new cluster conversion is
claimed.

## JSDoc annotation queue investigation (`tsr-2zk.16.147`)

Scoped the next existing queue items before claiming work. Import-type `typeof`
(`tsr-2zk.16.73`) fails in parent-reserved semantic import production before
node reuse: an actual split-file corpus control renders its source signature
and imported class references as `error`. Intersection-member printing and
namespace spread items likewise name reserved checker/spread producers. None
was claimed as an owned visitor fix.

Claimed the JSDoc annotation-reuse item `.147`, whose nullable/optional visitor
arms live in `node_reuse.rs`. Fresh five-target dump:
**101 RIGHT, 18 WRONG, 6 GAP** across 125 aligned lines. The repeated nullable
wrapper target expects written
`(((((number | null) | null) | null) | null) | null) | undefined`; TSR prints
normalized `number | null | undefined`. Other wrong lines require preserved
`Array<any>` or explicit optional `undefined`; some unrelated producer gaps
remain visible rather than silently excluded.

An actual JavaScript control with `@param {???!?number?=}` and
`@param {Array<number>}` assigned to JSDoc `never` variables produces identical
native/TSR diagnostics: TS2322 at `(4,7)` prints normalized
`(a?: number | null | undefined) => void`; at `(8,7)` it prints
`(values: number[]) => void`. The actual split-file corpus pipeline also
normalizes those annotations in TSR. This distinguishes semantic diagnostic
serialization from the type-baseline's written-node reuse path; changing
semantic types to duplicate null constituents would be wrong.

Pinned JSDoc nullable/optional visitor arms (`nodecopy.go:487` and `:494`) build
union nodes without flattening written wrappers. The owned port already emits
those nested structures. The parent-owned parameter annotation channel must
retain the reparsed JSDoc type node and original identity before that visitor
can preserve it. No parser, signature metadata, source/error intrinsic, or
visibility edits were made. Seven existing owned tests pass; no failing-path
exception or fake import-type completion was added. Production binary remains
unchanged, retaining full no-loss/missing-ID and unverified performance
receipts. `.147` remains unresolved on parent annotation retention.

## Ambient declaration visibility continuation (`tsr-2zk.16.75`)

Compared owned `is_declaration_visible` with pinned
`determineIfDeclarationIsVisible` (`emitresolver.go:131`), including its ambient
module-element exception and parent-visibility dependency. Actual native/TSR
CLI controls agree for an ambient namespace with unexported `Part` and `source`
declarations and a nonambient namespace with a private `Part` plus exported
`source`.

The actual corpus pipeline renders the ambient source's returned function as
`(value: Ambient.Part) => Ambient.Part` outside the namespace and under a local
`type Part = number` shadow. The nonambient private type keeps its declaration
view. This confirms the ambient-member and source/site identity path without
assuming every unexported declaration is inaccessible or making visibility
unconditional.

No production discrepancy was demonstrated, so no visibility policy, demand
side table, or shared writer contract changed. Seven existing owned tests pass.
Compiler source/binary and prior full loss/ID/performance receipts remain
unchanged. The measured open items still require parent annotation retention,
diagnostic/site-aware dispatch, and active dynamic signature scope; this
control does not establish cluster completion or a speed improvement.

## Existing JS reference compatibility continuation (`tsr-2zk.16.147`)

Read pinned `canReuseExistingJSTypeNode` (`nodebuilderimpl.go:501`) and its
compatibility worker (`nodebuilderimpl.go:518`). Native first excludes intended
JSDoc remappings. The argument-count check then applies only when the semantic
type is an object reference and the written reference's declared target equals
that semantic reference's target. A spelling or generic parameter count alone
does not prove that identity. The owned approximation checks required arguments
from the resolved source symbol, but extending it requires a demonstrated
same-target semantic reference contract, not a syntax guess.

Actual JavaScript native/TSR control defines generic JSDoc `Box<T>`, an omitted
`Box` parameter, and explicit `Box<number>`. Both TS2322 messages agree at
`(7,7)` and `(9,7)`: `(value: any) => void` and
`(value: Box<number>) => void`. Native additionally emits TS2314 at `(2,13)`:
`Generic type 'Box' requires 1 type argument(s).` TSR omits it. Actual corpus
rendering agrees on these parameter types. The missing diagnostic is semantic
reference checking, reserved to the parent, not a visitor reuse decision.

No reference-target cache, text comparison, blanket argument-count relaxation,
or shared producer edit was added. Seven tests pass. The `.147` five-target
retention failures and parent prerequisites remain; this control does not
claim a production fix. Full no-loss/missing-ID and unverified performance
receipts remain attributable to the unchanged compiler.

## Nullable node precedence continuation (`tsr-2zk.16.147`)

Compared owned JSDoc nullable/optional text emission with pinned AST union
construction (`nodecopy.go:487`, `:494`). Actual native/TSR CLI controls and
corpus pipeline agree for `?string[]` versus `(?string)[]`: the former prints
`string[] | null`, the latter `(string | null)[]`, including both parameter and
return positions of a returned function signature. This checks the owned
precedence transformation independently of parent JSDoc retention.

A separate `string=` TypeScript probe was syntactically invalid and produced
differing parser recovery diagnostics. It supplies no visitor evidence; no
parser edits or empty-output expectations were introduced. No nullable
production mismatch was demonstrated, so the visitor remains unchanged.
The measured failing `.147` targets still require reparsed annotation retention
at parent construction, not an inferred precedence workaround. Existing seven
tests and prior source-bound full loss/ID/performance receipts remain unchanged;
no conversion or accepted speed improvement is claimed.

## Computed property-name continuation (`tsr-2zk.16.75`)

Scoped `reused_property_name`, which prints computed entity names without a
`ReuseContext`, against pinned computed-name handling (`nodecopy.go:648`,
`:751`). Actual namespace and global unique-symbol controls define a source
returning `{ [key]: string }`, then reference it externally and under a local
`const key = 1` shadow. Native/TSR CLIs agree with `--target es2015 --noEmit
--pretty false`; both preserve `[key]` in diagnostics. The actual corpus
pipeline also preserves `[key]` in declaration and reference views.

These controls do not prove a missing qualification defect: native's
non-late-bindable-name handling and recovery can retain the written computed
name. Changing all computed entity names to site-qualified names solely because
the owned helper lacks a context argument would guess behavior rather than
port the observed operation. No computed-name rewrite, shared identity field,
visibility relaxation, or new cache was introduced. Production source remains
unchanged, preserving the prior full zero-loss/missing-ID and unverified
performance receipts. Existing shared-context prerequisites remain open; no
new target conversion is claimed.

## Failing nested-nullable diagnostic control (`tsr-2zk.16.147`)

Actual native/TSR CLIs on `declare function source(): ??number` and a returned
function with `??number` parameter/return annotations emit the same TS17020
syntax diagnostics at `(1,28)`, `(1,29)`, `(3,36)`, `(3,37)`, `(3,49)`,
`(3,50)`. Native TS2322 at `(2,7)` and `(4,7)` normalizes the displayed nullable
type to `number | null`; TSR incorrectly reuses `number | null | null`.
The actual corpus pipeline reproduces duplicated nullable spelling while the
standalone parameter's semantic type is `number | null`.

This is a failing runtime control, not proof that changing owned
`node_precedence` fixes it. Parent builder context/annotation eligibility must
select semantic diagnostic serialization rather than written syntax here.
Parenthesizing the nested wrapper would preserve duplicated syntax and remain
incorrect. No symptom-only precedence change, diagnostic suppression, or
unsupported empty expectation was added. Seven tests pass and the CLI hash
remains identical to baseline. Full prior loss/ID/performance receipts remain
unchanged; unresolved context prerequisites stay recorded in Beads.

## Literal cloning versus diagnostic serialization continuation

Actual native/TSR controls contain a written single-quoted union with emoji,
apostrophe/double-quote escapes, backslashes and newline, plus a template
literal type containing an escaped backtick and literal substitution.
Native TS2322 at `(2,7)` sorts/normalizes the union with double quotes;
at `(4,7)` it collapses the template to its literal string. TSR diagnostics
retain source single quotes and template syntax. The actual corpus pipeline
preserves those written forms but prints standalone parameter semantic types
normalized, confirming another diagnostic-context distinction.

Pinned string cloning (`nodecopy.go:811`) preserves source quote flags and adds
`EFNoAsciiEscaping`. Those written-node rules were not disproven by the failing
diagnostic control. Changing owned `quoted_literal` or template escaping would
harm legitimate reuse while leaving parent semantic serialization selection
incorrect. No literal-specific exception, quote workaround, or production edit
was added. Seven tests pass. Full no-loss/missing-ID and unverified performance
receipts remain attributable to unchanged compiler binaries. Remaining scoped
failures are shared-context prerequisites; more matching clone controls do not
make those reserved contracts implemented.

## Missing `typeof b` parameter contract coordination

Investigated parent-reported `objectTypesIdentityWithCallSignatures3` parameter
annotation fallback without editing the parent builder. On this Box's unchanged
source, actual target smoke is **52 RIGHT, 0 WRONG, 0 GAP**. Actual native/TSR
CLI outputs agree, including TS2304 for unresolved `b` at `(23,25)` and
`(24,25)`. This does not certify the parent's intervening source state.

Pinned writer is `PseudoChecker.GetTypeOfDeclaration`
(`internal/pseudochecker/lookup.go`): a present annotation becomes
`PseudoTypeDirect`, retaining the actual node rather than rendered text.
`serializeTypeForDeclaration` (`nodebuilderimpl.go:2181`) uses that candidate
with `pseudoTypeEquivalentToType` (`pseudotypenodebuilder.go:362`), whose
error-type charity can permit the written unresolved `typeof b` annotation.
The result then goes through `pseudoTypeToNodeWithCheckerFallback`; unsupported
reuse falls back to semantic serialization. This is not the strict constraint
identity rule of `typeToTypeNodeHelperWithPossibleReusableTypeNode`.

Required parent parameter-builder contract: retain the source annotation node
and its actual semantic/error identity; do not replace the missing query's
candidate merely because the parameter's displayed fallback is `any`. The
owned `reuse_annotation` can carry that identity and visitor entry can consume
it, but the parent controls selecting and publishing the parameter candidate.
No guessed `getExistingAnnotation` API was introduced: the pinned operation's
actual writer and consumers are named above. No parent alias, query/error
builder, signature, or scope fields changed. Seven owned tests pass; unchanged
compiler retains prior full no-loss/missing-ID and unverified performance
receipts. The reported parent prerequisite is coordinated, not silently
claimed fixed by the Box's already-RIGHT target.

## Cross-module mixed private-alias recovery continuation

Actual `declarationEmitAliasInlineing` target smoke remains
**25 RIGHT, 3 WRONG, 0 GAP**. Its external-module private aliases require
per-subnode inlining inside indexed access, `Omit`, and `keyof` at the consuming
module's type-baseline print site. A namespace-private counterpart preserves
written aliases in both native/TSR CLI diagnostics and corpus output; it is not
a valid substitute for the module accessibility context.

Built an actual two-file CLI control with private `ObjectShape` and `IndexShape`
in `a.ts`, exported `source`, and an imported source assigned to `never` in
`b.ts`. Native/TSR `--strict --noEmit --pretty false` outputs agree. This
confirms that the remaining target discrepancy is the baseline's print-site
accessibility/reuse policy, not a general semantic alias failure. Parent module
host and alias/reference display context must provide that site to the visitor;
those contracts are reserved. Unconditionally expanding namespace/private
spellings would break the matching declaration view.

No printed-name identity heuristic, module-specific exception, parent metadata
edit, or production change was added. `.75` remains unresolved; prior full
no-loss/missing-ID and unverified performance receipts remain unchanged.
The next implementation requires the shared print-site contract, not further
matching namespace controls.

## Import-type attributes reuse continuation

Inspected the owned `ImportTypeNode` refusal for attributes against pinned
`nodecopy.go:614`, which visits attributes and rewrites legacy assertion syntax.
An actual ambient-module control with `import("pkg", { with: {
"resolution-mode": "import" } }).Shape` exposes semantic prerequisites before
attribute cloning can be assessed as a fix.

Under `--module nodenext`, native emits TS2664 for the augmentation and TS2307
for both import references, then TS2322 with `any`; TSR accepts the augmentation
and prints `import("pkg").Shape`. Under `--module esnext`, native TS2322 at
`(3,7)` prints `() => (value: Shape) => Shape`; TSR prints the qualified import
form. Merely emitting the attributes would not implement either native
semantic resolution or diagnostic type naming. Actual corpus output reproduces
TSR's attribute-free import form.

The existing import-type issue `.32` names parent-reserved semantic import
production and is already claimed elsewhere; `.73` likewise concerns the
`typeof` semantic arm. Neither was claimed as a completed visitor slice.
No attributes serializer, guessed module route, unsupported fallback, or
shared producer change was added. Production remains unchanged; prior full
loss/ID and unverified performance receipts remain applicable. Parent semantic
resolution/context is the reachable prerequisite before extending this owned
visitor arm.

## Implemented emitted-node precedence correction (`tsr-2zk.16.147`)

An actual native/TSR control with `?string & Tag` exposed an owned grouping
error: TSR's reused node printed `string | null & Tag`, changing the source
AST's meaning. Native diagnostics additionally normalize the intersection to
`string & Tag`; that parent semantic diagnostic policy is intentionally not
changed here.

Pinned nodecopy replaces a nullable/optional wrapper with an ordinary union,
replaces variadic wrappers with arrays, and unwraps nonnullable/type-expression
nodes. Native `Printer.emitTypeNode` (`printer.go:2274`) computes precedence
from the **resulting node**, not its original JSDoc wrapper. Owned
`emit_reused_type` now applies the rewritten node's precedence to successful
visitor output. Existing normal-node and semantic-fallback rules remain intact.
Actual corpus smoke now prints `(string | null) & Tag` in both parameter and
return positions. A retained regression catches the old grouping error;
**8 release tests pass**, no ignored/filtered tests. Owned formatting checks
pass. No cache, traversal, identity guard, shared field, or visibility policy
was added; the result-kind check is constant work per already-visited node.

Fresh unfiltered candidate runs:

| Surface | RIGHT | WRONG | GAP | Protected losses | Missing/new IDs | Changes |
| --- | ---: | ---: | ---: | ---: | --- | ---: |
| Types | 469,839 | 7,175 | 964 | 0 | 0 / 0 | 0 |
| Diagnostics | 4,224 | 1,278 | — | 0 | 0 / 0 | 0 |

Diagnostics also retain 4,968 EMPTY_RIGHT and 100 EMPTY_WRONG. This corrects
an emitted grouping bug but claims **no whole-case conversion**. The earlier
parent context/retention prerequisites remain unresolved.

Ran fresh nine-sample native CLI timing after corpus/build processes finished:
domain-model observed wall ratio **1.036902**, generic-imports **0.957450**.
Both report matching scope/options/diagnostics, `verified_wall_ratio: null`,
`target_verified: false`, with complete-input and actual-work verification
still false. These are not equivalent-complete-work performance certification
or an accepted speed win; the <=0.50 release goal remains unmet.

## Post-fix unwrapped-node control continuation

Actual native/TSR controls after the emitted-node precedence fix validate
unwrapped nonnullable nodes: `!?string & Tag` retains correct grouping as
`(string | null) & Tag`, while `!(string | number) & Tag` agrees with native
at `(5,7)` as `(string | number) & Tag` in both parameter and return positions.
Native additionally normalizes the nullable intersection at `(3,7)` to
`string & Tag`; parent semantic diagnostic policy remains unchanged. The
actual corpus pipeline confirms the grouping and standalone semantic types.

A TypeScript variadic-JSDoc syntax probe was invalid and produced parser
recovery differences; it supplied no visitor evidence and no parser or
unsupported-completion changes were made. Eight owned regression tests pass.
No additional production change was justified. The fresh full candidate
zero-protected-loss/missing-ID pair and native timing observations from the
implemented correction remain applicable; verification remains false, not an
accepted <=0.50 release result.

## Merged source-symbol identity continuation (`tsr-2zk.16.75`)

Actual native/TSR CLI controls merge two `Shared` namespace declarations and
`Part` interfaces, then reference a written returned-function annotation from
an external site and a consumer containing an unrelated nested `Shared`.
The real corpus pipeline qualifies the original merged symbol as `Shared.Part`
and uses `globalThis.Shared.Part` under the unrelated namespace shadow.
Native/TSR CLI diagnostics agree. This exercises merged/export symbol identity
rather than equating equal namespace spellings.

Owned `track_existing_entity_name` normalizes resolved aliases, export symbols,
and merged symbols before comparing source/site identity. No production defect
was demonstrated by this control, so no scope/name/alias shared-field change
was made. Eight tests pass. Actual four-case `.75` smoke remains
**152 RIGHT, 24 WRONG, 0 GAP** across 176 aligned lines; those known failures
retain the parent context prerequisites above.

Current corrected CLI SHA-256:
`178ae0f763cd0fcce076fa42c2ecfbb1f8f77e62b7e08748577386e415d03a32`.
Native hash remains the pinned value above. The fresh full no-protected-loss/
missing-ID pair and performance observations recorded for the emitted-node
precedence correction apply to this unchanged implementation. No extra
conversion or performance certification is claimed.

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
