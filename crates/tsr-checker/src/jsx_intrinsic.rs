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
pub(crate) fn is_intrinsic_jsx_name(name: &str) -> bool {
    let Some(first) = name.chars().next() else { return false };
    first.is_ascii_lowercase() || name.contains('-')
}

impl Checker<'_, '_> {
    /// Pinned tsgo 5b1047d, jsx.go getJsxType/getJsxNamespaceAt. Factory
    /// namespace selection precedes the global fallback, including Element.
    pub(crate) fn jsx_type_symbol(&mut self, location: NodeId, name: &str) -> Option<SymbolId> {
        let namespace = self
            .jsx_namespace_symbol(location)
            .or_else(|| self.binder.globals().get(JSX).copied())?;
        let namespace = self.binder.merged_symbol(namespace);
        self.binder.symbols().get(namespace).exports.get(name).copied()
    }

    fn jsx_container_property(&mut self, location: NodeId, name: &str) -> Option<String> {
        let symbol = self.jsx_type_symbol(location, name)?;
        let ty = self.get_declared_type_of_symbol(symbol);
        let names = self.get_property_names_of_type(ty)?;
        match names.as_slice() {
            [] => Some(String::new()),
            [name] => Some(name.clone()),
            _ => None,
        }
    }

    pub(crate) fn jsx_children_name(&mut self, location: NodeId) -> Option<String> {
        if matches!(self.jsx_emit, tsr_core::JsxEmit::ReactJsx | tsr_core::JsxEmit::ReactJsxDev) {
            return Some("children".to_string());
        }
        self.jsx_container_property(location, "ElementChildrenAttribute")
            .filter(|name| !name.is_empty())
    }

    /// getContextualJsxElementAttributesType / inferJsxTypeArguments (jsx.go).
    /// Private Checker tables use the opening-like `NodeId`, not the tag symbol:
    /// the same component at two elements must have independent fixing state.
    /// Active contexts are attached to the containing element (jsx.go moves
    /// contextualInfos there so sibling body children can find the mapper).
    /// Active entries expose the candidate's uninstantiated props; completed
    /// signatures publish only after both passes and the final mapper succeed.
    /// `resolving_signature_calls` blocks re-entry, never publishes an assumption.
    /// The worker makes a skip-context-sensitive image, then checks attributes
    /// and semantic children in source order. Completed attribute callbacks feed
    /// the canonical intra-expression sites; `live_contextual_mapper` consumes
    /// those sites before fixing an input. The context is removed on every exit.
    /// Existing resolved signatures avoid repeating those walks. No new cache,
    /// cross-Checker reuse, or performance claim; work attribution is tsr-1yb.11.
    /// Overload selection and implicit runtime namespaces remain declined.
    pub(crate) fn jsx_attributes_context(
        &mut self,
        opening: NodeId,
    ) -> Option<crate::types::TypeId> {
        let context_node = self.jsx_inference_context_node(opening);
        if let Some(context) = self.active_inference_contexts.get(&context_node) {
            let first = context.signature.parameters.first().cloned();
            return first.map(|parameter| self.parameter_type(&parameter));
        }
        if let Some(signature) = self.resolved_call_signatures.get(&opening) {
            let first = signature.parameters.first().cloned();
            return first.map(|parameter| self.parameter_type(&parameter));
        }
        if !self.resolving_signature_calls.insert(opening) {
            return None;
        }
        let result = self.resolve_jsx_attributes_context(opening);
        self.resolving_signature_calls.remove(&opening);
        result
    }

    fn jsx_inference_context_node(&self, opening: NodeId) -> NodeId {
        match self.nodes.parent(opening).and_then(|parent| self.node_map.get(parent)) {
            Some(Node::JsxElement(element))
                if element.opening_element.and_then(|node| node.node_id) == Some(opening) =>
            {
                element.node_id.unwrap_or(opening)
            }
            _ => opening,
        }
    }

