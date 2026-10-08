# Lane notes: r5-errorsplit2 (tsr-2zk.960, continuing bd tsr-2zk.944)

Single owner of the error-type contract, step 2: moving producers from the
port's gap and its `any` stand-ins to upstream's `errorType`
(`Intrinsics::native_error`). Step 1 and its instrument are
[r4-errorsplit](r4-errorsplit.md); the decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md),
superseding [ADR-0038](../../adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md).
Pinned upstream: `vendor/typescript-go` @ `5b1047d`.

**Correction to r4-errorsplit.** Step 1's notes and code cited the decision
record as "ADR-0047". That number went to r4-variants
(`0047-configuration-varied-cases-are-judged-per-configuration.md`) and no
error-split ADR was ever written under it. The citations in this lane's
owned files now read ADR-0048; `r4-errorsplit.md` and `unions.rs` still
carry the old number and are not this lane's to edit.

## §1 Baseline and method

Frozen at `ac56208` (origin/main after PR #6; this branch fast-forwarded to
it). Types 543,119 RIGHT / 1,113 GAP / 8,301 WRONG (552,533 aligned lines);
diagnostics 5,070 RIGHT / 5,551 EMPTY_RIGHT / 1,521 WRONG / 96 EMPTY_WRONG.
`ceiling` at the base: 4,644 matched gap lines (152 attributed), 0
`native_error` lines, 36 in ADR-0038's `any`-vs-`error` bucket.

**Native verification.** A `.types` line cannot say whether upstream's `any`
was `errorType` or `anyType`, so every switch below was checked against a
native build (`scripts/offline-cargo/build-tsgo.sh`) through a small probe
compiled into the vendored module with `go build -overlay` (nothing tracked
is touched). For every node it prints whether `GetTypeAtLocation` is
`GetErrorType()`, `GetAnyType()` or other, plus signature return types,
parameter types and type-parameter constraints. The probe's source is
reproduced in §6.

## §2 (a) `checkIdentifier`'s arms

Native answers, from the probe, each at `checker.go` @ `5b1047d`:

| arm (`expressions.rs`, `Expression::Identifier`) | port before | native | switched |
|---|---|---|---|
| §475: a name resolving only to a type parameter (`class C<T> extends T`, `return U`) | `any` | `errorType` (`:11048`, `unknownSymbol`) | **yes** |
| assignment to a non-variable (`import {v}…; v++`, `v = 2`) | gap | `errorType` (`:11094`) | **yes** |
| assignment to a readonly symbol (`const k = 1; k = 2`) | `any` | `errorType` (`:11102`) | **yes** |
| empty name (parse recovery, `yield*` with no operand) | `any` | `errorType` (`:11048`) | held: diff A |
| final `else`: absent in every meaning, no import machinery | `any` | `errorType` (`:11048`) | held: diff B |
| *new*: unresolved name under a JS `require(x)` call | gap or `any` | `anyType` (`requireSymbol`, `nameresolver.go:322-326`, `checker.go:16584`) | **ported** |

The `require` arm was found as a loss of the final-`else` switch:
`ensureNoCrashExportAssignmentDefineProperrtyPotentialMerge`, the
`commonJSImport*` family, `moduleExportAssignment3` and others record
`require : any`, and that `any` is upstream's `anyType`, not its `errorType`.
Ported on its own it converts `modulePreserve2:0:3` (GAP→RIGHT): a JS file
with import machinery used to gap there under the §31 gate.

**Measured, commit 1** (the three switched arms, the `require` arm, and the
writer guards in §3), against the frozen base: types 543,120 / 1,112 / 8,301
(+1 GAP→RIGHT), diagnostics unchanged; both loss checks empty. `ceiling`:
credited gap 4,644 → 4,494; `native_error` lines 0 → 171, all matched
(169 in a case with an `.errors.txt`, 1 attributed).

**Per-arm loss attribution** (each measured unfiltered, both dumps):

