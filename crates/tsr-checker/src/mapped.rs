//! Semantic mapped type metadata, ported from internal/checker/checker.go.
use crate::{Checker, types::TypeId};
use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

#[derive(Clone, Debug)]
pub(crate) struct MappedTypeInfo {
    pub(crate) declaration: tsr_ast::NodeId,
    pub(crate) parameter: TypeId,
    pub(crate) constraint: TypeId,
    pub(crate) constraint_intersection: Option<Vec<TypeId>>,
    pub(crate) template: TypeId,
    pub(crate) name_type: Option<TypeId>,
    pub(crate) optionality: Option<bool>,
    pub(crate) readonly: Option<bool>,
    pub(crate) modifiers_source: Option<TypeId>,
    pub(crate) keyof_constraint: bool,
    pub(crate) homomorphic_symbol: Option<SymbolId>,
}

/// `ReverseMappedType` (types.go): the source, mapped target and constraint
/// whose members resolveReverseMappedTypeMembers produces on first read.
#[derive(Clone, Debug)]
pub(crate) struct ReverseMappedInfo {
    pub(crate) source: TypeId,
    pub(crate) target: TypeId,
    pub(crate) info: MappedTypeInfo,
    pub(crate) operand: TypeId,
    pub(crate) constraint: TypeId,
}

