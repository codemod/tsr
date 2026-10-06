//! `checkExternalEmitHelpers` (`internal/checker/checker.go:28576`): under
//! `importHelpers`, a construct whose downlevel emit calls a `tslib` helper
//! requires that helper to exist in the `tslib` the file imports.
//!
//! TS2354 when `tslib` itself cannot be found (`resolveHelpersModule`,
//! `checker.go:28673`), TS2343 when it lacks a helper, TS2807 when the
//! private-field helpers are too old.
//!
//! # Ownership and caching (the checker port convention)
//!
//! - **Native operation:** `checkExternalEmitHelpers` +
//!   `resolveHelpersModule`, pinned 5b1047d.
//! - **Key and owner:** the importing `SourceFile` node, in
//!   [`Checker::external_helpers`]: upstream's two `sourceFileLinks` fields,
//!   `externalHelpersModule` and `requestedExternalEmitHelpers`. Per checker,
//!   like every links table here.
//! - **Publication:** the module is published once, on the first request in
//!   the file, `None` standing for `unknownSymbol` (not found, or a resolution
//!   this port declines to describe); the requested mask only grows. A helper
//!   is therefore reported at most once per file, at the first construct
//!   that needs it — upstream's order, because the checker walks a file in
//!   source order.
//! - **Expensive work:** one module resolution per file, and one export
//!   lookup per (file, helper).
//!
//! The program half — the synthetic `tslib` import every eligible file gets
//! (`fileloader.go:543`) — is the loader's; the checker reads its result
//! through [`crate::resolution::ModuleHost::import_helpers_module`].
//!
//! Call sites: upstream has 26 (`checker.go`, one per construct). This module
//! is reached from the names lane's import/export checks; the others belong to
//! other lanes' files and are listed with a measured patch in
//! `docs/parity/notes/names-emit-helpers.diff` (notes §7).

use tsr_ast::{Node, NodeId};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::resolution::ImportHelpersModule;

/// `ExternalEmitHelpers` (`internal/checker/types.go:114`): one bit per
/// helper, in upstream's order, so a mask walks them as upstream does.
#[allow(dead_code)] // the constants other lanes' call sites pass (notes §7)
pub(crate) mod helpers {
    pub(crate) const REST: u32 = 1 << 0;
    pub(crate) const DECORATE: u32 = 1 << 1;
    pub(crate) const METADATA: u32 = 1 << 2;
    pub(crate) const PARAM: u32 = 1 << 3;
    pub(crate) const AWAITER: u32 = 1 << 4;
    pub(crate) const AWAIT: u32 = 1 << 5;
    pub(crate) const ASYNC_GENERATOR: u32 = 1 << 6;
    pub(crate) const ASYNC_DELEGATOR: u32 = 1 << 7;
    pub(crate) const ASYNC_VALUES: u32 = 1 << 8;
    pub(crate) const EXPORT_STAR: u32 = 1 << 9;
    pub(crate) const IMPORT_STAR: u32 = 1 << 10;
    pub(crate) const IMPORT_DEFAULT: u32 = 1 << 11;
    pub(crate) const MAKE_TEMPLATE_OBJECT: u32 = 1 << 12;
    pub(crate) const CLASS_PRIVATE_FIELD_GET: u32 = 1 << 13;
    pub(crate) const CLASS_PRIVATE_FIELD_SET: u32 = 1 << 14;
    pub(crate) const CLASS_PRIVATE_FIELD_IN: u32 = 1 << 15;
    pub(crate) const SET_FUNCTION_NAME: u32 = 1 << 16;
    pub(crate) const PROP_KEY: u32 = 1 << 17;
    pub(crate) const ADD_DISPOSABLE_RESOURCE_AND_DISPOSE_RESOURCES: u32 = 1 << 18;
    pub(crate) const REWRITE_RELATIVE_IMPORT_EXTENSION: u32 = 1 << 19;
    pub(crate) const ES_DECORATE_AND_RUN_INITIALIZERS: u32 = DECORATE;
    pub(crate) const FIRST: u32 = REST;
    pub(crate) const LAST: u32 = REWRITE_RELATIVE_IMPORT_EXTENSION;
    pub(crate) const FOR_AWAIT_OF_INCLUDES: u32 = ASYNC_VALUES;
    pub(crate) const ASYNC_GENERATOR_INCLUDES: u32 = AWAIT | ASYNC_GENERATOR;
    pub(crate) const ASYNC_DELEGATOR_INCLUDES: u32 = AWAIT | ASYNC_DELEGATOR | ASYNC_VALUES;
}

