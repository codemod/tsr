//! Whether a type's alias, or an entity name the node builder re-spells, can
//! be named at the print site: `IsTypeSymbolAccessible` and
//! `IsSymbolAccessible` as the **type printer** asks them.
//!
//! Pinned tsgo 5b1047d:
//!
//! - the node builder's alias arm, `typeToTypeNodeHelper`
//!   (`nodebuilderimpl.go:3362`): a type with `t.alias` prints `Alias<Args>`
//!   when `UseAliasDefinedOutsideCurrentScope` is set or
//!   `IsTypeSymbolAccessible(t.alias.Symbol(), ctx.enclosingDeclaration)`
//!   holds; otherwise it falls through to its structure. The `.types`
//!   baseline never sets that flag (`type_symbol_baseline.go:395`);
//! - `serializeTypeName` (`nodebuilderimpl.go:436`), the reuse visitor's
//!   recovery for a written entity name that does not survive the move to
//!   the print site (`tryVisitTypeReference`, `nodecopy.go:442`):
//!   `IsSymbolAccessible(symbol, ctx.enclosingDeclaration, meaning, false)`
//!   must answer `Accessible`, or the written reference is serialized from
//!   its type;
//! - `IsTypeSymbolAccessible` / `IsValueSymbolAccessible` /
//!   `IsSymbolAccessible` (`symbolaccessibility.go:11`, `:16`, `:839`): all
//!   `isSymbolAccessibleWorker` with `shouldComputeAliasesToMakeVisible =
//!   false` and `allowModules = true`.
//!
//! # Why this is not a new walk
//!
//! The walk under these questions (`IsAnySymbolAccessible`,
//! `getAccessibleSymbolChain`, `getContainersOfSymbol`,
//! `hasVisibleDeclarations`) is already ported whole for the declaration
//! emitter's tracker ([`crate::symbol_accessibility`],
//! `docs/parity/notes/r5-declemit3.md` §2), with its declines listed there.
//! This file asks it from the type printer and adds nothing to it. The chain
//! walk is `tsr-2zk.39`'s (ADR-0044 step 2): a line whose answer needs more of
//! the chain than that port has is routed there
//! (`docs/parity/notes/r6-accessible.md` §3).
//!
//! The resolver's `isVisible` links are native's `EmitResolver` links. With
//! `shouldComputeAliasesToMakeVisible = false`, `hasVisibleDeclarations`
//! paints nothing (`addVisibleAlias` is a no-op), so a resolver built for one
//! question and dropped answers what native's long-lived one answers.
//!
//! # Checker port convention record (`docs/conventions.md`)
//!
//! - **Native operation**: `isSymbolAccessibleWorker(symbol, enclosing,
//!   meaning, false, true).Accessibility == SymbolAccessibilityAccessible`.
//! - **Key identity and owner**: no new cache. Native caches the chains under
//!   the verdict (`symbolContainerLinks.accessibleChainCache`) and never the
//!   verdict itself. Here the chains are cached by the resolver built for the
//!   query ([`crate::symbol_accessibility`]'s `AccessibilityCache`, keyed as
//!   native's: symbol, external-aliasing flag, first scope location,
//!   meaning) and dropped with it. A verdict cache keyed (alias symbol ×
//!   enclosing declaration × meaning) on the checker was built and removed:
//!   it needs a `Checker` field (main's `checker.rs`), and the walk runs only
//!   when a type is printed, so there is no measured cost to pay down
//!   (`r6-accessible.md` §2).
//! - **Publication states**: none published. Alias resolution forced by the
//!   walk is the checker's own memo (`resolve_alias`), with its own states.
//! - **Receiver/alias context**: only the enclosing declaration. A type's
//!   alias *arguments* never enter the question; native asks of the symbol.
//! - **Expensive work**: the scope-table chain walk and the alias resolution
//!   it forces, once per printed alias or refused entity name. A refusal's
//!   error-name strings are computed by the shared worker and discarded.

use tsr_ast::NodeId;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::Checker;
use crate::symbol_access::DeclarationEmitResolver;
use crate::types::TypeId;

impl Checker<'_, '_> {
    /// `IsTypeSymbolAccessible(symbol, enclosing)` (`symbolaccessibility.go:11`):
    /// whether the node builder's alias arm (`nodebuilderimpl.go:3362`) may
    /// print `symbol`'s name at `enclosing`.
    pub fn is_type_symbol_accessible_at(&mut self, symbol: SymbolId, enclosing: NodeId) -> bool {
        DeclarationEmitResolver::new(self).is_type_symbol_accessible(symbol, enclosing)
    }

    /// `IsSymbolAccessible(symbol, enclosing, meaning, false).Accessibility ==
    /// Accessible` (`symbolaccessibility.go:839`), as `serializeTypeName`
    /// (`nodebuilderimpl.go:436`) asks it. Its `meaning` is `SymbolFlagsValue`
    /// for a `typeof` query and `SymbolFlagsType` otherwise, which are
    /// `IsValueSymbolAccessible` and `IsTypeSymbolAccessible` (`:16`, `:11`).
    pub fn is_symbol_accessible_at(
        &mut self,
        symbol: SymbolId,
        enclosing: NodeId,
        meaning: SymbolFlags,
    ) -> bool {
        let mut resolver = DeclarationEmitResolver::new(self);
        if meaning == SymbolFlags::VALUE {
            resolver.is_value_symbol_accessible(symbol, enclosing)
        } else {
            resolver.is_type_symbol_accessible(symbol, enclosing)
        }
    }

    /// The node builder's alias arm (`nodebuilderimpl.go:3362`) for a type
    /// whose alias this port records: ADR-0045's `alias_of` attribute, else a
    /// type-alias reference in `type_reference_targets` (a generic alias's
    /// instantiation, keyed by the alias symbol and its arguments). `true`
    /// when the type has an alias and `IsTypeSymbolAccessible` holds at
    /// `enclosing`, so `Alias<Args>` is printed; `false` when it has none or
    /// the alias cannot be named there, and the type prints its structure.
    pub fn prints_accessible_alias(&mut self, id: TypeId, enclosing: NodeId) -> bool {
        let alias = match self.alias_of.get(&id) {
            Some(&(alias, _)) => Some(alias),
            None => {
                self.type_reference_targets.get(&id).map(|&(target, _)| target).filter(|&target| {
                    self.binder.symbols().get(target).flags.contains(SymbolFlags::TYPE_ALIAS)
                })
            }
        };
        alias.is_some_and(|alias| self.is_type_symbol_accessible_at(alias, enclosing))
    }
}
