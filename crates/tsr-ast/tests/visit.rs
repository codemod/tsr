//! The generated visitor actually descends.
//!
//! Building a small tree by hand and counting what the walker reaches — a visitor
//! that compiles but silently skips children would otherwise look fine until the
//! binder started missing declarations.

use tsr_ast::{
    BinaryExpression, Expression, Identifier, Node, SyntaxKind, Token, Visit, visit::walk_node,
};

/// Records every node the walk reaches.
#[derive(Default)]
struct Recorder {
    identifiers: Vec<String>,
    binary_expressions: usize,
}

impl<'a> Visit<'a> for Recorder {
    fn visit_identifier(&mut self, node: &'a Identifier<'a>) {
        self.identifiers.push(node.text.to_string());
    }

    fn visit_binary_expression(&mut self, node: &'a BinaryExpression<'a>) {
        self.binary_expressions += 1;
        // Continue into children; omitting this would prune the subtree.
        tsr_ast::visit::walk_binary_expression(self, node);
    }
}

#[test]
fn walk_reaches_nested_children() {
    // `a + b` nested as `(a + b) + c`.
    let a = Identifier::new("a");
    let b = Identifier::new("b");
    let c = Identifier::new("c");
    let plus = Token::new(SyntaxKind::PlusToken);

    let inner = BinaryExpression::new(
        &[],
        Some(Expression::Identifier(&a)),
        None,
        Some(&plus),
        Some(Expression::Identifier(&b)),
    );
    let outer = BinaryExpression::new(
        &[],
        Some(Expression::BinaryExpression(&inner)),
        None,
        Some(&plus),
        Some(Expression::Identifier(&c)),
    );

    let mut recorder = Recorder::default();
    walk_node(&mut recorder, Node::BinaryExpression(&outer));

    assert_eq!(recorder.binary_expressions, 2, "should reach both binary expressions");
    assert_eq!(
        recorder.identifiers,
        vec!["a", "b", "c"],
        "should reach every identifier, in source order"
    );
}

#[test]
fn not_calling_walk_prunes_the_subtree() {
    /// Visits binary expressions but never descends.
    #[derive(Default)]
    struct Shallow {
        identifiers: usize,
    }

    impl<'a> Visit<'a> for Shallow {
        fn visit_identifier(&mut self, _node: &'a Identifier<'a>) {
            self.identifiers += 1;
        }
        fn visit_binary_expression(&mut self, _node: &'a BinaryExpression<'a>) {
            // Deliberately does not call `walk_binary_expression`.
        }
    }

    let a = Identifier::new("a");
    let b = Identifier::new("b");
    let plus = Token::new(SyntaxKind::PlusToken);
    let expr = BinaryExpression::new(
        &[],
        Some(Expression::Identifier(&a)),
        None,
        Some(&plus),
        Some(Expression::Identifier(&b)),
    );

    let mut shallow = Shallow::default();
    walk_node(&mut shallow, Node::BinaryExpression(&expr));
    assert_eq!(shallow.identifiers, 0, "pruned subtree should not be visited");
}
