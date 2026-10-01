//! Base constraints, ported from internal/checker/checker.go.
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use tsr_binder::SymbolId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BaseConstraintKey {
    ty: TypeId,
    bindings: Vec<(SymbolId, TypeId)>,
}

impl Checker<'_, '_> {
    fn has_base_constraint_shape(&self, ty: TypeId) -> bool {
        self.store.get(ty).flags.intersects(
            TypeFlags::TYPE_PARAMETER
                | TypeFlags::INDEXED_ACCESS
                | TypeFlags::CONDITIONAL
                | TypeFlags::SUBSTITUTION
                | TypeFlags::INDEX
                | TypeFlags::UNION
                | TypeFlags::INTERSECTION
                | TypeFlags::TEMPLATE_LITERAL
                | TypeFlags::STRING_MAPPING,
        ) || self.deferred_keyof_operands.contains_key(&ty)
    }

    /// getBaseConstraintOfType/getResolvedBaseConstraint. This port's node
    /// evaluator uses outer alias frames, so cache identities include that mapper.
    pub(crate) fn base_constraint_of_type(&mut self, ty: TypeId) -> Option<TypeId> {
        if !self.has_base_constraint_shape(ty) || ty == self.intrinsics.error {
            return None;
        }
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        let key = BaseConstraintKey { ty, bindings };
        if let Some(&cached) = self.base_constraint_cache.get(&key) {
            return cached;
        }
        // getResolvedBaseConstraint's final safety stop is 50 nested constraints.
        if self.base_constraint_depth >= 50 {
            return None;
        }
        self.base_constraint_cache.insert(key.clone(), None);
        self.base_constraint_depth += 1;
        let result = self.compute_base_constraint(ty);
        self.base_constraint_depth -= 1;
        self.base_constraint_cache.insert(key, result);
        result
    }

    fn next_base_constraint(&mut self, ty: TypeId) -> Option<TypeId> {
        if ty == self.intrinsics.error {
            None
        } else if self.has_base_constraint_shape(ty) {
            self.base_constraint_of_type(ty)
        } else {
            Some(ty)
        }
    }

