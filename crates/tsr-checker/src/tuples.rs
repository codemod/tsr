//! Tuple argument normalization, ported from typescript-go's
//! `TupleNormalizer.normalize` (`internal/checker/checker.go`).

use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

/// A resolved tuple argument with its written element information. Spread
/// arguments contain the tuple/array operand until normalization expands it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TupleElement {
    pub(crate) r#type: TypeId,
    pub(crate) spread: bool,
    pub(crate) optional: bool,
    pub(crate) label: Option<String>,
}

impl Checker<'_, '_> {
    /// `sliceTupleType` (internal/checker/relater.go). A destructured copy
    /// retains element flags and labels, and always becomes mutable. Slices
    /// beyond the fixed prefix use the remaining element union as an array.
    pub(crate) fn slice_tuple_type(
        &mut self,
        source: TypeId,
        index: usize,
        end_skip_count: usize,
    ) -> Option<TypeId> {
        let mut elements = if let Some((elements, _)) = self.variadic_tuple_elements.get(&source) {
            elements.clone()
        } else {
            let (types, _) = self.tuple_element_lists.get(&source)?;
            let optional = self.tuple_optional_masks.get(&source);
            let labels = self.tuple_labels.get(&source);
            types
                .iter()
                .enumerate()
                .map(|(i, &r#type)| TupleElement {
                    r#type,
                    spread: false,
                    optional: optional.and_then(|mask| mask.get(i)).copied().unwrap_or(false),
                    label: labels.and_then(|labels| labels.get(i)).cloned().flatten(),
                })
                .collect()
        };
        // Tuple type arguments include undefined for optional slots, except
        // when exact optional properties use an implicit missing type instead.
        if self.strict_null_checks && !self.exact_optional_property_types {
            for element in &mut elements {
                if element.optional {
                    element.r#type =
                        self.get_union_type(&[element.r#type, self.intrinsics.undefined]);
                }
            }
        }
        let fixed_length =
            elements.iter().position(|element| element.spread).unwrap_or(elements.len());
        let end_index = elements.len().saturating_sub(end_skip_count);
        if index > fixed_length {
            if fixed_length < elements.len() {
                let element = self.variadic_tuple_element_type(source, fixed_length, false)?;
                let array = self.global_type_symbol("Array")?;
                return Some(self.create_type_reference(array, vec![element]));
            }
            return Some(self.create_tuple_type(Vec::new(), false));
        }
        if index >= end_index {
            return Some(self.create_tuple_type(Vec::new(), false));
        }
        Some(self.normalize_variadic_tuple(elements[index..end_index].to_vec(), false))
    }

    /// The generic tuple arm of computeBaseConstraint
    /// (internal/checker/checker.go). Only variadic type parameters whose
    /// resolved constraints are arrays or non-generic tuples are substituted.
    pub(crate) fn tuple_base_constraint(&mut self, id: TypeId) -> TypeId {
        let Some((mut elements, readonly)) = self.variadic_tuple_elements.get(&id).cloned() else {
            return id;
        };
        let mut changed = false;
        for element in &mut elements {
            if !element.spread
                || !self.store.get(element.r#type).flags.contains(TypeFlags::TYPE_PARAMETER)
            {
                continue;
            }
            let mut constraint = element.r#type;
            let mut seen = Vec::new();
            while self.store.get(constraint).flags.contains(TypeFlags::TYPE_PARAMETER) {
                if seen.contains(&constraint) {
                    break;
                }
                seen.push(constraint);
                let Some(next) = self.type_parameter_constraint(constraint) else { break };
                constraint = next;
            }
            if constraint != element.r#type
                && self.non_generic_array_or_tuple_constraint(constraint)
            {
                element.r#type = constraint;
                changed = true;
            }
        }
        if changed { self.normalize_variadic_tuple(elements, readonly) } else { id }
    }

    fn non_generic_array_or_tuple_constraint(&mut self, id: TypeId) -> bool {
        // tuple_spread_array_element also models any spread recovery; any is
        // not an array constraint in computeBaseConstraint's admission test.
        if self.store.get(id).flags.contains(TypeFlags::ANY) {
            return false;
        }
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            return types.into_iter().all(|t| self.non_generic_array_or_tuple_constraint(t));
        }
        if self.tuple_element_lists.contains_key(&id)
            || self.tuple_spread_array_element(id).is_some()
        {
            return true;
        }
        if let Some((elements, _)) = self.variadic_tuple_elements.get(&id).cloned() {
            return elements.iter().all(|element| {
                !element.spread || self.tuple_spread_array_element(element.r#type).is_some()
            });
        }
        false
    }

    /// Parentheses for a union/intersection under the optional tuple element
    /// node emitted by typeToTypeNode (internal/checker/nodebuilder.go).
    pub(crate) fn optional_tuple_element_text(&mut self, id: TypeId) -> String {
        // Optional tuple nodes omit implicit missing, but retain a written
        // undefined. An elision's missing-only type therefore prints never?.
        let id = self.remove_missing_type(id);
        let text = self.type_to_string(id);
        // boolean's true | false constituents render as a keyword node.
        if id != self.intrinsics.boolean
            && matches!(
                self.store.get(id).data,
                TypeData::Union { symbol: None, .. } | TypeData::Intersection { symbol: None, .. }
            )
        {
            format!("({text})")
        } else {
            text
        }
    }
    /// The array and tuple cases of `isArrayLikeType`, including a type
    /// parameter's base constraint (`internal/checker/checker.go`).
    pub(crate) fn tuple_array_like(&mut self, id: TypeId) -> bool {
        fn visit(checker: &mut Checker<'_, '_>, id: TypeId, seen: &mut Vec<TypeId>) -> bool {
            if seen.contains(&id) || checker.is_error(id) {
                return false;
            }
            if checker.tuple_element_lists.contains_key(&id)
                || checker.variadic_tuple_elements.contains_key(&id)
                || checker.tuple_spread_array_element(id).is_some()
            {
                return true;
            }
            seen.push(id);
            let answer = if let Some(&source) = checker.mapped_identity_sources.get(&id) {
                visit(checker, source, seen)
            } else if let Some(constraint) = checker.type_parameter_constraint(id) {
                visit(checker, constraint, seen)
            } else if let TypeData::Union { types, .. } = &checker.store.get(id).data {
                let types = types.clone();
                types.into_iter().all(|part| visit(checker, part, seen))
            } else {
                false
            };
            seen.pop();
            answer
        }
        visit(self, id, &mut Vec::new())
    }
    /// Synthetic tuple properties inherit the target's readonly flag
    /// (`getTupleTargetType`, internal/checker/checker.go).
    pub(crate) fn tuple_is_readonly(&self, id: TypeId) -> bool {
        self.tuple_element_lists
            .get(&id)
            .map(|(_, readonly)| *readonly)
            .or_else(|| self.variadic_tuple_elements.get(&id).map(|(_, readonly)| *readonly))
            .unwrap_or(false)
    }

    /// The numeric tuple cases of `getIndexedAccessTypeOrUndefined`
    /// (`internal/checker/checker.go`). An access into a generic rest is
    /// deferred until its object and index arguments can be mapped.
    pub(crate) fn tuple_index_type(
        &mut self,
        object: TypeId,
        index: TypeId,
        include_undefined: bool,
    ) -> Option<TypeId> {
        let position = match &self.store.get(index).data {
            TypeData::NumberLiteral(text) | TypeData::StringLiteral(text) => {
                let position = text.parse::<usize>().ok()?;
                if position.to_string() != *text {
                    return None;
                }
                Some(position)
            }
            _ if index == self.intrinsics.number => None,
            _ => return None,
        };
        if object == self.intrinsics.any {
            return Some(object);
        }
        // shouldDeferIndexedAccessType compares a literal index with the total
        // fixed element count before attempting the numeric index signature.
        let defer = if let Some((elements, _)) = self.variadic_tuple_elements.get(&object).cloned()
        {
            let is_generic = elements.iter().any(|element| {
                element.spread && self.tuple_spread_array_element(element.r#type).is_none()
            });
            let fixed = elements.iter().filter(|element| !element.spread).count();
            is_generic && position.is_none_or(|position| position >= fixed)
        } else {
            self.store.get(object).flags.contains(TypeFlags::TYPE_PARAMETER)
                && self.tuple_array_like(object)
        };
        if defer {
            let key = (object, index, include_undefined);
            if let Some(&cached) = self.deferred_indexed_access_cache.get(&key) {
                return Some(cached);
            }
            let text = format!("{}[{}]", self.type_to_string(object), self.type_to_string(index));
            let id = self.store.new_named(TypeFlags::INDEXED_ACCESS, text, None);
            self.deferred_indexed_access_types.insert(id, key);
            self.deferred_indexed_access_cache.insert(key, id);
            self.deferred_index_mints.insert(id);
            return Some(id);
        }
        if let Some(position) = position {
            if let Some(t) = self.variadic_tuple_element_type(object, position, include_undefined) {
                return Some(t);
            }
            if self.tuple_element_lists.contains_key(&object) {
                return self.get_type_of_property_of_type(object, &position.to_string());
            }
        } else if let Some(t) = self.variadic_tuple_index_union(object) {
            return Some(if include_undefined {
                self.get_union_type(&[t, self.intrinsics.undefined])
            } else {
                t
            });
        }
        self.array_or_tuple_element_access(object, index, include_undefined)
    }

    /// `TupleNormalizer.normalize`: expand concrete tuples after substitution,
    /// retain generic spreads, and reduce an all-rest tuple to an array.
    pub(crate) fn normalize_variadic_tuple(
        &mut self,
        elements: Vec<TupleElement>,
        readonly: bool,
    ) -> TypeId {
        // `createNormalizedTupleTypeEx` distributes variadic union arguments
        // before flattening; a variadic `never` annihilates the whole tuple.
        let mut cross_product = 1usize;
        for element in elements.iter().filter(|element| element.spread) {
            if self.store.get(element.r#type).flags.contains(TypeFlags::NEVER) {
                return self.intrinsics.never;
            }
            if let TypeData::Union { types, .. } = &self.store.get(element.r#type).data {
                cross_product = cross_product.saturating_mul(types.len());
            }
        }
        if cross_product >= 100_000 {
            return self.intrinsics.error;
        }
        for (index, element) in elements.iter().enumerate() {
            if element.spread
                && let TypeData::Union { types, .. } = &self.store.get(element.r#type).data
            {
                let constituents = types.clone();
                let mut alternatives = Vec::with_capacity(constituents.len());
                for constituent in constituents {
                    let mut branch = elements.clone();
                    branch[index].r#type = constituent;
                    let normalized = self.normalize_variadic_tuple(branch, readonly);
                    if normalized == self.intrinsics.error {
                        return normalized;
                    }
                    alternatives.push(normalized);
                }
                return self.get_union_type(&alternatives);
            }
        }
        let mut normalized = Vec::new();
        for mut element in elements {
            if element.spread && element.r#type == self.intrinsics.any {
                let Some(array) = self.global_type_symbol("Array") else {
                    return self.intrinsics.error;
                };
                element.r#type = self.create_type_reference(array, vec![element.r#type]);
            }
            if element.spread
                && let Some((types, _)) = self.tuple_element_lists.get(&element.r#type).cloned()
            {
                let mask = self.tuple_optional_masks.get(&element.r#type);
                let labels = self.tuple_labels.get(&element.r#type);
                if normalized.len() + types.len() >= 10_000 {
                    return self.intrinsics.error;
                }
                normalized.extend(types.into_iter().enumerate().map(|(index, t)| TupleElement {
                    r#type: t,
                    spread: false,
                    optional: mask.and_then(|m| m.get(index)).copied().unwrap_or(false),
                    label: labels.and_then(|l| l.get(index)).cloned().flatten(),
                }));
            } else if element.spread
                && let Some((elements, _)) =
                    self.variadic_tuple_elements.get(&element.r#type).cloned()
            {
                if normalized.len() + elements.len() >= 10_000 {
                    return self.intrinsics.error;
                }
                normalized.extend(elements);
            } else {
                normalized.push(element);
            }
        }
        // `TupleNormalizer.add` retains optionality in the type even when a
        // following required element removes the optional element flag.
        if let Some(last_required) = normalized.iter().rposition(|e| !e.spread && !e.optional) {
            for element in &mut normalized[..last_required] {
                if element.optional {
                    element.r#type =
                        self.get_union_type(&[element.r#type, self.intrinsics.undefined]);
                    element.optional = false;
                }
            }
        }
        let first_rest = normalized
            .iter()
            .position(|e| e.spread && self.tuple_spread_array_element(e.r#type).is_some());
        let last_optional_or_rest = normalized.iter().rposition(|e| {
            e.optional || (e.spread && self.tuple_spread_array_element(e.r#type).is_some())
        });
        if let (Some(first), Some(last)) = (first_rest, last_optional_or_rest)
            && first < last
        {
            let mut types = Vec::new();
            for element in &normalized[first..=last] {
                let t = if element.spread {
                    self.tuple_index_type(element.r#type, self.intrinsics.number, false)
                        .unwrap_or(self.intrinsics.error)
                } else {
                    element.r#type
                };
                types.push(t);
                if element.optional {
                    types.push(self.intrinsics.undefined);
                }
            }
            let union = self.get_union_type(&types);
            let Some(array) = self.global_type_symbol("Array") else {
                return self.intrinsics.error;
            };
            normalized[first].r#type = self.create_type_reference(array, vec![union]);
            normalized[first].optional = false;
            normalized.drain(first + 1..=last);
        }
        if normalized.iter().all(|element| !element.spread) {
            if normalized.iter().any(|element| element.optional || element.label.is_some()) {
                let types: Vec<_> = normalized.iter().map(|e| (e.r#type, e.optional)).collect();
                let labels: Vec<_> = normalized.into_iter().map(|e| e.label).collect();
                return self.create_optional_tuple_type(&types, &labels, readonly);
            }
            return self
                .create_tuple_type(normalized.into_iter().map(|e| e.r#type).collect(), readonly);
        }
        if normalized.iter().all(|element| element.spread) {
            let types = normalized
                .iter()
                .map(|e| self.tuple_spread_array_element(e.r#type))
                .collect::<Option<Vec<_>>>();
            if let Some(types) = types {
                let union = self.get_union_type(&types);
                let name = if readonly { "ReadonlyArray" } else { "Array" };
                if let Some(array) = self.global_type_symbol(name) {
                    return self.create_type_reference(array, vec![union]);
                }
                return self.intrinsics.error;
            }
        }
        let key = (normalized.clone(), readonly);
        if let Some(&cached) = self.variadic_tuple_types.get(&key) {
            return cached;
        }
        let mut pieces = Vec::with_capacity(normalized.len());
        for element in &normalized {
            let text = self.type_to_string(element.r#type);
            let text = match (&element.label, element.spread, element.optional) {
                (Some(label), true, _) => format!("...{label}: {text}"),
                (Some(label), false, true) => format!("{label}?: {text}"),
                (Some(label), false, false) => format!("{label}: {text}"),
                (None, true, _) => format!("...{text}"),
                (None, false, true) => {
                    format!("{}?", self.optional_tuple_element_text(element.r#type))
                }
                (None, false, false) => text,
            };
            pieces.push(text);
        }
        let text = format!("{}[{}]", if readonly { "readonly " } else { "" }, pieces.join(", "));
        let id = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.variadic_tuple_elements.insert(id, (normalized, readonly));
        self.variadic_tuple_types.insert(key, id);
        id
    }

    /// `getElementTypeOfArrayType`, plus the `any` spread recovery used by
    /// `TupleNormalizer.normalize` (`internal/checker/checker.go`).
    pub(crate) fn tuple_spread_array_element(&mut self, id: TypeId) -> Option<TypeId> {
        if self.store.get(id).flags.contains(TypeFlags::ANY) {
            return Some(id);
        }
        let (target, arguments) = self.type_reference_targets.get(&id).cloned()?;
        let [element] = arguments.as_slice() else { return None };
        ["Array", "ReadonlyArray"]
            .iter()
            .any(|name| {
                self.global_type_symbol(name).is_some_and(|symbol| {
                    self.binder.merged_symbol(symbol) == self.binder.merged_symbol(target)
                })
            })
            .then_some(*element)
    }

    /// Fixed-start properties and `getTupleElementTypeOutOfStartCount`
    /// (`internal/checker/checker.go`). Generic variadic operands contribute
    /// their deferred numeric indexed access to the remaining element union.
    pub(crate) fn variadic_tuple_element_type(
        &mut self,
        id: TypeId,
        index: usize,
        include_undefined: bool,
    ) -> Option<TypeId> {
        let (elements, _) = self.variadic_tuple_elements.get(&id).cloned()?;
        let start = elements.iter().position(|element| element.spread)?;
        if index < start {
            let element = &elements[index];
            return Some(if element.optional {
                self.get_union_type(&[element.r#type, self.intrinsics.undefined])
            } else {
                element.r#type
            });
        }
        let mut types = Vec::new();
        for element in &elements[start..] {
            let t = if element.spread {
                self.tuple_rest_element_type(element.r#type)?
            } else {
                element.r#type
            };
            types.push(t);
            if element.optional {
                types.push(self.intrinsics.undefined);
            }
        }
        let fixed_count = elements.iter().filter(|element| !element.spread).count();
        if include_undefined && index >= fixed_count {
            types.push(self.intrinsics.undefined);
        }
        Some(self.get_union_type(&types))
    }

    fn tuple_rest_element_type(&mut self, operand: TypeId) -> Option<TypeId> {
        if let Some(element) = self.tuple_spread_array_element(operand) {
            return Some(element);
        }
        if self.store.get(operand).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return self.resolved_indexed_access_type(operand, self.intrinsics.number, false);
        }
        None
    }

    /// The numeric index signature of a rest tuple, corresponding to
    /// `getElementTypeOfSliceOfTupleType` (`internal/checker/checker.go`).
    pub(crate) fn variadic_tuple_index_union(&mut self, id: TypeId) -> Option<TypeId> {
        let (elements, _) = self.variadic_tuple_elements.get(&id).cloned()?;
        let mut types = Vec::with_capacity(elements.len());
        for element in elements {
            types.push(if element.spread {
                self.tuple_rest_element_type(element.r#type)?
            } else {
                element.r#type
            });
            if element.optional {
                types.push(self.intrinsics.undefined);
            }
        }
        Some(self.get_union_type(&types))
    }
}

/// The out-of-bounds arm of `getPropertyTypeForIndexType`
/// (`checker.go:27060`–`:27078`): a numeric literal name that no tuple
/// constituent declares, on a type whose every constituent is a tuple with no
/// variable element, reports on the access node's index
/// (`getIndexNodeForAccessExpression`) unless `AccessFlagsAllowMissing` is set.
impl Checker<'_, '_> {
    /// The element count of a tuple with no rest or variadic element, or
    /// `None` when `ty` is not such a tuple (or its shape is unresolved here).
    fn fixed_tuple_arity(&self, ty: TypeId) -> Option<usize> {
        if self.variadic_tuple_elements.contains_key(&ty)
            || self.variadic_tuple_nodes.contains_key(&ty)
            || self.tuple_rest_tails.contains_key(&ty)
        {
            return None;
        }
        self.tuple_element_lists.get(&ty).map(|(elements, _)| elements.len())
    }

    /// Reports TS2493/TS2514 (one tuple) or TS2339 (a union of tuples) when
    /// `prop_name` names no element of `object_type`. `object_type` is the
    /// apparent object type of the access.
    pub(crate) fn report_tuple_index_out_of_bounds(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        index_node: NodeId,
    ) {
        if !crate::index_signatures::is_numeric_literal_name(prop_name) {
            return;
        }
        let object_type = self.binding_type_alias_body(object_type);
        let parts = match &self.store.get(object_type).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![object_type],
        };
        let mut arities = Vec::with_capacity(parts.len());
        for &part in &parts {
            let part = self.binding_type_alias_body(part);
            let Some(arity) = self.fixed_tuple_arity(part) else { return };
            arities.push(arity);
        }
        // getPropertyOfType: an element property exists for every position
        // below the arity. A union property is found when any constituent
        // declares it: every tuple has a number index signature, so a
        // constituent lacking the element never makes it ReadPartial.
        let declared = prop_name
            .parse::<usize>()
            .ok()
            .filter(|index| index.to_string() == prop_name)
            .is_some_and(|index| arities.iter().any(|&arity| index < arity));
        if declared {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(index_node) else { return };
        let span = self.error_span(index_node);
        let diagnostic = if parts.len() == 1 {
            if prop_name.starts_with('-') {
                Diagnostic::new(
                    &messages::A_TUPLE_TYPE_CANNOT_BE_INDEXED_WITH_A_NEGATIVE_VALUE,
                    span,
                )
            } else {
                Diagnostic::with_args(
                    &messages::TUPLE_TYPE_0_OF_LENGTH_1_HAS_NO_ELEMENT_AT_INDEX_2,
                    span,
                    [
                        self.type_to_string(object_type),
                        arities[0].to_string(),
                        prop_name.to_string(),
                    ],
                )
            }
        } else {
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [prop_name.to_string(), self.type_to_string(object_type)],
            )
        };
        self.report(file, diagnostic);
    }

    /// `checkElementAccessExpression` (`checker.go:10679`) into
    /// `getIndexedAccessTypeOrUndefined` with the access expression as the
    /// access node and no `AccessFlagsAllowMissing`.
    pub(crate) fn check_element_access_tuple_bounds(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(expression), Some(argument)) = (access.expression, access.argument_expression)
        else {
            return;
        };
        let Some(argument_id) = argument.node_id() else { return };
        let index_type = self.check_expression(argument);
        if !self
            .store
            .get(index_type)
            .flags
            .intersects(TypeFlags::NUMBER_LITERAL | TypeFlags::STRING_LITERAL)
        {
            return;
        }
        let Some(prop_name) = self.property_name_from_index(index_type) else { return };
        let object_type = self.check_expression(expression);
        let object_type = self.check_non_null_type(object_type);
        if self.is_error(object_type) {
            return;
        }
        // getReducedApparentType: a type variable is read through its constraint.
        let object_type = if self.store.get(object_type).flags.intersects(TypeFlags::INSTANTIABLE) {
            self.apparent_type(object_type)
        } else {
            object_type
        };
        self.report_tuple_index_out_of_bounds(object_type, &prop_name, argument_id);
    }

    /// `getTypeFromIndexedAccessTypeNode` (`checker.go:24164`) into
    /// `getIndexedAccessType` with the type node as the access node: a
    /// non-generic tuple object reports on the index type node.
    pub(crate) fn check_indexed_access_type_tuple_bounds(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::IndexedAccessTypeNode(access)) = self.node_map.get(node) else { return };
        let (Some(object_node), Some(index_node)) = (access.object_type, access.index_type) else {
            return;
        };
        let Some(index_id) = index_node.node_id() else { return };
        let index_type = self.get_type_from_type_node(index_node);
        if !self
            .store
            .get(index_type)
            .flags
            .intersects(TypeFlags::NUMBER_LITERAL | TypeFlags::STRING_LITERAL)
        {
            return;
        }
        let Some(prop_name) = self.property_name_from_index(index_type) else { return };
        let object_type = self.get_type_from_type_node(object_node);
        if self.is_error(object_type) {
            return;
        }
        self.report_tuple_index_out_of_bounds(object_type, &prop_name, index_id);
    }

    /// `getBindingElementTypeFromParentType`'s positional arm
    /// (`checker.go:17768`): an array-like parent is indexed with the element's
    /// position, the element's name as access node, and
    /// `AccessFlagsAllowMissing` when the element has a default.
    pub(crate) fn check_binding_element_tuple_bounds(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(node) else { return };
        if element.dot_dot_dot_token.is_some() || element.initializer.is_some() {
            return;
        }
        let Some(name) = element.name.and_then(|name| name.node_id()) else { return };
        let Some(pattern_id) = self.nodes.parent(node) else { return };
        if self.nodes.kind(pattern_id) != SyntaxKind::ArrayBindingPattern {
            return;
        }
        let Some(Node::BindingPattern(pattern)) = self.node_map.get(pattern_id) else { return };
        let Some(index) = pattern.elements.iter().position(|e| e.node_id == Some(node)) else {
            return;
        };
        let Some(holder) = self.nodes.parent(pattern_id) else { return };
        let parent_type = self.get_type_for_binding_element_parent(holder);
        if parent_type == self.intrinsics.any || self.is_error(parent_type) {
            return;
        }
        let parent_type = self.destructuring_parent_adjusted(node, holder, parent_type);
        if self.binding_parent_is_array_like(parent_type) != Some(true) {
            return;
        }
        let apparent = self.apparent_type(parent_type);
        self.report_tuple_index_out_of_bounds(apparent, &index.to_string(), name);
    }

    /// `checkArrayLiteralDestructuringElementAssignment` (`checker.go:12663`)
    /// for the outermost `=` target: each non-spread, non-omitted element
    /// without a default indexes an array-like source at its position.
    pub(crate) fn check_array_assignment_tuple_bounds(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ArrayLiteralExpression(literal)) = self.node_map.get(node) else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else { return };
        if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
            || binary.left.and_then(|left| left.node_id()) != Some(node)
        {
            return;
        }
        let Some(right) = binary.right else { return };
        let elements: Vec<_> = literal.elements.iter().map(tsr_ast::Expression::node_id).collect();
        let source = self.check_expression(right);
        if source == self.intrinsics.any || self.is_error(source) {
            return;
        }
        if self.binding_parent_is_array_like(source) != Some(true) {
            return;
        }
        let apparent = self.apparent_type(source);
        for (index, element) in elements.into_iter().enumerate() {
            let Some(element) = element else { continue };
            match self.node_map.get(element) {
                Some(Node::OmittedExpression(_) | Node::SpreadElement(_)) | None => continue,
                // hasDefaultValue: `[x = 1] = …` sets AccessFlagsAllowMissing.
                Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken) =>
                {
                    continue;
                }
                Some(_) => {}
            }
            self.report_tuple_index_out_of_bounds(apparent, &index.to_string(), element);
        }
    }
}
