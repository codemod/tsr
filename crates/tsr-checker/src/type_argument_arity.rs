//! TS2314 / TS2707 — a generic type reference written with the wrong number of
//! type arguments.
//!
//! `getTypeFromClassOrInterfaceReference` (`checker.go:23169`), the arity arm.
//!
//! # A type rule this port can answer without computing a type
//!
//! Upstream asks the *declared type* for its local type parameters. Everything
//! that decision needs, though, is on the **declaration**: a class or interface
//! writes its type parameters syntactically, and a parameter with a default is
//! exactly the one that may be omitted (`getMinTypeArgumentCount`). So the rule
//! reads the declaration list and counts, which puts it in the same family as
//! the grammar rules in [`crate::check`] rather than in
//! [`crate::assignreport`]'s: it reports on a syntactic fact and has no
//! incompleteness to leak.
//!
//! The one thing it must get right is *which* declaration, and that is a name
//! resolution — the same one [`crate::check::Checker::check_value_identifier`]
//! makes, asked in type space.
//!
//! # What it declines
//!
//! | decline | why |
//! |---|---|
//! | a `.js` file | upstream substitutes `Expected_0_type_arguments_provide_these_with_an_extends_tag` when a JSDoc `@augments` tag is missing (`checker.go:23181`), and with `noImplicitAny` off it reports nothing at all |
//! | a type **alias** | its arity error is TS2315 / TS2558 from a different function |
//! | a name that resolves to more than one kind of declaration | a class merged with an interface or a namespace has type parameters upstream reads off the merged symbol, and this port's merge is not upstream's |
//! | a name that does not resolve | TS2304 / TS2552 territory, already a row |

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The arity check for one written type reference.
    pub(crate) fn check_type_argument_arity(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let (name, written) = match self.node_map.get(node) {
            Some(Node::TypeReferenceNode(reference)) => (
                reference.type_name.and_then(|name| name.node_id()),
                reference.type_arguments.len(),
            ),
            Some(Node::ExpressionWithTypeArguments(reference)) => (
                reference.expression.and_then(|expression| expression.node_id()),
                reference.type_arguments.len(),
            ),
            _ => return,
        };
        let Some(name) = name else { return };
        // An `ExpressionWithTypeArguments` is also the syntax of an
        // *instantiation expression* (`f<number>`), which is a value position
        // and has nothing to do with this rule. Only a heritage clause's is a
        // type reference.
        if self.nodes.kind(node) == SyntaxKind::ExpressionWithTypeArguments
            && self
                .nodes
                .parent(node)
                .is_none_or(|parent| self.nodes.kind(parent) != SyntaxKind::HeritageClause)
        {
            return;
        }
        let Some((minimum, maximum)) = self.declared_type_parameter_arity(name) else { return };
        // `maximum == 0` is an **answer**, not a failure to compute one: the
        // class or interface resolved and declares no type parameters. Upstream
        // does not treat it as an arity mismatch at all — `checkNoTypeArguments`
        // (`checker.go:23220`) issues a different diagnostic, on the same error
        // node this rule already uses. §345.
        if maximum == 0 {
            if written == 0 {
                return;
            }
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let span = self.error_span(node);
            let printed = self.written_type_name(name);
            self.report(
                file,
                Diagnostic::with_args(&messages::TYPE_0_IS_NOT_GENERIC, span, [printed]),
            );
            return;
        }
        if written >= minimum && written <= maximum {
            return;
        }
        let message = if minimum < maximum {
            &messages::GENERIC_TYPE_0_REQUIRES_BETWEEN_1_AND_2_TYPE_ARGUMENTS
        } else {
            &messages::GENERIC_TYPE_0_REQUIRES_1_TYPE_ARGUMENT_S
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let printed = self.written_type_name(name);
        self.report(
            file,
            Diagnostic::with_args(
                message,
                span,
                [printed, minimum.to_string(), maximum.to_string()],
            ),
        );
    }

    /// The same arity question for a **call**: `f<number>()`, `new D<number>()`.
    ///
    /// `getTypeArgumentArityError` (`checker.go:9852`). Written type arguments
    /// make both value-arity arms decline (`if !call.type_arguments.is_empty()`)
    /// and nothing took over, so this position had no owner at all. §347.
    ///
    /// The error node is upstream's `loc`: the type-argument **list**, whose
    /// `Pos` is just past the `<`, so after `SkipTrivia` the span starts at the
    /// first type argument rather than at the callee.
    ///
    /// Confined to upstream's own `len(signatures) == 1` branch — one
    /// declaration, a function or a class. The overload arm needs a signature
    /// set this port does not build and carries a different message.
    pub(crate) fn check_call_type_argument_arity(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (callee, type_arguments) = match self.node_map.get(node) {
            Some(Node::CallExpression(call)) => {
                (call.expression.and_then(|e| e.node_id()), call.type_arguments)
            }
            Some(Node::NewExpression(call)) => {
                (call.expression.and_then(|e| e.node_id()), call.type_arguments)
            }
            _ => return,
        };
        let (Some(callee), [first, ..]) = (callee, type_arguments) else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(callee) else { return };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            callee,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `len(signatures) == 1`. An overload set is upstream's other branch.
        let [declaration] = entry.declarations.as_slice() else { return };
        let parameters = match self.node_map.get(*declaration) {
            Some(Node::FunctionDeclaration(function)) => function.type_parameters,
            Some(Node::ClassDeclaration(class)) => class.type_parameters,
            _ => return,
        };
        // A generic constructor of its own would supply the signature's type
        // parameters instead of the class's, so a class is only asked when its
        // constructors declare none.
        if let Some(Node::ClassDeclaration(class)) = self.node_map.get(*declaration)
            && class.members.iter().any(|member| {
                matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(constructor)
                    if !constructor.type_parameters.is_empty())
            })
        {
            return;
        }
        let maximum = parameters.len();
        if maximum == 0 {
            return;
        }
        let minimum = parameters
            .iter()
            .position(|parameter| parameter.default_type.is_some())
            .unwrap_or(maximum);
        let written = type_arguments.len();
        if written >= minimum && written <= maximum {
            return;
        }
        let Some(argument) = first.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(argument);
        let expected =
            if minimum < maximum { format!("{minimum}-{maximum}") } else { minimum.to_string() };
        self.report(
            file,
            Diagnostic::with_args(
                &messages::EXPECTED_0_TYPE_ARGUMENTS_BUT_GOT_1,
                span,
                [expected, written.to_string()],
            ),
        );
    }

    /// `(getMinTypeArgumentCount(typeParameters), len(typeParameters))` for the
    /// class or interface an entity name refers to.
    ///
    /// `getMinTypeArgumentCount` (`checker.go`) counts the parameters up to the
    /// first one carrying a default: a parameter with a default may be omitted,
    /// and so may every parameter after it.
    fn declared_type_parameter_arity(&self, name: NodeId) -> Option<(usize, usize)> {
        // Only a simple name is asked. A qualified `N.C` needs the namespace
        // resolved first, which is `resolve_entity_name`'s job and a different
        // failure surface.
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return None };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name,
            identifier.text,
            SymbolFlags::TYPE,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if entry.flags.intersects(SymbolFlags::TYPE_ALIAS | SymbolFlags::TYPE_PARAMETER) {
            return None;
        }
        if !entry.flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            return None;
        }
        let mut arity: Option<(usize, usize)> = None;
        for declaration in &entry.declarations {
            let parameters = match self.node_map.get(*declaration) {
                Some(Node::ClassDeclaration(class)) => class.type_parameters,
                Some(Node::ClassExpression(class)) => class.type_parameters,
                Some(Node::InterfaceDeclaration(interface)) => interface.type_parameters,
                // A merged declaration of another kind — a namespace, an enum,
                // a variable — means the symbol's type parameters are not read
                // off any one of these lists.
                _ => return None,
            };
            let maximum = parameters.len();
            let minimum = parameters
                .iter()
                .position(|parameter| parameter.default_type.is_some())
                .unwrap_or(maximum);
            match arity {
                // Every declaration of one interface must repeat the same type
                // parameter list; if this port sees two that disagree, it is
                // seeing something upstream would have merged differently.
                Some(seen) if seen != (minimum, maximum) => return None,
                _ => arity = Some((minimum, maximum)),
            }
        }
        arity
    }

    /// The written text of a type reference's name, for the message argument.
    ///
    /// Named apart from `declared::entity_name_text`, which answers a different
    /// question (the printed *chain*) and lives on the same impl.
    fn written_type_name(&self, name: NodeId) -> String {
        match self.node_map.get(name) {
            Some(Node::Identifier(identifier)) => identifier.text.to_string(),
            _ => String::new(),
        }
    }
}
