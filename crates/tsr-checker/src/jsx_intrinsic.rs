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
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `JsxNames.IntrinsicElements`.
const INTRINSIC_ELEMENTS: &str = "IntrinsicElements";

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
    /// Upstream resolves the `JSX` namespace at the location — through the
    /// `jsxFactory`/`jsxFragmentFactory` pragma when one is set, otherwise the
    /// global `JSX` — and then looks `IntrinsicElements` up in its exports.
    /// **The pragma path is not ported**: a file setting `@jsx` to a factory
    /// whose namespace declares its own `IntrinsicElements` would be answered
    /// from the global `JSX` instead. That can only make this rule *report*
    /// where upstream is silent, so it is a falsifier for the wrong column
    /// rather than a silent gap — see §170.
    fn jsx_intrinsic_elements_exists(&mut self, location: NodeId) -> bool {
        // `getJsxNamespaceAt` (`jsx.go:1306`) resolves the **pragma's**
        // namespace first and looks for a `JSX` namespace among its exports;
        // only with no pragma does it fall back to resolving `JSX` itself.
        // §211 — and it is reachable only since §208, which made a namespace in
        // a `.d.ts` export what it declares.
        if let Some(namespace) = self.jsx_namespace_from_pragma(location) {
            return self.exports_intrinsic_elements(namespace);
        }
        let Some(namespace) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            location,
            "JSX",
            SymbolFlags::MODULE,
        ) else {
            return false;
        };
        self.exports_intrinsic_elements(namespace)
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

    /// The `JSX` namespace an `@jsx` pragma points at, if the file has one.
    ///
    /// Two hops, both upstream's: resolve the factory's namespace, then find
    /// `JSX` among its exports (`jsx.go:1321`). The factory is routinely
    /// **imported**, so `resolveSymbol` is applied to the alias — §187 paid for
    /// the same omission on TS2694.
    ///
    /// A pragma this port cannot resolve returns `None` and the caller falls
    /// back to the global lookup, which is upstream's own order rather than a
    /// bound chosen here.
    fn jsx_namespace_from_pragma(&mut self, location: NodeId) -> Option<tsr_binder::SymbolId> {
        let file = self.source_file_of_for_diagnostics(location)?;
        let factory = self.module_host?.jsx_factory_namespace(file)?;
        let container = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            location,
            &factory,
            SymbolFlags::MODULE | SymbolFlags::ALIAS,
        )?;
        let container = self.binder.merged_symbol(container);
        let container = if self.binder.symbols().get(container).flags.intersects(SymbolFlags::ALIAS)
        {
            self.binder.merged_symbol(self.resolve_alias(container)?)
        } else {
            container
        };
        self.binder.symbols().get(container).exports.get("JSX").copied()
    }
}
