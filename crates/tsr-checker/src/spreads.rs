//! Type-level object spread fold, ported from checker.go:13283–13496.
use tsr_ast::{ObjectLiteralElementLike, ObjectLiteralExpression};
use tsr_binder::SymbolId;

use crate::{
    Checker,
    flags::TypeFlags,
    objects::{AnonymousProperty, Member, render_object_type},
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    pub(crate) fn check_object_spread_literal(
        &mut self,
        node: &ObjectLiteralExpression<'_>,
    ) -> TypeId {
        let readonly = node.node_id.is_some_and(|id| self.is_const_context(id));
        let owner = node.node_id.and_then(|id| self.binder.symbol_of(id));
        let mut result = self.intrinsics.empty_object;
        let mut start = 0;
        for (index, property) in node.properties.iter().enumerate() {
            let ObjectLiteralElementLike::SpreadAssignment(spread) = property else { continue };
            if start < index {
                let batch = ObjectLiteralExpression {
                    node_id: node.node_id,
                    properties: &node.properties[start..index],
                    multi_line: node.multi_line,
                };
                let right = self.check_object_literal_members(&batch);
                result = self.get_spread_type(result, right, owner, readonly);
            }
            start = index + 1;
            let Some(expression) = spread.expression else { return self.intrinsics.error };
            let source = self.check_expression(expression);
            if source == self.intrinsics.error || !self.is_valid_spread_type(source) {
                return self.intrinsics.error;
            }
            result = self.get_spread_type(result, source, owner, readonly);
        }
        if start < node.properties.len() {
            let batch = ObjectLiteralExpression {
                node_id: node.node_id,
                properties: &node.properties[start..],
                multi_line: node.multi_line,
            };
            let right = self.check_object_literal_members(&batch);
            result = self.get_spread_type(result, right, owner, readonly);
        }
        if result == self.intrinsics.empty_object {
            result = self.mint_spread_properties(Vec::new(), owner);
        }
        if node.node_id.is_some_and(|id| self.in_js_file(id)) {
            let parts = match self.store.get(result).data.clone() {
                TypeData::Union { types, .. } => types,
                _ => vec![result],
            };
            for part in parts {
                if self.object_literal_spread_flags.contains_key(&part) {
                    self.js_literal_types.insert(part);
                }
            }
        }
        result
    }

    /// Native getSpreadType folds whole types before resolving concrete members.
    /// Union distribution and generic intersections must precede property lookup.
    fn get_spread_type(
        &mut self,
        mut left: TypeId,
        mut right: TypeId,
        owner: Option<SymbolId>,
        readonly: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if left == error || right == error {
            return error;
        }
        let flags = self.store.get(left).flags | self.store.get(right).flags;
        if flags.intersects(TypeFlags::ANY) {
            return self.intrinsics.any;
        }
        if flags.intersects(TypeFlags::UNKNOWN) {
            return self.intrinsics.unknown;
        }
        if self.store.get(left).flags.intersects(TypeFlags::NEVER) {
            return right;
        }
        if self.store.get(right).flags.intersects(TypeFlags::NEVER) {
            return left;
        }
        left = self.try_merge_union_of_object_type_and_empty_object(left);
        let union_size = |ty| match &self.store.get(ty).data {
            TypeData::Union { types, .. } => types.len(),
            _ => 1,
        };
        if union_size(left).saturating_mul(union_size(right)) >= 100_000 {
            return error;
        }
        if let TypeData::Union { types, .. } = self.store.get(left).data.clone() {
            let parts: Vec<_> = types
                .into_iter()
                .map(|part| self.get_spread_type(part, right, owner, readonly))
                .collect();
            return if parts.contains(&error) { error } else { self.get_union_type(&parts) };
        }
        right = self.try_merge_union_of_object_type_and_empty_object(right);
        if let TypeData::Union { types, .. } = self.store.get(right).data.clone() {
            let parts: Vec<_> = types
                .into_iter()
                .map(|part| self.get_spread_type(left, part, owner, readonly))
                .collect();
            return if parts.contains(&error) { error } else { self.get_union_type(&parts) };
        }
        if self.store.get(right).flags.intersects(
            TypeFlags::BOOLEAN_LIKE
                | TypeFlags::NUMBER_LIKE
                | TypeFlags::BIG_INT_LIKE
                | TypeFlags::STRING_LIKE
                | TypeFlags::ENUM_LIKE
                | TypeFlags::NON_PRIMITIVE
                | TypeFlags::INDEX,
        ) {
            return left;
        }
        if self.spread_generic_flags(left, &mut Vec::new()).0
            || self.spread_generic_flags(right, &mut Vec::new()).0
        {
            if left == self.intrinsics.empty_object || self.is_empty_anonymous_object_type(left) {
                return right;
            }
            if let TypeData::Intersection { mut types, .. } = self.store.get(left).data.clone()
                && let Some(last) = types.last().copied()
                && self.store.get(last).flags.intersects(TypeFlags::OBJECT)
                && self.store.get(right).flags.intersects(TypeFlags::OBJECT)
                && !self.spread_generic_flags(last, &mut Vec::new()).0
                && !self.spread_generic_flags(right, &mut Vec::new()).0
            {
                let merged = self.get_spread_type(last, right, owner, readonly);
                if merged == error {
                    return error;
                }
                *types.last_mut().unwrap() = merged;
                return self.get_intersection_type(&types, None);
            }
            return self.get_intersection_type(&[left, right], None);
        }
        let Some(mut properties) = self.spread_properties(right, readonly) else { return error };
        let Some(left_properties) = self.spread_properties(left, readonly) else { return error };
        for left_property in left_properties {
            if let Some(right_property) =
                properties.iter_mut().find(|p| p.name == left_property.name)
            {
                if right_property.optional {
                    let left_type = left_property.r#type;
                    let left_present = self.remove_missing_or_undefined_type(left_type);
                    let right_present =
                        self.remove_missing_or_undefined_type(right_property.r#type);
                    let value = if left_present == right_present {
                        left_type
                    } else {
                        let Some(value) =
                            self.union_with_subtype_reduction(&[left_type, right_present])
                        else {
                            return error;
                        };
                        value
                    };
                    *right_property = left_property;
                    right_property.r#type = value;
                    // A collision creates a new property symbol without Readonly.
                    right_property.readonly = false;
                    let displayed = self.remove_missing_type(value);
                    right_property.printed_type = self.type_to_string(displayed);
                }
            } else {
                properties.push(left_property);
            }
        }
        self.mint_spread_properties(properties, owner)
    }

    fn mint_spread_properties(
        &mut self,
        mut properties: Vec<AnonymousProperty>,
        owner: Option<SymbolId>,
    ) -> TypeId {
        properties.sort_by(|left, right| match (left.origin, right.origin) {
            (Some(left), Some(right)) => self.compare_symbols(left, right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => left.name.cmp(&right.name),
        });
        let members = crate::callable_expandos::property_members(&properties);
        let result = self.store.new_named(TypeFlags::OBJECT, render_object_type(&members), owner);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_members.insert(result, members);
        self.object_literal_spread_flags.insert(result, true);
        self.fresh_object_literal_types.insert(result);
        result
    }

    /// Complete typed batches bypass the literal's binder table: the latter
    /// includes declarations from every batch of this same object literal.
    fn spread_properties(
        &mut self,
        source: TypeId,
        readonly: bool,
    ) -> Option<Vec<AnonymousProperty>> {
        if source == self.intrinsics.empty_object
            || self.store.get(source).flags.intersects(TypeFlags::NULLABLE)
        {
            return Some(Vec::new());
        }
        self.resolve_mapped_type_members(source);
        // Index declaration provenance and method symbol flags are not yet
        // carried by this representation. Keep those incomplete sources gapped.
        if !self.get_index_infos_of_type(source)?.is_empty() {
            return None;
        }
        if let Some((properties, _)) = self.anonymous_properties.get(&source) {
            let mut properties = properties.clone();
            for property in &mut properties {
                property.readonly = readonly;
                // Captured declarations store the written value type. A symbol
                // read includes its optional marker before merging spread values.
                if property.optional && self.strict_null_checks {
                    property.r#type = self.get_optional_type(property.r#type, true);
                }
            }
            return Some(properties);
        }
        let members = self.spread_members_of(source)?;
        members
            .into_iter()
            .map(|member| {
                let Member::Property { name, optional, printed, .. } = member else { return None };
                let value = self.get_type_of_property_of_type(source, &name)?;
                (value != self.intrinsics.error).then(|| AnonymousProperty {
                    origin: self.property_origin(source, &name),
                    name: name.clone(),
                    printed_name: name,
                    printed_type: printed,
                    optional,
                    readonly,
                    r#type: value,
                })
            })
            .collect()
    }

    /// getGenericObjectFlags: object/index genericity are separate flags.
    fn spread_generic_flags(&mut self, ty: TypeId, visited: &mut Vec<TypeId>) -> (bool, bool) {
        if visited.contains(&ty) {
            return (false, false);
        }
        visited.push(ty);
        let flags = self.store.get(ty).flags;
        let result = if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.store.get(ty).data.clone()
        {
            types.into_iter().fold((false, false), |result, part| {
                let part = self.spread_generic_flags(part, visited);
                (result.0 || part.0, result.1 || part.1)
            })
        } else {
            let mut object = flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE);
            let index = flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE | TypeFlags::INDEX)
                || self.template_literal_parts.get(&ty).cloned().is_some_and(|parts| {
                    parts.types.iter().any(|&part| self.spread_generic_flags(part, visited).1)
                })
                || self
                    .string_mapping_types
                    .get(&ty)
                    .copied()
                    .is_some_and(|(_, part)| self.spread_generic_flags(part, visited).1);
            if let Some(mapped) = self.mapped_types.get(&ty).cloned() {
                object |= self.spread_generic_flags(mapped.constraint, visited).1;
                if let Some(name) = mapped.name_type {
                    let name = self.instantiate_type(
                        name,
                        &[(mapped.parameter, mapped.constraint)],
                        &[mapped.parameter],
                        &[],
                    );
                    object |= self.spread_generic_flags(name, visited).1;
                }
            }
            if let Some((elements, _)) = self.variadic_tuple_elements.get(&ty).cloned() {
                object |= elements.iter().any(|element| {
                    element.spread && self.tuple_spread_array_element(element.r#type).is_none()
                });
            }
            (object, index)
        };
        visited.pop();
        result
    }
}