/// `externalHelpersModuleNameText` (`checker.go`).
const EXTERNAL_HELPERS_MODULE_NAME: &str = "tslib";

/// Upstream's two `sourceFileLinks` fields for one file.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExternalHelpersLinks {
    /// Whether `externalHelpersModule` has been resolved.
    resolved: bool,
    /// `externalHelpersModule` once resolved; `None` is `unknownSymbol`.
    module: Option<SymbolId>,
    /// `requestedExternalEmitHelpers`.
    requested: u32,
}

impl Checker<'_, '_> {
    /// `checkExternalEmitHelpers(location, helpers)` (`checker.go:28576`).
    pub(crate) fn check_external_emit_helpers(&mut self, location: NodeId, helpers: u32) {
        if !self.import_helpers {
            return;
        }
        let Some(file) = self.source_file_of(location) else { return };
        // `location.Flags & NodeFlagsAmbient`, through the ancestor walk the
        // unused-declaration checks use (the parser does not set the flag).
        if !self.is_effective_external_module(file) || self.is_in_ambient_context(location) {
            return;
        }
        let Some(module) = self.resolve_helpers_module(file, location) else { return };
        let links = self.external_helpers.get(&file).copied().unwrap_or_default();
        if links.requested & helpers != helpers {
            let unchecked = helpers & !links.requested;
            let mut helper = helpers::FIRST;
            while helper <= helpers::LAST {
                if unchecked & helper != 0 {
                    for &name in self.helper_names(helper) {
                        self.check_external_emit_helper(module, location, helper, name);
                    }
                }
                helper <<= 1;
            }
        }
        self.external_helpers.entry(file).or_default().requested |= helpers;
    }

    /// One name of one helper: TS2343 when `tslib` lacks it, TS2807 when a
    /// private-field helper's arity is too small.
    fn check_external_emit_helper(
        &mut self,
        module: SymbolId,
        location: NodeId,
        helper: u32,
        name: &'static str,
    ) {
        // `resolveSymbol(getSymbol(getExportsOfModule(module), name, Value))`.
        let symbol = self
            .get_export_of_module(module, name)
            .filter(|&symbol| self.get_symbol_flags(symbol).intersects(SymbolFlags::VALUE));
        let report = match symbol {
            None => Some(Diagnostic::with_args(
                &messages::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_1_WHICH_DOES_NOT_EXIST_IN_0_CONSIDER_UPGRADING_YOUR_VERSION_OF_0,
                self.error_span(location),
                [EXTERNAL_HELPERS_MODULE_NAME.to_string(), name.to_string()],
            )),
            Some(symbol) => {
                let required = if helper & helpers::CLASS_PRIVATE_FIELD_GET != 0 {
                    Some(4)
                } else if helper & helpers::CLASS_PRIVATE_FIELD_SET != 0 {
                    Some(5)
                } else {
                    None
                };
                match required {
                    Some(required) if !self.has_signature_with_arity_greater_than(symbol, required - 1) => {
                        Some(Diagnostic::with_args(
                            &messages::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_NAMED_1_WITH_2_PARAMETERS_WHICH_IS_NOT_COMPATIBLE_WITH_THE_ONE_IN_0_CONSIDER_UPGRADING_YOUR_VERSION_OF_0,
                            self.error_span(location),
                            [
                                EXTERNAL_HELPERS_MODULE_NAME.to_string(),
                                name.to_string(),
                                required.to_string(),
                            ],
                        ))
                    }
                    _ => None,
                }
            }
        };
        if let Some(diagnostic) = report
            && let Some(file) = self.source_file_of_for_diagnostics(location)
        {
            self.report(file, diagnostic);
        }
    }

    /// `hasSignatureWithArityGreaterThan` (`checker.go:28614`). The parameter
    /// count is the declared parameter list's length: `getParameterCount`'s
    /// rest-tuple expansion does not arise for a helper declaration.
    fn has_signature_with_arity_greater_than(&mut self, symbol: SymbolId, arity: usize) -> bool {
        let symbol = self.resolve_alias_fully(symbol);
        self.get_signatures_of_symbol(symbol)
            .is_some_and(|signatures| signatures.iter().any(|s| s.parameters.len() > arity))
    }

