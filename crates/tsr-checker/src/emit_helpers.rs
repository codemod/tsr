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
//! Call sites: upstream has 26 (`checker.go`, one per construct). The
//! import/export ones are reached from the names lane's checks; the
//! construct-level ones from [`Checker::check_construct_emit_helpers`], one
//! call at the top of the check walk (`docs/parity/notes/r4-helpers.md`).
//! The `@importHelpers` harness directive is a patch outside this lane:
//! `docs/parity/notes/r4-helpers-harness.diff`.

use tsr_ast::{Node, NodeFlags, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_core::ScriptTarget;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::expressions::AssignmentTargetKind;
use crate::resolution::ImportHelpersModule;

/// `ExternalEmitHelpers` (`internal/checker/types.go:114`): one bit per
/// helper, in upstream's order, so a mask walks them as upstream does.
#[allow(dead_code)] // the constants of call sites not yet ported (r4-helpers notes)
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

impl Checker<'_, '_> {
    /// The construct-level `checkExternalEmitHelpers` requests, one arm per
    /// upstream call site, each under the condition its owner tests. Called
    /// once per node at the top of the check walk (`check_node`); the walk is
    /// pre-order in source order, which is the order upstream's
    /// `checkSourceElement` reaches these nodes, so the first request in a
    /// file — the one a missing helper is reported at — is upstream's.
    pub(crate) fn check_construct_emit_helpers(&mut self, node: NodeId, typed: Node<'_>) {
        if !self.import_helpers {
            return;
        }
        self.check_signature_emit_helpers(node, typed);
        match typed {
            Node::VariableDeclarationList(_) => {
                self.check_variable_declaration_list_emit_helpers(node);
            }
            Node::BindingElement(element) => {
                self.check_binding_element_emit_helpers(node, element);
            }
            Node::PropertyAccessExpression(access) => {
                if matches!(access.name, Some(tsr_ast::MemberName::PrivateIdentifier(_))) {
                    let left = access.expression.and_then(|left| left.node_id());
                    self.check_private_access_emit_helpers(node, left);
                }
            }
            Node::BinaryExpression(binary) => {
                // `checkInExpression` (`checker.go:13081-13086`): `#x in o`,
                // after both operands are checked.
                if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::InKeyword)
                    && let Some(tsr_ast::Expression::PrivateIdentifier(left)) = binary.left
                    && let Some(left) = left.node_id
                    && self.will_transform_private_names()
                {
                    self.request_operand_emit_helpers_first(
                        binary.right.and_then(|right| right.node_id()),
                    );
                    self.check_external_emit_helpers(left, helpers::CLASS_PRIVATE_FIELD_IN);
                }
            }
            Node::ForInOrOfStatement(statement) => {
                if statement.await_modifier.is_some() {
                    self.check_for_await_emit_helpers(node);
                }
            }
            Node::YieldExpression(expression) => {
                if expression.asterisk_token.is_some() {
                    let operand = expression.expression.and_then(|operand| operand.node_id());
                    self.check_yield_star_emit_helpers(node, operand);
                }
            }
            Node::SpreadAssignment(_) => self.check_destructuring_rest_emit_helpers(node),
            _ => {}
        }
        self.check_decorators_emit_helpers(typed);
    }

    /// `checkVariableLikeDeclaration`'s binding-element arm
    /// (`checker.go:5821`): an object rest element before ES2018.
    fn check_binding_element_emit_helpers(
        &mut self,
        node: NodeId,
        element: &tsr_ast::BindingElement<'_>,
    ) {
        if element.dot_dot_dot_token.is_some()
            && element.name.is_some()
            && self
                .nodes
                .parent(node)
                .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::ObjectBindingPattern)
            && self.language_version < ScriptTarget::ES2018
        {
            self.check_external_emit_helpers(node, helpers::REST);
        }
    }

    /// `checkPropertyAccessExpressionOrQualifiedName`'s private-name requests
    /// (`checker.go:11268-11278`): a write asks for `__classPrivateFieldSet`,
    /// a read for `__classPrivateFieldGet`, a compound assignment for both.
    fn check_private_access_emit_helpers(&mut self, node: NodeId, left: Option<NodeId>) {
        if !self.will_transform_private_names() {
            return;
        }
        self.request_operand_emit_helpers_first(left);
        let kind = self.assignment_target_kind(node);
        if kind != AssignmentTargetKind::None {
            self.check_external_emit_helpers(node, helpers::CLASS_PRIVATE_FIELD_SET);
        }
        if kind != AssignmentTargetKind::Definite {
            self.check_external_emit_helpers(node, helpers::CLASS_PRIVATE_FIELD_GET);
        }
    }

    /// The condition both private-name sites test: private names are
    /// downleveled before ES2022, before `ESNext` (decorators), or when class
    /// fields are not `[[Define]]` (`GetUseDefineForClassFields`, which
    /// `standard_class_fields` holds).
    fn will_transform_private_names(&self) -> bool {
        self.language_version < ScriptTarget::ES2022
            || self.language_version < ScriptTarget::ESNext
            || !self.standard_class_fields
    }

    /// `checkSignatureDeclaration`'s requests (`checker.go:2731-2741`), on
    /// `GetFunctionFlags` (`ast/functionflags.go:13`): a generator only for
    /// the three kinds that can carry `*`, async by syntactic modifier, and
    /// invalid without a body.
    fn check_signature_emit_helpers(&mut self, node: NodeId, typed: Node<'_>) {
        let Some(flags) = function_flags(typed) else { return };
        if !flags.valid || !flags.is_async {
            return;
        }
        if flags.generator && self.language_version < ScriptTarget::ES2018 {
            self.check_external_emit_helpers(node, helpers::ASYNC_GENERATOR_INCLUDES);
        }
        if !flags.generator && self.language_version < ScriptTarget::ES2017 {
            self.check_external_emit_helpers(node, helpers::AWAITER);
        }
    }

    /// `checkForOfStatement`'s request (`checker.go:4036-4045`): `for await`
    /// in an async function or async generator before ES2018. Upstream makes
    /// it before checking the statement's parts, as this pre-order walk does.
    fn check_for_await_emit_helpers(&mut self, node: NodeId) {
        // `getContainingFunctionOrClassStaticBlock`.
        let container =
            self.nodes.ancestors(node).find(|&a| self.is_function_like_or_static_block(a));
        let Some(container) = container else { return };
        if self.nodes.kind(container) == SyntaxKind::ClassStaticBlockDeclaration {
            return;
        }
        let flags = self.node_map.get(container).and_then(function_flags);
        if flags.is_some_and(|flags| flags.valid && flags.is_async)
            && self.language_version < ScriptTarget::ES2018
        {
            self.check_external_emit_helpers(node, helpers::FOR_AWAIT_OF_INCLUDES);
        }
    }

    /// `checkYieldExpression`'s `yield*` request (`checker.go:10971-10978`),
    /// made after the operand is checked: in an async generator before
    /// ES2018.
    fn check_yield_star_emit_helpers(&mut self, node: NodeId, operand: Option<NodeId>) {
        // `ast.GetContainingFunction`.
        let Some(function) = self.containing_function(node) else { return };
        let Some(flags) = self.node_map.get(function).and_then(function_flags) else { return };
        if !flags.generator || !flags.is_async || self.language_version >= ScriptTarget::ES2018 {
            return;
        }
        self.request_operand_emit_helpers_first(operand);
        self.check_external_emit_helpers(node, helpers::ASYNC_DELEGATOR_INCLUDES);
    }

    /// `checkObjectLiteralDestructuringPropertyAssignment`'s rest request
    /// (`checker.go:12619-12626`): `{ ...rest } = value` before ES2018, only
    /// for a spread in last position (a misplaced one is an error and
    /// requests nothing).
    fn check_destructuring_rest_emit_helpers(&mut self, node: NodeId) {
        if self.language_version >= ScriptTarget::ES2018 {
            return;
        }
        let Some(literal) = self.nodes.parent(node) else { return };
        let Some(Node::ObjectLiteralExpression(object)) = self.node_map.get(literal) else {
            return;
        };
        if object.properties.last().and_then(tsr_ast::ObjectLiteralElementLike::node_id)
            != Some(node)
        {
            return;
        }
        if self.assignment_target_kind(literal) != AssignmentTargetKind::Definite {
            return;
        }
        self.check_external_emit_helpers(node, helpers::REST);
    }

    /// Requests a node's subtree would make, ahead of the node's own, for the
    /// sites upstream reaches only after checking an operand
    /// (`checkPropertyAccessExpression` checks `left` first, `checkInExpression`
    /// runs after both operands, `checkYieldExpression` after its operand).
    /// The walk visits that operand again afterwards; a request is
    /// idempotent once made (the per-file mask), so the revisit is silent and
    /// the first request — the reported one — is upstream's.
    fn request_operand_emit_helpers_first(&mut self, operand: Option<NodeId>) {
        let Some(operand) = operand else { return };
        let mut stack = vec![operand];
        while let Some(node) = stack.pop() {
            let Some(typed) = self.node_map.get(node) else { continue };
            self.check_construct_emit_helpers(node, typed);
            let start = stack.len();
            tsr_ast::for_each_child_id(typed, |child| stack.push(child));
            stack[start..].reverse();
        }
    }

    /// `checkVariableDeclarationList` (`checker.go:5774`): `using` and
    /// `await using` before `ESNext`.
    fn check_variable_declaration_list_emit_helpers(&mut self, node: NodeId) {
        let scope = self.nodes.flags(node) & NodeFlags::BLOCK_SCOPED;
        if (scope == NodeFlags::USING || scope == NodeFlags::USING | NodeFlags::CONST)
            && self.language_version < ScriptTarget::ESNext
        {
            self.check_external_emit_helpers(
                node,
                helpers::ADD_DISPOSABLE_RESOURCE_AND_DISPOSE_RESOURCES,
            );
        }
    }

    /// `checkDecorators`' legacy and ES requests (`checker.go:6031-6038`), at
    /// the first decorator.
    ///
    /// Not ported here: the `ast.NodeCanBeDecorated` half of the entry
    /// guard (its port is private to `grammar.rs`; a node it rejects already
    /// carries TS1206), and the `__setFunctionName` / `__propKey` arms.
    fn check_decorators_emit_helpers(&mut self, typed: Node<'_>) {
        let Some(first) = crate::check::modifiers_of(typed).and_then(|modifiers| {
            modifiers.iter().find_map(|modifier| match modifier {
                tsr_ast::ModifierLike::Decorator(decorator) => decorator.node_id,
                tsr_ast::ModifierLike::Token(_) => None,
            })
        }) else {
            return;
        };
        // `ast.CanHaveDecorators`.
        if !matches!(
            typed,
            Node::ParameterDeclaration(_)
                | Node::PropertyDeclaration(_)
                | Node::MethodDeclaration(_)
                | Node::GetAccessorDeclaration(_)
                | Node::SetAccessorDeclaration(_)
                | Node::ClassExpression(_)
                | Node::ClassDeclaration(_)
        ) {
            return;
        }
        if self.legacy_decorators {
            self.check_external_emit_helpers(first, helpers::DECORATE);
            if matches!(typed, Node::ParameterDeclaration(_)) {
                self.check_external_emit_helpers(first, helpers::PARAM);
            }
        } else if self.language_version < ScriptTarget::ESNext {
            self.check_external_emit_helpers(first, helpers::ES_DECORATE_AND_RUN_INITIALIZERS);
        }
    }
}

