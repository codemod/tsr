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
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `markJsxAliasReferenced`'s not-found message (TS2874).
const TAG_MESSAGE: &tsr_diagnostics::Message =
    &messages::THIS_JSX_TAG_REQUIRES_0_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND;

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
            self.resolve_jsx_factory_name(location, &namespace, flags, report, true, TAG_MESSAGE);
        }
        if fragment {
            let root = self.jsx_factory_entity_root(file);
            self.resolve_jsx_factory_name(location, &root, flags, report, false, TAG_MESSAGE);
        }
    }

    /// One `resolveName(location, name, flags, jsxFactoryRefErr, isUse: true)`
    /// of [`Checker::mark_jsx_alias_referenced`].
    ///
    /// `isUse` marks the symbol with `flags` (`nameresolver.go:314`); the
    /// first lookup then marks it again with `SymbolFlagsAll`
    /// (`symbolReferenced`, `checker.go:28527`), which `mark_all` stands for.
    /// A miss under `jsx: react` is `onFailedToResolveSymbol` with `message`
    /// (TS2874 here, TS2879 for [`Checker::check_jsx_fragment_factory_in_scope`])
    /// as the not-found message: the missing-lib and spelling arms first
    /// (`checker.go:1585-1607`), so a near neighbour turns it into TS2552.
    fn resolve_jsx_factory_name(
        &mut self,
        location: NodeId,
        name: &str,
        flags: SymbolFlags,
        report: bool,
        mark_all: bool,
        message: &'static tsr_diagnostics::Message,
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
        let Some(file) = self.source_file_of_for_diagnostics(location) else { return };
        let span = self.nodes.span(location);
        let diagnostic = if let Some(lib) = crate::check::suggested_lib_for(name) {
            Diagnostic::with_args(message, span, [name.to_string(), lib.to_string()])
        } else if let Some(suggestion) = self.spelling_suggestion_for(location, name, flags) {
            Diagnostic::with_args(
                &messages::CANNOT_FIND_NAME_0_DID_YOU_MEAN_1,
                span,
                [name.to_string(), suggestion],
            )
        } else {
            Diagnostic::with_args(message, span, [name.to_string()])
        };
        self.report(file, diagnostic);
    }

    /// `checkJsxFragment`'s factory check (`jsx.go:111-120`): under a JSX
    /// transform, a `jsxFactory` option or an `@jsx` pragma needs a
    /// fragment factory beside it — TS17016 when the option set the element
    /// factory, TS17017 when the pragma did.
    ///
    /// Upstream reports it from the fragment's type function; this port
    /// reports from the per-node walk, which visits each fragment once (the
    /// same split as `check_jsx_expression`, §2).
    pub(crate) fn check_jsx_fragment_factory(&mut self, node: NodeId, typed: Node<'_>) {
        if matches!(typed, Node::JsxOpeningFragment(_)) {
            self.check_jsx_fragment_factory_in_scope(node);
            return;
        }
        if !matches!(typed, Node::JsxFragment(_)) {
            return;
        }
        let Some(factory_option) = self.jsx_fragment_factory_missing else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let (jsx_pragma, jsx_frag_pragma) =
            self.module_host.map_or((false, false), |host| host.jsx_pragmas_present(file));
        if !(factory_option || jsx_pragma) || jsx_frag_pragma {
            return;
        }
        let message = if factory_option {
            &messages::THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WITH_THE_JSXFACTORY_COMPILER_OPTION
        } else {
            &messages::AN_JSXFRAG_PRAGMA_IS_REQUIRED_WHEN_USING_AN_JSX_PRAGMA_WITH_JSX_FRAGMENTS
        };
        let span = self.nodes.span(node);
        self.report(file, Diagnostic::new(message, span));
    }

    /// `getJSXFragmentType`'s factory lookup (`jsx.go:499-522`), which
    /// `resolveJsxOpeningLikeElement` reaches for an opening fragment: when
    /// the classic runtime is selected (`jsx: react`) or a
    /// `jsxFragmentFactory` option is set, and the fragment factory
    /// (`getJsxNamespace` at the fragment) is not `null`, it is resolved as a
    /// value at the fragment — unless the automatic runtime's module answers
    /// (`getJsxNamespaceContainerForImplicitImport`) — with TS2879 as the
    /// not-found message (Enum excluded under `preserve`/`react-native`).
    ///
    /// Upstream caches the answer in the file's links (`jsxFragmentType`), so
    /// only the first fragment it types reports. Asking from the fragment
    /// itself, the first opening fragment of a pre-order walk stands for
    /// that first request (the TS2875 shape, [`Checker::check_jsx_runtime_module`]);
    /// no per-file table is added. The fragment's type itself
    /// (`React.Fragment`'s signatures) is not built here.
    fn check_jsx_fragment_factory_in_scope(&mut self, node: NodeId) {
        let name = self.jsx_namespace_at(node, true);
        let classic = self.jsx_emit == tsr_core::JsxEmit::React;
        if !(classic || self.jsx_fragment_namespace.is_some()) || name == "null" {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if self.module_host.is_some_and(|host| host.jsx_implicit_import_base(file).is_some())
            && self.jsx_implicit_import_container(node).is_some()
        {
            return;
        }
        if self.first_jsx_opening_fragment_in(file, self.nodes.span(node).start) != Some(node) {
            return;
        }
        let mut flags = SymbolFlags::VALUE;
        if matches!(self.jsx_emit, tsr_core::JsxEmit::Preserve | tsr_core::JsxEmit::ReactNative) {
            flags.remove(SymbolFlags::ENUM);
        }
        self.resolve_jsx_factory_name(node,
            &name,
            flags,
            true,
            false,
            &messages::USING_JSX_FRAGMENTS_REQUIRES_FRAGMENT_FACTORY_0_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND,
        );
    }

    /// The pre-order walk's first `JsxOpeningFragment` starting no later than
    /// `limit` (subtrees after it are not entered).
    fn first_jsx_opening_fragment_in(&self, root: NodeId, limit: u32) -> Option<NodeId> {
        let node = self.node_map.get(root)?;
        if matches!(node, Node::JsxOpeningFragment(_)) {
            return Some(root);
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(node, |child| children.push(child));
        children
            .into_iter()
            .take_while(|&child| self.nodes.span(child).start <= limit)
            .find_map(|child| self.first_jsx_opening_fragment_in(child, limit))
    }

    /// TS2875, `getJsxNamespaceContainerForImplicitImport`'s report
    /// (`jsx.go:1451-1486`): the runtime module the file imports
    /// (`getJSXRuntimeImportSpecifier`, the loader's synthetic import,
    /// `fileloader.go:551`) does not resolve. Upstream resolves it lazily for
    /// the first JSX type question in the file and caches the answer in the
    /// file's links, so the error is reported once, at `firstJSXTagInFile`:
    /// the first `JsxElement`, `JsxSelfClosingElement` or (for a fragment)
    /// `JsxOpeningFragment` of a pre-order walk. Here it is asked from that
    /// tag's own check, which gives the same single report without a
    /// per-file table.
    ///
    /// `resolveExternalModule` (`checker.go:15149`) reports the given message
    /// only when no ambient module, resolution or pattern ambient module
    /// answers. A specifier that resolves to a file the program does not hold
    /// (an untyped package) reports TS7016 instead; that arm is not ported, so
    /// any found resolution is silent.
    pub(crate) fn check_jsx_runtime_module(&mut self, node: NodeId, typed: Node<'_>) {
        let tag = match typed {
            Node::JsxOpeningElement(_) => match self.nodes.parent(node) {
                Some(parent) => match self.node_map.get(parent) {
                    Some(Node::JsxElement(element))
                        if element.opening_element.and_then(|opening| opening.node_id)
                            == Some(node) =>
                    {
                        parent
                    }
                    _ => return,
                },
                None => return,
            },
            Node::JsxSelfClosingElement(_) | Node::JsxOpeningFragment(_) => node,
            _ => return,
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let Some(host) = self.module_host else { return };
        let Some(base) = host.jsx_implicit_import_base(file) else { return };
        let runtime = if self.jsx_emit == tsr_core::JsxEmit::ReactJsxDev {
            "jsx-dev-runtime"
        } else {
            "jsx-runtime"
        };
        let specifier = format!("{base}/{runtime}");
        if self.ambient_module(&specifier).is_some()
            || host.module_resolution_found(file, &specifier)
            || (self.has_pattern_ambient_modules
                && self.binder.pattern_ambient_module(&specifier).is_some())
        {
            return;
        }
        if self.first_jsx_tag_in(file, self.nodes.span(tag).start) != Some(tag) {
            return;
        }
        let span = self.nodes.span(tag);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::THIS_JSX_TAG_REQUIRES_THE_MODULE_PATH_0_TO_EXIST_BUT_NONE_COULD_BE_FOUND_MAKE_SURE_YOU_HAVE_TYPES_FOR_THE_APPROPRIATE_PACKAGE_INSTALLED,
                span,
                [specifier],
            ),
        );
    }

    /// `firstJSXTagInFile` (`jsx.go:1458-1472`): the pre-order walk's first
    /// `JsxElement` or `JsxSelfClosingElement`, or a fragment's opening.
    /// Subtrees starting after `limit` cannot hold an earlier tag and are not
    /// entered, so asking for the tag at `limit` walks only what precedes it.
    fn first_jsx_tag_in(&self, root: NodeId, limit: u32) -> Option<NodeId> {
        let node = self.node_map.get(root)?;
        match node {
            Node::JsxElement(_) | Node::JsxSelfClosingElement(_) => return Some(root),
            Node::JsxFragment(fragment) => return fragment.opening_fragment?.node_id,
            _ => {}
        }
        let mut children = Vec::new();
        tsr_ast::for_each_child_id(node, |child| children.push(child));
        children
            .into_iter()
            .take_while(|&child| self.nodes.span(child).start <= limit)
            .find_map(|child| self.first_jsx_tag_in(child, limit))
    }

    /// `getJsxNamespaceContainerForImplicitImport` (`jsx.go:1451`): the module
    /// the automatic runtime imports — `GetJSXRuntimeImport`
    /// (`utilities.go:2794`), `<base>/jsx-runtime` or `/jsx-dev-runtime` —
    /// resolved from the file, merged, past its `export =`.
    ///
    /// `None` when the classic runtime is selected or the module does not
    /// resolve. Upstream reports TS2875 at the file's first JSX tag in the
    /// second case; that report is [`Checker::check_jsx_runtime_module`].
    pub(crate) fn jsx_implicit_import_container(&mut self, location: NodeId) -> Option<SymbolId> {
        let file = self.source_file_of_for_diagnostics(location)?;
        let host = self.module_host?;
        let base = host.jsx_implicit_import_base(file)?;
        let runtime = if self.jsx_emit == tsr_core::JsxEmit::ReactJsxDev {
            "jsx-dev-runtime"
        } else {
            "jsx-runtime"
        };
        let specifier = format!("{base}/{runtime}");
        let module = self.ambient_module(&specifier).or_else(|| {
            host.resolved_module(file, &specifier).and_then(|target| self.binder.symbol_of(target))
        })?;
        Some(self.resolve_external_module_symbol(self.binder.merged_symbol(module)))
    }
}
