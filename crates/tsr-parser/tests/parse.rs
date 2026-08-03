//! Parser behaviour.

use tsr_ast::{Expression, Statement, SyntaxKind};
use tsr_core::Arena;
use tsr_parser::parse;

/// Parse and assert no diagnostics, returning the statements.
fn statements<'a>(arena: &'a Arena, source: &'a str) -> &'a [Statement<'a>] {
    let result = parse(arena, source);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics for {source:?}: {:?}",
        result.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    result.source_file.statements
}

#[test]
fn empty_source_parses_to_an_empty_file() {
    let arena = Arena::new();
    assert!(statements(&arena, "").is_empty());
}

#[test]
fn variable_statements() {
    let arena = Arena::new();
    let statements = statements(&arena, "const x: number = 1;\nlet y;\nvar z = 'a';");
    assert_eq!(statements.len(), 3);
    assert!(statements.iter().all(|s| matches!(s, Statement::VariableStatement(_))));
}

#[test]
fn destructuring_binds() {
    let arena = Arena::new();
    let statements = statements(&arena, "const { a, b: c } = o; const [d, , e] = arr;");
    assert_eq!(statements.len(), 2);
}

#[test]
fn binary_precedence_groups_correctly() {
    // `a + b * c` must nest as `a + (b * c)`, so the top operator is `+`.
    let arena = Arena::new();
    let statements = statements(&arena, "a + b * c;");
    let Statement::ExpressionStatement(statement) = statements[0] else {
        panic!("expected an expression statement")
    };
    let Some(Expression::BinaryExpression(outer)) = statement.expression else {
        panic!("expected a binary expression")
    };
    assert_eq!(outer.operator_token.map(|t| t.kind), Some(SyntaxKind::PlusToken));
    assert!(
        matches!(outer.right, Some(Expression::BinaryExpression(_))),
        "the multiplication must be the right operand"
    );
}

#[test]
fn exponentiation_is_right_associative() {
    // `a ** b ** c` is `a ** (b ** c)`, unlike every other binary operator.
    let arena = Arena::new();
    let statements = statements(&arena, "a ** b ** c;");
    let Statement::ExpressionStatement(statement) = statements[0] else { panic!() };
    let Some(Expression::BinaryExpression(outer)) = statement.expression else { panic!() };
    assert!(
        matches!(outer.right, Some(Expression::BinaryExpression(_))),
        "right-associative nesting must put the inner `**` on the right"
    );
    assert!(matches!(outer.left, Some(Expression::Identifier(_))));
}

#[test]
fn subtraction_is_left_associative() {
    // `a - b - c` is `(a - b) - c`; getting this backwards changes arithmetic.
    let arena = Arena::new();
    let statements = statements(&arena, "a - b - c;");
    let Statement::ExpressionStatement(statement) = statements[0] else { panic!() };
    let Some(Expression::BinaryExpression(outer)) = statement.expression else { panic!() };
    assert!(matches!(outer.left, Some(Expression::BinaryExpression(_))));
    assert!(matches!(outer.right, Some(Expression::Identifier(_))));
}

#[test]
fn call_and_member_chains() {
    let arena = Arena::new();
    statements(&arena, "a.b.c(1)[2]!.d?.e?.(3);");
}

#[test]
fn control_flow_statements() {
    let arena = Arena::new();
    let source = "
        if (a) { b(); } else c();
        while (x) {}
        do {} while (y);
        for (let i = 0; i < 10; i++) {}
        for (const k in o) {}
        for (const v of a) {}
        switch (e) { case 1: break; default: }
        try { f(); } catch (err) { g(); } finally { h(); }
        try { f(); } catch { g(); }
        outer: for (;;) { continue outer; }
    ";
    assert_eq!(statements(&arena, source).len(), 10);
}

#[test]
fn functions_and_arrows() {
    let arena = Arena::new();
    let source = "
        function f<T>(a: T, b = 1, ...rest: string[]): T { return a; }
        const g = (x: number) => x + 1;
        const h = x => x;
        const i = () => { return 1; };
        function* gen() { yield 1; }
    ";
    assert_eq!(statements(&arena, source).len(), 5);
}

