# r5-classfields — class checks gated on `target` and `useDefineForClassFields`

Lane `tsr-2zk.1004` (= `.987`), round 5. Source population:
`docs/parity/notes/r5-variants2.md` §4.1 row 3 — 39 configured rows over 23
cases at the branch point. Ported against `vendor/typescript-go` @ `5b1047d`.
New code lives in `crates/tsr-checker/src/class_fields.rs` (TS2373 body arm,
TS2818) and `crates/tsr-checker/src/heritage_conformance.rs`
(`check_kinds_of_property_member_overrides`); the TS2301 and TS2699 changes
are inside their existing lane-specific functions in `check.rs`.

Population at the frozen baseline (`9cccd51`), as the configured diagnostic
rows whose differing codes touch the lane's codes (2818, 2373, 2301, 2729,
2610, 2611, 2612, 2699, 18037): **36 rows**, 30 of them "sole" (every
differing code is a lane code). r5-variants2 counted 39 / 34 at its own
branch point.

## 1. TS2610 / TS2611 — `checkKindsOfPropertyMemberOverrides` (`checker.go:4536`)

**Forcing constraint.** 15 configured rows (`autoAccessor6` ×6,
`autoAccessor7` ×6, `abstractProperty` ×2, `abstractPropertyBasics`) were
wrong for two upstream rules the old `check.rs::check_override_kind` did not
have:

1. **An auto-accessor binds as an accessor.** `bindPropertyWorker`
   (`binder/binder.go:747`) gives `accessor a` `SymbolFlagsAccessor`, so a
   derived plain property over it is TS2610 and a derived `get a()` over it
   is no error. The old reader classified every `PropertyDeclaration` as a
   property and reported the inverse (TS2611 on the getter).
2. **`arePropertiesAbstractOrInterface`** (`checker.go:4692`): when the base
   member is `abstract` and none of its declarations is an initialized
   property, the kinds need not match. The old reader ignored `abstract`.

Also brought to upstream while the function moved:
- **The nearest base declaration decides, private or not.** Upstream asks
  `getPropertyOfObjectType(t, name)` and skips when *either* side is private;
  the old walk dropped private base members and so kept looking further up
  the chain, comparing against a grandparent upstream never sees.
- **Modifier flags of an accessor pair come from the getter**
  (`getDeclarationModifierFlagsFromSymbol`), and the error node is the
  derived symbol's first declaration.
- **No parse-error gate.** `checkClassLikeDeclaration` has none (r4-heritage
  measured the same for its own arms).
- The third message argument is now the derived class's name (it was the
  base's name twice); messages are not scored, but the text is upstream's.

**Alternatives.** A type-level port (`getPropertiesOfType(baseType)` with
`getTargetSymbol`, symbol flags and `CheckFlagsMapped`) is the literal
transliteration. Rejected for this lane: the existing §309/§708 decision reads
both sides from declarations because every input to the rule is a declaration
kind or modifier, and the type-level walk would route a pure syntax question
through member resolution, whose gaps (merged and late-bound members) would
then decide a kinds check. What would change the call: a base reached only
through a type — an intersection, a mixin, an interface-merged class — whose
members this reader cannot see. Those are declined (no report), never
guessed.

**Accepted.** Base resolution is still identifier-only `extends` naming a
class declaration; class expressions and qualified bases decline. TS2612
(`Property will overwrite the base property`, the `GetUseDefineForClassFields`
arm) is **not ported**: it needs `isPropertyInitializedInConstructor`, and no
row of either dump expects or reports TS2612 today, so it has no measurable
target. The abstract-member half of the upstream function is TS2515's own
check (`check.rs`, §761) and untouched.

**Falsifier.** A row where TSR reports TS2610/2611 on a derived member whose
upstream base symbol is not the nearest same-named declaration up the chain
(a merged interface or a mixin base).

## 2. TS2373 for a body declaration — `onSuccessfullyResolvedSymbol` (`checker.go:1850`)

