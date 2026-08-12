//! §226. `Checker::base_type_of_heritage_entry` — the checker half of the
//! `React.Component<Prop, {}>` row.
//!
//! Upstream's baseline writer records `GetTypeAtLocation(node.Parent)` for a
//! node whose parent is an `ExpressionWithTypeArguments` in a class `extends`
//! clause (`type_symbol_baseline.go:370-374`), which lands in `getTypeOfNode`'s
//! arm at `checker.go:31958` and answers the class's first base type. So
//! `class C extends Base<number, string>` records `Base<number, string>` where
//! this port records `typeof Base`.
//!
//! **These tests exist because the caller does not.** The call site is in
//! `tsr-conformance`'s `types_producer`, owned by the other lane, and shipping
//! a function with no caller and no test is how §215 measured `+0` — correct
//! code that never ran. Exercising it here means the boundary is handed over
//! verified rather than merely compiling.

use tsr_ast::{Node, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Render the base type of the first `extends` entry in `source`.
///
/// Resolves the entry's expression the way the producer will — by name, in
/// TYPE meaning — and hands the symbol plus the written argument nodes to the
/// function under test.
fn base_type_of_first_extends(source: &str) -> Option<String> {
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

    let entry = (0..u32::try_from(parsed.nodes.len()).expect("fits"))
        .map(tsr_ast::NodeId::new)
        .find(|&id| parsed.nodes.kind(id) == SyntaxKind::ExpressionWithTypeArguments)
        .and_then(|id| match parsed.node_map.get(id) {
            Some(Node::ExpressionWithTypeArguments(entry)) => Some(entry),
            _ => None,
        })
        .expect("the fixture has a heritage entry");

    let Some(tsr_ast::Expression::Identifier(name)) = entry.expression else {
        panic!("this helper only resolves a bare identifier base");
    };
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let base = bound
        .lookup_local(root, name.text)
        .unwrap_or_else(|| panic!("`{}` is declared", name.text));
    assert!(
        bound.symbols().get(base).flags.intersects(SymbolFlags::INTERFACE | SymbolFlags::CLASS),
        "the base resolves to a class or interface"
    );

    let id = checker.base_type_of_heritage_entry(base, entry.type_arguments)?;
    Some(checker.type_to_string(id))
}

#[test]
fn a_heritage_entry_with_type_arguments_answers_the_instantiated_base() {
    assert_eq!(
        base_type_of_first_extends(
            "interface Base<T, U> {}\ninterface C extends Base<number, string> {}"
        )
        .as_deref(),
        Some("Base<number, string>")
    );
    // A class, which is the shape all twelve witnesses are.
    assert_eq!(
        base_type_of_first_extends("class Base<T, U> {}\nclass C extends Base<number, {}> {}")
            .as_deref(),
        Some("Base<number, {}>")
    );
}

/// The two declines, each for a reason stated at the site rather than
/// discovered here.
#[test]
fn the_heritage_entry_declines_where_it_would_have_to_guess() {
    // No written arguments: upstream answers `B`, but that population is
    // unmeasured and the caller's fallback is today's answer, so it gaps
    // rather than riding along on a measured change.
    assert_eq!(
        base_type_of_first_extends("interface Base {}\ninterface C extends Base {}"),
        None
    );
}

/// An unresolvable type argument flows through, and this pins that rather than
/// wishing otherwise.
///
/// The first draft of the arm declined here, on the reasoning that
/// `Base<error>` is a wrong line rather than a gap. **This assertion is what
/// showed the guard named a case it could not detect**: the rendering is
/// `Base<Missing>`, because `get_type_from_type_node` answers an unresolved
/// name with a type printed by that name, never the error type. The guard was
/// removed instead of re-aimed — the spelling is whatever every other
/// reference in this port already produces, so the arm adds no new wrongness,
/// and inventing a detection here would be unmeasured machinery for a case the
/// corpus has not shown.
#[test]
fn an_unresolvable_type_argument_is_spelled_the_way_it_is_spelled_everywhere() {
    assert_eq!(
        base_type_of_first_extends("interface Base<T> {}\ninterface C extends Base<Missing> {}")
            .as_deref(),
        Some("Base<Missing>")
    );
}
