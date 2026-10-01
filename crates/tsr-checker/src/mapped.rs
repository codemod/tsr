//! Semantic mapped type metadata, ported from internal/checker/checker.go.
use crate::{Checker, types::TypeId};
use tsr_ast::{Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

#[derive(Clone, Debug)]
pub(crate) struct MappedTypeInfo {
    pub(crate) parameter: TypeId,
    pub(crate) constraint: TypeId,
    pub(crate) template: TypeId,
    pub(crate) optionality: Option<bool>,
    pub(crate) readonly: Option<bool>,
    pub(crate) modifiers_source: Option<TypeId>,
}

impl<'a> Checker<'a, '_> {
    /// isGenericMappedType plus getHomomorphicTypeVariable for tuple context.
    pub(crate) fn is_generic_homomorphic_mapped_type(&self, id: TypeId) -> bool {
        self.mapped_types.get(&id).is_some_and(|mapped| {
            self.deferred_keyof_operands.get(&mapped.constraint).is_some_and(|operand| {
                self.store.get(*operand).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
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
        if node.name_type.is_some() {
            return;
        }
        let Some(parameter) = node.type_parameter else { return };
        let Some(symbol) = parameter.node_id.and_then(|id| self.binder.symbol_of(id)) else {
            return;
        };
        let parameter_type = self.get_declared_type_of_symbol(symbol);
        let Some(constraint) = parameter.constraint else { return };
        let Some(template) = node.r#type else { return };
        let mut modifiers_source = None;
        let constraint = if let TypeNode::TypeOperatorNode(operator) = constraint
            && operator.operator.kind == SyntaxKind::KeyOfKeyword
            && let Some(operand) = operator.r#type
        {
            let operand = self.get_type_from_type_node(operand);
            modifiers_source = Some(operand);
            self.resolved_keyof_type(operand).unwrap_or(self.intrinsics.error)
        } else {
            self.get_type_from_type_node(constraint)
        };
        self.mapped_template_depth += 1;
        let template = self.get_type_from_type_node(template);
        self.mapped_template_depth -= 1;

        if constraint == self.intrinsics.error || template == self.intrinsics.error {
            return;
        }
        self.mapped_types.insert(
            id,
            MappedTypeInfo {
                parameter: parameter_type,
                constraint,
                template,
                optionality: node.question_token.map(|token| token.kind != SyntaxKind::MinusToken),
                readonly: node.readonly_token.map(|token| token.kind != SyntaxKind::MinusToken),
                modifiers_source,
            },
        );
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
}
