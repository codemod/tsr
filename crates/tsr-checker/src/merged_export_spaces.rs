//! TS2395 — `Individual declarations in merged declaration '{0}' must be all
//! exported or all local.`
//!
//! `checkExportsOnMergedDeclarations` (`checker.go:6909`-`:6958`).
//!
//! **Six call sites, and a function declaration is not one of them**:
//! `checkClassLikeDeclaration` (`:4298`), `checkInterfaceDeclaration` (`:5000`),
//! `checkEnumDeclaration` (`:5073`), `checkModuleDeclaration` (`:5161`),
//! `checkVariableLikeDeclaration` (`:5941`) and `checkTypeAliasDeclaration`
//! (`:6883`). Hooking it on functions as well cost twelve wrong lines and three
//! `extraonly` cases — overload sets have their *own* diagnostics, TS2383 and
//! TS2384, at exactly the positions TS2395 was landing on. §961.
//!
//! `docs/architecture/checker-notes-diag2.md` §960.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `DeclarationSpaces` (`checker.go:6961`).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Spaces(u8);

impl Spaces {
    const NONE: Self = Self(0);
    const TYPE: Self = Self(1);
    const VALUE: Self = Self(2);

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl Checker<'_, '_> {
    /// One declaration that may be part of a merge.
    pub(crate) fn check_exports_on_merged_declarations(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(name) = self.declaration_name_of(node) else { return };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        // Once per symbol. Upstream runs once per *kind*, which repeats the
        // whole report for a symbol whose declarations differ in kind; §953's
        // set is the shape used here instead. §960.
        if !self.merged_spaces_checked.insert(symbol) {
            return;
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        if declarations.len() < 2 {
            return;
        }
        let (mut exported, mut non_exported, mut default_exported) =
            (Spaces::NONE, Spaces::NONE, Spaces::NONE);
        for &declaration in &declarations {
            let spaces = self.declaration_spaces(declaration);
            if self.declaration_has_modifier(declaration, SyntaxKind::ExportKeyword) {
                if self.declaration_has_modifier(declaration, SyntaxKind::DefaultKeyword) {
                    default_exported = default_exported.union(spaces);
                } else {
                    exported = exported.union(spaces);
                }
            } else {
                non_exported = non_exported.union(spaces);
            }
        }
        let non_default = exported.union(non_exported);
        let common_exports_and_locals = exported.intersection(non_exported);
        let common_default = default_exported.intersection(non_default);
        if common_exports_and_locals.is_empty() && common_default.is_empty() {
            return;
        }
        for declaration in declarations {
            let spaces = self.declaration_spaces(declaration);
            // **The `else if` is load-bearing even though its first arm is not
            // emitted.** Upstream's default branch is TS2652, which this port
            // does not report; computing its condition is what keeps a
            // declaration in the default-common set from taking TS2395.
            if spaces.intersects(common_default) {
                continue;
            }
            if !spaces.intersects(common_exports_and_locals) {
                continue;
            }
            let Some(at) = self.declaration_name_of(declaration) else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
            let span = self.error_span(at);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::INDIVIDUAL_DECLARATIONS_IN_MERGED_DECLARATION_0_MUST_BE_ALL_EXPORTED_OR_ALL_LOCAL,
                    span,
                    [text.clone()],
                ),
            );
        }
    }

    /// `getDeclarationSpaces` (`checker.go:6961`), the syntactic arms only.
    ///
    /// A module declaration needs `GetModuleInstanceState` and every alias form
    /// needs `resolveAlias`; both answer `NONE` here, which can only shrink the
    /// intersection and so under-reports rather than mis-reports. §960.
    fn declaration_spaces(&self, declaration: NodeId) -> Spaces {
        match self.nodes.kind(declaration) {
            SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::MethodSignature
            | SyntaxKind::PropertySignature => Spaces::TYPE,
            SyntaxKind::ClassDeclaration | SyntaxKind::EnumDeclaration | SyntaxKind::EnumMember => {
                Spaces::TYPE.union(Spaces::VALUE)
            }
            SyntaxKind::VariableDeclaration
            | SyntaxKind::BindingElement
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::ImportSpecifier => Spaces::VALUE,
            _ => Spaces::NONE,
        }
    }

    /// `getEffectiveDeclarationFlags`, restricted to the declaration's own
    /// modifier list — the only source of `export`/`default` in the shapes this
    /// rule reaches.
    fn declaration_has_modifier(&self, declaration: NodeId, keyword: SyntaxKind) -> bool {
        let Some(typed) = self.node_map.get(declaration) else { return false };
        let modifiers = match typed {
            Node::InterfaceDeclaration(n) => n.modifiers,
            Node::TypeAliasDeclaration(n) => n.modifiers,
            Node::ClassDeclaration(n) => n.modifiers,
            Node::EnumDeclaration(n) => n.modifiers,
            Node::FunctionDeclaration(n) => n.modifiers,
            Node::ModuleDeclaration(n) => n.modifiers,
            // A `var` carries its modifiers on the statement above it.
            Node::VariableDeclaration(_) => {
                return self
                    .nodes
                    .ancestors(declaration)
                    .find(|a| self.nodes.kind(*a) == SyntaxKind::VariableStatement)
                    .is_some_and(|statement| self.declaration_has_modifier(statement, keyword));
            }
            Node::VariableStatement(n) => n.modifiers,
            _ => return false,
        };
        modifiers
            .iter()
            .any(|modifier| modifier.node_id().is_some_and(|id| self.nodes.kind(id) == keyword))
    }
}
