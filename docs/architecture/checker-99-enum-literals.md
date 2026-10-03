# Enum literal values and their consumers

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 056e7e87, evidence 4b615ef3: 455,405/478,855 matching assertions.
The active 99% target requires 474,067 matches.

## Representation and identity

The old enum member representation used Named plus ENUM, including constants.
The constant evaluator and enum_value_types already knew the values, but readers
of STRING_LITERAL/NUMBER_LITERAL flags could not see them. Computed properties
therefore became index signatures or disappeared, string-enum annotations lacked
primitive members, and discriminant and literal consumers took computed-enum paths.

getEnumLiteralType (checker.go:25362) stores ENUM_LITERAL with the appropriate
primitive literal flag. TypeData::EnumLiteral now carries the constant string or
number, the owning enum and the canonical member symbol, plus display text. The
owner prevents equal values in distinct enums from becoming the same literal.
The existing per-enum value cache reuses the first member for duplicate values.
Freshness remains separate; literal-to-base widening reads the payload owner.
Unsupported constant expressions still use computed-enum types. This unit does
not extend the evaluator to imported constants or replace its numeric formatting.

Single-value enums now use the regular literal itself, as getDeclaredTypeOfEnum
does, instead of a one-element union. Display twins retain this port's existing
member-declaration versus enum-reference spelling. A completed union of all
regular members reuses the declared enum after flattening, preventing E from
being printed as E.A | E.B after logical operations. Ordinary alias-union wrappers
retain their existing representation.

The alternative of special-casing enum names in computed properties was rejected:
it would leave indexed access, truthiness, primitive members, widening and
relations inconsistent. The first payload candidate gained 55 matches but lost
44 RIGHT rows, establishing that the consumer changes are required parts of this
representation rather than independent formatting improvements.

## Literal consumers

Property construction and indexed access now read actual enum values. The
checked object property array supplies semantic keys even without a binder key;
when a late-bound key differs from its binder symbol, member lookup uses those
captured properties. Otherwise a correctly printed {0: string} still failed
inference because getTypeOfProperty searched the binder under the computed name.

Truthiness and definitely-falsy extraction read the payload. Non-strict enum
literal facts include Falsy, matching BaseStringFacts/BaseNumberFacts; logical
negation consumes those facts. Ordinary non-enum literal facts retain their
previous non-strict limitation. getBaseTypeOfLiteralType now resolves enum
members to their owning enum, before primitive widening. The relater admits
ENUM_LITERAL and preserves nominal enum checks before primitive relations.

signatureHasLiteralTypes is syntactic: getSignatureFromDeclaration
(checker.go:19870) sets it for a LiteralType parameter node. Enum-member type
references are not specialized overloads merely because their resolved types
carry literal flags. Calls and constructors share the same syntax predicate.

## Property inference into indexes

inferFromIndexTypes (inference.go:911) collects applicable properties and source
index values, unions those values, then also infers from the applicable source
index. Optional properties contribute their value without missing/undefined.
The implementation preserves the mapped-type inference priority and uses native
index applicability, including numeric string names. Interfaces and classes do
not gain an implicit index from their properties. Source shapes whose synthetic
rest/reverse-mapped identity is not represented still have the existing limits;
this is not a claim that all index inference is complete.

## Context after generic inference

With the literal flags corrected, annotation_member_context could retain an enum
literal using a generic parameter instantiated from that same argument. The
object's captured type and symbol road then disagreed; Object.assign selected its
any-returning fallback. Before inference resolves the call, this annotation-only
path now refuses the inferred generic signature. After resolution it reads the
recorded concrete signature. A final check rebuilds a non-context-sensitive
object argument under that concrete context, without changing the already inferred
return type.

Discarded alternatives are measured. Refusing every instantiated generic context
without the final check lost two previously correct boolean object displays.
Clearing complete argument subtrees for the final check lost 357 RIGHT rows:
it erased established initializer-symbol types and reopened cycles. Rebuilding
only the outer literal reduced that to seven rows in one async loop. The
non-generic call there already had a concrete context; repeating its check
re-entered a flow-dependent initializer. The retained extra check applies only
to generic declarations and preserves their checked initializer state. Existing
context-sensitive arguments retain their established checking path. New cyclic
or callback regressions in the full corpus would falsify that boundary.

## Validation and remaining limits

Six pipeline tests cover 49 outcomes checked against native declarations or
existing native type baselines: string/numeric/negative/duplicate/single-member
keys, spread and method keys, lookup, const values, freshness, primitive methods,
overloads, strict and non-strict truthiness, index filtering and optional values,
generic object argument contexts, and a flow-dependent non-generic loop. Native inputs are retained as
/tmp/tsr-99-enum-{values,semantics,index-inference,context}.ts, with native output
directories alongside them. Every native invocation explicitly sets strict mode.
Duplicate-key, invalid-index and excess-property diagnostics are expected in
some controls; declarations are still emitted.

The negative interface-without-index probe still returns error instead of
native unknown. The numeric-string enum probe succeeds for '0'; the invalid
'00' array access retains the existing error-versus-any gap. These negative
probes are not asserted as native matches. Nonlocal string-enum spread and
qualified alias display remain unresolved, as do enum static index inference
and mapped-type reverse inference exposed by the broader property collection.

