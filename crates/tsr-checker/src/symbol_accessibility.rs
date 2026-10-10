//! `IsSymbolAccessible` and the `getAccessibleSymbolChain` walk under it, for
//! the declaration emitter's `SymbolTracker`.
//!
//! Pinned tsgo 5b1047d: `internal/checker/symbolaccessibility.go` (876 lines),
//! as one unit: `isSymbolAccessibleWorker` (`:845`), `IsAnySymbolAccessible`
//! (`:26`), `getContainersOfSymbol` (`:280`), `getWithAlternativeContainers`
//! (`:117`), `getAlternativeContainingModules` (`:170`),
//! `getAliasForSymbolInContainer` (`:344`), `getAccessibleSymbolChainEx`
//! (`:440`), `trySymbolTable` (`:535`), `getCandidateListForSymbol` (`:617`),
//! `isAccessible` (`:644`), `canQualifySymbol` (`:676`), `needsQualification`
//! (`:688`) and `someSymbolTableInScope` (`:746`).
//!
//! # Why a second walk beside the printer's naming code
//!
//! The type printer names symbols through [`Checker::symbol_chain`] and
//! [`Checker::is_shadowed_at`], each a *piece* of this file scoped to one
//! printer question and documented as not the whole walk (the conflated
//! `needs_qualification` OR is load-bearing there, `checker-notes-sitename.md`
//! §8). Re-pointing the printer is measured work in another lane's files; this
//! port answers only the tracker's question, so it is written whole and called
//! from nowhere else (`docs/parity/notes/r5-declemit3.md` §2).
//!
//! # Declines (each can only *add* a container upstream would also find, or
//! # miss a route — the tracker reports only on the arms listed in §2)
//!
//! - `getExportsOfSymbol` is a module's own exports plus what its
//!   `export *` declarations reach (first name wins, `default` skipped), and
//!   any other symbol's raw `exports`. Late-bound class statics are not read.
//! - `getAlternativeContainingModules` reads the enclosing file's
//!   statement-level import/export specifiers; dynamic `import()`, `require`
//!   and import types are not collected. Its all-files fallback
//!   (`extendedContainers`) reads the program's `SourceFile` rows
//!   (r7-printer §1.2). The caches are the checker's, as native's
//!   checker-lifetime symbol links: a file whose first query resolved one of
//!   its imports keeps that answer for every later query in the file
//!   (r7-printer §1.4).
//! - `getContainersOfSymbol`'s JavaScript `exports.A = class {}` arm.
//! - The port has no `globalThis` symbol (binder `merge_into_globals`); the
//!   globals arm of `trySymbolTable` is answered by
//!   [`DeclarationEmitResolver::is_global_this_accessible`] for the one
//!   question the tracker asks about it.

use rustc_hash::{FxHashMap, FxHashSet};
use tsr_ast::{Node, NodeId, SyntaxKind as K};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::symbol_access::DeclarationEmitResolver;

/// `ast.InternalSymbolNameExportStar`, the binder's `export *` entry (the
/// constant is private to the binder and to `symbols.rs`).
const INTERNAL_EXPORT_STAR: &str = "__export";

/// `printer.SymbolAccessibility`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolAccessibility {
    /// `SymbolAccessibilityAccessible`.
    Accessible,
    /// `SymbolAccessibilityNotAccessible`.
    NotAccessible,
    /// `SymbolAccessibilityCannotBeNamed`.
    CannotBeNamed,
    /// `SymbolAccessibilityNotResolved`.
    NotResolved,
}

/// `printer.SymbolAccessibilityResult`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolAccessibilityResult {
    /// The verdict.
    pub accessibility: SymbolAccessibility,
    /// `AliasesToMakeVisible`: statements an accessible route paints.
    pub aliases_to_make_visible: Vec<NodeId>,
    /// `ErrorSymbolName`.
    pub error_symbol_name: String,
    /// `ErrorModuleName`, empty when none.
    pub error_module_name: String,
    /// `ErrorNode`, set only for an enclosing declaration in a JS file.
    pub error_node: Option<NodeId>,
}

impl SymbolAccessibilityResult {
    fn accessible(aliases_to_make_visible: Vec<NodeId>) -> Self {
        Self {
            accessibility: SymbolAccessibility::Accessible,
            aliases_to_make_visible,
            error_symbol_name: String::new(),
            error_module_name: String::new(),
            error_node: None,
        }
    }
}

/// `symbolTableID`: which table a scope step hands the callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TableId {
    /// `location.Locals()`.
    Locals(NodeId),
    /// `sym.Exports` of a source file or module declaration.
    Exports(SymbolId),
    /// A class/interface's `members` filtered to `Type & ^Assignment`.
    Members(SymbolId),
    /// `c.globals`.
    Globals,
    /// `getExportsOfSymbol(sym)`.
    ResolvedExports(SymbolId),
    /// `getClassExpressionNameTable` (native keys it like the locals of the
    /// class expression; it is its own variant here because the port's class
    /// expression has no locals table to collide with).
    ClassExpressionName(NodeId),
}

/// One `someSymbolTableInScope` callback invocation.
#[derive(Clone, Copy, Debug)]
struct ScopeTable {
    table: TableId,
    is_local_name_lookup: bool,
    location: Option<NodeId>,
}

/// `accessibleSymbolChainContext`.
struct ChainContext {
    symbol: Option<SymbolId>,
    enclosing: NodeId,
    meaning: SymbolFlags,
    use_only_external_aliasing: bool,
}

