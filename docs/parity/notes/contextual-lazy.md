# Contextual recovery — tsr-2zk.16.425

## Recovered native operations

Recovery base: `0e7824ddb2f06af1dd4cbbe9779e0a17b720a0f3`.
Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Inspected recovery source `box/parity-contextual` through `754673bc`;
no branch merge or snapshot cherry-pick. Owned implementation corresponds to
`c4b69044`, `2f3038f3`, `70f524fc`, `5c801fff`, and `8126dbbd`.
The array-rest change first appeared in unverified `ab1cd894`; this recovery
re-read native `getContextualTypeForBindingElement` and reproduced its failed
controls before implementing it. Historical receipts are not current passes.

- `getContextualTypeForBindingElement`: written holder annotation precedes
  recursive binding-holder or contextually typed parameter projection. Native
  array projection applies even to invalid initialized rest bindings. Defaults
  do not remove undefined from the projected context.
- `getContextualType` / `getContextualTypeForInitializerExpression`: parentheses
  preserve the declaration's implied binding-pattern context; written
  annotations retain precedence.
- `getContextualReturnType` / `getReturnTypeFromAnnotation`: getter annotation
  precedes paired setter effective annotation. Existing raw JSDoc return and
  setter-parameter metadata supplies native reparsed annotations. This lookup
  does not resolve the accessor's type or check its getter body.

### Ownership and work contract

Holder/function `NodeId`, annotation AST nodes, contextual parameter signatures,
`TypeId`, and JSDoc host metadata belong to the same private Checker and its
binder/node store. Strict-null options remain Checker-local. No new cache,
mapper, table, structural member image, or publication state is introduced.
Binding-holder and parentheses traversals follow AST parents; paired setters
are read from the bound accessor symbol's declaration list. Annotation reads
leave completion to the existing type-node/parameter/accessor producers;
missing raw context remains missing, never a completed `any` assumption.
Receiver and written alias context remain those of the actual annotation and
property projection. Expensive work remains existing type-node and contextual
parameter resolution; no duplicated accessor/body work or optimization claim.
Attribution remains the existing `tsr-1yb.11` boundary.

Unannotated holder initializer fallback still requires the inference owner's
explicit-context declaration-checking scope. Do not recursively infer a holder
from the same default initializer. Canonical accessor JSDoc symbol publication
remains the symbols owner's operation; correct contextual getter expressions
alone do not establish the setter's exported contract.

## Current verification

Durable ignored receipts: `target/recovery/contextual/` (not `/tmp`).

Before controls: callback/rest parameters were `any` rather than native
`number`; parentheses produced required rather than optional implied property;
four getter/JSDoc literal controls widened to `string`. Nine dedicated controls
now pass, including annotation-precedence and unannotated-getter controls.
Actual pinned-native declaration emit and TSR CLI positive controls complete
without errors. Invalid rest control produces native TS2322 at `(3,9)` in TSR;
native also produces TS1186 at `(3,14)`, which TSR still lacks. That forbidden
parser/diagnostic consumer is not repaired or suppressed here. The spurious
getter-return TS2322 observed in the baseline CLI is gone.

Unfiltered current type dump: **469785 -> 469801 RIGHT / 477970**, **+16**.
All **469785** prior RIGHT keys remain present and RIGHT. Diagnostic dump:
**9190** prior RIGHT/EMPTY_RIGHT keys, zero vanished keys, zero losses, zero
gains. Both diagnostic dumps contain 10570 rows. Current recovery-base numbers
differ from saved history; no historical baseline was substituted.

Gains: two `classExpressionNames`, six `contextualTypeFromJSDoc`, two
`instantiateTemplateTagTypeParameterOnVariableStatement`, six
`objectLiteralGettersAndSetters` rows. The first target converts completely;
other targets remain incomplete.

Workspace release tests, all-target workspace clippy with `-D warnings`, and
format check pass. All sixteen corpus suites were exercised in an ignored
source mirror to avoid touching protected snapshots. Corpus discovery: 12444
cases. Current checker case parity **8052/9538 (84.42%)**, type-line parity
**98.11%**; diagnostics **4222/5502 (76.74%)**. Other suite results are in
`all-suites.log`; running the suites is not a claim of complete parity.
The source mirror's printed upstream label is its copied root's git fallback
`0e7824dd`, not the oracle pin; the real vendor pin was independently checked.

Fresh-process interleaved measurements, two warmups, 21 baseline pairs and
11 pinned-native pairs, existing whole-project harness:

| Project | candidate/base wall | candidate/base CPU | TSR/native wall |
| --- | ---: | ---: | ---: |
| domain-model | 1.00347 | 1.01196 | 0.99814 |
| generic-imports | 1.00257 | 1.00720 | 0.95570 |

Matched observed options, loaded scope and diagnostics; observed inputs stable.
No observed baseline regression above 3%. Complete-input and actual-checked-work
proof remain false; verified ratio is null. **Neither 99.9% exact parity nor
verified complete-work wall <=0.50 is achieved.** These observations are not
release acceptance or proof of every hotpath's behavior.
Candidate CLI SHA-256:
`0579777d0991709cf9dc94baf76fafa4f097bcb33d95bc7120b344e8fe6efc26`.

## Serialized JSX contextual-stack prerequisite

No JSX file or shared `checker.rs` edit is included. The remote worker has no
cross-worker messaging tool. The initial task names recover-calls' requested
provider but does not include its agreed API. Exact minimal integration request:

```rust
// Checker field; initializer Vec::new()
pub(crate) contextual_type_stack: Vec<crate::contextual::ContextualTypeFrame>;
```

Owned API to connect atomically with that field and real calls/JSX consumers:
`push_contextual_type(node, Option<TypeId>, is_cache)`, `pop_contextual_type()`,
and `stacked_contextual_type(node, include_caches) -> Option<Option<TypeId>>`.
Native `contextualInfos`/`pushContextualType`/`findContextualNode` scan
**oldest-first**. Explicit nil must stop fallback; absent frame must not.
Cached frames are excluded for nonzero `ContextFlags`, not automatically for
`CheckModeSkipContextSensitive`. Native context-free expression checking pushes
explicit `any` and retains its skip-context-sensitive check mode.

Native `getContextNode` maps non-self-closing JSX attributes to the containing
root JSX element so attributes and semantic children share candidate inference
context. The inference/calls owner must pair candidate props with that same
root's existing active inference context, preserve check modes and scope both
stacks around the real attribute/children check on every return path. A
self-closing attribute context stays on the attributes node. No JSX heuristic,
extra contextual-signature resolution, or premature completed signature.

Stack key/type identities and alias/receiver context remain private-Checker
local. A frame is active/provisional dynamic context, not completed expression
checking or inference; popping ends its lifetime. Cache frames capture an
already-queried result including nil and cannot publish recursive success.
Expensive checking/inference remains consumer-owned; lookup is only a small
linear stack scan. No second semantic cache is justified.

An exact minimal proposed hunk is retained in ignored
`contextual-stack-integration.patch`, not installed as dead scaffold or presented
as verified implementation. Connecting it requires the serialized field/API
agreement and JSX/calls-owner cutover; those files are forbidden to this worker.
The existing issue remains open: `bd prime` ran, but `bd show` and history cannot
find `tsr-2zk.16.425` in this Box's local database. No duplicate issue created.
Integration owner must update the authoritative existing issue.