#[test]
fn arrow_versus_parenthesised_expression() {
    // `(a)` and `(a) => a` are identical until the `=>`; the parser speculates.
    let arena = Arena::new();
    let statements = statements(&arena, "(a); (a) => a;");
    let Statement::ExpressionStatement(first) = statements[0] else { panic!() };
    assert!(
        matches!(first.expression, Some(Expression::ParenthesizedExpression(_))),
        "`(a)` alone is parenthesised, not an arrow"
    );
    let Statement::ExpressionStatement(second) = statements[1] else { panic!() };
    assert!(matches!(second.expression, Some(Expression::ArrowFunction(_))));
}

#[test]
fn object_and_array_literals() {
    let arena = Arena::new();
    statements(&arena, "const o = { a: 1, b, 'c': 3, [d]: 4, ...e };");
    statements(&arena, "const a = [1, , 3, ...rest];");
}

#[test]
fn template_literals() {
    let arena = Arena::new();
    let statements = statements(&arena, "const a = `plain`; const b = `x${1}y${2}z`;");
    assert_eq!(statements.len(), 2);
}

#[test]
fn type_annotations() {
    let arena = Arena::new();
    let source = "
        let a: string | number;
        let b: A & B;
        let c: string[];
        let d: Array<Map<string, number>>;
        let e: { x: number; y(): void };
        let f: [string, number];
        let g: typeof window;
        let h: keyof T;
        let i: 'literal';
        let j: T['key'];
        let k: (a: number) => void;
    ";
    assert_eq!(statements(&arena, source).len(), 11);
}

#[test]
fn nested_generics_split_the_closing_angle_brackets() {
    // `Map<string, Array<number>>` lexes the tail as `>>`; the parser must split it.
    let arena = Arena::new();
    statements(&arena, "let x: Map<string, Array<number>>;");
}

#[test]
fn function_and_constructor_types() {
    let arena = Arena::new();
    let statements = statements(
        &arena,
        "let a: (x: number) => void;\nlet b: <T>(x: T) => T;\nlet c: new (x: number) => Foo;",
    );
    assert_eq!(statements.len(), 3);
}

#[test]
fn a_parenthesised_type_is_not_mistaken_for_a_function_type() {
    // `(string)` has no `=>`, so the speculative parse must rewind.
    let arena = Arena::new();
    statements(&arena, "let a: (string);");
}

#[test]
fn as_and_satisfies_take_a_type_on_the_right() {
    let arena = Arena::new();
    let statements = statements(&arena, "const a = x as number; const b = y satisfies T;");
    assert_eq!(statements.len(), 2);
}

#[test]
fn automatic_semicolon_insertion() {
    let arena = Arena::new();
    // No semicolons anywhere; line breaks must terminate each statement.
    let statements = statements(&arena, "const a = 1\nconst b = 2\nreturn\n");
    assert_eq!(statements.len(), 3);
}

#[test]
fn regular_expressions_are_recognised_in_expression_position() {
    let arena = Arena::new();
    statements(&arena, "const re = /ab+c/gi;");
    // A slash in operand position is division, not a regex.
    statements(&arena, "const q = a / b / c;");
}

// ---- error recovery -------------------------------------------------------

#[test]
fn a_missing_semicolon_is_reported_but_parsing_continues() {
    let arena = Arena::new();
    let result = parse(&arena, "const a = 1 const b = 2;");
    assert!(!result.diagnostics.is_empty());
    assert_eq!(result.source_file.statements.len(), 2, "both statements must survive");
}

#[test]
fn unexpected_tokens_do_not_hang_the_parser() {
    // The parser must always consume something; a stall here would spin forever.
    for source in ["@@@", ")", "}", "const", "if (", "function", "]]]", ": :"] {
        let arena = Arena::new();
        let result = parse(&arena, source);
        assert!(!result.diagnostics.is_empty(), "{source:?} should report something");
    }
}

#[test]
fn unterminated_constructs_terminate() {
    for source in ["function f() {", "if (a) {", "const x = [1, 2", "`unterminated"] {
        let arena = Arena::new();
        let _ = parse(&arena, source);
    }
}

