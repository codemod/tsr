//! Base constraints, ported from internal/checker/checker.go.
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use tsr_ast::{NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BaseConstraintKey {
    ty: TypeId,
    bindings: Vec<(SymbolId, TypeId)>,
}

impl Checker<'_, '_> {
    /// getDefaultConstraintOfConditionalType (checker.go:17263). Select the
    /// semantic branches without distributing the check or following either
    /// branch's base constraint. An any branch is elided, not made viral.
    pub(crate) fn default_constraint_of_conditional_type(&mut self, ty: TypeId) -> Option<TypeId> {
        let (yes, no) = self.conditional_inference_branches(ty)?;
        Some(if self.store.get(yes).flags.contains(TypeFlags::ANY) {
            no
        } else if self.store.get(no).flags.contains(TypeFlags::ANY) {
            yes
        } else {
            self.get_union_type(&[yes, no])
        })
    }

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

    /// getBaseConstraintOfType/getResolvedBaseConstraint (checker.go:27447).
    /// This port's node evaluator uses outer alias frames, so cache identities
    /// include that mapper.
    ///
    /// `base_constraint_cache` (Checker-owned, whole-check lifetime) holds only
    /// completed answers: `Some` constraint, or `None` for no constraint and
    /// for a circular one (native `noConstraintType`/`circularConstraintType`,
    /// both of which `getBaseConstraintOfType` answers as nil). In-progress
    /// work is the `ResolvedBaseConstraint` frame on the resolution stack: a
    /// re-entry fails every frame of the cycle, and a failed type parameter
    /// frame reports TS2313 at its constraint declaration.
    pub(crate) fn base_constraint_of_type(&mut self, ty: TypeId) -> Option<TypeId> {
        use crate::resolution::{PropertyName, ResolutionTarget};
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
        if !self.resolutions.push(
            ResolutionTarget::BaseConstraint(key.clone()),
            PropertyName::ResolvedBaseConstraint,
        ) {
            return None;
        }
        self.base_constraint_depth += 1;
        let mut result = self.compute_base_constraint(ty);
        self.base_constraint_depth -= 1;
        if !self.resolutions.pop() {
            self.report_circular_constraint(ty);
            result = None;
        }
        self.base_constraint_cache.insert(key, result);
        result
    }

    /// getResolvedBaseConstraint's failed-pop report (checker.go:27447): a
    /// type parameter reports TS2313 at getConstraintDeclaration's node.
    /// Native's related "circularity originates" location is not carried.
    fn report_circular_constraint(&mut self, ty: TypeId) {
        use tsr_diagnostics::{Diagnostic, messages};
        if !self.store.get(ty).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return;
        }
        let Some(&symbol) = self.type_parameter_symbols.get(&ty) else { return };
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let Some(constraint) =
            declarations.iter().find_map(|&declaration| match self.node_map.get(declaration) {
                Some(tsr_ast::Node::TypeParameterDeclaration(parameter)) => {
                    parameter.constraint.and_then(|constraint| constraint.node_id())
                }
                _ => None,
            })
        else {
            return;
        };
        let Some(file) = self.source_file_of_for_diagnostics(constraint) else { return };
        if !self.circularity_reported.insert(constraint) {
            return;
        }
        let name = self.binder.symbols().get(symbol).name.to_string();
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_PARAMETER_0_HAS_A_CIRCULAR_CONSTRAINT,
                self.error_span(constraint),
                [name],
            ),
        );
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
            self.resolve_constraint_mapped_type_parameters(ty);
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

