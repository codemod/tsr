//! The text of a `ModuleExportName`, identifier or string literal.
//!
//! ES2022 lets an import or export specifier name the export with a string
//! literal — `export { x as "<X>" }`, `import { "<X>" as y }`. Upstream never
//! distinguishes the two spellings once it has the name: every reader goes
//! through `Node.Text()` (`internal/ast/ast.go`), and the binder keys the
//! module's `exports` table by the same text (`getDeclarationName`,
//! `internal/binder/binder.go`; here `export_name` in
//! `crates/tsr-binder/src/binder.rs`). So the checker's lookups must too, or a
//! string-literal export is bound under a key no lookup can reach. See
//! `docs/parity/notes/r5-modexports.md` §1.

/// `name.Text()` for a `ModuleExportName` (`internal/ast/ast.go`, `Node.Text`):
/// the identifier's text or the string literal's cooked value. The binder keys
/// `exports` by exactly this text.
pub(crate) fn module_export_name_text(name: tsr_ast::ModuleExportName<'_>) -> &str {
    match name {
        tsr_ast::ModuleExportName::Identifier(name) => name.text,
        tsr_ast::ModuleExportName::StringLiteral(name) => name.text,
    }
}

/// `ast.ModuleExportNameIsDefault` (`internal/ast/utilities.go:2539`):
/// `node.Text() == InternalSymbolNameDefault`, so `{ "default" as d }` is the
/// default import exactly as `{ default as d }` is.
pub(crate) fn module_export_name_is_default(name: tsr_ast::ModuleExportName<'_>) -> bool {
    module_export_name_text(name) == "default"
}

/// Which of native's two synthetic-default caches a type lives in
/// (`CachedTypeKindDefaultOnlyType` / `CachedTypeKindSyntheticType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SyntheticDefaultKind {
    /// `getTypeWithSyntheticDefaultOnly` (`checker.go:15632`).
    DefaultOnly,
    /// `getTypeWithSyntheticDefaultImportType` (`checker.go:15646`).
    SyntheticImport,
}

impl crate::Checker<'_, '_> {
    /// `getTypeWithSyntheticDefaultOnly` (`checker.go:15632`): a module that
    /// is only importable as a default — a JSON module (or `.d.json.ts`)
    /// imported with ES syntax under `node16`..`nodenext`
    /// (`isOnlyImportableAsDefault`, `checker.go:14800`) — reads as
    /// `{ default: T }` for a namespace import or a dynamic `import()`.
    ///
    /// `symbol` is the resolved module (`resolveExternalModuleSymbol`),
    /// `specifier` the module specifier of the usage. `None` is native's `nil`:
    /// the module is importable some other way, or its type is an error.
    ///
    /// Convention record (`docs/conventions.md`): pinned native operation
    /// `getTypeWithSyntheticDefaultOnly`; key the module value's `TypeId`
    /// (native `typeId: t.id`), owned by
    /// [`crate::Checker::synthetic_default_types`]; published once, complete,
    /// never revised; receiver context none (the wrapper depends on the value
    /// type alone — the usage only decides whether it is asked for); the
    /// expensive work is `get_type_of_symbol` of the module, which the
    /// `isOnlyImportableAsDefault` test gates, so ordinary imports pay one
    /// module-kind comparison.
    pub(crate) fn get_type_with_synthetic_default_only(
        &mut self,
        symbol: tsr_binder::SymbolId,
        original: tsr_binder::SymbolId,
        specifier: tsr_ast::NodeId,
    ) -> Option<crate::types::TypeId> {
        if !self.is_only_importable_as_default(original, specifier) {
            return None;
        }
        let value = self.get_type_of_symbol(symbol);
        if self.is_error(value) {
            return None;
        }
        let key = (SyntheticDefaultKind::DefaultOnly, value);
        if let Some(&cached) = self.synthetic_default_types.get(&key) {
            return Some(cached);
        }
        let wrapper = self.create_default_property_wrapper_for_module(value);
        self.synthetic_default_types.insert(key, wrapper);
        Some(wrapper)
    }

