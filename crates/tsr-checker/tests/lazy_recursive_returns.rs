//! Semantic controls pinned to typescript-go 5b1047d1. These compare `TypeIds`
//! and completed return slots, independently of the recursive site printer.
use tsr_ast::{Node, NodeId};
use tsr_binder::{BindResult, SymbolId};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn with_checker(source: &str, test: impl FnOnce(&mut Checker<'_, '_>, &BindResult<'_>, NodeId)) {
    with_checker_in_file(source, "control.ts", test);
}

fn with_checker_in_file(
    source: &str,
    name: &str,
    test: impl FnOnce(&mut Checker<'_, '_>, &BindResult<'_>, NodeId),
) {
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(parsed.diagnostics.is_empty());
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    if std::path::Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("js")) {
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    test(&mut checker, &bound, root);
}

fn symbol(bound: &BindResult<'_>, root: NodeId, name: &str) -> SymbolId {
    bound.lookup_local(root, name).unwrap()
}

#[test]
fn parameter_only_typeof_sources_keep_identity_and_complete_calls_in_either_order() {
    for first in ["foo", "bar", "fooResult", "barResult"] {
        with_checker(
            "interface Array<T> { [index: number]: T; } type FormalInputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never; function foo(arg: FormalInputs<typeof bar>[0]) { return arg; } function bar(arg: string, count: number) { return foo(arg); } const fooResult = foo('cold'); const barResult = bar('warm', 23);",
            |checker, bound, root| {
                checker.set_strict_null_checks(true);
                checker.set_no_implicit_any(true);
                checker.get_type_of_symbol(symbol(bound, root, first));
                let foo = symbol(bound, root, "foo");
                let bar = symbol(bound, root, "bar");
                let foo_type = checker.get_type_of_symbol(foo);
                let bar_type = checker.get_type_of_symbol(bar);
                assert_ne!(foo_type, bar_type);
                for reverse in [false, true, false] {
                    let mut calls = ["fooResult", "barResult"];
                    if reverse {
                        calls.reverse();
                    }
                    for call in calls {
                        let returned = checker.get_type_of_symbol(symbol(bound, root, call));
                        assert_eq!(
                            returned,
                            checker.intrinsics().string,
                            "cold={first}, call={call}"
                        );
                    }
                    let foo_signature = checker.get_signatures_of_symbol(foo).unwrap().remove(0);
                    let bar_signature = checker.get_signatures_of_symbol(bar).unwrap().remove(0);
                    assert_eq!(foo_signature.parameters.len(), 1);
                    assert_eq!(bar_signature.parameters.len(), 2);
                    assert_eq!(foo_signature.parameters[0].r#type, checker.intrinsics().string);
                    assert_eq!(bar_signature.parameters[0].r#type, checker.intrinsics().string);
                    assert_eq!(bar_signature.parameters[1].r#type, checker.intrinsics().number);
                    assert_eq!(foo_signature.r#type, checker.intrinsics().string);
                    assert_eq!(bar_signature.r#type, checker.intrinsics().string);
                    assert_eq!(checker.get_type_of_symbol(foo), foo_type);
                    assert_eq!(checker.get_type_of_symbol(bar), bar_type);
                }
                checker.check_source_file(
                    root,
                    FileContext { ambient: false, has_parse_errors: false },
                );
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn return_type_queries_keep_the_eager_source_route_in_either_order() {
    for first in ["make", "returned", "result"] {
        with_checker(
            "interface Array<T> { [index: number]: T; } type ReturnType<F> = F extends (...args: any[]) => infer R ? R : never; function make() { return { value: 23 }; } declare const returned: ReturnType<typeof make>; const result = make();",
            |checker, bound, root| {
                checker.set_no_implicit_any(true);
                checker.get_type_of_symbol(symbol(bound, root, first));
                let callable = symbol(bound, root, "make");
                let identity = checker.get_type_of_symbol(callable);
                for names in
                    [["returned", "result"], ["result", "returned"], ["returned", "result"]]
                {
                    for name in names {
                        let ty = checker.get_type_of_symbol(symbol(bound, root, name));
                        let value = checker.get_type_of_property_of_type(ty, "value").unwrap();
                        assert_eq!(value, checker.intrinsics().number);
                    }
                    assert_eq!(checker.get_type_of_symbol(callable), identity);
                }
                checker.check_source_file(
                    root,
                    FileContext { ambient: false, has_parse_errors: false },
                );
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn a_self_return_preserves_callable_identity_in_both_query_orders_and_on_rerun() {
    for signatures_first in [false, true] {
        with_checker("function returnSelf() { return returnSelf; }", |checker, bound, root| {
            let f = symbol(bound, root, "returnSelf");
            if signatures_first {
                checker.get_signatures_of_symbol(f).unwrap();
            }
            let ty = checker.get_type_of_symbol(f);
            for _ in 0..3 {
                let signatures = checker.get_signatures_of_symbol(f).unwrap();
                assert_eq!(signatures.len(), 1);
                assert_eq!(signatures[0].r#type, ty, "return f is not return f()");
                assert_eq!(checker.get_type_of_symbol(f), ty);
            }
        });
    }
}

#[test]
fn captured_objects_publish_before_their_member_returns_in_either_order() {
    for first in ["factory", "result", "returned"] {
        with_checker(
            "function factory() { let x = { value: 17, recur: () => x }; return x; } const result = factory(); const returned = result.recur();",
            |checker, bound, root| {
                checker.set_no_implicit_any(true);
                checker.get_type_of_symbol(symbol(bound, root, first));
                for _ in 0..3 {
                    let result = checker.get_type_of_symbol(symbol(bound, root, "result"));
                    let returned = checker.get_type_of_symbol(symbol(bound, root, "returned"));
                    assert_eq!(returned, result);
                    let value = checker.get_type_of_property_of_type(returned, "value").unwrap();
                    assert_eq!(checker.type_to_string(value), "number");
                }
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn nested_captured_objects_keep_the_outer_identity_until_actual_return_demand() {
    for first in ["factory", "result", "returned"] {
        with_checker(
            "function factory() { let x = { outer: { value: 17, recur: () => x } }; return x; } const result = factory(); const returned = result.outer.recur();",
            |checker, bound, root| {
                checker.set_no_implicit_any(true);
                checker.get_type_of_symbol(symbol(bound, root, first));
                for _ in 0..3 {
                    let result = checker.get_type_of_symbol(symbol(bound, root, "result"));
                    let returned = checker.get_type_of_symbol(symbol(bound, root, "returned"));
                    assert_eq!(returned, result);
                    let outer = checker.get_type_of_property_of_type(returned, "outer").unwrap();
                    let value = checker.get_type_of_property_of_type(outer, "value").unwrap();
                    assert_eq!(checker.type_to_string(value), "number");
                }
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn lazy_captured_generic_returns_do_not_publish_guessed_recursive_mapper_images() {
    for first in ["factory", "stringResult", "numberResult"] {
        with_checker(
            "function factory<T>() { let x = { foo: <U>(u: U) => x, bar: (t: T) => x }; return x; } const stringResult = factory<string>(); const numberResult = factory<number>(); const stringAgain = stringResult.foo(23).bar('yes'); const numberAgain = numberResult.foo('other').bar(5);",
            |checker, bound, root| {
                checker.set_no_implicit_any(true);
                checker.get_type_of_symbol(symbol(bound, root, first));
                let original =
                    checker.get_signatures_of_symbol(symbol(bound, root, "factory")).unwrap()[0]
                        .r#type;
                let bar = checker.get_type_of_property_of_type(original, "bar").unwrap();
                let signature = checker.resolve_call_signature(bar, None).unwrap();
                assert_eq!(signature.r#type, original);
                let parameter = signature.parameters[0].r#type;
                assert_eq!(checker.type_to_string(parameter), "T");
                // A direct mapper hit must win without forcing pending metadata.
                let image = checker.intrinsics().string;
                assert_eq!(
                    checker.instantiate_type(
                        parameter,
                        &[(parameter, image)],
                        &[parameter],
                        &["T"]
                    ),
                    image
                );
                for _ in 0..3 {
                    for name in ["stringResult", "numberResult", "stringAgain", "numberAgain"] {
                        let result = checker.get_type_of_symbol(symbol(bound, root, name));
                        // Native has recursive mapped objects here. The current
                        // eager mapper cannot rebuild that cycle; an honest gap
                        // must not become a cached unchanged T or guessed any.
                        assert_eq!(result, checker.intrinsics().error);
                        assert_eq!(
                            checker.resolve_call_signature(bar, None).unwrap().r#type,
                            original
                        );
                    }
                }
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn constructor_return_expressions_do_not_infer_constructor_signature_returns() {
    with_checker(
        "class C { constructor() { return make(); } } function make() { return new C(); }",
        |checker, bound, root| {
            checker.set_no_implicit_any(true);
            for _ in 0..3 {
                let returned =
                    checker.get_signatures_of_symbol(symbol(bound, root, "make")).unwrap()[0]
                        .r#type;
                assert_eq!(checker.type_to_string(returned), "C");
            }
            assert!(checker.diagnostics().is_empty());
        },
    );
}

#[test]
fn circular_call_node_rechecks_after_failed_pop_without_promoting_unported_nodes() {
    for source in [
        "function left() { return right(); } function right() { return left(); }",
        "function unported() { return import('missing'); }",
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "control.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.set_no_implicit_any(true);
        let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
        let name = if source.contains("unported") { "unported" } else { "left" };
        checker.get_signatures_of_symbol(symbol(&bound, root, name)).unwrap();
        let mut pending = vec![Node::SourceFile(parsed.source_file)];
        let mut return_count = 0;
        while let Some(node) = pending.pop() {
            tsr_ast::push_children(node, &mut pending);
            if let Node::ReturnStatement(returned) = node {
                let expression = returned.expression.unwrap();
                for _ in 0..3 {
                    let ty = checker.check_expression(expression);
                    assert_eq!(
                        checker.type_to_string(ty),
                        if name == "unported" { "error" } else { "any" }
                    );
                }
                return_count += 1;
            }
        }
        assert_eq!(return_count, if name == "unported" { 1 } else { 2 });
        if name == "left" {
            assert_eq!(
                checker.diagnostics().iter().filter(|(_, d)| d.message.code() == 7023).count(),
                2
            );
        }
    }
}

#[test]
fn mutual_callable_values_keep_distinct_identities_in_either_order() {
    for first in ["left", "right"] {
        with_checker(
            "function left() { return right; } function right() { return left; }",
            |checker, bound, root| {
                checker.get_signatures_of_symbol(symbol(bound, root, first)).unwrap();
                let left = symbol(bound, root, "left");
                let right = symbol(bound, root, "right");
                let l = checker.get_type_of_symbol(left);
                let r = checker.get_type_of_symbol(right);
                assert_ne!(l, r);
                for _ in 0..3 {
                    assert_eq!(checker.get_signatures_of_symbol(left).unwrap()[0].r#type, r);
                    assert_eq!(checker.get_signatures_of_symbol(right).unwrap()[0].r#type, l);
                }
            },
        );
    }
}

#[test]
fn recursive_calls_fall_back_on_return_slots_not_whole_callable_types() {
    for first in ["callLeft", "callRight"] {
        with_checker(
            "function callLeft() { return callRight(); } function callRight() { return callLeft(); }",
            |checker, bound, root| {
                checker.set_no_implicit_any(true);
                checker.get_signatures_of_symbol(symbol(bound, root, first)).unwrap();
                for name in ["callLeft", "callRight"] {
                    let f = symbol(bound, root, name);
                    let ty = checker.get_type_of_symbol(f);
                    let returned = checker.get_signatures_of_symbol(f).unwrap()[0].r#type;
                    assert_ne!(ty, returned, "the callable itself must not become any");
                    assert_eq!(checker.type_to_string(returned), "any");
                    checker.get_signatures_of_symbol(f).unwrap();
                }
                let codes: Vec<_> =
                    checker.diagnostics().iter().map(|(_, d)| d.message.code()).collect();
                assert_eq!(codes, [7023, 7023], "one error per participating return slot");
            },
        );
    }
}

#[test]
fn self_calls_with_a_grounded_alternative_are_not_circular_value_returns() {
    with_checker(
        "function direct() { return direct(); } function asymmetric(n: number) { if (n) return asymmetric(n - 1); return 'ground'; }",
        |checker, bound, root| {
            for (name, expected) in [("direct", "never"), ("asymmetric", "string")] {
                let f = symbol(bound, root, name);
                for _ in 0..3 {
                    let returned = checker.get_signatures_of_symbol(f).unwrap()[0].r#type;
                    assert_eq!(checker.type_to_string(returned), expected);
                }
            }
        },
    );
}

#[test]
fn direct_arrow_and_expression_initializers_reuse_their_callable_identity() {
    for source in [
        "const f = () => f; const result = f();",
        "const f = function () { return f; }; const result = f();",
        "const f = function named() { return named; }; const result = f();",
    ] {
        with_checker(source, |checker, bound, root| {
            let f = symbol(bound, root, "f");
            let result = symbol(bound, root, "result");
            let ty = checker.get_type_of_symbol(f);
            assert_eq!(checker.get_type_of_symbol(result), ty, "{source}");
            assert_eq!(checker.get_type_of_symbol(f), ty);
        });
    }
}

#[test]
fn recursive_function_and_constructor_annotations_keep_identity() {
    for source in [
        "declare const f: () => typeof f; const result = f();",
        "declare const f: new () => typeof f; const result = new f();",
        "declare const f: (x: typeof f) => typeof x; const result = f(f);",
    ] {
        with_checker(source, |checker, bound, root| {
            let f = symbol(bound, root, "f");
            let result = symbol(bound, root, "result");
            let ty = checker.get_type_of_symbol(f);
            assert_eq!(checker.get_type_of_symbol(result), ty);
            assert_eq!(checker.get_type_of_symbol(f), ty);
        });
    }
}

#[test]
fn contextual_literals_and_mapped_returns_remain_distinct_after_warm_queries() {
    with_checker(
        "function identity<T>(x: T): T { return x; } const n = identity(23); const s = identity('mapped'); const a: () => 'one' = () => 'one'; const b: () => 7 = () => 7; const aResult = a(); const bResult = b();",
        |checker, bound, root| {
            for _ in 0..3 {
                for (name, expected) in
                    [("n", "23"), ("s", "\"mapped\""), ("aResult", "\"one\""), ("bResult", "7")]
                {
                    let ty = checker.get_type_of_symbol(symbol(bound, root, name));
                    assert_eq!(checker.type_to_string(ty), expected);
                }
            }
            checker
                .check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
        },
    );
}

#[test]
fn effect_only_self_calls_do_not_poison_return_slots() {
    for source in [
        "const f = function named() { named(); }; const result = f();",
        "const f: () => void = function named() { named(); }; const result = f();",
        "const f = function named(x: number) { named(x); }; const result = f(2);",
    ] {
        with_checker(source, |checker, bound, root| {
            checker.set_no_implicit_any(true);
            for _ in 0..3 {
                let result = checker.get_type_of_symbol(symbol(bound, root, "result"));
                assert_eq!(checker.type_to_string(result), "void", "{source}");
            }
            assert!(checker.diagnostics().is_empty(), "{source}");
        });
    }
}

#[test]
fn jsdoc_effects_distinguish_named_self_calls_from_annotated_and_inferred_never() {
    with_checker_in_file(
        "/** @typedef {(a: string, b: number) => void} Fn */\n/** @type {Fn} */\nconst callback = /** @param {string} a @param {number} [b] */ function self(a, b) { self(''); };\n/** @returns {never} */\nfunction stop() { throw 0; }\nfunction inferredStop() { throw 0; }\nconst noEnd = () => { stop(); };\nconst hasEnd = () => { inferredStop(); };",
        "control.js",
        |checker, bound, root| {
            checker.set_no_implicit_any(true);
            checker.set_strict_null_checks(true);
            for _ in 0..3 {
                for (name, expected) in [("noEnd", "() => never"), ("hasEnd", "() => void")] {
                    let ty = checker.get_type_of_symbol(symbol(bound, root, name));
                    assert_eq!(checker.type_to_string(ty), expected);
                }
                checker.get_type_of_symbol(symbol(bound, root, "callback"));
            }
            checker
                .check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
            assert!(checker.diagnostics().is_empty());
        },
    );
}

#[test]
fn cold_effect_helper_does_not_cache_a_provisional_outer_return() {
    for first in ["outer", "helper"] {
        with_checker(
            "function outer(b: boolean) { if (b) return 1; helper(); } function helper() { return outer(false); }",
            |checker, bound, root| {
                checker.set_strict_null_checks(true);
                checker.set_no_implicit_any(true);
                checker.get_signatures_of_symbol(symbol(bound, root, first)).unwrap();
                for _ in 0..3 {
                    for name in ["outer", "helper"] {
                        let returned =
                            checker.get_signatures_of_symbol(symbol(bound, root, name)).unwrap()[0]
                                .r#type;
                        assert_eq!(checker.type_to_string(returned), "1 | undefined");
                    }
                }
                assert!(checker.diagnostics().is_empty());
            },
        );
    }
}

#[test]
fn effects_preflight_preserves_explicit_never_and_predicates() {
    with_checker(
        "declare function stop(): never; const noEnd = () => { stop(); }; function isString(x: unknown) { return typeof x === 'string'; } function explicit(x: unknown): x is string { return typeof x === 'string'; } function inferred(x: unknown) { if (isString(x)) return x; return 'fallback'; } function annotated(x: unknown) { if (explicit(x)) return x; return 'fallback'; }",
        |checker, bound, root| {
            checker.set_strict_null_checks(true);
            let no_end = checker.get_type_of_symbol(symbol(bound, root, "noEnd"));
            assert_eq!(checker.type_to_string(no_end), "() => never");
            for _ in 0..3 {
                for name in ["inferred", "annotated"] {
                    let returned =
                        checker.get_signatures_of_symbol(symbol(bound, root, name)).unwrap()[0]
                            .r#type;
                    assert_eq!(checker.type_to_string(returned), "string");
                }
            }
        },
    );
}

#[test]
fn constant_self_calls_and_mutable_or_shadowed_aliases_remain_distinct() {
    for (source, expected) in [
        ("const f = () => { return f(); }; const result = f();", "never"),
        ("const f = function named() { return named(); }; const result = f();", "never"),
        ("let f = () => { return f(); }; const result = f();", "any"),
        ("function f(f: () => number) { return f(); } const result = f(() => 17);", "number"),
    ] {
        with_checker(source, |checker, bound, root| {
            for _ in 0..3 {
                let result = checker.get_type_of_symbol(symbol(bound, root, "result"));
                assert_eq!(checker.type_to_string(result), expected, "{source}");
            }
        });
    }
}