    fn resolve_jsx_attributes_context(&mut self, opening: NodeId) -> Option<crate::types::TypeId> {
        use crate::{
            inference::{InferenceContextSnapshot, InferenceFlags},
            signatures::SignatureKind,
        };
        use tsr_ast::{Expression, JsxTagNameExpression};
        let (tag, arguments) = match self.node_map.get(opening)? {
            Node::JsxOpeningElement(node) => (node.tag_name?, node.type_arguments),
            Node::JsxSelfClosingElement(node) => (node.tag_name?, node.type_arguments),
            _ => return None,
        };
        if let JsxTagNameExpression::Identifier(name) = tag
            && is_intrinsic_jsx_name(name.text)
        {
            let symbol = self.jsx_type_symbol(opening, INTRINSIC_ELEMENTS)?;
            let table = self.get_declared_type_of_symbol(symbol);
            return self.get_type_of_property_of_type(table, name.text).or_else(|| {
                // getIntrinsicAttributesTypeFromJsxOpeningLikeElement
                // (pinned 5b1047d jsx.go:1190) uses the actual tag key, so
                // overlapping pattern indexes retain their intersection.
                let key = self.store.intern_literal(
                    crate::flags::TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(name.text.to_owned()),
                    false,
                );
                self.get_applicable_index_info(table, key).map(|info| info.value)
            });
        }
        let expression = Expression::try_from(Node::from(tag)).ok()?;
        let ty = self.check_expression(expression);
        let mut signatures = self.signatures_of_type_kind(ty, SignatureKind::Construct)?;
        let class_reference =
            signatures.first().is_some_and(|signature| signature.kind == SignatureKind::Construct);
        if signatures.is_empty() {
            signatures = self.call_signatures_of_type(ty)?;
        }
        if signatures.is_empty() {
            // getUninstantiatedJsxSignaturesOfType (5b1047d jsx.go:898):
            // only completed empty WHOLE-tag construct/call sets permit the
            // mixed union fallback. Retain the original tag/alias for managed
            // attributes. Constituents supply completed, nongeneric singleton
            // vectors; their declaration/argument vectors are never rewritten.
            let apparent = self.apparent_type(ty);
            let body = self.binding_type_alias_body(apparent);
            let crate::types::TypeData::Union { types, .. } = self.store.get(body).data.clone()
            else {
                return None;
            };
            let mut lists = Vec::with_capacity(types.len());
            for part in types {
                if self.store.get(part).flags.intersects(crate::flags::TypeFlags::ANY_OR_UNKNOWN)
                    || self.is_error(part)
                {
                    return None;
                }
                let mut preferred = self.signatures_of_type_kind(part, SignatureKind::Construct)?;
                if preferred.is_empty() {
                    preferred = self.call_signatures_of_type(part)?;
                }
                let [signature] = preferred.as_mut_slice() else { return None };
                if !signature.type_parameters.is_empty()
                    || signature.kind == SignatureKind::AbstractConstruct
                    || signature.union_contains_abstract
                    || self.is_error(signature.r#type)
                    || signature.parameters.iter().any(|parameter| {
                        let parameter_type = self.parameter_type(parameter);
                        self.is_error(parameter_type)
                    })
                {
                    return None;
                }
                // Native signature lists have no call/construct kind. Project
                // only these ephemeral JSX clones for the canonical combiner.
                // Overall getJsxReferenceKind (jsx.go:1159) is Mixed here, so
                // effective props use the first parameter, not instance props.
                signature.kind = SignatureKind::Call;
                lists.push(preferred);
            }
            signatures = self.union_signatures_of_lists(&lists)?;
            // This result belongs only to the opening NodeId below, never to
            // composite_signature_types[(tag TypeId, Call/Construct)]. Existing
            // resolver guards distinguish active, declined and completed work.
        }
        if signatures.len() != 1 {
            return None;
        }
        let mut signature = signatures.remove(0);
        let mut props = if class_reference {
            match self.jsx_container_property(opening, "ElementAttributesProperty") {
                None => signature.parameters.first().map(|p| self.parameter_type(p)),
                Some(name) if name.is_empty() => Some(signature.r#type),
                Some(name) => self.get_type_of_property_of_type(signature.r#type, &name),
            }
        } else {
            signature.parameters.first().map(|p| self.parameter_type(p))
        }
        .unwrap_or(self.intrinsics.unknown);
        if let Some(managed) = self.jsx_type_symbol(opening, "LibraryManagedAttributes") {
            props = self.evaluate_alias_body(managed, &[ty, props])?;
        }
        if class_reference
            && let Some(intrinsic) = self.jsx_type_symbol(opening, "IntrinsicClassAttributes")
        {
            let intrinsic = match self.local_type_parameters_of(intrinsic).len() {
                0 => self.get_declared_type_of_symbol(intrinsic),
                1 => self.create_type_reference(intrinsic, vec![signature.r#type]),
                _ => return None,
            };
            props = self.get_intersection_type(&[intrinsic, props], None);
        }
        if let Some(intrinsic) = self.jsx_type_symbol(opening, "IntrinsicAttributes") {
            let intrinsic = self.get_declared_type_of_symbol(intrinsic);
            props = self.get_intersection_type(&[intrinsic, props], None);
        }
        signature.parameters =
            vec![crate::signatures::Parameter::new("props".to_string(), false, false, props, None)];
        if signature.type_parameters.is_empty() {
            self.resolved_call_signatures.insert(opening, signature);
            return Some(props);
        }
        let parameters = self.type_parameter_types(&signature)?;
        let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        if !arguments.is_empty() {
            if arguments.len() != parameters.len() {
                return None;
            }
            let map: Vec<_> = parameters
                .iter()
                .zip(arguments)
                .map(|(&p, &t)| (p, self.get_type_from_type_node(t)))
                .collect();
            let mut resolved =
                self.instantiate_signature(signature.clone(), &map, &parameters, &names)?;
            resolved.type_parameters.clear();
            let props = self.parameter_type(&resolved.parameters[0]);
            self.resolved_call_signatures.insert(opening, resolved);
            return Some(props);
        }
        let context_node = self.jsx_inference_context_node(opening);
        self.active_inference_contexts.insert(
            context_node,
            InferenceContextSnapshot {
                signature: signature.clone(),
                inferences: Vec::new(),
                return_inferences: Vec::new(),
                flags: if self.in_js_file(opening) {
                    InferenceFlags::ANY_DEFAULT
                } else {
                    InferenceFlags::NONE
                },
                inferential: false,
                intra_expression_sites: Vec::new(),
                outer_return_map: None,
            },
        );
        let result = (|| {
            let source = self.jsx_attributes_inference_type(opening, true)?;
            let mut infos = Vec::new();
            self.infer_from_types(source, props, &parameters, &mut infos, 0);
            let context = self.active_inference_contexts.get_mut(&context_node)?;
            context.inferences = infos;
            context.inferential = true;
            let source = self.jsx_attributes_inference_type(opening, false)?;
            let context = self.active_inference_contexts.get(&context_node)?.clone();
            let mut infos = context.inferences;
            self.infer_from_types(source, props, &parameters, &mut infos, 0);
            let map =
                self.resolved_inference_map(&infos, &signature, &parameters, context.flags)?;
            let mut resolved =
                self.instantiate_signature(signature.clone(), &map, &parameters, &names)?;
            resolved.type_parameters.clear();
            let props = self.parameter_type(&resolved.parameters[0]);
            self.resolved_call_signatures.insert(opening, resolved);
            Some(props)
        })();
        self.active_inference_contexts.remove(&context_node);
        result
    }

    fn jsx_inference_expression(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        skip: bool,
    ) -> Option<crate::types::TypeId> {
        if skip {
            if let tsr_ast::Expression::JsxExpression(node) = expression {
                return self.jsx_inference_expression(node.expression?, true);
            }
            if let Some(id) = expression.node_id()
                && matches!(
                    expression,
                    tsr_ast::Expression::ArrowFunction(_)
                        | tsr_ast::Expression::FunctionExpression(_)
                )
                && let Some(ty) = self.context_free_function_type(id)
            {
                // SkipContextSensitive checks a return-only producer without
                // fixing its context. Its inferred return follows the ordinary
                // function-return literal widening, not a fresh literal source.
                if let Some(mut signatures) = self.signature_types.get(&ty).cloned() {
                    for signature in &mut signatures {
                        signature.r#type = self.get_widened_literal_type(signature.r#type);
                    }
                    self.signature_types.insert(ty, signatures);
                }
                return Some(ty);
            }
            return self.context_free_object_inference_type(expression);
        }
        let ty = self.check_expression_for_mutable_location(expression);
        (ty != self.intrinsics.error).then_some(ty)
    }

    fn jsx_attributes_inference_type(
        &mut self,
        opening: NodeId,
        skip: bool,
    ) -> Option<crate::types::TypeId> {
        use tsr_ast::{Expression, JsxAttributeLike, JsxAttributeName};
        let attributes = match self.node_map.get(opening)? {
            Node::JsxOpeningElement(node) => node.attributes?,
            Node::JsxSelfClosingElement(node) => node.attributes?,
            _ => return None,
        };
        // createJsxAttributesTypeFromAttributesProperty (jsx.go:742/853)
        // carries propagating flags from expressions, including placeholders.
        // The source is still partially inferable through its other properties.
        let mut non_inferrable = false;
        let mut properties: Vec<crate::objects::AnonymousProperty> = Vec::new();
        for attribute in attributes.properties {
            let property = match attribute {
                JsxAttributeLike::JsxAttribute(node) => {
                    let JsxAttributeName::Identifier(name) = node.name? else { return None };
                    let expression = node
                        .initializer
                        .and_then(|value| Expression::try_from(Node::from(value)).ok());
                    let ty = match expression {
                        Some(expression) => self.jsx_inference_expression(expression, skip)?,
                        None => self.intrinsics.true_type,
                    };
                    non_inferrable |= self.non_inferrable_types.contains(&ty);
                    if !skip
                        && let Some(Expression::JsxExpression(wrapper)) = expression
                        && let Some(inner) = wrapper.expression
                        && self.is_context_sensitive_argument(&inner)
                        && let Some(id) = inner.node_id()
                    {
                        self.add_intra_expression_inference_site(id, ty);
                    }
                    crate::objects::AnonymousProperty {
                        name: name.text.to_string(),
                        printed_name: name.text.to_string(),
                        printed_type: self.type_to_string(ty),
                        r#type: ty,
                        origin: node.node_id.and_then(|id| self.binder.symbol_of(id)),
                        checked_declaration: None,
                        optional: false,
                        readonly: false,
                        method: false,
                        accessor_write: None,
                    }
                }
                JsxAttributeLike::JsxSpreadAttribute(node) => {
                    let ty = self.check_expression(node.expression?);
                    non_inferrable |= self.non_inferrable_types.contains(&ty);
                    let (spread, _) = self.spread_properties(ty, false)?;
                    for property in spread {
                        if let Some(index) = properties.iter().position(|p| p.name == property.name)
                        {
                            properties[index] = property;
                        } else {
                            properties.push(property);
                        }
                    }
                    continue;
                }
            };
            if let Some(index) = properties.iter().position(|p| p.name == property.name) {
                properties[index] = property;
            } else {
                properties.push(property);
            }
        }
        if let Some(parent) = self.nodes.parent(opening)
            && let Some(Node::JsxElement(element)) = self.node_map.get(parent)
            && element.opening_element.and_then(|node| node.node_id) == Some(opening)
        {
            let children: Vec<_> =
                element.children.iter().copied().filter(semantic_jsx_child).collect();
            let mut types = Vec::new();
            let mut non_inferrable_children = false;
            for child in &children {
                let ty = match child {
                    tsr_ast::JsxChild::JsxText(_) => self.intrinsics.string,
                    _ => self.jsx_inference_expression(
                        Expression::try_from(Node::from(*child)).ok()?,
                        skip,
                    )?,
                };
                non_inferrable_children |= self.non_inferrable_types.contains(&ty);
                types.push(ty);
            }
            if !types.is_empty()
                && let Some(name) = self.jsx_children_name(opening)
            {
                let ty = if types.len() == 1 {
                    types[0]
                } else {
                    let props = self.jsx_attributes_context(opening)?;
                    let field = self.get_type_of_property_of_type(props, &name);
                    let parts = field
                        .map(|field| match self.store.get(field).data.clone() {
                            crate::types::TypeData::Union { types, .. } => types,
                            _ => vec![field],
                        })
                        .unwrap_or_default();
                    if parts.iter().any(|field| {
                        self.tuple_element_lists.contains_key(field)
                            || self.variadic_tuple_elements.contains_key(field)
                    }) {
                        self.create_tuple_type(types, false)
                    } else {
                        let element = self.get_union_type(&types);
                        let array = self.global_type_symbol("Array")?;
                        self.create_type_reference(array, vec![element])
                    }
                };
                if non_inferrable_children {
                    self.non_inferrable_types.insert(ty);
                    non_inferrable = true;
                }
                properties.retain(|p| p.name != name);
                properties.push(crate::objects::AnonymousProperty {
                    name: name.clone(),
                    printed_name: name,
                    printed_type: self.type_to_string(ty),
                    r#type: ty,
                    origin: None,
                    checked_declaration: None,
                    optional: false,
                    readonly: false,
                    method: false,
                    accessor_write: None,
                });
            }
        }
        let members = crate::callable_expandos::property_members(&properties);
        let ty = self.store.new_named(
            crate::flags::TypeFlags::OBJECT,
            crate::objects::render_object_type(&members),
            None,
        );
        if non_inferrable {
            self.non_inferrable_types.insert(ty);
        }
        self.anonymous_properties.insert(ty, (properties, true));
        self.object_literal_members.insert(ty, members);
        Some(ty)
    }

    /// Pinned jsx.go:267 / relater.go:1212. Attributes supply explicit values
    /// (including bare true); absent optional discriminants supply undefined,
    /// except the children field when semantic body children are present.
    /// Some is completed selection, including an unchanged union; None is an
    /// unsupported property/value/relation, never computed signature absence.
    /// Only non-generic primitive/unit discriminant metadata is certified here.
    /// The opening-like `NodeId` and contextual `TypeId` stay Checker-local; no new
    /// cache or provisional publication. This repeats the native selection walk
    /// per contextual query, retaining the existing opening-signature cache.
    /// Work attribution remains tsr-1yb.11, not a performance claim.
    fn discriminate_jsx_attributes(
        &mut self,
        opening: NodeId,
        contextual: crate::types::TypeId,
    ) -> Option<crate::types::TypeId> {
        use crate::{
            flags::TypeFlags,
            relater::{Relation, Ternary},
            types::TypeData,
        };
        use tsr_ast::{Expression, JsxAttributeLike, JsxAttributeName};
        let TypeData::Union { types, .. } = self.store.get(contextual).data.clone() else {
            return None;
        };
        let attributes = match self.node_map.get(opening)? {
            Node::JsxOpeningElement(node) => node.attributes?,
            Node::JsxSelfClosingElement(node) => node.attributes?,
            _ => return None,
        };
        let mut items = Vec::new();
        let mut present = Vec::new();
        for attribute in attributes.properties {
            let JsxAttributeLike::JsxAttribute(attribute) = attribute else { continue };
            let JsxAttributeName::Identifier(name) = attribute.name? else { return None };
            present.push(name.text);
            let expression = attribute
                .initializer
                .and_then(|value| Expression::try_from(Node::from(value)).ok());
            if expression.is_some_and(|expression| !possibly_jsx_discriminant_value(expression)) {
                continue;
            }
            if !self.jsx_is_discriminant_property(&types, name.text)? {
                continue;
            }
            let value = match expression {
                Some(expression) => self.jsx_discriminant_value_type(expression)?,
                None => self.intrinsics.true_type,
            };
            if value == self.intrinsics.error {
                return None;
            }
            items.push((name.text.to_string(), self.get_regular_type_of_literal_type(value)));
        }
        let semantic_children =
            self.nodes.parent(opening).and_then(|parent| self.node_map.get(parent)).is_some_and(
                |parent| {
                    matches!(parent, Node::JsxElement(element)
                if element.opening_element.and_then(|node| node.node_id) == Some(opening)
                    && element.children.iter().any(semantic_jsx_child))
                },
            );
        let children_name = self.jsx_children_name(opening);
        let mut common: Option<Vec<String>> = None;
        for &part in &types {
            if self.store.get(part).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
                continue;
            }
            let names = self.get_property_names_of_type(part)?;
            match &mut common {
                None => common = Some(names),
                Some(common) => common.retain(|name| names.contains(name)),
            }
        }
        for name in common.unwrap_or_default() {
            if present.contains(&name.as_str())
                || (semantic_children && children_name.as_deref() == Some(name.as_str()))
            {
                continue;
            }
            let mut optional = false;
            for &part in &types {
                // Pinned checker.go:21455-21479 ORs optionality for unions,
                // ANDs intersection contributors, and changes the flag only
                // for ClassMember roots. Name enumeration above certifies
                // declaration completion and declines mapped modifiers; raw
                // composite getPropertyOfType has no synthetic symbol here.
                let roots = self.intersection_property_symbols(part, &name);
                if roots.is_empty() {
                    return None;
                }
                let intersection =
                    matches!(self.store.get(part).data, TypeData::Intersection { .. });
                let mut part_optional = intersection;
                for root in roots {
                    if !self.binder.symbols().get(root).flags.intersects(SymbolFlags::CLASS_MEMBER)
                    {
                        continue;
                    }
                    if intersection {
                        part_optional &= self.property_is_optional(root);
                    } else {
                        part_optional |= self.property_is_optional(root);
                    }
                }
                optional |= part_optional;
            }
            if optional && self.jsx_is_discriminant_property(&types, &name)? {
                items.push((name, self.intrinsics.undefined));
            }
        }
        let mut include: Vec<_> = types
            .iter()
            .map(|&part| {
                !self.store.get(part).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER)
            })
            .collect();
        for (name, value) in items {
            let values = match self.store.get(value).data.clone() {
                TypeData::Union { types, .. } => types,
                _ => vec![value],
            };
            let mut matched = false;
            let mut maybe = Vec::new();
            for (index, &part) in types.iter().enumerate() {
                if !include[index] {
                    continue;
                }
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(name.clone()),
                    false,
                );
                let member = self
                    .get_type_of_property_of_type(part, &name)
                    .or_else(|| self.get_applicable_index_info(part, key).map(|info| info.value));
                let Some(member) = member else { continue };
                let mut accepts = false;
                for &value in &values {
                    match self.relate_ternary(value, member, Relation::Assignable) {
                        Ternary::Related => {
                            accepts = true;
                            break;
                        }
                        Ternary::NotRelated => {}
                        Ternary::Unknown => return None,
                    }
                }
                if accepts {
                    matched = true;
                } else {
                    maybe.push(index);
                }
            }
            if matched {
                for index in maybe {
                    include[index] = false;
                }
            }
        }
        let survivors: Vec<_> = types
            .iter()
            .enumerate()
            .filter_map(|(index, &part)| include[index].then_some(part))
            .collect();
        Some(if survivors.is_empty() || survivors.len() == types.len() {
            contextual
        } else {
            self.get_union_type_without_reduction(&survivors)
        })
    }

    /// getContextFreeTypeOfExpression uses a raw expression check, not the
    /// mutable-location inference image (which asks this contextual query
    /// again). Nonconstant templates still need native's scoped any-context;
    /// decline them until that override exists rather than recurse or widen.
    fn jsx_discriminant_value_type(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> Option<crate::types::TypeId> {
        use tsr_ast::Expression;
        match expression {
            Expression::JsxExpression(node) => self.jsx_discriminant_value_type(node.expression?),
            Expression::ParenthesizedExpression(node) => {
                self.jsx_discriminant_value_type(node.expression?)
            }
            Expression::TemplateExpression(_) => None,
            _ => Some(self.check_expression(expression)),
        }
    }

    /// isDiscriminantProperty's nonuniform/literal, nongeneric metadata slice.
    fn jsx_is_discriminant_property(
        &mut self,
        types: &[crate::types::TypeId],
        name: &str,
    ) -> Option<bool> {
        use crate::{flags::TypeFlags, types::TypeData};
        let mut first = None;
        let mut nonuniform = false;
        let mut literal = false;
        let mut primitive = true;
        for &part in types {
            let Some(member) = self.get_type_of_property_of_type(part, name) else { continue };
            let member = self.binding_type_alias_body(member);
            if member == self.intrinsics.error {
                return None;
            }
            let regular = self.get_regular_type_of_literal_type(member);
            if let Some(first) = first {
                nonuniform |= first != regular;
            } else {
                first = Some(regular);
            }
            let leaves = match self.store.get(member).data.clone() {
                TypeData::Union { types, .. } => types,
                _ => vec![member],
            };
            if leaves
                .iter()
                .any(|&part| self.store.get(part).flags.intersects(TypeFlags::INSTANTIABLE))
            {
                return None;
            }
            literal |= self.store.get(member).flags.contains(TypeFlags::BOOLEAN)
                || leaves
                    .iter()
                    .all(|&part| self.store.get(part).flags.intersects(TypeFlags::UNIT));
            primitive &= leaves.iter().all(|&part| {
                self.store.get(part).flags.intersects(
                    TypeFlags::PRIMITIVE | TypeFlags::ANY | TypeFlags::UNKNOWN | TypeFlags::NEVER,
                )
            });
        }
        if nonuniform && literal && !primitive {
            return None;
        }
        Some(nonuniform && literal)
    }

    pub(crate) fn jsx_attribute_context(
        &mut self,
        attribute: NodeId,
    ) -> Option<crate::types::TypeId> {
        let attributes = self.nodes.parent(attribute)?;
        let opening = self.nodes.parent(attributes)?;
        let props = self.jsx_attributes_context(opening)?;
        let context_sensitive = match self.node_map.get(attribute)? {
            Node::JsxAttribute(node) => node.initializer.is_some_and(|initializer| {
                tsr_ast::Expression::try_from(Node::from(initializer))
                    .is_ok_and(|expression| self.is_context_sensitive_argument(&expression))
            }),
            _ => false,
        };
        // getApparentTypeOfContextualType(attributes, Signature),
        // jsx.go:219/checker.go:30683, resolves instantiable operands through
        // the live non-fixing mapper before apparent type and property lookup.
        // Scalar context keeps its original generic type for literal inference.
        let props = if context_sensitive {
            self.instantiate_contextual_inference_type(props, attributes)
        } else {
            props
        };
        let props = self.apparent_contextual_type(props);
        if self.store.get(props).flags.contains(crate::flags::TypeFlags::ANY) {
            return None;
        }
        if let Node::JsxAttribute(node) = self.node_map.get(attribute)?
            && let tsr_ast::JsxAttributeName::Identifier(name) = node.name?
            && let Some(selected) = self.discriminate_jsx_attributes(opening, props)
        {
            return self.contextual_property_type(selected, name.text);
        }
        match self.node_map.get(attribute)? {
            Node::JsxAttribute(node) => {
                let tsr_ast::JsxAttributeName::Identifier(name) = node.name? else { return None };
                if context_sensitive {
                    self.certified_jsx_property_context(props, name.text)
                } else {
                    self.contextual_property_type(props, name.text)
                }
            }
            Node::JsxSpreadAttribute(_) => Some(props),
            _ => None,
        }
    }

    pub(crate) fn jsx_child_context(
        &mut self,
        element: NodeId,
        child: NodeId,
    ) -> Option<crate::types::TypeId> {
        let Node::JsxElement(node) = self.node_map.get(element)? else { return None };
        let opening = node.opening_element?.node_id?;
        let children: Vec<_> =
            node.children.iter().filter(|child| semantic_jsx_child(child)).collect();
        let index = children.iter().position(|node| node.node_id() == Some(child))?;
        let name = self.jsx_children_name(element)?;
        let props = self.jsx_attributes_context(opening)?;
        let props = self.apparent_contextual_type(props);
        if self.store.get(props).flags.contains(crate::flags::TypeFlags::ANY) {
            return None;
        }
        let field = if let Some(selected) = self.discriminate_jsx_attributes(opening, props) {
            self.contextual_property_type(selected, &name)?
        } else if tsr_ast::Expression::try_from(Node::from(*children[index]))
            .is_ok_and(|expression| self.is_context_sensitive_argument(&expression))
        {
            self.certified_jsx_property_context(props, &name)?
        } else {
            self.contextual_property_type(props, &name)?
        };
        if children.len() == 1 {
            return Some(field);
        }
        let parts = match self.store.get(field).data.clone() {
            crate::types::TypeData::Union { types, .. } => types,
            _ => vec![field],
        };
        let index = self.store.intern_literal(
            crate::flags::TypeFlags::NUMBER_LITERAL,
            crate::types::TypeData::NumberLiteral(index.to_string()),
            false,
        );
        let mut types = Vec::new();
        for part in parts {
            // jsx.go:241 maps array-like constituents, not arbitrary
            // iterables or numeric-indexed objects. Reuse checker.go:23520's
            // canonical alias-body/ReadonlyArray assignability consumer;
            // an unknown relation remains unsupported, not false.
            types.push(if self.binding_parent_is_array_like(part)? {
                self.resolved_indexed_access_type(part, index, false)?
            } else {
                part
            });
        }
        Some(self.get_union_type_without_reduction(&types))
    }

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
    /// The diagnostics of `checkJsxExpression` (pinned 5b1047d `jsx.go:89`):
    /// `checkGrammarJsxExpression` (`grammarchecks.go:1192`) — TS18007 on a
    /// comma sequence, through `grammarErrorOnNode`'s parse-error gate — then
    /// TS2609 on a spread child whose type is neither `any` nor an array type
    /// (`isArrayType`, `checker.go:23481`). Reached once per `JsxExpression`
    /// from the per-node walk; the type is the memoised `check_expression`,
    /// and a gap (`errorType` here) reports nothing.
    pub(crate) fn check_jsx_expression(&mut self, node: NodeId) {
        let Some(Node::JsxExpression(jsx)) = self.node_map.get(node) else { return };
        let Some(expression) = jsx.expression else { return };
        let Some(expression_id) = expression.node_id() else { return };
        if is_comma_sequence(expression) && !self.file_has_parse_errors {
            if let Some(file) = self.source_file_of_for_diagnostics(node) {
                let span = self.error_span(expression_id);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::JSX_EXPRESSIONS_MAY_NOT_USE_THE_COMMA_OPERATOR_DID_YOU_MEAN_TO_WRITE_AN_ARRAY,
                        span,
                    ),
                );
            }
        }
        if jsx.dot_dot_dot_token.is_none() {
            return;
        }
        let ty = self.check_expression(expression);
        if ty == self.intrinsics.any || self.is_error(ty) || self.is_jsx_array_type(ty) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(&messages::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE, span));
    }

