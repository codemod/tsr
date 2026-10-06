//! Object-literal widening contexts, checker.go:18359–18503.
use rustc_hash::FxHashMap;
use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    objects::{AnonymousProperty, Member, PrintedSlot, PropertySlot},
    types::{TypeData, TypeId},
};

#[derive(Clone)]
pub(crate) struct WideningProperty {
    property: AnonymousProperty,
    symbol: Option<SymbolId>,
    member: Member,
}

/// `WideningContext` (checker.go). Child siblings are resolved once per property;
/// each context memoizes object images independently of the root cache.
#[derive(Default)]
pub(crate) struct WideningContext {
    siblings: Vec<TypeId>,
    properties: Option<Vec<WideningProperty>>,
    children: FxHashMap<String, usize>,
    widened: FxHashMap<TypeId, TypeId>,
}

impl Checker<'_, '_> {
    /// createArrayLiteralType (checker.go:8103): a separately cached reference
    /// image carries `ArrayLiteral` without changing the canonical array/tuple.
    pub(crate) fn create_array_literal_type(&mut self, id: TypeId) -> TypeId {
        if self.array_literal_bases.contains_key(&id) {
            return id;
        }
        if let Some(&cached) = self.array_literal_images.get(&id) {
            return cached;
        }
        if !self.type_reference_targets.contains_key(&id)
            && !self.tuple_element_lists.contains_key(&id)
            && !self.variadic_tuple_elements.contains_key(&id)
        {
            return id;
        }
        let TypeData::Named { text, members } = self.store.get(id).data.clone() else { return id };
        let result = self.store.new_named(self.store.get(id).flags, text, members);
        if let Some(data) = self.type_reference_targets.get(&id).cloned() {
            self.type_reference_targets.insert(result, data);
        }
        if let Some(data) = self.reference_display_arity.get(&id).copied() {
            self.reference_display_arity.insert(result, data);
        }
        if let Some(data) = self.tuple_element_lists.get(&id).cloned() {
            self.tuple_element_lists.insert(result, data);
        }
        if let Some(data) = self.tuple_optional_masks.get(&id).cloned() {
            self.tuple_optional_masks.insert(result, data);
        }
        if let Some(data) = self.tuple_labels.get(&id).cloned() {
            self.tuple_labels.insert(result, data);
        }
        if let Some(data) = self.tuple_rest_tails.get(&id).copied() {
            self.tuple_rest_tails.insert(result, data);
        }
        if let Some(data) = self.variadic_tuple_nodes.get(&id).copied() {
            self.variadic_tuple_nodes.insert(result, data);
        }
        if let Some(data) = self.variadic_tuple_elements.get(&id).cloned() {
            self.variadic_tuple_elements.insert(result, data);
        }
        self.array_literal_images.insert(id, result);
        self.array_literal_bases.insert(result, id);
        result
    }

    /// unionObjectAndArrayLiteralCandidates (inference.go:1470).
    pub(crate) fn union_object_and_array_literal_candidates(
        &mut self,
        candidates: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        if candidates.len() <= 1 {
            return Some(candidates.to_vec());
        }
        let (literals, mut others): (Vec<_>, Vec<_>) =
            candidates.iter().copied().partition(|&ty| {
                self.is_object_literal_type(ty) || self.array_literal_bases.contains_key(&ty)
            });
        if literals.is_empty() {
            return Some(others);
        }
        others.push(self.union_with_subtype_reduction(&literals)?);
        Some(others)
    }

