//! The JSX factory: `getJsxNamespace`, `getJsxFactoryEntity` and
//! `markJsxAliasReferenced` (`checker/jsx.go:1341-1414`,
//! `checker.go:28502`).
//!
//! Upstream parses each factory name into a synthetic entity
//! (`parseIsolatedEntityName`) and caches it in `sourceFileLinks`. Every
//! caller here reads only the entity's **first identifier**, so this port
//! keeps that string and nothing else: the pragma half is computed once per
//! file by the parser (`FileReferences`) and reached through the
//! [`crate::resolution::ModuleHost`], the option half once per checker in
//! [`Checker::apply_compiler_options`]. See `docs/parity/notes/jsx.md` §3.

use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// The first identifier of `parser.ParseIsolatedEntityName(text)`
/// (`parser.go:279`), or `None` where that parse fails.
///
/// `parseEntityName(allowReservedWords: true)` that must reach end of input
/// with no diagnostics: identifier names (keywords included) joined by `.`.
/// The same function as `tsr_parser::pragma::isolated_entity_name_root`; the
/// checker does not depend on the parser crate, and the parse is eight lines.
pub(crate) fn isolated_entity_name_root(text: &str) -> Option<&str> {
    let mut root = None;
    for part in text.split('.') {
        let part = part.trim_matches(char::is_whitespace);
        let mut chars = part.chars();
        let first = chars.next()?;
        if !tsr_scanner::is_identifier_start(first) || !chars.all(tsr_scanner::is_identifier_part) {
            return None;
        }
        root.get_or_insert(part);
    }
    root
}

impl Checker<'_, '_> {
    /// `getJsxNamespace(location)` (`jsx.go:1341`).
    ///
    /// A fragment asks its `@jsxFrag` pragma, then `jsxFragmentFactory`; any
    /// other location asks the `@jsx` pragma. Both fall back on the
    /// option-derived default, [`Checker::jsx_namespace`] — which is why an
    /// `@jsxFrag null` that no option backs still answers `null` (the pragma
    /// parses: `null` is an identifier *name*) and a file with neither answers
    /// `React`.
    pub(crate) fn jsx_namespace_at(&self, location: NodeId, fragment: bool) -> String {
        let host = self.module_host;
        let file = self.source_file_of_for_diagnostics(location);
        if fragment {
            if let (Some(host), Some(file)) = (host, file)
                && let Some(namespace) = host.jsx_fragment_factory_namespace(file)
            {
                return namespace;
            }
            // `getJsxFragmentFactoryEntity`'s option arm (`jsx.go:1431`).
            if let Some(namespace) = &self.jsx_fragment_namespace {
                return namespace.clone();
            }
        } else if let (Some(host), Some(file)) = (host, file)
            && let Some(namespace) = host.jsx_factory_namespace(file)
        {
            return namespace;
        }
        self.jsx_namespace.clone()
    }

    /// `GetFirstIdentifier(getJsxFactoryEntity(file))` (`jsx.go:1406`): the
    /// file's `@jsx` pragma, else the option default — never the fragment
    /// factory, which is what makes the second lookup in
    /// [`Checker::mark_jsx_alias_referenced`] a different name.
    fn jsx_factory_entity_root(&self, file: NodeId) -> String {
        if let Some(host) = self.module_host
            && let Some(namespace) = host.jsx_factory_namespace(file)
        {
            return namespace;
        }
        self.jsx_namespace.clone()
    }