    /// `getHelperNames` (`checker.go:28623`).
    fn helper_names(&self, helper: u32) -> &'static [&'static str] {
        match helper {
            helpers::REST => &["__rest"],
            helpers::DECORATE if self.legacy_decorators => &["__decorate"],
            helpers::DECORATE => &["__esDecorate", "__runInitializers"],
            helpers::METADATA => &["__metadata"],
            helpers::PARAM => &["__param"],
            helpers::AWAITER => &["__awaiter"],
            helpers::AWAIT => &["__await"],
            helpers::ASYNC_GENERATOR => &["__asyncGenerator"],
            helpers::ASYNC_DELEGATOR => &["__asyncDelegator"],
            helpers::ASYNC_VALUES => &["__asyncValues"],
            helpers::EXPORT_STAR => &["__exportStar"],
            helpers::IMPORT_STAR => &["__importStar"],
            helpers::IMPORT_DEFAULT => &["__importDefault"],
            helpers::MAKE_TEMPLATE_OBJECT => &["__makeTemplateObject"],
            helpers::CLASS_PRIVATE_FIELD_GET => &["__classPrivateFieldGet"],
            helpers::CLASS_PRIVATE_FIELD_SET => &["__classPrivateFieldSet"],
            helpers::CLASS_PRIVATE_FIELD_IN => &["__classPrivateFieldIn"],
            helpers::SET_FUNCTION_NAME => &["__setFunctionName"],
            helpers::PROP_KEY => &["__propKey"],
            helpers::ADD_DISPOSABLE_RESOURCE_AND_DISPOSE_RESOURCES => {
                &["__addDisposableResource", "__disposeResources"]
            }
            helpers::REWRITE_RELATIVE_IMPORT_EXTENSION => &["__rewriteRelativeImportExtension"],
            _ => &[],
        }
    }

    /// `resolveHelpersModule(file, errorNode)` (`checker.go:28673`):
    /// `resolveExternalModule(…, "tslib", TS2354, errorNode)` once per file.
    /// `None` is `unknownSymbol`.
    ///
    /// The resolution is upstream's order: `tryFindAmbientModule`, then the
    /// program's resolution of the synthetic import, then the pattern
    /// ambient modules. Only a resolution that found nothing reports TS2354;
    /// one that named a file the program does not hold (TS7016 and kin
    /// upstream) answers `unknownSymbol` silently, the TS2307 rule's bound.
    fn resolve_helpers_module(&mut self, file: NodeId, error_node: NodeId) -> Option<SymbolId> {
        if let Some(links) = self.external_helpers.get(&file)
            && links.resolved
        {
            return links.module;
        }
        let module = self.resolve_helpers_module_worker(file, error_node);
        let links = self.external_helpers.entry(file).or_default();
        links.resolved = true;
        links.module = module;
        module
    }

    fn resolve_helpers_module_worker(
        &mut self,
        file: NodeId,
        error_node: NodeId,
    ) -> Option<SymbolId> {
        if let Some(ambient) = self.binder.ambient_module(EXTERNAL_HELPERS_MODULE_NAME)
            && self.binder.symbols().get(ambient).flags.intersects(SymbolFlags::VALUE_MODULE)
        {
            return Some(ambient);
        }
        let host = self.module_host?;
        match host.import_helpers_module(file) {
            ImportHelpersModule::File(target) => return self.binder.symbol_of(target),
            ImportHelpersModule::NotRequested | ImportHelpersModule::OutsideProgram => {
                return None;
            }
            ImportHelpersModule::NotFound => {}
        }
        if self.has_pattern_ambient_modules
            && let Some(pattern) = self.binder.pattern_ambient_module(EXTERNAL_HELPERS_MODULE_NAME)
        {
            return Some(pattern);
        }
        if let Some(diagnostic_file) = self.source_file_of_for_diagnostics(error_node) {
            let diagnostic = Diagnostic::with_args(
                &messages::THIS_SYNTAX_REQUIRES_AN_IMPORTED_HELPER_BUT_MODULE_0_CANNOT_BE_FOUND,
                self.error_span(error_node),
                [EXTERNAL_HELPERS_MODULE_NAME.to_string()],
            );
            self.report(diagnostic_file, diagnostic);
        }
        None
    }

    /// `ast.IsEffectiveExternalModule` (`ast/utilities.go:1669`), the
    /// `IsExternalModule` half: the `CommonJSModuleIndicator` half (a JS
    /// file under a CommonJS-containing module kind) reads the binder's
    /// indicator, which is not exposed, so such a JS file declines.
    fn is_effective_external_module(&self, file: NodeId) -> bool {
        matches!(self.node_map.get(file), Some(Node::SourceFile(source))
            if tsr_binder::is_external_module(source))
    }

    /// `GetEmitModuleFormatOfFile` (`program.go`): the file's implied format
    /// for emit, else the emit module kind.
    fn emit_module_format_of_file(&self, file: NodeId) -> tsr_core::ModuleKind {
        match self.module_host.map(|host| host.implied_node_format_for_emit(file)) {
            Some(format) if format != tsr_core::ModuleKind::None => format,
            _ => self.module_kind,
        }
    }

    /// The helper requests of `checkImportDeclaration` (`checker.go:5285-5313`)
    /// and `checkExportDeclaration` (`:5537-5545`), made where both have
    /// passed `checkExternalImportOrExportDeclaration` — the names lane's
    /// `check_module_specifier` position test.
    pub(crate) fn check_declaration_emit_helpers(&mut self, declaration: NodeId) {
        if !self.import_helpers {
            return;
        }
        let Some(file) = self.source_file_of(declaration) else { return };
        let commonjs = self.emit_module_format_of_file(file) == tsr_core::ModuleKind::CommonJS;
        match self.node_map.get(declaration) {
            Some(Node::ImportDeclaration(import)) => {
                let Some(clause) = import.import_clause else { return };
                let mut needs_import_star = false;
                if let Some(tsr_ast::NamedImportBindings::NamespaceImport(_)) =
                    clause.named_bindings
                    && commonjs
                {
                    // import * as ns from "foo";
                    needs_import_star = true;
                    self.check_external_emit_helpers(declaration, helpers::IMPORT_STAR);
                }
                if clause.name.is_some() && !needs_import_star && commonjs {
                    // import d from "foo";
                    self.check_external_emit_helpers(declaration, helpers::IMPORT_DEFAULT);
                }
            }
            Some(Node::ExportDeclaration(export)) => {
                if export.module_specifier.is_none() || !commonjs {
                    return;
                }
                match export.export_clause {
                    // export { x } from "foo": the specifiers' business.
                    Some(tsr_ast::NamedExportBindings::NamedExports(_)) => {}
                    // export * as ns from "foo";
                    Some(tsr_ast::NamedExportBindings::NamespaceExport(_)) => {
                        self.check_external_emit_helpers(declaration, helpers::IMPORT_STAR);
                    }
                    // export * from "foo"
                    None => self.check_external_emit_helpers(declaration, helpers::EXPORT_STAR),
                }
            }
            _ => {}
        }
    }

    /// The `default` specifier requests: `checkImportBinding`'s import
    /// specifier arm (`checker.go:5380-5384`) and `checkExportSpecifier`'s
    /// module-specifier arm (`:5569-5572`), `ModuleExportNameIsDefault` of
    /// `PropertyNameOrName` in a file emitted as `CommonJS`.
    pub(crate) fn check_specifier_default_emit_helper(&mut self, specifier: NodeId) {
        if !self.import_helpers {
            return;
        }
        let name = match self.node_map.get(specifier) {
            Some(Node::ImportSpecifier(node)) => {
                node.property_name.or(node.name.map(tsr_ast::ModuleExportName::Identifier))
            }
            Some(Node::ExportSpecifier(node)) => {
                // `hasModuleSpecifier := node.Parent.Parent.ModuleSpecifier() != nil`.
                let has_module_specifier = matches!(
                    self.nodes
                        .parent(specifier)
                        .and_then(|list| self.nodes.parent(list))
                        .and_then(|declaration| self.node_map.get(declaration)),
                    Some(Node::ExportDeclaration(export)) if export.module_specifier.is_some()
                );
                if !has_module_specifier {
                    return;
                }
                node.property_name.or(node.name)
            }
            _ => return,
        };
        let is_default = match name {
            Some(tsr_ast::ModuleExportName::Identifier(name)) => name.text == "default",
            Some(tsr_ast::ModuleExportName::StringLiteral(name)) => name.text == "default",
            None => false,
        };
        let Some(file) = self.source_file_of(specifier) else { return };
        if is_default && self.emit_module_format_of_file(file) == tsr_core::ModuleKind::CommonJS {
            self.check_external_emit_helpers(specifier, helpers::IMPORT_DEFAULT);
        }
    }
}
