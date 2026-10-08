//! What an object literal **expression** must get right.
//!
//! The printed form is the same one `{ a: string }` produces as a type node, so
//! every assertion here has to be about a fixture the *type-node* path cannot
//! reach — an expression. See
//! [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Object literal expressions, and the two widenings".

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeFlags};
use tsr_core::Arena;

/// The printed type of the `index`th statement's first declaration's initialiser.
fn type_of_initialiser_at(source: &str, index: usize) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

fn type_of_initialiser(source: &str) -> String {
    type_of_initialiser_at(source, 0)
}

#[test]
fn an_object_literal_prints_structurally_in_upstreams_exact_form() {
    // Whole-line comparison, so the spaces and the trailing `; ` are the answer
    // and not a style choice.
    assert_eq!(type_of_initialiser("const o = { a: 1 };"), "{ a: number; }");
    assert_eq!(type_of_initialiser("const o = {};"), "{}");
    assert_eq!(type_of_initialiser(r#"const o = { a: 1, b: "s" };"#), "{ a: number; b: string; }");
    // Members keep **source** order, unlike a union's constituents. Upstream
    // builds a symbol table in declaration order and never sorts it.
    assert_eq!(type_of_initialiser(r#"const o = { b: "s", a: 1 };"#), "{ b: string; a: number; }");
}

#[test]
fn a_member_widens_where_a_bare_const_does_not() {
    // The property boundary is where freshness stops, and it is the single
    // easiest thing to get wrong in an object-literal port.
    //
    // `const n = 1` is `1` — `getWidenedLiteralTypeForInitializer` returns the
    // initialiser's type unchanged for a constant. `const o = { a: 1 }` is
    // `{ a: number; }`, because `checkExpressionForMutableLocation` widens each
    // member as the literal is checked (`checker.go:13885`). Upstream records
    // `>obj1 : { a: number; }` for exactly this source.
    assert_eq!(type_of_initialiser("const n = 1;"), "1");
    assert_eq!(type_of_initialiser("const o = { a: 1 };"), "{ a: number; }");
    assert_eq!(type_of_initialiser(r#"const o = { a: "s" };"#), "{ a: string; }");
    assert_eq!(type_of_initialiser("const o = { a: true };"), "{ a: boolean; }");
    // `let` versus `const` makes no difference *inside* a literal — the member
    // widened before the declaration was ever consulted.
    assert_eq!(type_of_initialiser("let o = { a: 1 };"), "{ a: number; }");
}

#[test]
fn a_nested_object_literal_is_typed_by_the_same_rule() {
    assert_eq!(type_of_initialiser("const o = { a: { b: 1 } };"), "{ a: { b: number; }; }");
}

#[test]
fn a_string_named_property_prints_unquoted_only_when_it_is_an_identifier() {
    assert_eq!(type_of_initialiser(r#"const o = { "a": 1 };"#), "{ a: number; }");
    // A name that is not an identifier is re-quoted the way upstream's printer
    // does. This asserted `error` until quoting landed; the fixture was found by
    // grepping the suite for stand-ins BEFORE the work rather than after, which
    // is the gap-fixture rule applied in the direction that actually helps.
    assert_eq!(type_of_initialiser(r#"const o = { "a-b": 1 };"#), r#"{ "a-b": number; }"#);
}

#[test]
fn a_member_this_port_cannot_type_makes_the_whole_literal_a_gap() {
    // The same rule the type-node path already follows: a partial object type is
    // a wrong answer that looks like a right one. §31 changed the MEMBER's
    // answer: a truly unresolved name is upstream's TS2304 `any`, so the
    // literal builds `{ a: any; }` — upstream's own baseline shape here.
    assert_eq!(type_of_initialiser("const o = { a: unknownThing };"), "{ a: any; }");
    // A method needs a signature. A gap, not faked.
    //
    // The shorthand `{ a }` used to be asserted here as a second gap. It is now
    // ported (see `tests/shorthand_properties.rs`) and answers `{ a: number; }`,
    // so the line was removed rather than updated: this test is about members
    // that CANNOT be typed, and a member that can no longer belongs in it. That
    // is the gap-fixture hazard — a fixture standing in for "unported" must be a
    // failure, not a form, or it silently asserts the opposite of its name.
    //
    // **`{ ...{ a: 1 } }` was removed for the same reason and by the same rule**
    // when object spread landed (`bd tsr-sps`); it now answers
    // `{ a: number; }` and is covered by `tests/members_object_spread.rs`. The
    // precedent set two lines above is what said to delete rather than update,
    // and this is the second time this fixture file has paid for having it
    // written down.
    // **And a third time, for the method fixture.** `{ m() { return 1; } }`
    // now answers `{ m(): number; }` — object-literal methods are ported
    // (`crate::objects`' `MethodDeclaration` arm) — so by the rule stated
    // above it is removed here rather than updated, and covered positively in
    // `a_method_member_prints_as_a_signature` below.
    //
    // The rule has now been applied three times in this one file. What is left
    // must be a member that genuinely cannot be typed: a computed name.
    // **Came due at §553.** A NUMBER- or STRING-literal computed name is
    // late-bound, and its printing is no longer unported:
    // `computed_member_index_key` already routed both to `LateBound` (upstream's
    // `StringOrNumberLiteralOrUnique` guard, `checker.go:13317`) while
    // `late_bound_symbol_member_name` answered only the SYMBOL half, so the
    // literal halves reached a `None` and the caller gapped the whole literal.
    // The name IS the member's name and takes the written-property spelling
    // rules. §553 measured 126 favourable against 20 adverse.
    assert_eq!(type_of_initialiser("const o = { [1]: 1 };"), "{ 1: number; }");
    // A method whose *signature* cannot be built used to keep the whole literal
    // a gap, which is the property the removed line was really testing.
    //
    // **§929 removed that population.** A parameter whose annotation does not
    // resolve no longer declines the signature: it keeps the written spelling
    // with an `any` type, because upstream's parameter carries `errorType` and
    // the node builder still reuses the written annotation node. So the literal
    // now types, and the assertion records the new shape rather than a
    // "cannot be built" case that this port no longer has.
    //
    // **The property the line was testing is therefore unasserted here**, and
    // saying so is better than inventing a fixture: §929 measured +442 with zero
    // `RIGHT->WRONG`, and finding a member that genuinely cannot be typed after
    // it is a search this file should not fake.
    assert_eq!(
        type_of_initialiser("const o = { m(x: keyof string) {} };"),
        "{ m(x: keyof string): void; }"
    );
}

/// §853 REPAIRED THIS TEST. It read, until the guard was gated:
///
/// > Upstream records **two different types** for one source line:
/// >
/// >     var c = {x: null};
/// >     >c : { x: any; }            <- getWidenedType, at the declaration
/// >     >{x: null} : { x: null; }   <- checkObjectLiteral
/// >
/// > This port has no call site for the first, so answering the second alone
/// > would make the declaration line wrong. A gap until both can be right.
///
/// **The two-types observation is the NON-STRICT one.** `createWideningType`
/// (`checker.go:25027`) returns the plain type when `strictNullChecks` is on,
/// so `undefinedWideningType` and `nullWideningType` *are* `undefined` and
/// `null` there, carry no `ContainsWideningType`, and
/// `getWidenedTypeWithContext`'s `RequiresWidening` gate never fires. Under
/// strict, upstream records `{ x: null; }` on **both** lines and there is no
/// second type to be wrong about.
///
/// This harness runs with `strictNullChecks` on, so it now asserts the strict
/// answer. The refusal is unchanged under non-strict, where the original
/// reasoning still holds exactly.
///
/// A residue pinned as an assertion reports its own repair (§800); a residue
/// described in prose does not. This test turned red the moment §853 made the
/// port correct, which is how the change was caught before it was pushed.
#[test]
fn a_nullable_member_is_kept_under_strict_and_widened_away_only_without_it() {
    assert_eq!(type_of_initialiser("var c = { x: null };"), "{ x: null; }");
    assert_eq!(type_of_initialiser("var c = { x: undefined };"), "{ x: undefined; }");
    // A non-nullable member is unaffected — the guard must not be a blanket one.
    assert_eq!(type_of_initialiser("var c = { x: 1 };"), "{ x: number; }");
}

#[test]
fn an_object_literal_type_carries_the_symbol_a_property_access_looks_in() {
    // Without it `o.a` cannot resolve, which is the point of computing the type.
    let source = "const o = { a: 1 };";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    let ty = checker.type_of(id);
    assert!(ty.flags.contains(TypeFlags::OBJECT), "an object literal type is an object type");
    let tsr_checker::TypeData::Named { members, .. } = &ty.data else {
        panic!("an object literal type prints structurally, like a type literal");
    };
    assert!(members.is_some(), "and carries the symbol its properties live in");
}

#[test]
fn a_method_member_prints_as_a_signature() {
    // `m(): void`, not `m: () => void` — the distinction `Member`'s own doc
    // calls out and the reason `Member::Signature` exists. The corpus prints
    // 3,012 object types carrying a method; `{ fn(): void; }` (68 instances)
    // and `{ log(msg: any): void; }` (366) are the head of that population.
    assert_eq!(type_of_initialiser("const o = { m() {} };"), "{ m(): void; }");
    assert_eq!(type_of_initialiser("const o = { m() { return 1; } };"), "{ m(): number; }");
    assert_eq!(
        type_of_initialiser("const o = { log(msg: any): void {} };"),
        "{ log(msg: any): void; }"
    );
    // Beside a property, so the two member spellings are rendered by one pass
    // and the ordering is the literal's own.
    assert_eq!(type_of_initialiser("const o = { a: 1, m() {} };"), "{ a: number; m(): void; }");
}

/// [`type_of_initialiser_at`] for a fixture that is DELIBERATELY not
/// well-formed.
///
/// A private name in an object literal is a grammar error; the parser reports
/// and the member is not a member. The point of §201 is that the *type* answer
/// survives the syntax error, so the fixture cannot be required to parse
/// cleanly.
fn type_of_first_initialiser_allowing_parse_errors(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("statement 0 must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// §201. A member that cannot name a property contributes NOTHING — not a
/// member, and not a gap.
///
/// `checkObjectLiteral` (`checker.go:13317-13332`) puts a computed-name member
/// in `propertiesTable` only when its name type carries
/// `StringOrNumberLiteralOrUnique`; otherwise it either sets an index-signature
/// flag or drops the member. A private name never reaches the table at all.
/// This port gapped the whole literal for both, turning a reported syntax error
/// into a second, silent type failure.
#[test]
fn a_member_that_cannot_name_a_property_leaves_the_literal_empty() {
    // A `boolean` computed name: not a literal, not assignable to
    // `string | number | symbol`. `conformance/parserComputedPropertyName41`.
    assert_eq!(type_of_initialiser_at("var v = { [0 in []]: true };", 0), "{}");
    // `conformance/privateNameInObjectLiteral-1` and `-2`.
    assert_eq!(type_of_first_initialiser_allowing_parse_errors("var o = { #foo: 1 };"), "{}");
    assert_eq!(type_of_first_initialiser_allowing_parse_errors("var o = { #foo() {} };"), "{}");
}

// Component expectations corrected against pinned native declaration emit in
// /tmp/tsr-99-index-unit-controls.ts. The semantic indexes previously printed
// synthesized signatures because component serialization was not yet ported.
/// The control that keeps §201 honest: a name type that CAN key a property must
/// not vanish. Without it, "drop every computed member" also passes the test
/// above.
///
/// It asserted `error` at §201, when the index signature was a named gap, and
/// asserts the signature itself since §206 built it. The *purpose* is unchanged
/// — the member must be represented somehow — which is why the fixture was
/// re-pointed rather than deleted.
///
/// **This control did its job twice.** §206's first draft classified the key by
/// `NUMBER_LIKE`, which contains `NUMBER_LITERAL`, and so turned the late-bound
/// `{ [1]: 1 }` into `{ [x: number]: number; }`. The neighbouring fixture
/// caught it: upstream tests `StringOrNumberLiteralOrUnique` **first**
/// (`checker.go:13317`), and a literal-typed name is a real member, not a
/// signature.
#[test]
fn a_string_like_computed_name_becomes_an_index_signature_rather_than_vanishing() {
    assert_eq!(
        type_of_initialiser_at("declare const k: string;\nvar v = { [k]: 1 };", 1),
        "{ [k]: number; }"
    );
    assert_eq!(
        type_of_initialiser_at("declare const n: number;\nvar v = { [n]: 1 };", 1),
        "{ [n]: number; }"
    );
    // And a LITERAL-typed name is late-bound: a real member whose printing is
    // unported, so it is still a gap and must not be swept into a signature.
    // **Came due at §553** — see the same-shaped assertion earlier in this
    // file. A literal-typed name is late-bound AND now printable; it is still
    // not swept into a signature, which is what this line really guards.
    assert_eq!(type_of_initialiser_at("var v = { [1]: 1 };", 0), "{ 1: number; }");
}

/// §206. A computed name that CAN key a property contributes an index
/// signature.
///
/// `checkObjectLiteral` (`checker.go:13195-13205`) appends one index info per
/// key kind, whose value type is the union of the contributing members'
/// (`getObjectLiteralIndexInfo`, `:19721`). §201 dropped these members and left
/// the signature as a named gap; this is that gap.
///
/// **The key-kind order is upstream's and it is not the obvious one**:
/// `isTypeAssignableTo(nameType, numberType)` is asked FIRST
/// (`checker.go:13319`), so an `any`-typed name yields a **number** index. That
/// is why `{ [await]: foo }` with an un-typeable `await` records
/// `{ [x: number]: any; }` in `conformance/asyncFunctionDeclaration8_es6`.
#[test]
fn a_computed_name_that_can_key_a_property_makes_an_index_signature() {
    assert_eq!(
        type_of_initialiser_at("declare const k: number;\nvar v = { [k]: 1 };", 1),
        "{ [k]: number; }"
    );
    assert_eq!(
        type_of_initialiser_at("declare const k: string;\nvar v = { [k]: 1 };", 1),
        "{ [k]: number; }"
    );
    // An `any` name takes the NUMBER arm, not the string one.
    assert_eq!(
        type_of_initialiser_at("declare const k: any;\nvar v = { [k]: 1 };", 1),
        "{ [k]: number; }"
    );
}

/// Components retain their individual declaration types. The pipeline tests
/// separately verify that indexing returns the union of all contributors.
#[test]
fn index_components_retain_every_contributing_declaration() {
    assert_eq!(
        type_of_initialiser_at(
            "declare const j: number;\ndeclare const k: number;\nvar v = { [j]: 1, [k]: \"s\" };",
            2
        ),
        "{ [j]: number; [k]: string; }"
    );
}

/// Mixed index kinds filter the complete property array. String indexes also
/// contain number components, so the numeric component displays twice.
#[test]
fn a_mixed_literal_keeps_components_in_index_order() {
    assert_eq!(
        // Named nonnumeric properties do not contribute to the number index.
        type_of_initialiser_at("declare const k: number;\nvar v = { a: 1, [k]: 2 };", 1),
        "{ [k]: number; a: number; }"
    );
    assert_eq!(
        type_of_initialiser_at(
            "declare const j: number;\ndeclare const k: string;\nvar v = { [j]: 1, [k]: 2 };",
            2
        ),
        "{ [j]: number; [k]: number; [j]: number; }"
    );
}

/// §890: `checkExpressionForMutableLocation` (`checker.go:13878`) has three
/// branches and this port wrote only the third, with `nil` hardcoded for the
/// contextual type — so every object-literal member widened unconditionally.
///
/// Branch one: a CONST CONTEXT keeps the literal.
#[test]
fn a_const_context_keeps_a_members_literal_type() {
    assert_eq!(type_of_initialiser("const o = { a: 1 } as const;"), "{ readonly a: 1; }");
}

/// Branch two: a TYPE ASSERTION returns the checked type untouched.
/// `isTypeAssertion` is `IsAssertionExpression(SkipParentheses(node))`.
#[test]
fn a_type_assertion_member_is_returned_untouched() {
    assert_eq!(type_of_initialiser("const o = { a: 1 as 1 };"), "{ a: 1; }");
}

/// Branch three with a real contextual type: `isLiteralOfContextualType` keeps a
/// literal whose contextual type is a literal of the same flavour.
#[test]
fn a_literal_contextual_type_keeps_the_members_literal() {
    let source = "const o: { a: \"x\" | \"y\" } = { a: \"x\" };";
    assert_eq!(type_of_initialiser(source), "{ a: \"x\"; }");
}

/// The control, and the rule the module header states: freshness stops at the
/// property boundary. With no const context, no assertion and no literal in the
/// contextual type, the member still widens.
#[test]
fn a_member_with_no_literal_context_still_widens() {
    assert_eq!(type_of_initialiser("const o: { a: number } = { a: 1 };"), "{ a: number; }");
}

/// Type the last expression statement in the fixture.
fn type_of_last_expression_statement(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut last = None;
    for statement in parsed.source_file.statements {
        if let Statement::ExpressionStatement(node) = statement {
            last = node.expression;
        }
    }
    let id = checker.check_expression(last.expect("an expression statement"));
    checker.type_to_string(id)
}

/// §892: an object-literal property's type is recorded on its SYMBOL as the
/// literal computes it — what upstream's `checkObjectLiteral` does through
/// `links.resolvedType`. Without it, `getTypeOfSymbol` RECOMPUTES the member,
/// and the two roads did not agree: caching measured **41 W→R against 2 R→W**,
/// so the literal's own computation is the more often correct one.
///
/// The invariant, stated as a test: **reading a property back gives what the
/// literal recorded for it.** The fixture is deliberately un-annotated, so both
/// sides are the literal's own answer — with an annotation the read would give
/// the DECLARED type (`{ a: "x" | "y" }` makes `o.a` be `"x" | "y"`), which is
/// correct and tests nothing here.
///
/// This harness has no `lib.d.ts` and cannot express the shapes that actually
/// moved — `const` type parameters reached through JSDoc. It pins the mechanism;
/// **the corpus pinned the gain**, the same division `uninitialized_reads_declared.rs`
/// records.
#[test]
fn reading_a_property_back_gives_what_the_literal_recorded() {
    let source = "const o = { a: \"x\" } as const;\no.a;";
    assert_eq!(type_of_last_expression_statement(source), "\"x\"");
}

/// §897: `contextualTypeHasPattern` (`checker.go:13252`) where the pattern is an
/// **assignment** pattern. The right-hand literal is typed against the left-hand
/// pattern, whose defaulted properties are optional (`checker.go:13248`), so the
/// right's copy that optionality.
///
/// §489 ported this copy for BINDING patterns; only its search was
/// binding-shaped. An assignment yields its right-hand type, so typing the whole
/// assignment is typing the right literal.
///
/// Corpus effect: `WRONG->RIGHT 72`, zero adverse —
/// `sourceMapValidationDestructuringForObjectBindingPatternDefaultValues2` 48,
/// `shorthandPropertyAssignmentsInDestructuring_ES6` 18. The two cases three
/// earlier entries aimed at and missed.
#[test]
fn an_assignment_pattern_makes_the_matching_member_optional() {
    let source = "let x;\nconst r = ({ a: x = 1 } = { a: 2 });";
    assert_eq!(type_of_initialiser_at(source, 1), "{ a?: number; }");
}

/// The shorthand spelling carries its default in `objectAssignmentInitializer`
/// rather than in an `=` binary, and counts the same.
#[test]
fn a_shorthand_default_in_an_assignment_pattern_counts() {
    let source = "let a;\nconst r = ({ a = 1 } = { a: 2 });";
    assert_eq!(type_of_initialiser_at(source, 1), "{ a?: number; }");
}

/// The control: an assignment pattern with NO default leaves the member
/// required. The arm must key on the default, not on the position.
#[test]
fn an_assignment_pattern_without_a_default_leaves_the_member_required() {
    let source = "let x;\nconst r = ({ a: x } = { a: 2 });";
    assert_eq!(type_of_initialiser_at(source, 1), "{ a: number; }");
}

/// §898: `getWidenedUniqueESSymbolType` (`checker.go:25505`) — a `unique symbol`
/// widens to plain `symbol` at a mutable location. Upstream has exactly one call
/// site, `getWidenedLiteralLikeTypeForContextualType` (`checker.go:25517`),
/// where it pairs with `getWidenedLiteralType`.
///
/// Corpus effect: `WRONG->RIGHT 16`, zero adverse.
#[test]
fn a_unique_symbol_widens_at_a_mutable_location() {
    let source = "declare const s: unique symbol;\nconst o = { a: s };";
    assert_eq!(type_of_initialiser_at(source, 1), "{ a: symbol; }");
}

/// §899: `getESSymbolLikeTypeForNode` (`checker.go:22982`) mints the unique type
/// only in a valid declaration position — `isValidESSymbolDeclaration`
/// (`checker/utilities.go:961`) — and answers plain `symbol` elsewhere.
///
/// A `const` in a variable statement is valid.
#[test]
fn a_unique_symbol_on_a_const_keeps_its_unique_type() {
    assert_eq!(
        type_of_last_expression_statement("declare const y: unique symbol;\ny;"),
        "unique symbol"
    );
}

/// A `let` is not: `IsVarConst` fails and the type is plain `symbol`.
#[test]
fn a_unique_symbol_on_a_let_is_plain_symbol() {
    assert_eq!(type_of_last_expression_statement("let x: unique symbol;\nx;"), "symbol");
}

/// A parameter is not a valid `unique symbol` declaration
/// (`isValidESSymbolDeclaration`), so its type is plain `symbol`
/// (`getESSymbolLikeTypeForNode`, `checker.go:22982`). Away from the written
/// annotation, that is what prints. Native, at the pinned commit:
/// `declare function f(arg: unique symbol): void; const n: number = f;`
/// reports TS2322 "Type '(arg: symbol) => void' is not assignable to type
/// 'number'". `(arg: unique symbol)` appears only where the printer may reuse
/// the written node, inside the declaration itself (`nodecopy.go:596`;
/// uniqueSymbolsErrors.types:17). This test used to pin the old per-node mint's
/// `unique symbol`; r5-instexpr's declaration-keyed identity (tsr-2zk.1005)
/// corrected it.
#[test]
fn a_parameter_is_not_a_unique_symbol_position() {
    assert_eq!(
        type_of_last_expression_statement("declare function f(arg: unique symbol): void;\nf;"),
        "(arg: symbol) => void"
    );
}

/// §901: `resolveUntypedCall` (`checker.go:9899`) — a tagged template whose TAG
/// is `any` resolves to `anySignature`, so the whole expression is `any`.
///
/// The call road has had this predicate since `checker-notes-calleegap.md`
/// (`is_untyped_call_target`); the tagged-template road went straight to
/// `resolve_call_signature`, which answers `None` for `any`, and gapped.
///
/// Corpus effect: `GAP->RIGHT 32`, zero adverse — `taggedTemplateStringsWithTagsTypedAsAny`
/// and its ES6 twin closed **entirely**, 16 rows each.
#[test]
fn a_tagged_template_with_an_any_tag_is_any() {
    assert_eq!(type_of_last_expression_statement("var f: any;\nf `abc`;"), "any");
}

/// The same through a substitution, and through a property access on the result
/// — the shapes the two cases actually hold.
#[test]
fn an_any_tag_survives_substitutions_and_access() {
    assert_eq!(type_of_last_expression_statement("var f: any;\nf `abc${1}def`;"), "any");
    assert_eq!(type_of_last_expression_statement("var f: any;\nf `abc`.member;"), "any");
}

/// §905: a MAPPED TYPE mints a PRINT-ONLY type carrying its written form, where
/// this port has no mapped-type subsystem at all (`members.rs` records
/// `ObjectFlagsMapped` as "not ported at all").
///
/// Upstream keeps a generic mapped type DEFERRED and its node builder prints it
/// from its own parts, which for an unevaluated mapped type are exactly the
/// written ones. `signatures.rs`'s §77 renderer already produced that spelling;
/// the mint is that renderer plus `new_named`, so the type EXISTS where it used
/// to be `errorType` and can be carried by whatever holds it.
///
/// These tests pin the SPELLING, which is what the mint emits. **The corpus
/// pinned the mint**: +416 net — 210 `WRONG->RIGHT`, 208 `GAP->RIGHT` against 92
/// `GAP->WRONG` and 3 `RIGHT->WRONG`, the largest single change of the session.
#[test]
fn a_mapped_type_prints_its_written_form() {
    let source = "declare function f<T>(x: { [P in keyof T]: T[P] }): void;\nf;";
    assert_eq!(
        type_of_last_expression_statement(source),
        "<T>(x: { [P in keyof T]: T[P]; }) => void"
    );
}

/// The modifiers travel with it: `readonly`, `?`, and their `+`/`-` spellings.
#[test]
fn a_mapped_types_modifiers_are_written_through() {
    let source = "declare function f<T>(x: { readonly [P in keyof T]?: T[P] }): void;\nf;";
    assert_eq!(
        type_of_last_expression_statement(source),
        "<T>(x: { readonly [P in keyof T]?: T[P]; }) => void"
    );
}

/// An `as` clause — key remapping — is part of the written form.
#[test]
fn a_key_remapping_as_clause_is_written_through() {
    let source = "declare function f<T>(x: { [P in keyof T as P]: T[P] }): void;\nf;";
    assert_eq!(
        type_of_last_expression_statement(source),
        "<T>(x: { [P in keyof T as P]: T[P]; }) => void"
    );
}

/// §906: a CONDITIONAL type mints a print-only type carrying its written form —
/// the same slice §905 took for mapped types. Upstream keeps a conditional whose
/// check type is generic DEFERRED and prints it from its parts.
///
/// Corpus effect: +117 net — 69 `WRONG->RIGHT`, 46 `GAP->RIGHT`, against 28
/// `GAP->WRONG` and 3 `RIGHT->WRONG`.
#[test]
fn a_conditional_type_prints_its_written_form() {
    let source = "declare function f<T>(x: T): T extends string ? 1 : 2;\nf;";
    assert_eq!(type_of_last_expression_statement(source), "<T>(x: T) => T extends string ? 1 : 2");
}

/// `infer T` inside the extends clause travels with it.
#[test]
fn an_infer_clause_is_written_through() {
    let source = "declare function f<T>(x: T): T extends (infer U)[] ? U : never;\nf;";
    assert_eq!(
        type_of_last_expression_statement(source),
        "<T>(x: T) => T extends (infer U)[] ? U : never"
    );
}

/// A CONSTRAINED infer (`infer U extends string`) is declined rather than
/// guessed — the renderer's admission set stays bounded, which is §77's rule.
#[test]
fn a_constrained_infer_declines() {
    let source =
        "declare function f<T>(x: T): T extends (infer U extends string)[] ? U : never;\nf;";
    assert_ne!(
        type_of_last_expression_statement(source),
        "<T>(x: T) => T extends (infer U extends string)[] ? U : never"
    );
}

/// §909: a mapped type that is the body of a NON-GENERIC type alias prints the
/// ALIAS NAME, not the body. `type T12 = { readonly [P in keyof Item]: Item[P] }`
/// records `>T12 : T12` — upstream carries an `aliasSymbol` on the type and its
/// node builder names it.
///
/// §905's mint printed the body everywhere, which is right at an anonymous site
/// and wrong at a named one — 21 of `conformance/mappedTypes1`'s rows.
///
/// Corpus effect: +102 — 81 `WRONG->RIGHT`, 22 `GAP->RIGHT` against 2 `GAP->WRONG`
/// and 1 `RIGHT->WRONG`.
#[test]
fn a_non_generic_mapped_alias_prints_its_name() {
    let source = "type Item = { a: number };\n\
         type T12 = { readonly [P in keyof Item]: Item[P] };\n\
         declare const x: T12;\nx;";
    assert_eq!(type_of_last_expression_statement(source), "T12");
}

/// A GENERIC alias is instantiated per reference and upstream prints the
/// instantiated body, which is what §905's 63-row gain in
/// `mappedTypeRelationships` is made of — so the naming must not reach it.
#[test]
fn a_generic_mapped_alias_still_expands() {
    let source = "declare function f<T>(x: { [P in keyof T]: T[P] }): void;\nf;";
    assert_eq!(
        type_of_last_expression_statement(source),
        "<T>(x: { [P in keyof T]: T[P]; }) => void"
    );
}

/// A CONDITIONAL alias is excluded: naming those measured 14 `RIGHT->WRONG`
/// (`conditionalTypes1` 7), because §92's alias evaluation owns that road and a
/// name in place of the evaluated branch is a wrong answer.
#[test]
fn a_non_generic_conditional_alias_is_not_named() {
    let source = "type C = string extends string ? 1 : 2;\ndeclare const c: C;\nc;";
    assert_ne!(type_of_last_expression_statement(source), "C");
}

/// Type the LAST object literal in the fixture, wherever it sits — an argument
/// literal is not reachable through an initialiser.
fn type_of_last_object_literal(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut last = None;
    for raw in 0..u32::try_from(parsed.nodes.len()).expect("fits") {
        let id = tsr_ast::NodeId::new(raw);
        if parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ObjectLiteralExpression {
            last = Some(id);
        }
    }
    let id = last.expect("the fixture must contain an object literal");
    let node = parsed.node_map.get(id).expect("the literal is in the map");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let ty = checker.check_expression(expression);
    checker.type_to_string(ty)
}

/// §910: §890's call-argument exclusion narrowed from "anywhere under a call" to
/// "under a NESTED object literal in a call argument".
///
/// Every row §890 lost was a member of `context: { tag: "A", value: 1 }` — a
/// literal INSIDE the argument literal — and the re-entry needs that second
/// level: checking the inner literal asks for its contextual type, which asks
/// for the outer literal's, which is the argument whose signature is being
/// resolved. A member of the argument literal ITSELF is one hop short of the
/// cycle.
///
/// Corpus effect: 81 `WRONG->RIGHT` against 3 `RIGHT->WRONG`, including
/// `compiler/temporal` 63 — blocked since §890.
#[test]
fn an_argument_literals_own_member_keeps_its_contextual_literal() {
    let source = "declare function f(o: { largestUnit: \"hour\" | \"minute\" }): void;\n         f({ largestUnit: \"hour\" });";
    assert_eq!(type_of_last_object_literal(source), "{ largestUnit: \"hour\"; }");
}

/// The control, and it needed correcting from what I first assumed: a member of
/// a NESTED literal inside a call argument is excluded from **branch 3**, and it
/// still keeps its literal — through §56's `annotation_member_context`, a
/// different road that reads the written annotation directly.
///
/// So the exclusion is narrower than "these members widen": it only keeps branch
/// 3 from *asking for a contextual type* at a position where that question
/// re-enters. §56 answers the same question from the annotation without asking.
#[test]
fn a_nested_literal_in_an_argument_keeps_its_literal_by_another_road() {
    let source = "declare function f(o: { inner: { k: \"a\" | \"b\" } }): void;\n\
         f({ inner: { k: \"a\" } });";
    assert_eq!(type_of_last_object_literal(source), "{ inner: { k: \"a\"; }; }");
}

/// Type the last `this` expression in the fixture.
fn type_of_last_this(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        no_implicit_this: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let mut last = None;
    for raw in 0..u32::try_from(parsed.nodes.len()).expect("fits") {
        let id = tsr_ast::NodeId::new(raw);
        if parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ThisKeyword {
            last = Some(id);
        }
    }
    let id = last.expect("the fixture must contain `this`");
    let ty = checker.check_this_expression_for_test(id);
    checker.type_to_string(ty)
}

/// §912: `getContextualThisParameterType`'s `noImplicitThis` branch — `this`
/// inside an object-literal method whose literal has a UNION contextual type is
/// that union **discriminated by the literal's own members**.
///
/// §911 built this answering the whole union and measured 4 `RIGHT->WRONG` plus
/// 5 `GAP->WRONG` **in the very case it was meant to fix**.
/// `discriminate_union_root` has done that selection since §750's family and was
/// never called from here — the tenth instance this session of a capability
/// present and a caller that does not consult it.
///
/// Corpus effect: 18 `WRONG->RIGHT`, 6 `GAP->RIGHT` against 4 `GAP->WRONG`, zero
/// `RIGHT->WRONG`.
#[test]
fn this_in_an_object_literal_method_is_the_discriminated_contextual_type() {
    let source = "interface X { type: \"x\"; value: string; method(): void; }\n\
         interface Y { type: \"y\"; value: number; method(): void; }\n\
         declare function foo(bar: X | Y): void;\n\
         foo({ type: \"y\", value: 1, method() { this; } });";
    assert_eq!(type_of_last_this(source), "Y");
}

/// getContextualThisParameterType uses a plain contextual object after the
/// signature's explicit this slot. The former union-only restriction protected
/// the then-missing index-signature lookup and is no longer needed.
#[test]
fn a_non_union_contextual_type_supplies_this() {
    let source = "interface I { [k: string]: any; method(): void; }\n\
         declare function foo(bar: I): void;\n\
         foo({ method() { this; } });";
    assert_eq!(type_of_last_this(source), "I");
}

/// §914: a tagged template's WRITTEN type arguments instantiate the return, as a
/// call's do. This road used to reject the whole expression the moment it saw
/// any — `!node.type_arguments.is_empty() → error` — before resolving anything.
#[test]
fn a_tagged_templates_written_type_arguments_instantiate_the_return() {
    let source = "declare function f<T>(s: TemplateStringsArray): T;\n\
         interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
         f<number>`x`;";
    assert_eq!(type_of_last_expression_statement(source), "number");
}

/// §914: an OVERLOADED tag is selected by ARITY. A tagged template's argument
/// count is `1 + spans` — a synthetic `TemplateStringsArray` plus one per
/// substitution — and that count needs no synthetic expression, which is what
/// made the selection reachable while argument CHECKING still is not.
#[test]
fn an_overloaded_tag_is_selected_by_arity() {
    let source = "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
         declare function f(s: TemplateStringsArray): string;\n\
         declare function f(s: TemplateStringsArray, a: number): boolean;\n\
         f`x${1}y`;";
    assert_eq!(type_of_last_expression_statement(source), "boolean");
}

/// The zero-substitution form takes the one-parameter overload.
#[test]
fn an_overloaded_tag_with_no_substitutions_takes_arity_one() {
    let source = "interface TemplateStringsArray { readonly raw: readonly string[]; }\n\
         declare function f(s: TemplateStringsArray): string;\n\
         declare function f(s: TemplateStringsArray, a: number): boolean;\n\
         f`x`;";
    assert_eq!(type_of_last_expression_statement(source), "string");
}

/// §922: `new D()` where `D` is a class MERGED with a namespace.
///
/// One symbol carries both declarations and the namespace comes first in source
/// order, so `declarations.first()` matched a `ModuleDeclaration`, fell to the
/// `_` arm and answered `error` — while every unmerged form worked.
/// `getDeclaredTypeOfSymbol` reads the class declaration wherever it sits, so
/// the fix is to search for it.
///
/// **Found by probing five `new` shapes at once**, which isolated the one that
/// fails; reading the arm would not have shown it, because the arm is correct
/// and its input was wrong.
///
/// Corpus effect: 71 `WRONG->RIGHT`, 9 `GAP->RIGHT`, zero adverse —
/// `cloduleTest2` 12, `interfaceClassMerging2` 10, `targetTypeTest1` 10.
#[test]
fn new_on_a_class_merged_with_a_namespace() {
    let source = "declare class C {}\n         namespace D { var x: number; }\n         declare class D extends C {}\n         new D();";
    assert_eq!(type_of_last_expression_statement(source), "D");
}

/// The unmerged forms, which always worked and are what hid it.
#[test]
fn new_on_unmerged_classes_is_unchanged() {
    assert_eq!(type_of_last_expression_statement("class C {}\nnew C();"), "C");
    assert_eq!(type_of_last_expression_statement("declare class D {}\nnew D();"), "D");
    assert_eq!(
        type_of_last_expression_statement(
            "declare class C {}\ndeclare class D extends C {}\nnew D();"
        ),
        "D"
    );
}

/// §925: a qualified type name whose root is an ENCLOSING namespace.
///
/// `qualified_type_reference` opened with an **undocumented** gate —
/// `if self.site_is_inside_namespace(site, namespace) { return error }` — so
/// `c.K` written inside `namespace c` answered `error` while the identical
/// reference from outside answered correctly. Upstream has no such rule:
/// `resolveEntityName` walks the scope chain, and a namespace is in scope inside
/// itself.
///
/// Corpus effect on removal: **320 `WRONG->RIGHT` + 92 `GAP->RIGHT`** against 48
/// `GAP->WRONG` and 1 `RIGHT->WRONG` — `bluebirdStaticThis` 69,
/// `complexRecursiveCollections` 51,
/// `resolvingClassDeclarationWhenInBaseTypeResolution` 51.
#[test]
fn a_qualified_name_rooted_at_an_enclosing_namespace_resolves() {
    // From INSIDE the namespace — the shape the gate declined.
    let inside = "namespace c { export class K {} export interface I { m(p: c.K): void } }\n\
         declare const v: c.I;\nv.m;";
    assert_eq!(type_of_last_expression_statement(inside), "(p: c.K) => void");
    // From a NESTED namespace, which the gate also declined.
    let nested = "namespace e { export class K {} \
         export namespace f { export interface I { m(p: e.K): void } } }\n\
         declare const v: e.f.I;\nv.m;";
    assert_eq!(type_of_last_expression_statement(nested), "(p: e.K) => void");
}

/// The cross-namespace form, which always worked and is what hid the gate.
#[test]
fn a_qualified_name_across_namespaces_is_unchanged() {
    let source = "namespace a { export class K {} }\n\
         namespace b { export interface I { m(p: a.K): void } }\n\
         declare const v: b.I;\nv.m;";
    assert_eq!(type_of_last_expression_statement(source), "(p: a.K) => void");
}
