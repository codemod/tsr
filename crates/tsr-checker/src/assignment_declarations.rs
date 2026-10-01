//! Assignment declaration values, ported from checker.go:
//! `getWidenedTypeForAssignmentDeclaration`, `getAssignmentDeclarationInitializerType`
//! and `getTypeFromPropertyDescriptor`. The binder has already classified and
//! attached these declarations to their property symbols.

use tsr_ast::{Expression, Node, SyntaxKind};
use tsr_binder::SymbolId;

use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    /// `isReadonlyAssignmentDeclaration` (checker.go). A value descriptor is
    /// readonly unless its writable property exists and is not literal false;
    /// an accessor descriptor is readonly when it has no setter.
    pub(crate) fn assignment_declaration_is_readonly(&mut self, symbol: SymbolId) -> bool {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        for declaration in declarations {
            let Some(Node::CallExpression(call)) = self.node_map.get(declaration) else { continue };
            let Some(&descriptor) = call.arguments.get(2) else { continue };
            let descriptor = self.check_expression(descriptor);
            if descriptor == self.intrinsics.error {
                continue;
            }
            if self.get_type_of_property_of_type(descriptor, "value").is_some() {
                let Some(writable) = self.get_property_of_type(descriptor, "writable") else {
                    return true;
                };
                let writable = match self
                    .binder
                    .symbols()
                    .get(writable)
                    .value_declaration
                    .and_then(|id| self.node_map.get(id))
                {
                    Some(Node::PropertyAssignment(property)) => {
                        property.initializer.map(|initializer| self.check_expression(initializer))
                    }
                    _ => Some(self.get_type_of_symbol(writable)),
                };
                if writable.is_some_and(|t| {
                    matches!(self.store.get(t).data, TypeData::BooleanLiteral(false))
                }) {
                    return true;
                }
            } else if self.get_type_of_property_of_type(descriptor, "set").is_none() {
                return true;
            }
        }
        false
    }

    pub(crate) fn get_widened_type_for_assignment_declaration(
        &mut self,
        symbol: SymbolId,
    ) -> TypeId {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut types = Vec::new();
        for (index, &declaration) in declarations.iter().enumerate() {
            let Some(node) = self.node_map.get(declaration) else { continue };
            let assigned = match node {
                Node::BinaryExpression(binary) => {
                    if let Some(annotation) =
                        binary.r#type.or_else(|| self.jsdoc_cast_annotation(declaration))
                    {
                        return self.get_type_from_type_node(annotation);
                    }
                    let Some(left) = binary.left else { return self.intrinsics.error };
                    let Some(right) = binary.right else { return self.intrinsics.error };
                    let target = match left {
                        Expression::PropertyAccessExpression(access) => access.expression,
                        Expression::ElementAccessExpression(access) => access.expression,
                        _ => None,
                    };
                    // Constructor/method this-property declarations require the
                    // constructor flow and inherited-property precedence paths.
                    if matches!(target, Some(Expression::KeywordExpression(keyword)) if keyword.kind == SyntaxKind::ThisKeyword)
                    {
                        return self.intrinsics.error;
                    }
                    let exports = matches!(target, Some(Expression::Identifier(name)) if name.text == "exports")
                        || matches!(target, Some(Expression::PropertyAccessExpression(access))
                            if matches!(access.expression, Some(Expression::Identifier(name)) if name.text == "module")
                                && matches!(access.name, Some(tsr_ast::MemberName::Identifier(name)) if name.text == "exports"));
                    let assigned = if exports {
                        let mut rightmost = right;
                        while let Expression::BinaryExpression(assignment) = rightmost {
                            if assignment
                                .operator_token
                                .is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
                            {
                                break;
                            }
                            let Some(right) = assignment.right else {
                                return self.intrinsics.error;
                            };
                            rightmost = right;
                        }
                        let checked = self.check_expression(rightmost);
                        self.get_regular_type_of_literal_type(checked)
                    } else {
                        self.check_expression_for_mutable_location(right)
                    };
                    // CommonJS commonly starts an export with undefined and
                    // fills it later. Only its first undefined declaration is
                    // ignored, and only when another declaration exists.
                    if exports
                        && index == 0
                        && declarations.len() > 1
                        && assigned == self.intrinsics.undefined
                    {
                        continue;
                    }
                    if self.is_empty_array_literal_type(assigned)
                        && !self.has_parent_with_type_annotation(symbol)
                    {
                        let Some(array) = self.global_type_symbol("Array") else {
                            return self.intrinsics.error;
                        };
                        self.create_type_reference(array, vec![self.intrinsics.any])
                    } else {
                        assigned
                    }
                }
                Node::CallExpression(call) => {
                    let Some(&descriptor) = call.arguments.get(2) else {
                        return self.intrinsics.error;
                    };
                    self.get_type_from_property_descriptor(descriptor)
                }
                _ => continue,
            };
            if assigned == self.intrinsics.error {
                return assigned;
            }
            if !types.contains(&assigned) {
                types.push(assigned);
            }
        }
        let t = if types.is_empty() { self.intrinsics.any } else { self.get_union_type(&types) };
        // getWidenedType does not widen regular CommonJS literal types. Mutable
        // assignment and descriptor values have already widened at their own
        // expression boundary. JS all-nullable assignment inference is any,
        // including when strictNullChecks is enabled.
        let nullable_only = match &self.store.get(t).data {
            TypeData::Union { types, .. } => {
                types.iter().all(|&t| self.store.get(t).flags.intersects(TypeFlags::NULLABLE))
            }
            _ => self.store.get(t).flags.intersects(TypeFlags::NULLABLE),
        };
        if nullable_only
            && self
                .binder
                .symbols()
                .get(symbol)
                .value_declaration
                .is_some_and(|id| self.in_js_file(id))
        {
            return self.intrinsics.any;
        }
        t
    }

    /// `isEmptyArrayLiteralType` / `isEmptyLiteralType` (checker.go).
    pub(crate) fn is_empty_array_literal_type(&mut self, id: TypeId) -> bool {
        let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned() else {
            return false;
        };
        let Some(array) = self.global_type_symbol("Array") else { return false };
        self.binder.merged_symbol(target) == self.binder.merged_symbol(array)
            && arguments.len() == 1
            && self.is_empty_literal_type(arguments[0])
    }

    pub(crate) fn is_empty_literal_type(&self, id: TypeId) -> bool {
        id == if self.strict_null_checks {
            self.intrinsics.implicit_never
        } else {
            self.intrinsics.undefined_widening
        }
    }

    /// `hasParentWithTypeAnnotation` (checker.go): the function initializer's
    /// containing declaration supplies the annotation, not the expando itself.
    fn has_parent_with_type_annotation(&self, symbol: SymbolId) -> bool {
        let Some(parent) = self.binder.symbols().get(symbol).parent else { return false };
        let Some(declaration) = self.binder.symbols().get(parent).value_declaration else {
            return false;
        };
        if !matches!(
            self.node_map.get(declaration),
            Some(Node::FunctionExpression(_) | Node::ArrowFunction(_))
        ) {
            return false;
        }
        self.nodes
            .parent(declaration)
            .is_some_and(|parent| self.type_annotation_of(parent).is_some())
    }

    fn get_type_from_property_descriptor(&mut self, descriptor: Expression<'a>) -> TypeId {
        let descriptor = self.check_expression(descriptor);
        if descriptor == self.intrinsics.error {
            return descriptor;
        }
        if let Some(value) = self.get_type_of_property_of_type(descriptor, "value") {
            return value;
        }
        if let Some(getter) = self.get_type_of_property_of_type(descriptor, "get")
            && let Some(signature) = self.single_call_signature(getter)
        {
            return signature.r#type;
        }
        if let Some(setter) = self.get_type_of_property_of_type(descriptor, "set")
            && let Some(signature) = self.single_call_signature(setter)
        {
            return if signature.parameters.is_empty() {
                self.intrinsics.never
            } else {
                self.signature_type_at_position(&signature, 0).unwrap_or(self.intrinsics.error)
            };
        }
        self.intrinsics.any
    }
}