    /// `isArrayType` (`checker.go:23481`): a reference to the global `Array`
    /// or `ReadonlyArray`.
    fn is_jsx_array_type(&mut self, ty: crate::types::TypeId) -> bool {
        let Some(&(target, _)) = self.type_reference_target(ty) else { return false };
        self.global_type_symbol("Array") == Some(target)
            || self.global_type_symbol("ReadonlyArray") == Some(target)
    }

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
            // §255 — `checkJsxElementDeferred` (`jsx.go:81`) resolves the
            // closing name too, and `getIntrinsicTagSymbol` is what reports.
            // The baseline is unambiguous: `<span>1</span>` wants TS7026 at the
            // opening `span` **and** at the closing one.
            Node::JsxClosingElement(element) => element.tag_name,
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

    /// TS2339 — `Property '{0}' does not exist on type
    /// 'JSX.IntrinsicElements'.`
    ///
    /// `getIntrinsicTagSymbol`'s "wasn't found" arm (`jsx.go:1228-1250`):
    /// with a `JSX.IntrinsicElements` type in hand, an intrinsic tag that is
    /// neither a property of it nor covered by an applicable index signature
    /// reports on the element, opening and closing tag alike (§255's node
    /// set). Unlike TS7026 this arm is not inside `noImplicitAny`.
    ///
    /// The table must be completely enumerable
    /// ([`Checker::get_property_names_of_type`] answering `Some`): a name
    /// missing from an unresolved table is not evidence, so that declines.
    pub(crate) fn check_jsx_intrinsic_tag_exists(&mut self, node: NodeId, typed: Node<'_>) {
        let tag = match typed {
            Node::JsxOpeningElement(element) => element.tag_name,
            Node::JsxSelfClosingElement(element) => element.tag_name,
            Node::JsxClosingElement(element) => element.tag_name,
            _ => return,
        };
        let Some(tag_id) = tag.and_then(|tag| tag.node_id()) else { return };
        let name = match self.node_map.get(tag_id) {
            Some(Node::Identifier(identifier)) if is_intrinsic_jsx_name(identifier.text) => {
                identifier.text.to_string()
            }
            Some(Node::JsxNamespacedName(namespaced)) => {
                match (namespaced.namespace, namespaced.name) {
                    (Some(namespace), Some(name)) => format!("{}:{}", namespace.text, name.text),
                    _ => return,
                }
            }
            _ => return,
        };
        let Some(symbol) = self.jsx_type_symbol(node, INTRINSIC_ELEMENTS) else { return };
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE) {
            return;
        }
        let table = self.get_declared_type_of_symbol(symbol);
        if self.is_error(table) {
            return;
        }
        let Some(names) = self.get_property_names_of_type(table) else { return };
        if names.contains(&name) {
            return;
        }
        let key = self.store.intern_literal(
            crate::flags::TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral(name.clone()),
            false,
        );
        if self.get_applicable_index_info(table, key).is_some() {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [name, format!("{JSX}.{INTRINSIC_ELEMENTS}")],
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
    ///    `jsxImportSource`, under `jsx: react-jsx` (`:1451`) —
    ///    [`Checker::jsx_implicit_import_container`], resolved through the
    ///    host's record of the loader's synthetic runtime import;
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
    /// # The direction of every decline is the safety argument
    ///
    /// Road 1 resolves by specifier *text* (the loader records the synthetic
    /// runtime import; a runtime named only by an `@jsxImportSource` pragma is
    /// not loaded and so does not resolve here). Every road here is a **lookup** and the rule fires only when all of them
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
        //
        // The fallback is `getGlobalSymbol(JSX, Namespace)` (`jsx.go:1334`):
        // the **globals** table, not a scoped lookup from the tag. A module
        // that declares its own `namespace JSX` is not seen here, which is
        // why upstream reports TS7026 in `jsxPropsAsIdentifierNames`.
        let Some(&namespace) = self.binder.globals().get(JSX) else { return false };
        let namespace = self.binder.merged_symbol(namespace);
        if !self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::NAMESPACE) {
            return false;
        }
        self.exports_intrinsic_elements(namespace)
    }

    /// Road 2: the JSX namespace hanging off `getJsxNamespace`'s name.
    fn jsx_namespace_symbol(&mut self, location: NodeId) -> Option<SymbolId> {
        // Road 1 (`jsx.go:1315`): the automatic runtime's module, when it
        // resolves. Only when it does not is road 2's name looked up.
        let container = if let Some(container) = self.jsx_implicit_import_container(location) {
            container
        } else {
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
            self.resolve_jsx_namespace_container(container)
        };
        let container = self.follow_export_assignment(container);
        let candidate = self.binder.symbols().get(container).exports.get(JSX).copied()?;
        // The outer `c.resolveSymbol(...)` (`jsx.go:1321`): preact's runtime
        // writes `export import JSX = JSXInternal`.
        let candidate = self.binder.merged_symbol(candidate);
        if self.binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS) {
            let target = self.resolve_alias_fully(candidate);
            return Some(self.binder.merged_symbol(target));
        }
        Some(candidate)
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

