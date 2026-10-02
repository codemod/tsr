# Polymorphic and contextual object receivers

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline: 270ada6b, evidence 0200c432; 456,390/478,855 matching assertions.

## Plain references have a this argument too

Native getDeclaredTypeOfClassOrInterface gives every class, and any interface
that may reference this, a reference target with a this type parameter. Thus a
non-generic interface's init(this: this) member is instantiated with the receiver
just like a generic interface's member. The port stores plain declared types as
Named values without a type_reference_targets entry. instantiate_for_reference
returned their raw members, missing four assertions in thisTypeInFunctions2.

The member instantiation seam now recognizes named class/interface instances and
adds their owning this mapping. Explicit generic arguments still map through the
same seam. Identity entries are omitted: running a no-op T-to-T map through the
current general instantiator can needlessly evaluate a mapped template.

The first broad attempt gained 20 matches but lost 91. It replaced polymorphic
this and constrained receiver T with the apparent constraint's declared class;
it also expanded identity-mapped templates. Native getApparentType and
getTypeWithThisArgument retain the original receiver when obtaining apparent
members (checker.go:21741). A private member lookup now accepts that receiver
separately from the lookup type. The existing public entry point supplies itself;
property access supplies the original receiver before apparent-type projection.
This preserves both this.self(): this and value.self(): T for T extends Base.
With those prerequisites the full run gains 18 with no losses or other changes.

## Object contexts and ThisType markers

getContextualThisParameterType (checker.go:12021) first uses an explicit contextual
signature. Under noImplicitThis or JavaScript, object methods, accessors and
function-valued properties then consult their containing object's apparent
context. getThisTypeOfObjectLiteralFromContextualType checks for ThisType<T>, also
walking directly enclosing property-assignment literals. Union branches combine
marker results; intersections prefer the first marker. The global marker is
recognized by symbol identity, so a local same-named interface is ordinary data.
The marker argument is instantiated with the non-fixing inference mapper,
including its object fields. An explicit contextual this parameter takes
precedence, and an arrow retains its lexical receiver.

Without a marker, the non-null contextual object type supplies this. The earlier
union-only gate protected an unported contextual index-signature route; that
route is now present. Removing the stale gate enables plain object contexts as
well as discriminated unions. Existing handling of literals with no contextual
type remains; this is not a complete port of native context-free object checking
or JavaScript assignment-based receiver rules.

The first object-context draft gained 39 but lost four assertions in
neverReturningFunctions1. The generic alias ComponentDefinition<T> hid the marker,
so the fallback context supplied an uninstantiated Partial<Component> member.
Structural alias expansion exposes the marker. Full marker instantiation then
exposed another prerequisite: ComponentPublicInstance<any, any> is semantically
any, even though this port can retain a Named display wrapper. The marker consumer
uses the existing structural alias-body query and returns intrinsic reductions
while preserving unreduced alias display identities. This draft gained 40 but
lost one assertion in contextualTypeBasedOnIntersectionWithAnyInTheMix5 before
the alias reduction was added. These counts are recorded as rejected drafts.

## Controls and limits

contextual_this_objects exercises ordinary/generic polymorphic receivers,
constrained receivers, inherited returns, identity mapped templates, contextual
methods and function properties, nested markers, explicit receiver precedence,
aliases, active inference into receiver fields, marker identity, intrinsic alias
reduction, lexical arrows and loose-mode gating. Native strict declaration
controls are under /tmp/tsr-99-polythis-native-tests and the individual native
programs/logs; Rust controls use the ordinary conformance pipeline.

This preserves the port's existing assigned-method-slot lifecycle. The native
full signature-links/context-checked state machine, context-free literal self
types and check-mode threading remain broader work on tsr-8. No global alias
representation or name-rendering rewrite is claimed.

The loose-mode negative control exposed an older this-container omission: object
methods/accessors without an explicit or contextual receiver fell through to
globalThis. Native tryGetThisTypeAtEx stops at that function boundary and yields
any. Adding the boundary initially gained 41 but lost eight assertions in the
two computedPropertyNames22/29_ES6 fixtures: a computed name executes outside
the declaration whose name it computes. Native checkThisExpression skips that
object member before continuing the lexical walk. The port now follows that
object-member order. Extending the skip to class computed names lost eight
assertions in computedPropertyNames21/23_ES6; native class-computed-name
checking has a separate diagnostic container path and its baseline queries
require a distinct audit. Existing class lookup is preserved in this unit. The active-path recursion guard for marker
lookup also releases each visited type before another union branch, so a shared
marker is not mistaken for an alias cycle. Both rules have direct regressions.

All strict native test programs compile except the lexical-global-this negative
control, which intentionally reports TS7041. It still captures globalThis; the
error is a diagnostic, not a rebinding to the marker.

## Frozen candidate evidence

The final candidate has 456,431 RIGHT, 2,697 GAP and 15,115 WRONG across
474,243 aligned rows. Relative to 0200c432: 41 WRONG-to-RIGHT, zero RIGHT losses
or GAP-to-WRONG, and 37 changed already-WRONG rows. Those remaining changes
cluster in recursive conditionals, JS/accessor inference, and nested generic
contexts. Full verdict: /tmp/tsr-99-polythis-object-scope.tsv. Production hash:
be00b5c16fcd5f3cfa9d4bee74ee501c7026eb292977e6f05ad052b3887b2bae
(sorted checker/src/*.rs then conformance/src/trace_case.rs, paths and bytes).
Ten direct regression tests pass. Review is sequential in the primary thread
under the user's AGENTS override; no independent reviewer is claimed.

The older objects unit test a_non_union_contextual_type_keeps_any asserted the
port's previous restriction rather than native behavior. It is renamed to
a_non_union_contextual_type_supplies_this and now requires I for a plain
contextual interface with a method and string indexer. This is the behavior
proved by the new direct Model and FunctionModel controls as well.

Main-checkout validation: all 217 release workspace result blocks pass, as do
release clippy, formatting and 3,320 upstream anchors. The final source is frozen
for committed verification; failed intermediate runs are retained in the scratch
logs rather than reported as successful validation.
