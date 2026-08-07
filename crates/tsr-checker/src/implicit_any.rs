//! `noImplicitAny` for **parameters** — TS7006 and TS7019.
//!
//! `reportImplicitAny` (`checker.go:18275`), reached from
//! `reportErrorsFromWidening` (`checker.go:20452`) when a declaration's widened
//! type still carries `ObjectFlagsContainsWideningType`.
//!
//! # Why this is a syntactic rule here and a widening rule upstream
//!
//! Upstream decides *"is this an implicit any"* from the type: the parameter has
//! no annotation, nothing supplied one, and what came out of widening still
//! contains the widening marker. This port has no widening marker, so the same
//! question is asked of the syntax — **and it can only be asked safely where the
//! answer cannot depend on a contextual type**, because contextual typing is the
//! one thing that supplies a parameter's type without an annotation and this
//! port's version of it is partial (`docs/architecture/checker-notes-*`, the
//! refused `ArrowFunction` row).
//!
//! So the rule fires only for parameters of declarations that **cannot** be
//! contextually typed: a `function` declaration, a class method, a constructor.
//! A function expression, an arrow, an object-literal method and an accessor are
//! each declined by construction, not by measurement — every one of them has a
//! route by which upstream supplies the type and this port does not.
//!
//! # The blast radius is the option
//!
//! Like [`crate::unused`], nothing here fires unless the case writes
//! `@noImplicitAny` (or `@strict`), so a wrong answer can only reach the cases
//! that opted in.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// Every parameter of one function-like declaration that is an implicit
    /// `any`.
    ///
    /// Called from the walk at the *declaration*, not at the parameter, because
    /// the decision is about the declaration's kind and asking it once per
    /// parameter would ask it N times.
    pub(crate) fn check_implicit_any_parameters(&mut self, node: NodeId, ambient: bool) {
        if !self.no_implicit_any || self.file_has_parse_errors {
            return;
        }
        if !self.parameters_cannot_be_contextually_typed(node) {
            return;
        }
        // `reportImplicitAny` (`checker.go:18276`) already declines a `.js` file
        // without `checkJs`; this declines **every** `.js` file, and the reason
        // is JSDoc. A `@param {string} x` supplies the type upstream reads and
        // this port does not parse into one, so a checked JS file reports an
        // implicit any on every annotated parameter. It was the *whole* of the
        // first measurement's wrong column — 28 lines, 12 cases, all `.js`, and
        // `typedefOnStatements` alone was 15 of them.
        if self.in_js_file(node) {
            return;
        }
        let parameters: Vec<NodeId> = self.implicit_any_candidates(node);
        for parameter in parameters {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                continue;
            };
            if declaration.r#type.is_some() || declaration.initializer.is_some() {
                continue;
            }
            // A binding pattern parameter reports TS7031 per element, which is
            // its own row.
            let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { continue };
            // `this` is not a parameter for this purpose — it has no symbol and
            // upstream's widening never reaches it.
            if name.text == "this" {
                continue;
            }
            // "Report an implicit any error if there is no body … and node is
            // not a private method in an ambient context" is the *return type*
            // arm; a parameter in an ambient context still reports, and
            // `ParameterList4` is an ordinary declaration. The ambient flag is
            // threaded here only so a `.d.ts` stays silent, matching every other
            // rule in `crate::check`.
            if ambient {
                continue;
            }
            let rest = declaration.dot_dot_dot_token.is_some();
            let message = if rest {
                &messages::REST_PARAMETER_0_IMPLICITLY_HAS_AN_ANY_TYPE
            } else {
                &messages::PARAMETER_0_IMPLICITLY_HAS_AN_1_TYPE
            };
            let Some(file) = self.source_file_of_for_diagnostics(parameter) else { continue };
            // The position is the **parameter declaration**, modifiers included:
            // `ParameterList4.ts(1,12)` for `function F(public A)` is the
            // `public`, not the `A`.
            let span = self.error_span(parameter);
            let text = name.text.to_string();
            let diagnostic = if rest {
                Diagnostic::with_args(message, span, [text])
            } else {
                Diagnostic::with_args(message, span, [text, "any".to_string()])
            };
            self.report(file, diagnostic);
        }
    }

    /// Can this declaration's parameters get their types from a contextual
    /// signature?
    ///
    /// `getContextualSignatureForFunctionLikeDeclaration` (`checker.go:20464`)
    /// answers for the shapes that *can*; the shapes below are the ones for
    /// which it structurally cannot, and they are the whole of what this rule
    /// admits.
    ///
    /// A method written inside an **object literal** is contextually typed by
    /// the literal's contextual type, so it is excluded even though it shares a
    /// node kind with a class method. An accessor is excluded because a `set`
    /// accessor's parameter type comes from its `get` counterpart.
    fn parameters_cannot_be_contextually_typed(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(_) | Node::ConstructorDeclaration(_)) => true,
            Some(Node::MethodDeclaration(_)) => self.nodes.parent(node).is_some_and(|parent| {
                self.nodes.kind(parent) != SyntaxKind::ObjectLiteralExpression
            }),
            // A function expression or arrow **can** be contextually typed, so
            // it is admitted only in the two positions where nothing can supply
            // a signature: an initialiser with no annotation on the variable,
            // and a bare expression statement. Every other position — an
            // argument, an annotated declaration, a property assignment, a
            // return, a JSX attribute, an `as` — has a contextual type this port
            // computes only partially, and that partiality is a *wrong* TS7006.
            Some(Node::FunctionExpression(_) | Node::ArrowFunction(_)) => {
                match self.nodes.parent(node).and_then(|parent| self.node_map.get(parent)) {
                    Some(Node::VariableDeclaration(declaration)) => declaration.r#type.is_none(),
                    Some(Node::ExpressionStatement(_) | Node::ReturnStatement(_)) => true,
                    Some(Node::PropertyDeclaration(property)) => property.r#type.is_none(),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The parameter nodes of a declaration this rule admits.
    fn implicit_any_candidates(&self, node: NodeId) -> Vec<NodeId> {
        let collect = |parameters: &[&tsr_ast::ParameterDeclaration<'_>]| -> Vec<NodeId> {
            parameters.iter().filter_map(|parameter| parameter.node_id).collect()
        };
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => collect(n.parameters),
            Some(Node::MethodDeclaration(n)) => collect(n.parameters),
            Some(Node::ConstructorDeclaration(n)) => collect(n.parameters),
            Some(Node::FunctionExpression(n)) => collect(n.parameters),
            Some(Node::ArrowFunction(n)) => collect(n.parameters),
            _ => Vec::new(),
        }
    }
}
