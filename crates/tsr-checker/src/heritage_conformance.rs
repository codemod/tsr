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
}
