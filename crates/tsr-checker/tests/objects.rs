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
    // A method whose *signature* cannot be built keeps the whole literal a
    // gap, which is the property the removed line was really testing.
    assert_eq!(type_of_initialiser("const o = { m(x: keyof string) {} };"), "error");
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
        "{ [x: string]: number; }"
    );
    assert_eq!(
        type_of_initialiser_at("declare const n: number;\nvar v = { [n]: 1 };", 1),
        "{ [x: number]: number; }"
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
        "{ [x: number]: number; }"
    );
    assert_eq!(
        type_of_initialiser_at("declare const k: string;\nvar v = { [k]: 1 };", 1),
        "{ [x: string]: number; }"
    );
    // An `any` name takes the NUMBER arm, not the string one.
    assert_eq!(
        type_of_initialiser_at("declare const k: any;\nvar v = { [k]: 1 };", 1),
        "{ [x: number]: number; }"
    );
}

/// The value is the union of the contributors, not the first of them.
#[test]
fn the_index_value_unions_every_contributing_member() {
    assert_eq!(
        type_of_initialiser_at(
            "declare const j: number;\ndeclare const k: number;\nvar v = { [j]: 1, [k]: \"s\" };",
            2
        ),
        "{ [x: number]: string | number; }"
    );
}

/// The two shapes slice 1 deliberately still gaps, and they are controls rather
/// than decoration: a literal mixing NAMED and computed members needs
/// `getObjectLiteralIndexInfo`'s name filter, and one mixing KEY KINDS needs
/// upstream's string/number/symbol emission order. Guessing either prints a
/// plausible wrong line.
#[test]
fn a_mixed_literal_still_gaps() {
    assert_eq!(
        // **Came due at §551.** Upstream does not decline a mixed literal:
        // `getObjectLiteralIndexInfo` (`checker.go:19721`) filters
        // `propertiesArray` by whether each property's name suits the key, and
        // for a NUMBER key that is the numerically-named members only — so `a`
        // stays a PROPERTY and contributes nothing to the index value. §206's
        // stated reason (*"no numeric-name predicate for a written name"*) was
        // stale: `printing::normalise_number` normalises written numeric names
        // a few lines from the mint. The STRING-key half and mixed key kinds
        // still decline, each for its own recorded reason.
        // §593 CORRECTED the ORDER in this expectation, which was written by
        // hand and put the index last. An anonymous object prints its index
        // signatures **before** its properties whatever the source order:
        // `computedPropertyNames49_ES5` writes `{ p1: 10, get [1 + 1]() {…}, …
        // p2: 20 }` — `p1` first — and its baseline records
        // `{ [x: number]: any; p1: number; readonly foo: number; p2: number; }`.
        // `get_type_from_type_literal` had the group order right all along
        // (`signatures, indexes, properties`); the object-literal road pushed
        // the index onto the end, and this assertion pinned that.
        type_of_initialiser_at("declare const k: number;\nvar v = { a: 1, [k]: 2 };", 1),
        "{ [x: number]: number; a: number; }"
    );
    assert_eq!(
        type_of_initialiser_at(
            "declare const j: number;\ndeclare const k: string;\nvar v = { [j]: 1, [k]: 2 };",
            2
        ),
        "error"
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

/// The predicate declines only where it can SEE an invalid declaration. An
/// unrecognised shape keeps the unique type, because this walk climbs a
/// syntactic parent chain that a JSDoc `@type` does not share — treating
/// "not recognised" as invalid cost 6 `RIGHT→WRONG` in `compiler/uniqueSymbolJs2`.
#[test]
fn an_unrecognised_position_keeps_the_unique_type() {
    assert_eq!(
        type_of_last_expression_statement("declare function f(arg: unique symbol): void;\nf;"),
        "(arg: unique symbol) => void"
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
