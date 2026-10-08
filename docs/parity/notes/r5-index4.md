# Lane notes: r5-index4 (tsr-2zk.1012)

Judgment calls made by the `r5-index4` parity box, continuing
[r4-index](r4-index.md), [r4-index2](r4-index2.md) and
[r4-index3](r4-index3.md). Baseline frozen at `f4ae684` (the integration
head at dispatch). Native behaviour is read from the pinned submodule
(`vendor/typescript-go` @ `5b1047d`) and its reference baselines.

## 1. TS7017 for a dotted name `typeof globalThis` does not have

**Forcing constraint.** `checkPropertyAccessExpressionOrQualifiedName`
(`checker.go:11337-11345`) has an arm for a receiver whose symbol is
`globalThisSymbol`. When the name is not a member and no index info applies
(`typeof globalThis` has none), it reports TS2339 for a block-scoped global
and otherwise, under `noImplicitAny`, TS7017 at the name: `Element
implicitly has an 'any' type because type 'typeof globalThis' has no index
signature.` The answer is `anyType` either way. This port ported only the
TS2339 half (`check_nonexistent_property`) and the `any` answer
(`members.rs`, the `global_this_type` arm). `nonexistent_property.rs`
records that TS7017 was left out deliberately: an implicit-any report does
not belong in the nonexistent-property rule.

Witnesses: script-level `this.x` (`this` is `typeof globalThis` at the top of
a script and in an arrow there) and `globalThis.x`. The cases are
`emitCapturingThisInTupleDestructuring1` (3 lines), `parserConditionalExpression1`
(2), `jsxReactTestSuite` (3) and `globalThisUnknownNoImplicitAny` (2 of its
5 missing lines; the other 3 are element accesses).

**Where it lives.** `check_global_this_property_access`
(`index_access_reports.rs`) is called from `check.rs`'s per-node walk next to
the element-access arm. The arm uses the same membership test as the member
road for this receiver: a global that is not block-scoped and is a value
(`resolveAnonymousTypeMembers` keeps the non-block-scoped exports, and
`getPropertyOfType` filters them with `symbolIsValue`). It reads
`check_expression(receiver)`, which is already cached, and the globals
table. No side table, cache or traversal is added.

**`globalThis` and `undefined` are members.** The checker files
`globalThisSymbol` (`checker.go:964`) and `undefinedSymbol`
(`addUndefinedToGlobalsOrErrorOnRedeclaration`) in `globals` itself, so
`globalThis.globalThis` and `globalThis.undefined` resolve natively. This
binder's globals table has neither, and without the exception the first
measurement put a false TS7017 on `globalThisReadonlyProperties:1`.

**Not covered.** `typeofThis:57` (`typeof this.no` in an arrow inside a
namespace). The port answers `any` for `this` there (the namespace arm of
`check_this_expression`), so the receiver is never `typeof globalThis`.
That is a `this`-typing question, outside this lane.

**Falsifier.** A baseline TS7017 that is missing where the receiver is
`typeof globalThis`, or an extra one on a name that some global declares.
