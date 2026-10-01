//! Semantic mapped type metadata, ported from internal/checker/checker.go.
use crate::{Checker, types::TypeId};
use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

#[derive(Clone, Debug)]
pub(crate) struct MappedTypeInfo {
    pub(crate) parameter: TypeId,
    pub(crate) constraint: TypeId,
    pub(crate) constraint_intersection: Option<Vec<TypeId>>,
    pub(crate) template: TypeId,
    pub(crate) optionality: Option<bool>,
    pub(crate) readonly: Option<bool>,
    pub(crate) modifiers_source: Option<TypeId>,
    pub(crate) homomorphic_symbol: Option<SymbolId>,
}

impl<'a> Checker<'a, '_> {
    /// getTypeFromMappedTypeNode and createMappedTypeNodeFromType (checker.go,
    /// nodebuilderimpl.go). Build a deferred mapped type from semantic parts
    /// when its template is outside the bounded written-node renderer.
    pub(crate) fn create_semantic_mapped_type(
        &mut self,
        node: &'a tsr_ast::MappedTypeNode<'a>,
    ) -> Option<TypeId> {
        let info = self.mapped_type_info(node)?;
        let name = node.type_parameter?.name?.text;
        // The node builder preserves the top-level keyof operator even
        // when resolving its operand would produce a concrete key union.
        let constraint =
            if let Some(source) = info.modifiers_source {
                let text = self.type_to_string(source);
                if self.store.get(source).flags.intersects(
                    crate::flags::TypeFlags::UNION | crate::flags::TypeFlags::INTERSECTION,
                ) {
                    format!("keyof ({text})")
                } else {
                    format!("keyof {text}")
                }
            } else {
                self.type_to_string(info.constraint)
            };
        let template = self.type_to_string(info.template);
        let readonly = match node.readonly_token.map(|token| token.kind) {
            None => "",
            Some(SyntaxKind::ReadonlyKeyword) => "readonly ",
            Some(SyntaxKind::PlusToken) => "+readonly ",
            Some(SyntaxKind::MinusToken) => "-readonly ",
            _ => return None,
        };
        let optional = match node.question_token.map(|token| token.kind) {
            None => "",
            Some(SyntaxKind::QuestionToken) => "?",
            Some(SyntaxKind::PlusToken) => "+?",
            Some(SyntaxKind::MinusToken) => "-?",
            _ => return None,
        };
        let text = format!("{{ {readonly}[{name} in {constraint}]{optional}: {template}; }}");
        let ty = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(ty, info);
        Some(ty)
    }

    /// getIndexedMappedTypeSubstitutedTypeOfContextualType
    /// (checker.go:30607). Generic key domains use their base constraints,
    /// while substitution retains the mapped template's indexed identities.
    pub(crate) fn generic_mapped_contextual_property_type(
        &mut self,
        id: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        use crate::{flags::TypeFlags, types::TypeData};
        let info = self.mapped_types.get(&id)?.clone();
        let parameters: Vec<_> = self.type_parameter_symbols.keys().copied().collect();
        let constraints =
            info.constraint_intersection.clone().unwrap_or_else(|| vec![info.constraint]);
        if !constraints
            .iter()
            .any(|&constraint| self.mentions_type_parameter(constraint, &parameters, &[]))
        {
            return None;
        }
        let bases: Vec<_> = constraints
            .into_iter()
            .map(|constraint| {
                self.contextual_mapped_key_base_constraint(constraint, &mut Vec::new())
            })
            .collect();
        let constraint = self.get_intersection_type(&bases, None);
        let key = self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral(name.to_owned()),
            false,
        );
        if !self.is_type_assignable_to(key, constraint) {
            return None;
        }
        Some(self.instantiate_mapped_template(&info, key, false))
    }

    /// The parameter, union/intersection and index arms of
    /// computeBaseConstraint (checker.go:27486–27533).
    fn contextual_mapped_key_base_constraint(
        &mut self,
        id: TypeId,
        stack: &mut Vec<TypeId>,
    ) -> TypeId {
        use crate::{flags::TypeFlags, types::TypeData};
        if stack.contains(&id) {
            return id;
        }
        stack.push(id);
        let result = if self.store.get(id).flags.contains(TypeFlags::INDEX) {
            self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ])
        } else if let Some(constraint) = self.type_parameter_constraint(id) {
            self.contextual_mapped_key_base_constraint(constraint, stack)
        } else {
            match self.store.get(id).data.clone() {
                TypeData::Union { types, .. } => {
                    let types: Vec<_> = types
                        .into_iter()
                        .map(|ty| self.contextual_mapped_key_base_constraint(ty, stack))
                        .collect();
                    self.get_union_type(&types)
                }
                TypeData::Intersection { types, .. } => {
                    let types: Vec<_> = types
                        .into_iter()
                        .map(|ty| self.contextual_mapped_key_base_constraint(ty, stack))
                        .collect();
                    self.get_intersection_type(&types, None)
                }
                _ => id,
            }
        };
        stack.pop();
        result
    }

    /// isGenericMappedType plus getHomomorphicTypeVariable for tuple context.
    pub(crate) fn is_generic_homomorphic_mapped_type(&self, id: TypeId) -> bool {
        self.is_generic_homomorphic_mapped_type_inner(id, &mut Vec::new())
    }

    fn is_generic_homomorphic_mapped_type_inner(
        &self,
        id: TypeId,
        visited: &mut Vec<TypeId>,
    ) -> bool {
        if visited.contains(&id) {
            return false;
        }
        visited.push(id);
        self.mapped_types.get(&id).is_some_and(|mapped| {
            self.deferred_keyof_operands.get(&mapped.constraint).is_some_and(|operand| {
                self.store.get(*operand).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
                    || (mapped.homomorphic_symbol.is_some()
                        && self.is_generic_homomorphic_mapped_type_inner(*operand, visited))
            })
        })
    }
    /// getConstraintTypeFromMappedType/getTemplateTypeFromMappedType. Keep
    /// semantic indexed accesses during template evaluation, under its mapper.
    pub(crate) fn capture_mapped_type(
        &mut self,
        id: TypeId,
        node: &'a tsr_ast::MappedTypeNode<'a>,
    ) {
        if let Some(info) = self.mapped_type_info(node) {
            self.mapped_types.insert(id, info);
        }
    }

    fn mapped_type_info(
        &mut self,
        node: &'a tsr_ast::MappedTypeNode<'a>,
    ) -> Option<MappedTypeInfo> {
        if node.name_type.is_some() {
            return None;
        }
        let parameter = node.type_parameter?;
        let symbol = parameter.node_id.and_then(|id| self.binder.symbol_of(id))?;
        let parameter_type = self.get_declared_type_of_symbol(symbol);
        let constraint = parameter.constraint?;
        let template = node.r#type?;
        let mut constraint_node = constraint;
        while let TypeNode::ParenthesizedTypeNode(node) = constraint_node {
            let Some(inner) = node.r#type else { break };
            constraint_node = inner;
        }
        // getLimitedConstraint reads the unreduced intersection origin even
        // when intersection normalization distributes it into a union.
        let constraint_intersection = if let TypeNode::IntersectionTypeNode(node) = constraint_node
        {
            Some(node.types.iter().map(|&ty| self.mapped_constraint_type(ty)).collect::<Vec<_>>())
        } else {
            None
        };
        let mut modifiers_source = None;
        let mut homomorphic_symbol = None;
        let constraint = if let Some(parts) = &constraint_intersection {
            self.get_intersection_type(parts, None)
        } else if let TypeNode::TypeOperatorNode(operator) = constraint
            && operator.operator.kind == SyntaxKind::KeyOfKeyword
            && let Some(operand) = operator.r#type
        {
            if let TypeNode::TypeReferenceNode(reference) = operand {
                homomorphic_symbol = reference.type_name.and_then(|name| {
                    self.resolve_entity_name(name, SymbolFlags::TYPE).filter(|&symbol| {
                        self.binder
                            .symbols()
                            .get(symbol)
                            .flags
                            .contains(SymbolFlags::TYPE_PARAMETER)
                    })
                });
            }
            let operand = self.get_type_from_type_node(operand);
            modifiers_source = Some(operand);
            self.resolved_keyof_type(operand).unwrap_or(self.intrinsics.error)
        } else {
            self.mapped_constraint_type(constraint)
        };
        self.mapped_template_depth += 1;
        let template = self.get_type_from_type_node(template);
        self.mapped_template_depth -= 1;

        if constraint == self.intrinsics.error || template == self.intrinsics.error {
            return None;
        }
        Some(MappedTypeInfo {
            parameter: parameter_type,
            constraint,
            constraint_intersection,
            template,
            optionality: node.question_token.map(|token| token.kind != SyntaxKind::MinusToken),
            readonly: node.readonly_token.map(|token| token.kind != SyntaxKind::MinusToken),
            modifiers_source,
            homomorphic_symbol,
        })
    }

    /// Resolve key operators semantically under a mapped type's mapper.
    /// Union/intersection constraints retain each keyof operand for inference.
    pub(crate) fn mapped_constraint_type(&mut self, node: TypeNode<'a>) -> TypeId {
        match node {
            TypeNode::TypeOperatorNode(operator)
                if operator.operator.kind == SyntaxKind::KeyOfKeyword =>
            {
                let Some(operand) = operator.r#type else { return self.intrinsics.error };
                let operand = self.get_type_from_type_node(operand);
                self.resolved_keyof_type(operand).unwrap_or(self.intrinsics.error)
            }
            TypeNode::ParenthesizedTypeNode(node) => {
                node.r#type.map_or(self.intrinsics.error, |ty| self.mapped_constraint_type(ty))
            }
            TypeNode::UnionTypeNode(node) => {
                let types: Vec<_> =
                    node.types.iter().map(|&ty| self.mapped_constraint_type(ty)).collect();
                self.get_union_type(&types)
            }
            TypeNode::IntersectionTypeNode(node) => {
                let types: Vec<_> =
                    node.types.iter().map(|&ty| self.mapped_constraint_type(ty)).collect();
                self.get_intersection_type(&types, None)
            }
            _ => self.get_type_from_type_node(node),
        }
    }

    /// A mapped alias keeps its printed identity while its mapper supplies
    /// the constraint/template identities used by inference.
    pub(crate) fn capture_mapped_alias(
        &mut self,
        id: TypeId,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return;
        }
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return;
        };
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return;
        };
        let Some(TypeNode::MappedTypeNode(mapped)) = alias.r#type else { return };
        if mapped.name_type.is_some()
            || alias.type_parameters.len() != arguments.len()
            || !self.mapped_alias_in_progress.insert(symbol)
        {
            return;
        }
        let frame = alias
            .type_parameters
            .iter()
            .filter_map(|p| p.node_id)
            .filter_map(|id| self.binder.symbol_of(id))
            .zip(arguments.iter().copied())
            .collect();
        self.alias_evaluation_bindings.push(frame);
        self.capture_mapped_type(id, mapped);
        self.alias_evaluation_bindings.pop();
        self.mapped_alias_in_progress.remove(&symbol);
    }

    /// resolveMappedTypeMembers (checker.go:20894). Enumerate known property
    /// keys and capture their template substitutions before publishing members.
    pub(crate) fn resolve_mapped_type_members(&mut self, id: TypeId) {
        if !self.mapped_types.contains_key(&id)
            || self.anonymous_properties.contains_key(&id)
            || !self.mapped_members_in_progress.insert(id)
        {
            return;
        }
        self.resolve_mapped_type_members_worker(id);
        self.mapped_members_in_progress.remove(&id);
    }

    fn resolve_mapped_type_members_worker(&mut self, id: TypeId) {
        use crate::{flags::TypeFlags, types::TypeData};
        let Some(info) = self.mapped_types.get(&id).cloned() else { return };
        if self.anonymous_properties.contains_key(&id) {
            return;
        }
        let mut keys = Vec::new();
        let modifiers = info.modifiers_source.map(|source| self.apparent_type(source));
        if let Some(source) = modifiers {
            if self.store.get(source).flags.contains(TypeFlags::TYPE_PARAMETER) {
                return;
            }
            for name in self.property_names_of(source) {
                keys.push(self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(name),
                    false,
                ));
            }
            if let Some(indexes) = self.get_index_infos_of_type(source) {
                keys.extend(indexes.into_iter().map(|index| index.key));
            }
        } else {
            let mut pending = vec![info.constraint];
            while let Some(key) = pending.pop() {
                if let TypeData::Union { types, .. } = &self.store.get(key).data {
                    pending.extend(types.iter().rev().copied());
                } else if self.store.get(key).flags.intersects(
                    TypeFlags::STRING_LITERAL
                        | TypeFlags::NUMBER_LITERAL
                        | TypeFlags::STRING
                        | TypeFlags::NUMBER,
                ) {
                    keys.push(key);
                } else if key != self.intrinsics.never {
                    return;
                }
            }
        }
        // Recursive references observe the empty table, as upstream's upfront
        // setStructuredTypeMembers does. Types are published after substitution.
        self.anonymous_properties.insert(id, (Vec::new(), true));
        let mut properties = Vec::new();
        let mut indexes = Vec::new();
        for key in keys {
            let name = match &self.store.get(key).data {
                TypeData::StringLiteral(name) | TypeData::NumberLiteral(name) => Some(name.clone()),
                _ => None,
            };
            let Some(name) = name else {
                let value = self.instantiate_type(
                    info.template,
                    &[(info.parameter, key)],
                    &[info.parameter],
                    &[],
                );
                indexes.push(crate::index_signatures::IndexInfo { key, value });
                continue;
            };
            if properties
                .iter()
                .any(|property: &crate::objects::AnonymousProperty| property.name == name)
            {
                continue;
            }
            let source_property =
                modifiers.and_then(|source| self.get_property_of_type(source, &name));
            let captured =
                modifiers.and_then(|source| self.anonymous_properties.get(&source)).and_then(
                    |(properties, _)| properties.iter().find(|property| property.name == name),
                );
            let was_optional = captured.map_or_else(
                || source_property.is_some_and(|property| self.property_is_optional(property)),
                |property| property.optional,
            );
            let was_readonly = captured.map_or_else(
                || source_property.is_some_and(|property| self.is_readonly_property(property)),
                |property| property.readonly,
            );
            let optional = info.optionality.unwrap_or(was_optional);
            let readonly = info.readonly.unwrap_or(was_readonly);
            let printed_name =
                captured.map_or_else(|| name.clone(), |property| property.printed_name.clone());
            let mut value = self.instantiate_type(
                info.template,
                &[(info.parameter, key)],
                &[info.parameter],
                &[],
            );
            // getTypeOfMappedSymbol (checker.go:20993). Excluding optionality
            // strips undefined from an originally optional source property.
            if self.strict_null_checks && !optional && was_optional {
                value = self.get_type_with_facts(value, crate::flow::TypeFacts::NE_UNDEFINED);
            }
            properties.push(crate::objects::AnonymousProperty {
                name,
                printed_name,
                printed_type: self.type_to_string(value),
                optional,
                readonly,
                r#type: value,
            });
        }
        self.anonymous_properties.insert(id, (properties, true));
        self.object_literal_index_infos.insert(id, indexes);
    }

    /// instantiateMappedArrayType/instantiateMappedTupleType
    /// (checker.go:22585). Homomorphic aliases transform sequence elements
    /// before resolving ordinary object members.
    pub(crate) fn instantiate_mapped_alias_sequence(
        &mut self,
        id: TypeId,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        use crate::{flags::TypeFlags, tuples::TupleElement, types::TypeData};
        let info = self.mapped_types.get(&id)?.clone();
        let source = info.modifiers_source?;
        let parameter = info.homomorphic_symbol?;
        if self.store.get(source).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return Some(source);
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let slot = alias
            .type_parameters
            .iter()
            .position(|p| p.node_id.and_then(|id| self.binder.symbol_of(id)) == Some(parameter))?;
        let replace_source = |checker: &mut Self, source: TypeId| {
            let mut arguments = arguments.to_vec();
            arguments[slot] = source;
            checker.create_type_reference(symbol, arguments)
        };
        if let TypeData::Union { types, .. } = &self.store.get(source).data {
            let types = types.clone();
            let mapped: Vec<_> = types.into_iter().map(|ty| replace_source(self, ty)).collect();
            let union = self.get_union_type(&mapped);
            if union == self.intrinsics.error {
                return None;
            }
            // mapTypeWithAlias retains the mapped alias and its arguments
            // on a distributed union, while exposing its constituents.
            let alias_text = self.type_to_string(id);
            return Some(self.union_with_origin_text(union, alias_text));
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(source).data {
            let types = types.clone();
            if types.iter().all(|&ty| self.is_mapped_sequence_input(ty)) {
                let mapped: Vec<_> = types.into_iter().map(|ty| replace_source(self, ty)).collect();
                return Some(self.get_intersection_type(&mapped, None));
            }
        }
        let tuple = self.variadic_tuple_elements.get(&source).cloned().or_else(|| {
            self.tuple_element_lists.get(&source).map(|(types, readonly)| {
                let mask = self.tuple_optional_masks.get(&source);
                let labels = self.tuple_labels.get(&source);
                (
                    types
                        .iter()
                        .enumerate()
                        .map(|(i, &ty)| TupleElement {
                            r#type: ty,
                            spread: false,
                            optional: mask.and_then(|m| m.get(i)).copied().unwrap_or(false),
                            label: labels.and_then(|l| l.get(i)).cloned().flatten(),
                        })
                        .collect::<Vec<_>>(),
                    *readonly,
                )
            })
        });
        if let Some((mut elements, readonly)) = tuple {
            let fixed = elements.iter().take_while(|e| !e.spread).count();
            for (index, element) in elements.iter_mut().enumerate() {
                if index < fixed {
                    let key = self.store.intern_literal(
                        TypeFlags::STRING_LITERAL,
                        TypeData::StringLiteral(index.to_string()),
                        false,
                    );
                    element.r#type = self.instantiate_mapped_template(&info, key, element.optional);
                } else if element.spread {
                    element.r#type = replace_source(self, element.r#type);
                } else {
                    let array = self.global_type_symbol("Array")?;
                    let array = self.create_type_reference(array, vec![element.r#type]);
                    let mapped = replace_source(self, array);
                    element.r#type =
                        self.tuple_spread_array_element(mapped).unwrap_or(self.intrinsics.unknown);
                }
                if !element.spread {
                    element.optional = info.optionality.unwrap_or(element.optional);
                    // TupleNormalizer.add applies optionality to the new
                    // element type as well as retaining its element flag.
                    if element.optional && self.strict_null_checks {
                        element.r#type = self.get_optional_type(element.r#type, true);
                    }
                }
                if element.r#type == self.intrinsics.error {
                    return Some(element.r#type);
                }
            }
            return Some(
                self.normalize_variadic_tuple(elements, info.readonly.unwrap_or(readonly)),
            );
        }
        let any_array = if self.store.get(source).flags.contains(TypeFlags::ANY) {
            let variable = self.get_declared_type_of_symbol(parameter);
            self.type_parameter_constraint(variable).is_some_and(|constraint| {
                let types = match &self.store.get(constraint).data {
                    TypeData::Union { types, .. } => types.clone(),
                    _ => vec![constraint],
                };
                types.into_iter().all(|ty| {
                    !matches!(self.store.get(ty).data, TypeData::Intersection { .. })
                        && self.is_mapped_sequence_input(ty)
                })
            })
        } else {
            false
        };
        if any_array
            || (!self.store.get(source).flags.contains(TypeFlags::ANY)
                && self.tuple_spread_array_element(source).is_some())
        {
            let element = self.instantiate_mapped_template(&info, self.intrinsics.number, true);
            if element == self.intrinsics.error {
                return Some(element);
            }
            let readonly = self.type_reference_targets.get(&source).is_some_and(|(symbol, _)| {
                self.global_type_symbol("ReadonlyArray") == Some(*symbol)
            });
            let array = self.global_type_symbol(if info.readonly.unwrap_or(readonly) {
                "ReadonlyArray"
            } else {
                "Array"
            })?;
            return Some(self.create_type_reference(array, vec![element]));
        }
        None
    }

    /// isArrayOrTupleOrIntersection (checker.go): only concrete sequence
    /// constituents enter the intersection transformation branch.
    fn is_mapped_sequence_input(&mut self, id: TypeId) -> bool {
        if self.tuple_element_lists.contains_key(&id)
            || self.variadic_tuple_elements.contains_key(&id)
        {
            return true;
        }
        if let crate::types::TypeData::Intersection { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            return types.into_iter().all(|ty| self.is_mapped_sequence_input(ty));
        }
        !self.store.get(id).flags.contains(crate::flags::TypeFlags::ANY)
            && self.tuple_spread_array_element(id).is_some()
    }

    /// instantiateMappedTypeTemplate (checker.go:22646). Include optionality
    /// before tuple construction; exclude only undefined from optional inputs.
    fn instantiate_mapped_template(
        &mut self,
        info: &MappedTypeInfo,
        key: TypeId,
        optional: bool,
    ) -> TypeId {
        let value =
            self.instantiate_type(info.template, &[(info.parameter, key)], &[info.parameter], &[]);
        if self.strict_null_checks && info.optionality == Some(true) {
            self.get_optional_type(value, true)
        } else if self.strict_null_checks && info.optionality == Some(false) && optional {
            self.get_type_with_facts(value, crate::flow::TypeFacts::NE_UNDEFINED)
        } else {
            value
        }
    }

    /// getResolvedApparentTypeOfMappedType (checker.go:21772). A generic
    /// homomorphic alias with an array/tuple base constraint exposes the mapped
    /// sequence's methods, rather than transforming the array's method names.
    pub(crate) fn apparent_mapped_type(&mut self, id: TypeId) -> TypeId {
        let Some(info) = self.mapped_types.get(&id).cloned() else { return id };
        if let Some(&cached) = self.mapped_apparent_types.get(&id) {
            return cached;
        }
        self.mapped_apparent_types.insert(id, id);
        let resolved = (|| {
            let source = info.modifiers_source?;
            let parameter = info.homomorphic_symbol?;
            let base = if self.is_generic_homomorphic_mapped_type(source) {
                self.apparent_mapped_type(source)
            } else {
                self.type_parameter_constraint(source)?
            };
            let types = match &self.store.get(base).data {
                crate::types::TypeData::Union { types, .. } => types.clone(),
                _ => vec![base],
            };
            if !types.into_iter().all(|ty| self.is_mapped_sequence_input(ty)) {
                return None;
            }
            let (symbol, mut arguments) = self.type_reference_targets.get(&id)?.clone();
            let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
            let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
                return None;
            };
            let index = alias.type_parameters.iter().position(|p| {
                p.node_id.and_then(|id| self.binder.symbol_of(id)) == Some(parameter)
            })?;
            arguments[index] = base;
            Some(self.create_type_reference(symbol, arguments))
        })()
        .unwrap_or(id);
        self.mapped_apparent_types.insert(id, resolved);
        resolved
    }
}
