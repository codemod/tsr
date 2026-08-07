//! TS2554 / TS2555 — a call written with the wrong number of arguments.
//!
//! `getArgumentArityError` (`checker.go:9705`), reached from
//! `reportCallResolutionErrors` when no candidate signature accepts the argument
//! count.
//!
//! # The same trick as [`crate::type_argument_arity`]
//!
//! Upstream asks a *`Signature`* for `getMinArgumentCount` and
//! `getParameterCount`. Both are syntactic properties of the parameter list: the
//! minimum is the index of the first parameter that is optional, has an
//! initialiser, or is a rest element, and the maximum is the list's length. So
//! the rule reads the declaration and counts, and — like the arity rule for type
//! arguments — reports on a syntactic fact with no incompleteness to leak.
//!
//! What it cannot do syntactically is decide **which** signature a call resolves
//! to, which is why the rule is confined to a callee naming a symbol with
//! exactly one function-like declaration. An overload set is
//! `crate::calls`' business and `docs/architecture/checker-notes-callres.md`'s
//! row.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The argument-count check for one call expression.
    pub(crate) fn check_call_arity(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::CallExpression(call)) = self.node_map.get(node) else { return };
        // A type-argument list means the callee is generic and the resolution
        // upstream performs is not the one this rule models.
        let Some(callee) = call.expression.and_then(|expression| expression.node_id()) else {
            return;
        };
        // `getSpreadArgumentIndex` (`checker.go:9706`) short-circuits to a
        // different message entirely; a spread also makes the count unknowable
        // without a tuple type.
        if call.arguments.iter().any(|argument| {
            argument.node_id().is_some_and(|id| self.nodes.kind(id) == SyntaxKind::SpreadElement)
        }) {
            return;
        }
        // A written type-argument list makes the call a *generic* one, and
        // `f < A, B > 7` parses as a call here and as a comparison chain
        // upstream (`grammarAmbiguities1`). Both are declined.
        if !call.type_arguments.is_empty() {
            return;
        }
        let arguments = call.arguments.len();
        let Some((minimum, maximum)) = self.sole_signature_arity(callee) else { return };
        let unbounded = maximum.is_none();
        if arguments >= minimum && maximum.is_none_or(|maximum| arguments <= maximum) {
            return;
        }
        let message = if unbounded {
            &messages::EXPECTED_AT_LEAST_0_ARGUMENTS_BUT_GOT_1
        } else {
            &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1
        };
        // The error node is **not the same for the two directions**
        // (`checker.go:9770` and `:9804`). Too few arguments reports on the call
        // — `getErrorNodeForCallNode` (`:9843`), the callee, or its `.name` when
        // the callee is a property access. Too many reports on the *first excess
        // argument*: `functionCall6.ts(4,12)` for `foo('foo', 'bar')` is the
        // `'bar'`, and reporting at the callee there was 16 of this rule's first
        // 24 wrong lines.
        let at = match maximum {
            Some(maximum) if arguments > maximum => call
                .arguments
                .get(maximum)
                .and_then(tsr_ast::Expression::node_id)
                .unwrap_or_else(|| self.call_error_node(callee)),
            _ => self.call_error_node(callee),
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.nodes.span(at);
        let range = match maximum {
            Some(maximum) if maximum > minimum => format!("{minimum}-{maximum}"),
            _ => minimum.to_string(),
        };
        self.report(file, Diagnostic::with_args(message, span, [range, arguments.to_string()]));
    }

    /// `getErrorNodeForCallNode` (`checker.go:9843`).
    fn call_error_node(&self, callee: NodeId) -> NodeId {
        match self.node_map.get(callee) {
            Some(Node::PropertyAccessExpression(access)) => {
                access.name.and_then(|name| name.node_id()).unwrap_or(callee)
            }
            _ => callee,
        }
    }

    /// `(getMinArgumentCount, getParameterCount)` where the callee names exactly
    /// one function-like declaration.
    ///
    /// `None` for every other shape, and each `None` is a decline with a reason:
    ///
    /// - **not a bare identifier** — a property access needs the receiver's type
    ///   to find the member, which is [`crate::members`]' road;
    /// - **more than one declaration** — an overload set, whose selection is
    ///   `resolveCall`'s and whose arity error upstream computes across *all*
    ///   candidates (`checker.go:9715`);
    /// - **not a function or method declaration** — a class is a construct
    ///   signature, a variable holds a function *type* whose parameter list is
    ///   not on the declaration.
    fn sole_signature_arity(&self, callee: NodeId) -> Option<(usize, Option<usize>)> {
        let Some(Node::Identifier(identifier)) = self.node_map.get(callee) else { return None };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            callee,
            identifier.text,
            SymbolFlags::VALUE,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::FUNCTION) || entry.declarations.len() != 1 {
            return None;
        }
        let Some(Node::FunctionDeclaration(declaration)) = self.node_map.get(entry.declarations[0])
        else {
            return None;
        };
        // An overload set of one — a declaration with no body — is still an
        // overload set as far as the corpus is concerned, and its implementation
        // may be in another file this rule has not looked at.
        declaration.body?;
        let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = declaration
            .parameters
            .iter()
            .copied()
            // `this` is not an argument (`getParameterCount` skips it).
            .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
            .collect();
        if parameters.iter().any(|parameter| parameter.dot_dot_dot_token.is_some()) {
            let minimum = parameters
                .iter()
                .position(|parameter| {
                    parameter.question_token.is_some()
                        || parameter.initializer.is_some()
                        || parameter.dot_dot_dot_token.is_some()
                })
                .unwrap_or(parameters.len());
            return Some((minimum, None));
        }
        let maximum = parameters.len();
        let mut minimum = parameters
            .iter()
            .position(|parameter| {
                parameter.question_token.is_some() || parameter.initializer.is_some()
            })
            .unwrap_or(maximum);
        // `getMinArgumentCountEx` (`relater.go:1737`) walks back from the
        // minimum and drops every trailing parameter whose type contains
        // `void` — a `void` parameter may be omitted. `callWithMissingVoid` is
        // the whole of what this costs, and it was 4 wrong lines.
        while minimum > 0 && self.parameter_annotation_is_void(parameters[minimum - 1]) {
            minimum -= 1;
        }
        Some((minimum, Some(maximum)))
    }

    /// Is this parameter's **written** annotation the `void` keyword?
    ///
    /// Upstream asks the computed type whether any constituent carries
    /// `TypeFlagsVoid`; this asks the annotation, which answers the same for
    /// `void` and for `T | void` and declines every alias and generic
    /// instantiation that would need the type. A decline here raises the
    /// minimum, so it costs a *wrong* diagnostic rather than a missing one —
    /// which is why the shapes it does not cover are named rather than assumed:
    /// an alias for `void`, and a type parameter instantiated with it.
    fn parameter_annotation_is_void(&self, parameter: &tsr_ast::ParameterDeclaration<'_>) -> bool {
        let Some(annotation) = parameter.r#type else { return false };
        let Some(id) = annotation.node_id() else { return false };
        match self.node_map.get(id) {
            Some(Node::KeywordTypeNode(keyword)) => keyword.kind == SyntaxKind::VoidKeyword,
            Some(Node::UnionTypeNode(union)) => union.types.iter().any(|member| {
                member.node_id().and_then(|id| self.node_map.get(id)).is_some_and(|node| {
                    matches!(node, Node::KeywordTypeNode(keyword)
                        if keyword.kind == SyntaxKind::VoidKeyword)
                })
            }),
            _ => false,
        }
    }

    /// Is this the `this` parameter — the one that is a type annotation wearing
    /// a parameter's syntax?
    fn is_this_parameter_declaration(parameter: &tsr_ast::ParameterDeclaration<'_>) -> bool {
        matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
    }
}