No corpus expectation, scoring rule, denominator or pinned source changes.
Primary-thread correctness, testing, maintainability, standards and adversarial
review are sequential under the repository's no-delegation instructions.
Independent or cross-model review is not claimed. Final measurements, committed
checkout validation and mutation results are appended below after they run.

Frozen candidate: 455,694 RIGHT, 2,804 GAP and 15,745 WRONG among 474,243
aligned rows. This is 95.16% of the unchanged 478,855 full denominator, with
18,373 matches still required for 99%. Relative to 056e7e87: 236 WRONG→RIGHT,
53 GAP→RIGHT, zero RIGHT losses, three GAP→WRONG, 17 changed WRONG and one
WRONG→GAP. The new WRONG rows are the existing isomorphic mapped-type reverse
inference limitation; the new gap is nonlocal string-enum spread. These are not
counted as correct outcomes. All 207 release workspace result blocks, clippy,
formatting and 3,331 upstream anchors pass. Both diagnostic shape examples now
classify EnumLiteral explicitly; their exhaustive matches had correctly failed
to compile until the new payload was handled.

Frozen verdict: /tmp/tsr-99-enum-frozen.tsv, byte-identical to the earlier final
candidate. Source SHA-256 (checker sources plus trace_case):
1187c9e9586b537f70a4191383dd984280a66922bff3f41aa84f4c0adbfa27cd.
Logs: /tmp/tsr-99-enum-workspace4.log, /tmp/tsr-99-enum-clippy4.log,
/tmp/tsr-99-enum-anchors.log and /tmp/tsr-99-enum-final-delta.txt.

Committed verification at fd9b42be0e6f232f6907484bff4c3de7869b9861 used the
isolated /tmp/tsr-99-enum-verify checkout. Its full verdict is byte-identical to
the frozen candidate, and its source hash matches. The checker snapshot records
6,895/9,538 complete cases (72.29%), 24 more than the previous checkpoint.
All 207 release workspace result blocks, clippy, formatting and 3,331 anchors
pass there. Fresh depend: 505 non-gapping roots, 210 cycles, zero depth-cap hits
and 3,626 walked gaps. C3 balances; C1/C4 remain stale (tsr-6.29) and are not
coverage evidence. The snapshot is copied from this committed checkout.

Committed evidence: /tmp/tsr-99-enum-verified.tsv and
/tmp/tsr-99-enum-verified-{coverage,workspace,clippy,anchors,depend}.log.

Five isolated mutations fail their intended assertions, with successful compilation:
removing native literal flags fails words; removing property index candidates
fails numericEnum; restoring semantic overload specialization fails pickedA;
allowing premature inferred annotation contexts fails assign; repeating the
final check for non-generic calls fails the loop's lastId assignment. Each source
is restored in a finally block. The final restored hash matches the committed
source hash byte-for-byte.

Mutation logs: /tmp/tsr-99-enum-mutation-{flags,inference,specialized,context,loop}.log.
Review receipt: /tmp/compound-engineering-501/ce-code-review/enum-literals/review.json.

All six focused tests pass after restoration: /tmp/tsr-99-enum-restored-tests.log.

## Constant-expression completion after 8f8f4e1a

The per-enum duplicate-value cache already reuses the first member, but a
constant never reaches that cache when the evaluator rejects its expression.
The `constEnums` native baseline exposes this for same-enum literal element
references. The bounded extension follows pinned `evaluator.NewEvaluator`
(`internal/evaluator/evaluator.go:24`): skip parentheses, accept no-substitution
template text, and fold unary complement. `evaluateEntity`
(`checker.go:24024`) admits written string-literal-like element names; computed
key expressions and assertions deliberately remain unevaluated.

The same evaluator used Rust's saturating float-to-i32 conversion for bitwise
operators. Native `jsnum.Number.toInt32` (`internal/jsnum/jsnum.go:52`) instead
truncates and wraps modulo 2^32, mapping nonfinite inputs to zero. Unsigned
right shift retains a uint32 result instead of converting it back to signed.
An enum-local helper implements that conversion without changing general
numeric spelling or the expression-checking and alias-instantiation workers'
separate responsibilities.

Two focused tests failed against the baseline and pass with the extension.
They distinguish literal from computed element keys, parentheses from type
assertions, positive and negative overflow, signed and unsigned boundaries,
fractional truncation, shift-count masking, and nonfinite inputs. Expected
bitwise values follow the pinned jsnum operations independently of the Rust
implementation; a fresh native executable was not available in this orb.
The complete enum suite passes 9/9. Cross-enum/imported constants, mixed
string-number concatenation, and interpolated templates remain outside this
bounded evaluator extension.

The exact-base full scorepair moves 458,022 → 458,048 RIGHT among 474,244
aligned rows: 26 WRONG→RIGHT (`constEnums` 13,
`isolatedDeclarationErrorsEnums` 9, `constEnumPropertyAccess3` 4). GAP stays
2,445 and WRONG falls to 13,751. Independent multiline row comparison finds
zero RIGHT losses, added/removed rows, or same-verdict payload changes. This
is the enum unit alone, not a sum with other workers' reported gains.