    /// `getTypeWithSyntheticDefaultImportType` (`checker.go:15646`): when the
    /// module can have a synthetic default for this usage
    /// (`canHaveSyntheticDefault`, `checker.go:14818`), the module's type
    /// spread with `{ default: <the module> }`, so a real `default` member is
    /// overridden — per emit, `__importStar` wraps a module with no
    /// `__esModule` marker. Otherwise `value` itself.
    ///
    /// Native's wrapper here is owned by a `TypeLiteral` symbol rather than
    /// an object literal one; the spread result is the same anonymous object
    /// either way. Convention record: as
    /// [`Self::get_type_with_synthetic_default_only`], under
    /// [`SyntheticDefaultKind::SyntheticImport`]. Native caches by the value
    /// type alone although the answer depends on the usage too; that quirk
    /// is kept (`docs/parity/notes/r5-modexports.md` §3).
    pub(crate) fn get_type_with_synthetic_default_import_type(
        &mut self,
        value: crate::types::TypeId,
        original: tsr_binder::SymbolId,
        specifier: tsr_ast::NodeId,
    ) -> crate::types::TypeId {
        if self.is_error(value) {
            return value;
        }
        let key = (SyntheticDefaultKind::SyntheticImport, value);
        if let Some(&cached) = self.synthetic_default_types.get(&key) {
            return cached;
        }
        let synthetic = if self.can_have_synthetic_default_for_usage(original, specifier) {
            let wrapper = self.create_default_property_wrapper_for_module(value);
            if self.is_valid_spread_type(value) {
                self.get_spread_type(value, wrapper, None, false)
            } else {
                wrapper
            }
        } else {
            value
        };
        self.synthetic_default_types.insert(key, synthetic);
        synthetic
    }

    /// `checkImportCallExpression`'s module type (`checker.go:8305`-`:8310`)
    /// after the `getTypeWithSyntheticDefaultOnly` arm:
    /// `getTypeWithSyntheticDefaultImportType(getTypeOfSymbol(esModuleSymbol), …)`
    /// with `esModuleSymbol = resolveExternalModuleSymbol(module)`.
    ///
    /// `namespace` is the caller's mint for the module object, which stands
    /// for the module symbol's own type. A module with `export =` resolves
    /// to its target, whose type is asked for directly; this replaces
    /// r5-modexports §3's decline, which skipped such modules.
    pub(crate) fn import_call_module_type(
        &mut self,
        module: tsr_binder::SymbolId,
        namespace: crate::types::TypeId,
        specifier: tsr_ast::NodeId,
    ) -> crate::types::TypeId {
        let es_module = self.resolve_external_module_symbol(module);
        let value =
            if es_module == module { namespace } else { self.get_type_of_symbol(es_module) };
        self.get_type_with_synthetic_default_import_type(value, module, specifier)
    }

    /// `createDefaultPropertyWrapperForModule` (`checker.go:15707`): an
    /// anonymous object whose one member is `default`, typed as the module.
    ///
    /// Native's member is a fresh alias symbol whose `aliasTarget` is the
    /// module; its type is the module's type. This port's checker cannot mint
    /// symbols into the binder's store, so the member is an
    /// [`crate::objects::AnonymousProperty`] carrying that type directly —
    /// the representation `getRestType`'s anonymous objects already use
    /// ([`crate::Checker::mint_rest_properties`]), which property access,
    /// enumeration and printing read without a symbol.
    fn create_default_property_wrapper_for_module(
        &mut self,
        value: crate::types::TypeId,
    ) -> crate::types::TypeId {
        let printed = self.type_to_string(value);
        let default = crate::objects::AnonymousProperty {
            accessor_write: None,
            method: false,
            origin: None,
            checked_declaration: None,
            name: "default".to_owned(),
            printed_name: "default".to_owned(),
            printed_slot: crate::objects::PrintedSlot::printed(printed),
            optional: false,
            readonly: false,
            slot: crate::objects::PropertySlot::resolved(value),
        };
        self.mint_rest_properties(vec![default], Vec::new())
    }
}
