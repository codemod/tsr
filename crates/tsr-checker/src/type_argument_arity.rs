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
        if self.file_has_parse_errors || self.in_js_file(node) {
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
        if maximum == 0 || (written >= minimum && written <= maximum) {
            return;
        }
        let message = if minimum < maximum {
            &messages::GENERIC_TYPE_0_REQUIRES_BETWEEN_1_AND_2_TYPE_ARGUMENTS
        } else {
            &messages::GENERIC_TYPE_0_REQUIRES_1_TYPE_ARGUMENT_S
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
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