- §475 alone: zero losses, zero type transitions besides the `require` gain.
- empty name: 9 RIGHT→WRONG, all `() => Generator<any, void, any>` /
  `AsyncGenerator` lines (`YieldExpression5_es6`, `YieldStarExpression3_es6`,
  four `parser.asyncGenerators.*.es2018(alwaysstrict=true)`). The consumer
  is `signatures.rs`' §642 `yield*` arm, which asks `operand_type == any`
  where upstream's `getIterationTypesOfIterable` asks `IsTypeAny`
  (`checker.go:20343`), true for `errorType` too. With that one consumer
  fixed: zero losses, zero gains. Shipped as
  [diff A](r5-errorsplit2-empty-name.diff); `signatures.rs` is not this
  lane's file.
- final `else`: losses in three consumers outside this lane, plus one the
  `require` arm fixes:
  - `members.rs` `check_property_access_expression`: a `native_error`
    receiver falls through to member lookup and answers the gap; upstream
    answers `errorType` for any any-like receiver that `isErrorType`
    (`checker.go:11314-11320`). Seen as `parser509534:0:5`/`:0:11` (the
    function assigned to `module.exports.route` loses its type through the
    gap's contextual decline) and `destructuringParameterDeclaration4`'s
    extra TS2345.
  - `array_literals.rs` `array_spread_element_type`: spreading
    `native_error` must contribute it unchanged (`IsTypeAny(inputType)`).
  - the array-literal and pattern roads decline on `Checker::is_error`,
    which step 1 defined as true for both identities (§5).
  With the `members.rs` and `array_literals.rs` fixes: 1 type loss left
  (`assignmentRestElementWithErrorSourceType:0:2`, `[...c] : any[]` →
  `unknown[]`, the destructuring-target road's `tuple_array_like` test), 0
  diagnostics losses. Shipped as [diff B](r5-errorsplit2-final-else.diff)
  with that residual named; not landable as is.

## §3 The writer guards

`types_producer::type_id_at_location_tracking`'s position guards asked
`computed == error || computed == any`. `native_error` is neither, so without
a change a switched producer at a guarded position would print `error`
where upstream's writer prints `any`. They now ask `is_error_identity`
(gap or `native_error`) — the guards are upstream's `writeTypeOrSymbol`
conditions on an any-flagged type, so they apply to both identities. Only
the gap sets `saw_checker_error`, which reports the port's own failure. The
string rewrites in `render_file` already worked: `render` prints
`native_error` as `error` by identity, and they rewrite that string.

## §4 (d) Narrowing the writer's gap rewrites: measured, not done

`types_producer` now records, per line, the first writer rewrite that
printed the port's gap as `any` (`GapRewrite`), and `ceiling` names the
gap's producer through `gap_reason` (computed after the walk, so the
attribution cannot perturb the rendered types). Narrowing the rewrites to
`native_error` prints exactly those lines `error`. At commit 1:

| | lines |
|---|---:|
| **RIGHT → GAP** | **5,063** in 1,028 cases |
| WRONG → GAP | 1,488 |

By rewrite (RIGHT→GAP): `HadErrorBaseline` (SS180) 3,531; `AtLocation`
(the guards of §3) 1,119; `StatementName` 312; `AccessOrQualifiedParent`
78; `GlobalAugmentation` 23.

By producer (RIGHT→GAP, top rows; `gap_reason`'s leading clause):

| lines | producer |
|---:|---|
| 870 | reference, the name does not resolve |
| 544 | declaration name, symbol has no type: ALIAS / no value declaration |
| 353 | declaration name, symbol has no type: FUNCTION_SCOPED_VARIABLE / VariableDeclaration |
| 224 | expression answered error: CallExpression |
| 218 | expression answered error: ElementAccessExpression |
| 215 | expression answered error: JsxElement |
| 159 | expression answered error: JsxSelfClosingElement |
| 122 | reference, symbol has a type (the line differs for another reason) |
| 102 | declaration name, symbol has no type: BLOCK_SCOPED_VARIABLE / VariableDeclaration |
| 98 | the name of a JsxNamespacedName, the name does not resolve |
| 84 | expression answered error: BinaryExpression PlusToken |
| 76 | reference, symbol has no type: ALIAS / no value declaration |
| 57 | expression answered error: NewExpression |

**Why 5,063 and not the 4,644/4,494 of `ceiling`'s credited-gap row.** That
row counts matched lines whose *top-level type* is the gap. Of its 4,494 at
commit 1, 3,944 are rewritten (the four string rewrites) and 550 match
because the baseline itself prints `error` (no `.errors.txt`, upstream's
`errorType` through the fast path) — narrowing does not touch those. The
1,119 `AtLocation` lines are not in that row at all: the guard returns
`any`, so their top-level type is not the gap. 3,944 + 1,119 = 5,063.

The two largest producer rows are the populations step 2 is for: 870 are
unresolved references that the §31 gate keeps as the gap, and 544 are
aliases of modules this port does not type, P4 below. Every row that moves
to `native_error` leaves this table without loss. What remains is the cost
of narrowing, and it is the integrator's decision (ADR-0048).

## §5 `Checker::is_error` is the open question

Step 1 made `is_error` true for both identities. 201 call sites use it, and
nearly all of them mean "decline, the port computed nothing" — which is
wrong for `native_error`, a type upstream computed. Measured with the full
final-`else` switch and both consumer fixes:

| `is_error` answers | type losses | diagnostics losses |
|---|---:|---:|
| gap and `native_error` (step 1) | 1 (`assignmentRestElementWithErrorSourceType`) | 0 |
| gap only | 0 | 2 (`typeofProperty`'s TS2564 ×3, `for-of-excess-declarations`' TS2538) |

The second row's losses are consumers that used `is_error` as upstream's
flag test: `check.rs`' TS2564 arm (`t.flags&TypeFlagsAnyOrUnknown`,
`checker.go:4946`) and `index_access_reports.rs`' TS2538 arm. Neither
definition is right everywhere, so it stays as step 1 defined it. While no
producer outside this lane's three arms reaches `native_error`, the choice
cannot move a line (commit 1 measures zero losses under it). The audit of
the 201 call sites is the precondition for diff B; tracked as tsr-2zk.960's
follow-up in the §6 report.

## §6 P4 / P10 / P11 / P12 (r4-anyaudit §2) against native

| row | native (probe) | verdict |
|---|---|---|
| P4 `get_type_of_alias`' unfindable/missing-export arms (`symbols.rs`) | `errorType` for all four: missing named export (TS2305), missing default (TS1192), unfindable `import = require`, unfindable namespace import | switch to `native_error`; `symbols.rs` is not this lane's: diff, §7 |
| P10 return aggregate (`signatures.rs`) | `errorType` return when the single return is `errorType` (`function r() { return nosuch; }`) | switch only where the return expression is `native_error`, never for the gap; follows the final-`else` switch, not before it |
| P11 `parameter_of`, unresolved annotation (`signatures.rs`) | **not** `errorType`: `p: Missing` is the per-name unresolved reference (`getUnresolvedSymbolForEntityName`, `checker.go:23102`, declared type `unresolvedType`), printed `Missing` | do **not** switch; the port's `unresolved_types` is the matching shape |
| P12 `type_parameter_of`, unresolved constraint | **not** `errorType`: the constraint is the same unresolved reference, printed `Missing2` | do **not** switch |

Probe source (`/tmp/claude-0/probe/main.go`, built with
`go build -modfile=$WORK/tsgo.mod -overlay overlay.json ./cmd/errprobe`
where the overlay maps `cmd/errprobe/main.go` to it): it builds a program
from a directory's `tsconfig.json`, runs `GetSemanticDiagnostics`, then walks
the file printing `AT line:col kind text {ERROR|ANY|other:T}` for every
identifier and expression via `GetTypeAtLocation`, `RET` for
`GetReturnTypeOfSignature(GetSignatureFromDeclaration(fn))`, `PARAM` for
`GetTypeOfSymbol(param)` and `CONSTRAINT` for
`GetConstraintOfTypeParameter`.
