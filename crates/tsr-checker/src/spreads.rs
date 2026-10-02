//! Type-level object spread fold, ported from checker.go:13283–13496.
use tsr_ast::{ObjectLiteralElementLike, ObjectLiteralExpression};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    Checker,
    flags::TypeFlags,
    index_signatures::IndexInfo,
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
                if !self.anonymous_properties.contains_key(&right) {
                    return self.intrinsics.error;
                }
                result = self.get_spread_type(result, right, owner, readonly);
            }
            start = index + 1;
            let Some(expression) = spread.expression else { return self.intrinsics.error };
            let source = self.check_expression(expression);
            if source == self.intrinsics.error || !self.is_valid_spread_type(source) {
                return self.intrinsics.error;
            }
            let source = self.try_merge_union_of_object_type_and_empty_object(source);
            result = self.get_spread_type(result, source, owner, readonly);
        }
        if start < node.properties.len() {
            let batch = ObjectLiteralExpression {
                node_id: node.node_id,
                properties: &node.properties[start..],
                multi_line: node.multi_line,
            };
            let right = self.check_object_literal_members(&batch);
            if !self.anonymous_properties.contains_key(&right) {
                return self.intrinsics.error;
            }
            result = self.get_spread_type(result, right, owner, readonly);
        }
        if result == self.intrinsics.empty_object {
            result = self.mint_spread_properties(Vec::new(), Vec::new(), owner);
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
        if let TypeData::Union { types, .. } = self.store.get(left).data.clone() {
            if types.len().saturating_mul(union_size(right)) >= 100_000 {
                return error;
            }
            let parts: Vec<_> = types
                .into_iter()
                .map(|part| self.get_spread_type(part, right, owner, readonly))
                .collect();
            return if parts.contains(&error) { error } else { self.get_union_type(&parts) };
        }
        right = self.try_merge_union_of_object_type_and_empty_object(right);
        if let TypeData::Union { types, .. } = self.store.get(right).data.clone() {
            if types.len() >= 100_000 {
                return error;
            }
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
        let Some(resolved_left) = self.resolved_spread_source(left) else { return error };
        let Some(resolved_right) = self.resolved_spread_source(right) else { return error };
        let indexes = if left == self.intrinsics.empty_object {
            self.get_index_infos_of_type(resolved_right)
        } else {
            self.union_index_infos(&[resolved_left, resolved_right])
        };
        let Some(mut indexes) = indexes else { return error };
        for index in &mut indexes {
            index.readonly = readonly;
        }
        let Some((mut properties, skipped_private)) = self.spread_properties(right, readonly)
        else {
            return error;
        };
        let Some((left_properties, _)) = self.spread_properties(left, readonly) else {
            return error;
        };
        for left_property in left_properties {
            if skipped_private.contains(&left_property.name) {
                continue;
            }
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
                    right_property.method = false;
                    right_property.accessor_write = None;
                    let displayed = self.remove_missing_type(value);
                    right_property.printed_type = self.type_to_string(displayed);
                }
            } else {
                properties.push(left_property);
            }
        }
        self.mint_spread_properties(properties, indexes, owner)
    }

    fn mint_spread_properties(
        &mut self,
        properties: Vec<AnonymousProperty>,
        indexes: Vec<IndexInfo>,
        owner: Option<SymbolId>,
    ) -> TypeId {
        let result = self.mint_anonymous_properties(properties, indexes, owner);
        if result != self.intrinsics.error {
            self.object_literal_spread_flags.insert(result, true);
            self.fresh_object_literal_types.insert(result);
        }
        result
    }

    /// `getRestType`'s `newAnonymousType(symbol, members, nil, nil,
    /// getIndexInfosOfType(source))` (`checker.go:17792`): the same anonymous
    /// member construction as a spread, without the object-literal and
    /// freshness flags a spread carries.
    pub(crate) fn mint_rest_properties(
        &mut self,
        properties: Vec<AnonymousProperty>,
        indexes: Vec<IndexInfo>,
    ) -> TypeId {
        self.mint_anonymous_properties(properties, indexes, None)
    }

    fn mint_anonymous_properties(
        &mut self,
        mut properties: Vec<AnonymousProperty>,
        indexes: Vec<IndexInfo>,
        owner: Option<SymbolId>,
    ) -> TypeId {
        properties.sort_by(|left, right| match (left.origin, right.origin) {
            (Some(left), Some(right)) => self.compare_symbols(left, right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => left.name.cmp(&right.name),
        });
        let mut members = Vec::new();
        for index in &indexes {
            let Some(index_members) = self.index_info_members(index) else {
                return self.intrinsics.error;
            };
            members.extend(index_members);
        }
        let Some(property_members) = self.anonymous_property_members(&properties) else {
            return self.intrinsics.error;
        };
        members.extend(property_members);
        let result = self.store.new_named(TypeFlags::OBJECT, render_object_type(&members), owner);
        self.object_literal_index_infos.insert(result, indexes);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_members.insert(result, members);
        result
    }

    /// Complete batches bypass the enclosing literal's binder table, which also
    /// contains declarations outside the current batch. Other sources resolve
    /// semantic property types through the reference/heritage instantiation seam.
    pub(crate) fn spread_properties(
        &mut self,
        source: TypeId,
        readonly: bool,
    ) -> Option<(Vec<AnonymousProperty>, Vec<String>)> {
        if source == self.intrinsics.empty_object
            || self.store.get(source).flags.intersects(TypeFlags::NULLABLE)
        {
            return Some((Vec::new(), Vec::new()));
        }
        let source = self.resolved_spread_source(source)?;
        if let TypeData::Intersection { types, .. } = self.store.get(source).data.clone() {
            return self.intersection_spread_properties(&types, readonly);
        }
        self.resolve_mapped_type_members(source);
        let captured =
            self.anonymous_properties.get(&source).map(|(properties, _)| properties.clone());
        let names = if let Some(properties) = &captured {
            properties.iter().map(|property| property.name.clone()).collect()
        } else {
            self.get_property_names_of_type(source)?
        };
        let mut properties = Vec::new();
        let mut skipped_private = Vec::new();
        for name in names {
            let held = captured
                .as_ref()
                .and_then(|properties| properties.iter().find(|property| property.name == name));
            let origin = held
                .map_or_else(|| self.property_origin(source, &name), |property| property.origin);
            if let Some(symbol) = origin {
                if self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                    self.member_declaration_has_modifier(
                        declaration,
                        tsr_ast::SyntaxKind::PrivateKeyword,
                    ) || self.member_declaration_has_modifier(
                        declaration,
                        tsr_ast::SyntaxKind::ProtectedKeyword,
                    )
                }) {
                    skipped_private.push(name);
                    continue;
                }
                if !self.is_spreadable_property(symbol) {
                    continue;
                }
            }
            let flags = origin
                .map_or(SymbolFlags::empty(), |symbol| self.binder.symbols().get(symbol).flags);
            let set_only = flags.contains(SymbolFlags::SET_ACCESSOR)
                && !flags.contains(SymbolFlags::GET_ACCESSOR);
            let mut property = if let Some(property) = held {
                property.clone()
            } else {
                let value = self.get_type_of_property_of_type(source, &name)?;
                if value == self.intrinsics.error {
                    return None;
                }
                let displayed = self.remove_missing_type(value);
                AnonymousProperty {
                    accessor_write: origin.and_then(|symbol| self.accessor_write_parameter(symbol)),
                    method: flags.contains(SymbolFlags::METHOD),
                    origin,
                    printed_name: self.spread_property_name(origin?, &name)?,
                    printed_type: self.type_to_string(displayed),
                    optional: origin.is_some_and(|symbol| self.property_is_optional(symbol)),
                    readonly: origin.is_some_and(|symbol| self.is_readonly_symbol(symbol)),
                    name,
                    r#type: value,
                }
            };
            // getSpreadSymbol reuses a method symbol only if readonly agrees.
            if property.readonly != readonly || set_only {
                property.method = false;
                property.accessor_write = None;
            }
            property.readonly = readonly;
            if set_only {
                property.r#type = self.intrinsics.undefined;
                "undefined".clone_into(&mut property.printed_type);
            } else if property.optional && self.strict_null_checks {
                property.r#type = self.get_optional_type(property.r#type, true);
            }
            properties.push(property);
        }
        Some((properties, skipped_private))
    }

    /// `getPropertiesOfUnionOrIntersectionType` (`checker.go:18861`) for an
    /// intersection source: names in constituent order, each read through
    /// `createUnionOrIntersectionProperty` (`checker.go:21452`). A name held by one
    /// constituent is that constituent's own symbol; a shared name is a
    /// synthetic `Property` whose type intersects the constituents' types and
    /// which is optional only when every holder is. A private or protected
    /// holder marks the synthetic property private, so it is skipped.
    fn intersection_spread_properties(
        &mut self,
        types: &[TypeId],
        readonly: bool,
    ) -> Option<(Vec<AnonymousProperty>, Vec<String>)> {
        let mut groups: Vec<Vec<AnonymousProperty>> = Vec::new();
        let mut skipped_private = Vec::new();
        for &ty in types {
            let (properties, skipped) = self.spread_properties(ty, readonly)?;
            for property in properties {
                if let Some(group) = groups.iter_mut().find(|group| group[0].name == property.name)
                {
                    group.push(property);
                } else {
                    groups.push(vec![property]);
                }
            }
            for name in skipped {
                if !skipped_private.contains(&name) {
                    skipped_private.push(name);
                }
            }
        }
        let mut properties = Vec::with_capacity(groups.len());
        for group in groups {
            if skipped_private.contains(&group[0].name) {
                continue;
            }
            if group.len() == 1 {
                properties.extend(group);
                continue;
            }
            let value_types: Vec<TypeId> = group.iter().map(|property| property.r#type).collect();
            let value = self.get_intersection_type(&value_types, None);
            if value == self.intrinsics.error {
                return None;
            }
            let mut combined = group[0].clone();
            combined.optional = group.iter().all(|property| property.optional);
            combined.method = false;
            combined.accessor_write = None;
            combined.r#type = value;
            let displayed = self.remove_missing_type(value);
            combined.printed_type = self.type_to_string(displayed);
            properties.push(combined);
        }
        Some((properties, skipped_private))
    }

    /// Alias symbols have no value members of their own. Resolve their body
    /// before enumerating members, while retaining the original alias identity
    /// on the generic spread/intersection path.
    pub(crate) fn resolved_spread_source(&mut self, mut source: TypeId) -> Option<TypeId> {
        let mut visited = Vec::new();
        while !self.mapped_types.contains_key(&source)
            && !self.anonymous_properties.contains_key(&source)
        {
            if visited.contains(&source) {
                return None;
            }
            visited.push(source);
            let Some((symbol, arguments)) = self.type_reference_targets.get(&source).cloned()
            else {
                break;
            };
            if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
                break;
            }
            source = self.evaluate_alias_body(symbol, &arguments)?;
            if source == self.intrinsics.error {
                return None;
            }
        }
        Some(source)
    }

    fn spread_property_name(&mut self, symbol: SymbolId, fallback: &str) -> Option<String> {
        for id in self.binder.symbols().get(symbol).declarations.clone() {
            let name = match self.node_map.get(id) {
                Some(tsr_ast::Node::PropertyDeclaration(node)) => Some(node.name),
                Some(tsr_ast::Node::PropertySignatureDeclaration(node)) => Some(node.name),
                Some(tsr_ast::Node::PropertyAssignment(node)) => Some(node.name),
                Some(tsr_ast::Node::MethodDeclaration(node)) => Some(node.name),
                Some(tsr_ast::Node::MethodSignatureDeclaration(node)) => Some(node.name),
                Some(tsr_ast::Node::GetAccessorDeclaration(node)) => Some(node.name),
                Some(tsr_ast::Node::SetAccessorDeclaration(node)) => Some(node.name),
                _ => None,
            };
            if let Some(tsr_ast::PropertyName::ComputedPropertyName(computed)) = name {
                return self.late_bound_symbol_member_name(computed).map(|(name, _)| name);
            }
        }
        Some(self.callable_property_name(symbol, fallback))
    }

    /// isSpreadableProperty approximates own properties using declaration kind:
    /// class methods/accessors and private identifier members are not copied.
    pub(crate) fn is_spreadable_property(&self, symbol: SymbolId) -> bool {
        let symbol = self.binder.symbols().get(symbol);
        let class_member = symbol.declarations.iter().any(|&id| {
            self.nodes.parent(id).and_then(|parent| self.node_map.get(parent)).is_some_and(
                |parent| {
                    matches!(
                        parent,
                        tsr_ast::Node::ClassDeclaration(_) | tsr_ast::Node::ClassExpression(_)
                    )
                },
            )
        });
        let private_identifier = symbol.declarations.iter().any(|&id| {
            matches!(self.node_map.get(id), Some(tsr_ast::Node::PropertyDeclaration(property))
                if matches!(property.name, tsr_ast::PropertyName::PrivateIdentifier(_)))
        });
        (!private_identifier
            && !symbol.flags.intersects(
                SymbolFlags::METHOD | SymbolFlags::GET_ACCESSOR | SymbolFlags::SET_ACCESSOR,
            ))
            || !class_member
    }

    pub(crate) fn anonymous_property_members(
        &mut self,
        properties: &[AnonymousProperty],
    ) -> Option<Vec<Member>> {
        let mut members = Vec::new();
        for property in properties {
            if let Some(write) = &property.accessor_write
                && !property.readonly
                && write.r#type != property.r#type
            {
                let name = &property.printed_name;
                members.push(Member::Signature {
                    printed: format!("get {name}(): {}", property.printed_type),
                });
                members.push(Member::Signature {
                    printed: format!(
                        "set {name}({}: {})",
                        write.name,
                        self.type_to_string(write.r#type)
                    ),
                });
                continue;
            }
            if !property.method || property.readonly {
                members.extend(crate::callable_expandos::property_members(std::slice::from_ref(
                    property,
                )));
                continue;
            }
            let value = self.remove_missing_or_undefined_type(property.r#type);
            let signatures = if let Some(signatures) = self.signature_types.get(&value) {
                signatures.clone()
            } else {
                match self.store.get(value).data {
                    TypeData::Anonymous { symbol, .. } => self.get_signatures_of_symbol(symbol)?,
                    _ => self.signature_candidates_of_named_type(
                        value,
                        crate::signatures::SignatureKind::Call,
                    )?,
                }
            };
            if signatures.is_empty() {
                return None;
            }
            let name =
                if property.printed_name == "new" { "\"new\"" } else { &property.printed_name };
            for signature in signatures {
                members.push(Member::Signature {
                    printed: format!(
                        "{name}{}{}",
                        if property.optional { "?" } else { "" },
                        crate::objects::signature_member_text(self, &signature)
                    ),
                });
            }
        }
        Some(members)
    }

    /// getGenericObjectFlags: object/index genericity are separate flags.
    fn spread_generic_flags(&mut self, ty: TypeId, visited: &mut Vec<TypeId>) -> (bool, bool) {
        if visited.contains(&ty) {
            return (false, false);
        }
        visited.push(ty);
        if let Some(resolved) = self.resolved_spread_source(ty)
            && resolved != ty
        {
            let result = self.spread_generic_flags(resolved, visited);
            visited.pop();
            return result;
        }
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
                || (flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
                    && !self.is_pattern_template(ty));
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