/// The resolver's accessibility side tables.
///
/// Checker port convention record (`docs/conventions.md`):
///
/// - **Native operation**: `symbolContainerLinks.accessibleChainCache`,
///   `symbolContainerLinks.extendedContainersByFile`,
///   `symbolContainerLinks.extendedContainers` and
///   `Checker.symbolTableAliasCache`.
/// - **Key identity and owner**: the chain cache is keyed like native's
///   (`symbol`, `useOnlyExternalAliasing`, first relevant scope location,
///   `meaning`); the alias cache by table identity (globals and export
///   tables only, as native); the containing-modules cache by (`symbol`,
///   enclosing file), written only when the file's imports found a module;
///   the extended-containers cache by `symbol` alone, as native's; the
///   program's external-module list once; `getExportsOfSymbol` by symbol.
///   Owned by the checker (`Checker::accessibility_links`), as native's
///   links, and lent to each [`DeclarationEmitResolver`] for its life
///   (r7-printer §1.4).
/// - **Publication states**: absent = not computed; present = final.
///   Native publishes a chain even when the per-call `visitedSymbolTablesMap`
///   cut a recursive route short, and so does this port.
/// - **Receiver/alias context**: the enclosing declaration enters only
///   through the scope location in the key.
/// - **Expensive work**: alias resolution (`resolve_alias`, the checker's
///   memo) and `getExportsOfSymbol` per alias candidate; on the
///   `extendedContainers` fallback, one pass over the node table's kinds and
///   `getAliasForSymbolInContainer` per external module.
#[derive(Default)]
pub(crate) struct AccessibilityCache {
    chains: FxHashMap<(SymbolId, bool, Option<NodeId>, SymbolFlags), Vec<SymbolId>>,
    aliases: FxHashMap<TableId, Vec<SymbolId>>,
    containing_modules: FxHashMap<(SymbolId, NodeId), Vec<SymbolId>>,
    /// `symbolContainerLinks.extendedContainers`: the program-wide fallback
    /// of `getAlternativeContainingModules`, keyed by symbol alone (native's
    /// is not location-specific either).
    extended_containers: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// The program's external-module file symbols, in program order; built
    /// on the first fallback.
    program_external_modules: Option<Vec<SymbolId>>,
    /// `getExportsOfSymbol` per symbol (native's `resolvedExports` link).
    resolved_exports: FxHashMap<SymbolId, std::rc::Rc<[(String, SymbolId)]>>,
}

/// `modulespecifiers.CountPathComponents` (`modulespecifiers/compare.go:7`).
fn count_path_components(path: &str) -> usize {
    path.strip_prefix("./").unwrap_or(path).matches('/').count()
}

/// `getQualifiedLeftMeaning` (`symbolaccessibility.go:108`).
fn qualified_left_meaning(meaning: SymbolFlags) -> SymbolFlags {
    if meaning == SymbolFlags::VALUE { SymbolFlags::VALUE } else { SymbolFlags::NAMESPACE }
}

