//! `getTypeFromImportTypeNode` (`checker.go:24575`), the UNQUALIFIED arm with
//! type meaning: `import("./foo")` written as a type, with no qualifier and no
//! `typeof`.
//!
//! The module resolves through `resolveExternalModuleSymbol`, so a module
//! that writes `export = Conn` is `Conn`. When that symbol has type meaning,
//! `resolveImportSymbolType` answers `getTypeReferenceType(node,
//! resolveSymbol(symbol))`: for a class, its instance type
//! (`declarationImportTypeAliasInferredAndEmittable`: `type Conn =
//! import("./foo")` makes `declare var x: Conn` the class `Conn`, which the
//! node builder spells `import("./foo")` where `Conn` is not in scope).
//! A module without type meaning is TS2709's error and is not answered here.
//!
//! Scope, stated: non-generic targets only. A generic class or interface
//! reached unqualified needs `getTypeReferenceType`'s arity window over the
//! import node's written arguments, which this port's reference road does not
//! take for an `ImportTypeNode`; it keeps the gap.
//! `docs/parity/notes/r6-typesroots.md` §8.

use tsr_binder::SymbolFlags;

use crate::checker::Checker;
use crate::types::TypeId;

impl<'a> Checker<'a, '_> {
    /// The type of an unqualified, non-`typeof` import type node, or `None`
    /// where this port does not answer (the caller keeps its gap).
    #[expect(dead_code, reason = "consumer: r6-typesroots-import-type-meaning.diff")]
    pub(crate) fn unqualified_import_type_meaning(
        &mut self,
        node: &tsr_ast::ImportTypeNode<'a>,
    ) -> Option<TypeId> {
        if node.is_type_of || node.qualifier.is_some() || !node.type_arguments.is_empty() {
            return None;
        }
        let site = node.node_id?;
        let tsr_ast::TypeNode::LiteralTypeNode(literal) = node.argument? else { return None };
        let specifier = literal.literal.and_then(|l| l.node_id())?;
        let inner = self.resolve_external_module_name(site, specifier)?;
        let module = self.resolve_external_module_symbol(inner);
        // resolveImportSymbolType: getTypeReferenceType wants the resolved
        // symbol.
        let resolved = self.binder.merged_symbol(self.resolve_alias_fully(module));
        let flags = self.binder.symbols().get(resolved).flags;
        if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE | SymbolFlags::ENUM) {
            return None;
        }
        if !self.local_type_parameters_of(resolved).is_empty() {
            return None;
        }
        let declared = self.get_declared_type_of_symbol(resolved);
        (!self.is_gap(declared)).then(|| self.get_regular_type_of_literal_type(declared))
    }

    /// The instance-side twin of `export_equals_class_text_at`
    /// (`printing.rs`): `symbolToTypeNode` for a class that a module exports
    /// as `export =`, at a site where no accessible chain names it
    /// (`getAccessibleSymbolChain` fails, and `getContainersOfSymbol` offers
    /// the module, `symbolaccessibility.go:280`), writes the module as an
    /// import type with no qualifier: `import("./foo")`
    /// (`nodebuilderimpl.go:1249`, `getSpecifierForModuleSymbol`).
    ///
    /// Non-generic classes only: a reference's arguments would follow the
    /// specifier, and this port has no instance of that in reach.
    #[expect(dead_code, reason = "consumer: r6-typesroots-import-type-meaning.diff")]
    pub(crate) fn export_equals_class_instance_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let crate::types::TypeData::Named { members: Some(symbol), .. } = self.store.get(id).data
        else {
            return None;
        };
        let symbol = self.binder.merged_symbol(symbol);
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS)
            || !self.local_type_parameters_of(symbol).is_empty()
            || self.declared_types.get(&symbol) != Some(&id)
        {
            return None;
        }
        let declarations = self.binder.symbols().get(symbol).declarations.to_vec();
        let module = declarations.into_iter().find_map(|declaration| {
            let file = self.source_file_of_for_diagnostics(declaration)?;
            if self.nodes.parent(declaration)? != file {
                return None;
            }
            let module = self.binder.symbol_of(file)?;
            let exported = self.resolve_external_module_symbol(module);
            (exported != module
                && self.binder.merged_symbol(self.resolve_alias_fully(exported)) == symbol)
                .then_some(module)
        })?;
        let enclosing = self.nodes.parent(reference).unwrap_or(reference);
        if self.best_name(symbol, enclosing).is_some() {
            return None;
        }
        let specifier = self.module_specifier_for_symbol(module, reference)?;
        Some(format!("import({specifier})"))
    }
}