#[test]
fn deeply_nested_input_reports_rather_than_overflowing_the_stack() {
    // A depth guard turns pathological input into a diagnostic. Without it this
    // is a crash, which a language service cannot recover from.
    let arena = Arena::new();
    let source = format!("const x = {}1{};", "(".repeat(2000), ")".repeat(2000));
    let result = parse(&arena, &source);
    assert!(!result.diagnostics.is_empty(), "should report hitting the depth limit");
}

#[test]
fn scanner_diagnostics_are_merged_in_source_order() {
    let arena = Arena::new();
    let result = parse(&arena, "const a = 'unterminated\nconst b = ;");
    assert!(result.diagnostics.len() >= 2);
    let positions: Vec<u32> = result.diagnostics.iter().map(|d| d.span.start).collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "diagnostics must be in source order");
}

// ---- node registration ----------------------------------------------------

#[test]
fn nodes_are_registered_with_kinds_and_spans() {
    let arena = Arena::new();
    let result = parse(&arena, "const x = 1;");
    assert!(result.nodes.len() > 3, "every node should be registered");

    let root = result.source_file;
    let id = root.node_id.get().expect("the source file is registered");
    assert_eq!(result.nodes.kind(id), SyntaxKind::SourceFile);
    assert_eq!(result.nodes.span(id).start, 0);
}

#[test]
fn spans_cover_the_construct_they_describe() {
    let arena = Arena::new();
    let source = "  const x = 1;";
    let result = parse(&arena, source);
    let Statement::VariableStatement(statement) = result.source_file.statements[0] else {
        panic!()
    };
    let id = statement.node_id.get().expect("registered");
    let span = result.nodes.span(id);
    assert_eq!(span.start, 2, "the span starts after leading trivia");
    assert_eq!(&source[span.start as usize..span.end as usize], "const x = 1;");
}

// ---- declarations ---------------------------------------------------------

#[test]
fn class_declarations() {
    let arena = Arena::new();
    let source = "
        class A {}
        class B<T> extends A implements I, J {
            x: number = 1;
            private readonly y?: string;
            static z = 2;
            constructor(private a: number) { super(); }
            m<U>(p: U): U { return p; }
            get g(): number { return 1; }
            ;
        }
        export default class {}
    ";
    assert_eq!(statements(&arena, source).len(), 3);
}

#[test]
fn interface_declarations() {
    let arena = Arena::new();
    let source = "
        interface I {}
        interface J<T> extends I {
            a: string;
            b?: number;
            c(x: T): void;
        }
    ";
    assert_eq!(statements(&arena, source).len(), 2);
}

#[test]
fn type_aliases_and_enums() {
    let arena = Arena::new();
    let source = "
        type A = string;
        type B<T> = T | null;
        enum E { A, B = 1, C = 'c' }
        const enum F { X }
    ";
    assert_eq!(statements(&arena, source).len(), 4);
}

#[test]
fn export_and_declare_modifiers_reach_the_declaration() {
    let arena = Arena::new();
    let source = "
        export const a = 1;
        export function f() {}
        export class C {}
        declare const b: number;
        export abstract class D {}
    ";
    let statements = statements(&arena, source);
    assert_eq!(statements.len(), 5);
    assert!(matches!(statements[0], Statement::VariableStatement(_)));
    assert!(matches!(statements[2], Statement::ClassDeclaration(_)));
}

#[test]
fn contextual_keywords_remain_usable_as_names() {
    // `type` and `interface` are contextual: `type = 1` is an assignment.
    let arena = Arena::new();
    statements(&arena, "type = 1;");
    statements(&arena, "let type = 1;");
    statements(&arena, "const of = 1;");
}

#[test]
fn nested_parentheses_do_not_blow_up() {
    let arena = Arena::new();
    let source = format!("const x = {}1{};", "(".repeat(40), ")".repeat(40));
    let _ = parse(&arena, &source);
}

#[test]
fn nested_assignments_in_parentheses_parse_in_linear_time() {
    // Regression: deciding "is this an arrow function?" by speculatively parsing
    // a parameter list is exponential here, because a parameter's initializer is
    // parsed with `parse_assignment_expression`, which speculates again. Each
    // nesting level re-parses the whole tail.
    //
    // The corpus case `parsingDeepParenthensizedExpression.ts` has this shape and
    // took the parser to 16 GB before the token-only lookahead replaced it. At 40
    // levels the exponential version does not terminate; the linear one is
    // instant, so a plain timeout-free test is a sufficient guard.
    let arena = Arena::new();
    let mut source = String::from("E = ");
    for _ in 0..40 {
        source.push_str("(E = ");
    }
    source.push('1');
    for _ in 0..40 {
        source.push(')');
    }
    source.push(';');
    let _ = parse(&arena, &source);
}

