//! Semantic mapped type metadata, ported from internal/checker/checker.go.
use crate::{Checker, types::TypeId};
use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

/// One evaluation of a mapped type node
/// ([`Checker::evaluate_mapped_type_node`]).
pub(crate) enum MappedNodeType {
    /// The node's type, published for its context.
    Built(TypeId),
    /// No type was built; the evaluated parts, when there are any, are for
    /// the caller's own image of the node.
    Declined(Option<MappedTypeInfo>),
}

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
    /// The node's `typeNodeLinks` context ([`Checker::type_literal_key`])
    /// when the info was captured from a written node: two captures under one
    /// key are one native type and share one member table (ADR-0050).
    pub(crate) node_key: Option<std::rc::Rc<crate::declared::TypeLiteralKey>>,
    /// For an instance minted by [`Checker::instantiate_mapped_type`]: the
    /// mapped type it instantiates and the mapper, native's
    /// `MappedType.target` and `.mapper`, which
    /// getResolvedApparentTypeOfMappedType re-instantiates with the
    /// homomorphic type variable prepended ([`Checker::apparent_mapped_type`]).
    pub(crate) instance: Option<std::rc::Rc<MappedInstance>>,
}

/// The target and mapper of a mapped instance ([`MappedTypeInfo::instance`]).
#[derive(Debug)]
pub(crate) struct MappedInstance {
    pub(crate) target: TypeId,
    pub(crate) mapper: CombinedMapper,
}

/// A type mapper in `instantiate_type`'s form: the pairs, the parameters
/// they may mention, and those parameters' names.
pub(crate) type CombinedMapper = (Vec<(TypeId, TypeId)>, Vec<TypeId>, Vec<String>);

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

/// The node builder's `approximateLength` and `truncating` context
/// (nodebuilderimpl.go:60) for one top-level print.
#[derive(Default)]
struct TruncationBudget {
    approximate_length: usize,
    truncating: bool,
    /// The mapped objects whose members are being printed, outermost first:
    /// one met again prints its fixed text instead of recursing.
    visiting: Vec<TypeId>,
}

impl TruncationBudget {
    /// `noTruncationMaximumTruncationLength` (nodebuilderimpl.go:114).
    const NO_TRUNCATION_MAXIMUM_LENGTH: usize = 1_000_000;

    /// checkTruncationLength (nodebuilderimpl.go:140): once set, truncating
    /// stays set for the rest of the print.
    fn check(&mut self) -> bool {
        if !self.truncating {
            self.truncating = self.approximate_length > Self::NO_TRUNCATION_MAXIMUM_LENGTH;
        }
        self.truncating
    }
}

