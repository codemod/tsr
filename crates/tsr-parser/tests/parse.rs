//! Parser behaviour.

use tsr_ast::{Expression, ModifierLike, Statement, SyntaxKind};
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
    let id = root.node_id.expect("the source file is registered");
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
    let id = statement.node_id.expect("registered");
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
fn class_implements_is_a_heritage_clause_not_a_name() {
    // `implements` is a future reserved word, so it is a legal binding identifier
    // outside strict mode and `class implements … ` is ambiguous. Upstream looks one
    // token past it: an identifier or keyword means a heritage clause
    // (`isImplementsClause`, `parser.go:1806`). Read as a name instead, two
    // `class implements` expressions in one file became a duplicate identifier.
    /// The class expression a `const C = class … ` statement initialises.
    fn class_of(statement: Statement<'_>) -> &tsr_ast::ClassExpression<'_> {
        let Statement::VariableStatement(variable) = statement else { panic!("a variable") };
        let list = variable.declaration_list.expect("a declaration list");
        let Some(Expression::ClassExpression(class)) = list.declarations[0].initializer else {
            panic!("a class expression")
        };
        class
    }

    let arena = Arena::new();
    let heritage = statements(
        &arena,
        "const C = class implements number {};\nconst D = class implements string {};\n",
    );
    assert_eq!(heritage.len(), 2);
    for statement in heritage {
        let class = class_of(*statement);
        assert!(class.name.is_none(), "`implements` opens the heritage clause");
        assert_eq!(class.heritage_clauses.len(), 1, "the `implements` clause is present");
    }

    // The name is still read when `implements` is not followed by a type — that is
    // the other half of the ambiguity, and it must keep working.
    let named = statements(&arena, "const C = class implements {};\n");
    let class = class_of(named[0]);
    assert_eq!(class.name.map(|name| name.text), Some("implements"), "a class named `implements`");
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

// ---- long-tail syntax -----------------------------------------------------

#[test]
fn tagged_templates() {
    let arena = Arena::new();
    statements(&arena, "const a = String.raw``;");
    statements(&arena, "const b = tag`x${1}y`;");
    statements(&arena, "const c = a.b.c`text`;");
}

#[test]
fn modifier_keywords_remain_usable_as_identifiers() {
    // `declare` is contextual: it is a modifier only when a declaration follows.
    let arena = Arena::new();
    statements(&arena, "var declare: any;\ndeclare instanceof C;");
    statements(&arena, "const async = 1;");
    statements(&arena, "const x = async => async;");
}

#[test]
fn instantiation_expressions() {
    // `f<T>` without a call is legal since TS 4.7, but `a < b > c` is arithmetic;
    // only what follows the `>` separates them.
    let arena = Arena::new();
    statements(&arena, "const f = foo<string>;");
    statements(&arena, "declare function g<T>(t: T): typeof g<T>;");
    statements(&arena, "const cmp = a < b > c;");
}

#[test]
fn shift_tokens_are_split_where_brackets_are_expected() {
    // `<K extends Key<U>>` ends in one `>>` token, and `Foo<<T>() => void>`
    // opens with one `<<`. Both must be split into separate brackets.
    let arena = Arena::new();
    statements(&arena, "const a = <K extends Key<U>>(k: K) => k;");
    statements(&arena, "type B = ReturnType<<T>(x: T) => number>;");
    statements(&arena, "type C = Map<string, Array<number>>;");
}

#[test]
fn arrow_return_types_may_contain_brackets_and_arrows() {
    // The "is this an arrow?" lookahead must track depth: in `(): (() => T) => x`
    // the inner `)` must not end the scan and the inner `=>` must not satisfy it.
    let arena = Arena::new();
    statements(&arena, "const a = <T>(): (() => T) => null as any;");
    statements(&arena, "const b = (): Iterable<number, any> => null!;");
    statements(&arena, "const c = (up: U): up is Filter<U, Q> => true;");
}

#[test]
fn decorators_in_their_several_positions() {
    let arena = Arena::new();
    statements(&arena, "@dec class A {}");
    statements(&arena, "@((t, c) => {}) class B {}");
    statements(&arena, "class C { @(x['y']) m() {} }");
    // `[` after an unparenthesised decorator is the *member's* computed name.
    statements(&arena, "class D { @dec ['1']() {} }");
    statements(&arena, "export default @dec class {}");
}

#[test]
fn explicit_resource_management() {
    let arena = Arena::new();
    statements(&arena, "async function f() { await using x = r(); using y = s(); }");
    statements(&arena, "async function g() { for (await using z of []) {} }");
}

#[test]
fn import_attributes() {
    let arena = Arena::new();
    statements(&arena, r#"import d from "./d.json" with { type: "json" };"#);
    statements(&arena, r#"export { a } from "./m" with { type: "json" };"#);
}

#[test]
fn keyword_named_declarations_and_types() {
    let arena = Arena::new();
    statements(&arena, "class require { }");
    statements(&arena, "namespace require { }");
    // `string` names a namespace when a `.` follows.
    statements(&arena, "var x: string.X;");
}

#[test]
fn abstract_constructor_types_and_infer_constraints() {
    let arena = Arena::new();
    statements(&arena, "type A = abstract new (...args: any) => object;");
    statements(&arena, "type B = T extends [infer R extends string] ? R : never;");
}

#[test]
fn optional_tuple_elements_versus_named_members() {
    // `[any?]` is an optional element; `[a?: number]` is a named one. Only the
    // `:` after the `?` tells them apart.
    let arena = Arena::new();
    statements(&arena, "type A = [number, any?];");
    statements(&arena, "type B = [a: number, b?: string];");
}

#[test]
fn a_shebang_is_trivia_on_the_first_line_only() {
    let arena = Arena::new();
    statements(&arena, "#!/usr/bin/env node\nclass A {}");
    // Elsewhere `#` still starts a private name.
    statements(&arena, "class B { #x = 1; m() { return this.#x; } }");
}

// ---- parent assignment ----------------------------------------------------

#[test]
fn every_node_but_the_root_has_a_parent() {
    let arena = Arena::new();
    let source =
        "class C { m(a: number) { return [a, {b: 1}]; } }\nexport const x = <T,>(y: T) => y;";
    let parsed = tsr_parser::parse(&arena, source);
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");

    let mut checked = 0;
    let mut stack = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        children.clear();
        tsr_ast::push_children(node, &mut children);
        for child in &children {
            if let Some(id) = child.node_id() {
                assert!(parsed.nodes.parent(id).is_some(), "{child:?} has no parent");
                checked += 1;
            }
            stack.push(*child);
        }
    }
    assert!(checked > 20, "walked {checked} nodes, expected the tree to be bigger");
    assert_eq!(parsed.nodes.parent(root), None, "the root has no parent");
}

#[test]
fn parents_chain_back_to_the_root() {
    let arena = Arena::new();
    let source = "function f() { if (a) { while (b) { c(); } } }";
    let parsed = tsr_parser::parse(&arena, source);
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");

    // Take the deepest node by span and walk up; it must terminate at the root
    // rather than cycling or dead-ending.
    let deepest = (0..parsed.nodes.len())
        .map(|i| tsr_ast::NodeId::new(u32::try_from(i).unwrap()))
        .max_by_key(|id| {
            let span = parsed.nodes.span(*id);
            (span.start, std::cmp::Reverse(span.end))
        })
        .expect("some nodes");

    let mut current = deepest;
    let mut steps = 0;
    while let Some(parent) = parsed.nodes.parent(current) {
        current = parent;
        steps += 1;
        assert!(steps < parsed.nodes.len(), "parent chain does not terminate");
    }
    assert_eq!(current, root, "the chain ended somewhere other than the root");
}

#[test]
fn parent_assignment_can_be_skipped() {
    let arena = Arena::new();
    let options = tsr_parser::ParseOptions::default().without_parents();
    let parsed = tsr_parser::parse_with_options(&arena, "let x = 1;", options);
    let has_any_parent = (0..parsed.nodes.len())
        .any(|i| parsed.nodes.parent(tsr_ast::NodeId::new(u32::try_from(i).unwrap())).is_some());
    assert!(!has_any_parent, "no parent should have been recorded");
}

#[test]
fn deeply_nested_input_does_not_overflow_the_parent_pass() {
    // The parser's depth guard bounds how deep it descends, not how deep a tree
    // it can produce, so the pass over that tree has to be iterative.
    let arena = Arena::new();
    let source = format!("let x = {}1{};", "(".repeat(2000), ")".repeat(2000));
    let parsed = tsr_parser::parse(&arena, &source);
    assert!(!parsed.nodes.is_empty());
}

// ---- thread-safety of the finished tree -----------------------------------

#[test]
fn a_parsed_tree_is_send_and_sync() {
    // The property the parallel binder and checker will need: once parsing is
    // done, `&SourceFile` can be handed to any number of threads. It holds only
    // because nodes carry no interior mutability — the arena returns `&mut` for a
    // fresh allocation, so the parser writes each node's id before any shared
    // reference exists. See docs/adr/0012-ast-is-sync.md.
    //
    // A compile-time assertion, not a runtime one: if a `Cell` reappears in a
    // node, this stops compiling, which is the only way to keep the property from
    // being lost silently.
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<tsr_ast::SourceFile<'static>>();
    assert_send_sync::<tsr_ast::Node<'static>>();
    assert_send_sync::<tsr_ast::Statement<'static>>();
    assert_send_sync::<tsr_ast::Expression<'static>>();
    assert_send_sync::<tsr_ast::NodeTable>();
    assert_send_sync::<tsr_ast::Token<'static>>();
}

#[test]
fn the_tree_can_actually_be_read_from_several_threads() {
    // The assertion above is a type-level claim; this is it being used. Every
    // thread walks the whole tree and counts nodes, and they must agree.
    let arena = Arena::new();
    let source = "class C { m(a: number) { return [a, {b: 1}]; } }\n\
                  export const f = <T,>(y: T) => y;\n\
                  interface I { x: string }";
    let parsed = tsr_parser::parse(&arena, source);
    let root = tsr_ast::Node::SourceFile(parsed.source_file);

    let counts: Vec<usize> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || {
                    let mut seen = 0;
                    let mut stack = vec![root];
                    let mut children = Vec::new();
                    while let Some(node) = stack.pop() {
                        seen += 1;
                        children.clear();
                        tsr_ast::push_children(node, &mut children);
                        stack.extend(children.iter().copied());
                    }
                    seen
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("thread panicked")).collect()
    });

    assert!(counts[0] > 20, "walked {} nodes, expected more", counts[0]);
    assert!(counts.iter().all(|c| *c == counts[0]), "threads disagreed: {counts:?}");
}

#[test]
fn a_default_export_keeps_the_default_keyword_as_a_modifier() {
    // `default` is the only thing that distinguishes this from `export class C`,
    // and dropping it is silent: the parse still succeeds.
    let arena = Arena::new();
    for source in [
        "export default class C {}",
        "export default function f() {}",
        "export default @dec class C {}",
        // `export default abstract class C {}` belongs here and does not parse;
        // filed as tsr-y4u.14.
    ] {
        let statements = statements(&arena, source);
        let modifiers = match statements[0] {
            Statement::ClassDeclaration(class) => class.modifiers,
            Statement::FunctionDeclaration(function) => function.modifiers,
            other => panic!("unexpected statement for {source:?}: {other:?}"),
        };
        let keywords: Vec<SyntaxKind> = modifiers
            .iter()
            .filter_map(|modifier| match modifier {
                ModifierLike::Token(token) => Some(token.kind),
                ModifierLike::Decorator(_) => None,
            })
            .collect();
        assert!(
            keywords.contains(&SyntaxKind::ExportKeyword)
                && keywords.contains(&SyntaxKind::DefaultKeyword),
            "{source:?} kept {keywords:?}"
        );
    }
}

#[test]
fn a_umd_global_declaration_is_not_an_export_assignment() {
    // `export as namespace N` and `export default N` mean different things; both
    // used to parse to `ExportAssignment`.
    let arena = Arena::new();
    let statements = statements(&arena, "export as namespace N;");
    match statements[0] {
        Statement::NamespaceExportDeclaration(declaration) => {
            assert_eq!(declaration.name.map(|name| name.text), Some("N"));
        }
        other => panic!("unexpected statement: {other:?}"),
    }
}

#[test]
fn a_contextual_keyword_in_expression_position_parses_as_an_identifier() {
    // A `KeywordExpression` carries a kind and no text, so parsing `module` as
    // one lost the name: `module.exports` referred to nothing.
    let arena = Arena::new();
    for (source, text) in [
        ("module.exports = 1;", "module"),
        ("const x = type;", "type"),
        ("of(1);", "of"),
        ("declare;", "declare"),
    ] {
        let statements = statements(&arena, source);
        let found = statements.iter().any(|statement| {
            fn names(node: tsr_ast::Node<'_>, text: &str) -> bool {
                if matches!(node, tsr_ast::Node::Identifier(id) if id.text == text) {
                    return true;
                }
                let mut children = Vec::new();
                tsr_ast::push_children(node, &mut children);
                children.into_iter().any(|child| names(child, text))
            }
            names(tsr_ast::Node::from(*statement), text)
        });
        assert!(found, "{source:?} should contain an identifier named {text:?}");
    }
}

#[test]
fn a_reserved_word_in_expression_position_stays_a_keyword() {
    let arena = Arena::new();
    let statements = statements(&arena, "const a = this;\nconst b = null;\nconst c = true;\n");
    assert_eq!(statements.len(), 3);
}

#[test]
fn a_second_static_is_a_member_name_not_a_modifier() {
    // `class C { static static }` declares a static member called `static`.
    // Upstream's `hasSeenStaticModifier`: no rule about what *follows* reaches
    // this, which is why the test is a whitelist of what may follow a modifier
    // rather than a blacklist of what may not.
    let arena = Arena::new();
    for source in [
        "class C { static static }",
        "class C { static static\n  m() {} }",
        "class C { static override static }",
        "class C { static \n static }",
    ] {
        let statements = statements(&arena, source);
        let Statement::ClassDeclaration(class) = statements[0] else { panic!("a class") };
        let named_static = class.members.iter().any(|member| {
            matches!(member, tsr_ast::ClassElement::PropertyDeclaration(property)
                if matches!(property.name, tsr_ast::PropertyName::Identifier(name)
                    if name.text == "static"))
        });
        assert!(named_static, "{source:?} should declare a member named `static`");
    }
}

#[test]
fn a_modifier_on_its_own_line_does_not_modify_the_next_one() {
    // `nextTokenIsOnSameLineAndCanFollowModifier`: everything except `static`
    // must be followed on the same line by what it modifies.
    let arena = Arena::new();
    let statements = statements(&arena, "interface Foo {\n  public\n  biz;\n}\n");
    let Statement::InterfaceDeclaration(interface) = statements[0] else { panic!("an interface") };
    assert_eq!(interface.members.len(), 2, "`public` is a member of its own");
}

#[test]
fn a_decorator_may_follow_export() {
    // `canFollowExportModifier` names `@` explicitly. It is an error later, but
    // it parses — and treating `export` as a name here lost the whole class.
    let arena = Arena::new();
    statements(&arena, "@dec export @dec class C {}");
}

// ---------------------------------------------------------------------------
// `NodeMap` — the way back from an id to the typed node (ADR-0033).
//
// The map is a `Vec` pushed in id order rather than an indexed store, which is
// what makes it nearly free. That is only sound while entry `n` really is the
// node whose id is `n`, so the invariant is tested directly rather than assumed.
// ---------------------------------------------------------------------------

/// Every id in the table resolves to the node that carries that id.
fn assert_map_is_aligned(source: &str) {
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert_eq!(
        parsed.node_map.len(),
        parsed.nodes.len(),
        "the map and the node table must stay the same length: {source:?}"
    );
    for index in 0..parsed.nodes.len() {
        let id = tsr_ast::NodeId::new(u32::try_from(index).expect("fits"));
        let node = parsed.node_map.get(id).expect("every registered id has a node");
        assert_eq!(
            node.node_id(),
            Some(id),
            "map entry {index} holds a node whose id is {:?}, in {source:?}",
            node.node_id()
        );
        assert_eq!(
            parsed.nodes.kind(id),
            parsed.nodes.kind(node.node_id().expect("id")),
            "kind disagrees at {index} in {source:?}"
        );
    }
}

#[test]
fn the_node_map_is_aligned_with_the_node_table() {
    assert_map_is_aligned("const a: string = 'x';");
    assert_map_is_aligned("class C { m(p: number): void {} }");
    assert_map_is_aligned("switch (1) { case 2: break; default: break; }");
    assert_map_is_aligned("for (const q of [1, 2]) { q; }");
}

#[test]
fn the_node_map_survives_speculative_backtracking() {
    // The case the lockstep `truncate` exists for. Each of these makes the
    // parser commit to a guess and then abandon it, registering nodes that never
    // enter the tree; if only `NodeTable` rolled back, every id after the
    // abandoned attempt would name a different node in each table.
    //
    // `(a)` starts as a possible arrow-function parameter list and turns out to
    // be a parenthesised expression; `<T>` in a `.ts` file is a type assertion
    // until proven a generic call; and the `<` chains force repeated lookahead.
    assert_map_is_aligned("const f = (a) => a;");
    assert_map_is_aligned("const g = (a);");
    assert_map_is_aligned("const h = <T,>(x: T) => x;");
    assert_map_is_aligned("const i = a < b > c;");
    assert_map_is_aligned("const j = f<number>(1);");
    assert_map_is_aligned("type K = A extends B ? C : D;");
}

#[test]
fn a_declarations_annotation_is_reachable_through_the_map() {
    // The query the checker actually makes, from an id rather than from a
    // reference held across the tree.
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, "const a: string = 'x';");
    let statement = parsed.source_file.statements[0];
    let tsr_ast::Statement::VariableStatement(statement) = statement else {
        panic!("expected a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let id = declaration.node_id.expect("registered");

    let Some(tsr_ast::Node::VariableDeclaration(found)) = parsed.node_map.get(id) else {
        panic!("expected a VariableDeclaration from the map");
    };
    assert!(found.r#type.is_some(), "the annotation must be reachable");
    assert!(found.initializer.is_some(), "the initialiser must be reachable");
}

// ---------------------------------------------------------------------------
// The generated named-child accessors (`bd tsr-5e7.8`).
//
// `Node::name_id`, `expression_id` and `initializer_id` exist because upstream's
// node-selection predicates all ask "is this node its parent's name / expression
// / initializer?" — an identity question, so an id is the whole answer. Tested
// here rather than in tsr-ast because a tree is what exercises them.
// ---------------------------------------------------------------------------

/// Every node of a parse, by a `push_children` walk.
fn all_nodes(root: tsr_ast::Node<'_>) -> Vec<tsr_ast::Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    let mut kids = Vec::new();
    while let Some(node) = stack.pop() {
        out.push(node);
        kids.clear();
        tsr_ast::push_children(node, &mut kids);
        stack.extend(kids.iter().copied());
    }
    out
}

#[test]
fn a_named_child_accessor_returns_an_actual_child() {
    // The invariant that makes the accessors usable for identity questions: what
    // they return must be a *child* of the node, not some other node that
    // happens to have an id. A generator emitting the wrong field would still
    // return `Some`, and only this catches that.
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(
        &arena,
        "const a: string = 'x';\n\
         function f(p = 1) { return p; }\n\
         class C { m = 2; get g() { return 1; } }\n\
         const o = { k: 3 };\n\
         a.b;\n",
    );
    let mut checked = 0;
    for node in all_nodes(tsr_ast::Node::SourceFile(parsed.source_file)) {
        let mut kids = Vec::new();
        tsr_ast::push_children(node, &mut kids);
        let child_ids: Vec<_> = kids.iter().filter_map(tsr_ast::Node::node_id).collect();
        for id in
            [node.name_id(), node.expression_id(), node.initializer_id()].into_iter().flatten()
        {
            assert!(
                child_ids.contains(&id),
                "{:?} returned {id:?}, which is not one of its children",
                parsed.nodes.kind(node.node_id().expect("registered"))
            );
            checked += 1;
        }
    }
    assert!(checked > 10, "the fixture must actually exercise the accessors, saw {checked}");
}

#[test]
fn every_named_child_accessor_fires_on_something() {
    // The bug this exists for: `ast.json` spells the fields `name`, `Expression`
    // and `Initializer` — not uniformly lower-case. Matching the wrong spelling
    // emits an accessor with **zero match arms**, which compiles cleanly and
    // returns `None` for every node in the language. That shipped once and was
    // caught by counting generated arms, not by any test.
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, "const a = b.c;\nfunction f(p = 1) {}\n");
    let nodes = all_nodes(tsr_ast::Node::SourceFile(parsed.source_file));

    assert!(nodes.iter().any(|n| n.name_id().is_some()), "no node answered name_id");
    assert!(nodes.iter().any(|n| n.expression_id().is_some()), "no node answered expression_id");
    assert!(nodes.iter().any(|n| n.initializer_id().is_some()), "no node answered initializer_id");
}

#[test]
fn is_declaration_node_matches_ast_jsons_declaration_base() {
    // Generated from `ast.json`'s `extends` chains, mirroring upstream's
    // `IsDeclarationNode` — `node.DeclarationData() != nil` (`ast.go:1511`) —
    // which is a structural test, not a list of kinds.
    use tsr_ast::SyntaxKind as K;

    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(
        &arena,
        "const v = 1;\nclass C { get g() { return 1; } }\nfunction f(p) {}\n\
         const o = { k: 1 };\na.b;\ng(1);\nx = 2;\n",
    );
    let mut seen: std::collections::HashMap<tsr_ast::SyntaxKind, bool> =
        std::collections::HashMap::new();
    for node in all_nodes(tsr_ast::Node::SourceFile(parsed.source_file)) {
        if let Some(id) = node.node_id() {
            seen.insert(parsed.nodes.kind(id), node.is_declaration_node());
        }
    }
    for kind in [K::VariableDeclaration, K::ClassDeclaration, K::FunctionDeclaration, K::Parameter]
    {
        assert_eq!(seen.get(&kind), Some(&true), "{kind:?} is a declaration");
    }

    // **A getter reaches `DeclarationBase` only transitively**, through
    // `FunctionLikeDeclarationBase`. Exactly four definitions do — the two
    // accessors and the function/constructor *type* nodes — so a generator that
    // looked only at each node's direct `extends` would miss all four while still
    // passing every other assertion here.
    assert_eq!(seen.get(&K::GetAccessor), Some(&true));

    // **`a.b` is not a declaration**, and this is the case that rules out the
    // tempting shortcut: `PropertyAccessExpression` *has* a `name` field — `b` —
    // so "has a name" would misclassify every property access as a declaration
    // name and silently change which nodes the `.types` producer emits.
    assert_eq!(seen.get(&K::PropertyAccessExpression), Some(&false));

    // **A call and a binary expression ARE declarations**, which looks wrong and
    // is not: `ast.json` gives both `DeclarationBase` because they can be
    // *assignment* declarations — `module.exports = ...`, `a.b = function () {}`
    // — which is exactly the `case KindBinaryExpression, KindCallExpression`
    // branch of `getTypeOfVariableOrParameterOrPropertyWorker`
    // (`checker.go:16623`). Asserted so nobody "fixes" it.
    assert_eq!(seen.get(&K::CallExpression), Some(&true));
    assert_eq!(seen.get(&K::BinaryExpression), Some(&true));
}

#[test]
fn const_is_a_modifier_on_a_class_member_not_a_member_name() {
    // `static const H = 1` is not legal TypeScript, and what upstream does with
    // it decides the shape of the tree every later stage sees: `const` is an
    // erroneous *modifier* and `H` is the member's name — one declaration, not
    // two. `permitConstAsModifier` (`parser.go:1854`, `:3919`).
    //
    // Without this the modifier run ended at `static`, `const` became the
    // member name, and `H = 1` was read as a *second* member. The `.types`
    // producer then emitted `>const` where upstream emits `>H`.
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, "class C {\n  static const H = 1;\n}");
    let members: Vec<_> = all_nodes(tsr_ast::Node::SourceFile(parsed.source_file))
        .into_iter()
        .filter_map(|n| n.node_id())
        .filter(|id| parsed.nodes.kind(*id) == tsr_ast::SyntaxKind::PropertyDeclaration)
        .collect();
    assert_eq!(members.len(), 1, "one member, not two");
    let name = parsed
        .node_map
        .get(members[0])
        .and_then(|n| n.name_id())
        .expect("the member has a name");
    let span = parsed.nodes.span(name);
    assert_eq!(&"class C {\n  static const H = 1;\n}"[span.start as usize..span.end as usize], "H");
}

#[test]
fn const_still_opens_a_declaration_everywhere_else() {
    // The other half of `permitConstAsModifier`: outside a class member `const`
    // is a declaration keyword, and consuming it as a modifier would leave
    // `export const a = 1` looking like a bare expression.
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, "export const a = 1;");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics.len());
    let kinds: Vec<_> = all_nodes(tsr_ast::Node::SourceFile(parsed.source_file))
        .into_iter()
        .filter_map(|n| n.node_id())
        .map(|id| parsed.nodes.kind(id))
        .collect();
    assert!(kinds.contains(&tsr_ast::SyntaxKind::VariableStatement));
}