#[test]
fn a_later_arrow_is_not_attributed_to_an_earlier_parenthesised_group() {
    // The lookahead must stop at a statement boundary, or `(a); x => y` would
    // read `(a)` as an arrow's parameter list.
    let arena = Arena::new();
    let statements = statements(&arena, "(a); x => y;");
    let Statement::ExpressionStatement(first) = statements[0] else { panic!() };
    assert!(matches!(first.expression, Some(Expression::ParenthesizedExpression(_))));
}

// ---- modules --------------------------------------------------------------

#[test]
fn import_declarations() {
    let arena = Arena::new();
    let source = "
        import 'side-effect';
        import d from 'm';
        import { a, b as c } from 'm';
        import * as ns from 'm';
        import d, { a } from 'm';
        import d, * as ns from 'm';
        import type { T } from 'm';
        import ts = require('typescript');
        import A = B.C;
    ";
    assert_eq!(statements(&arena, source).len(), 9);
}

#[test]
fn export_declarations() {
    let arena = Arena::new();
    let source = "
        export { a, b as c };
        export { a } from 'm';
        export * from 'm';
        export * as ns from 'm';
        export type { T } from 'm';
        export default 1;
        export default class {}
        export = x;
        export const y = 1;
    ";
    assert_eq!(statements(&arena, source).len(), 9);
}

#[test]
fn namespaces_and_modules() {
    let arena = Arena::new();
    let source = "
        namespace N { const a = 1; }
        module M { }
        declare module 'ambient' { }
        namespace A.B { }
    ";
    assert_eq!(statements(&arena, source).len(), 4);
}

#[test]
fn type_is_still_usable_as_an_imported_name() {
    // `import type from 'm'` imports a binding called `type`; only
    // `import type { … }` is the type-only modifier.
    let arena = Arena::new();
    statements(&arena, "import type from 'm';");
    statements(&arena, "import { type } from 'm';");
}

#[test]
fn function_and_class_expressions() {
    let arena = Arena::new();
    let source = "
        const a = function () { return 1; };
        const b = function named<T>(x: T): T { return x; };
        const c = function* () { yield 1; };
        xs.every(function (v) { return v; });
        const d = class {};
        const e = class Named extends Base {};
    ";
    assert_eq!(statements(&arena, source).len(), 6);
}

#[test]
fn conditional_and_infer_types() {
    let arena = Arena::new();
    let source = "
        type A = T extends U ? 'y' : 'n';
        type B<T> = T extends Array<infer E> ? E : never;
        type C = A extends B ? C extends D ? 1 : 2 : 3;
    ";
    assert_eq!(statements(&arena, source).len(), 3);
}

#[test]
fn modifier_keywords_are_usable_as_member_names() {
    // `interface I { abstract(): void }` declares a method named `abstract`,
    // not an abstract member.
    let arena = Arena::new();
    statements(&arena, "interface abstract { abstract(): void; }");
    statements(&arena, "class C { static: number; readonly = 1; }");
}

// ---- JSX ------------------------------------------------------------------

/// Parse as `.tsx` and assert no diagnostics.
fn tsx<'a>(arena: &'a Arena, source: &'a str) -> &'a [Statement<'a>] {
    let result = tsr_parser::parse_with_script_kind(arena, source, tsr_parser::ScriptKind::Tsx);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics for {source:?}: {:?}",
        result.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    result.source_file.statements
}

#[test]
fn jsx_elements() {
    let arena = Arena::new();
    tsx(&arena, "const a = <div />;");
    tsx(&arena, "const b = <div>text</div>;");
    tsx(&arena, "const c = <div><span /></div>;");
    tsx(&arena, "const d = <></>;");
    tsx(&arena, "const e = <>text{expr}</>;");
}

