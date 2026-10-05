//! TS2415 / TS2420 / TS2720 / TS2430 / TS2416 — a class or interface that does
//! not satisfy what it says it does.
//!
//! `checkClassLikeDeclaration`'s `extends` and `implements` arms and
//! `checkInterfaceDeclaration`'s `extends` loop (`checker.go`). A class runs
//! `checkTypeAssignableTo(typeWithThis, baseWithThis)` silently and, on
//! failure, `issueMemberSpecificError`: each own instance member whose type is
//! not assignable to the base's same-named member is TS2416 at the member's
//! name, and only when none is the broad class diagnostic reported at the
//! class name. An interface reports TS2430 at its name for each failing base.
//!
//! The verdict is §25's `relate_ternary`; a pair the relation cannot decide is
//! not reported, and neither is a broad diagnostic whose member walk met an
//! undecidable member, because upstream would have reported that member.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, Message, messages};

use crate::{
    check::{has_modifier, modifiers_of},
    checker::Checker,
    relater::Relation,
    relater::Ternary,
    types::TypeId,
};

impl Checker<'_, '_> {
    /// The heritage conformance checks for one class or interface declaration.
    pub(crate) fn check_heritage_conformance(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        match self.node_map.get(node) {
            Some(Node::ClassDeclaration(_) | Node::ClassExpression(_)) => {
                self.check_class_heritage_conformance(node);
            }
            Some(Node::InterfaceDeclaration(_)) => self.check_interface_heritage_conformance(node),
            _ => {}
        }
    }

