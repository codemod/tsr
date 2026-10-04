//! Native 5b1047d real-Program controls archived in typeof-this-wave25.
//! Matching controls also cover the wave47 receiver-query admission boundary.
use crate::Checker;
use tsr_ast::{Node, NodeId, SyntaxKind, TypeNode};

fn with_checker(source: &str, test: impl Fn(&mut Checker<'_, '_>)) {
    for strict in [false, true] {
        for precheck in [false, true] {
            let arena = tsr_core::Arena::new();
            let parsed = tsr_parser::parse(&arena, source);
            assert!(parsed.diagnostics.is_empty());
            let root = parsed.source_file.node_id.expect("registered file");
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "query-this.ts", text: source },
            );
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.set_strict_null_checks(strict);
            if precheck {
                checker.check_source_file(
                    root,
                    crate::check::FileContext { ambient: false, has_parse_errors: false },
                );
            }
            test(&mut checker);
            test(&mut checker);
        }
    }
}

fn node(checker: &Checker<'_, '_>, source: &str, kind: SyntaxKind, text: &str) -> NodeId {
    (0..checker.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).expect("node count fits")))
        .find(|&id| {
            let span = checker.nodes.span(id);
            checker.nodes.kind(id) == kind
                && source[span.start as usize..span.end as usize].trim() == text
        })
        .unwrap_or_else(|| panic!("missing {kind:?}: {text}"))
}

fn query_reference(checker: &Checker<'_, '_>, source: &str, text: &str) -> NodeId {
    let id = node(checker, source, SyntaxKind::TypeQuery, text);
    let Some(Node::TypeQueryNode(query)) = checker.node_map.get(id) else { unreachable!() };
    query.expr_name.and_then(|name| name.node_id()).expect("query reference")
}

#[test]
fn query_this_matches_only_runtime_this_after_target_normalization() {
    let source = r#"function f(this: {p: "left"}) {type T = typeof this;
        this; (this); this!; this satisfies {p: "left"}; (this = this); (17, this);}"#;
    with_checker(source, |checker| {
        let query = query_reference(checker, source, "typeof this");
        assert!(!checker.references_match(query, query));
        assert!(!checker.is_constant_reference(query));
        for (kind, text, expected) in [
            (SyntaxKind::ThisKeyword, "this", true),
            (SyntaxKind::ParenthesizedExpression, "(this)", true),
            (SyntaxKind::NonNullExpression, "this!", true),
            (SyntaxKind::SatisfiesExpression, r#"this satisfies {p: "left"}"#, false),
            (SyntaxKind::BinaryExpression, "this = this", true),
            (SyntaxKind::BinaryExpression, "17, this", true),
        ] {
            let target = node(checker, source, kind, text);
            assert_eq!(checker.references_match(query, target), expected, "{text}");
            assert!(!checker.references_match(target, query), "reverse {text}");
        }
        let runtime = node(checker, source, SyntaxKind::ThisKeyword, "this");
        assert!(checker.is_constant_reference(runtime));
        assert!(checker.references_match(runtime, runtime));
    });
}

#[test]
fn qualified_query_this_does_not_match_itself_or_a_sibling() {
    let source = r#"function f(this: {p: {q: "left"}; z: 17}) {
        type P = typeof this.p; type Q = typeof this.p.q;
        this.p; this.p.q; (this.p); this.p!;
        this.p satisfies {q: "left"}; (this.p = this.p); (17, this.p); this.z;}"#;
    with_checker(source, |checker| {
        let query = query_reference(checker, source, "typeof this.p");
        let nested = query_reference(checker, source, "typeof this.p.q");
        for id in [query, nested] {
            assert!(!checker.references_match(id, id), "qualified self");
            assert!(!checker.is_constant_reference(id));
        }
        for (kind, text, expected) in [
            (SyntaxKind::PropertyAccessExpression, "this.p", true),
            (SyntaxKind::ParenthesizedExpression, "(this.p)", true),
            (SyntaxKind::NonNullExpression, "this.p!", true),
            (SyntaxKind::SatisfiesExpression, r#"this.p satisfies {q: "left"}"#, false),
            (SyntaxKind::BinaryExpression, "this.p = this.p", true),
            (SyntaxKind::BinaryExpression, "17, this.p", true),
            (SyntaxKind::PropertyAccessExpression, "this.z", false),
        ] {
            let target = node(checker, source, kind, text);
            assert_eq!(checker.references_match(query, target), expected, "{text}");
            assert!(!checker.references_match(target, query), "reverse {text}");
        }
        let target = node(checker, source, SyntaxKind::PropertyAccessExpression, "this.p.q");
        assert!(checker.references_match(nested, target));
        assert!(!checker.references_match(target, nested));
    });
}