impl DeclarationEmitResolver<'_, '_, '_> {
    /// `Checker.IsSymbolAccessible` (`symbolaccessibility.go:841`) with
    /// `allowModules`, as `SymbolTrackerImpl.TrackSymbol` calls it
    /// (`shouldComputeAliasesToMakeVisible`).
    pub(crate) fn is_symbol_accessible(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> SymbolAccessibilityResult {
        self.is_symbol_accessible_worker(symbol, enclosing, meaning, true)
    }

    /// `IsTypeSymbolAccessible` (`:11`): type meaning, no alias painting.
    pub(crate) fn is_type_symbol_accessible(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
    ) -> bool {
        self.is_symbol_accessible_worker(symbol, enclosing, SymbolFlags::TYPE, false).accessibility
            == SymbolAccessibility::Accessible
    }

    /// `IsValueSymbolAccessible` (`:16`): value meaning, no alias painting.
    pub(crate) fn is_value_symbol_accessible(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
    ) -> bool {
        self.is_symbol_accessible_worker(symbol, enclosing, SymbolFlags::VALUE, false).accessibility
            == SymbolAccessibility::Accessible
    }

    /// `isSymbolAccessibleWorker` (`:845`) with `allowModules`.
    fn is_symbol_accessible_worker(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
        compute_aliases: bool,
    ) -> SymbolAccessibilityResult {
        if let Some(result) = self.is_any_symbol_accessible(
            &[symbol],
            enclosing,
            symbol,
            meaning,
            compute_aliases,
            true,
        ) {
            return result;
        }
        // A symbol not exported from its external module, or one from another
        // external module that no alias reaches.
        let declarations = self.checker.binder.symbols().get(symbol).declarations.clone();
        let symbol_module = declarations
            .iter()
            .find_map(|&declaration| self.external_module_container(declaration));
        if let Some(symbol_module) = symbol_module
            && Some(symbol_module) != self.external_module_container(enclosing)
        {
            return SymbolAccessibilityResult {
                accessibility: SymbolAccessibility::CannotBeNamed,
                aliases_to_make_visible: Vec::new(),
                error_symbol_name: self.symbol_error_name(symbol, enclosing, meaning),
                error_module_name: self.module_symbol_to_string(symbol_module),
                error_node: self.checker.in_js_file(enclosing).then_some(enclosing),
            };
        }
        SymbolAccessibilityResult {
            accessibility: SymbolAccessibility::NotAccessible,
            aliases_to_make_visible: Vec::new(),
            error_symbol_name: self.symbol_error_name(symbol, enclosing, meaning),
            error_module_name: String::new(),
            error_node: None,
        }
    }

    /// `IsAnySymbolAccessible` (`symbolaccessibility.go:26`).
    fn is_any_symbol_accessible(
        &mut self,
        symbols: &[SymbolId],
        enclosing: NodeId,
        initial: SymbolId,
        meaning: SymbolFlags,
        compute_aliases: bool,
        allow_modules: bool,
    ) -> Option<SymbolAccessibilityResult> {
        if symbols.is_empty() {
            return None;
        }
        let mut had_accessible_chain = None;
        let mut early_module_bail = false;
        for &symbol in symbols {
            let chain = self.accessible_symbol_chain(Some(symbol), enclosing, meaning, false);
            if let Some(&first) = chain.first() {
                had_accessible_chain = Some(symbol);
                if let Some(aliases) = self.has_visible_declarations(first, compute_aliases) {
                    return Some(SymbolAccessibilityResult::accessible(aliases));
                }
            }
            if allow_modules {
                let declarations = self.checker.binder.symbols().get(symbol).declarations.clone();
                if declarations
                    .iter()
                    .any(|&d| self.has_non_global_augmentation_external_module_symbol(d))
                {
                    if compute_aliases {
                        early_module_bail = true;
                        continue;
                    }
                    return Some(SymbolAccessibilityResult::accessible(Vec::new()));
                }
            }
            let containers = self.containers_of_symbol(symbol, enclosing, meaning);
            let next_meaning =
                if initial == symbol { qualified_left_meaning(meaning) } else { meaning };
            if let Some(result) = self.is_any_symbol_accessible(
                &containers,
                enclosing,
                initial,
                next_meaning,
                compute_aliases,
                allow_modules,
            ) {
                return Some(result);
            }
        }
        if early_module_bail {
            return Some(SymbolAccessibilityResult::accessible(Vec::new()));
        }
        let had = had_accessible_chain?;
        let error_module_name = if had == initial {
            String::new()
        } else {
            self.symbol_error_name(had, enclosing, SymbolFlags::NAMESPACE)
        };
        Some(SymbolAccessibilityResult {
            accessibility: SymbolAccessibility::NotAccessible,
            aliases_to_make_visible: Vec::new(),
            error_symbol_name: self.symbol_error_name(initial, enclosing, meaning),
            error_module_name,
            error_node: None,
        })
    }

    /// `hasNonGlobalAugmentationExternalModuleSymbol` (`:101`).
    fn has_non_global_augmentation_external_module_symbol(&self, declaration: NodeId) -> bool {
        match self.checker.node_map.get(declaration) {
            Some(Node::ModuleDeclaration(module)) => {
                matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            }
            Some(Node::SourceFile(file)) => tsr_binder::is_external_module(file),
            _ => false,
        }
    }

    /// `hasExternalModuleSymbol` (`:248`).
    fn has_external_module_symbol(&self, declaration: NodeId) -> bool {
        match self.checker.node_map.get(declaration) {
            Some(Node::ModuleDeclaration(module)) => {
                matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                    || module.keyword.kind == K::GlobalKeyword
            }
            Some(Node::SourceFile(file)) => tsr_binder::is_external_module(file),
            _ => false,
        }
    }

    /// `getExternalModuleContainer` (`:252`).
    fn external_module_container(&self, declaration: NodeId) -> Option<SymbolId> {
        let mut current = Some(declaration);
        while let Some(node) = current {
            if self.has_external_module_symbol(node) {
                return self
                    .checker
                    .binder
                    .symbol_of(node)
                    .map(|s| self.checker.binder.merged_symbol(s));
            }
            current = self.checker.nodes.parent(node);
        }
        None
    }

    /// `getParentOfSymbol`: the merged parent.
    fn parent_of_symbol(&self, symbol: SymbolId) -> Option<SymbolId> {
        let parent = self.checker.binder.symbols().get(symbol).parent?;
        Some(self.checker.binder.merged_symbol(parent))
    }

    /// `getSymbolIfSameReference`'s test.
    fn same_reference(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let identity = |this: &mut Self, symbol: SymbolId| {
            let merged = this.checker.binder.merged_symbol(symbol);
            let resolved = this.checker.resolve_alias_fully(merged);
            this.checker.binder.merged_symbol(resolved)
        };
        identity(self, a) == identity(self, b)
    }

    /// `getFileSymbolIfFileSymbolExportEqualsContainer` (`:260`).
    fn file_symbol_if_export_equals_container(
        &mut self,
        declaration: NodeId,
        container: SymbolId,
    ) -> Option<SymbolId> {
        let file_symbol = self.external_module_container(declaration)?;
        let exported = *self.checker.binder.symbols().get(file_symbol).exports.get("export=")?;
        self.same_reference(exported, container).then_some(file_symbol)
    }

    /// `getContainersOfSymbol` (`:280`).
    fn containers_of_symbol(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> Vec<SymbolId> {
        let flags = self.checker.binder.symbols().get(symbol).flags;
        if let Some(container) = self.parent_of_symbol(symbol)
            && !flags.contains(SymbolFlags::TYPE_PARAMETER)
        {
            return self.with_alternative_containers(container, symbol, enclosing, meaning);
        }
        let declarations = self.checker.binder.symbols().get(symbol).declarations.clone();
        let mut candidates: Vec<SymbolId> = Vec::new();
        for declaration in declarations {
            let ambient_module = matches!(
                self.checker.node_map.get(declaration),
                Some(Node::ModuleDeclaration(module))
                    if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                        || module.keyword.kind == K::GlobalKeyword
            );
            let Some(parent) = self.checker.nodes.parent(declaration) else { continue };
            if ambient_module {
                continue;
            }
            // Direct children of a module.
            if self.has_non_global_augmentation_external_module_symbol(parent) {
                if let Some(owner) = self.checker.binder.symbol_of(parent) {
                    let owner = self.checker.binder.merged_symbol(owner);
                    if !candidates.contains(&owner) {
                        candidates.push(owner);
                    }
                }
                continue;
            }
            // An `export =`'d member of an ambient module.
            if self.checker.nodes.kind(parent) == K::ModuleBlock
                && let Some(module) = self.checker.nodes.parent(parent)
                && let Some(owner) = self.checker.binder.symbol_of(module)
            {
                let owner = self.checker.binder.merged_symbol(owner);
                if self.checker.resolve_external_module_symbol(owner) == symbol
                    && !candidates.contains(&owner)
                {
                    candidates.push(owner);
                }
            }
        }
        let mut best = Vec::new();
        let mut alternatives = Vec::new();
        for container in candidates {
            if self.alias_for_symbol_in_container(container, symbol).is_none() {
                continue;
            }
            let all = self.with_alternative_containers(container, symbol, enclosing, meaning);
            let Some((&first, rest)) = all.split_first() else { continue };
            best.push(first);
            alternatives.extend_from_slice(rest);
        }
        best.extend(alternatives);
        best
    }

    /// `getWithAlternativeContainers` (`:117`).
    fn with_alternative_containers(
        &mut self,
        container: SymbolId,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> Vec<SymbolId> {
        let declarations = self.checker.binder.symbols().get(container).declarations.clone();
        let additional: Vec<SymbolId> = declarations
            .iter()
            .filter_map(|&d| self.file_symbol_if_export_equals_container(d, container))
            .collect();
        let reexports = self.alternative_containing_modules(symbol, enclosing);
        let object_literal = self.variable_declaration_of_object_literal(container, meaning);
        let left = qualified_left_meaning(meaning);
        let container_flags = self.checker.binder.symbols().get(container).flags;
        if container_flags.intersects(left)
            && !self
                .accessible_symbol_chain(Some(container), enclosing, SymbolFlags::NAMESPACE, false)
                .is_empty()
        {
            let mut out = vec![container];
            out.extend(additional);
            out.extend(reexports);
            out.extend(object_literal);
            return out;
        }
        // A member of the instance side of something: a variable in scope
        // with the container's type acts like a namespace (`Symbol` for
        // `Symbol.toStringTag`).
        let mut variable_matches = Vec::new();
        if meaning == SymbolFlags::VALUE
            && !container_flags.intersects(left)
            && container_flags.intersects(SymbolFlags::TYPE)
        {
            let declared = self.checker.get_declared_type_of_symbol(container);
            if self.checker.store.get(declared).flags.intersects(crate::flags::TypeFlags::OBJECT) {
                for scope in self.scope_tables(enclosing) {
                    let entries = self.table_entries(scope.table);
                    let mut found = false;
                    for candidate in entries {
                        if self.checker.binder.symbols().get(candidate).flags.intersects(left)
                            && self.checker.get_type_of_symbol(candidate) == declared
                        {
                            variable_matches.push(candidate);
                            found = true;
                        }
                    }
                    if found {
                        break;
                    }
                }
                variable_matches.sort_by_cached_key(|&s| self.checker.compare_symbols_key(s));
            }
        }
        let mut out = variable_matches;
        out.extend(additional);
        out.push(container);
        out.extend(object_literal);
        out.extend(reexports);
        out
    }

    /// `getAlternativeContainingModules` (`:170`), over the enclosing file's
    /// statement-level module specifiers (module docs).
    fn alternative_containing_modules(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
    ) -> Vec<SymbolId> {
        let Some(file) = self.checker.source_file_of(enclosing) else { return Vec::new() };
        if let Some(cached) = self.accessibility.containing_modules.get(&(symbol, file)) {
            return cached.clone();
        }
        let mut specifiers = Vec::new();
        if let Some(Node::SourceFile(source)) = self.checker.node_map.get(file) {
            for statement in source.statements {
                let specifier = match statement {
                    tsr_ast::Statement::ImportDeclaration(node) => node.module_specifier,
                    tsr_ast::Statement::ExportDeclaration(node) => node.module_specifier,
                    tsr_ast::Statement::ImportEqualsDeclaration(node) => {
                        match node.module_reference {
                            Some(tsr_ast::ModuleReference::ExternalModuleReference(external)) => {
                                external.expression
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(id) = specifier.and_then(|e| Node::from(e).node_id()) {
                    specifiers.push(id);
                }
            }
        }
        let mut results = Vec::new();
        for specifier in specifiers {
            let Some(module) = self.checker.resolve_external_module_name(enclosing, specifier)
            else {
                continue;
            };
            let module = self.checker.binder.merged_symbol(module);
            if self.alias_for_symbol_in_container(module, symbol).is_some() {
                results.push(module);
            }
        }
        if !results.is_empty() {
            self.accessibility.containing_modules.insert((symbol, file), results.clone());
            return results;
        }
        // No results from files already imported by this one: every external
        // module of the program (`extendedContainers`, `:217`), not
        // location-specific, so cached per symbol.
        if let Some(cached) = self.accessibility.extended_containers.get(&symbol) {
            return cached.clone();
        }
        for module in self.program_external_modules() {
            if self.alias_for_symbol_in_container(module, symbol).is_some() {
                results.push(module);
            }
        }
        self.accessibility.extended_containers.insert(symbol, results.clone());
        results
    }

    /// `c.program.SourceFiles()` filtered to `ast.IsExternalModule`, as
    /// their merged module symbols, in program order. The checker holds no
    /// file list; the node table's `SourceFile` rows are exactly the
    /// program's files, allocated in program order (the order
    /// [`Checker::compare_symbols_key`] already reads). Computed once per
    /// checker, only when `getAlternativeContainingModules` first falls back.
    fn program_external_modules(&mut self) -> Vec<SymbolId> {
        if let Some(modules) = &self.accessibility.program_external_modules {
            return modules.clone();
        }
        let nodes = self.checker.nodes;
        let modules: Vec<SymbolId> = (0..nodes.len())
            .filter_map(|index| {
                let file = NodeId::new(u32::try_from(index).ok()?);
                if nodes.kind(file) != K::SourceFile {
                    return None;
                }
                let Some(Node::SourceFile(source)) = self.checker.node_map.get(file) else {
                    return None;
                };
                if !tsr_binder::is_external_module(source) {
                    return None;
                }
                let module = self.checker.binder.symbol_of(file)?;
                Some(self.checker.binder.merged_symbol(module))
            })
            .collect();
        self.accessibility.program_external_modules = Some(modules.clone());
        modules
    }

    /// `getVariableDeclarationOfObjectLiteral` (`:226`).
    fn variable_declaration_of_object_literal(
        &self,
        symbol: SymbolId,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        if !meaning.intersects(SymbolFlags::VALUE) {
            return None;
        }
        let first = *self.checker.binder.symbols().get(symbol).declarations.first()?;
        let parent = self.checker.nodes.parent(first)?;
        let Some(Node::VariableDeclaration(variable)) = self.checker.node_map.get(parent) else {
            return None;
        };
        let is_initializer = self.checker.nodes.kind(first) == K::ObjectLiteralExpression
            && variable.initializer.and_then(|e| Node::from(e).node_id()) == Some(first);
        let is_type = self.checker.nodes.kind(first) == K::TypeLiteral
            && variable.r#type.and_then(|t| t.node_id()) == Some(first);
        if !(is_initializer || is_type) {
            return None;
        }
        self.checker.binder.symbol_of(parent).map(|s| self.checker.binder.merged_symbol(s))
    }

    /// `getExportsOfSymbol`, as the module docs scope it: a module's exports
    /// with its `export *` targets', any other symbol's raw exports.
    /// Memoized per symbol for the checker's lifetime, as native's
    /// `resolvedExports` link is: the binder's tables do not change once
    /// checking starts.
    fn exports_of_symbol(&mut self, symbol: SymbolId) -> std::rc::Rc<[(String, SymbolId)]> {
        if let Some(exports) = self.accessibility.resolved_exports.get(&symbol) {
            return exports.clone();
        }
        let mut visited = Vec::new();
        let exports: std::rc::Rc<[(String, SymbolId)]> =
            self.exports_with_stars(symbol, &mut visited).into();
        self.accessibility.resolved_exports.insert(symbol, exports.clone());
        exports
    }

    fn exports_with_stars(
        &mut self,
        symbol: SymbolId,
        visited: &mut Vec<SymbolId>,
    ) -> Vec<(String, SymbolId)> {
        if visited.contains(&symbol) {
            return Vec::new();
        }
        visited.push(symbol);
        let entry = self.checker.binder.symbols().get(symbol);
        let is_module = entry.flags.intersects(SymbolFlags::MODULE);
        let mut table: Vec<(String, SymbolId)> = entry
            .exports
            .iter()
            .filter(|(name, _)| **name != INTERNAL_EXPORT_STAR)
            .map(|(name, &s)| ((*name).to_string(), s))
            .collect();
        if !is_module {
            return table;
        }
        let stars: Vec<NodeId> = entry
            .exports
            .get(INTERNAL_EXPORT_STAR)
            .map(|&star| self.checker.binder.symbols().get(star).declarations.to_vec())
            .unwrap_or_default();
        let mut seen: FxHashSet<String> = table.iter().map(|(n, _)| n.clone()).collect();
        let mut nested: Vec<(String, SymbolId)> = Vec::new();
        let mut nested_seen = FxHashSet::default();
        for declaration in stars {
            let Some(Node::ExportDeclaration(node)) = self.checker.node_map.get(declaration) else {
                continue;
            };
            let Some(specifier) = node.module_specifier.and_then(|e| Node::from(e).node_id())
            else {
                continue;
            };
            let Some(target) = self.checker.resolve_external_module_name(declaration, specifier)
            else {
                continue;
            };
            let target = self.checker.binder.merged_symbol(target);
            for (name, s) in self.exports_with_stars(target, visited) {
                if name != "default" && nested_seen.insert(name.clone()) {
                    nested.push((name, s));
                }
            }
        }
        for (name, s) in nested {
            if seen.insert(name.clone()) {
                table.push((name, s));
            }
        }
        table
    }

    /// `getAliasForSymbolInContainer` (`:344`).
    fn alias_for_symbol_in_container(
        &mut self,
        container: SymbolId,
        symbol: SymbolId,
    ) -> Option<SymbolId> {
        if Some(container) == self.parent_of_symbol(symbol) {
            return Some(symbol);
        }
        if let Some(&export_equals) =
            self.checker.binder.symbols().get(container).exports.get("export=")
            && self.same_reference(export_equals, symbol)
        {
            return Some(container);
        }
        let exports = self.exports_of_symbol(container);
        let name = self.checker.binder.symbols().get(symbol).name;
        if let Some(&(_, quick)) = exports.iter().find(|(n, _)| n == name)
            && self.same_reference(quick, symbol)
        {
            return Some(quick);
        }
        let mut candidates: Vec<SymbolId> = Vec::new();
        for &(_, exported) in exports.iter() {
            if self.same_reference(exported, symbol) {
                candidates.push(exported);
            }
        }
        candidates.sort_by_cached_key(|&s| self.checker.compare_symbols_key(s));
        candidates.first().copied()
    }

    /// `getSymbolChain` (`nodebuilderimpl.go:1087`): the accessible chain
    /// of `symbol` at `enclosing`, else a container's chain followed by the
    /// symbol (or its alias in that container), else the symbol alone when
    /// `end_of_chain` or it is not an anonymous type. The containers are
    /// `getContainersOfSymbol`'s, so a member of an interface reached through
    /// a variable of its type takes that variable (`Symbol.iterator`,
    /// `getWithAlternativeContainers`' variable-match arm). Parents are tried
    /// in `sortByBestName`'s order with this port's module specifiers
    /// ([`Checker::module_specifier_for_symbol`]).
    ///
    /// `use_only_external_aliasing` is false, `symbolToExpression`'s flags.
    pub(crate) fn symbol_chain_at(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
        end_of_chain: bool,
        yield_module_symbol: bool,
        depth: usize,
    ) -> Vec<SymbolId> {
        // Native has no cap; a container cycle would be a binder defect.
        if depth > 8 {
            return Vec::new();
        }
        let mut chain = self.accessible_symbol_chain(Some(symbol), enclosing, meaning, false);
        let qualifier_meaning =
            if chain.len() > 1 { qualified_left_meaning(meaning) } else { meaning };
        if chain.is_empty() || self.needs_qualification(chain[0], enclosing, qualifier_meaning) {
            let root = chain.first().copied().unwrap_or(symbol);
            let parents = self.containers_of_symbol(root, enclosing, meaning);
            let mut parents: Vec<(SymbolId, String)> = parents
                .into_iter()
                .map(|parent| {
                    let declarations =
                        self.checker.binder.symbols().get(parent).declarations.clone();
                    let module = declarations
                        .iter()
                        .any(|&d| self.has_non_global_augmentation_external_module_symbol(d));
                    let specifier = if module {
                        self.checker
                            .module_specifier_for_symbol(parent, enclosing)
                            .map(|quoted| tsr_core::strip_quotes(&quoted).to_string())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    (parent, specifier)
                })
                .collect();
            parents.sort_by(|(a, specifier_a), (b, specifier_b)| {
                if !specifier_a.is_empty() && !specifier_b.is_empty() {
                    let relative_a = tsr_path::path_is_relative(specifier_a);
                    let relative_b = tsr_path::path_is_relative(specifier_b);
                    return if relative_a == relative_b {
                        count_path_components(specifier_a).cmp(&count_path_components(specifier_b))
                    } else if relative_b {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }
                self.checker.compare_symbols_key(*a).cmp(&self.checker.compare_symbols_key(*b))
            });
            for (parent, _) in parents {
                let parent_chain = self.symbol_chain_at(
                    parent,
                    enclosing,
                    qualified_left_meaning(meaning),
                    false,
                    yield_module_symbol,
                    depth + 1,
                );
                if parent_chain.is_empty() {
                    continue;
                }
                if let Some(&exported) =
                    self.checker.binder.symbols().get(parent).exports.get("export=")
                    && self.same_reference(exported, symbol)
                {
                    // The parent chain's root is the symbol: a module's
                    // `export =` looks like its own parent.
                    chain = parent_chain;
                    break;
                }
                let next = if chain.is_empty() {
                    vec![self.alias_for_symbol_in_container(parent, symbol).unwrap_or(symbol)]
                } else {
                    chain
                };
                chain = parent_chain;
                chain.extend(next);
                break;
            }
        }
        if !chain.is_empty() {
            return chain;
        }
        let record = self.checker.binder.symbols().get(symbol);
        if end_of_chain
            || !record.flags.intersects(SymbolFlags::TYPE_LITERAL | SymbolFlags::OBJECT_LITERAL)
        {
            // An external-module parent is not written (`x` over `"foo/bar".x`).
            let declarations = record.declarations.clone();
            if !end_of_chain
                && !yield_module_symbol
                && declarations
                    .iter()
                    .any(|&d| self.has_non_global_augmentation_external_module_symbol(d))
            {
                return Vec::new();
            }
            return vec![symbol];
        }
        Vec::new()
    }

    /// `getAccessibleSymbolChain` (`:373`).
    pub(crate) fn accessible_symbol_chain(
        &mut self,
        symbol: Option<SymbolId>,
        enclosing: NodeId,
        meaning: SymbolFlags,
        use_only_external_aliasing: bool,
    ) -> Vec<SymbolId> {
        let context = ChainContext { symbol, enclosing, meaning, use_only_external_aliasing };
        let mut visited = FxHashMap::default();
        self.accessible_symbol_chain_ex(&context, &mut visited)
    }

    /// `getAccessibleSymbolChainEx` (`:440`).
    fn accessible_symbol_chain_ex(
        &mut self,
        context: &ChainContext,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> Vec<SymbolId> {
        let Some(symbol) = context.symbol else { return Vec::new() };
        if self.is_property_or_method_declaration_symbol(symbol) {
            return Vec::new();
        }
        let scopes = self.scope_tables(context.enclosing);
        let first_location = scopes.first().and_then(|scope| scope.location);
        let key = (symbol, context.use_only_external_aliasing, first_location, context.meaning);
        if let Some(cached) = self.accessibility.chains.get(&key) {
            return cached.clone();
        }
        let mut result = Vec::new();
        for scope in scopes {
            let chain = self.chain_from_symbol_table(
                context,
                symbol,
                scope.table,
                false,
                scope.is_local_name_lookup,
                visited,
            );
            if !chain.is_empty() {
                result = chain;
                break;
            }
        }
        self.accessibility.chains.insert(key, result.clone());
        result
    }

    /// `getAccessibleSymbolChainFromSymbolTable` (`:478`).
    fn chain_from_symbol_table(
        &mut self,
        context: &ChainContext,
        symbol: SymbolId,
        table: TableId,
        ignore_qualification: bool,
        is_local_name_lookup: bool,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> Vec<SymbolId> {
        if !visited.entry(symbol).or_default().insert(table) {
            return Vec::new();
        }
        let result = self.try_symbol_table(
            context,
            symbol,
            table,
            ignore_qualification,
            is_local_name_lookup,
            visited,
        );
        if let Some(tables) = visited.get_mut(&symbol) {
            tables.remove(&table);
        }
        result
    }

    /// `trySymbolTable` (`:535`).
    fn try_symbol_table(
        &mut self,
        context: &ChainContext,
        symbol: SymbolId,
        table: TableId,
        ignore_qualification: bool,
        is_local_name_lookup: bool,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> Vec<SymbolId> {
        let name = self.checker.binder.symbols().get(symbol).name;
        let direct = self.table_get(table, name);
        if let Some(found) = direct
            && self.is_accessible(context, symbol, found, None, ignore_qualification, visited)
        {
            return vec![symbol];
        }
        let mut candidate_chains: Vec<Vec<SymbolId>> = Vec::new();
        if let Some(found) = direct
            && let Some(export_symbol) = self.checker.binder.symbols().get(found).export_symbol
        {
            let export_symbol = self.checker.binder.merged_symbol(export_symbol);
            if self.is_accessible(
                context,
                symbol,
                export_symbol,
                None,
                ignore_qualification,
                visited,
            ) {
                candidate_chains.push(vec![symbol]);
            }
        }
        let in_external_module = self
            .checker
            .source_file_of(context.enclosing)
            .and_then(|file| match self.checker.node_map.get(file) {
                Some(Node::SourceFile(source)) => Some(tsr_binder::is_external_module(source)),
                _ => None,
            })
            .unwrap_or(false);
        for alias in self.table_aliases(table) {
            let entry = self.checker.binder.symbols().get(alias);
            let declarations = entry.declarations.clone();
            // `export=` and `default` (stored as `""` by this binder).
            if matches!(entry.name, "export=" | "default" | "") {
                continue;
            }
            let umd = declarations
                .first()
                .is_some_and(|&d| self.checker.nodes.kind(d) == K::NamespaceExportDeclaration);
            if umd && in_external_module {
                continue;
            }
            if context.use_only_external_aliasing
                && !declarations.iter().any(|&d| self.is_external_module_import_equals(d))
            {
                continue;
            }
            if is_local_name_lookup
                && declarations.iter().any(|&d| self.is_namespace_reexport_declaration(d))
            {
                continue;
            }
            if !ignore_qualification
                && declarations.iter().any(|&d| self.checker.nodes.kind(d) == K::ExportSpecifier)
            {
                continue;
            }
            // `resolveAlias` → `unknownSymbol`, which neither matches nor has
            // exports, when the port cannot resolve the target.
            let Some(resolved) = self.checker.resolve_alias(alias) else { continue };
            let candidate = self.candidate_list_for_symbol(
                context,
                symbol,
                alias,
                resolved,
                ignore_qualification,
                visited,
            );
            if !candidate.is_empty() {
                candidate_chains.push(candidate);
            }
        }
        if !candidate_chains.is_empty() {
            // `compareSymbolChains`: shorter first, then element-wise.
            candidate_chains.sort_by(|a, b| {
                a.len().cmp(&b.len()).then_with(|| {
                    for (x, y) in a.iter().zip(b) {
                        let order = self.checker.compare_symbols(*x, *y);
                        if order != std::cmp::Ordering::Equal {
                            return order;
                        }
                    }
                    std::cmp::Ordering::Equal
                })
            });
            return candidate_chains.swap_remove(0);
        }
        // The globals table's `globalThis` arm: the port has no `globalThis`
        // symbol (module docs); its `[globalThis, symbol]` route is a
        // qualified name, which the tracker never needs to spell, so only its
        // existence matters, and that is the shadowing question
        // `is_global_this_accessible` answers.
        if table == TableId::Globals
            && self.checker.binder.globals().get(name).is_some_and(|&g| {
                self.checker.binder.merged_symbol(g) == self.checker.binder.merged_symbol(symbol)
            })
            && self.is_global_this_accessible(context.enclosing)
        {
            return vec![symbol];
        }
        Vec::new()
    }

    /// `getCandidateListForSymbol` (`:617`).
    fn candidate_list_for_symbol(
        &mut self,
        context: &ChainContext,
        symbol: SymbolId,
        from_table: SymbolId,
        resolved: SymbolId,
        ignore_qualification: bool,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> Vec<SymbolId> {
        if self.is_accessible(
            context,
            symbol,
            from_table,
            Some(resolved),
            ignore_qualification,
            visited,
        ) {
            return vec![from_table];
        }
        let from_exports = self.chain_from_symbol_table(
            context,
            symbol,
            TableId::ResolvedExports(resolved),
            true,
            false,
            visited,
        );
        if from_exports.is_empty() {
            return Vec::new();
        }
        if !self.can_qualify_symbol(
            context,
            from_table,
            qualified_left_meaning(context.meaning),
            visited,
        ) {
            return Vec::new();
        }
        let mut chain = vec![from_table];
        chain.extend(from_exports);
        chain
    }

    /// `isAccessible` (`:644`).
    fn is_accessible(
        &mut self,
        context: &ChainContext,
        symbol: SymbolId,
        from_table: SymbolId,
        resolved_alias: Option<SymbolId>,
        ignore_qualification: bool,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> bool {
        let merged = |this: &Self, s: SymbolId| this.checker.binder.merged_symbol(s);
        let target = merged(self, symbol);
        let like = Some(symbol) == resolved_alias
            || symbol == from_table
            || resolved_alias.is_some_and(|r| merged(self, r) == target)
            || merged(self, from_table) == target;
        if !like {
            return false;
        }
        let declarations = self.checker.binder.symbols().get(from_table).declarations.clone();
        if declarations.iter().any(|&d| self.has_non_global_augmentation_external_module_symbol(d))
        {
            return false;
        }
        ignore_qualification
            || self.can_qualify_symbol(context, merged(self, from_table), context.meaning, visited)
    }

    /// `canQualifySymbol` (`:676`).
    fn can_qualify_symbol(
        &mut self,
        context: &ChainContext,
        symbol: SymbolId,
        meaning: SymbolFlags,
        visited: &mut FxHashMap<SymbolId, FxHashSet<TableId>>,
    ) -> bool {
        if !self.needs_qualification(symbol, context.enclosing, meaning) {
            return true;
        }
        let parent = self.checker.binder.symbols().get(symbol).parent;
        let parent_context = ChainContext {
            symbol: parent,
            enclosing: context.enclosing,
            meaning: qualified_left_meaning(meaning),
            use_only_external_aliasing: context.use_only_external_aliasing,
        };
        !self.accessible_symbol_chain_ex(&parent_context, visited).is_empty()
    }

    /// `needsQualification` (`:688`).
    fn needs_qualification(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> bool {
        let name = self.checker.binder.symbols().get(symbol).name;
        for scope in self.scope_tables(enclosing) {
            let Some(found) = self.table_get(scope.table, name) else { continue };
            let mut from_table = self.checker.binder.merged_symbol(found);
            if from_table == symbol {
                return false;
            }
            let entry = self.checker.binder.symbols().get(from_table);
            let should_resolve_alias = entry.flags.contains(SymbolFlags::ALIAS)
                && !entry
                    .declarations
                    .iter()
                    .any(|&d| self.checker.nodes.kind(d) == K::ExportSpecifier);
            let flags = if should_resolve_alias {
                // `unknownSymbol` (`SymbolFlagsProperty`) when the target
                // does not resolve here.
                match self.checker.resolve_alias(from_table) {
                    Some(target) => {
                        from_table = target;
                        self.checker.get_symbol_flags(from_table)
                    }
                    None => SymbolFlags::PROPERTY,
                }
            } else {
                entry.flags
            };
            if flags.intersects(meaning) {
                return true;
            }
        }
        false
    }

    /// `isPropertyOrMethodDeclarationSymbol` (`:728`).
    fn is_property_or_method_declaration_symbol(&self, symbol: SymbolId) -> bool {
        let declarations = &self.checker.binder.symbols().get(symbol).declarations;
        !declarations.is_empty()
            && declarations.iter().all(|&d| {
                matches!(
                    self.checker.nodes.kind(d),
                    K::PropertyDeclaration | K::MethodDeclaration | K::GetAccessor | K::SetAccessor
                )
            })
    }

    fn is_external_module_import_equals(&self, declaration: NodeId) -> bool {
        matches!(
            self.checker.node_map.get(declaration),
            Some(Node::ImportEqualsDeclaration(import))
                if matches!(import.module_reference, Some(tsr_ast::ModuleReference::ExternalModuleReference(_)))
        )
    }

    /// `isNamespaceReexportDeclaration`: `export * as ns from "x"`.
    fn is_namespace_reexport_declaration(&self, declaration: NodeId) -> bool {
        if self.checker.nodes.kind(declaration) != K::NamespaceExport {
            return false;
        }
        let Some(parent) = self.checker.nodes.parent(declaration) else { return false };
        matches!(self.checker.node_map.get(parent),
            Some(Node::ExportDeclaration(export)) if export.module_specifier.is_some())
    }

    /// `someSymbolTableInScope` (`:746`), as the list of tables it would
    /// hand its callback, outermost last.
    fn scope_tables(&mut self, enclosing: NodeId) -> Vec<ScopeTable> {
        let mut out = Vec::new();
        let mut current = Some(enclosing);
        while let Some(location) = current {
            let global_source_file = matches!(self.checker.node_map.get(location),
                Some(Node::SourceFile(file)) if !tsr_binder::is_external_module(file));
            if !global_source_file && self.checker.binder.locals(location).is_some() {
                out.push(ScopeTable {
                    table: TableId::Locals(location),
                    is_local_name_lookup: true,
                    location: Some(location),
                });
            }
            match self.checker.nodes.kind(location) {
                K::SourceFile | K::ModuleDeclaration => {
                    if !global_source_file
                        && let Some(owner) = self.checker.binder.symbol_of(location)
                    {
                        let owner = self.checker.binder.merged_symbol(owner);
                        out.push(ScopeTable {
                            table: TableId::Exports(owner),
                            is_local_name_lookup: true,
                            location: Some(location),
                        });
                    }
                }
                K::ClassDeclaration | K::ClassExpression | K::InterfaceDeclaration => {
                    if let Some(owner) = self.checker.binder.symbol_of(location) {
                        let owner = self.checker.binder.merged_symbol(owner);
                        if !self.table_entries(TableId::Members(owner)).is_empty() {
                            out.push(ScopeTable {
                                table: TableId::Members(owner),
                                is_local_name_lookup: false,
                                location: Some(location),
                            });
                        }
                    }
                    if let Some(Node::ClassExpression(class)) = self.checker.node_map.get(location)
                        && class.name.is_some()
                    {
                        out.push(ScopeTable {
                            table: TableId::ClassExpressionName(location),
                            is_local_name_lookup: true,
                            location: Some(location),
                        });
                    }
                }
                _ => {}
            }
            current = self.checker.nodes.parent(location);
        }
        out.push(ScopeTable {
            table: TableId::Globals,
            is_local_name_lookup: true,
            location: None,
        });
        out
    }

    /// The members filter of `someSymbolTableInScope` (`:775`).
    fn is_type_member(&self, symbol: SymbolId) -> bool {
        let flags = self.checker.binder.symbols().get(symbol).flags;
        flags.intersects(SymbolFlags::TYPE.difference(SymbolFlags::ASSIGNMENT))
    }

    /// One table lookup by name.
    fn table_get(&mut self, table: TableId, name: &str) -> Option<SymbolId> {
        let binder = self.checker.binder;
        match table {
            TableId::Locals(node) => binder.locals(node)?.get(name).copied(),
            TableId::Exports(owner) => binder.symbols().get(owner).exports.get(name).copied(),
            TableId::Members(owner) => binder
                .symbols()
                .get(owner)
                .members
                .get(name)
                .copied()
                .filter(|&s| self.is_type_member(s)),
            TableId::Globals => binder.globals().get(name).copied(),
            TableId::ResolvedExports(owner) => {
                self.exports_of_symbol(owner).iter().find_map(|(n, s)| (n == name).then_some(*s))
            }
            TableId::ClassExpressionName(node) => {
                let Some(Node::ClassExpression(class)) = self.checker.node_map.get(node) else {
                    return None;
                };
                let text = class.name?.text;
                if text != name {
                    return None;
                }
                binder.symbol_of(node).map(|s| binder.merged_symbol(s))
            }
        }
    }

    /// Every symbol of a table.
    fn table_entries(&mut self, table: TableId) -> Vec<SymbolId> {
        let binder = self.checker.binder;
        match table {
            TableId::Locals(node) => {
                binder.locals(node).map(|t| t.values().copied().collect()).unwrap_or_default()
            }
            TableId::Exports(owner) => {
                binder.symbols().get(owner).exports.iter().map(|(_, &s)| s).collect()
            }
            TableId::Members(owner) => binder
                .symbols()
                .get(owner)
                .members
                .iter()
                .map(|(_, &s)| s)
                .filter(|&s| self.is_type_member(s))
                .collect(),
            TableId::Globals => binder.globals().values().copied().collect(),
            TableId::ResolvedExports(owner) => {
                self.exports_of_symbol(owner).iter().map(|&(_, s)| s).collect()
            }
            TableId::ClassExpressionName(node) => {
                binder.symbol_of(node).map(|s| vec![binder.merged_symbol(s)]).unwrap_or_default()
            }
        }
    }

    /// `getSymbolTableAliases` (`:497`): the alias-flagged entries, cached
    /// for globals and export tables as native does; members never hold one.
    fn table_aliases(&mut self, table: TableId) -> Vec<SymbolId> {
        if matches!(table, TableId::Members(_)) {
            return Vec::new();
        }
        let cached =
            matches!(table, TableId::Globals | TableId::Exports(_) | TableId::ResolvedExports(_));
        if cached && let Some(aliases) = self.accessibility.aliases.get(&table) {
            return aliases.clone();
        }
        let aliases: Vec<SymbolId> = self
            .table_entries(table)
            .into_iter()
            .filter(|&s| self.checker.binder.symbols().get(s).flags.contains(SymbolFlags::ALIAS))
            .collect();
        if cached {
            self.accessibility.aliases.insert(table, aliases.clone());
        }
        aliases
    }

    /// Whether native's `globalThisSymbol` is accessible at `enclosing` with
    /// value meaning: `trySymbolTable` reaches it in `c.globals`, where
    /// `canQualifySymbol` asks `needsQualification` — which stops at the
    /// first table in scope holding the name `globalThis`. Native's own entry
    /// is in `c.globals` and answers "no qualification"; any earlier table
    /// holding a different `globalThis` of value meaning shadows it, and a
    /// parentless `globalThis` then has no qualified route. This port has no
    /// `globalThis` symbol, so the same walk is asked by name.
    pub(crate) fn is_global_this_accessible(&mut self, enclosing: NodeId) -> bool {
        for scope in self.scope_tables(enclosing) {
            if scope.table == TableId::Globals {
                return true;
            }
            let Some(found) = self.table_get(scope.table, "globalThis") else { continue };
            let found = self.checker.binder.merged_symbol(found);
            let entry = self.checker.binder.symbols().get(found);
            let should_resolve_alias = entry.flags.contains(SymbolFlags::ALIAS)
                && !entry
                    .declarations
                    .iter()
                    .any(|&d| self.checker.nodes.kind(d) == K::ExportSpecifier);
            let flags = if should_resolve_alias {
                match self.checker.resolve_alias(found) {
                    Some(target) => self.checker.get_symbol_flags(target),
                    None => SymbolFlags::PROPERTY,
                }
            } else {
                entry.flags
            };
            if flags.intersects(SymbolFlags::VALUE) {
                return false;
            }
        }
        true
    }

    /// `symbolToStringEx(symbol, enclosing, meaning, AllowAnyNodeKind)` for
    /// an accessibility error's names: the accessible chain's names joined by
    /// `.`, else the symbol's own name (`getNameOfSymbolAsWritten`'s
    /// declaration-name arm). The tracker only reports results whose symbol
    /// has no inaccessible *named* container (§2), where the two agree.
    fn symbol_error_name(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> String {
        let chain = self.accessible_symbol_chain(Some(symbol), enclosing, meaning, false);
        if !chain.is_empty() {
            return chain
                .iter()
                .map(|&s| self.symbol_name_as_written(s))
                .collect::<Vec<_>>()
                .join(".");
        }
        self.symbol_name_as_written(symbol)
    }

    /// `getNameOfSymbolAsWritten` (`nodebuilderimpl.go:973`): the first
    /// declaration name's text, else the symbol name.
    fn symbol_name_as_written(&self, symbol: SymbolId) -> String {
        let entry = self.checker.binder.symbols().get(symbol);
        for &declaration in &entry.declarations {
            if let Some(name) = self.checker.declaration_name_of(declaration)
                && let Some(Node::Identifier(identifier)) = self.checker.node_map.get(name)
            {
                return identifier.text.to_string();
            }
        }
        entry.name.to_string()
    }

    /// `symbolToString` of an external module symbol, as
    /// `ErrorModuleName` carries it: the quoted module name — an ambient
    /// module's own name, a file's path without its extension.
    fn module_symbol_to_string(&self, module: SymbolId) -> String {
        let name = self.checker.binder.symbols().get(module).name;
        let bare = tsr_core::strip_quotes(name);
        let bare = bare.strip_prefix('/').unwrap_or(bare);
        format!("\"{bare}\"")
    }
}
