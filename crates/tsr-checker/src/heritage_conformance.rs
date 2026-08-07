//! TS2420 / TS2430 — a class or interface that does not satisfy what it says it
//! does.
//!
//! `checkClassDeclaration`'s `implements` loop and `checkInterfaceDeclaration`'s
//! `extends` loop (`checker.go`), both of which run
//! `checkTypeAssignableTo(typeWithThis, baseWithThis, node.Name())`.
//!
//! # Two rows that only became reachable once the relation could say "I cannot"
//!
//! Nothing here is new machinery. The declared type of a class or interface has
//! existed since `crate::declared`; the verdict is §25's `relate_ternary`. What
//! was missing until §25 was a relation that distinguishes *"does not hold"*
//! from *"could not tell"* — and a conformance check is the purest example of a
//! consumer that acts on a negative, because a class satisfying its interface is
//! the *normal* case and every undecidable pair would have been an error.
//!
//! The error node is the **name**: `declareClassInterfaceImplementation.ts(5,15)`
//! is the `Buffer` of `declare class Buffer implements IBuffer`.

use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{checker::Checker, relater::Relation, relater::Ternary, types::TypeId};

/// Which of the three (declaration, keyword) pairs is being checked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Heritage {
    /// A `class`, whose `extends` is TS2415 and whose `implements` is TS2420.
    Class,
    /// An `interface`, whose `extends` is TS2430.
    Interface,
}

