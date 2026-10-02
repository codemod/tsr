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
    /// getEffectiveConstraintOfIntersection (internal/checker/relater.go:2282).
    /// Preserve the source variable for union targets so identity proofs remain
    /// available while the constraint's disjoint domains distribute.
    pub(crate) fn effective_constraint_of_intersection(
        &mut self,
        types: &[TypeId],
        target_is_union: bool,
    ) -> Option<TypeId> {
        let mut constraints = Vec::new();
        let mut domains = Vec::new();
        for &ty in types {
            if self.store.get(ty).flags.intersects(TypeFlags::INSTANTIABLE) {
                let mut seen = vec![ty];
                let mut current = ty;
                loop {
                    let constraint =
                        if self.store.get(current).flags.contains(TypeFlags::TYPE_PARAMETER) {
                            self.type_parameter_constraint(current)
                        } else {
                            self.base_constraint_of_type(current)
                        };
                    let Some(constraint) = constraint else { break };
                    if self.is_error(constraint) || seen.contains(&constraint) {
                        break;
                    }
                    if self.store.get(constraint).flags.intersects(
                        TypeFlags::TYPE_PARAMETER | TypeFlags::INDEX | TypeFlags::CONDITIONAL,
                    ) {
                        seen.push(constraint);
                        current = constraint;
                        continue;
                    }
                    constraints.push(constraint);
                    if target_is_union {
                        constraints.push(ty);
                    }
                    break;
                }
            } else if self.store.get(ty).flags.intersects(TypeFlags::DISJOINT_DOMAINS)
                || self.is_empty_anonymous_object_type(ty)
            {
                domains.push(ty);
            }
        }
        if constraints.is_empty() || (!target_is_union && domains.is_empty()) {
            return None;
        }
        constraints.extend(domains);
        let constraint = self.get_intersection_without_constraint_reduction(&constraints);
        (!self.is_error(constraint)).then_some(constraint)
    }

    /// A retained keyof alias reference has `IndexType` semantics even though its
    /// written-reference representation carries OBJECT until its body is read.
    fn is_keyof_alias_reference(&self, ty: TypeId) -> bool {
        self.type_reference_targets.get(&ty).is_some_and(|(symbol, _)| {
            self.binder.symbols().get(*symbol).declarations.iter().any(|&declaration| {
                matches!(self.node_map.get(declaration),
                    Some(tsr_ast::Node::TypeAliasDeclaration(alias))
                        if matches!(alias.r#type,
                            Some(tsr_ast::TypeNode::TypeOperatorNode(operator))
                                if operator.operator.kind == tsr_ast::SyntaxKind::KeyOfKeyword))
            })
        })
    }

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
            || self.is_keyof_alias_reference(ty)
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
        // Native aliases already have their body's semantic identity. Resolve
        // this port's retained reference before following its base constraint.
        if self.is_keyof_alias_reference(ty)
            && let Some((symbol, arguments)) = self.type_reference_targets.get(&ty).cloned()
            && let Some(body) = self.evaluate_alias_body(symbol, &arguments)
            && body != ty
        {
            return self.next_base_constraint(body);
        }
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
        if self.store.get(ty).flags.contains(TypeFlags::CONDITIONAL)
            && !self.conditional_constraint_branches.contains_key(&ty)
            && let Some((symbol, arguments)) = self.type_reference_targets.get(&ty).cloned()
        {
            self.capture_conditional_alias_branches(ty, symbol, &arguments);
        }
        if let Some(&(yes, no)) = self
            .conditional_constraint_branches
            .get(&ty)
            .or_else(|| self.mapped_conditional_branches.get(&ty))
        {
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
        if let Some(&base) = self.non_null_refinement_bases.get(&ty) {
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
        if let Some((symbol, arguments)) = self.type_reference_targets.get(&ty).cloned() {
            return self.alias_reference_is_generic(symbol, arguments, aliases);
        }
        false
    }

    /// The type-alias arm of [`Self::context_type_is_generic_inner`]: an alias
    /// reference carries the generic flags of its instantiated body
    /// (`getTypeFromTypeAliasReference` instantiates the declared type, so
    /// `getGenericObjectFlags` sees the body). Ordinary references such as
    /// `Array<T>` are not generic at the top level.
    ///
    /// A body that is itself a reference to another alias (`type Baz<T> =
    /// Foo<T>`) follows that alias with the bound arguments rather than
    /// evaluating it: the evaluator answers `error` for a deferred conditional
    /// reached that way, and upstream's `Baz<T>` IS the instantiated `Foo<T>`.
    fn alias_reference_is_generic(
        &mut self,
        symbol: SymbolId,
        arguments: Vec<TypeId>,
        aliases: &mut Vec<SymbolId>,
    ) -> bool {
        if aliases.contains(&symbol)
            || !self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
        {
            return false;
        }
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return false;
        };
        let Some(tsr_ast::Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
        else {
            return false;
        };
        let Some(body) = alias.r#type else { return false };
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
        let result = if matches!(body, tsr_ast::TypeNode::ConditionalTypeNode(_)) {
            true
        } else {
            let body_type = self.get_type_from_type_node(body);
            let referenced_alias = match body {
                tsr_ast::TypeNode::TypeReferenceNode(reference)
                    if body_type == self.intrinsics.error =>
                {
                    let target = reference.node_id.and_then(|id| {
                        let tsr_ast::EntityName::Identifier(name) = reference.type_name? else {
                            return None;
                        };
                        self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            id,
                            name.text,
                            tsr_binder::SymbolFlags::TYPE,
                        )
                    });
                    target.map(|target| {
                        let arguments: Vec<TypeId> = reference
                            .type_arguments
                            .iter()
                            .map(|argument| self.get_type_from_type_node(*argument))
                            .collect();
                        (target, arguments)
                    })
                }
                _ => None,
            };
            match referenced_alias {
                Some((target, arguments)) => {
                    self.alias_reference_is_generic(target, arguments, aliases)
                }
                None => self.context_type_is_generic_inner(body_type, aliases),
            }
        };
        self.alias_evaluation_bindings.pop();
        aliases.pop();
        result
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
