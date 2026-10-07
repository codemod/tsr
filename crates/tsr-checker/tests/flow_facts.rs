//! Flow facts controls for pinned typescript-go getTypeFactsWorker/isZeroBigInt.
//! Native declaration receipts live in target/recovery/flow (ignored).

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn narrowed_type(source: &str, strict_null_checks: bool) -> String {
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
    checker.set_strict_null_checks(strict_null_checks);
    let statements = match parsed.source_file.statements.last().unwrap() {
        Statement::FunctionDeclaration(function) => {
            let Some(tsr_ast::FunctionBody::Block(body)) = function.body else {
                panic!("fixture requires a function block");
            };
            body.statements
        }
        _ => parsed.source_file.statements,
    };
    let Statement::IfStatement(condition) = statements.last().unwrap() else {
        panic!("fixture must end in an if statement");
    };
    let Some(Statement::Block(block)) = condition.then_statement else {
        panic!("fixture must have a block");
    };
    let Statement::ExpressionStatement(reference) = block.statements.last().unwrap() else {
        panic!("fixture must end in a reference");
    };
    let ty = checker.check_expression(reference.expression.unwrap());
    checker.type_to_string(ty)
}

#[test]
fn binding_initial_default_retains_literal_before_assignment_reduction() {
    assert_eq!(
        narrowed_type(
            "function f() {
                 const { value = true } = { value: 1 as number | undefined };
                 if (true) { value; }
             }",
            true,
        ),
        "number | true"
    );
}

#[test]
fn unknown_in_property_intersects_original_receiver_with_global_record() {
    assert_eq!(
        narrowed_type(
            "type Record<K extends keyof any, V> = { [P in K]: V };
             function f<T extends object>(x: T) { if ('field' in x) { x; } }",
            true,
        ),
        "T & Record<\"field\", unknown>"
    );
    assert_eq!(
        narrowed_type(
            "type Record<K extends keyof any, V> = { [P in K]: V };
             function f<T extends object>(x: T) { if (!('field' in x)) { x; } }",
            true,
        ),
        "T"
    );
}

#[test]
fn bigint_zero_truthiness_uses_semantic_digits() {
    for zero in ["0n", "0x0n", "0o0n", "0b0n", "0x000_000n"] {
        assert_eq!(
            narrowed_type(&format!("declare let x: {zero} | 1n; if (x) {{ x; }}"), true),
            "1n",
            "truthy branch for {zero}"
        );
        assert_eq!(
            narrowed_type(&format!("declare let x: {zero} | 1n; if (!x) {{ x; }}"), true),
            "0n",
            "falsy branch for {zero}"
        );
    }
}

#[test]
fn template_literal_facts_are_nonempty_string_facts() {
    for template in ["`prefix${string}`", "`${number}`"] {
        let source = format!("declare let x: {template}; if (!x) {{ x; }}");
        assert_eq!(narrowed_type(&source, true), "never", "strict {template}");
        assert_eq!(narrowed_type(&source, false), template, "loose {template}");
    }
}

#[test]
fn loose_scalar_literals_keep_falsy_nullish_possibilities() {
    for scalar in ["'text'", "1", "true"] {
        let source = format!("declare let x: {scalar}; if (!x) {{ x; }}");
        assert_eq!(narrowed_type(&source, true), "never", "strict {scalar}");
        assert_eq!(
            narrowed_type(&source, false),
            if scalar == "'text'" { "\"text\"" } else { scalar },
            "loose {scalar}"
        );
    }
}

#[test]
fn nonzero_bigints_remain_truthy() {
    for nonzero in ["1n", "0x100000000000000000000000000000000n"] {
        assert_eq!(
            narrowed_type(&format!("declare let x: {nonzero}; if (!x) {{ x; }}"), true),
            "never",
            "nonzero {nonzero}"
        );
    }
}

#[test]
fn loose_bigint_falsy_branch_keeps_nullish_possibilities() {
    assert_eq!(narrowed_type("declare let x: 0n | 1n; if (!x) { x; }", false), "0n | 1n");
    assert_eq!(narrowed_type("declare let x: bigint; if (!x) { x; }", false), "bigint");
}

#[test]
fn unique_symbol_typeof_facts_exclude_other_domains() {
    assert_eq!(
        narrowed_type(
            "declare const key: unique symbol;
             declare let x: typeof key | string; if (typeof x !== 'symbol') { x; }",
            true,
        ),
        "string"
    );
    assert_eq!(
        narrowed_type(
            "declare const key: unique symbol;
             declare let x: typeof key; if (typeof x === 'number') { x; }",
            true,
        ),
        "never"
    );
}

#[test]
fn symbol_falsy_facts_follow_null_check_mode() {
    for symbol in ["symbol", "typeof key"] {
        let source = format!(
            "declare const key: unique symbol;
             declare let x: {symbol}; if (!x) {{ x; }}"
        );
        assert_eq!(narrowed_type(&source, true), "never", "strict {symbol}");
        if symbol == "symbol" {
            assert_eq!(narrowed_type(&source, false), "symbol", "loose symbol");
        }
    }
}