/// A deferred conditional retains the declaration and outer mapper, just as
/// ConditionalRoot/getConditionalTypeInstantiation do in checker.go.
#[derive(Clone, Debug)]
pub(crate) struct MappedConditionalInfo {
    pub(crate) declaration: tsr_ast::NodeId,
    pub(crate) bindings: rustc_hash::FxHashMap<SymbolId, TypeId>,
    pub(crate) operands: [TypeId; 4],
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
        let text = self.mapped_type_text(&info)?;
        let ty = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(ty, info);
        Some(ty)
    }

    fn mapped_type_text(&self, info: &MappedTypeInfo) -> Option<String> {
        let Some(Node::MappedTypeNode(node)) = self.node_map.get(info.declaration) else {
            return None;
        };
        let name = node.type_parameter?.name?.text;
        // The node builder preserves the top-level keyof operator even
        // when resolving its operand would produce a concrete key union.
        let constraint =
            if let Some(source) = info.modifiers_source
                && info.keyof_constraint
            {
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
        let remapping = info
            .name_type
            .map_or_else(String::new, |ty| format!(" as {}", self.type_to_string(ty)));
        Some(format!("{{ {readonly}[{name} in {constraint}{remapping}]{optional}: {template}; }}"))
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
        if let Some(name_type) = info.name_type {
            // getMappedTypeNameTypeKind relates a conditional through its
            // default constraint (getDefaultConstraintOfConditionalType).
            let name_constraint =
                if let Some(&(yes, no)) = self.mapped_conditional_branches.get(&name_type) {
                    if self.store.get(yes).flags.contains(TypeFlags::ANY) {
                        no
                    } else if self.store.get(no).flags.contains(TypeFlags::ANY) {
                        yes
                    } else {
                        self.get_union_type(&[yes, no])
                    }
                } else {
                    name_type
                };
            if !self.is_type_assignable_to(name_constraint, info.parameter) {
                return None;
            }
        }
        let constraints =
            info.constraint_intersection.clone().unwrap_or_else(|| vec![info.constraint]);
        if !constraints
            .iter()
            .any(|&constraint| self.mentions_registered_type_parameter(constraint))
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
        if self.is_excluded_mapped_property_name(info.constraint, key)
            || info.name_type.is_some_and(|ty| self.is_excluded_mapped_property_name(ty, key))
        {
            return None;
        }
        if !self.is_type_assignable_to(key, constraint) {
            return None;
        }
        Some(self.instantiate_mapped_template(&info, key, false))
    }

    /// isExcludedMappedPropertyName (checker.go:30624), for a conditional
    /// that excludes its extends type and otherwise keeps the check variable.
    fn is_excluded_mapped_property_name(&mut self, ty: TypeId, key: TypeId) -> bool {
        if let Some(info) = self.mapped_conditionals.get(&ty).cloned() {
            let [check, extends, yes, no] = info.operands;
            return yes == self.intrinsics.never
                && no == check
                && self.is_type_assignable_to(key, extends);
        }
        if let crate::types::TypeData::Intersection { types, .. } = self.store.get(ty).data.clone()
        {
            return types.into_iter().any(|ty| self.is_excluded_mapped_property_name(ty, key));
        }
        false
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
        // Only a mapped type recurses; anything else answers without
        // recording a visit (and without allocating the visited list).
        let Some(mapped) = self.mapped_types.get(&id) else { return false };
        if visited.contains(&id) {
            return false;
        }
        visited.push(id);
        self.deferred_keyof_operands.get(&mapped.constraint).is_some_and(|operand| {
            self.store
                .get(*operand)
                .flags
                .intersects(crate::flags::TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
                || (mapped.homomorphic_symbol.is_some()
                    && self.is_generic_homomorphic_mapped_type_inner(*operand, visited))
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
            modifiers_source = self.indirect_mapped_modifiers_source(constraint);
            self.mapped_constraint_type(constraint)
        };
        self.mapped_template_depth += 1;
        let template = self.get_type_from_type_node(template);
        let name_type = node.name_type.map(|node| self.get_type_from_type_node(node));
        self.mapped_template_depth -= 1;

        if constraint == self.intrinsics.error
            || template == self.intrinsics.error
            || name_type == Some(self.intrinsics.error)
        {
            return None;
        }
        Some(MappedTypeInfo {
            declaration: node.node_id?,
            parameter: parameter_type,
            constraint,
            constraint_intersection,
            template,
            name_type,
            optionality: node.question_token.map(|token| token.kind != SyntaxKind::MinusToken),
            readonly: node.readonly_token.map(|token| token.kind != SyntaxKind::MinusToken),
            modifiers_source,
            keyof_constraint: matches!(parameter.constraint, Some(TypeNode::TypeOperatorNode(operator))
                if operator.operator.kind == SyntaxKind::KeyOfKeyword),
            homomorphic_symbol,
        })
    }

    /// getModifiersTypeFromMappedType (checker.go:28127): a declared key
    /// parameter can inherit `keyof T`. Resolve that declaration before applying
    /// the active alias mapper, so a concrete key union cannot erase T's identity.
    fn indirect_mapped_modifiers_source(&mut self, constraint: TypeNode<'a>) -> Option<TypeId> {
        let bindings = std::mem::take(&mut self.alias_evaluation_bindings);
        let declared = self.mapped_constraint_type(constraint);
        let extended = self.type_parameter_constraint(declared).unwrap_or(declared);
        let operand = self.deferred_keyof_operands.get(&extended).copied();
        self.alias_evaluation_bindings = bindings;
        let operand = operand?;
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let map: Vec<_> = bindings
            .into_iter()
            .map(|(symbol, ty)| (self.get_declared_type_of_symbol(symbol), ty))
            .collect();
        let parameters: Vec<_> = map.iter().map(|&(parameter, _)| parameter).collect();
        Some(self.instantiate_type(operand, &map, &parameters, &[]))
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
        if alias.type_parameters.len() != arguments.len() {
            return;
        }
        if !self.mapped_alias_in_progress.insert(symbol) {
            self.deferred_mapped_aliases.insert(id, (symbol, arguments.to_vec()));
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

    /// getConstraintTypeFromMappedType/getTemplateTypeFromMappedType
    /// (checker.go:22697) resolve a mapped type's parts on first use. A self
    /// reference created while its alias was being captured is captured here,
    /// after the outer capture has finished.
    pub(crate) fn ensure_mapped_type_info(&mut self, id: TypeId) {
        if self.mapped_types.contains_key(&id) {
            return;
        }
        if let Some((symbol, arguments)) = self.deferred_mapped_aliases.remove(&id) {
            self.capture_mapped_alias(id, symbol, &arguments);
        }
    }

    /// getTemplateTypeFromMappedType (checker.go:22697) adds optionality to
    /// the written template when the mapped type includes `?`. The captured
    /// `template` keeps the written type for member substitution, whose
    /// optional flag carries that undefined separately.
    pub(crate) fn mapped_template_type(&mut self, info: &MappedTypeInfo) -> TypeId {
        if self.strict_null_checks && info.optionality == Some(true) {
            self.get_optional_type(info.template, true)
        } else {
            info.template
        }
    }

    /// resolveMappedTypeMembers (checker.go:20894). Enumerate known property
    /// keys and capture their template substitutions before publishing members.
    pub(crate) fn resolve_mapped_type_members(&mut self, id: TypeId) {
        self.complete_reverse_mapped_type(id);
        if !self.mapped_types.contains_key(&id)
            || self.anonymous_properties.contains_key(&id)
            || !self.mapped_members_in_progress.insert(id)
        {
            return;
        }
        self.resolve_mapped_type_members_worker(id);
        self.mapped_members_in_progress.remove(&id);
    }

    /// getIndexTypeForMappedType (checker.go:26871). Unremapped keys are
    /// exactly the constraint; remapped keys follow the same per-property mapper.
    pub(crate) fn mapped_index_type(&mut self, id: TypeId) -> Option<TypeId> {
        let info = self.mapped_types.get(&id)?.clone();
        let Some(name_type) = info.name_type else { return Some(info.constraint) };
        let modifiers = info.modifiers_source.map(|source| self.apparent_type(source));
        let keys = self.mapped_member_keys(&info, modifiers)?;
        let mut names = Vec::new();
        for key in keys {
            let name =
                self.instantiate_type(name_type, &[(info.parameter, key)], &[info.parameter], &[]);
            if name == self.intrinsics.error {
                return None;
            }
            names.push(name);
            if name == self.intrinsics.string {
                names.push(self.intrinsics.number);
            }
        }
        Some(self.get_union_type(&names))
    }

    fn mapped_member_keys(
        &mut self,
        info: &MappedTypeInfo,
        modifiers: Option<TypeId>,
    ) -> Option<Vec<TypeId>> {
        use crate::{flags::TypeFlags, types::TypeData};
        let mut keys = Vec::new();
        if info.name_type.is_some()
            && info.keyof_constraint
            && modifiers.is_some()
            && self.signature_parameter_type_is_generic(info.constraint)
        {
            return None;
        }
        if let Some(source) = modifiers.filter(|_| info.keyof_constraint) {
            if self.store.get(source).flags.contains(TypeFlags::TYPE_PARAMETER) {
                return None;
            }
            if self.is_mapped_sequence_input(source) {
                let key = self.resolved_keyof_type(source)?;
                return Some(match self.store.get(key).data.clone() {
                    TypeData::Union { types, .. } => types,
                    _ => vec![key],
                });
            }
            // An open homomorphic map can enumerate an object constraint, but
            // an unsupported constraint is not a proven empty member table.
            let names = if info
                .modifiers_source
                .is_some_and(|source| self.signature_parameter_type_is_generic(source))
            {
                let names = self.get_property_names_of_type(source)?;
                // Native 5b1047d resolveMappedTypeMembers (checker.go:20943)
                // links composite source declarations and modifier flags.
                // A complete name list is not that symbol image: generic
                // composite modifiers still need represented property roots
                // before this producer can publish mapped members. tsr-6.47.4.1.
                if self
                    .store
                    .get(source)
                    .flags
                    .intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
                    && names.iter().any(|name| self.get_property_of_type(source, name).is_none())
                {
                    return None;
                }
                names
            } else {
                self.property_names_of(source)
            };
            for name in names {
                // Native 5b1047d resolveMappedTypeMembers enumerates through
                // getLiteralTypeFromProperty (checker.go:22729), preserving
                // numeric versus quoted names from the source member's origin.
                // Reuse the Checker-owned provenance read, not a printed name;
                // this walk publishes no key or member image of its own.
                let key = self.literal_type_of_property(source, &name);
                if key == self.intrinsics.error {
                    return None;
                }
                keys.push(key);
            }
            if let Some(indexes) = self.get_index_infos_of_type(source) {
                keys.extend(indexes.into_iter().map(|index| index.key));
            }
        } else {
            let mut pending = vec![info.constraint];
            while let Some(key) = pending.pop() {
                // getLowerBoundOfKeyType (native checker.go:21040) preserves
                // only primitive-first canonical-empty intersections. Other
                // concrete intersections use ordinary reduction before this
                // producer publishes their index key, without erasing literals
                // elsewhere in the constraint union. Open work still declines.
                if let TypeData::Intersection { types, .. } = &self.store.get(key).data {
                    let types = types.clone();
                    if self.signature_parameter_type_is_generic(key) {
                        return None;
                    }
                    let preserved =
                        types.len() == 2
                            && self.store.get(types[0]).flags.intersects(
                                TypeFlags::STRING | TypeFlags::NUMBER | TypeFlags::BIG_INT,
                            )
                            && self.is_unaliased_empty_type_literal(types[1]);
                    if !preserved {
                        let reduced = self.get_intersection_type(&types, None);
                        if reduced != key {
                            pending.push(reduced);
                            continue;
                        }
                    }
                }
                if let TypeData::Union { types, .. } = &self.store.get(key).data {
                    pending.extend(types.iter().rev().copied());
                } else if info.name_type.is_some()
                    || self.store.get(key).flags.intersects(TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL)
                    // Native 5b1047d resolveMappedTypeMembers (checker.go:20956)
                    // also admits concrete index keys, including string & {}.
                    // Reuse the existing validity worker; retain the original
                    // key TypeId in the existing member/index publication.
                    || self.is_valid_index_key_type(key)
                {
                    if self.signature_parameter_type_is_generic(key) {
                        return None;
                    }
                    keys.push(key);
                } else if key != self.intrinsics.never {
                    return None;
                }
            }
        }
        Some(keys)
    }

    fn resolve_mapped_type_members_worker(&mut self, id: TypeId) {
        use crate::types::TypeData;
        let Some(info) = self.mapped_types.get(&id).cloned() else { return };
        if self.anonymous_properties.contains_key(&id) {
            return;
        }
        let modifiers = info.modifiers_source.map(|source| self.apparent_type(source));
        let Some(keys) = self.mapped_member_keys(&info, modifiers) else { return };
        // resolveMappedTypeMembers combines source keys before substituting
        // the template, so colliding names see the entire key union.
        let mut members: Vec<(TypeId, TypeId, TypeId)> = Vec::new();
        for key in keys {
            let name = info.name_type.map_or(key, |name| {
                self.instantiate_type(name, &[(info.parameter, key)], &[info.parameter], &[])
            });
            if name == self.intrinsics.error {
                return;
            }
            let names = match self.store.get(name).data.clone() {
                TypeData::Union { types, .. } => types,
                _ => vec![name],
            };
            for name in names {
                if name == self.intrinsics.never {
                    continue;
                }
                if self.signature_parameter_type_is_generic(name) {
                    return;
                }
                if let Some((_, keys, _)) = members.iter_mut().find(|(existing, _, _)| {
                    matches!(
                        self.store.get(name).data,
                        TypeData::StringLiteral(_) | TypeData::NumberLiteral(_)
                    ) && (existing == &name
                        || match (&self.store.get(*existing).data, &self.store.get(name).data) {
                            (
                                TypeData::StringLiteral(a) | TypeData::NumberLiteral(a),
                                TypeData::StringLiteral(b) | TypeData::NumberLiteral(b),
                            ) => a == b,
                            _ => false,
                        })
                }) {
                    *keys = self.get_union_type(&[*keys, key]);
                } else {
                    members.push((name, key, key));
                }
            }
        }
        let link_declarations =
            info.name_type.is_none_or(|name| self.is_type_assignable_to(name, info.parameter));
        // Recursive references observe the empty table, as upstream's upfront
        // setStructuredTypeMembers does. Types are published after substitution.
        self.anonymous_properties.insert(id, (Vec::new(), true));
        let mut properties = Vec::new();
        let mut indexes: Vec<crate::index_signatures::IndexInfo> = Vec::new();
        for (name_type, key, first_key) in members {
            let name = match &self.store.get(name_type).data {
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
                if self.is_valid_index_key_type(name_type) || name_type == self.intrinsics.any {
                    let key = if name_type == self.intrinsics.any {
                        self.intrinsics.string
                    } else {
                        name_type
                    };
                    let readonly = info.readonly.unwrap_or_else(|| {
                        info.modifiers_source
                            .and_then(|source| self.get_applicable_index_info(source, name_type))
                            .is_some_and(|index| index.readonly)
                    });
                    if let Some(existing) = indexes.iter_mut().find(|index| index.key == key) {
                        existing.value = self.get_union_type(&[existing.value, value]);
                        existing.readonly |= readonly;
                    } else {
                        indexes.push(crate::index_signatures::IndexInfo {
                            components: None,
                            declaration: None,
                            key,
                            value,
                            readonly,
                        });
                    }
                }
                continue;
            };
            if properties
                .iter()
                .any(|property: &crate::objects::AnonymousProperty| property.name == name)
            {
                continue;
            }
            let source_name = match &self.store.get(first_key).data {
                TypeData::StringLiteral(name) | TypeData::NumberLiteral(name) => Some(name.clone()),
                _ => None,
            };
            let source_property = modifiers
                .zip(source_name.as_deref())
                .and_then(|(source, name)| self.get_property_of_type(source, name));
            let captured = modifiers
                .and_then(|source| self.anonymous_properties.get(&source))
                .and_then(|(properties, _)| {
                    properties
                        .iter()
                        .find(|property| Some(property.name.as_str()) == source_name.as_deref())
                });
            let was_optional = captured.map_or_else(
                || source_property.is_some_and(|property| self.property_is_optional(property)),
                |property| property.optional,
            );
            let was_readonly = captured.map_or_else(
                || source_property.is_some_and(|property| self.is_readonly_property(property)),
                |property| property.readonly,
            );
            let inherited =
                modifiers.and_then(|source| self.mapped_identity_optionality.get(&source));
            let was_optional = inherited.and_then(|modifiers| modifiers.0).unwrap_or(was_optional);
            let was_readonly = inherited.and_then(|modifiers| modifiers.1).unwrap_or(was_readonly);
            let optional = info.optionality.unwrap_or(was_optional);
            let readonly = info.readonly.unwrap_or(was_readonly);
            let printed_name = if info.name_type.is_some() {
                if crate::objects::is_identifier_text(&name)
                    || matches!(self.store.get(name_type).data, TypeData::NumberLiteral(_))
                {
                    name.clone()
                } else {
                    crate::printing::quote(&name)
                }
            } else {
                captured.map_or_else(|| name.clone(), |property| property.printed_name.clone())
            };
            let origin = link_declarations
                .then(|| captured.and_then(|property| property.origin).or(source_property))
                .flatten();
            let mut value = self.instantiate_type(
                info.template,
                &[(info.parameter, key)],
                &[info.parameter],
                &[],
            );
            // getTypeOfMappedSymbol (checker.go:20993). Excluding optionality
            // strips missing in exact mode, otherwise undefined.
            if self.strict_null_checks && !optional && was_optional {
                value = self.remove_missing_or_undefined_type(value);
            }
            properties.push(crate::objects::AnonymousProperty {
                accessor_write: None,
                method: false,
                origin,
                checked_declaration: None,
                name,
                printed_name,
                printed_slot: crate::objects::PrintedSlot::printed(self.type_to_string(value)),
                optional,
                readonly,
                slot: crate::objects::PropertySlot::resolved(value),
            });
        }
        self.anonymous_properties.insert(id, (properties, true));
        self.object_literal_index_infos.insert(id, indexes);
    }

    /// getObjectTypeInstantiation/instantiateMappedType (checker.go). Map
    /// captured constraint and template identities for an anonymous mapped type.
    pub(crate) fn instantiate_mapped_type(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let key = (id, map.to_vec());
        if let Some(&cached) = self.instantiated_objects.get(&key) {
            return cached;
        }
        self.instantiated_objects.insert(key.clone(), self.intrinsics.error);
        let result = self.instantiate_mapped_type_worker(id, map, parameters, names);
        self.instantiated_objects.insert(key, result);
        result
    }

    fn instantiate_mapped_type_worker(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        use crate::{flags::TypeFlags, objects::Member};
        let Some(mut info) = self.mapped_types.get(&id).cloned() else {
            return self.intrinsics.error;
        };
        let variable = self
            .deferred_keyof_operands
            .get(&info.constraint)
            .copied()
            .filter(|&ty| self.store.get(ty).flags.contains(TypeFlags::TYPE_PARAMETER));
        let mapped_variable = variable.map(|ty| self.instantiate_type(ty, map, parameters, names));
        if let Some(mapped) = mapped_variable
            && self.store.get(mapped).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER)
        {
            return mapped;
        }
        info.constraint = self.instantiate_type(info.constraint, map, parameters, names);
        info.template = self.instantiate_type(info.template, map, parameters, names);
        info.name_type = info.name_type.map(|ty| self.instantiate_type(ty, map, parameters, names));
        info.modifiers_source = info
            .modifiers_source
            .map(|source| self.instantiate_type(source, map, parameters, names));
        info.constraint_intersection = info.constraint_intersection.map(|types| {
            types.into_iter().map(|ty| self.instantiate_type(ty, map, parameters, names)).collect()
        });
        if let (Some(variable), Some(mapped)) = (variable, mapped_variable)
            && variable != mapped
        {
            info.homomorphic_symbol = self.type_parameter_symbols.get(&variable).copied();
            let replace_source = |checker: &mut Self, source: TypeId| {
                let mut map = map.to_vec();
                map.retain(|&(parameter, _)| parameter != variable);
                map.insert(0, (variable, source));
                let mut parameters = parameters.to_vec();
                if !parameters.contains(&variable) {
                    parameters.push(variable);
                }
                checker.instantiate_type(id, &map, &parameters, names)
            };
            if let Some(sequence) = self.instantiate_mapped_sequence(&info, None, replace_source) {
                return sequence;
            }
        }
        if info.constraint == self.intrinsics.error
            || info.template == self.intrinsics.error
            || info.name_type == Some(self.intrinsics.error)
        {
            return self.intrinsics.error;
        }
        let Some(text) = self.mapped_type_text(&info) else { return self.intrinsics.error };
        let mapped = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(mapped, info.clone());
        self.resolve_mapped_type_members(mapped);
        let Some((properties, _)) = self.anonymous_properties.get(&mapped).cloned() else {
            return mapped;
        };
        let indexes = self.object_literal_index_infos.get(&mapped).cloned().unwrap_or_default();
        let mut members: Vec<_> = indexes
            .iter()
            .map(|index| Member::Index {
                readonly: info.readonly == Some(true),
                name: "x".to_string(),
                key: self.type_to_string(index.key),
                value: self.type_to_string(index.value),
            })
            .collect();
        members.extend(properties.iter().map(|property| Member::Property {
            name: property.printed_name.clone(),
            optional: property.optional,
            readonly: property.readonly,
            printed: self.property_printed_type(property).into_owned(),
        }));
        let text = crate::objects::render_object_type(&members);
        let result = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(result, info);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_index_infos.insert(result, indexes);
        result
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
        let info = self.mapped_types.get(&id)?.clone();
        let parameter = info.homomorphic_symbol?;
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
        self.instantiate_mapped_sequence(&info, Some(id), replace_source)
    }

    fn instantiate_mapped_sequence(
        &mut self,
        info: &MappedTypeInfo,
        alias: Option<TypeId>,
        mut replace_source: impl FnMut(&mut Self, TypeId) -> TypeId,
    ) -> Option<TypeId> {
        use crate::{flags::TypeFlags, tuples::TupleElement, types::TypeData};
        let source = info.modifiers_source?;
        let parameter = info.homomorphic_symbol?;
        if self.store.get(source).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER)
            || self.is_error(source)
        {
            return Some(source);
        }
        if let TypeData::Union { types, .. } = &self.store.get(source).data {
            let types = types.clone();
            let mapped: Vec<_> = types.into_iter().map(|ty| replace_source(self, ty)).collect();
            let union = self.get_union_type(&mapped);
            if union == self.intrinsics.error {
                return None;
            }
            // mapTypeWithAlias retains the mapped alias and its arguments
            // on a distributed union, while exposing its constituents.
            return Some(if let Some(alias) = alias {
                let alias_text = self.type_to_string(alias);
                self.union_with_origin_text(union, alias_text)
            } else {
                union
            });
        }
        // An as clause remaps properties even on arrays and tuples.
        if info.name_type.is_some() {
            return None;
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
                    element.r#type = self.instantiate_mapped_template(info, key, element.optional);
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
            let element = self.instantiate_mapped_template(info, self.intrinsics.number, true);
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

    /// The mapped arms of getSimplifiedIndexedAccessTypeWorker and
    /// computeBaseConstraint (checker.go). Remapped names cannot substitute
    /// the queried key directly for the iteration parameter.
    pub(crate) fn mapped_indexed_access_constraint(
        &mut self,
        object: TypeId,
        index: TypeId,
    ) -> Option<TypeId> {
        let info = self.mapped_types.get(&object)?.clone();
        let generic = self.signature_parameter_type_is_generic(info.constraint);
        if let Some(name) = info.name_type
            && !self.is_type_assignable_to(name, info.parameter)
        {
            return None;
        }
        if !generic
            && (info.name_type.is_some()
                || info.optionality == Some(false)
                || !self.signature_parameter_type_is_generic(index))
        {
            return None;
        }
        let value = self.instantiate_type(
            info.template,
            &[(info.parameter, index)],
            &[info.parameter],
            &[],
        );
        if value == self.intrinsics.error {
            return None;
        }
        let optional = info.optionality == Some(true)
            || if generic {
                info.modifiers_source.is_some_and(|source| {
                    self.combined_mapped_optionality(source, &mut Vec::new()) > 0
                })
            } else {
                self.could_access_optional_mapped_property(object, index)
            };
        Some(if self.strict_null_checks && optional {
            self.get_optional_type(value, true)
        } else {
            value
        })
    }

    /// getCombinedMappedTypeOptionality (checker.go:29040).
    fn combined_mapped_optionality(&self, ty: TypeId, visiting: &mut Vec<TypeId>) -> i8 {
        if visiting.contains(&ty) {
            return 0;
        }
        visiting.push(ty);
        let result = if let Some(info) = self.mapped_types.get(&ty) {
            match info.optionality {
                Some(true) => 1,
                Some(false) => -1,
                None => info
                    .modifiers_source
                    .map_or(0, |ty| self.combined_mapped_optionality(ty, visiting)),
            }
        } else if let Some((Some(optional), _)) = self.mapped_identity_optionality.get(&ty) {
            if *optional { 1 } else { -1 }
        } else if let crate::types::TypeData::Intersection { types, .. } = &self.store.get(ty).data
        {
            let first =
                types.first().map_or(0, |&ty| self.combined_mapped_optionality(ty, visiting));
            if types
                .iter()
                .skip(1)
                .all(|&ty| self.combined_mapped_optionality(ty, visiting) == first)
            {
                first
            } else {
                0
            }
        } else {
            0
        };
        visiting.pop();
        result
    }

    /// couldAccessOptionalProperty (checker.go:29309), using captured mapped
    /// members and the index's base constraint to select accessible properties.
    fn could_access_optional_mapped_property(&mut self, object: TypeId, index: TypeId) -> bool {
        let Some(constraint) = self.base_constraint_of_type(index) else { return false };
        self.resolve_mapped_type_members(object);
        let Some((properties, _)) = self.anonymous_properties.get(&object).cloned() else {
            return false;
        };
        let mapped_constraint = self.mapped_types.get(&object).map(|info| info.constraint);
        let keys = mapped_constraint
            .map(|keys| match self.store.get(keys).data.clone() {
                crate::types::TypeData::Union { types, .. } => types,
                _ => vec![keys],
            })
            .unwrap_or_default();
        properties.into_iter().any(|property| {
            if !property.optional {
                return false;
            }
            // This path only accepts unremapped types, so a literal key in
            // the mapped constraint is also the property's original key type.
            // Preserve number versus quoted-number identity before checking
            // the access constraint (getLiteralTypeFromProperty).
            let key = keys.iter().copied().find(|&key| matches!(
                &self.store.get(key).data,
                crate::types::TypeData::StringLiteral(name) | crate::types::TypeData::NumberLiteral(name)
                    if name == &property.name
            )).unwrap_or_else(|| self.store.intern_literal(
                crate::flags::TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(property.name),
                false,
            ));
            self.is_type_assignable_to(key, constraint)
        })
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

#[cfg(test)]
mod member_producer_tests {
    use crate::{Checker, flags::TypeFlags, types::TypeData};
    use tsr_ast::Statement;
    use tsr_core::Arena;

    #[test]
    fn open_homomorphic_members_keep_constraint_roots_and_deferred_values() {
        let source = "interface Shape { readonly a?: string; b: number }
type Req<T> = { [P in keyof T]-?: T[P] };
type Part<T> = { [P in keyof T]?: T[P] };
type Read<T> = { readonly [P in keyof T]: T[P] };
type Mutable<T> = { -readonly [P in keyof T]: T[P] };
function read<T extends Shape>(req: Req<T>, part: Part<T>, read: Read<T>, mutable: Mutable<T>) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-members.ts", text: source },
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[5] else {
            panic!("function");
        };
        // Cold name and symbol queries must both synthesize members, without
        // a prior value read warming the resolver. Exact optional mode changes
        // absence, not these mapped modifiers or deferred T[P] identities.
        for exact in [false, true] {
            for symbols_first in [false, true] {
                let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                checker.strict_null_checks = true;
                checker.exact_optional_property_types = exact;
                let shape = checker.get_declared_type_of_symbol(bound.globals()["Shape"]);
                let a = checker.get_property_of_type(shape, "a").unwrap();
                let b = checker.get_property_of_type(shape, "b").unwrap();
                let expected = [
                    [(false, true), (false, false)],
                    [(true, true), (true, false)],
                    [(true, true), (false, true)],
                    [(true, false), (false, false)],
                ];
                for (parameter, flags) in function.parameters.iter().zip(expected) {
                    let mapped = checker.get_type_from_type_node(parameter.r#type.unwrap());
                    if symbols_first {
                        assert_eq!(checker.get_property_of_type(mapped, "a"), Some(a));
                        assert_eq!(checker.get_property_of_type(mapped, "b"), Some(b));
                    }
                    assert_eq!(
                        checker.get_property_names_of_type(mapped),
                        Some(vec!["a".into(), "b".into()])
                    );
                    let (properties, complete) = &checker.anonymous_properties[&mapped];
                    assert!(*complete);
                    assert_eq!((properties[0].optional, properties[0].readonly), flags[0]);
                    assert_eq!((properties[1].optional, properties[1].readonly), flags[1]);
                    assert_eq!(properties[0].origin, Some(a));
                    assert_eq!(properties[1].origin, Some(b));
                    assert!(properties.iter().all(|property| {
                        let ty = checker.peek_property_type(property).unwrap();
                        checker.type_of(ty).flags.contains(TypeFlags::INDEXED_ACCESS)
                    }));
                    let b_value = checker.peek_property_type(&properties[1]).unwrap();
                    assert_eq!(
                        checker.base_constraint_of_type(b_value),
                        Some(checker.intrinsics.number)
                    );
                    let b_read = checker.get_type_of_property_of_type(mapped, "b").unwrap();
                    if flags[1].0 {
                        let TypeData::Union { types, .. } = &checker.type_of(b_read).data else {
                            panic!("optional read must include undefined");
                        };
                        assert!(types.contains(&b_value));
                        assert!(types.iter().any(|&ty| {
                            checker.type_of(ty).flags.contains(TypeFlags::UNDEFINED)
                        }));
                    } else {
                        assert_eq!(b_read, b_value);
                    }
                    assert_eq!(checker.get_property_of_type(mapped, "a"), Some(a));
                    assert_eq!(checker.get_property_of_type(mapped, "b"), Some(b));
                    assert_eq!(checker.get_property_of_type(mapped, "absent"), None);
                    // A mapped override must not mutate its declaration root.
                    assert!(checker.property_is_optional(a));
                    assert!(checker.is_readonly_property(a));
                    assert!(!checker.property_is_optional(b));
                    assert!(!checker.is_readonly_property(b));
                }
            }
        }
    }

    #[test]
    fn unsupported_open_keys_do_not_publish_complete_empty_members() {
        let source = "type Req<T> = { [P in keyof T]-?: T[P] };
function read<T extends { a: string; b: number } | { a: string; c: boolean }, K extends 'a'>(
    union: Req<T>, open: { [P in K]: string }) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-members.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[1] else {
            panic!("function");
        };
        for parameter in function.parameters {
            let mapped = checker.get_type_from_type_node(parameter.r#type.unwrap());
            checker.resolve_mapped_type_members(mapped);
            assert_eq!(checker.get_property_names_of_type(mapped), None);
            assert!(!checker.anonymous_properties.contains_key(&mapped));
            assert_eq!(checker.get_property_of_type(mapped, "a"), None);
        }
    }

    #[test]
    fn concrete_mapped_index_keys_keep_literal_members_and_original_index_identity() {
        for (domain, lookup, preserved) in [
            ("string & {}", "other", true),
            ("{} & string", "other", false),
            ("number & {}", "7", true),
            ("`west-${string}` & {}", "west-two", false),
        ] {
            let source = format!(
                "type Keys = ({domain}) | 'fixed';
                 function read(map: {{ [P in Keys]: string }}) {{}}"
            );
            let arena = Arena::new();
            let parsed = tsr_parser::parse(&arena, &source);
            assert!(parsed.diagnostics.is_empty());
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "mapped-index-keys.ts", text: &source },
            );
            let Statement::FunctionDeclaration(function) = parsed.source_file.statements[1] else {
                panic!("function");
            };
            for indexes_first in [false, true] {
                let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                checker.strict_null_checks = true;
                let mapped =
                    checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
                let constraint = checker.mapped_types[&mapped].constraint;
                let TypeData::Union { types, .. } = &checker.store.get(constraint).data else {
                    panic!("key union");
                };
                let key = *types
                    .iter()
                    .find(|&&key| {
                        matches!(checker.store.get(key).data, TypeData::Intersection { .. })
                    })
                    .unwrap();
                assert!(checker.is_valid_index_key_type(key));
                let expected_key = if preserved {
                    key
                } else {
                    let TypeData::Intersection { types, .. } = &checker.store.get(key).data else {
                        unreachable!()
                    };
                    *types
                        .iter()
                        .find(|&&id| {
                            checker
                                .store
                                .get(id)
                                .flags
                                .intersects(TypeFlags::STRING | TypeFlags::TEMPLATE_LITERAL)
                        })
                        .unwrap()
                };
                if indexes_first {
                    assert_eq!(
                        checker.get_index_infos_of_type(mapped).unwrap()[0].key,
                        expected_key
                    );
                } else {
                    checker.resolve_mapped_type_members(mapped);
                }
                assert_eq!(checker.get_property_names_of_type(mapped), Some(vec!["fixed".into()]));
                let indexes = checker.get_index_infos_of_type(mapped).unwrap();
                assert_eq!(indexes.len(), 1);
                assert_eq!(indexes[0].key, expected_key);
                assert_eq!(indexes[0].value, checker.intrinsics.string);
                assert!(!indexes[0].readonly);
                let numeric = domain == "number & {}";
                let lookup = checker.store.intern_literal(
                    if numeric { TypeFlags::NUMBER_LITERAL } else { TypeFlags::STRING_LITERAL },
                    if numeric {
                        TypeData::NumberLiteral(lookup.into())
                    } else {
                        TypeData::StringLiteral(lookup.into())
                    },
                    false,
                );
                let index = checker.get_applicable_index_info(mapped, lookup).unwrap();
                assert_eq!(index.key, expected_key);
                assert_eq!(index.value, checker.intrinsics.string);
                let properties = checker.anonymous_properties[&mapped].clone();
                assert!(properties.1);
                assert_eq!(properties.0.len(), 1);
                assert_eq!(checker.property_type(&properties.0[0]), checker.intrinsics.string);
                assert_eq!(properties.0[0].origin, None);
                assert!(!properties.0[0].optional);
                let count = checker.type_count();
                for _ in 0..3 {
                    checker.resolve_mapped_type_members(mapped);
                    assert_eq!(
                        checker.get_property_names_of_type(mapped),
                        Some(vec!["fixed".into()])
                    );
                    assert_eq!(checker.get_index_infos_of_type(mapped), Some(indexes.clone()));
                    let (properties, complete) = checker.anonymous_properties[&mapped].clone();
                    assert!(complete);
                    assert_eq!(properties.len(), 1);
                    let property = &properties[0];
                    let property_type = checker.property_type(property);
                    assert_eq!(
                        (
                            &*property.name,
                            &*property.printed_name,
                            &*checker.property_printed_type(property),
                            property_type,
                            property.optional,
                            property.readonly,
                            property.origin,
                            property.method,
                            property.accessor_write.is_none()
                        ),
                        (
                            "fixed",
                            "fixed",
                            "string",
                            checker.intrinsics.string,
                            false,
                            false,
                            None,
                            false,
                            true
                        )
                    );
                    assert_eq!(checker.type_count(), count);
                }
            }
        }
    }
}
