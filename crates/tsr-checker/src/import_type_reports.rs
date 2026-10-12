//! TS1340 / TS2694 — the two reports of `getTypeFromImportTypeNode`
//! (`checker.go:24575`) once its module resolves.
//!
//! - **Unqualified, not `typeof`.** `import("./m")` written as a type needs the
//!   module (after `resolveExternalModuleSymbol`, so `export = x` is `x`) to
//!   carry type meaning; otherwise TS1340 *Module '{0}' does not refer to a
//!   type, but is used as a type here*, at the import type node.
//! - **Qualified, not `typeof`.** Each qualifier segment is looked up in the
//!   current namespace's exports (`getSymbol(getExportsOfSymbol(ns), name,
//!   meaning)`, namespace meaning except the last, which wants type meaning);
//!   a miss is TS2694 *Namespace '{0}' has no exported member '{1}'*, at the
//!   segment, naming the namespace by `getFullyQualifiedName`.
//!
//! The typing road ([`Checker::unqualified_import_type_meaning`] and
//! `declared.rs`'s qualified arm) already declines to `error` on both
//! shapes; this module is the diagnostic native issues on the way there.
//! `typeof import(…)` (value meaning, `getPropertyOfType` over the
//! namespace's type) is not ported. r7-calls §47.

use tsr_ast::{EntityName, Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `getTypeFromImportTypeNode`'s TS1340 and TS2694 for one import type
    /// node whose argument is a string literal.
    pub(crate) fn check_import_type_resolution(&mut self, node: NodeId) {
        let Some(Node::ImportTypeNode(import)) = self.node_map.get(node) else { return };
        if import.is_type_of {
            return;
        }
        let qualifier = import.qualifier;
        let Some(argument) = import.argument.and_then(|a| a.node_id()) else { return };
        let Some(Node::LiteralTypeNode(literal)) = self.node_map.get(argument) else { return };
        let Some(specifier) = literal.literal.and_then(|l| l.node_id()) else { return };
        let Some(Node::StringLiteral(text)) = self.node_map.get(specifier) else { return };
        let module_text = text.text.to_string();
        // A module that does not resolve has already been reported (TS2307).
        let Some(inner) = self.resolve_external_module_name(node, specifier) else { return };
        // `resolveExternalModuleSymbol(inner, false)`: `export=`, resolved.
        let immediate = self.resolve_external_module_symbol(inner);
        let module = self.resolve_symbol(immediate);
        let Some(qualifier) = qualifier else {
            if !self.import_type_symbol_is_known(module) {
                return;
            }
            if self.get_symbol_flags(module).intersects(SymbolFlags::TYPE) {
                return;
            }
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let span = self.error_span(node);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::MODULE_0_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE_DID_YOU_MEAN_TYPEOF_IMPORT_0,
                    span,
                    [module_text],
                ),
            );
            return;
        };
        let chain = identifier_chain(qualifier);
        let last = chain.len().saturating_sub(1);
        let mut current = module;
        for (index, (name, name_id)) in chain.into_iter().enumerate() {
            let meaning = if index == last { SymbolFlags::TYPE } else { SymbolFlags::NAMESPACE };
            let namespace = self.binder.merged_symbol(self.resolve_symbol(current));
            let exports_known = self.import_type_exports_are_complete(namespace);
            let mut found = self.import_type_symbol_in_exports(namespace, name, meaning);
            // A CommonJS module might have typedefs exported alongside an
            // `export=` (`checker.go:24613`): look in the file module's own
            // exports when the immediate `export=` is a `module.exports`
            // assignment.
            if found.is_none() && immediate != inner {
                let assigned = self.binder.symbols().get(immediate).declarations.iter().any(
                    |&declaration| {
                        matches!(self.node_map.get(declaration),
                            Some(Node::BinaryExpression(binary))
                                if binary.left.is_some_and(crate::assignment_declarations::is_module_exports_access))
                    },
                );
                if assigned && let Some(parent) = self.binder.symbols().get(immediate).parent {
                    found = self.import_type_symbol_in_exports(parent, name, meaning);
                }
            }
            match found {
                Some(Found::Symbol(next)) => current = next,
                // An alias whose target this port cannot resolve proves
                // nothing either way.
                Some(Found::Undecided) => return,
                None => {
                    if !exports_known
                        || (meaning == SymbolFlags::NAMESPACE
                            && self.miss_is_on_a_jsdoc_alias(namespace, name))
                    {
                        return;
                    }
                    let printed = self.fully_qualified_name(current);
                    let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
                    let span = self.nodes.span(name_id);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::NAMESPACE_0_HAS_NO_EXPORTED_MEMBER_1,
                            span,
                            [printed, name.to_string()],
                        ),
                    );
                    return;
                }
            }
        }
    }

    /// A module symbol this port bound from a body: a source file, or a
    /// `declare module "m" { … }` / namespace with a block. A shorthand
    /// ambient module (`declare module "m";`) has no table to miss in.
    fn import_type_symbol_is_known(&self, symbol: SymbolId) -> bool {
        let record = self.binder.symbols().get(symbol);
        if !record.flags.intersects(SymbolFlags::MODULE) {
            return true;
        }
        record.declarations.iter().any(|&declaration| match self.node_map.get(declaration) {
            Some(Node::SourceFile(_)) => true,
            Some(Node::ModuleDeclaration(module)) => module.body.is_some(),
            _ => false,
        })
    }

    /// A namespace-meaning miss on a name a JSDoc alias declares. Native
    /// parses `@typedef {T} A.B` with a dotted name and reparses it into a
    /// namespace `A` (`parseJSDocTypeNameWithNamespace`, `reparseUnhosted`);
    /// this parser keeps only `A` and binds it as a type alias, so the miss
    /// proves nothing (`jsDeclarationsImportNamespacedType`).
    fn miss_is_on_a_jsdoc_alias(&self, namespace: SymbolId, name: &str) -> bool {
        let Some(&entry) = self.binder.symbols().get(namespace).exports.get(name) else {
            return false;
        };
        self.binder.symbols().get(entry).declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::JSDocTypedefTag | SyntaxKind::JSDocCallbackTag
            )
        })
    }

    /// Whether a miss in `getExportsOfSymbol(namespace)` is a native miss.
    /// Only a module or namespace bound from a body vouches for its table:
    /// a class or function is a late-binding container whose computed
    /// statics resolve late (`getResolvedMembersOrExportsOfSymbol`), and a
    /// JS variable's expando table is not this lookup's to certify.
    fn import_type_exports_are_complete(&self, namespace: SymbolId) -> bool {
        let flags = self.binder.symbols().get(namespace).flags;
        flags.intersects(SymbolFlags::MODULE)
            && !flags.intersects(SymbolFlags::CLASS | SymbolFlags::FUNCTION)
            && self.import_type_symbol_is_known(namespace)
    }

    /// `getSymbol(getExportsOfSymbol(namespace), name, meaning)`
    /// (`checker.go`): the entry when its own flags carry `meaning`, or an
    /// alias whose target's do. A module's table includes its `export *`
    /// re-exports (`getExportsOfModule`).
    fn import_type_symbol_in_exports(
        &mut self,
        namespace: SymbolId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<Found> {
        let entry = if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::MODULE) {
            self.get_export_of_module(namespace, name)
        } else {
            self.binder.symbols().get(namespace).exports.get(name).copied()
        }?;
        let entry = self.binder.merged_symbol(entry);
        let flags = self.binder.symbols().get(entry).flags;
        if flags.intersects(meaning) {
            return Some(Found::Symbol(entry));
        }
        if !flags.intersects(SymbolFlags::ALIAS) {
            return None;
        }
        if self.resolve_alias(entry).is_none() {
            return Some(Found::Undecided);
        }
        self.get_symbol_flags(entry).intersects(meaning).then_some(Found::Symbol(entry))
    }
}

/// A qualifier lookup's answer.
enum Found {
    Symbol(SymbolId),
    Undecided,
}

/// `getIdentifierChain` (`checker.go`): the qualifier's identifiers, leftmost
/// first.
fn identifier_chain(name: EntityName<'_>) -> Vec<(&str, NodeId)> {
    let mut chain = Vec::new();
    let mut current = name;
    loop {
        match current {
            EntityName::Identifier(identifier) => {
                if let Some(id) = identifier.node_id {
                    chain.push((identifier.text, id));
                }
                break;
            }
            EntityName::QualifiedName(qualified) => {
                if let Some(right) = qualified.right
                    && let Some(id) = right.node_id
                {
                    chain.push((right.text, id));
                }
                let Some(left) = qualified.left else { break };
                current = left;
            }
        }
    }
    chain.reverse();
    chain
}