    pub(crate) fn widen_type_with_context(
        &mut self,
        id: TypeId,
        context: Option<usize>,
        contexts: &mut Vec<WideningContext>,
    ) -> TypeId {
        let cached = context.map_or_else(
            || self.widened_object_types.get(&id).copied(),
            |index| contexts[index].widened.get(&id).copied(),
        );
        if let Some(cached) = cached {
            return cached;
        }
        // Preserve identity on a recursive visit while constructing its image.
        if let Some(index) = context {
            contexts[index].widened.insert(id, id);
        } else {
            self.widened_object_types.insert(id, id);
        }
        // `checker.go:18368`: a `createWideningType` nullable widens to `any`.
        let result = if self.intrinsics.is_widening_nullable(id) {
            self.intrinsics.any
        } else {
            match self.store.get(id).data.clone() {
                TypeData::Union { types, .. } => {
                    let index = context.unwrap_or_else(|| {
                        let index = contexts.len();
                        contexts.push(WideningContext {
                            siblings: types.clone(),
                            ..Default::default()
                        });
                        index
                    });
                    let widened: Vec<_> = types
                        .iter()
                        .map(|&ty| self.widen_type_with_context(ty, Some(index), contexts))
                        .collect();
                    if widened == types {
                        id
                    } else if widened.iter().any(|&ty| {
                        self.is_empty_anonymous_object_type(ty)
                            && self.object_literal_members.get(&ty).is_none_or(Vec::is_empty)
                    }) {
                        self.union_with_subtype_reduction(&widened).unwrap_or(self.intrinsics.error)
                    } else {
                        self.get_union_type(&widened)
                    }
                }
                TypeData::Intersection { types, .. } => {
                    let widened: Vec<_> = types
                        .iter()
                        .map(|&ty| self.widen_type_with_context(ty, None, contexts))
                        .collect();
                    if widened == types { id } else { self.get_intersection_type(&widened, None) }
                }
                _ if self.is_object_literal_type(id) => {
                    self.widen_object_in_context(id, context, contexts)
                }
                _ => self.widen_array_members(id, contexts),
            }
        };
        if let Some(index) = context {
            contexts[index].widened.insert(id, result);
        } else {
            self.widened_object_types.insert(id, result);
        }
        result
    }

    fn widen_array_members(&mut self, id: TypeId, contexts: &mut Vec<WideningContext>) -> TypeId {
        let id = self.array_literal_bases.get(&id).copied().unwrap_or(id);
        if let Some((mut elements, readonly)) = self.variadic_tuple_elements.get(&id).cloned() {
            let mut changed = false;
            for element in &mut elements {
                let widened = self.widen_type_with_context(element.r#type, None, contexts);
                changed |= widened != element.r#type;
                element.r#type = widened;
            }
            return if changed { self.normalize_variadic_tuple(elements, readonly) } else { id };
        }
        if let Some((elements, readonly)) = self.tuple_element_lists.get(&id).cloned() {
            let widened: Vec<_> = elements
                .iter()
                .map(|&ty| self.widen_type_with_context(ty, None, contexts))
                .collect();
            if widened == elements {
                return id;
            }
            if let Some(mask) = self.tuple_optional_masks.get(&id).cloned() {
                let labels =
                    self.tuple_labels.get(&id).cloned().unwrap_or_else(|| vec![None; mask.len()]);
                let elements: Vec<_> = widened.into_iter().zip(mask).collect();
                return self.create_optional_tuple_type(&elements, &labels, readonly);
            }
            return self.create_tuple_type(widened, readonly);
        }
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&id).cloned()
            && ["Array", "ReadonlyArray"]
                .iter()
                .any(|name| self.global_type_symbol(name) == Some(symbol))
        {
            let widened: Vec<_> = arguments
                .iter()
                .map(|&ty| self.widen_type_with_context(ty, None, contexts))
                .collect();
            if widened != arguments {
                return self.create_type_reference(symbol, widened);
            }
        }
        id
    }

