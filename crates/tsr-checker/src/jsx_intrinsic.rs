//! TS7026 — `JSX element implicitly has type 'any' because no interface
//! 'JSX.IntrinsicElements' exists.`
//!
//! `getIntrinsicTagSymbol` (`jsx.go:1253`), the arm reached when every lookup
//! for the intrinsic-element table failed.
//!
//! # Why this is a resolver rule and not a type rule
//!
//! The message sits at the end of `getJsxType` → `getJsxNamespaceAt` →
//! `getExportsOfSymbol`, which reads like the type machinery and is the reason
//! the row was priced as expensive for six sessions — while also being filed
//! under the wrong subsystem entirely (§158: it is JSX, not `declare global`
//! merging). But the branch that fires is the one where the lookup found
//! **nothing**, and *"is there a namespace `JSX` exporting an interface
//! `IntrinsicElements` in scope here"* is a question the binder answers.
//!
//! It answers it **correctly only since §166**, which stopped
//! `resolve_name`'s `globals` fallback from returning any name under any
//! meaning. Before that this rule could not have been written: `JSX` would have
//! resolved from `globals` in every file that had a `JSX` of any kind.
//!
//! `docs/architecture/checker-notes-diag2.md` §170.

use tsr_ast::{Node, NodeId};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `JsxNames.IntrinsicElements`.
const INTRINSIC_ELEMENTS: &str = "IntrinsicElements";

/// `JsxNames.JSX`.
const JSX: &str = "JSX";

/// `scanner.IsIntrinsicJsxName` (`scanner/utilities.go:98`).
///
/// Upstream's two disjuncts exactly: a leading lowercase ASCII letter, **or** a
/// hyphen anywhere. The second is what makes `<foo-bar/>` and `<my-element/>`
/// intrinsic regardless of case, and dropping it would send custom elements to
/// the value-tag path instead.
fn is_intrinsic_jsx_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else { return false };
    first.is_ascii_lowercase() || name.contains('-')
}