    fn compute_base_constraint(&mut self, ty: TypeId) -> Option<TypeId> {
        if self.store.get(ty).flags.contains(TypeFlags::TYPE_PARAMETER) {
            let constraint = self.type_parameter_constraint(ty)?;
            return self.next_base_constraint(constraint);
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.store.get(ty).data.clone()
        {
            let union = self.store.get(ty).flags.contains(TypeFlags::UNION);
            let constraints: Vec<_> =
                types.iter().filter_map(|&ty| self.next_base_constraint(ty)).collect();
            if constraints == types {
                return Some(ty);
            }
            return if union && constraints.len() == types.len() {
                Some(self.get_union_type(&constraints))
            } else if !union && !constraints.is_empty() {
                Some(self.get_intersection_type(&constraints, None))
            } else {
                None
            };
        }
        if let Some(&(object, index, include_undefined)) =
            self.deferred_indexed_access_types.get(&ty)
        {
            if let Some(substituted) = self.mapped_indexed_access_constraint(object, index) {
                return self.next_base_constraint(substituted);
            }
            let object = self.next_base_constraint(object)?;
            let index = self.next_base_constraint(index)?;
            let value = self.resolved_indexed_access_type(object, index, include_undefined)?;
            return self.next_base_constraint(value);
        }
        if self.deferred_keyof_operands.contains_key(&ty) {
            return Some(self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ]));
        }
        if let Some(parts) = self.template_literal_parts.get(&ty).cloned() {
            let constraints: Option<Vec<_>> =
                parts.types.into_iter().map(|ty| self.next_base_constraint(ty)).collect();
            return Some(constraints.map_or(self.intrinsics.string, |types| {
                self.get_template_literal_type(&parts.texts, &types)
            }));
        }
        if let Some((symbol, target)) = self.string_mapping_types.get(&ty).copied() {
            let constraint = self.next_base_constraint(target);
            return Some(match constraint {
                Some(constraint) if constraint != target => {
                    self.get_string_mapping_type(symbol, constraint)
                }
                _ => self.intrinsics.string,
            });
        }
        if let Some(&(yes, no)) = self.mapped_conditional_branches.get(&ty) {
            let constraint = if self.store.get(yes).flags.contains(TypeFlags::ANY) {
                no
            } else if self.store.get(no).flags.contains(TypeFlags::ANY) {
                yes
            } else {
                self.get_union_type(&[yes, no])
            };
            return self.next_base_constraint(constraint);
        }
        None
    }
    /// getNarrowableTypeForReference/isConstraintPosition (checker.go:31491).
    /// Substitute union constraints only where apparent types or a concrete
    /// contextual type determine the reference's behavior, outside inference.
    pub(crate) fn narrowable_type_for_reference(
        &mut self,
        ty: TypeId,
        reference: tsr_ast::NodeId,
    ) -> TypeId {
        if !self.active_inference_contexts.is_empty() {
            return ty;
        }
        let parts = match self.store.get(ty).data.clone() {
            TypeData::Union { types, .. } => types,
            _ => vec![ty],
        };
        if !parts.iter().any(|&part| self.generic_type_with_union_constraint(part)) {
            return ty;
        }
        let parent = self.nodes.parent(reference).and_then(|id| self.node_map.get(id));
        let constraint_position = match parent {
            Some(tsr_ast::Node::PropertyAccessExpression(_) | tsr_ast::Node::QualifiedName(_)) => {
                true
            }
            Some(tsr_ast::Node::CallExpression(call)) => {
                call.expression.and_then(|node| node.node_id()) == Some(reference)
            }
            Some(tsr_ast::Node::NewExpression(call)) => {
                call.expression.and_then(|node| node.node_id()) == Some(reference)
            }
            Some(tsr_ast::Node::ElementAccessExpression(access))
                if access.expression.and_then(|node| node.node_id()) == Some(reference) =>
            {
                let generic_non_nullable =
                    parts.iter().any(|&part| self.generic_type_without_nullable_constraint(part));
                let generic_index = access.argument_expression.is_some_and(|index| {
                    let index = self.check_expression(index);
                    self.indexed_access_index_is_generic(index)
                });
                !(generic_non_nullable && generic_index)
            }
            _ => false,
        };
        // JSX tag contexts resolve their tag recursively and are excluded by
        // hasContextualTypeWithNoGenericTypes.
        let jsx_tag = matches!(
            parent,
            Some(tsr_ast::Node::JsxOpeningElement(_) | tsr_ast::Node::JsxSelfClosingElement(_))
        );
        let contextual_reference = matches!(
            self.nodes.kind(reference),
            tsr_ast::SyntaxKind::Identifier
                | tsr_ast::SyntaxKind::PropertyAccessExpression
                | tsr_ast::SyntaxKind::ElementAccessExpression
        );
        let concrete_context = !constraint_position
            && contextual_reference
            && !jsx_tag
            && self
                .get_contextual_type(reference)
                .is_some_and(|context| !self.context_type_is_generic(context));
        if !constraint_position && !concrete_context {
            return ty;
        }
        let constraints: Vec<_> =
            parts.into_iter().map(|part| self.base_constraint_or_type(part)).collect();
        self.get_union_type(&constraints)
    }

    /// isGenericType/getGenericObjectFlags, including semantic metadata used
    /// by the older named representations of indexed and non-null types.
    fn context_type_is_generic(&mut self, ty: TypeId) -> bool {
        self.context_type_is_generic_inner(ty, &mut Vec::new())
    }

    fn context_type_is_generic_inner(&mut self, ty: TypeId, aliases: &mut Vec<SymbolId>) -> bool {
        if self.signature_parameter_type_is_generic(ty)
            || self.deferred_keyof_operands.contains_key(&ty)
            || self.deferred_indexed_access_types.contains_key(&ty)
            || self.is_generic_homomorphic_mapped_type(ty)
        {
            return true;
        }
        if let Some(&(base, _)) = self.non_null_mint_bases.get(&ty) {
            return self.context_type_is_generic_inner(base, aliases);
        }
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.store.get(ty).data.clone()
        {
            return types.into_iter().any(|part| self.context_type_is_generic_inner(part, aliases));
        }
        if let Some(parts) = self.template_literal_parts.get(&ty).cloned() {
            return parts
                .types
                .into_iter()
                .any(|part| self.context_type_is_generic_inner(part, aliases));
        }
        if let Some(&(_, target)) = self.string_mapping_types.get(&ty) {
            return self.context_type_is_generic_inner(target, aliases);
        }
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&ty).cloned()
            && !aliases.contains(&symbol)
            && self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(body) = alias.r#type
        {
            // Ordinary references such as Array<T> are not generic at the
            // top level. Aliases inherit the generic flags of their body.
            let parameters = self.local_type_parameters_of(symbol);
            let frame = parameters
                .iter()
                .zip(arguments)
                .filter_map(|(parameter, argument)| {
                    parameter
                        .node_id
                        .and_then(|id| self.binder.symbol_of(id))
                        .map(|symbol| (symbol, argument))
                })
                .collect();
            aliases.push(symbol);
            self.alias_evaluation_bindings.push(frame);
            let body_type = self.get_type_from_type_node(body);
            let result = matches!(body, tsr_ast::TypeNode::ConditionalTypeNode(_))
                || self.context_type_is_generic_inner(body_type, aliases);
            self.alias_evaluation_bindings.pop();
            aliases.pop();
            return result;
        }
        false
    }

    fn generic_type_with_union_constraint(&mut self, ty: TypeId) -> bool {
        if let TypeData::Intersection { types, .. } = self.store.get(ty).data.clone() {
            return types.into_iter().any(|ty| self.generic_type_with_union_constraint(ty));
        }
        if self.instantiable_constraint_type(ty) {
            let constraint = self.base_constraint_or_type(ty);
            return self
                .store
                .get(constraint)
                .flags
                .intersects(TypeFlags::NULLABLE | TypeFlags::UNION);
        }
        false
    }

    fn generic_type_without_nullable_constraint(&mut self, ty: TypeId) -> bool {
        if let TypeData::Intersection { types, .. } = self.store.get(ty).data.clone() {
            return types.into_iter().any(|ty| self.generic_type_without_nullable_constraint(ty));
        }
        if self.instantiable_constraint_type(ty) {
            let constraint = self.base_constraint_or_type(ty);
            return !self.maybe_type_of_kind(constraint, TypeFlags::NULLABLE);
        }
        false
    }

    fn instantiable_constraint_type(&self, ty: TypeId) -> bool {
        self.store.get(ty).flags.intersects(
            TypeFlags::TYPE_PARAMETER
                | TypeFlags::INDEXED_ACCESS
                | TypeFlags::CONDITIONAL
                | TypeFlags::SUBSTITUTION
                | TypeFlags::INDEX
                | TypeFlags::TEMPLATE_LITERAL
                | TypeFlags::STRING_MAPPING,
        ) || self.deferred_keyof_operands.contains_key(&ty)
    }
}
