# Lane notes: r4-index3 (tsr-2zk.951)

Judgment calls made by the `r4-index3` parity box, continuing
[r4-index](r4-index.md) and [r4-index2](r4-index2.md) on the two index
causes [r4-realworld2](r4-realworld2.md) found on TypeScript's own sources
(N1 `tsr-2zk.945`, N2 `tsr-2zk.946`). Baseline frozen at `56cec1e`
(integration head, which already carries r4-realworld2's repros). Native
behaviour is a `tsgo` built from the pinned submodule (`5b1047d`) by
`scripts/offline-cargo/build-tsgo.sh`.

Real-world numbers use r4-realworld2's method (an untracked
`tsconfig.scratch.json` per project, `types: []`, keys
`(file, line, column, code)`):

| | base `56cec1e` | after §1 | after §1+§2 | native |
|---|---|---|---|---|
| `src/compiler` TSR diagnostics | 428 | 407 | 404 | 86 |
| `src/compiler` TS7053 | 24 | 3 | 0 | 0 |
| `src/services` TSR diagnostics | 750 | 722 | 719 | 260 |

Every removed key is TSR-only (none is in native's output); nothing was
added. On `services` the 28 removed keys are 27 TS7053 (the compiler's 21
plus the 6 of `transpile.ts`) and one TS2339, `documentRegistry.ts:222`,
explained in §1. The conformance corpus does not exercise the shape: both
dumps are verdict-for-verdict identical to the baseline after §1 and
after §2.

## 1. An index signature whose value is `errorType` still declares its key (N1)

**Forcing constraint.** `getIndexInfosOfIndexSymbol` (`checker.go:19634`)
builds an `IndexInfo` for each valid key of a one-parameter signature with
`valueType := getTypeFromTypeNode(returnTypeNode)`, whatever that answered
(`:19650-19657`). The port's `index_infos_of_declaration`
(`index_signatures.rs`) returned no info when the value was `errorType`.

That is not a gap marker but a confident wrong answer: "this interface has
no string index". `CompilerOptions`/`OptionsBase` declare
`[option: string]: CompilerOptionsValue | TsConfigSourceFile | undefined`,
which cause 1 (`tsr-2zk.16.46`, the union origin gate) turns into
`errorType`. With the info dropped,

- `report_implicit_any_element_access` found no applicable info and
  reported TS7053 on every `options[name]` (21 in `src/compiler`, 6 in
  `services/transpile.ts`). The arm already declines an error-valued info
  (`infos.iter().any(.. is_error(info.value))`); it never saw one;
- a property read on `CompilerOptions | MinimalResolutionCacheHost`
  (`documentRegistry.ts:222`, `settingsOrHost.getCompilationSettings`)
  missed the union's index fallback and reported TS2339.

**Alternative rejected.** Teaching the TS7053 arm to re-read the signature
declarations would be a side pass re-deriving what `get_index_infos_of_type`
should have answered (box protocol §3a). The arm's question — "does an
applicable index info exist" — is already the native one; the producer was
wrong.

**Consequence.** An access through such an info now answers `errorType`
(silent, like native's `any`-printing error), where before it answered the
port's unresolved/error marker with a TS7053 on top. Fixing cause 1 makes
the value real; this change stays correct then.

**Falsifier.** A baseline case where a signature whose value type fails to
resolve still reports TS7053/TS2339 natively at an access through it.

## 2. An intersection's index infos read each constituent's apparent type (N2)

**Forcing constraint.** `getIndexInfosOfType(t)` =
`getIndexInfosOfStructuredType(getReducedApparentType(t))`. For an
intersection, `getApparentType` → `getApparentTypeOfIntersectionType`
(`checker.go:21796`) → `getTypeWithThisArgument(t, t, needApparentType)`,
which maps every non-reference constituent through `getApparentType`;
`resolveIntersectionTypeMembers` then appends their index infos. So
`string & { __pathBrand: any }` carries `String`'s `[index: number]: string`.
The port's intersection arm recursed on the raw `string`, which has no
infos, and `p[i]` reported TS7053 (`moduleNameResolver.ts:1255`,
`resolutionCache.ts:1619`, `sys.ts:712`); native reports nothing there, and
TS2322 `string` → `number` on the reduced repro.

**What was ported.** Only the per-constituent `apparent_type` in
`get_index_infos_of_type`'s intersection arm. `members.rs` `apparent_type`
still has no intersection arm (not owned); the index road no longer needs
one because it maps the constituents itself, which is the same composition
native performs. `getTypeWithThisArgument`'s `this` instantiation of a
reference constituent is not ported here; for index infos it matters only
for an index value that mentions `this`.

**Not changed.** The top level of `get_index_infos_of_type` still answers
no infos for a bare primitive (native: `String`'s), and the union arm does
not map constituents either. Both are reached by many callers (relation,
inference, property fallback) whose primitive handling is their own; neither
had a measured miss in this lane.

**Cost.** `apparent_type` on a non-primitive, non-instantiable constituent
is a flag test and returns the input; it runs only on intersection receivers.
Perf gate numbers are in the commit message.

**Falsifier.** A baseline case where a generic constituent `T & X` now
picks up `T`'s constraint's index signature and native does not.
