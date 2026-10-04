//! Hosted annotation ownership from native5b1047d, independent of auto/flow adoption.
use std::collections::BTreeMap;
use tsr_ast::{BindingName, Node};
use tsr_conformance::{TestCase, types_producer};

fn declared(body: &str, js: bool, strict: bool, check_js: bool) -> BTreeMap<String, String> {
    let name = if js { "fixture.js" } else { "fixture.ts" };
    let case = TestCase::parse(
        "probe/jsdoc-variable-selector",
        name,
        &format!("// @allowJs: true\n// @strict: {strict}\n// @checkJs: {check_js}\n{body}"),
    );
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file(name).unwrap();
    let mut pending = vec![Node::SourceFile(file.source_file())];
    let mut result = BTreeMap::new();
    while let Some(node) = pending.pop() {
        tsr_ast::push_children(node, &mut pending);
        if let Node::VariableDeclaration(variable) = node
            && let Some(BindingName::Identifier(name)) = variable.name
        {
            let symbol = program.binder().symbol_of(variable.node_id.unwrap()).unwrap();
            let cold = checker.get_type_of_symbol(symbol);
            let warm = checker.get_type_of_symbol(symbol);
            assert_eq!(cold, warm, "cached declared identity of {}", name.text);
            result.insert(name.text.to_string(), checker.type_to_string(cold));
        }
    }
    result
}

#[test]
fn last_document_and_direct_first_tag_are_distinct_from_statement_tag_allocation() {
    let body = r"function control() {
        /** @type {string} */ /** @type {boolean} */ var lastStatement = 17;
        /** @type {string} */ /** final prose */ var proseStatement = 19;
        var /** @type {string} */ /** @type {boolean} */ lastDirect = 23;
        var /** @type {string} */ /** final prose */ proseDirect = 29;
        var /** @type {string} @type {number} */ firstDirectTag = 31;
        /** @type {boolean} */ var /** @type {string} */ /** final prose */ fallback = 37, other = 41;
        /** @type {string} @type {boolean} */ var initialized = 43, uninitialized, remaining = 47;
        /** @type {string} @type {boolean} */ var /** @type {number} */ already = 53, eligible = 59, /** @type {string} */ later = 61, second = 67, unowned = 71;
        for (var /** @type {string} */ head = 0, headOther = 1; headOther < 2; headOther++) {}
        /** @type {boolean} */ for (var statementHead = 0; statementHead < 1; statementHead++) {}
    }";
    for strict in [true, false] {
        for check_js in [true, false] {
            let got = declared(body, true, strict, check_js);
            for (name, want) in [
                ("lastStatement", "boolean"),
                ("proseStatement", "number"),
                ("lastDirect", "boolean"),
                ("proseDirect", "number"),
                ("firstDirectTag", "string"),
                ("fallback", "boolean"),
                ("other", "number"),
                ("initialized", "string"),
                ("uninitialized", "boolean"),
                ("remaining", "number"),
                ("already", "number"),
                ("eligible", "string"),
                ("later", "string"),
                ("second", "boolean"),
                ("unowned", "number"),
                ("head", "string"),
                ("headOther", "number"),
                ("statementHead", "number"),
            ] {
                assert_eq!(got[name], want, "{name}: strict={strict},checkJs={check_js}");
            }
            let ts = declared(body, false, strict, check_js);
            assert_eq!(ts["lastStatement"], "number");
            assert_eq!(ts["firstDirectTag"], "number");
            assert_eq!(ts["uninitialized"], "any");
            assert_eq!(ts["head"], "number");
        }
    }
}

#[test]
fn written_and_direct_annotated_siblings_are_skipped_without_inspecting_initializers() {
    // Written type syntax in JS is ownership-only evidence; the TS row is valid syntax.
    let body = r"function control() {
        /** @type {string} @type {boolean} */ var written: number = 17, /** @type {number} */ direct = 19, third = 23, fourth, fifth = true;
        var /** @type {any} */ completeAny = 47;
    }";
    let js = declared(body, true, true, true);
    for (name, want) in [
        ("written", "number"),
        ("direct", "number"),
        ("third", "string"),
        ("fourth", "boolean"),
        ("fifth", "boolean"),
        ("completeAny", "any"),
    ] {
        assert_eq!(js[name], want, "{name}");
    }
    let ts = declared(body, false, true, true);
    assert_eq!(ts["written"], "number");
    assert_eq!(ts["fourth"], "any");
}

#[test]
fn null_and_undefined_initializers_do_not_consume_annotation_eligibility() {
    let body = r"function control() {
        var /** @type {string} */ direct = null;
        /** @type {boolean} @type {number} */ var first = null, second = undefined, other = null;
        /** @type {string} */ var /** final prose */ fallback = null, unowned = null;
    }";
    for strict in [true, false] {
        for check_js in [true, false] {
            let js = declared(body, true, strict, check_js);
            for (name, want) in [
                ("direct", "string"),
                ("first", "boolean"),
                ("second", "number"),
                ("other", "any"),
                ("fallback", "string"),
                ("unowned", "any"),
            ] {
                assert_eq!(js[name], want, "{name}: strict={strict},checkJs={check_js}");
            }
        }
    }
}

#[test]
fn mixed_unrepresented_trees_and_iteration_consumers_keep_their_legacy_boundary() {
    let body = r"function control() {
        /** @type {string} @typedef {Object} Shape */ var mixed = 17;
        var /** @type {string} @callback Handler */ directCallback = 19;
        /** @callback Other @type {string} */ var statementCallback = 23;
        /** @type {string} */ /** @type */ var incomplete = 37;
        for (var /** @type {number} */ key in {a: 1}) {}
        for (var /** @type {boolean} */ item of [29, 31]) {}
    }";
    let got = declared(body, true, true, true);
    // These are explicit retained legacy controls, not general native equivalence.
    assert_eq!(got["mixed"], "number");
    assert_eq!(got["directCallback"], "number");
    assert_eq!(got["statementCallback"], "string");
    assert_eq!(got["incomplete"], "string");
    assert_eq!(got["key"], "string");
    assert_eq!(got["item"], "number");
}