impl<'a> Checker<'a, '_> {
    /// getTypeFromMappedTypeNode and createMappedTypeNodeFromType (checker.go,
    /// nodebuilderimpl.go). Build a deferred mapped type from semantic parts
    /// when its template is outside the bounded written-node renderer.
    ///
    /// getTypeFromMappedTypeNode (checker.go:24170) answers one type per node
    /// (`typeNodeLinks.resolvedType`). The answer is published in
    /// `type_literal_types`, the table type-literal and function-type nodes
    /// already use for the same links, under the same context key
    /// ([`Checker::type_literal_key`]: the node, the alias-evaluation
    /// bindings and whether a mapped template encloses it), so a re-resolved
    /// alias body or an enclosing template keeps its own image. Without it
    /// every evaluation minted a fresh type, and identity tests such as
    /// `typeToTypeNodeHelperWithPossibleReusableTypeNode`'s
    /// `getTypeFromTypeNode(node) == t` never held (r5-mapped4.md §2). A
    /// declined build is not published, so a later evaluation retries it.
    pub(crate) fn create_semantic_mapped_type(
        &mut self,
        node: &'a tsr_ast::MappedTypeNode<'a>,
    ) -> Option<TypeId> {
        match self.evaluate_mapped_type_node(node) {
            MappedNodeType::Built(ty) => Some(ty),
            MappedNodeType::Declined(_) => None,
        }
    }

    /// [`Checker::create_semantic_mapped_type`] for a caller that has a
    /// fallback for a declined build: the node's parts are evaluated once,
    /// and a declined build hands them back, so the caller publishes them
    /// on its own image ([`Checker::publish_mapped_type_info`]) instead of
    /// evaluating the node a second time through
    /// [`Checker::capture_mapped_type`]. Native evaluates a mapped node
    /// once per context (getTypeFromMappedTypeNode's
    /// `typeNodeLinks.resolvedType`, checker.go:24170); the second
    /// evaluation was a port artifact that cost domain-model 6.7 M Ir on
    /// 80 `DeepReadonly<T[K]>` templates (r5-mapped6.md §1).
    pub(crate) fn evaluate_mapped_type_node(
        &mut self,
        node: &'a tsr_ast::MappedTypeNode<'a>,
    ) -> MappedNodeType {
        let key = node.node_id.map(|id| self.type_literal_key(id));
        if let Some(key) = &key
            && let Some(&ty) = self.type_literal_types.get(key)
        {
            return MappedNodeType::Built(ty);
        }
        let Some(info) = self.mapped_type_info(node) else {
            return MappedNodeType::Declined(None);
        };
        let ty = if self.is_generic_mapped_info(&info) {
            let Some(text) = self.mapped_type_text(&info) else {
                return MappedNodeType::Declined(Some(info));
            };
            let ty = self.store.new_named(crate::flags::TypeFlags::OBJECT, text, None);
            self.mapped_types.insert(ty, info);
            ty
        } else {
            match self.resolved_mapped_object(info) {
                Ok(ty) => ty,
                Err(info) => return MappedNodeType::Declined(Some(info)),
            }
        };
        if let Some(key) = key {
            self.type_literal_types.insert(key, ty);
        }
        MappedNodeType::Built(ty)
    }

    fn mapped_type_text(&mut self, info: &MappedTypeInfo) -> Option<String> {
        let Some(Node::MappedTypeNode(node)) = self.node_map.get(info.declaration) else {
            return None;
        };
        let name = node.type_parameter?.name?.text;
        // The node builder preserves the top-level keyof operator even
        // when resolving its operand would produce a concrete key union.
        let constraint = if let Some(source) = info.modifiers_source
            && info.keyof_constraint
        {
            // emitTypeOperator's operand precedence (`printer.go:2274`):
            // a union, intersection, conditional or function operand is
            // parenthesised; an aliased union is a reference and is not.
            let text = self.type_to_string(source);
            if crate::node_reuse::binds_below_type_operator(&text) {
                format!("keyof ({text})")
            } else {
                format!("keyof {text}")
            }
        } else {
            self.type_to_string(info.constraint)
        };
        // createMappedTypeNodeFromType (nodebuilderimpl.go:1471) prints
        // removeMissingType(getTemplateTypeFromMappedType(t), isOptional): the
        // template with `?`'s optionality, minus the missing type that exact
        // optional properties add (r4-mapped.md §3).
        let template = if self.exact_optional_property_types {
            info.template
        } else {
            self.mapped_template_type(info)
        };
        let template = self.type_to_string(template);
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
    /// (checker.go:30607) for a property with no name type: its key is
    /// `getStringLiteralType(name)`.
    pub(crate) fn generic_mapped_contextual_property_type(
        &mut self,
        id: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        let key = self.store.intern_literal(
            crate::flags::TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral(name.to_owned()),
            false,
        );
        self.generic_mapped_contextual_property_type_of_key(id, key)
    }

    /// getIndexedMappedTypeSubstitutedTypeOfContextualType
    /// (checker.go:30607) for the property name type `key`: a late-bound
    /// computed name passes its own type (`typeof A` for `[A]`), not a string
    /// spelling of its display name. Generic key domains use their base
    /// constraints, while substitution retains the mapped template's indexed
    /// identities.
    pub(crate) fn generic_mapped_contextual_property_type_of_key(
        &mut self,
        id: TypeId,
        key: TypeId,
    ) -> Option<TypeId> {
        use crate::flags::TypeFlags;
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
        // The inner walk answers `false` for a type that is not mapped; ask
        // that before allocating its cycle path (`r5-perf4.md` §5).
        self.mapped_types.contains_key(&id)
            && self.is_generic_homomorphic_mapped_type_inner(id, &mut Vec::new())
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
            self.publish_mapped_type_info(id, info);
        }
    }

    /// Record `info` as the mapped parts of `id`, a written-text image of
    /// the node `info` was evaluated from.
    pub(crate) fn publish_mapped_type_info(&mut self, id: TypeId, info: MappedTypeInfo) {
        // getTypeFromMappedTypeNode's `typeNodeLinks.resolvedType`: the
        // first type captured for this node and context is the node's type,
        // so a later evaluation of the node answers it instead of a second
        // image of one mapped type (ADR-0050).
        if let Some(key) = info.node_key.as_deref() {
            self.type_literal_types.entry(key.clone()).or_insert(id);
        }
        self.mapped_types.insert(id, info);
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
        let node_key = node.node_id.map(|id| std::rc::Rc::new(self.type_literal_key(id)));
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
            homomorphic_symbol = self.keyof_operand_type_parameter(operand);
            let operand = self.get_type_from_type_node(operand);
            modifiers_source = Some(operand);
            self.resolved_keyof_type(operand).unwrap_or(self.intrinsics.error)
        } else {
            modifiers_source = self.indirect_mapped_modifiers_source(constraint);
            self.mapped_constraint_type(constraint)
        };
        // getConstraintFromTypeParameter (checker.go:17085): an `any` key
        // constraint of a mapped type parameter is stringNumberSymbolType.
        let constraint = if constraint != self.intrinsics.error
            && self.store.get(constraint).flags.contains(crate::flags::TypeFlags::ANY)
        {
            self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ])
        } else {
            constraint
        };
        // A node evaluated under alias-evaluation frames is this port's image
        // of an instantiated mapped type (the alias body re-resolved under its
        // arguments, where native instantiates one type). Its iteration
        // parameter is the instance's clone, as instantiateAnonymousType
        // makes it (checker.go:22461): the frames are the clone's mapper, so
        // its constraint is the declared one instantiated (`keyof U` for
        // `MyMap<U>`). The template and `as` clause are resolved once with
        // `P` bound to the clone, in a frame of its own, rather than renamed
        // afterwards: a rename re-resolves every alias reference they hold,
        // and each of those would rename again (r5-mapped4.md §5).
        let parameter_type = if self.alias_evaluation_bindings.is_empty() {
            parameter_type
        } else {
            let map = self.alias_evaluation_map();
            let parameters: Vec<_> = map.iter().map(|&(parameter, _)| parameter).collect();
            self.clone_mapped_type_parameter(parameter_type, &map, &parameters, &[]).0
        };
        let rebinds = !self.alias_evaluation_bindings.is_empty();
        if rebinds {
            self.alias_evaluation_bindings.push([(symbol, parameter_type)].into_iter().collect());
        }
        self.mapped_template_depth += 1;
        let template = self.get_type_from_type_node(template);
        let name_type = node.name_type.map(|node| self.get_type_from_type_node(node));
        self.mapped_template_depth -= 1;
        if rebinds {
            self.alias_evaluation_bindings.pop();
        }

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
            node_key,
            instance: None,
        })
    }

    /// getHomomorphicTypeVariable (checker.go): the type parameter a
    /// `keyof T` constraint names, read from the written operand.
    fn keyof_operand_type_parameter(&mut self, operand: TypeNode<'a>) -> Option<SymbolId> {
        let TypeNode::TypeReferenceNode(reference) = operand else { return None };
        reference.type_name.and_then(|name| {
            self.resolve_entity_name(name, SymbolFlags::TYPE).filter(|&symbol| {
                self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER)
            })
        })
    }

    /// The homomorphic type variable of a mapped alias body, without
    /// resolving its constraint (getHomomorphicTypeVariable reads only the
    /// constraint declaration's operand).
    fn mapped_alias_homomorphic_parameter(&mut self, symbol: SymbolId) -> Option<SymbolId> {
        let declaration = self.type_alias_declaration_of(symbol)?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(TypeNode::MappedTypeNode(mapped)) = alias.r#type else { return None };
        let Some(TypeNode::TypeOperatorNode(operator)) = mapped.type_parameter?.constraint else {
            return None;
        };
        if operator.operator.kind != SyntaxKind::KeyOfKeyword {
            return None;
        }
        self.keyof_operand_type_parameter(operator.r#type?)
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
        let map = self.alias_evaluation_map();
        let parameters: Vec<_> = map.iter().map(|&(parameter, _)| parameter).collect();
        Some(self.instantiate_type(operand, &map, &parameters, &[]))
    }

    /// The active alias-evaluation frames as a type mapper: each bound
    /// parameter's declared type to its argument (an inner frame wins).
    fn alias_evaluation_map(&mut self) -> Vec<(TypeId, TypeId)> {
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        bindings
            .into_iter()
            .map(|(symbol, ty)| (self.get_declared_type_of_symbol(symbol), ty))
            .collect()
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
        let Some(declaration) = self.type_alias_declaration_of(symbol) else {
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
            return;
        }
        // A concrete instance of a mapped alias (`Partial<Foo1>`), or the
        // image of an argument-less one (`Funcs`), is minted with its
        // members resolved through the source (`instantiate_identity_mapped_alias`)
        // and no mapped info. Native's instance is a MappedType all the same
        // (getTypeAliasInstantiation → instantiateMappedType), which
        // isMappedTypeGenericIndexedAccess and getConstraintFromIndexedAccess
        // (checker.go:17227) read, so its parts are captured on first ask.
        let target = match self.type_reference_targets.get(&id) {
            Some((symbol, arguments)) => Some((*symbol, arguments.clone())),
            None => match self.store.get(id).data {
                crate::types::TypeData::Named { members: Some(symbol), .. } => {
                    Some((symbol, Vec::new()))
                }
                _ => None,
            },
        };
        if let Some((symbol, arguments)) = target {
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

    /// getIndexTypeForMappedType (checker.go:26871) over a generic key
    /// domain, the branch `computeBaseConstraint`'s Index arm (`:27523`)
    /// and checkIndexedAccessIndexType reach for a generic mapped type with
    /// an `as` clause. getIndexType itself defers that `keyof`
    /// (shouldDeferIndexType), so [`Checker::mapped_index_type`] declines.
    /// Each constituent of the constraint, generic ones included, is mapped
    /// through the name type (forEachType). A homomorphic mapping answers
    /// `None`: its keys come from getIndexTypeForGenericType, a deferred
    /// `keyof` the caller already holds.
    pub(crate) fn index_type_for_generic_mapped_type(&mut self, id: TypeId) -> Option<TypeId> {
        self.ensure_mapped_type_info(id);
        let info = self.mapped_types.get(&id)?.clone();
        let Some(name_type) = info.name_type else { return Some(info.constraint) };
        if !self.is_generic_index_type(info.constraint) {
            return self.mapped_index_type(id);
        }
        if info.keyof_constraint {
            return None;
        }
        let keys = match self.store.get(info.constraint).data.clone() {
            crate::types::TypeData::Union { types, .. } => types,
            _ => vec![info.constraint],
        };
        let mut names = Vec::with_capacity(keys.len());
        for key in keys {
            let name =
                self.instantiate_type(name_type, &[(info.parameter, key)], &[info.parameter], &[]);
            if name == self.intrinsics.error {
                return None;
            }
            // `keyof` of a concrete string index is `string | number`.
            if name == self.intrinsics.string {
                names.push(self.intrinsics.number);
            }
            names.push(name);
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
            let composite =
                self.store.get(source).flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION);
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
                if composite
                    && names.iter().any(|name| self.get_property_of_type(source, name).is_none())
                {
                    return None;
                }
                names
            } else if composite {
                self.composite_modifiers_property_names(source)
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
            // forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType
            // (checker.go:22731): an `any` modifiers type contributes a
            // string key in place of index signatures.
            if self.store.get(source).flags.contains(TypeFlags::ANY) {
                keys.push(self.intrinsics.string);
            } else if let Some(indexes) = self.get_index_infos_of_type(source) {
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

    /// getPropertiesOfUnionOrIntersectionType (checker.go:18861) for a
    /// concrete composite modifiers type: candidate names come from each
    /// constituent's own properties in order, and a name is kept when the
    /// composite resolves it (getPropertyOfUnionOrIntersectionType, which
    /// drops a union's partial properties). A union reads past its first
    /// constituent only while constituents carry index signatures.
    ///
    /// `Readonly<string[] & { brand }>` reaches this through
    /// instantiateAnonymousType, because isArrayOrTupleOrIntersection needs
    /// every constituent to be an array or tuple. The certified enumeration
    /// (`get_property_names_of_type`) declines on array methods that mention
    /// `this`; names alone are all a key walk needs, since each member's value
    /// is the template under its key (r4-mapped.md §2). Nothing is published
    /// here: the caller's member image is the only table.
    fn composite_modifiers_property_names(&mut self, source: TypeId) -> Vec<String> {
        use crate::types::TypeData;
        let (types, is_union) = match &self.store.get(source).data {
            TypeData::Union { types, .. } => (types.clone(), true),
            TypeData::Intersection { types, .. } => (types.clone(), false),
            _ => return self.property_names_of(source),
        };
        let mut checked = rustc_hash::FxHashSet::default();
        let mut names = Vec::new();
        for part in types {
            for name in self.property_names_of(part) {
                if checked.insert(name.clone())
                    && self.get_property_of_type(source, &name).is_some()
                {
                    names.push(name);
                }
            }
            if is_union && self.get_index_infos_of_type(part).is_some_and(|infos| infos.is_empty())
            {
                break;
            }
        }
        names
    }

    /// isTypeUsableAsPropertyName/getPropertyNameFromType (checker.go): a
    /// string, number or enum literal key names a property by its value.
    fn mapped_key_property_name(&self, ty: TypeId) -> Option<String> {
        use crate::types::{EnumLiteralValue, TypeData};
        match &self.store.get(ty).data {
            TypeData::StringLiteral(name)
            | TypeData::NumberLiteral(name)
            | TypeData::EnumLiteral {
                value: EnumLiteralValue::String(name) | EnumLiteralValue::Number(name),
                ..
            } => Some(name.clone()),
            _ => None,
        }
    }

    fn resolve_mapped_type_members_worker(&mut self, id: TypeId) {
        use crate::{flags::TypeFlags, types::TypeData};
        let Some(info) = self.mapped_types.get(&id).cloned() else { return };
        if self.anonymous_properties.contains_key(&id) {
            return;
        }
        // getTypeFromMappedTypeNode answers one type per node and context
        // (`typeNodeLinks.resolvedType`); this port can capture the same node
        // and context twice (an alias instance's `capture_mapped_alias`, and
        // the node's own evaluation). The second capture reads the first's
        // member table, whose slots stay owned by the first, so each mapped
        // symbol is instantiated once (ADR-0050).
        if let Some(key) = info.node_key.as_deref()
            && let Some(&first) = self.type_literal_types.get(key)
            && first != id
            && let Some((properties, true)) = self.anonymous_properties.get(&first).cloned()
        {
            let indexes = self.object_literal_index_infos.get(&first).cloned().unwrap_or_default();
            self.anonymous_properties.insert(id, (properties, true));
            self.object_literal_index_infos.insert(id, indexes);
            return;
        }
        let modifiers = info.modifiers_source.map(|source| self.apparent_type(source));
        let Some(keys) = self.mapped_member_keys(&info, modifiers) else { return };
        // resolveMappedTypeMembers combines source keys before substituting
        // the template, so colliding names see the entire key union.
        let mut members: Vec<(TypeId, TypeId, TypeId)> = Vec::new();
        // The first member each property name landed on: resolveMappedTypeMembers
        // keys its member table by name, so the merge below is a lookup, not
        // a rescan of every earlier member (r5-mapped4.md §3).
        let mut member_by_name: rustc_hash::FxHashMap<String, usize> =
            rustc_hash::FxHashMap::default();
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
                // Keys naming one property share it; native unions their key
                // types under the shared name (resolveMappedTypeMembers).
                let property_name = self.mapped_key_property_name(name);
                if let Some(&index) =
                    property_name.as_ref().and_then(|name| member_by_name.get(name))
                {
                    let keys = &mut members[index].1;
                    *keys = self.get_union_type(&[*keys, key]);
                } else {
                    if let Some(property_name) = property_name {
                        member_by_name.insert(property_name, members.len());
                    }
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
        let mut property_names: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();
        let mut indexes: Vec<crate::index_signatures::IndexInfo> = Vec::new();
        for (name_type, key, first_key) in members {
            let Some(name) = self.mapped_key_property_name(name_type) else {
                // The index-signature arm instantiates templateType, which
                // getTemplateTypeFromMappedType built with addOptionality
                // (checker.go:22697): `Partial`-style `?` maps include
                // undefined in the index value (r4-mapped.md §3).
                let template = self.mapped_template_type(&info);
                let value = self.instantiate_type(
                    template,
                    &[(info.parameter, key)],
                    &[info.parameter],
                    &[],
                );
                let name_flags = self.store.get(name_type).flags;
                if self.is_valid_index_key_type(name_type)
                    || name_flags.intersects(TypeFlags::ANY | TypeFlags::ENUM)
                {
                    let key = if name_flags.intersects(TypeFlags::ANY | TypeFlags::STRING) {
                        self.intrinsics.string
                    } else if name_flags.intersects(TypeFlags::NUMBER | TypeFlags::ENUM) {
                        self.intrinsics.number
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
            if !property_names.insert(name.clone()) {
                continue;
            }
            let source_name = self.mapped_key_property_name(first_key);
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
            // getTypeOfMappedSymbol (checker.go:20984) instantiates the
            // template on the first read of the property's type, not here
            // (ADR-0050): the slot records the key and whether the modifier
            // strips the source's optionality.
            let strip_optional = self.strict_null_checks && !optional && was_optional;
            let index = u32::try_from(properties.len()).expect("member count fits u32");
            properties.push(crate::objects::AnonymousProperty {
                accessor_write: None,
                method: false,
                origin,
                checked_declaration: None,
                name,
                printed_name,
                printed_slot: crate::objects::PrintedSlot::on_demand(),
                optional,
                readonly,
                slot: crate::objects::PropertySlot::of_mapped(id, index, key, strip_optional),
            });
        }
        self.anonymous_properties.insert(id, (properties, true));
        self.object_literal_index_infos.insert(id, indexes);
    }

    /// getTypeOfMappedSymbol (checker.go:20984): the type of the `index`th
    /// property of the mapped type `owner`, instantiated from the template
    /// with `key` on the first read and published in `owner`'s member table,
    /// the port's `valueSymbolLinks.resolvedType` (ADR-0050).
    ///
    /// A read that re-enters while the instantiation runs answers
    /// `errorType`, as native's failed `pushTypeResolution` does. Native then
    /// also publishes `errorType` for the outer read and reports TS2615; the
    /// outer read here keeps its instantiation, as the eager member
    /// resolution this replaces did (a recorded divergence, ADR-0050).
    pub(crate) fn get_type_of_mapped_symbol(
        &mut self,
        owner: TypeId,
        index: u32,
        key: TypeId,
        strip_optional: bool,
    ) -> TypeId {
        if let Some(published) = self.peek_mapped_symbol_type(owner, index) {
            return published;
        }
        let Some(info) = self.mapped_types.get(&owner).cloned() else {
            return self.intrinsics.error;
        };
        self.publish_mapped_symbol_type(owner, index, self.intrinsics.error);
        let mut value =
            self.instantiate_type(info.template, &[(info.parameter, key)], &[info.parameter], &[]);
        // getTypeOfMappedSymbol (checker.go:20993). Excluding optionality
        // strips missing in exact mode, otherwise undefined.
        if strip_optional {
            value = self.remove_missing_or_undefined_type(value);
        }
        self.publish_mapped_symbol_type(owner, index, value);
        value
    }

    /// The published type of a mapped property ([`Checker::get_type_of_mapped_symbol`]),
    /// without instantiating it.
    pub(crate) fn peek_mapped_symbol_type(&self, owner: TypeId, index: u32) -> Option<TypeId> {
        let (properties, _) = self.anonymous_properties.get(&owner)?;
        let property = properties.get(index as usize)?;
        if property.slot.mapped().is_some() {
            return None;
        }
        self.peek_property_type(property)
    }

    fn publish_mapped_symbol_type(&mut self, owner: TypeId, index: u32, value: TypeId) {
        if let Some((properties, _)) = self.anonymous_properties.get_mut(&owner)
            && let Some(property) = properties.get_mut(index as usize)
        {
            property.slot = crate::objects::PropertySlot::resolved(value);
        }
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
        use crate::flags::TypeFlags;
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
        info.node_key = None;
        info.instance = Some(std::rc::Rc::new(MappedInstance {
            target: id,
            mapper: (
                map.to_vec(),
                parameters.to_vec(),
                names.iter().map(|&name| name.to_owned()).collect(),
            ),
        }));
        info.constraint = self.instantiate_type(info.constraint, map, parameters, names);
        // instantiateAnonymousType (checker.go:22461): the instance iterates a
        // fresh clone of the declared parameter, whose mapper combines
        // `P -> P'` with the instance's, so getConstraintOfTypeParameter(P')
        // is the instantiated constraint. The template and `as` clause are
        // instantiated under that combined mapper.
        let (fresh, (combined, sources, source_names)) =
            self.clone_mapped_type_parameter(info.parameter, map, parameters, names);
        info.parameter = fresh;
        let source_names: Vec<_> = source_names.iter().map(String::as_str).collect();
        info.template = self.instantiate_type(info.template, &combined, &sources, &source_names);
        info.name_type =
            info.name_type.map(|ty| self.instantiate_type(ty, &combined, &sources, &source_names));
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
        // createTypeNodeFromObjectType (nodebuilderimpl.go:2690) prints an
        // instance that is still isGenericMappedType from its parts
        // (createMappedTypeNodeFromType), as getTypeFromMappedTypeNode's
        // generic arm does: `{ -readonly [P in keyof T]: … }` for
        // `Promise.allSettled`'s mapped return under `T_1 := T`, where the
        // member print listed the constraint's array members.
        if self.is_generic_mapped_info(&info) {
            let Some(text) = self.mapped_type_text(&info) else { return self.intrinsics.error };
            let ty = self.store.new_named(TypeFlags::OBJECT, text, None);
            self.mapped_types.insert(ty, info);
            return ty;
        }
        self.resolved_mapped_object(info).unwrap_or(self.intrinsics.error)
    }

    /// The mapped arm of instantiateAnonymousType (checker.go:22461):
    /// `cloneTypeParameter(getTypeParameterFromMappedType(t))`, with the
    /// combined mapper `P -> P'` then `map` recorded as the clone's mapper, so
    /// getConstraintOfTypeParameter(P') resolves per instance through
    /// `instantiated_type_parameters`. Returns the clone and the combined
    /// mapper (pairs, parameters, names) for the instance's template.
    fn clone_mapped_type_parameter(
        &mut self,
        original: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> (TypeId, CombinedMapper) {
        let name = self.type_to_string(original);
        // One clone per instantiation: native clones inside
        // instantiateAnonymousType, which getObjectTypeInstantiation caches
        // by the outer type arguments. `P` stands for its mapped declaration,
        // so `(P, map)` is that cache's key in `instantiated_objects`, whose
        // other keys are object types.
        let key = (original, map.to_vec());
        let fresh = if let Some(&fresh) = self.instantiated_objects.get(&key) {
            fresh
        } else {
            let fresh =
                self.store.new_named(crate::flags::TypeFlags::TYPE_PARAMETER, name.clone(), None);
            if let Some(&symbol) = self.type_parameter_symbols.get(&original) {
                self.type_parameter_symbols.insert(fresh, symbol);
            }
            self.instantiated_objects.insert(key, fresh);
            fresh
        };
        let mut combined = Vec::with_capacity(map.len() + 1);
        combined.push((original, fresh));
        combined.extend_from_slice(map);
        let mut sources = Vec::with_capacity(parameters.len() + 1);
        sources.push(original);
        sources.extend_from_slice(parameters);
        let mut source_names = Vec::with_capacity(names.len() + 1);
        source_names.push(name);
        source_names.extend(names.iter().map(|&name| name.to_owned()));
        self.instantiated_type_parameters.entry(fresh).or_insert_with(|| {
            crate::inference::InstantiatedTypeParameter {
                target: original,
                map: combined.clone(),
                parameters: sources.clone(),
                names: source_names.clone(),
            }
        });
        (fresh, (combined, sources, source_names))
    }

    /// createTypeNodeFromObjectType (nodebuilderimpl.go:2690) for a mapped
    /// type: resolveMappedTypeMembers' table printed as a type literal. When
    /// member resolution declines, the mapped form stands in, as
    /// createMappedTypeNodeFromType would print it. When the mapped form
    /// cannot be printed either, the parts are handed back.
    fn resolved_mapped_object(&mut self, info: MappedTypeInfo) -> Result<TypeId, MappedTypeInfo> {
        use crate::flags::TypeFlags;
        let Some(text) = self.mapped_type_text(&info) else { return Err(info) };
        let mapped = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(mapped, info.clone());
        self.resolve_mapped_type_members(mapped);
        let Some((properties, _)) = self.anonymous_properties.get(&mapped).cloned() else {
            return Ok(mapped);
        };
        let indexes = self.object_literal_index_infos.get(&mapped).cloned().unwrap_or_default();
        let mut budget = TruncationBudget { visiting: vec![mapped], ..TruncationBudget::default() };
        let text = self.mapped_object_text(mapped, &mut budget);
        let result = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.mapped_types.insert(result, info);
        self.anonymous_properties.insert(result, (properties, true));
        self.object_literal_index_infos.insert(result, indexes);
        Ok(result)
    }

    /// createTypeNodeFromObjectType's member print of a resolved mapped type
    /// (`createTypeNodesFromResolvedType`, nodebuilderimpl.go:2627), with the
    /// node builder's truncation (`checkTruncationLength`, :140) under
    /// `TypeFormatFlagsNoTruncation`, the flags a `.types` baseline prints
    /// with (ADR-0050).
    ///
    /// Once the approximate length passes the limit, the remaining
    /// properties but the last are dropped (their `/* ... more elided ... */`
    /// comment is not printed), and an object printed after that is
    /// `{  }` (a `NotEmittedTypeElement`). A property whose type is itself a
    /// resolved mapped object is printed through this function with the
    /// same budget, as the node builder recurses; other property types count
    /// their printed length.
    fn mapped_object_text(&mut self, id: TypeId, budget: &mut TruncationBudget) -> String {
        use crate::objects::Member;
        let properties =
            self.anonymous_properties.get(&id).map(|(properties, _)| properties.clone());
        let properties = properties.unwrap_or_default();
        let indexes = self.object_literal_index_infos.get(&id).cloned().unwrap_or_default();
        if properties.is_empty() && indexes.is_empty() {
            budget.approximate_length += 2;
            return "{}".to_string();
        }
        if budget.check() {
            return "{  }".to_string();
        }
        let readonly = self.mapped_types.get(&id).is_some_and(|info| info.readonly == Some(true));
        let mut members: Vec<_> = indexes
            .iter()
            .map(|index| {
                let member = Member::Index {
                    readonly,
                    name: "x".to_string(),
                    key: self.type_to_string(index.key),
                    value: self.type_to_string(index.value),
                };
                if let Member::Index { key, value, .. } = &member {
                    budget.approximate_length += key.len() + value.len() + 6;
                }
                member
            })
            .collect();
        let count = properties.len();
        for (position, property) in properties.iter().enumerate() {
            if budget.check() && position + 3 < count - 1 {
                members.push(self.mapped_property_member(&properties[count - 1], budget));
                break;
            }
            members.push(self.mapped_property_member(property, budget));
        }
        budget.approximate_length += 2;
        crate::objects::render_object_type(&members)
    }

    /// addPropertyToElementList (nodebuilderimpl.go:2522) for a property of
    /// a resolved mapped object: its name, then its type.
    fn mapped_property_member(
        &mut self,
        property: &crate::objects::AnonymousProperty,
        budget: &mut TruncationBudget,
    ) -> crate::objects::Member {
        use crate::types::TypeData;
        budget.approximate_length += property.name.len() + 1;
        let ty = self.property_type(property);
        let printed = if !budget.visiting.contains(&ty) && self.is_member_printed_mapped_object(ty)
        {
            budget.visiting.push(ty);
            let printed = self.mapped_object_text(ty, budget);
            budget.visiting.pop();
            printed
        } else {
            let printed = self.property_printed_type(property).into_owned();
            // typeToTypeNode's string-literal arm counts the value and its
            // quotes; other kinds count their printed text.
            budget.approximate_length += match &self.store.get(ty).data {
                TypeData::StringLiteral(value) => value.len() + 2,
                _ => printed.len(),
            };
            printed
        };
        if property.readonly {
            budget.approximate_length += 9;
        }
        crate::objects::Member::Property {
            name: property.printed_name.clone(),
            optional: property.optional,
            readonly: property.readonly,
            printed,
        }
    }

    /// Whether `ty` prints its resolved members (createTypeNodeFromObjectType
    /// for a mapped type that is not `isGenericMappedType`).
    fn is_member_printed_mapped_object(&mut self, ty: TypeId) -> bool {
        let Some(info) = self.mapped_types.get(&ty).cloned() else { return false };
        self.anonymous_properties.get(&ty).is_some_and(|&(_, complete)| complete)
            && !self.is_generic_mapped_info(&info)
    }

    /// isGenericMappedType (checker.go:24908) of a type: a mapped type
    /// whose parts are generic ([`Checker::is_generic_mapped_info`]).
    // Read by `contextual_argument_type` (contextual.rs, main's) in
    // `r6-mapped-apparent-instance.diff` (r6-mapped.md §1).
    #[allow(dead_code)]
    pub(crate) fn is_generic_mapped_type(&mut self, id: TypeId) -> bool {
        let Some(info) = self.mapped_types.get(&id).cloned() else { return false };
        self.is_generic_mapped_info(&info)
    }

    /// isGenericMappedType (checker.go:24908): a generic key domain, or an
    /// `as` clause that is still generic once the constraint is substituted
    /// for the iteration type parameter.
    fn is_generic_mapped_info(&mut self, info: &MappedTypeInfo) -> bool {
        if self.is_generic_index_type(info.constraint) {
            return true;
        }
        info.name_type.is_some_and(|name| {
            let name = self.instantiate_type(
                name,
                &[(info.parameter, info.constraint)],
                &[info.parameter],
                &[],
            );
            self.is_generic_index_type(name)
        })
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
        let info = self.mapped_types.get(&id).cloned();
        let parameter = match &info {
            Some(info) => info.homomorphic_symbol?,
            None => self.mapped_alias_homomorphic_parameter(symbol)?,
        };
        let declaration = self.type_alias_declaration_of(symbol)?;
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
        let Some(info) = info else {
            // instantiateMappedType (checker.go:22535) distributes over the
            // mapped type variable before it reads the constraint, so a union
            // argument whose `keyof` this port cannot resolve (capture
            // declined) still maps per constituent (r4-mapped.md §1).
            let source = *arguments.get(slot)?;
            return self.distribute_mapped_union(source, Some(id), replace_source);
        };
        self.instantiate_mapped_sequence(&info, Some(id), replace_source)
    }

    /// The union arm of instantiateMappedType: mapTypeWithAlias over the
    /// instantiated type variable, one instantiation per constituent.
    fn distribute_mapped_union(
        &mut self,
        source: TypeId,
        alias: Option<TypeId>,
        mut replace_source: impl FnMut(&mut Self, TypeId) -> TypeId,
    ) -> Option<TypeId> {
        use crate::types::TypeData;
        let TypeData::Union { types, .. } = &self.store.get(source).data else { return None };
        let types = types.clone();
        let mapped: Vec<_> = types.into_iter().map(|ty| replace_source(self, ty)).collect();
        let union = self.get_union_type(&mapped);
        if union == self.intrinsics.error {
            return None;
        }
        // mapTypeWithAlias retains the mapped alias and its arguments
        // on a distributed union, while exposing its constituents.
        Some(if let Some(alias) = alias {
            let alias_text = self.type_to_string(alias);
            self.union_with_origin_text(union, alias_text)
        } else {
            union
        })
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
        if self.store.get(source).flags.contains(TypeFlags::UNION) {
            return self.distribute_mapped_union(source, alias, &mut replace_source);
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
            let declaration = self.type_alias_declaration_of(symbol)?;
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

    /// getResolvedApparentTypeOfMappedType (checker.go:21772) for a mapped type
    /// that is not an alias reference: `instantiateType(target,
    /// prependTypeMapping(typeVariable, baseConstraint, t.mapper))`, where
    /// `target` is the instantiated mapped type ([`MappedTypeInfo::instance`])
    /// or the type itself. `Promise.allSettled(fn())` under
    /// `T extends readonly unknown[]` reads `map` from
    /// `PromiseSettledResult<unknown>[]`, not from the mapped members.
    // Read by `apparent_mapped_type` once the contextual rest-argument
    // hunk of `r6-mapped-apparent-instance.diff` lands (r6-mapped.md §1).
    #[allow(dead_code)]
    fn apparent_mapped_instance(
        &mut self,
        id: TypeId,
        info: &MappedTypeInfo,
        base: TypeId,
    ) -> Option<TypeId> {
        let (target, (mut map, mut parameters, names)) = match &info.instance {
            Some(instance) => (instance.target, instance.mapper.clone()),
            None => (id, (Vec::new(), Vec::new(), Vec::new())),
        };
        let target_info = self.mapped_types.get(&target)?;
        // getHomomorphicTypeVariable(target), and no `as` clause on the
        // declaration.
        if target_info.name_type.is_some() {
            return None;
        }
        let variable =
            self.deferred_keyof_operands.get(&target_info.constraint).copied().filter(|&ty| {
                self.store.get(ty).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
            })?;
        map.retain(|&(parameter, _)| parameter != variable);
        map.insert(0, (variable, base));
        if !parameters.contains(&variable) {
            parameters.push(variable);
        }
        let names: Vec<_> = names.iter().map(String::as_str).collect();
        Some(self.instantiate_type(target, &map, &parameters, &names))
    }
}

#[cfg(test)]
mod member_producer_tests {
    use crate::{Checker, flags::TypeFlags, types::TypeData};
    use tsr_ast::Statement;
    use tsr_core::Arena;

    /// instantiateAnonymousType's mapped arm (checker.go:22461):
    /// `MyMap<U>` iterates a clone of `P` constrained to `keyof U`, one clone
    /// per instantiation, while the declaration's `P` keeps `keyof T`
    /// (`mappedTypeParameterConstraint`, r5-mapped4.md §5).
    #[test]
    fn an_instance_iterates_a_clone_with_the_instantiated_constraint() {
        let source = "type MyMap<T> = {[P in keyof T]: T[keyof T]};
function foo<U>(m: MyMap<U>, n: MyMap<U>) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-iteration.ts", text: source },
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[1] else {
            panic!("function");
        };
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut clones = Vec::new();
        for parameter in function.parameters {
            let mapped = checker.get_type_from_type_node(parameter.r#type.unwrap());
            checker.ensure_mapped_type_info(mapped);
            let clone = checker.mapped_types[&mapped].parameter;
            let constraint = checker.type_parameter_constraint(clone).unwrap();
            assert_eq!(checker.type_to_string(constraint), "keyof U");
            assert!(checker.instantiated_type_parameters.contains_key(&clone));
            let target = checker.instantiated_type_parameters[&clone].target;
            let declared = checker.type_parameter_constraint(target).unwrap();
            assert_eq!(checker.type_to_string(declared), "keyof T");
            clones.push(clone);
        }
        assert_eq!(clones[0], clones[1]);
    }

    /// getIndexTypeForMappedType over a generic key domain (checker.go:26892):
    /// `Mapped5<K>`'s keys are `K` mapped through its filtering `as` clause,
    /// and `Mapped6<K>`'s through its remapping one. getIndexType itself
    /// defers both (shouldDeferIndexType), so `mapped_index_type` declines.
    #[test]
    fn a_generic_key_domain_maps_each_constituent_through_the_name_type() {
        let source = "type Mapped5<K extends string> = { [P in K as P extends `_${string}` ? P : never]: P };
type Mapped6<K extends string> = { [P in K as `_${P}`]: P };
function f<K extends string>(a: Mapped5<K>, b: Mapped6<K>) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-generic-keys.ts", text: source },
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[2] else {
            panic!("function");
        };
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut keys = Vec::new();
        for parameter in function.parameters {
            let mapped = checker.get_type_from_type_node(parameter.r#type.unwrap());
            assert_eq!(checker.mapped_index_type(mapped), None);
            let key = checker.index_type_for_generic_mapped_type(mapped).unwrap();
            // A deferred conditional key is read by its operands: its written
            // print is declared.rs' mint (`tsr-2zk.16.71`).
            let key = checker.mapped_conditionals.get(&key).map_or(key, |info| info.operands[0]);
            keys.push(checker.type_to_string(key));
        }
        assert_eq!(keys, ["K", "`_${K}`"]);
    }

    /// getTypeAliasInstantiation → instantiateMappedType: a concrete
    /// instance of a mapped alias is a mapped type, so its parts are captured
    /// on first ask (isMappedTypeGenericIndexedAccess reads them).
    #[test]
    fn a_concrete_mapped_alias_instance_keeps_its_mapped_identity() {
        let source = "type Part<T> = { [P in keyof T]?: T[P] };
type Foo1 = { x: number; y: string };
function f(o: Part<Foo1>) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-instance.ts", text: source },
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[2] else {
            panic!("function");
        };
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let instance = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
        assert_eq!(checker.type_to_string(instance), "Part<Foo1>");
        checker.ensure_mapped_type_info(instance);
        let info = checker.mapped_types[&instance].clone();
        assert_eq!(checker.type_to_string(info.constraint), "keyof Foo1");
        assert_eq!(info.optionality, Some(true));
        assert!(info.name_type.is_none());
    }

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
                    let (properties, complete) = checker.anonymous_properties[&mapped].clone();
                    assert!(complete);
                    // getTypeOfMappedSymbol: no slot is instantiated before
                    // its first read (ADR-0050).
                    assert!(
                        properties
                            .iter()
                            .all(|property| { checker.peek_property_type(property).is_none() })
                    );
                    assert_eq!((properties[0].optional, properties[0].readonly), flags[0]);
                    assert_eq!((properties[1].optional, properties[1].readonly), flags[1]);
                    assert_eq!(properties[0].origin, Some(a));
                    assert_eq!(properties[1].origin, Some(b));
                    assert!(properties.iter().all(|property| {
                        let ty = checker.property_type(property);
                        checker.type_of(ty).flags.contains(TypeFlags::INDEXED_ACCESS)
                    }));
                    let b_value = checker.property_type(&properties[1]);
                    assert_eq!(checker.peek_property_type(&properties[1]), Some(b_value));
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

    #[test]
    fn any_key_domains_resolve_to_index_signatures() {
        // getConstraintFromTypeParameter (checker.go:17085): an `any` key
        // constraint is string | number | symbol. An `any` modifiers type
        // contributes a string key (checker.go:22731).
        let source = "type Id<T> = { [K in keyof T]: T[K] };
function read(a: { [P in any]: number }, b: Id<any>) {}";
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapped-any-keys.ts", text: source },
        );
        let Statement::FunctionDeclaration(function) = parsed.source_file.statements[1] else {
            panic!("function");
        };
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.strict_null_checks = true;
        let a = checker.get_type_from_type_node(function.parameters[0].r#type.unwrap());
        let keys: Vec<_> = checker
            .get_index_infos_of_type(a)
            .unwrap()
            .iter()
            .map(|index| checker.type_to_string(index.key))
            .collect();
        assert_eq!(keys, ["string", "number", "symbol"]);
        let b = checker.get_type_from_type_node(function.parameters[1].r#type.unwrap());
        let indexes = checker.get_index_infos_of_type(b).unwrap_or_default();
        assert!(indexes.iter().all(|index| index.key == checker.intrinsics.string));
    }
}