impl Checker<'_, '_> {
    /// One JSX opening-like element.
    ///
    /// **Opening-like only, and that bound is now known to be incomplete.**
    /// §189 justified it as *"upstream reaches `getIntrinsicTagSymbol` from the
    /// opening one; reporting on both would put two diagnostics where upstream
    /// has one"*. The corpus contradicts that: `jsxNamespacePrefixInName`'s
    /// baseline carries TS7026 at **both** `(2,20)` and `(2,31)` — the opening
    /// and the closing tag of `<a:element></a:element>` — so upstream checks the
    /// closing name as well.
    ///
    /// The bound is kept because lifting it is a *measurement*, not an
    /// inference, and this port emits **no** TS7026 in that file today for a
    /// separate reason. **Do not lift it without re-measuring**; §206 records
    /// what the baseline actually shows and what is unexplained.
    pub(crate) fn check_jsx_intrinsic_element(&mut self, node: NodeId, typed: Node<'_>) {
        // `if c.noImplicitAny` (`jsx.go:1252`) — the whole arm is inside it,
        // so the rule is silent under `noImplicitAny: false` rather than
        // reporting and being filtered later.
        if !self.no_implicit_any {
            return;
        }
        let tag = match typed {
            Node::JsxOpeningElement(element) => element.tag_name,
            Node::JsxSelfClosingElement(element) => element.tag_name,
            _ => return,
        };
        let Some(tag) = tag else { return };
        let Some(tag_id) = tag.node_id() else { return };
        // `isJsxIntrinsicTagName` (`checker/utilities.go:1116`): an identifier
        // whose text is intrinsic, **or** a namespaced name in any spelling.
        // A capitalised bare identifier is a *value* tag and takes an entirely
        // different path (`getJsxElementTagSymbol`'s else branch), which this
        // rule must not claim.
        let intrinsic = match self.node_map.get(tag_id) {
            Some(Node::Identifier(identifier)) => is_intrinsic_jsx_name(identifier.text),
            Some(Node::JsxNamespacedName(_)) => true,
            _ => false,
        };
        if !intrinsic {
            return;
        }
        if self.jsx_intrinsic_elements_exists(node) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `c.error(node, …)` — the **element**, not the tag name.
        // `tsxNoJsx.tsx`'s baseline underlines all eight characters of
        // `<nope />`, so this is the node's own span rather than `error_span`,
        // which would narrow to a declaration name it does not have.
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::JSX_ELEMENT_IMPLICITLY_HAS_TYPE_ANY_BECAUSE_NO_INTERFACE_JSX_0_EXISTS,
                span,
                [INTRINSIC_ELEMENTS.to_string()],
            ),
        );
    }

    /// `getJsxType(JsxNames.IntrinsicElements, location)` reduced to the
    /// question its failure arm asks: does the name resolve at all?
    ///
    /// # Upstream picks ONE name, then resolves it, then falls back once
    ///
    /// `getJsxNamespaceAt` (`internal/checker/jsx.go:1306`) is three roads:
    ///
    /// 1. the **implicit-import container** — `react/jsx-runtime` via
    ///    `jsxImportSource`, under `jsx: react-jsx` (`:1451`). Not ported; see
    ///    the note on direction below;
    /// 2. **`getJsxNamespace(location)` resolved as a namespace, then its `JSX`
    ///    export** (`:1317-1321`);
    /// 3. the **global `JSX`** (`:1334`), and only then.
    ///
    /// Road 2's *name* is a single choice made by `getJsxNamespace`
    /// (`:1341-1387`), not a sequence of attempts: the file's `@jsx` pragma if
    /// it has one, else `c._jsxNamespace` — which is initialised to **`React`**
    /// and only then overridden by `jsxFactory`'s first identifier or by
    /// `reactNamespace`. [`Checker::jsx_namespace_name`] is that choice.
    ///
    /// **The default is the whole point.** This rule shipped with road 3 alone
    /// under a comment claiming upstream used the pragma "when one is set,
    /// otherwise the global `JSX`". There is no "otherwise" — an unconfigured
    /// build already resolves `React.JSX`. With `@types/react` 19 that is the
    /// only road there is: `namespace JSX` sits inside `declare namespace
    /// React` and **there is no global one**. On a 22-package repository the
    /// rule reported **1,642 TS7026 against `tsc`'s zero**, one per JSX
    /// element. `checker-notes-diag2.md` §221.
    ///
    /// # Road 1 is not ported, and the direction of that is the safety argument
    ///
    /// It needs a module resolution with no specifier node to hang it on —
    /// upstream synthesises the reference from the first JSX tag in the file.
    /// Every road here is a **lookup** and the rule fires only when all of them
    /// miss, so an unported road can make this rule *report where upstream is
    /// silent*, never the reverse. The same holds for a qualified
    /// `export = A.B` and for a module with a real `default` export.
    fn jsx_intrinsic_elements_exists(&mut self, location: NodeId) -> bool {
        if let Some(namespace) = self.jsx_namespace_symbol(location) {
            return self.exports_intrinsic_elements(namespace);
        }
        // Road 3. Reached only when road 2 found no `JSX` at all — upstream
        // falls back on *namespace resolution* failing, not on the member
        // lookup failing, so a `React` that exports a `JSX` without
        // `IntrinsicElements` is answered from there and not from the global.
        let Some(namespace) =
            self.binder.resolve_name(self.nodes, self.node_map, location, JSX, SymbolFlags::MODULE)
        else {
            return false;
        };
        self.exports_intrinsic_elements(namespace)
    }

    /// Road 2: the JSX namespace hanging off `getJsxNamespace`'s name.
    fn jsx_namespace_symbol(&mut self, location: NodeId) -> Option<SymbolId> {
        let name = self.jsx_namespace_name(location)?;
        let container = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            location,
            &name,
            SymbolFlags::MODULE | SymbolFlags::ALIAS,
        )?;
        let container = self.binder.merged_symbol(container);
        // `c.resolveSymbol(resolvedNamespace)` before `getExportsOfSymbol`.
        let container = self.resolve_jsx_namespace_container(container);
        let container = self.follow_export_assignment(container);
        self.binder.symbols().get(container).exports.get(JSX).copied()
    }

    /// `getJsxNamespace(location)` (`jsx.go:1341`): the file's `@jsx` pragma,
    /// else the option-derived default.
    ///
    /// The pragma half is the host's, because a pragma is a *parse* fact and
    /// the checker holds no source text; the default half is
    /// [`Checker::jsx_namespace`], resolved once from the compiler options.
    fn jsx_namespace_name(&mut self, location: NodeId) -> Option<String> {
        if let Some(file) = self.source_file_of_for_diagnostics(location)
            && let Some(host) = self.module_host
            && let Some(pragma) = host.jsx_factory_namespace(file)
        {
            return Some(pragma);
        }
        (!self.jsx_namespace.is_empty()).then(|| self.jsx_namespace.clone())
    }

    /// `c.resolveSymbol(resolvedNamespace)`, for the one alias form the JSX
    /// namespace actually arrives through.
    ///
    /// `@types/react` ends with **both**:
    ///
    /// ```ts
    /// export = React;
    /// export as namespace React;
    /// ```
    ///
    /// so the name `React` in a file that never imports it resolves to the UMD
    /// global — a `NamespaceExportDeclaration` alias — and the namespace whose
    /// exports hold `JSX` is two hops away.
    /// `getTargetOfNamespaceExportDeclaration` (`checker.go:15011`) is those
    /// two hops in one line: take the alias declaration's **parent** symbol,
    /// which is the file's module symbol, and `resolveExternalModuleSymbol` it,
    /// which follows `export =` (`checker.go:15556`).
    ///
    /// Done here rather than by widening [`Checker::resolve_alias`]: that
    /// function's remaining arms are declined for *naming* reasons its own
    /// rustdoc sets out — an alias resolved there starts printing under the
    /// target's name — and this rule never prints the symbol it finds, it only
    /// asks whether it exists.
    fn resolve_jsx_namespace_container(&mut self, symbol: SymbolId) -> SymbolId {
        let umd = self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            matches!(self.node_map.get(declaration), Some(Node::NamespaceExportDeclaration(_)))
        });
        if umd && let Some(parent) = self.binder.symbols().get(symbol).parent {
            return self.resolve_external_module_symbol(self.binder.merged_symbol(parent));
        }
        if let Some(module) = self.namespace_import_module(symbol) {
            return module;
        }
        // Any other alias form (`import React = require("react")`), then the
        // `export =` hop for a directly-named module symbol.
        let resolved = self.resolve_alias(symbol).unwrap_or(symbol);
        self.resolve_external_module_symbol(resolved)
    }

    /// `getTargetOfNamespaceImport` (`checker.go:15053`) for
    /// `import * as React from "react"`.
    ///
    /// The commonest spelling in a real codebase, and the one
    /// [`Checker::resolve_alias`] declines: its rustdoc records that resolving
    /// a namespace import there would make it *print* as the target's stripped
    /// file path where upstream prints the alias's own name (`bd tsr-4jk`).
    /// That argument is about rendering and does not reach this rule, which
    /// asks only whether a `JSX` namespace exists behind the name.
    ///
    /// The `export =` hop is upstream's `resolveESModuleSymbol` composed with
    /// `resolveExternalModuleSymbol`; `@types/react` needs it, since its `JSX`
    /// namespace lives inside the `React` namespace the file assigns.
    ///
    /// **A default import takes the same road, and that is a simplification.**
    /// `import React from "react"` is `getTargetOfImportClause`
    /// (`checker.go:15020`), which looks for a `default` export first and only
    /// falls back to `resolveExternalModuleSymbol` under
    /// `allowSyntheticDefaultImports`. That fallback is the arm `@types/react`
    /// takes — it writes `export = React` and has no `default` — and it is the
    /// only one built here. A module with a real `default` export whose type
    /// carries a `JSX` namespace would be answered from its `export =` instead,
    /// find nothing, and leave this rule reporting: the same one-directional
    /// failure as every other decline in this file.
    fn namespace_import_module(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let declaration = *self.binder.symbols().get(symbol).declarations.iter().find(|&&d| {
            matches!(
                self.node_map.get(d),
                // `import * as React from "react"`, and `import React from
                // "react"` — the default-import clause, which reaches the same
                // place for this question.
                Some(Node::NamespaceImport(_) | Node::ImportClause(_))
            )
        })?;
        // NamespaceImport -> NamedImportBindings slot -> ImportClause ->
        // ImportDeclaration. Four levels at most, and the loop stops at the
        // first import declaration rather than counting.
        let mut import = declaration;
        for _ in 0..4 {
            if matches!(self.node_map.get(import), Some(Node::ImportDeclaration(_))) {
                break;
            }
            import = self.nodes.parent(import)?;
        }
        let Some(Node::ImportDeclaration(node)) = self.node_map.get(import) else { return None };
        let specifier = node.module_specifier?.node_id()?;
        let module = self.resolve_external_module_name(import, specifier)?;
        Some(self.resolve_external_module_symbol(module))
    }

    /// `getTargetOfExportAssignment` (`checker.go:14976`), narrowed to the one
    /// spelling that carries a JSX namespace.
    ///
    /// `resolveExternalModuleSymbol` hands back the `export=` **symbol**, which
    /// for `export = React` is an alias to a local namespace rather than the
    /// namespace itself — so its export table is empty and the `JSX` lookup
    /// above would miss. Upstream follows it through
    /// `getTargetOfAliasLikeExpression`, which for a bare identifier is
    /// `resolveEntityName` at the assignment's own location.
    ///
    /// Only the identifier spelling is ported. A qualified `export = A.B` or a
    /// call/require expression declines, and declining can only leave this rule
    /// reporting where upstream is silent — never the reverse.
    fn follow_export_assignment(&mut self, symbol: SymbolId) -> SymbolId {
        let Some(&declaration) = self.binder.symbols().get(symbol).declarations.first() else {
            return symbol;
        };
        let Some(Node::ExportAssignment(assignment)) = self.node_map.get(declaration) else {
            return symbol;
        };
        let Some(tsr_ast::Expression::Identifier(name)) = assignment.expression else {
            return symbol;
        };
        let Some(name_id) = name.node_id else { return symbol };
        self.binder
            .resolve_name(self.nodes, self.node_map, name_id, name.text, SymbolFlags::NAMESPACE)
            .map_or(symbol, |found| self.binder.merged_symbol(found))
    }

    /// `getSymbol(getExportsOfSymbol(namespace), IntrinsicElements, …)`.
    ///
    /// An interface is a TYPE, and asking for the meaning is what keeps a
    /// `const IntrinsicElements` from answering.
    fn exports_intrinsic_elements(&self, namespace: tsr_binder::SymbolId) -> bool {
        self.binder.symbols().get(namespace).exports.get(INTRINSIC_ELEMENTS).is_some_and(
            |&member| self.binder.symbols().get(member).flags.intersects(SymbolFlags::TYPE),
        )
    }
}