impl Checker<'_, '_> {
    /// `checkTypeReferenceOrImport`'s constraint arm (`checker.go:2998`) into
    /// `checkTypeArgumentConstraints` (`checker.go:3016`) for a written type
    /// reference: each type argument must be assignable to its parameter's
    /// constraint instantiated with the effective arguments, else TS2344 at
    /// the argument.
    ///
    /// The type parameters are `getTypeParametersForTypeAndSymbol`'s
    /// (`checker.go:17198`): the alias's for a type alias, the target's local
    /// ones for a class or interface reference. The relation is three-valued
    /// and only a definite `NotRelated` on a reportable pair is reported,
    /// which is `assignreport.rs`'s rule for the same `checkTypeAssignableTo`.
    /// No cache or side table: the type node, constraint and relation queries
    /// are the checker's existing memoised ones.
    pub(crate) fn check_type_argument_constraints(&mut self, node: NodeId) {
        let Some(tsr_ast::Node::TypeReferenceNode(reference)) = self.node_map.get(node) else {
            return;
        };
        if reference.type_arguments.is_empty() {
            return;
        }
        let Some(type_name) = reference.type_name else { return };
        let Ok(type_node) =
            tsr_ast::TypeNode::try_from(tsr_ast::Node::TypeReferenceNode(reference))
        else {
            return;
        };
        let resolved = self.get_type_from_type_node(type_node);
        if self.is_error(resolved) {
            return;
        }
        let Some(mut symbol) = self.resolve_entity_name(type_name, SymbolFlags::TYPE) else {
            return;
        };
        for _ in 0..8u8 {
            symbol = self.binder.merged_symbol(symbol);
            if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
                break;
            }
            let Some(target) = self.resolve_alias(symbol) else { return };
            symbol = target;
        }
        if !self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::TYPE_ALIAS | SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            return;
        }
        let declarations = self.local_type_parameters_of(symbol);
        if declarations.is_empty() || reference.type_arguments.len() > declarations.len() {
            return;
        }
        let Some(parameters) = self.local_type_parameter_types_of(symbol) else { return };
        let parameter_types: Vec<TypeId> = parameters.iter().map(|&(ty, _)| ty).collect();
        let names: Vec<String> = parameters.iter().map(|(_, name)| name.clone()).collect();
        let constraints: Vec<Option<TypeId>> = parameter_types
            .iter()
            .map(|&parameter| self.type_parameter_constraint(parameter))
            .collect();
        if constraints.iter().all(Option::is_none) {
            return;
        }
        // getEffectiveTypeArguments: the written arguments, then
        // fillMissingTypeArguments' defaults instantiated over the prefix.
        let mut arguments: Vec<TypeId> = Vec::with_capacity(parameter_types.len());
        for &argument in reference.type_arguments {
            arguments.push(self.get_type_from_type_node(argument));
        }
        for (index, declaration) in declarations.iter().enumerate().skip(arguments.len()) {
            let Some(default) = declaration.default_type else { return };
            let default = self.get_type_from_type_node(default);
            let map: Vec<(TypeId, TypeId)> =
                parameter_types[..index].iter().copied().zip(arguments.iter().copied()).collect();
            let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let filled = self.instantiate_type(default, &map, &parameter_types, &name_refs);
            arguments.push(filled);
        }
        if arguments.iter().any(|&argument| self.is_error(argument)) {
            return;
        }
        let map: Vec<(TypeId, TypeId)> =
            parameter_types.iter().copied().zip(arguments.iter().copied()).collect();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        for (index, constraint) in constraints.into_iter().enumerate() {
            let Some(constraint) = constraint else { continue };
            let Some(&argument_node) = reference.type_arguments.get(index) else { continue };
            let Some(at) = argument_node.node_id() else { continue };
            let target = self.instantiate_type(constraint, &map, &parameter_types, &name_refs);
            let source = arguments[index];
            if self.is_error(target)
                || self.type_argument_node_is_generic(at)
                || self.relation_undecidable_for_constraint(source)
                || self.relation_undecidable_for_constraint(target)
                || self.relate_ternary(source, target, crate::relater::Relation::Assignable)
                    != crate::relater::Ternary::NotRelated
                || !self.pair_is_reportable(source, target)
            {
                continue;
            }
            // `checkTypeAssignableTo(typeArgument, constraint, typeArgNode,
            // Type_0_does_not_satisfy_the_constraint_1)` (`checker.go:3016`):
            // `reportRelationError` (`relater.go:4751`) drops the TS2344 head
            // when the chain ends in the pair's missing-property message, so
            // the shared reporter chooses TS2741/TS2739/TS2740 or the head.
            let span = self.error_span(at);
            self.report_relation_failure(
                at,
                span,
                None,
                source,
                target,
                Some(&tsr_diagnostics::messages::TYPE_0_DOES_NOT_SATISFY_THE_CONSTRAINT_1),
            );
            // `result = result && checkTypeAssignableTo(...)` short-circuits:
            // the first failing argument ends the check.
            return;
        }
    }
}