/// `ast.IsCommaSequence`: a comma binary expression (the
/// `PartiallyEmittedExpression` arm is emit-only).
fn is_comma_sequence(expression: tsr_ast::Expression<'_>) -> bool {
    matches!(
        expression,
        tsr_ast::Expression::BinaryExpression(binary)
            if binary.operator_token.is_some_and(|token| token.kind == tsr_ast::SyntaxKind::CommaToken)
    )
}

pub(crate) fn semantic_jsx_child(child: &tsr_ast::JsxChild<'_>) -> bool {
    match child {
        tsr_ast::JsxChild::JsxText(text) => !text.contains_only_trivia_white_spaces,
        tsr_ast::JsxChild::JsxExpression(expression) => expression.expression.is_some(),
        _ => true,
    }
}

fn possibly_jsx_discriminant_value(expression: tsr_ast::Expression<'_>) -> bool {
    use tsr_ast::{Expression, SyntaxKind};
    match expression {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_)
        | Expression::TemplateExpression(_)
        | Expression::Identifier(_) => true,
        Expression::KeywordExpression(keyword) => matches!(
            keyword.kind,
            SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword
        ),
        Expression::PropertyAccessExpression(node) => {
            node.expression.is_some_and(possibly_jsx_discriminant_value)
        }
        Expression::ParenthesizedExpression(node) => {
            node.expression.is_some_and(possibly_jsx_discriminant_value)
        }
        Expression::JsxExpression(node) => {
            node.expression.is_none_or(possibly_jsx_discriminant_value)
        }
        _ => false,
    }
}

