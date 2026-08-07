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

impl Checker<'_, '_> {
    /// The conformance check for one class or interface declaration.
    pub(crate) fn check_heritage_conformance(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (name, clauses, implements) = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => {
                // A generic declaration's members are written in terms of type
                // parameters this port does not instantiate — the same decline
                // `crate::member_completeness` draws, for the same reason.
                if !class.type_parameters.is_empty() {
                    return;
                }
                (class.name.and_then(|n| n.node_id), class.heritage_clauses, true)
            }
            Some(Node::InterfaceDeclaration(interface)) => {
                if !interface.type_parameters.is_empty() {
                    return;
                }
                (interface.name.and_then(|n| n.node_id), interface.heritage_clauses, false)
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

        let wanted = if implements {
            tsr_ast::SyntaxKind::ImplementsKeyword
        } else {
            tsr_ast::SyntaxKind::ExtendsKeyword
        };
        let mut targets: Vec<TypeId> = Vec::new();
        for clause in clauses {
            if clause.token.kind != wanted {
                continue;
            }
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
                if !self
                    .binder
                    .symbols()
                    .get(base)
                    .flags
                    .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                {
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
            let message = if implements {
                &messages::CLASS_0_INCORRECTLY_IMPLEMENTS_INTERFACE_1
            } else {
                &messages::INTERFACE_0_INCORRECTLY_EXTENDS_INTERFACE_1
            };
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
