//! A property named by a `unique symbol` type: its name
//! (`getPropertyNameFromType`'s third arm) and its spelling at a print site
//! (`getPropertyNameNodeForSymbolFromNameType`'s `UniqueESSymbol` arm).
//!
//! Pinned tsgo 5b1047d:
//!
//! - `getPropertyNameFromType` (`utilities.go:886`): a
//!   `TypeFlagsUniqueESSymbol` name type names its property
//!   `t.AsUniqueESSymbolType().escapedName`, which `newUniqueESSymbolType`
//!   sets to `InternalSymbolNamePrefix + "@" + symbol.Name + "@" +
//!   getSymbolId(symbol)` (`getESSymbolLikeTypeForNode`, `checker.go:22982`).
//!   tsgo's prefix is `"\xFE"`; this port's internal names keep Strada's
//!   `"__"` (`__export`, and the relater's `__@` tests), so the name is
//!   `__@<name>@<id>`;
//! - `getPropertyNameNodeForSymbolFromNameType` (`nodebuilderimpl.go:2455`):
//!   such a property prints `[symbolToExpression(nameType.symbol, Value)]`;
//! - `symbolToExpression` → `lookupSymbolChain` → `getSymbolChain`
//!   (`nodebuilderimpl.go:1061`): the accessible chain at the site, else the
//!   symbol itself (`endOfChain`).
//!
//! Written, mapped and composite unique-symbol members share `__@name@id`
//! semantic keys. Bracketed expression text is presentation only; aliases of
//! one declaration share its key, while same-spelled declarations do not.
//!
//! The spelling at a site uses the chain API the printer already has
//! ([`Checker::needs_qualification`], [`Checker::best_name`],
//! [`Checker::symbol_chain`]), as `serializeTypeName`'s value arm in
//! `node_reuse.rs` does. Where that API has no chain, native's
//! `endOfChain` answer is the bare name. A route this API cannot find is
//! `tsr-2zk.39`'s.
//!
//! # Checker port convention record (`docs/conventions.md`)
//!
//! No cache, side table or traversal of its own. The unique-symbol type's
//! declaration symbol is read back from `unique_es_symbol_types`
//! (`crate::unique_symbols`' cache), keyed by merged `SymbolId`.
//! Late-binding keeps its existing `(owner, static)` publication boundary:
//! absent, active empty provisional list, completed names/declarations. Active
//! reads cannot publish structured/composite completion. Anonymous images keep
//! semantic `name` separate from `printed_name` and declaration provenance;
//! concrete receiver `this` and alias/mapper substitution remain the consumers'
//! responsibility. The existing expression/type workers perform key forcing;
//! this change adds no cache, member image or reusable graph traversal.

use tsr_ast::{Node, NodeId};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::Checker;
use crate::flags::TypeFlags;
use crate::symbol_access::DeclarationEmitResolver;
use crate::types::TypeId;

/// `"__@"`, the prefix of a unique symbol's escaped name.
const UNIQUE_PREFIX: &str = "__@";

impl Checker<'_, '_> {
    /// Pinned 5b1047d `getPropertyNameForKnownSymbolName` (flow.go:2107):
    /// protocol consumers ask the global Symbol value's member, never a
    /// source site's alias or shadowed Symbol.
    /// Without a usable lib declaration, retain the legacy unsupported spelling.
    pub(crate) fn well_known_symbol_property_name(&mut self, name: &str) -> String {
        let key = self.binder.global("Symbol").and_then(|symbol| {
            let ty = self.get_type_of_symbol(symbol);
            let key = self.get_type_of_property_of_type(ty, name)?;
            self.property_name_from_index(key)
        });
        key.unwrap_or_else(|| format!("[Symbol.{name}]"))
    }

    /// The declaration symbol of a `unique symbol` type
    /// (`t.AsUniqueESSymbolType().symbol`).
    #[must_use]
    pub fn unique_symbol_type_symbol(&self, ty: TypeId) -> Option<SymbolId> {
        if !self.store.get(ty).flags.contains(TypeFlags::UNIQUE_ES_SYMBOL) {
            return None;
        }
        self.unique_es_symbol_types
            .iter()
            .find_map(|(&symbol, &minted)| (minted == ty).then_some(symbol))
    }

    /// `getPropertyNameFromType`'s `UniqueESSymbol` arm: the escaped name
    /// `__@<name>@<symbol id>`.
    #[must_use]
    pub fn unique_symbol_property_name(&self, ty: TypeId) -> Option<String> {
        let symbol = self.unique_symbol_type_symbol(ty)?;
        let name = self.binder.symbols().get(symbol).name;
        Some(format!("{UNIQUE_PREFIX}{name}@{}", symbol.index()))
    }