/// `ast.GetFunctionFlags` (`ast/functionflags.go:13`) for the node kinds that
/// carry a body; `None` for any other kind (upstream's `Invalid` without a
/// body slot).
#[derive(Debug, Clone, Copy)]
struct FunctionFlags {
    is_async: bool,
    generator: bool,
    /// Not `FunctionFlagsInvalid`: the function has a body.
    valid: bool,
}

fn function_flags(typed: Node<'_>) -> Option<FunctionFlags> {
    let (modifiers, generator, body): (&[tsr_ast::ModifierLike<'_>], bool, bool) = match typed {
        Node::FunctionDeclaration(n) => (n.modifiers, n.asterisk_token.is_some(), n.body.is_some()),
        Node::MethodDeclaration(n) => (n.modifiers, n.asterisk_token.is_some(), n.body.is_some()),
        Node::FunctionExpression(n) => (n.modifiers, n.asterisk_token.is_some(), n.body.is_some()),
        Node::ArrowFunction(n) => (n.modifiers, false, n.body.is_some()),
        Node::ConstructorDeclaration(n) => (&[], false, n.body.is_some()),
        Node::GetAccessorDeclaration(n) => (&[], false, n.body.is_some()),
        Node::SetAccessorDeclaration(n) => (&[], false, n.body.is_some()),
        _ => return None,
    };
    Some(FunctionFlags {
        is_async: tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AsyncKeyword),
        generator,
        valid: body,
    })
}
