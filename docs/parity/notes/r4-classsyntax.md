# r4-classsyntax — class-syntax diagnostics (`tsr-2zk.912`)

Round-4 lane notes. Native source: `vendor/typescript-go` @ `5b1047d`,
`internal/checker/checker.go`. Baseline frozen at integration head `c02dbbd`.

## §1 TS2314: qualified names, class `extends` of a non-value, the `{0}` argument

`getTypeFromClassOrInterfaceReference` (`checker.go:23169`) is reached for
every class/interface reference, so a namespace-qualified name (`M.E`) is
checked like a bare one. Three divergences in `type_argument_arity.rs`, each
confirmed against a native tsgo built from the pinned submodule:

1. **`M.E` declined.** `qualified_type_name_symbol` resolved the left side
   with `NAMESPACE_MODULE | TYPE`. An *instantiated* namespace (one holding a
   class) is a `ValueModule`, not a `NamespaceModule`, so the lookup missed.
   Native's `resolveQualifiedName` resolves the left with
   `SymbolFlagsNamespace` (`ValueModule | NamespaceModule | Enum`); the port
   now uses `SymbolFlags::NAMESPACE`. The heritage form (`extends M.E`, a
   `PropertyAccessExpression`) now reaches the same helper.
   §1040's rejected widening was to `| VALUE`, not to `NAMESPACE`.
2. **`class D extends I` with `I` an interface.** The class's base is a
   value: native checks the expression, reports TS2689, and the base
   constructor type is the error type, so `resolveBaseTypesOfClass` never
   reaches the arity arm. `class_extends_entry_names_a_class` answered
   `true` for an identifier with no value; it now answers `false`.
3. **Message argument.** Native passes `typeStr`, the declared type printed
   (`C<T>`), not the written name (`C`). The TS leg now prints the declared
   type, as the JS leg already did.

Measured: 11 TS2314 lines fixed (`genericTypeReferenceWithoutTypeArgument`
+5, `.d` +2, `3` +2, `genericCloduleInModule2` +1; one wrong TS2314 removed
from `genericTypeReferenceWithoutTypeArgument2`); none lost.

Not fixed here (outside owned files):

- `var k = <M.E>null` still reports a TS2352 native does not: native's
  arity error returns `errorType` from the type reference, which silences the
  assertion. TSR's `get_type_from_type_reference` (`declared.rs`) does not.
- `declarationEmitExpressionInExtends4`'s TS2315 on
  `extends getSomething()<number, string>`: native reaches
  `checkNoTypeArguments` from `resolveBaseTypesOfClass` with the base
  constructor's symbol (`base_types.rs`), not from a name.
- `extendsTag5`'s four wrong TS2315 lines: a JS class's `@template` type
  parameters are not in `local_type_parameters_of` (`declared.rs`), so the
  class reads as non-generic (r4-jsdoc's `@template` hosting).

## §2 TS2683/TS2331/TS7041 on `typeof this`: `checkIdentifier`'s first arm

`typeof this.x` parses the `this` as an **identifier** (`parseEntityName`
with `allowReservedWords`), not a `ThisKeyword`, so the walk's
`ThisKeyword` dispatch never saw it. Native's `checkIdentifier`
(`checker.go:11043`) sends an identifier with `ast.IsThisInTypeQuery` to
`checkThisExpression`, which runs every arm (TS17009, TS2331, TS2465,
TS7041, TS2683). The port adds that dispatch for identifiers, reusing
`flow.rs`'s existing `is_this_in_type_query`; the test is a node-map read and
a text compare per identifier.

Measured: `typeofThis` TS2683 x3, TS2331 x2, TS7041 x1 fixed; none lost.

## §3 TS2680/TS2681/TS2730/TS2784: `checkParameter`'s `this` arm

Not ported before. `checkParameter` (`checker.go:2654`) reports, for a
parameter named `this` (or `new`): TS2680 when it is not first, TS2681 on a
constructor, construct signature or constructor type, TS2730 on an arrow
function, TS2784 on an accessor. Ported whole, in that order, at the
parameter. `GetContainingFunction(param)` is the parameter's parent; a parent
whose list `unused.rs`'s `parameters_of` does not enumerate (index
signatures, JSDoc function types) is never given TS2680, so a missing list
cannot read as "not first".

Measured: `thisTypeInFunctionsNegative` TS2681 x3, TS2680 x2, TS2730 x4
fixed; none lost.

**Accepted:** `assertionTypePredicates1` (already WRONG) gains TS2680 and
TS2784 at `set p2(x: asserts this is string)`. Native parses
`asserts this is string` in a parameter's type as a type predicate (TS1228);
TSR's parser recovers with TS1005 and a second parameter named `this`, which
this arm then reports. The fix is the parser's (type predicates in
`parseType`), not a decline here: native runs `checkParameter` on files with
parse errors too, and `thisTypeInFunctionsNegative`'s own fixed lines sit in
such a file.

## §4 Contextual `this` for compound assignment (`??=`, `||=`, …)

`getContextualThisParameterType` (`checker.go`) types `this` in
`obj.xxx = function () {…}` as `obj` when
`ast.IsAssignmentExpression(parent, false)`: *any* assignment operator,
compound included. `this_expression.rs`'s shape test accepted only `=`, so
`Element.prototype.remove ??= function () { this… }` reported TS2683 under
`noImplicitThis`. It now asks `is_assignment_operator()`.

Measured: `thisPrototypeMethodCompoundAssignment` and its `Js` twin lose
two wrong TS2683 lines each.