#[cfg(test)]
mod mixed_signature_tests {
    use crate::{Checker, signatures::SignatureKind};
    use tsr_ast::{Node, NodeId, SyntaxKind};

    #[test]
    fn jsx_mixed_signatures_publish_per_opening_without_changing_ordinary_vectors() {
        let source = r#"declare namespace JSX {
    interface Element {}
    interface ElementAttributesProperty { props: {} }
}
interface Props { a: string }
type Constructor = new (props: Props) => { props: { a: number } };
type Function = (props: Props) => JSX.Element;
declare const Mixed: Constructor | Function;
declare const Generic: Constructor | (<T>(props: Props) => JSX.Element);
declare const Overloaded: Constructor | { (props: Props): JSX.Element; (props: Props, second: number): JSX.Element };
declare const Opaque: any;
const first = <Mixed a="one" />;
const second = <Mixed a="two" />;
const generic = <Generic a="three" />;
const overloaded = <Overloaded a="four" />;
const opaque = <Opaque a="five" />;
"#;
        let arena = tsr_core::Arena::new();
        let parsed =
            tsr_parser::parse_with_script_kind(&arena, source, tsr_parser::ScriptKind::Tsx);
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mixed.tsx", text: source },
        );
        for reversed in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let mixed = bound.lookup_local(root, "Mixed").unwrap();
            let ty = checker.get_type_of_symbol(mixed);
            assert!(checker.call_signatures_of_type(ty).unwrap().is_empty());
            assert!(
                checker.signatures_of_type_kind(ty, SignatureKind::Construct).unwrap().is_empty()
            );
            let constructor = bound.lookup_local(root, "Constructor").unwrap();
            let constructor = checker.get_declared_type_of_symbol(constructor);
            let original =
                checker.signatures_of_type_kind(constructor, SignatureKind::Construct).unwrap();
            let original_image = format!("{original:?}");
            let mut openings: Vec<_> = (0..parsed.nodes.len())
                .map(|index| NodeId::new(u32::try_from(index).unwrap()))
                .filter(|&id| parsed.nodes.kind(id) == SyntaxKind::JsxSelfClosingElement)
                .collect();
            assert_eq!(openings.len(), 5);
            if reversed {
                openings.reverse();
            }
            for _ in 0..2 {
                for &opening in &openings {
                    let Some(Node::JsxSelfClosingElement(element)) = parsed.node_map.get(opening)
                    else {
                        unreachable!()
                    };
                    let tag = element.tag_name.unwrap();
                    let context = checker.jsx_attributes_context(opening);
                    if matches!(tag, tsr_ast::JsxTagNameExpression::Identifier(name) if name.text == "Mixed")
                    {
                        let context = context.expect("completed mixed context");
                        assert_eq!(
                            checker.get_type_of_property_of_type(context, "a"),
                            Some(checker.intrinsics.string)
                        );
                        assert!(checker.resolved_call_signatures.contains_key(&opening));
                    } else {
                        assert_eq!(context, None);
                        assert!(!checker.resolved_call_signatures.contains_key(&opening));
                    }
                }
                assert!(checker.call_signatures_of_type(ty).unwrap().is_empty());
                assert!(
                    checker
                        .signatures_of_type_kind(ty, SignatureKind::Construct)
                        .unwrap()
                        .is_empty()
                );
                for kind in [false, true] {
                    assert!(
                        checker
                            .composite_signature_types
                            .get(&(ty, kind))
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .is_empty()
                    );
                }
                let after =
                    checker.signatures_of_type_kind(constructor, SignatureKind::Construct).unwrap();
                assert_eq!(format!("{after:?}"), original_image);
                assert!(checker.resolving_signature_calls.is_empty());
                assert!(checker.active_inference_contexts.is_empty());
            }
            assert_eq!(checker.resolved_call_signatures.len(), 2);
        }
    }
}