    /// `markJsxAliasReferenced` (`checker.go:28502`), called from
    /// `checkJsxOpeningLikeElementOrOpeningFragment` for every opening tag
    /// and opening fragment.
    ///
    /// Resolves the factory namespace as a **value** at the tag name (or at
    /// the `<>`), marks what it finds referenced — which is what keeps
    /// `import React = require("react")` from being TS6133 under
    /// `noUnusedLocals` — and under `jsx: react` reports TS2874 when nothing
    /// is in scope. A fragment additionally resolves the *element* factory's
    /// root, because emit calls both.
    ///
    /// **`getJsxNamespaceContainerForImplicitImport` is answered by the host's
    /// `GetJSXImplicitImportBase` alone.** Upstream returns early only when
    /// the runtime module *resolves*; an unresolved one (TS2875) falls through
    /// to the marking below. This port returns whenever the automatic runtime
    /// is selected, so in that one combination it marks and reports less than
    /// upstream — which is today's behaviour for every automatic-runtime file,
    /// since nothing marked the factory before.
    pub(crate) fn mark_jsx_alias_referenced(&mut self, node: NodeId, typed: Node<'_>) {
        let (location, fragment) = match typed {
            Node::JsxOpeningElement(element) => (element.tag_name.and_then(|t| t.node_id()), false),
            Node::JsxSelfClosingElement(element) => {
                (element.tag_name.and_then(|t| t.node_id()), false)
            }
            Node::JsxOpeningFragment(_) => (Some(node), true),
            _ => return,
        };
        let Some(location) = location else { return };
        let Some(file) = self.source_file_of_for_diagnostics(location) else { return };
        if let Some(host) = self.module_host
            && host.jsx_implicit_import_base(file).is_some()
        {
            return;
        }
        let report = self.jsx_emit == tsr_core::JsxEmit::React;
        let should_factory_ref_err =
            !matches!(self.jsx_emit, tsr_core::JsxEmit::Preserve | tsr_core::JsxEmit::ReactNative);
        let mut flags = SymbolFlags::VALUE;
        if !should_factory_ref_err {
            flags.remove(SymbolFlags::ENUM);
        }
        let namespace = self.jsx_namespace_at(location, fragment);
        // #38720/60122: `null` is allowed as the fragment factory.
        if !(fragment && namespace == "null") {
            self.resolve_jsx_factory_name(file, location, &namespace, flags, report, true);
        }
        if fragment {
            let root = self.jsx_factory_entity_root(file);
            self.resolve_jsx_factory_name(file, location, &root, flags, report, false);
        }
    }

    /// One `resolveName(location, name, flags, jsxFactoryRefErr, isUse: true)`
    /// of [`Checker::mark_jsx_alias_referenced`].
    ///
    /// `isUse` marks the symbol with `flags` (`nameresolver.go:314`); the
    /// first lookup then marks it again with `SymbolFlagsAll`
    /// (`symbolReferenced`, `checker.go:28527`), which `mark_all` stands for.
    /// A miss under `jsx: react` is `onFailedToResolveSymbol` with TS2874 as
    /// the not-found message: the missing-lib and spelling arms first
    /// (`checker.go:1585-1607`), so a near neighbour turns it into TS2552.
    fn resolve_jsx_factory_name(
        &mut self,
        file: NodeId,
        location: NodeId,
        name: &str,
        flags: SymbolFlags,
        report: bool,
        mark_all: bool,
    ) {
        if let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, location, name, flags)
        {
            let symbol = self.binder.merged_symbol(symbol);
            let kinds = if mark_all { SymbolFlags::all() } else { flags };
            *self.symbol_reference_kinds.entry(symbol).or_default() |= kinds;
            return;
        }
        if !report {
            return;
        }
        let span = self.nodes.span(location);
        let diagnostic = if let Some(lib) = crate::check::suggested_lib_for(name) {
            Diagnostic::with_args(
                &messages::THIS_JSX_TAG_REQUIRES_0_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND,
                span,
                [name.to_string(), lib.to_string()],
            )
        } else if let Some(suggestion) = self.spelling_suggestion_for(location, name) {
            Diagnostic::with_args(
                &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_1,
                span,
                [name.to_string(), suggestion],
            )
        } else {
            Diagnostic::with_args(
                &messages::THIS_JSX_TAG_REQUIRES_0_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND,
                span,
                [name.to_string()],
            )
        };
        self.report(file, diagnostic);
    }
}
