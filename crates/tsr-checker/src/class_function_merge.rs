//! TS2813 / TS2814 — a function symbol merged with a non-ambient class.
//!
//! `checkFunctionOrConstructorSymbolWorker`'s class-merge arm
//! (`checker.go:3660`-`:3682`): when a symbol with `SymbolFlagsFunction` has a
//! class declaration outside an ambient context, every class declaration
//! reports `Class declaration cannot implement overload list for '{0}'` and
//! every function declaration `Function with bodies can only merge with
//! classes that are ambient`, each at its name.
//!
//! `check.rs`'s `check_function_or_constructor_symbol` ports the
//! implementation-expected arms of the same worker and declines class-merged
//! symbols; this is the arm it declines. Upstream runs the worker once per
//! symbol (`links.functionOrConstructorChecked`, `checker.go:3463`) from both
//! `checkFunctionOrMethodDeclaration` and `checkClassLikeDeclaration`; the
//! arm's output does not depend on which declaration triggered it, so this
//! port runs it from the symbol's first class or function declaration and
//! keeps no side table. The only traversal is the merged symbol's
//! declaration list and each declaration's ancestor chain for `declare`.

use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::check::modifiers_of;
use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The class-merge arm for the symbol of one class or function
    /// declaration.
    pub(crate) fn check_class_function_merge(&mut self, node: NodeId) {
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.contains(SymbolFlags::FUNCTION) {
            return;
        }
        let declarations = entry.declarations.clone();
        let name = entry.name.to_string();
        let first = declarations.iter().copied().find(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ClassDeclaration | SyntaxKind::FunctionDeclaration
            )
        });
        if first != Some(node) {
            return;
        }
        // `ast.IsClassLike(node) && !inAmbientContext` (`checker.go:3605`).
        let has_non_ambient_class = declarations.iter().any(|&declaration| {
            matches!(
                self.nodes.kind(declaration),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            ) && !self.declaration_has_ambient_flag(declaration)
        });
        if !has_non_ambient_class {
            return;
        }
        // `relatedDiagnostics`: one `Consider adding a 'declare' modifier to
        // this class.` per class declaration, shared by every report.
        let mut related = Vec::new();
        for &declaration in &declarations {
            if self.nodes.kind(declaration) == SyntaxKind::ClassDeclaration
                && let Some(record) = self.diagnostic_for_node(
                    declaration,
                    &messages::CONSIDER_ADDING_A_DECLARE_MODIFIER_TO_THIS_CLASS,
                    [],
                )
            {
                related.push(record);
            }
        }
        let related = std::sync::Arc::new(related);
        for declaration in declarations {
            let class = match self.nodes.kind(declaration) {
                SyntaxKind::ClassDeclaration => true,
                SyntaxKind::FunctionDeclaration => false,
                _ => continue,
            };
            let Some(file) = self.source_file_of_for_diagnostics(declaration) else { continue };
            let at = self.declaration_name_of(declaration).unwrap_or(declaration);
            let span = self.nodes.span(at);
            let mut diagnostic = if class {
                Diagnostic::with_args(
                    &messages::CLASS_DECLARATION_CANNOT_IMPLEMENT_OVERLOAD_LIST_FOR_0,
                    span,
                    [name.clone()],
                )
            } else {
                Diagnostic::new(
                    &messages::FUNCTION_WITH_BODIES_CAN_ONLY_MERGE_WITH_CLASSES_THAT_ARE_AMBIENT,
                    span,
                )
            };
            diagnostic.set_related_information(related.clone());
            self.report(file, diagnostic);
        }
    }

    /// `node.Flags & ast.NodeFlagsAmbient` for a declaration in any file: a
    /// `declare` modifier on it or an ancestor, or a declaration file. This
    /// parser never sets `NodeFlags::AMBIENT`, and the per-file ambient bit
    /// the check traversal carries describes only the file being checked.
    fn declaration_has_ambient_flag(&self, declaration: NodeId) -> bool {
        let mut at = Some(declaration);
        while let Some(current) = at {
            if self.nodes.kind(current) == SyntaxKind::SourceFile {
                return self.module_host.is_some_and(|host| host.is_declaration_file(current));
            }
            if let Some(modifiers) = self.node_map.get(current).and_then(modifiers_of)
                && tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::DeclareKeyword)
            {
                return true;
            }
            at = self.nodes.parent(current);
        }
        false
    }
}