#[test]
fn property_named_this_is_not_a_query_this_root_and_ordinary_matching_is_retained() {
    let source = r#"declare const obj: {this: "left"; p: 17};
        type T = typeof obj.this; obj.this; (obj.this); obj.this!;
        obj.this satisfies "left"; obj.p;
        function f(this: {this: {p: "inner"}}) {type P = typeof this.this.p; this.this.p;}"#;
    with_checker(source, |checker| {
        let ordinary = query_reference(checker, source, "typeof obj.this");
        let Some(Node::QualifiedName(qualified)) = checker.node_map.get(ordinary) else {
            unreachable!()
        };
        let name = qualified.right.and_then(|name| name.node_id).expect("property name");
        assert!(!checker.is_this_in_type_query(name));
        let object = node(checker, source, SyntaxKind::Identifier, "obj");
        assert!(checker.is_constant_reference(object));
        assert!(checker.references_match(ordinary, ordinary), "ordinary shortcut retained");
        let access = node(checker, source, SyntaxKind::PropertyAccessExpression, "obj.this");
        assert!(checker.references_match(ordinary, access));
        assert!(!checker.references_match(
            ordinary,
            node(checker, source, SyntaxKind::PropertyAccessExpression, "obj.p"),
        ));
        for (kind, text) in [
            (SyntaxKind::ParenthesizedExpression, "(obj.this)"),
            (SyntaxKind::NonNullExpression, "obj.this!"),
            (SyntaxKind::SatisfiesExpression, r#"obj.this satisfies "left""#),
        ] {
            let wrapper = node(checker, source, kind, text);
            assert!(checker.references_match(wrapper, access), "source wrapper {text}");
        }
        let query = query_reference(checker, source, "typeof this.this.p");
        assert!(!checker.references_match(query, query));
        assert!(checker.references_match(
            query,
            node(checker, source, SyntaxKind::PropertyAccessExpression, "this.this.p"),
        ));
    });
}

#[test]
fn dotted_and_bare_queries_read_their_native_receiver_flow() {
    let source = r#"function f(this: {p: "left" | undefined; q: 17}) {
        if (this.p) {type P = typeof this.p; type Q = typeof this.q; type T = typeof this;}}
    "#;
    with_checker(source, |checker| {
        for (text, expected) in [("typeof this.p", "\"left\""), ("typeof this.q", "17")] {
            let id = node(checker, source, SyntaxKind::TypeQuery, text);
            let ty = checker.get_type_from_type_node(
                TypeNode::try_from(checker.node_map.get(id).unwrap()).unwrap(),
            );
            assert_eq!(checker.type_to_string(ty), expected);
        }
        let id = node(checker, source, SyntaxKind::TypeQuery, "typeof this");
        let ty = checker.get_type_from_type_node(
            TypeNode::try_from(checker.node_map.get(id).unwrap()).unwrap(),
        );
        assert!(checker.store.get(ty).flags.contains(crate::flags::TypeFlags::OBJECT));
        let property = checker.get_type_of_property_of_type(ty, "q").expect("receiver property");
        assert_eq!(checker.type_to_string(property), "17");
        let query = query_reference(checker, source, "typeof this");
        let Some(Node::Identifier(identifier)) = checker.node_map.get(query) else {
            unreachable!()
        };
        assert_eq!(checker.check_expression(tsr_ast::Expression::Identifier(identifier)), ty);
        let runtime = node(checker, source, SyntaxKind::ThisKeyword, "this");
        assert_eq!(checker.check_this_expression(runtime), ty, "native receiver identity");
    });
}

#[test]
fn receiver_query_follows_arrow_creation_flow_but_not_an_opaque_receiver() {
    let source = r"type Wrong = {value: number}; type Right = {name: string};
        declare function isRight(value: any): value is Right;
        declare function accept(callback: () => void): void;
        function f(this: Right | Wrong) {
            if (!isRight(this)) return;
            accept(() => {type Arrow = typeof this;});
            accept(function() {type Opaque = typeof this;});
        }";
    with_checker(source, |checker| {
        for (alias, expected) in [("Arrow", "Right"), ("Opaque", "any")] {
            let declaration = node(
                checker,
                source,
                SyntaxKind::TypeAliasDeclaration,
                &format!("type {alias} = typeof this;"),
            );
            let Some(Node::TypeAliasDeclaration(alias)) = checker.node_map.get(declaration) else {
                unreachable!()
            };
            let ty = checker.get_type_from_type_node(alias.r#type.expect("query annotation"));
            assert_eq!(checker.type_to_string(ty), expected);
        }
    });
}

#[test]
fn circular_receiver_query_uses_symbol_resolution_instead_of_reentering_annotation() {
    let source = "function f(this: typeof this) { type T = typeof this; this; }";
    with_checker(source, |checker| {
        let query = node(checker, source, SyntaxKind::TypeQuery, "typeof this");
        let ty = checker.get_type_from_type_node(
            TypeNode::try_from(checker.node_map.get(query).unwrap()).unwrap(),
        );
        assert_eq!(checker.type_to_string(ty), "any");
    });
}