    /// `checkClassLikeDeclaration`'s base-class and implemented-type arms
    /// (`checker.go:4293`).
    fn check_class_heritage_conformance(&mut self, node: NodeId) {
        let clauses = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return,
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // A **merged** declaration assembles its member table from several
        // declarations, and upstream's merge is not this port's for private
        // and inherited members (`mergedInterfacesWithInheritedPrivates3`).
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        // The merged-declaration decline applies to the **base** as well:
        // `class B extends Uint8Array` names a lib symbol merged across files.
        let base_is_single = clauses
            .iter()
            .filter(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            .flat_map(|clause| clause.types.iter())
            .next()
            .and_then(|entry| self.base_symbol_of_heritage_entry(entry, false))
            .is_some_and(|base| self.has_single_type_declaration(base));
        if base_is_single && let Some(base) = self.first_base_type_of_class_symbol(symbol) {
            self.check_class_heritage_entry(
                node,
                source,
                base,
                &messages::CLASS_0_INCORRECTLY_EXTENDS_BASE_CLASS_1,
            );
        }
        for clause in clauses {
            if clause.token.kind != SyntaxKind::ImplementsKeyword {
                continue;
            }
            for entry in clause.types {
                let Some(expression) = entry.expression else { continue };
                let Some(implemented) = self.heritage_entity_symbol(expression, SymbolFlags::TYPE)
                else {
                    continue;
                };
                let flags = self.binder.symbols().get(implemented).flags;
                if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                    || !self.has_single_type_declaration(implemented)
                {
                    continue;
                }
                let Some(target) = self.instantiated_heritage_base(
                    implemented,
                    entry.type_arguments,
                    entry.node_id,
                ) else {
                    continue;
                };
                // `t.symbol.Flags&ast.SymbolFlagsClass` picks the message.
                let message = if flags.contains(SymbolFlags::CLASS) {
                    &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_CLASS_1_DID_YOU_MEAN_TO_EXTEND_1_AND_INHERIT_ITS_MEMBERS_AS_A_SUBCLASS
                } else {
                    &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1
                };
                self.check_class_heritage_entry(node, source, target, message);
            }
        }
    }

    /// `if !c.checkTypeAssignableTo(typeWithThis, baseWithThis, nil, nil) {
    /// c.issueMemberSpecificError(…) }` for one heritage entry.
    fn check_class_heritage_entry(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        broad: &'static Message,
    ) {
        if self.is_error(target) || !self.pair_is_reportable(source, target) {
            return;
        }
        if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated {
            return;
        }
        self.issue_member_specific_error(node, source, target, broad);
    }

    /// `issueMemberSpecificError` (`checker.go`): TS2416 on every own
    /// non-static member whose type is not assignable to the base member of
    /// the same name; the broad diagnostic at the class name when none is.
    fn issue_member_specific_error(
        &mut self,
        node: NodeId,
        source: TypeId,
        target: TypeId,
        broad: &'static Message,
    ) {
        let members: Vec<NodeId> = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
            }
            Some(Node::ClassExpression(class)) => {
                class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
            }
            _ => return,
        };
        let mut issued = false;
        let mut undecided = false;
        for member in members {
            let is_static = self
                .node_map
                .get(member)
                .and_then(modifiers_of)
                .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::StaticKeyword));
            if is_static {
                continue;
            }
            // `declaredProp.Name != ast.InternalSymbolNameComputed`: a member
            // without a name, or with a computed one, is skipped.
            let Some(name_node) = self.declaration_name_of(member) else { continue };
            let name = match self.node_map.get(name_node) {
                Some(Node::Identifier(name)) => name.text.to_string(),
                Some(Node::StringLiteral(name)) => name.text.to_string(),
                Some(Node::NumericLiteral(name)) => name.text.to_string(),
                Some(Node::PrivateIdentifier(name)) => name.text.to_string(),
                _ => continue,
            };
            if self.binder.symbol_of(member).is_none() {
                continue;
            }
            let (Some(property), Some(base_property)) = (
                self.get_type_of_property_of_type(source, &name),
                self.get_type_of_property_of_type(target, &name),
            ) else {
                continue;
            };
            match self.relate_ternary(property, base_property, Relation::Assignable) {
                Ternary::Related => continue,
                Ternary::NotRelated if self.pair_is_reportable(property, base_property) => {}
                _ => {
                    undecided = true;
                    continue;
                }
            }
            issued = true;
            let Some(file) = self.source_file_of_for_diagnostics(name_node) else { continue };
            let span = self.error_span(name_node);
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_IN_TYPE_1_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE_2,
                    span,
                    [name, source_text, target_text],
                ),
            );
        }
        if issued || undecided {
            return;
        }
        // `core.OrElse(node.Name(), node)`.
        let at = self.declaration_name_of(node).unwrap_or(node);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let source_text = self.type_to_string(source);
        let target_text = self.type_to_string(target);
        self.report(file, Diagnostic::with_args(broad, span, [source_text, target_text]));
    }

    /// `checkInterfaceDeclaration`'s base loop (`checker.go:4991`):
    /// `checkTypeAssignableTo(typeWithThis, baseWithThis, node.Name(),
    /// Interface_0_incorrectly_extends_interface_1)` for each base type.
    fn check_interface_heritage_conformance(&mut self, node: NodeId) {
        let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(node) else { return };
        let Some(name) = interface.name.and_then(|n| n.node_id) else { return };
        if !interface.type_parameters.is_empty() {
            return;
        }
        let clauses = interface.heritage_clauses;
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);
        let mut targets: Vec<TypeId> = Vec::new();
        for clause in clauses {
            if clause.token.kind != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for entry in clause.types {
                // A base with type arguments needs instantiation; a base that
                // is not a plain identifier needs `resolveEntityName`.
                if !entry.type_arguments.is_empty() {
                    return;
                }
                let Some(tsr_ast::Expression::Identifier(written)) = entry.expression else {
                    return;
                };
                let Some(written_id) = written.node_id else { return };
                let Some(base) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    written_id,
                    written.text,
                    SymbolFlags::TYPE,
                ) else {
                    return;
                };
                let base = self.binder.merged_symbol(base);
                if !self.is_single_class_or_interface(base) {
                    return;
                }
                targets.push(self.get_declared_type_of_class_or_interface(base));
            }
        }
        for target in targets {
            if !self.pair_is_reportable(source, target) {
                continue;
            }
            if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
            let span = self.error_span(name);
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::INTERFACE_0_INCORRECTLY_EXTENDS_INTERFACE_1,
                    span,
                    [source_text, target_text],
                ),
            );
        }
    }

    /// One class or interface declaration among the symbol's declarations; a
    /// merged `var` (`declare var A: { new(): A }`) contributes no members.
    fn has_single_type_declaration(&self, symbol: SymbolId) -> bool {
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter(|&&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ClassDeclaration
                        | SyntaxKind::ClassExpression
                        | SyntaxKind::InterfaceDeclaration
                )
            })
            .count()
            == 1
    }

    /// A class or interface with one declaration: a target whose members come
    /// from several declarations is a table this port assembles differently
    /// from upstream.
    fn is_single_class_or_interface(&self, symbol: SymbolId) -> bool {
        let entry = self.binder.symbols().get(symbol);
        entry.flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            && entry.declarations.len() == 1
    }
}
