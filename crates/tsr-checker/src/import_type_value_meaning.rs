//! `getTypeFromImportTypeNode` (`checker.go:24575`), the VALUE-meaning arm:
//! `typeof import("m")`, `typeof import("m").a.b`, and either with written
//! type arguments (`typeof import("m").f<A>`).
//!
//! # The native operation
//!
//! `targetMeaning` is `SymbolFlagsValue` when `n.IsTypeOf`. The module
//! resolves through `resolveExternalModuleSymbol(inner, false)`, so a module
//! that writes `export = X` is `X`, with its alias resolved.
//!
//! - **Unqualified** (`:24638`): when `getSymbolFlags(moduleSymbol)` has
//!   Value meaning, `resolveImportSymbolType(node, moduleSymbol, Value)`.
//!   Otherwise TS2497-class error and `errorType`.
//! - **Qualified** (`:24593-24635`): each identifier of the qualifier is read
//!   as a PROPERTY of the previous symbol's type,
//!   `getPropertyOfTypeEx(getTypeOfSymbol(getMergedSymbol(resolveSymbol(ns))),
//!   name, false, true)`, not through its exports table. This is the
//!   difference from the type-meaning walk, as the native comment there says
//!   (`typeof a.b.c` is `checkQualifiedName`'s property lookup). A miss is
//!   TS2694 and `errorType`.
//! - `resolveImportSymbolType` (`:24653`) with Value meaning answers
//!   `getInstantiationExpressionType(getTypeOfSymbol(symbol), node)`, on the
//!   UNRESOLVED symbol, so an alias's type is the alias's own.
//!
//! # What this port answers
//!
//! The type, or `None` where this port cannot follow upstream (an alias this
//! port does not resolve, a property miss, a module without Value meaning).
//! `None` keeps the caller's gap. The miss diagnostics (TS2694, TS2497-class)
//! are not this function's: they are reported by the checking pass over the
//! node (`check.rs`, `checkImportType`), and this function only computes.
//!
//! # Checker port boundaries (`docs/conventions.md`)
//!
//! - **Native operation:** `getTypeFromImportTypeNode`'s `IsTypeOf` arm, pinned
//!   at `5b1047d` (`checker.go:24575-24662`).
//! - **Key identity and owner:** no new cache. The answer is published through
//!   the caller's existing `typeNodeLinks` equivalent (the type-node cache in
//!   `get_type_from_type_node`); instantiation goes through
//!   `get_instantiation_expression_type`, whose cache is keyed by the node.
//! - **Publication states:** none of its own. A `None` publishes nothing.
//! - **Receiver/alias context:** none. The qualifier walk reads
//!   `getTypeOfSymbol` only, like native.
//! - **Work boundary:** one module resolution, one property lookup per
//!   qualifier segment, and one `getTypeOfSymbol` per segment.
//!
//! `docs/parity/notes/r6-typesroots2.md` §3.

use tsr_binder::SymbolFlags;

use crate::checker::Checker;
use crate::types::TypeId;

impl<'a> Checker<'a, '_> {
    /// The type of a `typeof import(…)` node, or `None` where this port
    /// does not answer (the caller keeps its gap).
    #[expect(dead_code, reason = "consumer: r6-typesroots2-import-type-value-meaning.diff")]
    pub(crate) fn import_type_value_meaning(
        &mut self,
        node: &tsr_ast::ImportTypeNode<'a>,
    ) -> Option<TypeId> {
        if !node.is_type_of {
            return None;
        }
        let site = node.node_id?;
        let tsr_ast::TypeNode::LiteralTypeNode(literal) = node.argument? else { return None };
        let specifier = literal.literal.and_then(|l| l.node_id())?;
        let inner = self.resolve_external_module_name(site, specifier)?;
        // resolveExternalModuleSymbol(inner, dontResolveAlias = false).
        let module = self.import_type_resolve_symbol(self.resolve_external_module_symbol(inner))?;
        let symbol = match node.qualifier {
            None => {
                if !self.get_symbol_flags(module).intersects(SymbolFlags::VALUE) {
                    return None;
                }
                module
            }
            Some(qualifier) => {
                let mut current = module;
                for name in identifier_chain(qualifier)? {
                    let resolved =
                        self.binder.merged_symbol(self.import_type_resolve_symbol(current)?);
                    let ty = self.get_type_of_symbol(resolved);
                    if self.is_gap(ty) {
                        return None;
                    }
                    current = self.get_property_of_type(ty, name)?;
                }
                current
            }
        };
        let ty = self.get_type_of_symbol(symbol);
        if self.is_gap(ty) {
            return None;
        }
        let answer = self.get_instantiation_expression_type(ty, site);
        (!self.is_gap(answer)).then_some(answer)
    }

    /// `resolveSymbol` (`checker.go:14361`, `dontResolveAlias = false`): an
    /// alias is its target. `None` where this port does not resolve it.
    fn import_type_resolve_symbol(
        &mut self,
        symbol: tsr_binder::SymbolId,
    ) -> Option<tsr_binder::SymbolId> {
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            return self.resolve_alias(symbol);
        }
        Some(symbol)
    }
}

/// `getIdentifierChain` (`checker.go:24646`): the qualifier's identifiers,
/// leftmost first.
fn identifier_chain(name: tsr_ast::EntityName<'_>) -> Option<Vec<&str>> {
    match name {
        tsr_ast::EntityName::Identifier(identifier) => Some(vec![identifier.text]),
        tsr_ast::EntityName::QualifiedName(qualified) => {
            let mut chain = identifier_chain(qualified.left?)?;
            chain.push(qualified.right?.text);
            Some(chain)
        }
    }
}
