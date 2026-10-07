//! Literal source views for const inference, following checkArrayLiteral,
//! checkObjectLiteral and getSpreadArgumentType in internal/checker/checker.go.
use crate::{
    Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};
use tsr_ast::{Expression, ObjectLiteralElementLike, PropertyName};

impl Checker<'_, '_> {
    /// The type-parameter/indexed/tuple base-constraint paths needed to decide
    /// mutable literal contexts (checker.go:27550 and :23526).
    fn const_literal_context_base(&mut self, id: TypeId, stack: &mut Vec<TypeId>) -> TypeId {
        if stack.contains(&id) {
            return id;
        }
        stack.push(id);
        let result = if let Some(constraint) = self.type_parameter_constraint(id) {
            self.const_literal_context_base(constraint, stack)
        } else if let Some((object, index, _)) =
            self.deferred_indexed_access_types.get(&id).copied()
        {
            let object = self.const_literal_context_base(object, stack);
            let index = self.const_literal_context_base(index, stack);
            self.resolved_indexed_access_type(object, index, false).unwrap_or(id)
        } else {
            self.tuple_base_constraint(id)
        };
        stack.pop();
        result
    }

    pub(crate) fn const_context_is_mutable_array_like(&mut self, id: TypeId) -> bool {
        if let TypeData::Union { types, .. } = self.store.get(id).data.clone() {
            return types.into_iter().any(|ty| self.const_context_is_mutable_array_like(ty));
        }
        let base = self.const_literal_context_base(id, &mut Vec::new());
        if self
            .store
            .get(base)
            .flags
            .intersects(TypeFlags::ANY | TypeFlags::NULL | TypeFlags::UNDEFINED)
        {
            return false;
        }
        if let Some((_, readonly)) = self.tuple_element_lists.get(&base) {
            return !readonly;
        }
        if let Some((_, readonly)) = self.variadic_tuple_elements.get(&base) {
            return !readonly;
        }
        if let Some((symbol, _)) = self.type_reference_targets.get(&base)
            && self.global_type_symbol("Array") == Some(*symbol)
        {
            return true;
        }
        let Some(array) = self.global_type_symbol("Array") else { return false };
        if let TypeData::Named { members: Some(owner), .. } = self.store.get(base).data
            && self.has_declared_array_base(owner, array, &mut Vec::new())
        {
            return true;
        }
        let array = self.create_type_reference(array, vec![self.intrinsics.any]);
        self.is_type_assignable_to(base, array)
    }

    /// Any instantiation of a declared array base is assignable to that array
    /// with an any element. Heritage arguments do not change this predicate.
    pub(crate) fn has_declared_array_base(
        &mut self,
        owner: tsr_binder::SymbolId,
        array: tsr_binder::SymbolId,
        visited: &mut Vec<tsr_binder::SymbolId>,
    ) -> bool {
        let owner = self.binder.merged_symbol(owner);
        if owner == self.binder.merged_symbol(array) {
            return true;
        }
        if visited.contains(&owner) {
            return false;
        }
        visited.push(owner);
        self.base_symbols_of_ex(owner, false).is_some_and(|bases| {
            bases.into_iter().any(|base| self.has_declared_array_base(base, array, visited))
        })
    }