impl Checker<'_, '_> {
    /// A side of a constraint check whose relation this port cannot decide:
    /// an instantiable type (type parameter, indexed access, conditional,
    /// substitution, `keyof`, template or string mapping) or one mentioning a
    /// type parameter through its reference arguments or signatures. The
    /// three-valued relater answers these from incomplete constraint and
    /// mapped/conditional machinery, so a `NotRelated` there is not
    /// upstream's answer; the check declines rather than report it.
    fn relation_undecidable_for_constraint(&mut self, ty: TypeId) -> bool {
        self.relation_undecidable_within(ty, 3)
    }

    /// The written-argument half of the same decline: a type argument whose
    /// syntax is generic: it names a type parameter, or writes a
    /// conditional, indexed-access, mapped, `infer`, type-operator, template,
    /// `typeof` or `this` type. This port may have resolved such a node
    /// eagerly to a concrete type upstream keeps deferred, so its type is not
    /// evidence about upstream's relation.
    fn type_argument_node_is_generic(&self, node: NodeId) -> bool {
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            match self.nodes.kind(current) {
                SyntaxKind::ConditionalType
                | SyntaxKind::IndexedAccessType
                | SyntaxKind::MappedType
                | SyntaxKind::InferType
                | SyntaxKind::TypeOperator
                | SyntaxKind::TemplateLiteralType
                | SyntaxKind::TypeQuery
                | SyntaxKind::ThisType => return true,
                SyntaxKind::TypeReference => {
                    if let Some(tsr_ast::Node::TypeReferenceNode(reference)) =
                        self.node_map.get(current)
                        && let Some(name) = reference.type_name
                        && self.resolve_entity_name(name, SymbolFlags::TYPE).is_some_and(|symbol| {
                            self.binder
                                .symbols()
                                .get(symbol)
                                .flags
                                .intersects(SymbolFlags::TYPE_PARAMETER)
                        })
                    {
                        return true;
                    }
                }
                _ => {}
            }
            if let Some(typed) = self.node_map.get(current) {
                tsr_ast::for_each_child_id(typed, |child| stack.push(child));
            }
        }
        false
    }

    fn relation_undecidable_within(&mut self, ty: TypeId, depth: u8) -> bool {
        if self.store.get(ty).flags.intersects(TypeFlags::INSTANTIABLE)
            || self.mapped_types.contains_key(&ty)
            || self.mentions_any_type_parameter(ty, depth)
        {
            return true;
        }
        if depth == 0 {
            return false;
        }
        let members = match &self.store.get(ty).data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => types.clone(),
            _ => self
                .type_reference_targets
                .get(&ty)
                .map(|(_, arguments)| arguments.clone())
                .unwrap_or_default(),
        };
        members.into_iter().any(|member| self.relation_undecidable_within(member, depth - 1))
    }
}

