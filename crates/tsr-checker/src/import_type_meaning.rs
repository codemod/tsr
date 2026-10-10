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
//! A generic class or interface takes the import node's written type
//! arguments (`getTypeReferenceType` → `getTypeFromClassOrInterfaceReference`
//! reads `node.TypeArguments()`, which an `ImportTypeNode` has): `import("./foo")
//! <{ x: number }>` over `export = Point` (`interface Point<T>`) is
//! `Point<{ x: number }>`. Scope, stated: a written count equal to the
//! parameter count only; a shorter list that defaults would fill, and the
//! arity-error window, keep the gap. `docs/parity/notes/r6-typesroots.md` §8,
//! `r6-typesroots3.md` §5.

use tsr_binder::SymbolFlags;

use crate::checker::Checker;
use crate::types::TypeId;

impl<'a> Checker<'a, '_> {
    /// The type of an unqualified, non-`typeof` import type node, or `None`
    /// where this port does not answer (the caller keeps its gap).
    pub(crate) fn unqualified_import_type_meaning(
        &mut self,
        node: &tsr_ast::ImportTypeNode<'a>,
    ) -> Option<TypeId> {
        if node.is_type_of || node.qualifier.is_some() {
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
        let parameters = self.local_type_parameters_of(resolved).len();
        if parameters != node.type_arguments.len() {
            return None;
        }
        if parameters != 0 {
            if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
                return None;
            }
            let arguments: Vec<TypeId> = node
                .type_arguments
                .iter()
                .map(|&argument| self.get_type_from_type_node(argument))
                .collect();
            if arguments.iter().any(|&argument| self.is_gap(argument)) {
                return None;
            }
            return Some(self.create_type_reference(resolved, arguments));
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
    /// A class or an interface, the two `getTypeReferenceType` targets whose
    /// instance prints through `symbolToTypeNode`. A generic one's reference
    /// prints its type arguments after the specifier, the import type node's
    /// own `typeArguments` (`nodebuilderimpl.go:1249` builds
    /// `import("…")<…>` from the last chain symbol's arguments): `import(
    /// "./foo")<{ x: number; }>` (importTypeGenericTypes).
    pub(crate) fn export_equals_class_instance_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let (symbol, arguments) = match self.store.get(id).data {
            crate::types::TypeData::Named { members: Some(symbol), .. } => {
                let symbol = self.binder.merged_symbol(symbol);
                if self.declared_types.get(&symbol) == Some(&id) {
                    (symbol, Vec::new())
                } else {
                    let (target, arguments) = self.type_reference_targets.get(&id).cloned()?;
                    (self.binder.merged_symbol(target) == symbol).then_some((symbol, arguments))?
                }
            }
            _ => return None,
        };
        if !self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            || self.local_type_parameters_of(symbol).len() != arguments.len()
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
        if arguments.is_empty() {
            return Some(format!("import({specifier})"));
        }
        let mut printed = Vec::with_capacity(arguments.len());
        for argument in arguments {
            printed.push(self.type_to_string_at(argument, reference)?);
        }
        Some(format!("import({specifier})<{}>", printed.join(", ")))
    }
}
