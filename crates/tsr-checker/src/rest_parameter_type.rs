//! TS2370 — `A rest parameter must be of an array type.`
//!
//! `checkParameter` (`checker.go:2696`) asks the relation:
//! `isTypeAssignableTo(getTypeOfSymbol(node.Symbol()), anyReadonlyArrayType)`.
//! The relation is not this workstream's; the **written** annotation is, and
//! every line the corpus asks for is a written annotation of a kind that can
//! never be array-like.
//!
//! `docs/architecture/checker-notes-diag2.md` §984.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// Interface declarations in `lib.d.ts` that a rest parameter may legally name.
const ARRAY_LIKE: &[&str] =
    &["Array", "ReadonlyArray", "ArrayLike", "Iterable", "IterableIterator", "ConcatArray"];

impl Checker<'_, '_> {
    /// One parameter declaration.
    pub(crate) fn check_rest_parameter_type(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(node) else { return };
        if parameter.dot_dot_dot_token.is_none() {
            return;
        }
        // Upstream skips a binding pattern: `checkGrammarParameterList` has
        // already reported and the type question is moot.
        if !matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return;
        }
        let Some(annotation) = parameter.r#type.and_then(|t| t.node_id()) else { return };
        if !self.written_type_is_never_array_like(annotation) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(&messages::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE, span),
        );
    }

    /// Can this **written** type never be array-like?
    ///
    /// Answers `false` for everything it is not sure about — an `ArrayType`, a
    /// tuple, `readonly T[]`, `any`, a type parameter, a union, and above all a
    /// **type alias**, since `type A = string[]` is a reference that resolves to
    /// a `TypeAliasDeclaration` and is legal. §984.
    fn written_type_is_never_array_like(&mut self, annotation: NodeId) -> bool {
        match self.nodes.kind(annotation) {
            SyntaxKind::StringKeyword
            | SyntaxKind::NumberKeyword
            | SyntaxKind::BooleanKeyword
            | SyntaxKind::SymbolKeyword
            | SyntaxKind::ObjectKeyword
            | SyntaxKind::VoidKeyword
            | SyntaxKind::NeverKeyword
            | SyntaxKind::UnknownKeyword
            | SyntaxKind::BigIntKeyword => true,
            SyntaxKind::TypeReference => self.type_reference_names_a_class_or_interface(annotation),
            _ => false,
        }
    }

    /// Does this reference name a class or interface declaration that is not
    /// one of `lib.d.ts`'s array-like interfaces? §984.
    fn type_reference_names_a_class_or_interface(&mut self, annotation: NodeId) -> bool {
        let Some(Node::TypeReferenceNode(reference)) = self.node_map.get(annotation) else {
            return false;
        };
        let Some(name) = reference.type_name.and_then(|n| n.node_id()) else { return false };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return false };
        if ARRAY_LIKE.contains(&text.as_str()) {
            return false;
        }
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, name, &text, SymbolFlags::TYPE)
        else {
            return false;
        };
        let declarations =
            self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
        // **A declaration with a heritage clause may inherit array-likeness.**
        // `interface MyThing extends Array<any> { }` is assignable to
        // `readonly any[]` and upstream does not report — even though
        // `restParametersOfNonArrayTypes2`'s own header comment says *"user
        // defined subtypes of array do not count, all of these are errors"*.
        // **The baseline is the oracle and it wants only TS1014 there**; taking
        // the comment at its word was §984's first measurement, 28 wrong lines
        // in that one fixture. §985.
        !declarations.is_empty()
            && declarations.iter().all(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ClassDeclaration
                        | SyntaxKind::ClassExpression
                        | SyntaxKind::InterfaceDeclaration
                ) && !self.declaration_has_heritage_clause(declaration)
            })
    }

    /// Does this declaration list any base type? §985.
    fn declaration_has_heritage_clause(&self, declaration: NodeId) -> bool {
        let clauses = match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(n)) => n.heritage_clauses,
            Some(Node::ClassExpression(n)) => n.heritage_clauses,
            Some(Node::InterfaceDeclaration(n)) => n.heritage_clauses,
            _ => return false,
        };
        !clauses.is_empty()
    }
}