    fn widening_properties(&mut self, id: TypeId) -> Vec<WideningProperty> {
        let stored = self.anonymous_properties.get(&id).map(|(properties, _)| properties.clone());
        let members = self.object_literal_members.get(&id).cloned().unwrap_or_default();
        let names = stored.as_ref().map_or_else(
            || self.property_names_of(id),
            |properties| properties.iter().map(|property| property.name.clone()).collect(),
        );
        let mut result = Vec::with_capacity(names.len());
        for name in names {
            let symbol = self.property_origin(id, &name);
            let property = stored
                .as_ref()
                .and_then(|properties| properties.iter().find(|property| property.name == name))
                .cloned()
                .unwrap_or_else(|| {
                    let ty = self
                        .get_type_of_property_of_type(id, &name)
                        .unwrap_or(self.intrinsics.error);
                    AnonymousProperty {
                        accessor_write: None,
                        method: symbol.is_some_and(|symbol| {
                            self.binder
                                .symbols()
                                .get(symbol)
                                .flags
                                .contains(tsr_binder::SymbolFlags::METHOD)
                        }),
                        origin: symbol,
                        checked_declaration: None,
                        name: name.clone(),
                        printed_name: name.clone(),
                        printed_slot: PrintedSlot::printed(self.type_to_string(ty)),
                        slot: PropertySlot::resolved(ty),
                        optional: symbol.is_some_and(|s| self.property_is_optional(s)),
                        readonly: symbol.is_some_and(|s| self.is_readonly_property(s)),
                    }
                });
            let member = members
                .iter()
                .find(|member| match member {
                    Member::Property { name, .. } | Member::Method { name, .. } => {
                        name == &property.printed_name
                    }
                    Member::Signature { printed } => {
                        printed.starts_with(&format!("{}(", property.printed_name))
                            || printed.starts_with(&format!("{}<", property.printed_name))
                    }
                    Member::Index { .. } => false,
                })
                .cloned()
                .unwrap_or_else(|| self.widening_property_member(&property));
            result.push(WideningProperty { property, symbol, member });
        }
        result
    }

    fn widening_member_has_name(member: &Member, property: &AnonymousProperty) -> bool {
        match member {
            Member::Property { name, .. } | Member::Method { name, .. } => {
                *name == property.printed_name
            }
            Member::Signature { printed } => {
                let quoted = crate::printing::quote(&property.name);
                [&property.printed_name, &property.name, &quoted].iter().any(|name| {
                    printed.starts_with(&format!("{name}("))
                        || printed.starts_with(&format!("{name}<"))
                        || printed.starts_with(&format!("get {name}("))
                        || printed.starts_with(&format!("set {name}("))
                })
            }
            Member::Index { .. } => false,
        }
    }

    fn widening_property_member(&mut self, property: &AnonymousProperty) -> Member {
        Member::Property {
            name: property.printed_name.clone(),
            optional: property.optional,
            readonly: property.readonly,
            printed: self.property_printed_type(property).into_owned(),
        }
    }

    fn widening_context_properties(
        &mut self,
        index: usize,
        contexts: &mut [WideningContext],
    ) -> Vec<WideningProperty> {
        if let Some(properties) = &contexts[index].properties {
            return properties.clone();
        }
        let siblings = contexts[index].siblings.clone();
        let mut properties: Vec<WideningProperty> = Vec::new();
        for sibling in siblings {
            if self.object_literal_spread_flags.get(&sibling) != Some(&false) {
                continue;
            }
            for property in self.widening_properties(sibling) {
                if let Some(existing) = properties
                    .iter_mut()
                    .find(|existing| existing.property.name == property.property.name)
                {
                    *existing = property;
                } else {
                    properties.push(property);
                }
            }
        }
        contexts[index].properties = Some(properties.clone());
        properties
    }

    fn child_widening_context(
        &mut self,
        parent: usize,
        name: &str,
        contexts: &mut Vec<WideningContext>,
    ) -> usize {
        if let Some(&child) = contexts[parent].children.get(name) {
            return child;
        }
        let mut siblings = Vec::new();
        for sibling in contexts[parent].siblings.clone() {
            if !self.is_object_literal_type(sibling) {
                continue;
            }
            if let Some(ty) = self
                .widening_properties(sibling)
                .iter()
                .find(|p| p.property.name == name)
                .map(|p| self.property_type(&p.property))
            {
                if let TypeData::Union { types, .. } = &self.store.get(ty).data {
                    siblings.extend(types);
                } else {
                    siblings.push(ty);
                }
            }
        }
        let child = contexts.len();
        contexts.push(WideningContext { siblings, ..Default::default() });
        contexts[parent].children.insert(name.to_owned(), child);
        child
    }