#[test]
fn jsx_attributes() {
    let arena = Arena::new();
    tsx(&arena, r#"const a = <div className="x" />;"#);
    tsx(&arena, "const b = <div onClick={handler} />;");
    tsx(&arena, "const c = <input disabled />;");
    tsx(&arena, "const d = <div {...props} />;");
    tsx(&arena, r#"const e = <div data-testid="x" aria-label="y" />;"#);
    tsx(&arena, r##"const f = <svg:circle xlink:href="#x" />;"##);
}

#[test]
fn jsx_tag_names() {
    let arena = Arena::new();
    tsx(&arena, "const a = <My.Component />;");
    tsx(&arena, "const b = <A.B.C />;");
    tsx(&arena, "const c = <my-element />;");
    tsx(&arena, "const d = <svg:circle />;");
}

#[test]
fn jsx_children_and_expressions() {
    let arena = Arena::new();
    tsx(&arena, "const a = <div>{items.map(i => <li key={i} />)}</div>;");
    tsx(&arena, "const b = <div>{}</div>;");
    tsx(&arena, "const c = <ul>{...items}</ul>;");
    // Entities are literal text, not escapes — the case that first exposed the
    // missing JSX scanner mode.
    tsx(&arena, "const d = <div>&#0123;&#x7d;</div>;");
}

#[test]
fn jsx_attribute_values_are_raw() {
    // `"a\b"` is four characters; treating `\b` as an escape is wrong in JSX.
    let arena = Arena::new();
    tsx(&arena, r#"const a = <div title="a\b" />;"#);
}

#[test]
fn angle_bracket_means_different_things_per_dialect() {
    // The whole reason `ScriptKind` exists.
    let arena = Arena::new();
    let statements = statements(&arena, "const a = <Foo>x;");
    let Statement::VariableStatement(_) = statements[0] else { panic!() };

    let arena = Arena::new();
    tsx(&arena, "const a = <Foo>x</Foo>;");

    // A type assertion is not available in `.tsx`; `as` is the alternative.
    let arena = Arena::new();
    tsx(&arena, "const b = x as Foo;");
}

#[test]
fn script_kind_is_inferred_from_the_file_name() {
    use tsr_parser::ScriptKind;
    assert_eq!(ScriptKind::from_file_name("a.tsx"), ScriptKind::Tsx);
    assert_eq!(ScriptKind::from_file_name("a.jsx"), ScriptKind::Tsx);
    assert_eq!(ScriptKind::from_file_name("a.ts"), ScriptKind::TypeScript);
    assert_eq!(ScriptKind::from_file_name("a.d.ts"), ScriptKind::TypeScript);
    assert!(ScriptKind::Tsx.allows_jsx());
    assert!(!ScriptKind::TypeScript.allows_jsx());
}

#[test]
fn jsx_whitespace_only_children_are_marked() {
    // `<div>\n  </div>` has a whitespace-only child that emit drops; `<div>  </div>`
    // has real text.
    let arena = Arena::new();
    tsx(&arena, "const a = <div>\n  </div>;");
    tsx(&arena, "const b = <div>  </div>;");
}

#[test]
fn nested_and_elided_array_destructuring() {
    // `[` opens a nested pattern; only a `:` after the closing bracket makes it a
    // computed key. Reading it as a key unconditionally broke every nested array
    // destructuring in the corpus.
    let arena = Arena::new();
    statements(&arena, "let [,,[,[],,[],]] = x;");
    statements(&arena, "function f([[]] = [[1,2,3]]) {}");
    statements(&arena, "var [, a, , ] = [3, 4, 5];");
    statements(&arena, "var [, , [, b, ]] = [3,5,[0,1]];");
    // A computed key still works.
    statements(&arena, "const { [k]: v } = o;");
}

#[test]
fn namespaces_may_be_named_with_contextual_keywords() {
    let arena = Arena::new();
    statements(&arena, "namespace require { }");
    statements(&arena, "namespace m1 { namespace require { } }");
    statements(&arena, "declare global { interface X {} }");
}

#[test]
fn import_types() {
    let arena = Arena::new();
    statements(&arena, "const a: import('mod').Type = x;");
    statements(&arena, "let b: import('mod').Ns.Type<string>;");
}