**Forcing constraint.** 11 configured rows (`classWithStaticFieldInParameter*.2`,
`nullishCoalescingOperatorInParameter*.2`, `optionalChainingInParameter*.2`,
`parameterInitializersForwardReferencing.2`, `capturedParametersInInitializers2`)
expect TS2373 when a parameter initializer names something the function's
**body** declares and the downlevel emit must move the parameter list into
the body: `useOuterVariableScopeInParameter` (`binder/nameresolver.go:346`)
is false exactly when `requiresScopeChange` holds for some parameter — a
static field without `GetEmitStandardClassFields`, `?.`/`??` below ES2020, an
object rest below ES2017. The resolver then keeps the body's symbol, and
`onSuccessfullyResolvedSymbol` reports it as declared after the parameter.

**How it is ported.** `class_fields.rs::check_parameter_reference_to_body_declaration`
replays the resolver's walk state for one identifier
(`parameter_initializer_scope`): the associated declaration is the first
`Parameter` or parameter `BindingElement` reached through its initializer or
binding-pattern name, and the walk returns `None` as soon as
`getIsDeferredContext` (`nameresolver.go:459`) holds for a location passed
before the function — a non-IIFE arrow/function expression, an async or
generator IIFE, a function-like declaration not entered through its name, a
non-static property declaration, a type query. The candidate is the
function's local of that name; a non-variable (class, function, enum) is
kept regardless of target, a variable only when
`declaration_requires_scope_change` (already ported in `name_suggestion.rs`
for the suggestion walk) holds. The check then asks `resolve_name` and
requires the same symbol, so an IIFE between that shadows the name declines.

**The later-parameter arm stays where it is.** A later *parameter* is
`parameter_self_reference.rs`'s, whose deferred-context test treats every
class expression and function-like as deferred; this check excludes
parameters so nothing is reported twice. `capturedParametersInInitializers2`
(target=es2015) is that arm's remainder (`static c = x` in a class
expression is *not* deferred upstream; `[x]` computed names are not either).
Reported as an outside-file change (§7).

**Accepted.** `DeclarationNameToString` prints a binding pattern's source
text; the checker has no source text, so the pattern is rebuilt from its
element names (`{ b, ...x }`). The code and span are upstream's; the message
argument can differ for a destructured parameter.

**Falsifier.** A TS2373 on a name that a resolver step this walk does not
replay (a `with`, a catch clause, a class's own members) would have answered
first.

## 3. TS2818 — `checkReflectCollision` (`checker.go:10582`)

**Forcing constraint.** `superInStaticMembers1` (target=es2015, es2021): 92
expected lines, none reported — TSR had no collision infrastructure.

**Upstream's shape.** `checkSuperExpression` (`checker.go:7946`) marks every
block-scope container above a `super.x`/`super[x]` whose (arrow-adjusted)
`super` container is a static property or static block, when
`languageVersion <= ES2021` (a script file is never marked).
`recordPotentialCollisionWithReflectInGeneratedCode` defers
`checkReflectCollision` for each declaration named `Reflect` that
`checkCollisionsForDeclarationName` sees, and the deferred check reads the
mark on the declaration's enclosing block-scope container (a class
expression: any member; a function expression: itself).

**How it is ported.** The mark is a pure function of the tree: a container
is marked exactly when such a `super` lies inside it. This port checks in
document order, so a mark *recorded* while checking would miss a `super`
later in the file; reading it off the subtree instead makes the deferral
unnecessary. The check only runs for an identifier spelled `Reflect` that is
the name of a declaration kind upstream passes to
`checkCollisionsForDeclarationName`, at `target <= ES2021`, after
`needCollisionCheckForIdentifier`'s exclusions (ambient, type-only import,
overload parameter). The subtree scan is bounded by the rarity of the name.

**Accepted.** The legality tests before the mark are reduced to what a static
member needs: a `super` crossing a computed property name, or in a class with
no `extends` clause or `extends null`, does not mark. A base that does not
resolve (`getBaseTypes` empty) still marks here where upstream returns early;
no corpus row exercises it. `IsExternalOrCommonJSModule` is read as
`is_external_module` (ESM syntax only).

**Falsifier.** A TS2818 on a declaration whose container holds only an
illegal `super` (one upstream errors on before marking).

## 4. TS2301 — `checkAndReportErrorForInvalidInitializer` (`checker.go:1514`)

