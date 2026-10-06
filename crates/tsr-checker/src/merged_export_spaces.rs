//! TS2395 — `Individual declarations in merged declaration '{0}' must be all
//! exported or all local.`
//!
//! `checkExportsOnMergedDeclarations` (`checker.go:6909`-`:6958`).
//!
//! **Six call sites, and a function declaration is not one of them**:
//! `checkClassLikeDeclaration` (`:4298`), `checkInterfaceDeclaration` (`:5000`),
//! `checkEnumDeclaration` (`:5073`), `checkModuleDeclaration` (`:5161`),
//! `checkVariableLikeDeclaration` (`:5941`) and `checkTypeAliasDeclaration`
//! (`:6883`), all six hooked here. Hooking it on functions as well cost
//! twelve wrong lines and three `extraonly` cases — overload sets have their
//! *own* diagnostics, TS2383 and TS2384, at exactly the positions TS2395 was
//! landing on. §961.
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
    const NAMESPACE: Self = Self(4);

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
    /// One declaration that may be part of a merge. No parse-error gate:
    /// upstream checks a file with syntax errors too (`innerModExport2`).
    pub(crate) fn check_exports_on_merged_declarations(&mut self, node: NodeId) {
        let Some(name) = self.declaration_name_of(node) else { return };
        let Some(text) = self.identifier_text(name).map(str::to_string) else { return };
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let Some(symbol) = self.export_merge_local_symbol(node, symbol, &text) else { return };
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
            if self.declaration_has_modifier(declaration, SyntaxKind::ExportKeyword)
                || self.is_exported_by_ambient_export_context(declaration)
            {
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

    /// The symbol `checkExportsOnMergedDeclarations` walks (`checker.go:6912`):
    /// `node.LocalSymbol()` for an exported declaration, else
    /// `getSymbolOfDeclaration(node)` when it has an `ExportSymbol`; `None`
    /// for a purely local symbol, which upstream returns on.
    ///
    /// Both are the **local** half `declareModuleMember` (`binder.go:406`)
    /// creates in the container's locals, so only the declarations of one
    /// container (one namespace block, one module file) take part. The
    /// merged export symbol would join blocks that upstream never compares —
    /// `declare module "foo"` in a script against a `module "foo"` exported
    /// from another ambient module (`module_augmentUninstantiatedModule`).
    ///
    /// This port's binder records the export symbol as the node's symbol and
    /// links the local to it (`Symbol::export_symbol`) without a node → local
    /// edge, so an exported declaration's local is found by name in the
    /// enclosing locals tables: the entry whose export link is the node's
    /// symbol and whose declarations include the node.
    pub(crate) fn export_merge_local_symbol(
        &self,
        node: NodeId,
        symbol: tsr_binder::SymbolId,
        name: &str,
    ) -> Option<tsr_binder::SymbolId> {
        let symbols = self.binder.symbols();
        if symbols.get(symbol).export_symbol.is_some() {
            return Some(symbol);
        }
        self.nodes.ancestors(node).find_map(|ancestor| {
            let &local = self.binder.locals(ancestor)?.get(name)?;
            let entry = symbols.get(local);
            (entry.export_symbol == Some(symbol) && entry.declarations.contains(&node))
                .then_some(local)
        })
    }

    /// The ambient arm of `getEffectiveDeclarationFlags` (`checker.go:3701`):
    /// a declaration in an ambient **export context** — a `.d.ts` or ambient
    /// module with no `export` declaration or assignment
    /// (`setExportContextFlag`, `binder.go`) — is exported without the
    /// keyword, unless it writes its own `declare`, is a class or interface
    /// member, or sits directly in a `declare global` block.
    ///
    /// Reached once module augmentations merge across files
    /// (`docs/parity/notes/names-modules.md` §4): `declare module "./o" {
    /// interface O<T> { … } }` adds an implicitly exported declaration to the
    /// `export declare class O<T>` it augments, and without this arm the pair
    /// read as one exported and one local declaration (TS2395, wrongly, in
    /// `moduleAugmentationExtendFileModule1`/`2`).
    ///
    /// The parser sets no ambient flag (`bd tsr-o9tl`), so ambience is an
    /// ancestor walk to a `declare` or ambient module, and the declaration's
    /// own file is asked of the host.
    pub(crate) fn is_exported_by_ambient_export_context(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::InterfaceDeclaration
                | SyntaxKind::ClassDeclaration
                | SyntaxKind::ClassExpression
        ) {
            return false;
        }
        // `flags&ast.ModifierFlagsAmbient == 0`: the declaration's own
        // `declare`.
        if self.declaration_has_modifier(declaration, SyntaxKind::DeclareKeyword) {
            return false;
        }
        // `getEnclosingContainer`: the nearest container ancestor. Only a
        // module declaration or the source file can be an export context.
        let Some(container) = self.nodes.ancestors(declaration).find(|&ancestor| {
            tsr_binder::container_flags(
                self.node_map.get(ancestor).expect("an ancestor is registered"),
                self.nodes,
            )
            .contains(tsr_binder::ContainerFlags::IS_CONTAINER)
        }) else {
            return false;
        };
        let statements = match self.node_map.get(container) {
            Some(Node::SourceFile(file)) => {
                // A script declares into `Locals` (`declareSymbol`, not
                // `declareModuleMember`), so nothing in it has an
                // `ExportSymbol` and upstream's check returns before it
                // starts (`checker.go:6913-6920`).
                if self.binder.symbol_of(container).is_none()
                    || !self.is_declaration_file_node(container)
                {
                    return false;
                }
                file.statements
            }
            Some(Node::ModuleDeclaration(module)) => {
                // `IsModuleBlock(n.Parent) && IsGlobalScopeAugmentation(n.Parent.Parent)`.
                if module.keyword.kind == SyntaxKind::GlobalKeyword {
                    return false;
                }
                if !self.is_ambient_declaration(container) {
                    return false;
                }
                match module.body {
                    Some(tsr_ast::ModuleBody::ModuleBlock(block)) => block.statements,
                    _ => return false,
                }
            }
            _ => return false,
        };
        // `hasExportDeclarations`.
        !statements.iter().any(|statement| {
            matches!(
                statement,
                tsr_ast::Statement::ExportDeclaration(_) | tsr_ast::Statement::ExportAssignment(_)
            )
        })
    }

    /// `node.Flags & ast.NodeFlagsAmbient` for a node in any file: a `.d.ts`,
    /// or under a `declare` or string-named module declaration.
    pub(crate) fn is_ambient_declaration(&self, node: NodeId) -> bool {
        std::iter::once(node).chain(self.nodes.ancestors(node)).any(|current| {
            self.is_ambient_module_declaration(current)
                || self.declaration_has_modifier(current, SyntaxKind::DeclareKeyword)
                || (self.nodes.kind(current) == SyntaxKind::SourceFile
                    && self.is_declaration_file_node(current))
        })
    }

    /// Whether `file` is a declaration file, asked of the host: the
    /// declaration may be in any file of the program.
    fn is_declaration_file_node(&self, file: NodeId) -> bool {
        self.module_host.is_some_and(|host| host.is_declaration_file(file))
    }

    /// `getDeclarationSpaces` (`checker.go:6961`), minus the alias arms.
    ///
    /// Every alias form (`ImportEqualsDeclaration`, `NamespaceImport`,
    /// `ImportClause`, an entity-name export assignment) needs `resolveAlias`
    /// and answers `NONE` here, which can only shrink the intersection and so
    /// under-reports rather than mis-reports. §960.
    fn declaration_spaces(&self, declaration: NodeId) -> Spaces {
        match self.nodes.kind(declaration) {
            SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::MethodSignature
            | SyntaxKind::PropertySignature => Spaces::TYPE,
            // `IsAmbientModule(node) || GetModuleInstanceState(node) !=
            // ModuleInstanceStateNonInstantiated` — the unadjusted state, so a
            // `const enum`-only namespace counts as a value here.
            SyntaxKind::ModuleDeclaration => {
                if self.is_ambient_module_declaration(declaration)
                    || self.module_instance_state_of(declaration)
                        != tsr_ast::ModuleInstanceState::NonInstantiated
                {
                    Spaces::NAMESPACE.union(Spaces::VALUE)
                } else {
                    Spaces::NAMESPACE
                }
            }
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

    /// `ast.GetModuleInstanceState` (`ast/utilities.go:2322`) for a module
    /// declaration of any file, through its ancestor chain.
    fn module_instance_state_of(&self, declaration: NodeId) -> tsr_ast::ModuleInstanceState {
        let Some(typed) = self.node_map.get(declaration) else {
            return tsr_ast::ModuleInstanceState::Instantiated;
        };
        let mut parents: Vec<_> = self
            .nodes
            .ancestors(declaration)
            .filter_map(|ancestor| self.node_map.get(ancestor))
            .collect();
        parents.reverse();
        tsr_ast::module_instance_state(typed, &parents)
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