    fn widen_object_in_context(
        &mut self,
        id: TypeId,
        context: Option<usize>,
        contexts: &mut Vec<WideningContext>,
    ) -> TypeId {
        let mut properties = self.widening_properties(id);
        for property in &mut properties {
            if matches!(property.member, Member::Signature { .. }) {
                continue;
            }
            let child = context
                .map(|index| self.child_widening_context(index, &property.property.name, contexts));
            let property_type = self.property_type(&property.property);
            let widened = self.widen_type_with_context(property_type, child, contexts);
            if widened != property_type {
                property.property.slot = PropertySlot::resolved(widened);
                property.property.printed_slot = PrintedSlot::printed(self.type_to_string(widened));
                property.member = self.widening_property_member(&property.property);
            }
        }
        let mut added = Vec::new();
        if let Some(index) = context {
            for mut property in self.widening_context_properties(index, contexts) {
                if properties
                    .iter()
                    .any(|existing| existing.property.name == property.property.name)
                {
                    continue;
                }
                let name = property.property.name.clone();
                if let Some(cached) = self.widening_undefined_properties.get(&name) {
                    property = cached.clone();
                } else {
                    let undefined = if self.exact_optional_property_types {
                        self.intrinsics.missing
                    } else {
                        self.intrinsics.undefined
                    };
                    property.property.slot = PropertySlot::resolved(undefined);
                    property.property.printed_slot = PrintedSlot::printed("undefined".to_owned());
                    property.property.optional = true;
                    property.member = self.widening_property_member(&property.property);
                    self.widening_undefined_properties.insert(name, property.clone());
                }
                added.push(property.property.name.clone());
                properties.push(property);
            }
        }
        if !added.is_empty() {
            properties.sort_by(|a, b| match (a.symbol, b.symbol) {
                (Some(a), Some(b)) => self.compare_symbols(a, b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => a.property.name.cmp(&b.property.name),
            });
        }
        // Keep original method/accessor/computed-name syntax. Only property
        // values that actually widen are replaced; synthetic missing members
        // are merged using the source-symbol order from getUndefinedProperty.
        let original = self.object_literal_members.get(&id).cloned().unwrap_or_default();
        let mut members: Vec<Member> = original
            .iter()
            .map(|member| {
                if let Member::Property { name, .. } = member
                    && let Some(property) =
                        properties.iter().find(|p| p.property.printed_name == *name)
                    && matches!(property.member, Member::Property { .. })
                {
                    property.member.clone()
                } else {
                    member.clone()
                }
            })
            .collect();
        if !added.is_empty() {
            let mut ordered = Vec::new();
            let mut used = vec![false; members.len()];
            for (index, member) in members.iter().enumerate() {
                if matches!(member, Member::Index { .. }) {
                    ordered.push(member.clone());
                    used[index] = true;
                }
            }
            for property in &properties {
                if added.contains(&property.property.name) {
                    ordered.push(property.member.clone());
                } else {
                    for (index, member) in members.iter().enumerate() {
                        if Self::widening_member_has_name(member, &property.property) {
                            ordered.push(member.clone());
                            used[index] = true;
                        }
                    }
                }
            }
            // Methods, accessors and indexes are already rendered at their
            // native declaration sites; preserve every unmatched member.
            ordered.extend(
                members
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, member)| (!used[index]).then_some(member)),
            );
            members = ordered;
        }
        let mut infos = self.object_literal_index_infos.get(&id).cloned().unwrap_or_default();
        for info in &mut infos {
            let widened = self.widen_type_with_context(info.value, None, contexts);
            if widened != info.value {
                let key_text = self.type_to_string(info.key);
                for member in &mut members {
                    if let Member::Index { key, value, .. } = member
                        && *key == key_text
                    {
                        *value = self.type_to_string(widened);
                    }
                }
                info.value = widened;
            }
        }
        let owner = match self.store.get(id).data {
            TypeData::Named { members, .. } => members,
            _ => None,
        };
        let result = self.store.new_named(
            TypeFlags::OBJECT,
            crate::objects::render_object_type(&members),
            owner,
        );
        self.anonymous_properties.insert(
            result,
            (properties.into_iter().map(|property| property.property).collect(), true),
        );
        self.object_literal_members.insert(result, members);
        if !infos.is_empty() {
            self.object_literal_index_infos.insert(result, infos);
        }
        if let Some(signatures) = self.signature_types.get(&id).cloned() {
            self.signature_types.insert(result, signatures);
        }
        if self.js_literal_types.contains(&id) {
            self.js_literal_types.insert(result);
        }
        if self.non_inferrable_types.contains(&id) {
            self.non_inferrable_types.insert(result);
        }
        result
    }
}
