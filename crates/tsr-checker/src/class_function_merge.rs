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
//! symbol per checker (`links.functionOrConstructorChecked`,
//! `checker.go:3463`) from both `checkFunctionOrMethodDeclaration` and
//! `checkClassLikeDeclaration`, from whichever declaration that checker
//! reaches first. The arm's output does not depend on the trigger, but which
//! checker runs it does: under several checkers each publishes only the
//! diagnostics on the files it owns, so the checker that owns a second
//! file's declaration must reach the arm from that declaration
//! (`duplicateIdentifiersAcrossFileBoundaries` under `--checkers 4`,
//! `tsr-2zk.1258`). An earlier port ran it only from the symbol's first
//! declaration, which only the checker owning that file ever reaches.
//!
//! Checker port convention: the link is `class_function_merge_checked`,
//! keyed by the merged `SymbolId`, owned by the `Checker`, set once on first
//! visit (no provisional state). The only traversal is the merged symbol's
//! declaration list and each declaration's ancestor chain for `declare`,
//! once per symbol per checker.

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
        // `if !links.functionOrConstructorChecked { … }`.
        if !self.class_function_merge_checked.insert(symbol) {
            return;
        }
        let declarations = entry.declarations.clone();
        let name = entry.name.to_string();
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
        for declaration in declarations {
            let class = match self.nodes.kind(declaration) {
                SyntaxKind::ClassDeclaration => true,
                SyntaxKind::FunctionDeclaration => false,
                _ => continue,
            };
            let Some(file) = self.source_file_of_for_diagnostics(declaration) else { continue };
            let at = self.declaration_name_of(declaration).unwrap_or(declaration);
            let span = self.nodes.span(at);
            let diagnostic = if class {
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