    /// The symbol a [`Checker::unique_symbol_property_name`] names, read
    /// back from the name (the id it was built from).
    #[must_use]
    pub fn symbol_of_unique_property_name(&self, name: &str) -> Option<SymbolId> {
        let rest = name.strip_prefix(UNIQUE_PREFIX)?;
        let (_, id) = rest.rsplit_once('@')?;
        let id: usize = id.parse().ok()?;
        self.unique_es_symbol_types.keys().copied().find(|symbol| symbol.index() == id)
    }

    /// `[symbolToExpression(symbol, Value)]` as the property name of a
    /// unique-symbol-named member printed at `site`; with no site, the
    /// chain is the symbol itself.
    pub fn unique_symbol_property_name_at(
        &mut self,
        symbol: SymbolId,
        site: Option<NodeId>,
    ) -> String {
        let text = self.binder.symbols().get(symbol).name.to_string();
        let Some(site) = site else { return format!("[{text}]") };
        let meaning = SymbolFlags::VALUE;
        let spelled = if !self.needs_qualification(symbol, &text, site, meaning) {
            text
        } else if let Some(better) = self.best_name(symbol, site).filter(|better| *better != text) {
            better
        } else if self.module_parent_unnamed_at(symbol, site) {
            // `getSymbolChain(parent, …, endOfChain = false,
            // yieldModuleSymbol = false)`: an external-module parent no
            // accessible chain reaches is not written (`nodebuilderimpl.go:1141`),
            // so `symbolToExpression` falls back to the symbol alone.
            text
        } else if let Some(prefix) = self.symbol_chain(symbol, site, meaning, 0) {
            format!("{prefix}{text}")
        } else {
            // The printer's `symbol_chain` qualifies through modules and
            // namespaces only. A member of an interface reached through a
            // variable of its type (`SymbolConstructor.iterator` through
            // `Symbol`) is getSymbolChain's container walk over
            // getWithAlternativeContainers' variable-match arm
            // (`symbolaccessibility.go:137`), the resolver's port.
            self.unique_symbol_container_chain_at(symbol, site).unwrap_or(text)
        };
        format!("[{spelled}]")
    }

    /// `createExpressionFromSymbolChain` (`nodebuilderimpl.go:858`) over the
    /// resolver's getSymbolChain, for a chain of identifier names, which is
    /// the property-access form: `None` when the chain is the symbol alone
    /// or holds a module or a name that needs the element-access form.
    fn unique_symbol_container_chain_at(
        &mut self,
        symbol: SymbolId,
        site: NodeId,
    ) -> Option<String> {
        let chain = DeclarationEmitResolver::new(self).symbol_chain_at(
            symbol,
            site,
            SymbolFlags::VALUE,
            true,
            false,
            0,
        );
        if chain.len() < 2 || chain.last() != Some(&symbol) {
            return None;
        }
        let names: Vec<&str> =
            chain.iter().map(|&link| self.binder.symbols().get(link).name).collect();
        // canUsePropertyAccess (`nodebuilderimpl.go:912`) for every link.
        names
            .iter()
            .all(|name| {
                let mut chars = name.chars();
                chars.next().is_some_and(tsr_scanner::is_identifier_start)
                    && chars.all(tsr_scanner::is_identifier_part)
            })
            .then(|| names.join("."))
    }

    /// Whether `symbol`'s container is an external module
    /// (`hasNonGlobalAugmentationExternalModuleSymbol`) that has no
    /// accessible chain at `site` (`getAccessibleSymbolChain` with namespace
    /// meaning, the resolver's port).
    fn module_parent_unnamed_at(&mut self, symbol: SymbolId, site: NodeId) -> bool {
        let Some(parent) = self.binder.symbols().get(symbol).parent else { return false };
        let external = self.binder.symbols().get(parent).declarations.iter().any(|&declaration| {
            match self.node_map.get(declaration) {
                Some(Node::SourceFile(file)) => tsr_binder::is_external_module(file),
                Some(Node::ModuleDeclaration(module)) => {
                    matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                }
                _ => false,
            }
        });
        external
            && DeclarationEmitResolver::new(self)
                .accessible_symbol_chain(Some(parent), site, SymbolFlags::NAMESPACE, false)
                .is_empty()
    }
}
