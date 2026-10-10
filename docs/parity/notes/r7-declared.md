# r7-declared: `declared.rs`, `mapped.rs`, `instantiation_expressions.rs`, `indexed.rs`, `unions.rs`, round 7

Lane `r7-declared` (`tsr-2zk.1273`), dispatched in `docs/parity/round7.md`.
Owned files: `declared.rs`, `mapped.rs`, `instantiation_expressions.rs`,
`indexed.rs`, `unions.rs`. Native source is `vendor/typescript-go` @
`5b1047d`. Only this lane writes this note.

## 0. Frozen base

`origin/main` at `9020aa67` (round-7 dispatch), frozen with
`scripts/parity_gate.sh freeze /tmp/box/base`:

- `verdictdump`: TOTAL 556,357, RIGHT 551,176, GAP 673, WRONG 4,508.

Every measurement below is an unfiltered `freeze` of the candidate tree and
`scripts/parity_gate.sh compare /tmp/box/base <candidate>`.

## 1. `tsr-2zk.1266`: r6-typesroots3's mapped stack, re-cut on the print-time plans

**What was held.** Batch CG+CH+CI backed out r6-typesroots3's four mapped
diffs (`1b5ef1a8` mapped alias-reference keys, `212e93b6` mapped member
names, `c786f303` mapped optional type, `53c5e4c4` intersection new alias;
reverted by `1751bac4`, `5aa8be47`, `63637259`, `0ff9e8e6`) because
mappedTypeIndexedAccessConstraint 0:96, 0:97 and 0:101 went from RIGHT to
`any` (`round5.md`, "Batch CG+CH+CI"). The four commits cherry-pick cleanly
onto `9020aa67`; the stack is
[`r7-declared-HELD-mapped-stack.diff`](r7-declared-HELD-mapped-stack.diff).

**The three lines were RIGHT by coincidence.** Probed with the base's
`probefile`: on `9020aa67` the declared type of

```ts
const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = { … };
```

is `{}` (the alias reference `PartMappings = SetOptional<Mappings, "foo">`
enumerates no keys, r6-typesroots3 §2.1), `mapper[key]` is `{}[K]`, and the
call `mapper[key](o)` resolves through that deferred access's template to
`PartMappings[K]`, which is the native print reached by the wrong road. With
the stack, `mapper` is native's
`{ foo?: ((o: MapperArgs<"foo">) => boolean | undefined) | undefined; "12": …; 42: …; }`
and `mapper[key]` is native's `((o: MapperArgs<K>) => PartMappings[K]) |
undefined`. Then the call answers `error` (printed `any`).

**The cause is in `calls.rs`.** `resolveCallExpression` (`checker.go:8511`)
passes every callee through `checkNonNullTypeWithReporter`
(`checker.go:7413`): it reports TS2722/TS2721/TS2723 and then resolves the
call against `GetNonNullableType(funcType)`. TSR's head reports the
diagnostic (`check_non_null_callee`, `calls.rs`), but the type road
(`check_call_expression`) strips the nullable half only for an optional
chain. Any call through `F | undefined` therefore answers `error`; this is
not specific to mapped types:

```ts
declare const g: ((o: number) => string) | undefined;
const a = g(1);   // native: string (with TS2722); TSR on 9020aa67: error
```

The port is [`r7-declared-calls-nonnull-callee.diff`](r7-declared-calls-nonnull-callee.diff):
the non-optional branch takes `check_non_null_type(raw_callee_type)`
(`members.rs`, the existing non-reporting `checkNonNullType`), and an
`errorType` result returns `error` as native's `resolveErrorCall` does.
`calls.rs` belongs to r7-calls, so it is routed, not committed here.

**Measured** (unfiltered against §0):

| Tree | types | diagnostics |
|---|---|---|
| strip alone (`calls.rs` diff) | +11 RIGHT, 0 lost | 0 / 0 |
| mapped stack alone | +57 RIGHT, **3 lost** (0:96, 0:97, 0:101) | 0 / 0 |
| stack + strip | **+68 RIGHT, 0 lost** | 0 / 0 |

The strip's own +11: logicalAssignment5 ×4 targets (2 each),
controlFlowOptionalChain 2, interfaceClassMerging 1. The stack's rows on
top of it: mappedTypeIndexedAccessConstraint 39, mappedTypeGenericIndexedAccess
11, reverseMappedPartiallyInferableTypes 4, declarationQuotedMembers 3. (r6
measured the stack at +75 on `f334de9`; intersectionTypeInference3 and the
caseInsensitive/exactOptional rows it named are RIGHT on `9020aa67` without
it.) mappedTypeIndexedAccessConstraint keeps 5 wrong lines, the
`Identity<Partial<M0>>` modifiers chain (r6-typesroots3 §2.5).

**Landing order.** The strip first (r7-calls, or the integrator), then the
four cherry-picks. The stack is not committed on `box/r7-declared` while the
strip is not on main, because alone it loses the three lines.
