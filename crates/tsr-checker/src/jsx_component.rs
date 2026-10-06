//! `checkJsxOpeningLikeElementOrOpeningFragment`'s component-type check
//! (`checker/jsx.go:131-150`) and `checkJsxReturnAssignableToAppropriateBound`
//! (`jsx.go:168`): TS2786, `'{0}' cannot be used as a JSX component.`
//!
//! See `docs/parity/notes/jsx.md` §6 for what is declined and why.

use tsr_ast::{
    Expression, JsxAttributeLike, JsxAttributeName, JsxAttributeValue, JsxTagNameExpression, Node,
    NodeId,
};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::relater::{Relation, Ternary};
use crate::signatures::SignatureKind;
use crate::types::TypeId;

/// `JsxReferenceKind` (`jsx.go`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum JsxReferenceKind {
    Component,
    Function,
    Mixed,
}

impl Checker<'_, '_> {
    /// The value-tag half of `checkJsxOpeningLikeElementOrOpeningFragment`
    /// after `getResolvedSignature`: relate the resolved signature's return
    /// type to the bound its reference kind selects, and report TS2786 on the
    /// tag name when it is not assignable.
    ///
    /// The signature is the one [`Checker::jsx_attributes_context`] resolves
    /// and publishes in `resolved_call_signatures` — a single candidate,
    /// instantiated when generic, which is what `resolveCall` answers for a
    /// one-candidate list. Every other shape declines (no report):
    ///
    /// - an intrinsic tag: its fake signature returns `JSX.Element`, which
    ///   the `Mixed` bound always accepts;
    /// - a `JSX.ElementType` in scope: upstream takes the other branch
    ///   (`elementTypeConstraint`, tag type against it), not ported here;
    /// - overloads, or a union tag: the resolved candidate is `resolveCall`'s
    ///   choice (calls lane);
    /// - an error return type or bound: `errorType` relates to everything.
    pub(crate) fn check_jsx_component_bound(&mut self, node: NodeId, typed: Node<'_>) {
        let tag = match typed {
            Node::JsxOpeningElement(element) => element.tag_name,
            Node::JsxSelfClosingElement(element) => element.tag_name,
            _ => return,
        };
        let Some(tag) = tag else { return };
        if let JsxTagNameExpression::Identifier(name) = tag
            && crate::jsx_intrinsic::is_intrinsic_jsx_name(name.text)
        {
            return;
        }
        if matches!(tag, JsxTagNameExpression::JsxNamespacedName(_)) {
            return;
        }
        let Some(tag_id) = tag.node_id() else { return };
        if self.jsx_type_symbol(node, "ElementType").is_some() {
            return;
        }
        let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
        let tag_type = self.check_expression(expression);
        if self.is_error(tag_type)
            || self.store.get(tag_type).flags.intersects(crate::flags::TypeFlags::UNION)
        {
            return;
        }
        let Some(kind) = self.jsx_reference_kind(tag_type) else { return };
        self.jsx_attributes_context(node);
        let Some(signature) = self.resolved_call_signatures.get(&node).cloned() else { return };
        let Some(instance) = self.get_return_type_of_signature(&signature) else { return };
        if self.is_error(instance) {
            return;
        }
        let Some(bound) = self.jsx_component_bound(node, kind) else { return };
        if self.relate_ternary(instance, bound, Relation::Assignable) != Ternary::NotRelated {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(tag_id) else { return };
        let span = self.error_span(tag_id);
        let text = self.jsx_tag_text(tag_id);
        self.report(
            file,
            Diagnostic::with_args(&messages::_0_CANNOT_BE_USED_AS_A_JSX_COMPONENT, span, [text]),
        );
    }

    /// `resolveJsxOpeningLikeElement`'s string-literal arm (`jsx.go:544-583`
    /// with `getUninstantiatedJsxSignaturesOfType`, `jsx.go:898`): a value tag
    /// whose type is a string literal is looked up in `JSX.IntrinsicElements`
    /// (`getIntrinsicAttributesTypeFromStringLiteralType`). A name that is
    /// neither a property nor covered by the table's `string` index reports
    /// TS2339 on the element; the signature list is then empty, and since a
    /// string literal is not an untyped call (`isUntypedFunctionCall`: not
    /// `any`, not assignable to `Function`), TS2604 follows on the tag name.
    ///
    /// Only this arm of the no-signature path is ported: it does not read
    /// any signature list, so it does not inherit the producer gap that keeps
    /// round 1's general arm unlanded (`docs/parity/notes/jsx.md` §8). An
    /// unenumerable table declines; with no `IntrinsicElements` upstream
    /// answers `anyType` and reports nothing.
    pub(crate) fn check_jsx_string_literal_tag(&mut self, node: NodeId, typed: Node<'_>) {
        let tag = match typed {
            Node::JsxOpeningElement(element) => element.tag_name,
            Node::JsxSelfClosingElement(element) => element.tag_name,
            _ => return,
        };
        let Some(tag) = tag else { return };
        if let JsxTagNameExpression::Identifier(name) = tag
            && crate::jsx_intrinsic::is_intrinsic_jsx_name(name.text)
        {
            return;
        }
        if matches!(tag, JsxTagNameExpression::JsxNamespacedName(_)) {
            return;
        }
        let Some(tag_id) = tag.node_id() else { return };
        let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
        let tag_type = self.check_expression(expression);
        let crate::types::TypeData::StringLiteral(value) = self.store.get(tag_type).data.clone()
        else {
            return;
        };
        let Some(symbol) = self.jsx_type_symbol(node, "IntrinsicElements") else { return };
        let table = self.get_declared_type_of_symbol(symbol);
        if self.is_error(table) {
            return;
        }
        let Some(names) = self.get_property_names_of_type(table) else { return };
        if names.contains(&value) {
            return;
        }
        let string = self.intrinsics.string;
        if self.get_applicable_index_info(table, string).is_some() {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [value, "JSX.IntrinsicElements".to_string()],
            ),
        );
        let span = self.error_span(tag_id);
        let text = self.jsx_tag_text(tag_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::JSX_ELEMENT_TYPE_0_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES,
                span,
                [text],
            ),
        );
    }

    /// `checkGrammarJsxElement` (`grammarchecks.go:1156`), called first by
    /// `checkJsxOpeningLikeElementOrOpeningFragment` for an opening-like
    /// element.
    ///
    /// `checkGrammarJsxName` (`:1180`) reports TS17010 on a property access
    /// whose object is a namespaced name and, under a JSX transform, TS2639
    /// on a namespaced tag whose namespace is not intrinsic; its answer is
    /// ignored. `checkGrammarTypeArguments` comes next and is not ported here
    /// (its JSX arm belongs with the type-argument grammar). Then the
    /// attributes, in order, stopping at the first duplicate name (TS17001 on
    /// the name) or empty `{}` initializer (TS17000 on the initializer).
    /// `grammarErrorOnNode` reports nothing in a file with parse errors.
    pub(crate) fn check_grammar_jsx_element(&mut self, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let (tag, attributes) = match typed {
            Node::JsxOpeningElement(element) => (element.tag_name, element.attributes),
            Node::JsxSelfClosingElement(element) => (element.tag_name, element.attributes),
            _ => return,
        };
        if let Some(tag) = tag.and_then(|tag| tag.node_id()) {
            self.check_grammar_jsx_name(tag);
        }
        let Some(attributes) = attributes else { return };
        let mut seen: Vec<String> = Vec::new();
        for attribute in attributes.properties {
            let JsxAttributeLike::JsxAttribute(attribute) = attribute else { continue };
            let (text, name_id) = match attribute.name {
                Some(JsxAttributeName::Identifier(name)) => (name.text.to_string(), name.node_id),
                Some(JsxAttributeName::JsxNamespacedName(name)) => {
                    let (Some(namespace), Some(local)) = (name.namespace, name.name) else {
                        continue;
                    };
                    (format!("{}:{}", namespace.text, local.text), name.node_id)
                }
                None => continue,
            };
            if seen.contains(&text) {
                if let Some(name_id) = name_id {
                    self.grammar_error_on_node(
                        name_id,
                        &messages::JSX_ELEMENTS_CANNOT_HAVE_MULTIPLE_ATTRIBUTES_WITH_THE_SAME_NAME,
                    );
                }
                return;
            }
            seen.push(text);
            if let Some(JsxAttributeValue::JsxExpression(initializer)) = attribute.initializer
                && initializer.expression.is_none()
            {
                if let Some(id) = initializer.node_id {
                    self.grammar_error_on_node(
                        id,
                        &messages::JSX_ATTRIBUTES_MUST_ONLY_BE_ASSIGNED_A_NON_EMPTY_EXPRESSION,
                    );
                }
                return;
            }
        }
    }

    /// `checkGrammarJsxName` (`grammarchecks.go:1180`).
    fn check_grammar_jsx_name(&mut self, tag: NodeId) {
        match self.node_map.get(tag) {
            Some(Node::PropertyAccessExpression(access)) => {
                if let Some(object) = access.expression.and_then(|e| e.node_id())
                    && self.nodes.kind(object) == tsr_ast::SyntaxKind::JsxNamespacedName
                {
                    self.grammar_error_on_node(
                        object,
                        &messages::JSX_PROPERTY_ACCESS_EXPRESSIONS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                    );
                }
            }
            Some(Node::JsxNamespacedName(name)) => {
                let transform = matches!(
                    self.jsx_emit,
                    tsr_core::JsxEmit::React
                        | tsr_core::JsxEmit::ReactJsx
                        | tsr_core::JsxEmit::ReactJsxDev
                );
                if transform
                    && let Some(namespace) = name.namespace
                    && !crate::jsx_intrinsic::is_intrinsic_jsx_name(namespace.text)
                {
                    self.grammar_error_on_node(
                        tag,
                        &messages::REACT_COMPONENTS_CANNOT_INCLUDE_JSX_NAMESPACE_NAMES,
                    );
                }
            }
            _ => {}
        }
    }

    /// `getJsxReferenceKind` (`jsx.go:1159`) for a value tag: construct
    /// signatures on the apparent type make a component, call signatures a
    /// function. `None` when a signature list is unresolved.
    fn jsx_reference_kind(&mut self, tag_type: TypeId) -> Option<JsxReferenceKind> {
        if !self.signatures_of_type_kind(tag_type, SignatureKind::Construct)?.is_empty() {
            return Some(JsxReferenceKind::Component);
        }
        if !self.call_signatures_of_type(tag_type)?.is_empty() {
            return Some(JsxReferenceKind::Function);
        }
        Some(JsxReferenceKind::Mixed)
    }

    /// The bound `checkJsxReturnAssignableToAppropriateBound` relates to:
    /// `JSX.Element | null` for a function (`getJsxStatelessElementTypeAt`),
    /// `JSX.ElementClass` for a class (`getJsxElementClassTypeAt`), their
    /// union for a mixed tag, which needs both. `None` where upstream skips
    /// the check — a missing `ElementClass` — or where a missing `Element`
    /// leaves `errorType` in the bound, which relates to everything.
    fn jsx_component_bound(&mut self, location: NodeId, kind: JsxReferenceKind) -> Option<TypeId> {
        let element = |checker: &mut Self| {
            let symbol = checker.jsx_type_symbol(location, "Element")?;
            let element = checker.get_declared_type_of_symbol(symbol);
            if checker.is_error(element) {
                return None;
            }
            let null = checker.intrinsics.null;
            Some(checker.get_union_type(&[element, null]))
        };
        let class = |checker: &mut Self| {
            let symbol = checker.jsx_type_symbol(location, "ElementClass")?;
            let class = checker.get_declared_type_of_symbol(symbol);
            (!checker.is_error(class)).then_some(class)
        };
        match kind {
            JsxReferenceKind::Function => element(self),
            JsxReferenceKind::Component => class(self),
            JsxReferenceKind::Mixed => {
                let element = element(self)?;
                let class = class(self)?;
                Some(self.get_union_type(&[element, class]))
            }
        }
    }

    /// `scanner.GetTextOfNode(tagName)` for the tag forms a value tag takes:
    /// an identifier, `this`, or a property-access chain of them.
    fn jsx_tag_text(&self, tag: NodeId) -> String {
        match self.node_map.get(tag) {
            Some(Node::Identifier(name)) => name.text.to_string(),
            Some(Node::PropertyAccessExpression(access)) => {
                let left = access
                    .expression
                    .and_then(|e| e.node_id())
                    .map_or_else(String::new, |e| self.jsx_tag_text(e));
                let right = access
                    .name
                    .and_then(|n| n.node_id())
                    .map_or_else(String::new, |n| self.jsx_tag_text(n));
                format!("{left}.{right}")
            }
            _ if self.nodes.kind(tag) == tsr_ast::SyntaxKind::ThisKeyword => "this".to_string(),
            _ => String::new(),
        }
    }
}