    /// Preserve expression-cache views and mutable variable candidates. Only
    /// inline literal nodes acquire the readonly inference source view.
    pub(crate) fn const_literal_inference_source(
        &mut self,
        expression: Expression<'_>,
        source: TypeId,
        contextual: TypeId,
        in_const_context: bool,
    ) -> TypeId {
        let in_const_context = in_const_context || self.is_const_type_variable(contextual, 0);
        match expression {
            Expression::ParenthesizedExpression(node) => {
                node.expression.map_or(source, |expression| {
                    self.const_literal_inference_source(
                        expression,
                        source,
                        contextual,
                        in_const_context,
                    )
                })
            }
            Expression::ArrayLiteralExpression(array) => {
                let Some((elements, source_readonly)) =
                    self.tuple_element_lists.get(&source).cloned()
                else {
                    return source;
                };
                if array
                    .elements
                    .iter()
                    .any(|element| matches!(element, Expression::OmittedExpression(_)))
                {
                    return source;
                }
                let base = self.const_literal_context_base(contextual, &mut Vec::new());
                let mut images = elements.clone();
                let mut index = 0;
                for &expression in array.elements {
                    if let Expression::SpreadElement(spread) = expression {
                        let Some(expression) = spread.expression else { return source };
                        let spread_source = self.check_expression(expression);
                        let Some((spread_elements, _)) =
                            self.tuple_element_lists.get(&spread_source).cloned()
                        else {
                            return source;
                        };
                        let end = index + spread_elements.len();
                        if end > images.len() {
                            return source;
                        }
                        let spread_image = self.const_literal_inference_source(
                            expression,
                            spread_source,
                            contextual,
                            in_const_context,
                        );
                        if let Some((spread_images, _)) =
                            self.tuple_element_lists.get(&spread_image)
                            && spread_images.len() == spread_elements.len()
                        {
                            images[index..end].copy_from_slice(spread_images);
                        }
                        index = end;
                        continue;
                    }
                    let Some(&element) = elements.get(index) else { return source };
                    let key = self.store.intern(
                        TypeFlags::NUMBER_LITERAL,
                        TypeData::NumberLiteral(index.to_string()),
                    );
                    let target = self
                        .resolved_indexed_access_type(base, key, false)
                        .unwrap_or(self.intrinsics.unknown);
                    images[index] = self.const_literal_inference_source(
                        expression,
                        element,
                        target,
                        in_const_context,
                    );
                    index += 1;
                }
                if index != elements.len() {
                    return source;
                }
                let readonly = if in_const_context {
                    !self.const_context_is_mutable_array_like(contextual)
                } else {
                    source_readonly
                };
                if images == elements && readonly == source_readonly {
                    return source;
                }
                self.create_tuple_type(images, readonly)
            }
            Expression::ObjectLiteralExpression(object) => {
                let Some((mut properties, _)) = self.anonymous_properties.get(&source).cloned()
                else {
                    return source;
                };
                let base = self.const_literal_context_base(contextual, &mut Vec::new());
                let mut changed = false;
                for property in &mut properties {
                    // Last-write origins matter: a later spread or shorthand
                    // contributes an existing value, not an earlier literal's
                    // recursively readonly source view.
                    let initializer = object
                        .properties
                        .iter()
                        .rev()
                        .find_map(|element| {
                            let (name, initializer) = match element {
                                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                                    (assignment.name, assignment.initializer)
                                }
                                ObjectLiteralElementLike::ShorthandPropertyAssignment(
                                    assignment,
                                ) => (assignment.name, None),
                                ObjectLiteralElementLike::SpreadAssignment(spread) => {
                                    let expression = spread.expression?;
                                    let source = self.check_expression(expression);
                                    return self
                                        .get_type_of_property_of_type(source, &property.name)
                                        .map(|_| None);
                                }
                                _ => return None,
                            };
                            let name = match name {
                                PropertyName::Identifier(name) => name.text,
                                PropertyName::StringLiteral(name) => name.text,
                                PropertyName::NumericLiteral(name) => name.text,
                                _ => return None,
                            };
                            (name == property.name).then_some(initializer)
                        })
                        .flatten();
                    if let Some(initializer) = initializer {
                        let target = self
                            .get_type_of_property_of_type(base, &property.name)
                            .unwrap_or(self.intrinsics.unknown);
                        let current = self.property_type(property);
                        let image = self.const_literal_inference_source(
                            initializer,
                            current,
                            target,
                            in_const_context,
                        );
                        changed |= image != current;
                        property.slot = crate::objects::PropertySlot::resolved(image);
                        property.printed_slot =
                            crate::objects::PrintedSlot::printed(self.type_to_string(image));
                    }
                    changed |= in_const_context && !property.readonly;
                    property.readonly |= in_const_context;
                }
                if !changed {
                    return source;
                }
                let members: Vec<_> = properties
                    .iter()
                    .map(|property| crate::objects::Member::Property {
                        name: property.printed_name.clone(),
                        optional: property.optional,
                        readonly: property.readonly,
                        printed: self.property_printed_type(property).into_owned(),
                    })
                    .collect();
                let owner = match self.store.get(source).data {
                    TypeData::Named { members, .. } => members,
                    _ => None,
                };
                let image = self.store.new_named(
                    TypeFlags::OBJECT,
                    crate::objects::render_object_type(&members),
                    owner,
                );
                self.anonymous_properties.insert(image, (properties, true));
                image
            }
            _ => source,
        }
    }
}
