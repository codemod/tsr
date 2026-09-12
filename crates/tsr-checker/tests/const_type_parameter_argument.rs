//! An array literal argument of a `const` type parameter is in a CONST
//! CONTEXT. §797.
//!
//! ```ts
//! declare function f<const T>(x: T): T;
//! f(["b", "c"]);   // readonly ["b", "c"], not string[]
//! ```
//!
//! # Two changes, and the second is a REFUSAL being narrowed
//!
//! **The arm**: `is_const_context` walks the parent chain syntactically and
//! cannot answer this, because const-ness here depends on the callee's
//! RESOLVED signature. `array_literal_argument_of_const_type_parameter` asks it
//! at the same seam §793 used for the tuple-context arm, behind the
//! `resolving_signature_calls` re-entry guard.
//!
//! **The decline**: §33 refused EVERY call through a const-marked signature,
//! because this port's inference widens where upstream keeps literals — 70
//! GAP→WRONG when it was written. STATUS §5's §796 entry re-measured that at
//! **42**, then built the arm above and showed the 42 are *all* one shape:
//!
//! ```ts
//! declare function test1<const T>(create: () => T): T;
//! test1(() => ['a']);   // readonly ["a"]
//! ```
//!
//! There const-ness must cross a FUNCTION BOUNDARY into the arrow's return
//! before the literal is reached, and nothing in this port carries a const
//! context across one. So the decline now asks for exactly that shape — a
//! parameter whose type is a function mentioning a const type parameter — and
//! every other const-marked call goes through inference with its arguments
//! correctly in const context.
//!
//! **A refusal narrowed to its actual cause is worth more than a refusal
//! lifted or kept whole.** Lifting it measured 42 GAP→WRONG against 8; keeping
//! it whole left those 8 unreachable; narrowing it is +8 with zero adverse.
//!
//! # The residue this recorded is now CLOSED (§798)
//!
//! Upstream distinguishes the LITERAL's own line from the CALL's:
//! `['a', ['b', 'c']]` is `["a", ["b", "c"]]` (a tuple, NOT readonly) while the
//! call is `readonly ["a", readonly ["b", "c"]]` — the readonly comes from
//! `getWidenedType` over the const type variable, not from the literal.
//!
//! §797 minted the readonly AT THE LITERAL, which made the call lines right and
//! the literal lines wrong. §798 moved it: the literal now mints a plain tuple
//! when its const context came from a const TYPE PARAMETER (an `as const`
//! assertion still keeps the readonly there), and
//! `Checker::readonly_tuple_image` applies it in the inference resolution loop
//! instead. That is **+43 on top of §797's +14**, and it also converted 14
//! lines in `conformance/jsdocTemplateTag6` that had nothing to do with the
//! case it was built for.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's initialiser.
fn type_of_initialiser(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|list| list.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let initialiser = declaration.initializer.expect("an initialiser");
            let id = checker.check_expression(initialiser);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

const LIB: &str = "interface Array<T> { length: number }\n\
                   interface ReadonlyArray<T> { length: number }\n";

/// The head case, from `conformance/typeParameterConstModifiers` — the CALL's
/// type. `f1(['a', ['b', 'c']])` records
/// `readonly ["a", readonly ["b", "c"]]` there (`:15-:16`). Since §798 the
/// readonly is applied at the inference site rather than at the literal.
#[test]
fn a_const_type_parameter_keeps_an_array_argument_as_a_readonly_tuple() {
    let source =
        format!("{LIB}declare function f<const T>(x: T): T;\nconst a = f([\"b\", \"c\"]);");
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [\"b\", \"c\"]");
}

/// Nested literals are const all the way down, which is what makes this the
/// const CONTEXT rather than a top-level readonly wrapper.
#[test]
fn the_const_context_reaches_nested_literals() {
    let source = format!(
        "{LIB}declare function f<const T>(x: T): T;\nconst a = f([\"a\", [\"b\", \"c\"]]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "readonly [\"a\", readonly [\"b\", \"c\"]]");
}

/// The control: a PLAIN type parameter still widens, which is what makes the
/// `const` modifier observable at all.
#[test]
fn a_plain_type_parameter_still_widens() {
    let source = format!("{LIB}declare function f<T>(x: T): T;\nconst a = f([\"b\", \"c\"]);");
    assert_eq!(type_of_initialiser(&source, "a"), "string[]");
}

/// The decline that survives: a CALLBACK parameter keeps §33's refusal,
/// because const-ness would have to cross the function boundary. Answering
/// here would put 42 GAP→WRONG back on the board.
#[test]
fn a_callback_shaped_const_signature_still_declines() {
    let source = format!(
        "{LIB}declare function test1<const T>(create: () => T): T;\n\
         const a = test1(() => [\"a\"]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "error");
}

/// A non-array argument through a const parameter is unaffected either way —
/// the arm is scoped to array literals, which is where the widening happened.
#[test]
fn a_scalar_argument_through_a_const_parameter_resolves() {
    let source = format!("{LIB}declare function f<const T>(x: T): T;\nconst a = f(\"b\");");
    assert_eq!(type_of_initialiser(&source, "a"), "\"b\"");
}

/// §798: the LITERAL's own line keeps a plain tuple. Upstream records
/// `['a', ['b', 'c']] : ["a", ["b", "c"]]` at `:18` — the readonly belongs to
/// the inferred type variable, not to `checkArrayLiteral`.
#[test]
fn the_literals_own_type_is_not_readonly() {
    let source = "interface Array<T> { length: number }\n\
                  interface ReadonlyArray<T> { length: number }\n\
                  declare function f<const T>(x: T): T;\n\
                  const a = [\"b\", \"c\"];\nf(a);";
    // The literal bound to its own variable never reaches the const context.
    assert_eq!(type_of_initialiser(source, "a"), "string[]");
}

/// An `as const` assertion still puts the readonly at the literal — the two
/// roads stayed separate.
#[test]
fn an_as_const_assertion_still_reads_readonly_at_the_literal() {
    let source = "interface Array<T> { length: number }\n\
                  interface ReadonlyArray<T> { length: number }\n\
                  const a = [\"b\", \"c\"] as const;";
    assert_eq!(type_of_initialiser(source, "a"), "readonly [\"b\", \"c\"]");
}

/// §799: an OBJECT literal argument of a `const` type parameter gets the
/// `readonly` at the CALL, the same as a tuple does.
///
/// **A known half-answer, pinned as it is rather than as it should be.**
/// Upstream records `{ readonly a: 1; readonly b: "x"; }`; this port keeps the
/// `readonly` and WIDENS the members. The cause is in the re-mint:
/// `readonly_tuple_image`'s object arm rebuilds through `spread_members_of`,
/// which reads each member's type from the SYMBOL table
/// (`get_type_of_symbol`) — the declared, widened type — not from the literal's
/// own retained members. The literal itself is right (`check_object_literal`
/// keeps `{ a: 1; }`); the re-mint throws that away.
///
/// It still converts 39 corpus lines, because on those the `readonly` is the
/// whole difference. Pinning the half-answer keeps the residue visible: when
/// the re-mint learns to carry the literal's members, this assertion is what
/// should change.
#[test]
fn an_object_argument_gets_the_readonly_at_the_call() {
    let source = "interface Array<T> { length: number }\n\
                  interface ReadonlyArray<T> { length: number }\n\
                  declare function f<const T>(x: T): T;\n\
                  const a = f({ a: 1, b: \"x\" });";
    assert_eq!(type_of_initialiser(source, "a"), "{ readonly a: number; readonly b: string; }");
}

/// The control: a plain type parameter widens the members and adds no
/// `readonly`.
#[test]
fn a_plain_type_parameter_widens_object_members() {
    let source = "interface Array<T> { length: number }\n\
                  interface ReadonlyArray<T> { length: number }\n\
                  declare function f<T>(x: T): T;\n\
                  const a = f({ a: 1, b: \"x\" });";
    assert_eq!(type_of_initialiser(source, "a"), "{ a: number; b: string; }");
}