impl<'a> Checker<'a, '_> {
    /// The eager half of `getTypeFromMappedTypeNode` (`checker.go:24255`)
    /// that `getConstraintFromTypeParameter` (`checker.go:17071`) reaches
    /// while it resolves a declared type parameter's constraint node: every
    /// mapped type the node builds calls `getConstraintTypeFromMappedType`,
    /// which is `getConstraintOfTypeParameter` of the mapped type's own
    /// parameter, guarded by `hasNonCircularBaseConstraint`. Inside this
    /// parameter's `ResolvedBaseConstraint` frame that closes
    /// `T extends { [P in T]: number }`'s cycle T -> P -> T, so both frames
    /// fail and both report TS2313 (`incorrectRecursiveMappedTypeConstraint`).
    ///
    /// This port mints a mapped type per evaluation rather than once per node,
    /// so the step runs here, where native's first evaluation of a constraint
    /// node happens (`docs/parity/notes/r4-typeparams.md` §2). Instantiated
    /// parameters take their target's constraint (`tp.target`) and are skipped,
    /// as native resolves no node for them.
    fn resolve_constraint_mapped_type_parameters(&mut self, ty: TypeId) {
        if self.instantiated_type_parameters.contains_key(&ty) {
            return;
        }
        let Some(&symbol) = self.type_parameter_symbols.get(&ty) else { return };
        // getConstraintDeclaration (`checker.go:29132`): the first declaration
        // that has a constraint.
        let declarations = &self.binder.symbols().get(symbol).declarations;
        let Some(constraint) =
            declarations.iter().find_map(|&declaration| match self.node_map.get(declaration) {
                Some(tsr_ast::Node::TypeParameterDeclaration(parameter)) => parameter.constraint,
                _ => None,
            })
        else {
            return;
        };
        self.resolve_eager_mapped_type_parameters(constraint);
    }

    /// Walk the constituents `getTypeFromTypeNode` resolves eagerly — not a
    /// type literal's or signature's members, nor a mapped type's template —
    /// and resolve each mapped type's parameter base constraint.
    fn resolve_eager_mapped_type_parameters(&mut self, node: tsr_ast::TypeNode<'a>) {
        use tsr_ast::TypeNode;
        match node {
            TypeNode::MappedTypeNode(mapped) => {
                let Some(parameter) = mapped
                    .type_parameter
                    .and_then(|parameter| parameter.node_id)
                    .and_then(|id| self.binder.symbol_of(id))
                else {
                    return;
                };
                let parameter = self.get_declared_type_of_symbol(parameter);
                if parameter != self.intrinsics.error {
                    self.base_constraint_of_type(parameter);
                }
            }
            TypeNode::ParenthesizedTypeNode(inner) => {
                if let Some(inner) = inner.r#type {
                    self.resolve_eager_mapped_type_parameters(inner);
                }
            }
            TypeNode::TypeOperatorNode(operator) => {
                if let Some(inner) = operator.r#type {
                    self.resolve_eager_mapped_type_parameters(inner);
                }
            }
            TypeNode::ArrayTypeNode(array) => {
                if let Some(element) = array.element_type {
                    self.resolve_eager_mapped_type_parameters(element);
                }
            }
            TypeNode::UnionTypeNode(union) => {
                for &member in union.types {
                    self.resolve_eager_mapped_type_parameters(member);
                }
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                for &member in intersection.types {
                    self.resolve_eager_mapped_type_parameters(member);
                }
            }
            TypeNode::IndexedAccessTypeNode(access) => {
                for part in [access.object_type, access.index_type].into_iter().flatten() {
                    self.resolve_eager_mapped_type_parameters(part);
                }
            }
            TypeNode::TypeReferenceNode(reference) => {
                for &argument in reference.type_arguments {
                    self.resolve_eager_mapped_type_parameters(argument);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Checker;

    /// The TS2313 spans `check_source_file` reports for `source`, as text.
    fn circular_constraint_reports(source: &str) -> Vec<String> {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.expect("registered file");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "circular.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.check_source_file(
            root,
            crate::check::FileContext { ambient: false, has_parse_errors: false },
        );
        checker
            .diagnostics
            .iter()
            .filter(|(_, diagnostic)| {
                diagnostic.message.code()
                    == tsr_diagnostics::messages::TYPE_PARAMETER_0_HAS_A_CIRCULAR_CONSTRAINT.code()
            })
            .map(|(_, diagnostic)| {
                source[diagnostic.span.start as usize..diagnostic.span.end as usize].to_string()
            })
            .collect()
    }

    /// `incorrectRecursiveMappedTypeConstraint`: building the constraint's
    /// mapped type resolves its key parameter's constraint, which is `T`
    /// again, so both `T` and `P` report (native reports both, at their
    /// constraint nodes).
    #[test]
    fn mapped_constraint_over_its_own_parameter_is_circular() {
        let mut reports = circular_constraint_reports(
            "function sum<T extends { [P in T]: number }, K extends keyof T>(n: number, v: T, k: K) {}",
        );
        reports.sort();
        assert_eq!(reports, ["T", "{ [P in T]: number }"]);
    }

    /// A mapped constraint whose key parameter reaches another parameter is
    /// not a cycle.
    #[test]
    fn mapped_constraint_over_another_parameter_is_not_circular() {
        let reports = circular_constraint_reports(
            "function f<U, T extends { [P in keyof U]: number }>(u: U, t: T) {}",
        );
        assert!(reports.is_empty(), "{reports:?}");
    }
}
