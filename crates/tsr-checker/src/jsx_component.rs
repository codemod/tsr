//! `checkJsxOpeningLikeElementOrOpeningFragment`'s component-type check
//! (`checker/jsx.go:131-150`) and `checkJsxReturnAssignableToAppropriateBound`
//! (`jsx.go:168`): TS2786, `'{0}' cannot be used as a JSX component.`
//!
//! See `docs/parity/notes/jsx.md` §6 for what is declined and why.

use tsr_ast::{
    Expression, JsxAttributeLike, JsxAttributeName, JsxAttributeValue, JsxTagNameExpression, Node,
    NodeId,
};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::relater::{Relation, Ternary};
use crate::signatures::SignatureKind;
use crate::types::TypeId;

/// [`Checker::jsx_excess_attribute`]'s decided answer: no excess member, or
/// the first one (`at` is `None` for the synthesized `children`).
enum JsxExcess {
    None,
    Member { at: Option<NodeId>, name: String },
}

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
    /// - a `JSX.ElementType` in scope: upstream takes the other branch,
    ///   [`Checker::check_jsx_element_type_constraint`], for every tag
    ///   (intrinsic included);
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
        let Some(tag_id) = tag.node_id() else { return };
        // `isJsxIntrinsicTagName`: an intrinsic identifier or any namespaced
        // name (`<a:b />`) resolves through the intrinsic-element table.
        let intrinsic = crate::jsx_intrinsic::jsx_intrinsic_tag_text(tag).is_some();
        if !intrinsic {
            self.check_jsx_signatureless_tag(tag, tag_id);
            self.check_jsx_class_attributes_member(node, typed, tag);
        }
        self.check_jsx_attributes_assignable(node, typed, tag, tag_id);
        if let Some(constraint) = self.jsx_element_type_type_at(node) {
            self.check_jsx_element_type_constraint(tag, tag_id, constraint);
            return;
        }
        if intrinsic {
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

    /// `resolveJsxOpeningLikeElement`'s no-signature arm (`jsx.go:566-581`)
    /// for a value tag whose type is neither `string` nor a string literal
    /// (those arms are `getUninstantiatedJsxSignaturesOfType`'s first two,
    /// the literal one ported as [`Checker::check_jsx_string_literal_tag`]):
    /// with an apparent type that is not `errorType`, an empty uninstantiated
    /// signature list that is not an untyped call (`isUntypedFunctionCall`
    /// with no construct count) reports TS2604 on the tag name.
    ///
    /// The list is [`Checker::jsx_uninstantiated_signatures_are_empty`]'s
    /// answer; every list this port cannot complete declines, so a missing
    /// signature producer costs a missing TS2604, never a false one.
    fn check_jsx_signatureless_tag(&mut self, tag: JsxTagNameExpression<'_>, tag_id: NodeId) {
        if let JsxTagNameExpression::Identifier(name) = tag
            && crate::jsx_intrinsic::is_intrinsic_jsx_name(name.text)
        {
            return;
        }
        let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
        let tag_type = self.check_expression(expression);
        if self.store.get(tag_type).flags.intersects(TypeFlags::STRING | TypeFlags::STRING_LITERAL)
        {
            return;
        }
        let apparent = self.apparent_type(tag_type);
        if self.is_error(apparent) || self.is_error(tag_type) {
            return;
        }
        // `isUntypedFunctionCall`'s list-free arms (`checker.go:9935`).
        let any = |checker: &Self, t: TypeId| checker.store.get(t).flags.intersects(TypeFlags::ANY);
        if any(self, tag_type)
            || any(self, apparent)
                && self.store.get(tag_type).flags.intersects(TypeFlags::TYPE_PARAMETER)
        {
            return;
        }
        if self.jsx_uninstantiated_signatures_are_empty(tag_type) != Some(true) {
            return;
        }
        // The signature-less arm: not a union, not reducing to `never`, and
        // not assignable to the global `Function`.
        if !self.store.get(apparent).flags.intersects(TypeFlags::UNION | TypeFlags::NEVER)
            && !self.intersection_has_never_discriminant(apparent)
        {
            let Some(function) = self.global_type_symbol_with_arity("Function", 0) else { return };
            let function = self.get_declared_type_of_symbol(function);
            if self.is_error(function)
                || self.relate_ternary(tag_type, function, Relation::Assignable)
                    != Ternary::NotRelated
            {
                return;
            }
        }
        let Some(file) = self.source_file_of_for_diagnostics(tag_id) else { return };
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

    /// `getJsxPropsTypeFromClassType`'s missing-member report (`jsx.go:953-958`):
    /// for a component reference (`getJsxReferenceKind`: construct signatures
    /// on the tag's apparent type) whose `JSX.ElementAttributesProperty` names
    /// a member, an instance type (the signature's return type, not `any`)
    /// without that member reports TS2607 on the element when the element
    /// has attributes (spreads count: `Attributes().Properties()`).
    ///
    /// Upstream reaches this from `getEffectiveFirstArgumentForJsxSignature`
    /// for every candidate `resolveCall` evaluates; the port answers only a
    /// single non-generic construct signature — the one candidate that is
    /// certainly evaluated — and declines overloads, generics and composite
    /// (union) signatures, whose `getJsxPropsTypeForSignatureFromMember` arm
    /// reads each constituent. Absence is a complete property table without
    /// the name.
    fn check_jsx_class_attributes_member(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        tag: JsxTagNameExpression<'_>,
    ) {
        let attributes = match typed {
            Node::JsxOpeningElement(element) => element.attributes,
            Node::JsxSelfClosingElement(element) => element.attributes,
            _ => return,
        };
        if attributes.is_none_or(|attributes| attributes.properties.is_empty()) {
            return;
        }
        if let JsxTagNameExpression::Identifier(name) = tag
            && crate::jsx_intrinsic::is_intrinsic_jsx_name(name.text)
        {
            return;
        }
        let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
        let tag_type = self.check_expression(expression);
        if self.is_error(tag_type) || self.store.get(tag_type).flags.intersects(TypeFlags::ANY) {
            return;
        }
        let Some(signatures) = self.signatures_of_type_kind(tag_type, SignatureKind::Construct)
        else {
            return;
        };
        let [signature] = signatures.as_slice() else { return };
        if !signature.type_parameters.is_empty() {
            return;
        }
        let signature = signature.clone();
        let Some(name) = self.jsx_element_properties_member_name(node) else { return };
        let Some(instance) = self.get_return_type_of_signature(&signature) else { return };
        if self.is_error(instance)
            || self.store.get(instance).flags.intersects(TypeFlags::ANY | TypeFlags::UNION)
        {
            return;
        }
        let Some(names) = self.get_property_names_of_type(instance) else { return };
        if names.contains(&name) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::JSX_ELEMENT_CLASS_DOES_NOT_SUPPORT_ATTRIBUTES_BECAUSE_IT_DOES_NOT_HAVE_A_0_PROPERTY,
                span,
                [name],
            ),
        );
    }

    /// The attributes relation of `resolveJsxOpeningLikeElement`: the
    /// intrinsic arm's `checkTypeAssignableToAndOptionallyElaborate(attributes,
    /// result, tagName, attributes)` (`jsx.go:549-551`) and, for a value tag,
    /// `checkApplicableSignatureForJsxCallLikeElement`'s
    /// `checkTypeRelatedToAndOptionallyElaborate(attributes, paramType,
    /// tagName, attributes)` (`jsx.go:682-698`) on the candidate `resolveCall`
    /// reports for — here only the single published signature
    /// ([`Checker::jsx_attributes_context`]); overloads are `chooseOverload`'s
    /// (calls lane) and decline.
    ///
    /// The error node is the tag name and the elaboration node the
    /// `JsxAttributes`; `elaborateError`'s `JsxAttributes` arm
    /// (`elaborateJsxComponents`) lives with the other elaboration arms in
    /// `assignreport.rs` (`docs/parity/notes/r4-jsx.md` §4).
    ///
    /// Declines: an `any` attributes type (`IsTypeAny` after an `any`
    /// spread), an unresolved props or attributes type, and a function tag
    /// whose smallest required argument count exceeds one, where
    /// `checkTagNameDoesNotExpectTooManyArguments` may answer TS6229 first.
    fn check_jsx_attributes_assignable(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        tag: JsxTagNameExpression<'_>,
        tag_id: NodeId,
    ) {
        let attributes = match typed {
            Node::JsxOpeningElement(element) => element.attributes,
            Node::JsxSelfClosingElement(element) => element.attributes,
            _ => return,
        };
        let Some(attributes) = attributes else { return };
        let Some(attributes_id) = attributes.node_id else { return };
        let intrinsic = crate::jsx_intrinsic::jsx_intrinsic_tag_text(tag).is_some();
        // `getJsxNamespaceAt` (`jsx.go:1306`) takes the automatic runtime's
        // module first. When the file selects that runtime and this port does
        // not resolve its module (a per-file `@jsxImportSource` the loader
        // does not read), the namespace found next is not upstream's — and
        // where the module truly is missing upstream reports TS2875 instead.
        if self.jsx_implicit_import_unresolved(node) {
            return;
        }
        if !intrinsic {
            let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
            let tag_type = self.check_expression(expression);
            let Some(calls) = self.call_signatures_of_type(tag_type) else { return };
            if calls.iter().any(|signature| self.signature_min_argument_count(signature) > 1) {
                return;
            }
        }
        for attribute in attributes.properties {
            if let JsxAttributeLike::JsxSpreadAttribute(spread) = attribute {
                let Some(expression) = spread.expression else { return };
                let spread_type = self.check_expression(expression);
                if self.store.get(spread_type).flags.intersects(TypeFlags::ANY)
                    || self.is_error(spread_type)
                {
                    return;
                }
            }
        }
        let Some(target) = self.jsx_attributes_context(node) else { return };
        if !intrinsic && !self.resolved_call_signatures.contains_key(&node) {
            return;
        }
        let Some(source) = self.jsx_checked_attributes_type(node) else { return };
        // `isComparingJsxAttributes` (`ObjectFlagsJsxAttributes` on the
        // source): a hyphenated member, written or spread, is known to
        // `hasCommonProperties` and skipped by `membersRelatedToIndexInfo`
        // (`isIgnoredJsxProperty`); everywhere else the relater relates it
        // like any member. This port's relater carries no such flag, so a
        // hyphenated source declines only where one of those two rules can
        // decide the answer (`jsx_hyphen_sensitive_target`).
        // Only the minted object carries `ObjectFlagsJsxAttributes`; an
        // intersection over a generic spread does not, and relates as any
        // intersection.
        let hyphenated: Vec<String> = match self.anonymous_properties.get(&source) {
            Some((properties, _)) => properties
                .iter()
                .filter(|property| property.name.contains('-'))
                .map(|property| property.name.clone())
                .collect(),
            None => Vec::new(),
        };
        if !hyphenated.is_empty() && self.jsx_hyphen_sensitive_target(target, &hyphenated) {
            return;
        }
        if self.is_error(source)
            || self.is_error(target)
            || self.store.get(target).flags.intersects(TypeFlags::ANY)
        {
            return;
        }
        // The attributes type is a fresh object literal (`ObjectFlagsFreshLiteral`,
        // `jsx.go:721`), so `isRelatedTo` meets `hasExcessProperties` before
        // the structural relation: an excess attribute fails the relation by
        // itself, whatever the members relate to. Only without one does the
        // structural answer decide. An excess member overrides a relation this
        // port did not answer `NotRelated` only when every object constituent
        // of the target has a complete member table: a name missing from an
        // unresolved table (a mapped or conditional target this port does not
        // enumerate) is not evidence.
        let Some(excess) = self.jsx_excess_attribute(attributes_id, source, target) else {
            return;
        };
        if self.relate_ternary(source, target, Relation::Assignable) != Ternary::NotRelated
            && (matches!(excess, JsxExcess::None) || !self.jsx_target_members_complete(target))
        {
            return;
        }
        // `checkTypeRelatedToAndOptionallyElaborate` (`checker.go`):
        // `elaborateError` on the attributes node first — its `JsxAttributes`
        // arm is `elaborateJsxComponents` — and only when that stays silent
        // `checkTypeRelatedToEx`, whose relation reports the excess member.
        if self.elaborate_jsx_components(attributes_id, source, target) != Some(false) {
            return;
        }
        if let JsxExcess::Member { at, name } = excess {
            self.report_jsx_excess_attribute(at.unwrap_or(tag_id), &name, source, target);
            return;
        }
        let span = self.error_span(tag_id);
        self.report_relation_failure(tag_id, span, None, source, target, None);
    }

    /// `elaborateJsxComponents` (`jsx.go:295`), the attributes half: each
    /// non-spread, non-hyphenated attribute is an `elaborateElement`
    /// (`relater.go:546`) — the target member (`getBestMatchIndexedAccessTypeOrUndefined`;
    /// absent or an indexed access on a generic skips), the source member,
    /// and on a failed relation the initializer's own elaboration, else the
    /// report on the attribute name. `Some(reported)`; `None` where this port
    /// cannot decide an element (a union target's best match, an undecided
    /// member relation) — the caller then reports nothing rather than a
    /// different line. The children half (TS2745/2746/2747) follows, in
    /// upstream's order, from [`Self::elaborate_jsx_children`].
    pub(crate) fn elaborate_jsx_components(
        &mut self,
        attributes: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let Some(Node::JsxAttributes(node)) = self.node_map.get(attributes) else {
            return Some(false);
        };
        let union_target = self.store.get(target).flags.intersects(TypeFlags::UNION);
        let mut elements = Vec::new();
        for attribute in node.properties {
            let JsxAttributeLike::JsxAttribute(attribute) = attribute else { continue };
            let Some(name_node) = attribute.name else { continue };
            let Some(text) = crate::jsx_intrinsic::jsx_attribute_name_text(name_node) else {
                continue;
            };
            if text.contains('-') {
                continue;
            }
            let Some(name_id) = name_node.node_id() else { continue };
            let target_member = match self.get_type_of_property_of_type(target, &text) {
                Some(member) => Some(member),
                None if union_target => return None,
                None => {
                    let key = self.store.intern_literal(
                        TypeFlags::STRING_LITERAL,
                        crate::types::TypeData::StringLiteral(text.clone()),
                        false,
                    );
                    self.get_applicable_index_info(target, key).map(|info| info.value)
                }
            };
            let Some(target_member) = target_member else { continue };
            if self.store.get(target_member).flags.intersects(TypeFlags::INDEXED_ACCESS) {
                continue;
            }
            let Some(source_member) = self.get_type_of_property_of_type(source, &text) else {
                continue;
            };
            match self.relate_ternary(source_member, target_member, Relation::Assignable) {
                Ternary::Related => continue,
                Ternary::Unknown => return None,
                Ternary::NotRelated => {}
            }
            // `next` is the initializer; `elaborateError` passes through a
            // `JsxExpression` to its expression.
            let next = match attribute.initializer {
                Some(JsxAttributeValue::JsxExpression(expression)) => {
                    expression.expression.and_then(|inner| inner.node_id())
                }
                Some(value) => value.node_id(),
                None => None,
            };
            elements.push((name_id, next, source_member, target_member));
        }
        let mut reported = false;
        for (name_id, next, source_member, target_member) in elements {
            let before = self.diagnostics.len();
            if let Some(next) = next {
                self.check_excess_properties(target_member, next);
            }
            if self.diagnostics.len() != before {
                reported = true;
                continue;
            }
            reported |= if let Some(next) = next {
                self.report_assignability_failure(name_id, next, source_member, target_member)
            } else {
                let span = self.error_span(name_id);
                self.report_relation_failure(
                    name_id,
                    span,
                    None,
                    source_member,
                    target_member,
                    None,
                )
            };
        }
        let children = self.elaborate_jsx_children(attributes, source, target)?;
        Some(reported || children)
    }

    /// `elaborateJsxComponents`' children half (`jsx.go:306-363`): for an
    /// opening element whose `JsxElement` has semantic children, the target's
    /// `children` member (`getIndexedAccessType`) is split into the parts
    /// assignable to `Iterable<any>` — or, without a global `Iterable`,
    /// `isArrayOrTupleLikeType` — and the rest. Several children against an
    /// array-like part elaborate element-wise
    /// (`elaborateIterableOrArrayLikeTargetElementwise`); against none, a
    /// failed `children` relation is TS2746 on the tag name. One child
    /// against a non-array-like part is `elaborateElement` (TS2747 for text);
    /// against none, TS2745. `Some(reported)`; `None` where a relation,
    /// member or iteration type this port cannot decide would choose the line.
    fn elaborate_jsx_children(
        &mut self,
        attributes: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let Some(opening) = self.nodes.parent(attributes) else { return Some(false) };
        let Some(Node::JsxOpeningElement(opening_node)) = self.node_map.get(opening) else {
            return Some(false);
        };
        let Some(Node::JsxElement(element)) =
            self.nodes.parent(opening).and_then(|parent| self.node_map.get(parent))
        else {
            return Some(false);
        };
        if element.opening_element.and_then(|node| node.node_id) != Some(opening) {
            return Some(false);
        }
        let valid: Vec<_> = element
            .children
            .iter()
            .copied()
            .filter(crate::jsx_intrinsic::semantic_jsx_child)
            .collect();
        if valid.is_empty() {
            return Some(false);
        }
        // `getJsxElementChildrenPropertyName`, with `InternalSymbolNameMissing`
        // read as `"children"`. A present container this port cannot read
        // (`""`, or more than one property) declines.
        let name = match self.jsx_children_name(attributes) {
            Some(name) => name,
            None if self.jsx_type_symbol(attributes, "ElementChildrenAttribute").is_none() => {
                "children".to_string()
            }
            None => return None,
        };
        let tag = opening_node.tag_name.and_then(|tag| tag.node_id())?;
        // `getIndexedAccessType(target, "children")` without an access node:
        // a target with no such member answers `unknown`, which nothing
        // below reports against (`getBestMatchIndexedAccessTypeOrUndefined`
        // misses it; every type relates to `unknown`).
        let Some(children_target) = self.get_type_of_property_of_type(target, &name) else {
            return Some(false);
        };
        let parts = match self.store.get(children_target).data.clone() {
            crate::types::TypeData::Union { types, .. } => types,
            _ => vec![children_target],
        };
        let iterable = self.global_type_symbol_with_arity("Iterable", 3).map(|iterable| {
            self.create_type_reference(
                iterable,
                vec![self.intrinsics.any, self.intrinsics.void, self.intrinsics.undefined],
            )
        });
        let mut array_like = Vec::new();
        let mut non_array_like = Vec::new();
        for part in parts {
            let is = match iterable {
                Some(iterable) => match self.relate_ternary(part, iterable, Relation::Assignable) {
                    Ternary::Related => true,
                    Ternary::NotRelated => false,
                    Ternary::Unknown => return None,
                },
                None => self.jsx_is_array_or_tuple_like(part)?,
            };
            if is {
                array_like.push(part);
            } else {
                non_array_like.push(part);
            }
        }
        let child_type = |checker: &mut Self| checker.get_type_of_property_of_type(source, &name);
        if valid.len() > 1 {
            if !array_like.is_empty() {
                let array_like = self.get_union_type(&array_like);
                return self.elaborate_jsx_children_elementwise(element.children, array_like);
            }
            let source_children = child_type(self)?;
            return match self.relate_ternary(source_children, children_target, Relation::Assignable)
            {
                Ternary::Related => Some(false),
                Ternary::Unknown => None,
                Ternary::NotRelated => {
                    let text = self.type_to_string(children_target);
                    self.report_jsx_children_arity(
                        tag,
                        &messages::THIS_JSX_TAG_S_0_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_1_BUT_MULTIPLE_CHILDREN_WERE_PROVIDED,
                        vec![name, text],
                    );
                    Some(true)
                }
            };
        }
        if non_array_like.is_empty() {
            let source_children = child_type(self)?;
            return match self.relate_ternary(source_children, children_target, Relation::Assignable)
            {
                Ternary::Related => Some(false),
                Ternary::Unknown => None,
                Ternary::NotRelated => {
                    let text = self.type_to_string(children_target);
                    self.report_jsx_children_arity(
                        tag,
                        &messages::THIS_JSX_TAG_S_0_PROP_EXPECTS_TYPE_1_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SINGLE_CHILD_WAS_PROVIDED,
                        vec![name, text],
                    );
                    Some(true)
                }
            };
        }
        // `elaborateElement(source, target, …, "children")`: the target member
        // through `getBestMatchIndexedAccessTypeOrUndefined`, whose union arm
        // (`getBestMatchingType`) this port does not model.
        if self.store.get(target).flags.intersects(TypeFlags::UNION) {
            return None;
        }
        // "Don't elaborate on indexes on generic variables".
        if self.store.get(children_target).flags.intersects(TypeFlags::INDEXED_ACCESS) {
            return Some(false);
        }
        let source_children = child_type(self)?;
        match self.relate_ternary(source_children, children_target, Relation::Assignable) {
            Ternary::Related => return Some(false),
            Ternary::Unknown => return None,
            Ternary::NotRelated => {}
        }
        let child = valid[0];
        self.report_jsx_child_failure(child, tag, &name, source_children, children_target)
    }

    /// `elaborateIterableOrArrayLikeTargetElementwise` (`jsx.go:419`) over
    /// `generateJsxChildren` (`jsx.go:373`): each child that is not
    /// whitespace-only text takes the next numeric index (an empty `{}`
    /// expression keeps its index, as upstream's counter does); its target is
    /// the iterated type of the non-array-like parts unioned with the
    /// array-like parts' element at that index, its source the child's own
    /// type (`checkJsxChildren`'s tuple element).
    fn elaborate_jsx_children_elementwise(
        &mut self,
        children: &[tsr_ast::JsxChild<'_>],
        target: TypeId,
    ) -> Option<bool> {
        let parts = match self.store.get(target).data.clone() {
            crate::types::TypeData::Union { types, .. } => types,
            _ => vec![target],
        };
        let mut tuple_like = Vec::new();
        let mut rest = Vec::new();
        for part in parts {
            if self.jsx_is_array_or_tuple_like(part)? {
                tuple_like.push(part);
            } else {
                rest.push(part);
            }
        }
        // `getBestMatchIndexedAccessTypeOrUndefined` over a union of
        // array-likes needs `getBestMatchingType`; not modelled.
        if tuple_like.len() > 1 {
            return None;
        }
        let iteration = if rest.is_empty() {
            None
        } else {
            let rest = self.get_union_type(&rest);
            let types = self
                .get_iteration_types_of_iterable(rest, crate::iteration::IterationUse::FOR_OF)
                .ok()?;
            Some(types.yield_type?)
        };
        let mut reported = false;
        let mut index = 0usize;
        for child in children {
            if let tsr_ast::JsxChild::JsxText(text) = child
                && text.contains_only_trivia_white_spaces
            {
                continue;
            }
            let position = index;
            index += 1;
            let indexed = match tuple_like.first() {
                Some(&array) => {
                    let key = self.store.intern_literal(
                        TypeFlags::NUMBER_LITERAL,
                        crate::types::TypeData::NumberLiteral(position.to_string()),
                        false,
                    );
                    self.array_or_tuple_element_access(array, key, false)
                }
                None => None,
            };
            let indexed = indexed
                .filter(|&ty| !self.store.get(ty).flags.intersects(TypeFlags::INDEXED_ACCESS));
            let target_member = match (iteration, indexed) {
                (Some(iteration), Some(indexed)) => self.get_union_type(&[iteration, indexed]),
                (Some(only), None) | (None, Some(only)) => only,
                (None, None) => continue,
            };
            let source_member = match child {
                tsr_ast::JsxChild::JsxText(_) => self.intrinsics.string,
                tsr_ast::JsxChild::JsxExpression(expression) if expression.expression.is_none() => {
                    continue;
                }
                _ => {
                    let expression = Expression::try_from(Node::from(*child)).ok()?;
                    self.check_expression_for_mutable_location(expression)
                }
            };
            match self.relate_ternary(source_member, target_member, Relation::Assignable) {
                Ternary::Related => continue,
                Ternary::Unknown => return None,
                Ternary::NotRelated => {}
            }
            let tag = self.jsx_children_tag(child)?;
            let name = self.jsx_children_name_for(child)?;
            reported |=
                self.report_jsx_child_failure(*child, tag, &name, source_member, target_member)?;
        }
        Some(reported)
    }

    /// The tag name and children property for a child's own containing
    /// element, for the TS2747 text.
    fn jsx_children_tag(&mut self, child: &tsr_ast::JsxChild<'_>) -> Option<NodeId> {
        let element = self.nodes.parent(child.node_id()?)?;
        let Some(Node::JsxElement(element)) = self.node_map.get(element) else { return None };
        element.opening_element?.tag_name?.node_id()
    }

    fn jsx_children_name_for(&mut self, child: &tsr_ast::JsxChild<'_>) -> Option<String> {
        let element = self.nodes.parent(child.node_id()?)?;
        let opening = match self.node_map.get(element) {
            Some(Node::JsxElement(element)) => element.opening_element?.node_id?,
            _ => return None,
        };
        match self.jsx_children_name(opening) {
            Some(name) => Some(name),
            None if self.jsx_type_symbol(opening, "ElementChildrenAttribute").is_none() => {
                Some("children".to_string())
            }
            None => None,
        }
    }

    /// `elaborateElement`'s report for one child (`getElaborationElementForJsxChild`,
    /// `jsx.go:390`): text reports TS2747 on the text through the custom
    /// diagnostic factory; an expression child reports on the `JsxExpression`
    /// with its inner expression elaborated; an element child is both the
    /// error node and the expression.
    fn report_jsx_child_failure(
        &mut self,
        child: tsr_ast::JsxChild<'_>,
        tag: NodeId,
        name: &str,
        source: TypeId,
        target: TypeId,
    ) -> Option<bool> {
        let at = child.node_id()?;
        match child {
            tsr_ast::JsxChild::JsxText(_) => {
                let tag_text = self.jsx_tag_text(tag);
                let target_text = self.type_to_string(target);
                self.report_jsx_children_arity(
                    at,
                    &messages::_0_COMPONENTS_DON_T_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BUT_THE_EXPECTED_TYPE_OF_1_IS_2,
                    vec![tag_text, name.to_string(), target_text],
                );
                Some(true)
            }
            tsr_ast::JsxChild::JsxExpression(expression) => {
                let Some(next) = expression.expression.and_then(|inner| inner.node_id()) else {
                    let span = self.error_span(at);
                    return Some(
                        self.report_relation_failure(at, span, None, source, target, None),
                    );
                };
                let before = self.diagnostics.len();
                self.check_excess_properties(target, next);
                Some(
                    self.diagnostics.len() != before
                        || self.report_assignability_failure(at, next, source, target),
                )
            }
            _ => Some(self.report_assignability_failure(at, at, source, target)),
        }
    }

    fn report_jsx_children_arity(
        &mut self,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        args: Vec<String>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// `isArrayOrTupleLikeType` (`checker.go:23556`): `isArrayLikeType`
    /// (an array reference, or a non-nullable type assignable to
    /// `readonly any[]`) or `isTupleLikeType`'s `"0"` member. `None` where
    /// the readonly-array relation is undecided.
    fn jsx_is_array_or_tuple_like(&mut self, ty: TypeId) -> Option<bool> {
        if self.binding_parent_is_array_like(ty)? {
            return Some(true);
        }
        Some(self.get_property_of_type(ty, "0").is_some())
    }

    /// Whether `isComparingJsxAttributes` can decide this relation for a
    /// source with these hyphenated members: some object constituent of the
    /// target (through unions and intersections) has index signatures, which
    /// `membersRelatedToIndexInfo` (`relater.go:4645`) does not relate a
    /// hyphenated member to, or may be a weak type that lacks one of them —
    /// `hasCommonProperties` (`relater.go:697`) counts such a member as
    /// common where the flagless check does not. A weak constituent declaring
    /// every hyphenated name knows them either way. An unreadable member or
    /// index table answers `true`. Every other relater rule treats a
    /// hyphenated member as an ordinary property, so outside these targets
    /// this port's flagless relation is upstream's.
    fn jsx_hyphen_sensitive_target(&mut self, target: TypeId, hyphenated: &[String]) -> bool {
        match self.store.get(target).data.clone() {
            crate::types::TypeData::Union { types, .. }
            | crate::types::TypeData::Intersection { types, .. } => {
                types.into_iter().any(|part| self.jsx_hyphen_sensitive_target(part, hyphenated))
            }
            _ if !self.store.get(target).flags.intersects(TypeFlags::OBJECT) => false,
            _ => {
                if self.get_index_infos_of_type(target).is_none_or(|infos| !infos.is_empty()) {
                    return true;
                }
                let Some(table) = self.relation_property_table(target) else { return true };
                table.iter().all(|(_, optional)| *optional)
                    && hyphenated.iter().any(|name| !table.iter().any(|(seen, _)| seen == name))
            }
        }
    }

    /// Whether the file selects the automatic runtime
    /// (`GetJSXImplicitImportBase`) but its module does not resolve here
    /// ([`Checker::jsx_implicit_import_container`]).
    fn jsx_implicit_import_unresolved(&mut self, location: NodeId) -> bool {
        let Some(file) = self.source_file_of_for_diagnostics(location) else { return false };
        let Some(host) = self.module_host else { return false };
        host.jsx_implicit_import_base(file).is_some()
            && self.jsx_implicit_import_container(location).is_none()
    }

    /// Whether every object constituent of `target` (through unions and
    /// intersections) has a complete member table
    /// ([`Checker::relation_members_are_complete`]), so a name it lacks is
    /// excess and not merely unenumerated.
    fn jsx_target_members_complete(&mut self, target: TypeId) -> bool {
        match self.store.get(target).data.clone() {
            crate::types::TypeData::Union { types, .. }
            | crate::types::TypeData::Intersection { types, .. } => {
                types.into_iter().all(|part| self.jsx_target_members_complete(part))
            }
            _ if !self.store.get(target).flags.intersects(TypeFlags::OBJECT) => true,
            _ => {
                if self.relation_members_are_complete(target) {
                    return true;
                }
                // An instantiated class or interface reference enumerates its
                // members through its (generic) heritage, which the declared
                // table refuses (§202) but `get_property_names_of_type` walks
                // with the instantiation; `IntrinsicClassAttributes<T>` is one.
                // A type-alias reference is not trusted: an alias whose body
                // this port does not resolve (`Defaultize<…>`) enumerates as an
                // empty object.
                let class_or_interface =
                    self.type_reference_targets.get(&target).is_some_and(|&(symbol, _)| {
                        self.binder
                            .symbols()
                            .get(symbol)
                            .flags
                            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                    });
                class_or_interface && self.get_property_names_of_type(target).is_some()
            }
        }
    }

    /// `hasExcessProperties` (`relater.go:2714`) for a JSX attributes source
    /// (`isComparingJsxAttributes`): the first written attribute — or the
    /// synthesized `children` member, which reports on the tag — that
    /// `isKnownProperty` does not find in the target. Hyphenated names are
    /// ignored (`isIgnoredJsxProperty`) and spread members are not checked
    /// (`shouldCheckAsExcessProperty`). `isEmptyObjectType` does not exempt a
    /// JSX source; `isTypeSubsetOf(globalObjectType, target)` does.
    /// `None` is undecided (a union target's `findMatchingDiscriminantType`,
    /// an incomplete member table).
    fn jsx_excess_attribute(
        &mut self,
        attributes: NodeId,
        source: TypeId,
        target: TypeId,
    ) -> Option<JsxExcess> {
        let Some(Node::JsxAttributes(node)) = self.node_map.get(attributes) else {
            return Some(JsxExcess::None);
        };
        // `isPerformingExcessPropertyChecks` needs `ObjectFlagsFreshLiteral`,
        // which `createJsxAttributesTypeFromAttributesProperty` (`jsx.go:722`)
        // adds to the shared flags only when it builds a chunk of written
        // attributes. Spreads and the children chunk are `getSpreadType`
        // results carrying the flags as they stand, so an element with no
        // written attribute — only spreads, only children — is not fresh.
        // A generic spread leaves an intersection, which is not an object
        // literal type (`isObjectLiteralType`).
        if !node.properties.iter().any(|p| matches!(p, JsxAttributeLike::JsxAttribute(_)))
            || !self.anonymous_properties.contains_key(&source)
        {
            return Some(JsxExcess::None);
        }
        if !self.is_excess_property_check_target(target) {
            return Some(JsxExcess::None);
        }
        if self.store.get(target).flags.intersects(TypeFlags::UNION) {
            return None;
        }
        if let Some(object) = self.global_type_symbol_with_arity("Object", 0)
            && self.get_declared_type_of_symbol(object) == target
        {
            return Some(JsxExcess::None);
        }
        let mut written: Vec<(Option<NodeId>, String)> = Vec::new();
        for attribute in node.properties {
            let JsxAttributeLike::JsxAttribute(attribute) = attribute else { continue };
            let Some(name) = attribute.name else { continue };
            let text = crate::jsx_intrinsic::jsx_attribute_name_text(name)?;
            if text.contains('-') || written.iter().any(|(_, seen)| *seen == text) {
                continue;
            }
            written.push((name.node_id(), text));
        }
        if let Some(children) = self.jsx_children_name(attributes)
            && !written.iter().any(|(_, seen)| *seen == children)
            && self.get_type_of_property_of_type(source, &children).is_some()
        {
            written.push((None, children));
        }
        for (at, name) in written {
            if !self.jsx_is_known_property(target, &name)? {
                return Some(JsxExcess::Member { at, name });
            }
        }
        Some(JsxExcess::None)
    }

    /// `isKnownProperty` (`relater.go:719`) with `isComparingJsxAttributes`:
    /// an object type's property or applicable index signature for the name,
    /// a hyphenated name, or — in an excess-property-check-target union or
    /// intersection — any constituent's. `None` where an object constituent's
    /// member table is incomplete and no constituent knows the name.
    fn jsx_is_known_property(&mut self, target: TypeId, name: &str) -> Option<bool> {
        if name.contains('-') {
            return Some(true);
        }
        let ty = self.store.get(target);
        if let crate::types::TypeData::Union { types, .. }
        | crate::types::TypeData::Intersection { types, .. } = &ty.data
        {
            if !self.is_excess_property_check_target(target) {
                return Some(false);
            }
            let types = types.clone();
            let mut undecided = false;
            for part in types {
                match self.jsx_is_known_property(part, name) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => undecided = true,
                }
            }
            return (!undecided).then_some(false);
        }
        if !ty.flags.intersects(TypeFlags::OBJECT) {
            return Some(false);
        }
        let names = self.get_property_names_of_type(target);
        if names.as_ref().is_some_and(|names| names.iter().any(|seen| seen == name)) {
            return Some(true);
        }
        let key = self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral(name.to_owned()),
            false,
        );
        if !self.get_index_infos_of_type(target)?.is_empty()
            && self.get_applicable_index_info(target, key).is_some()
        {
            return Some(true);
        }
        names.map(|_| false)
    }

    /// The JSX report of `hasExcessProperties`: `Property '{0}' does not
    /// exist on type '{1}'.` (or its `Did you mean` form through
    /// `getSuggestedSymbolForNonexistentJSXAttribute`, `jsx.go:484`: `for` →
    /// `htmlFor`, `class` → `className`, else the spelling suggestion) under
    /// `checkTypeRelatedToEx`'s TS2322 head, on the attribute name — or on
    /// the tag name for `children`, which is not a `JsxAttribute`.
    fn report_jsx_excess_attribute(
        &mut self,
        at: NodeId,
        name: &str,
        source: TypeId,
        target: TypeId,
    ) {
        let error_target =
            self.filter_type(target, |checker, t| checker.is_excess_property_check_target(t));
        let candidates = self.get_property_names_of_type(error_target).unwrap_or_default();
        let jsx_specific = match name {
            "for" => candidates.iter().find(|c| *c == "htmlFor").cloned(),
            "class" => candidates.iter().find(|c| *c == "className").cloned(),
            _ => None,
        };
        // `symbolToString(suggestion)`: a non-identifier name prints quoted.
        let suggestion = jsx_specific
            .or_else(|| {
                let candidates: Vec<&str> = candidates.iter().map(String::as_str).collect();
                crate::check::spelling_suggestion(name, &candidates).map(str::to_string)
            })
            .map(|suggestion| crate::jsx_intrinsic::jsx_printed_member_name(&suggestion));
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let printed = self.type_to_string(error_target);
        let detail = match suggestion {
            Some(suggestion) => Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1_DID_YOU_MEAN_2,
                span,
                [name.to_string(), printed, suggestion],
            ),
            None => Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [name.to_string(), printed],
            ),
        };
        let mut diagnostic = Diagnostic::with_args(
            &messages::TYPE_0_IS_NOT_ASSIGNABLE_TO_TYPE_1,
            span,
            [self.type_to_string(source), self.type_to_string(target)],
        );
        diagnostic.add_message_chain(Some(detail));
        self.report(file, diagnostic);
    }

    /// Whether `getUninstantiatedJsxSignaturesOfType` (`jsx.go:898`) answers
    /// an empty list for a non-string tag type: the apparent type's construct
    /// signatures, else its call signatures, else — for a union — the union
    /// of each constituent's list, which `getUnionSignatures`
    /// (`checker.go:21112`) makes empty as soon as one constituent's list is.
    /// `None` when a list is unresolved, a constituent is a string or string
    /// literal (the intrinsic-table arm), or every constituent has
    /// signatures (whether they combine is `getUnionSignatures`' matching,
    /// not answered here).
    fn jsx_uninstantiated_signatures_are_empty(&mut self, tag_type: TypeId) -> Option<bool> {
        if self.store.get(tag_type).flags.intersects(TypeFlags::STRING | TypeFlags::STRING_LITERAL)
        {
            return None;
        }
        // A qualified reference to a type alias is minted as a print-only
        // named type whose member table is the ALIAS symbol
        // (`declared.rs`, QUALIFIED-TYPEREF mint); the signature resolver
        // reads no call or construct member from a type-alias declaration and
        // answers a complete empty list that is not upstream's
        // (`React.SFC` in `reactSFCAndFunctionResolvable`). Not evidence.
        // Removable once that list answers `None` or the reference resolves
        // through `getTypeReferenceType` (`docs/parity/notes/r4-jsx.md` §2).
        let apparent = self.apparent_type(tag_type);
        if let crate::types::TypeData::Named { members: Some(symbol), .. } =
            self.store.get(apparent).data
            && self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE_ALIAS)
        {
            return None;
        }
        if !self.signatures_of_type_kind(tag_type, SignatureKind::Construct)?.is_empty()
            || !self.call_signatures_of_type(tag_type)?.is_empty()
        {
            return Some(false);
        }
        let crate::types::TypeData::Union { types, .. } = self.store.get(apparent).data.clone()
        else {
            return Some(true);
        };
        let mut any_empty = false;
        for part in types {
            any_empty |= self.jsx_uninstantiated_signatures_are_empty(part)?;
        }
        any_empty.then_some(true)
    }

    /// The `elementTypeConstraint` branch of
    /// `checkJsxOpeningLikeElementOrOpeningFragment` (`jsx.go:140-150`): with
    /// `JSX.ElementType` in scope, the tag's type — the tag name as a string
    /// literal for an intrinsic tag (`isJsxIntrinsicTagName`), else
    /// `checkExpression(tagName)` — is related to it, and a failure reports
    /// TS2786 on the tag name chained over `Its type '{0}' is not a valid JSX
    /// element type.` (the relation's own elaboration under that head is not
    /// modelled). Only a definite `NotRelated` reports; an undecided relation
    /// declines.
    fn check_jsx_element_type_constraint(
        &mut self,
        tag: JsxTagNameExpression<'_>,
        tag_id: NodeId,
        constraint: TypeId,
    ) {
        let tag_type = if let Some(name) = crate::jsx_intrinsic::jsx_intrinsic_tag_text(tag) {
            self.store.intern_literal(
                crate::flags::TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name),
                false,
            )
        } else {
            let Ok(expression) = Expression::try_from(Node::from(tag)) else { return };
            self.check_expression(expression)
        };
        if self.is_error(tag_type)
            || self.relate_ternary(tag_type, constraint, Relation::Assignable)
                != Ternary::NotRelated
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(tag_id) else { return };
        let span = self.error_span(tag_id);
        let detail = Diagnostic::with_args(
            &messages::ITS_TYPE_0_IS_NOT_A_VALID_JSX_ELEMENT_TYPE,
            span,
            [self.type_to_string(tag_type)],
        );
        let text = self.jsx_tag_text(tag_id);
        self.report(
            file,
            Diagnostic::new_chain(
                Some(detail),
                &messages::_0_CANNOT_BE_USED_AS_A_JSX_COMPONENT,
                [text],
            ),
        );
    }

    /// `getJsxElementTypeTypeAt` (`jsx.go:1279`): `JSX.ElementType`
    /// (`getJsxElementTypeSymbol`, a type-meaning export of the JSX
    /// namespace) through `instantiateAliasOrInterfaceWithDefaults`
    /// (`jsx.go:1032`) with no written arguments — every type parameter
    /// takes its default. `None` when absent or `errorType`, as upstream;
    /// also `None` where this port cannot fill the defaults
    /// ([`Checker::instantiated_heritage_base`] declines), which skips the
    /// check rather than taking the other branch.
    pub(crate) fn jsx_element_type_type_at(&mut self, location: NodeId) -> Option<TypeId> {
        let symbol = self.jsx_type_symbol(location, "ElementType")?;
        let symbol = self.binder.merged_symbol(symbol);
        let symbol = if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
            let target = self.resolve_alias_fully(symbol);
            self.binder.merged_symbol(target)
        } else {
            symbol
        };
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE) {
            return None;
        }
        let ty = self.instantiated_heritage_base(symbol, &[], Some(location))?;
        (!self.is_error(ty)).then_some(ty)
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
        // A qualified reference to a type alias is minted as a print-only
        // named type whose member table is the ALIAS symbol
        // (`declared.rs`, QUALIFIED-TYPEREF mint); the signature resolver
        // reads no call or construct member from a type-alias declaration and
        // answers a complete empty list that is not upstream's
        // (`React.SFC` in `reactSFCAndFunctionResolvable`). Not evidence.
        // Removable once that list answers `None` or the reference resolves
        // through `getTypeReferenceType` (`docs/parity/notes/r4-jsx.md` §2).
        let apparent = self.apparent_type(tag_type);
        if let crate::types::TypeData::Named { members: Some(symbol), .. } =
            self.store.get(apparent).data
            && self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE_ALIAS)
        {
            return None;
        }
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
