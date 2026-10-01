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
        let constraint = if let TypeNode::TypeOperatorNode(operator) = constraint
            && operator.operator.kind == SyntaxKind::KeyOfKeyword
            && let Some(operand) = operator.r#type
        {
            let operand = self.get_type_from_type_node(operand);
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
}
