//! `const data = [];` — the **symbol** is `any[]`, the **literal** is `never[]`.
//!
//! Upstream returns `c.autoArrayType` from `getTypeForVariableLikeDeclaration`
//! (`checker.go:16709`) before it ever consults the initialiser's type, and
//! leaves `checkArrayLiteral` (`checker.go:8098`) answering `never[]` for the
//! `[]` itself. Both lines appear on the same declaration:
//!
//! ```text
//! >data : any[]
//! >[] : never[]
//! ```
//!
//! # The falsifier is the second assertion in every test here
//!
//! A fix that types the **literal** instead of the symbol reads identically and
//! breaks **51 currently-right `>[] : never[]` lines**. So every test below
//! pins both halves, and `the_literal_is_still_never` exists for no other
//! reason. `bd tsr-5h0`; the sizing is
//! `docs/architecture/checker-notes-evolvearray.md`.

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
}

/// A minimal global `Array`, because these fixtures bind one file with no lib
/// and both `check_array_literal` and the new declaration arm reach
/// `global_type_symbol("Array")`. Without it every answer is `error` and the
/// tests pass or fail for a reason that has nothing to do with the arm.
const LIB: &str = "interface Array<T> { length: number; }\n";

fn bind<'a>(arena: &'a Arena, source: &str) -> Fixture<'a> {
    let source: &'a str = arena.alloc_str(&format!("{LIB}{source}"));
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let file = tsr_parser::parse_into(
        arena,
        source,
        tsr_parser::ParseOptions::default(),
        &mut nodes,
        &mut node_map,
    );
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind_into(
        BindResult::empty(),
        arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "/test.ts", text: source },
    );
    Fixture { nodes, node_map, bound }
}

/// The type of the **symbol** declared by the first variable declaration.
fn symbol_type(fixture: &Fixture<'_>) -> String {
    let mut checker = Checker::new(&fixture.bound, &fixture.nodes, &fixture.node_map);
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .find(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        .expect("the fixture declares a variable");
    let symbol = fixture.bound.symbol_of(declaration).expect("the declaration binds a symbol");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// The type of the **array literal expression** itself.
fn literal_type(fixture: &Fixture<'_>) -> String {
    let mut checker = Checker::new(&fixture.bound, &fixture.nodes, &fixture.node_map);
    let literal = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .find(|&id| fixture.nodes.kind(id) == SyntaxKind::ArrayLiteralExpression)
        .expect("the fixture contains an array literal");
    let node = fixture.node_map.get(literal).expect("the literal is in the map");
    let expression =
        tsr_ast::Expression::try_from(node).expect("an array literal is an expression");
    // Through `check_expression`, which is the path `types_producer` takes for
    // the `>[] : never[]` line — not through a private helper, so the test
    // exercises what the baseline exercises.
    let id = checker.check_expression(expression);
    checker.type_to_string(id)
}

#[test]
fn an_empty_array_initialiser_types_the_symbol_any_array() {
    let arena = Arena::new();
    let fixture = bind(&arena, "const data = [];\n");
    assert_eq!(symbol_type(&fixture), "any[]");
}

#[test]
fn the_literal_is_still_never() {
    // **The falsifier.** 51 corpus lines are this assertion. If the arm reached
    // into the literal rather than the declaration, this reads `any[]` and the
    // corpus loses every one of them while the gradient still appears to rise.
    let arena = Arena::new();
    for source in ["const data = [];\n", "let data = [];\n", "var data = [];\n"] {
        let fixture = bind(&arena, source);
        assert_eq!(literal_type(&fixture), "never[]", "the literal must not move: {source}");
    }
}

#[test]
fn a_non_empty_array_initialiser_is_untouched() {
    // The emptiness is the whole trigger — `isEmptyArrayLiteral`. `[1]` must
    // still take the ordinary widening path.
    let arena = Arena::new();
    let fixture = bind(&arena, "const data = [1];\n");
    assert_eq!(symbol_type(&fixture), "number[]");
}

#[test]
fn an_annotation_still_wins() {
    // Upstream returns `autoArrayType` only after `declaredType != nil` has
    // already returned (`checker.go:16694`). An annotation must not be
    // overridden by the arm.
    let arena = Arena::new();
    let fixture = bind(&arena, "const data: string[] = [];\n");
    assert_eq!(symbol_type(&fixture), "string[]");
}

#[test]
fn an_exported_or_ambient_declaration_is_excluded() {
    // Two of upstream's three syntactic guards (`checker.go:16698`).
    //
    // Asserted as "not `any[]`" rather than against an exact fallback: `export`
    // makes the fixture a module, so `LIB`'s `interface Array` stops being a
    // global and the ordinary path answers `error` rather than `never[]`. That
    // is a property of a one-file fixture with a hand-rolled lib, not of the
    // arm, and pinning it would pin the fixture.
    let arena = Arena::new();
    assert_ne!(symbol_type(&bind(&arena, "export const data = [];\n")), "any[]");
    // Ambient: the annotation is what answers, and the arm must not fire at all.
    assert_eq!(symbol_type(&bind(&arena, "declare const data: any[];\n")), "any[]");
}

#[test]
fn a_binding_pattern_is_excluded() {
    // The third guard, `!ast.IsBindingPattern(declaration.Name())`. A
    // destructuring declaration takes its type from the pattern.
    let arena = Arena::new();
    let fixture = bind(&arena, "const [first] = [];\n");
    assert_ne!(symbol_type(&fixture), "any[]", "a binding pattern must not take the arm");
}
