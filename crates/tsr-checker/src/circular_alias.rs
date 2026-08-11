//! TS2303 — `Circular definition of import alias '{0}'.`
//!
//! `resolveAlias` (`checker.go:16272`-`:16291`). Upstream detects the cycle
//! *generically*, with `pushTypeResolution`/`popTypeResolution` around the alias
//! target; that resolution stack is not in this port, and building it to serve
//! one diagnostic would be the tail wagging the compiler.
//!
//! The two shapes the corpus asks for that are decidable **syntactically** are
//! ported instead:
//!
//! ```text
//! namespace M { import A = B; import B = A; }          a chain in one scope
//! declare module "m" { import self = require("m"); }   a self-reference
//! ```
//!
//! The third — `export type { A } from './b'` with `b.ts` re-exporting from
//! `a.ts` — needs the module graph and export-star resolution and is declined.
//! Owner: the module graph, via `checker_types`.
//!
//! `docs/architecture/checker-notes-diag2.md` §955.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// One `import X = …` declaration.
    pub(crate) fn check_circular_import_alias(&mut self, node: NodeId) {
        let Some(Node::ImportEqualsDeclaration(declaration)) = self.node_map.get(node) else {
            return;
        };
        let Some(name) = declaration.name.and_then(|n| n.node_id) else { return };
        let text = self.identifier_text(name).map(str::to_string);
        let Some(text) = text else { return };
        let circular = match declaration.module_reference {
            // `import self = require("m")` **inside `declare module "m"`**. The
            // same text in a real file is a different question — the module
            // graph's — and is not decided here.
            Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                self.self_referencing_ambient_require(node, reference)
            }
            // `import A = B`, following the chain of entity-name aliases in
            // scope until it terminates or returns to this declaration.
            Some(tsr_ast::ModuleReference::Identifier(_)) => {
                self.alias_chain_returns_to(node, declaration.module_reference)
            }
            _ => false,
        };
        if !circular {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(&messages::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS_0, span, [text]),
        );
    }

    /// `import self = require("m")` where the enclosing `declare module` names
    /// `"m"` — the module resolves to the declaration containing it.
    fn self_referencing_ambient_require(
        &self,
        node: NodeId,
        reference: &tsr_ast::ExternalModuleReference<'_>,
    ) -> bool {
        let Some(specifier) = reference.expression.and_then(|e| e.node_id()) else { return false };
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return false };
        self.nodes.ancestors(node).any(|ancestor| {
            matches!(
                self.node_map.get(ancestor),
                Some(Node::ModuleDeclaration(module))
                    if module.name.and_then(|n| n.node_id())
                        .and_then(|id| self.node_map.get(id))
                        .is_some_and(|n| matches!(n, Node::StringLiteral(own) if own.text == literal.text))
            )
        })
    }

    /// Follow `import A = B; import B = A;` from `start` and report whether the
    /// chain comes back to it.
    ///
    /// **Terminating on resolution failure matters as much as on the cycle.**
    /// `import A = B; import B = C` with a real `C` must walk off the end
    /// rather than loop, which is falsifier 1.
    fn alias_chain_returns_to(
        &mut self,
        start: NodeId,
        reference: Option<tsr_ast::ModuleReference<'_>>,
    ) -> bool {
        let mut current = reference.and_then(|r| r.node_id());
        let mut seen = 0usize;
        while let Some(at) = current {
            // A chain longer than the file has declarations is a cycle that
            // does not pass through `start`, and is not this declaration's.
            seen += 1;
            if seen > 64 {
                return false;
            }
            let Some(text) = self.identifier_text(at).map(str::to_string) else { return false };
            let Some(symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                at,
                &text,
                SymbolFlags::NAMESPACE_MODULE | SymbolFlags::TYPE | SymbolFlags::VALUE,
            ) else {
                return false;
            };
            let declarations =
                self.binder.symbols().get(self.binder.merged_symbol(symbol)).declarations.clone();
            let [declaration] = declarations.as_slice() else { return false };
            if *declaration == start {
                return true;
            }
            let Some(Node::ImportEqualsDeclaration(next)) = self.node_map.get(*declaration) else {
                return false;
            };
            current = match next.module_reference {
                Some(tsr_ast::ModuleReference::Identifier(identifier)) => identifier.node_id,
                _ => return false,
            };
        }
        false
    }
}

/// Keeps `SyntaxKind` in the import list for the ambient-module walk above.
const _: SyntaxKind = SyntaxKind::ModuleDeclaration;