impl Checker<'_, '_> {
    /// The conformance check for one class or interface declaration.
    pub(crate) fn check_heritage_conformance(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (name, clauses, kind) = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                // A generic declaration's members are written in terms of type
                // parameters this port does not instantiate — the same decline
                // `crate::member_completeness` draws, for the same reason.
                if !class.type_parameters.is_empty() {
                    return;
                }
                (class.name.and_then(|n| n.node_id), class.heritage_clauses, Heritage::Class)
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                if !interface.type_parameters.is_empty() {
                    return;
                }
                (
                    interface.name.and_then(|n| n.node_id),
                    interface.heritage_clauses,
                    Heritage::Interface,
                )
            }
            _ => return,
        };
        let Some(name) = name else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // A **merged** declaration — two `interface I` bodies, or an interface
        // merged with a class — assembles its member table from several
        // declarations, and upstream's merge is not this port's for private and
        // inherited members. `mergedInterfacesWithInheritedPrivates3`,
        // `implementingAnInterfaceExtendingClassWithPrivates2`,
        // `interfacePropertiesWithSameName3` and `interfaceDeclaration3` were 8
        // of the first measurement's 20 wrong lines and share exactly this.
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
            return;
        }
        let source = self.get_declared_type_of_class_or_interface(symbol);

        let mut targets: Vec<(TypeId, &'static tsr_diagnostics::Message)> = Vec::new();
        for clause in clauses {
            // Three (declaration, keyword) pairs, three codes. A class's
            // `extends` is TS2415, its `implements` TS2420, an interface's
            // `extends` TS2430 — `checkClassDeclaration` and
            // `checkInterfaceDeclaration` run the same
            // `checkTypeAssignableTo(typeWithThis, baseWithThis, node.Name())`
            // at all three and differ only in the message.
            let message = match (kind, clause.token.kind) {
                (Heritage::Class, tsr_ast::SyntaxKind::ExtendsKeyword) => {
                    &messages::CLASS_0_INCORRECTLY_EXTENDS_BASE_CLASS_1
                }
                (Heritage::Class, tsr_ast::SyntaxKind::ImplementsKeyword) => {
                    &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1
                }
                (Heritage::Interface, tsr_ast::SyntaxKind::ExtendsKeyword) => {
                    &messages::INTERFACE_0_INCORRECTLY_EXTENDS_INTERFACE_1
                }
                _ => continue,
            };
            for entry in clause.types {
                // A base with type arguments needs instantiation
                // (`members::base_symbol_of_heritage_entry`'s first decline);
                // a base that is not a plain identifier needs
                // `resolveEntityName` or a construct signature.
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
                let base_entry = self.binder.symbols().get(base);
                if !base_entry.flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                    // The merged-declaration decline applies to the **base** as
                    // well as to the source, and for the same reason: a target
                    // whose members come from several declarations is a table
                    // this port assembles differently from upstream.
                    || base_entry.declarations.len() > 1
                {
                    return;
                }
                targets.push((self.get_declared_type_of_class_or_interface(base), message));
            }
        }

        for (target, message) in targets {
            if !self.pair_is_reportable(source, target) {
                continue;
            }
            if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(name) else { return };
            let span = self.nodes.span(name);
            let source_text = self.type_to_string(source);
            let target_text = self.type_to_string(target);
            self.report(file, Diagnostic::with_args(message, span, [source_text, target_text]));
            // Upstream reports once per failing heritage entry; this reports the
            // first and returns, because a second report at the same position
            // would be a duplicate under the multiset comparison for every case
            // whose two bases fail for the same reason.
            return;
        }
    }

    /// TS2416 — `Property '{0}' in type '{1}' is not assignable to the same
    /// property in base type '{2}'.`
    ///
    /// `checkKindsOfPropertyMemberOverrides` (`checker.go`), which compares each
    /// **own** member of a derived declaration against the base's member of the
    /// same name. Error node the member's own name:
    /// `baseClassImprovedMismatchErrors.ts(8,5)` is the `n` of `n: string |
    /// Derived`.
    ///
    /// Reported per member, and therefore *beside* TS2415 rather than instead of
    /// it — the baseline above records both codes for the same class, at
    /// different positions.
    pub(crate) fn check_property_overrides(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(node) else { return };
        if !class.type_parameters.is_empty() {
            return;
        }
        let Some(base) = self.sole_plain_base_type(class.heritage_clauses) else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        if self.binder.symbols().get(symbol).declarations.len() > 1 {
            return;
        }
        let derived = self.get_declared_type_of_class_or_interface(symbol);

        let members: Vec<(String, NodeId)> = class
            .members
            .iter()
            .filter_map(|member| {
                let name = match member {
                    tsr_ast::ClassElement::PropertyDeclaration(property) => property.name,
                    tsr_ast::ClassElement::MethodDeclaration(method) => method.name,
                    _ => return None,
                };
                let id = name.node_id()?;
                match self.node_map.get(id) {
                    Some(Node::Identifier(identifier)) => Some((identifier.text.to_string(), id)),
                    _ => None,
                }
            })
            .collect();

        for (name, at) in members {
            let (Some(derived_type), Some(base_type)) = (
                self.get_type_of_property_of_type(derived, &name),
                self.get_type_of_property_of_type(base, &name),
            ) else {
                continue;
            };
            if !self.pair_is_reportable(derived_type, base_type) {
                continue;
            }
            if self.relate_ternary(derived_type, base_type, Relation::Assignable)
                != Ternary::NotRelated
            {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.nodes.span(at);
            let derived_text = self.type_to_string(derived);
            let base_text = self.type_to_string(base);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_IN_TYPE_1_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE_2,
                    span,
                    [name, derived_text, base_text],
                ),
            );
        }
    }

    /// The declared type of a class's single, plain, non-generic base class.
    fn sole_plain_base_type(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) -> Option<TypeId> {
        let mut found = None;
        for clause in clauses {
            if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                continue;
            }
            for entry in clause.types {
                if found.is_some() || !entry.type_arguments.is_empty() {
                    return None;
                }
                let tsr_ast::Expression::Identifier(written) = entry.expression? else {
                    return None;
                };
                let base = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    written.node_id?,
                    written.text,
                    SymbolFlags::TYPE,
                )?;
                let base = self.binder.merged_symbol(base);
                let entry = self.binder.symbols().get(base);
                if !entry.flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
                    return None;
                }
                found = Some(self.get_declared_type_of_class_or_interface(base));
            }
        }
        found
    }
}