**Forcing constraint.** `classMemberInitializerScoping2(target=esnext,usedefineforclassfields=true)`
reported TS2301 where upstream reports nothing: under
`GetEmitStandardClassFields` an instance initializer is evaluated in class
scope, the callback returns `false`, and resolution goes on (to the outer
`const x`). The gate is added at the top of the TS2301-only helper
`property_initializer_referencing_a_constructor_parameter` (its one caller is
the invalid-initializer arm), so behaviour is upstream's.

`Checker::get_emit_standard_class_fields` (`class_fields.rs`) is
`GetEmitStandardClassFields` (`core/compileroptions.go:338`): the flag not
false **and** `getEmitScriptTarget() >= ES2022`. `standard_class_fields`
holds `GetUseDefineForClassFields` only.

## 5. TS2699 — `checkClassForStaticPropertyNameConflicts` (`checker.go:4393`)

**Forcing constraint.** `staticPropertyNameConflicts` (both configurations):
every miss was either a computed name (`static [FunctionPropertyNames.name]`)
or a member of a class *expression*.

1. `getEffectivePropertyNameForPropertyNameNode` (`checker.go:18703`) falls
   back to `tryGetNameFromType(getTypeOfExpression(expr))` for a computed
   name: a string or number literal type names the member. The check now
   asks `check_expression` and `property_name_from_index`, as
   `late_bound_members_of` already does.
2. `checkClassLikeDeclaration` runs for class expressions as well; the
   `ClassExpression` arm of `check_node` now calls the same function.

## 6. Measured

Frozen baseline `9cccd51` → §1-§5 together, unfiltered dumps
(`diagverdictdump`, `verdictdump`):

- **Diagnostics: +29 rows, 0 lost.** 28 configured — `abstractProperty` ×2,
  `abstractPropertyBasics`, `autoAccessor6` ×6, `autoAccessor7` ×6,
  `classMemberInitializerScoping2(esnext,udcf=true)`,
  `classWithStaticFieldInParameterBindingPattern.2` ×2,
  `classWithStaticFieldInParameterInitializer.2` ×2,
  `nullishCoalescingOperatorInParameterBindingPattern.2`,
  `nullishCoalescingOperatorInParameterInitializer.2`,
  `optionalChainingInParameterInitializer.2`,
  `parameterInitializersForwardReferencing.2`,
  `staticPropertyNameConflicts` ×2, `superInStaticMembers1` ×2 — and one plain,
  `constructorParameterShadowsOuterScopes2` (TS2301 under its default
  ESNext/standard-fields options). Two still-wrong rows changed, each losing
  a wrong lane line: `abstractPropertyNegative(es2015)` (TS2610 extra gone),
  `optionalChainingInParameterBindingPattern.2(es2015)` (TS2373 now right;
  an unrelated TS2537 extra remains).
- **Types: unchanged** (552,533 aligned lines, 543,274 RIGHT; 0 lost).
- **Coverage binary:** `diagnostics` 4,424 → **4,425**/5,502,
  `diagnostics_configured` 675 → **703**/1,089, `checker_types` 8,183/9,538
  and `checker_types_configured` 1,591/1,928 unchanged.
- **Perf** (median child CPU, new/old, 21 samples, diagnostics match):
  domain-model **0.944**, generic-imports **1.024**.

## 7. Needed outside owned files (not made)

- **TS2729 under `useDefineForClassFields` on an old target**
  (`initializationOrdering1(target=es2021,usedefineforclassfields=true)`,
  extra TS2729): `readonly_target.rs::emit_standard_class_fields` returns
  `standard_class_fields` and its comment says "the checker keeps no target";
  it does (`language_version`). Upstream's `GetEmitStandardClassFields`
  also requires `target >= ES2022`. Diff:
  `docs/parity/notes/r5-classfields-ts2729-emit-standard.diff`.
- **`standard_class_fields` ignores an unset target.** `checker.rs`
  `apply_compiler_options` defaults it from `options.target >= ES2022`;
  upstream's `GetUseDefineForClassFields` reads `GetEmitScriptTarget()`,
  which is ES2025 when `target` is unset. Measured separately (§8).
- **TS2373 for a later parameter** (`capturedParametersInInitializers2`,
  `capturedParametersInInitializers1`): `parameter_self_reference.rs`'s
  `enclosing_parameter_initializer` treats every class expression and
  function-like as deferred; `class_fields.rs::parameter_initializer_scope`
  is upstream's `getIsDeferredContext` walk and could replace it.
