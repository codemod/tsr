//! Binding-pattern implied types and parameter initializer padding, pinned to
//! typescript-go 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! getTypeFromBindingElement/Object/ArrayBindingPattern (17904-18029) and
//! padObjectLiteralType/padTupleType (16825-16890).
//!
//! No independent cache or context override is introduced. Declaration/symbol
//! resolution owns warm consumer reads; tuple construction retains its existing
//! semantic interning. Each object construction publishes a complete image only
//! after all values and indexes resolve, in this private Checker's store and
//! existing anonymous-property/index contracts. None is unsupported, not an empty
//! computed image. Source properties, aliases, indexes and readonly tuple flags
//! retain their identities. Direct object builders allocate just as native does.
//!
//! Cold work walks the pattern and checks context-independent defaults through
//! existing expression suppliers. Nested array literals reuse the existing
//! syntactic binding-pattern tuple context. Function-valued defaults and object
//! literals needing missing explicit pattern context decline before evaluation;
//! this is not a new contextual scheduling or inference API.

use tsr_ast::{
    BindingElement, BindingName, BindingPattern, Expression, NodeId, PropertyName, SyntaxKind,
};
use tsr_binder::SymbolId;

use crate::{
    Checker,
    flags::TypeFlags,
    index_signatures::IndexInfo,
    objects::AnonymousProperty,
    tuples::TupleElement,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    pub(crate) fn binding_pattern_implied_type(
        &mut self,
        pattern: &BindingPattern<'a>,
    ) -> Option<TypeId> {
        match self.nodes.kind(pattern.node_id?) {
            SyntaxKind::ObjectBindingPattern => self.object_pattern_implied_type(pattern),
            SyntaxKind::ArrayBindingPattern => self.array_pattern_implied_type(pattern),
            _ => None,
        }
    }

    pub(crate) fn object_pattern_implied_type(
        &mut self,
        pattern: &BindingPattern<'a>,
    ) -> Option<TypeId> {
        let mut properties: Vec<AnonymousProperty> = Vec::new();
        let mut indexes = Vec::new();
        for element in pattern.elements {
            if element.dot_dot_dot_token.is_some() {
                indexes = vec![IndexInfo {
                    components: None,
                    declaration: None,
                    key: self.intrinsics.string,
                    value: self.intrinsics.any,
                    readonly: false,
                }];
                continue;
            }
            let name = self.binding_pattern_property_name(element)?;
            let ty = self.binding_element_implied_type(element)?;
            let property = self.binding_pattern_property(name, ty, element.initializer.is_some());
            if let Some(index) = properties.iter().position(|held| held.name == property.name) {
                properties[index] = property;
            } else {
                properties.push(property);
            }
        }
        self.binding_pattern_object(properties, indexes, None)
    }

    fn array_pattern_implied_type(&mut self, pattern: &BindingPattern<'a>) -> Option<TypeId> {
        let rest = pattern.elements.last().filter(|element| element.dot_dot_dot_token.is_some());
        if pattern.elements.is_empty() || (pattern.elements.len() == 1 && rest.is_some()) {
            if let Some(iterable) = self.global_type_symbol_with_arity("Iterable", 3) {
                return Some(self.create_type_reference(
                    iterable,
                    vec![self.intrinsics.any, self.intrinsics.void, self.intrinsics.undefined],
                ));
            }
            // Retain the existing empty-pattern/no-lib boundary. Target-version
            // selection and a rest-only pattern without Iterable remain held.
            return pattern.elements.is_empty().then(|| self.create_tuple_type(Vec::new(), false));
        }
        let min_length = pattern
            .elements
            .iter()
            .rposition(|element| {
                element.name.is_some()
                    && element.initializer.is_none()
                    && element.dot_dot_dot_token.is_none()
            })
            .map_or(0, |index| index + 1);
        let mut elements = Vec::new();
        for (index, element) in pattern.elements.iter().enumerate() {
            let spread = element.dot_dot_dot_token.is_some();
            if spread
                && (index + 1 != pattern.elements.len()
                    || !matches!(element.name, Some(BindingName::Identifier(_))))
            {
                return None;
            }
            let ty = if element.name.is_some() {
                self.binding_element_implied_type(element)?
            } else {
                self.intrinsics.any
            };
            elements.push(TupleElement {
                r#type: ty,
                spread,
                optional: !spread && index >= min_length,
                label: None,
            });
        }
        let result = self.normalize_variadic_tuple(elements, false);
        (result != self.intrinsics.error).then_some(result)
    }

    fn binding_element_implied_type(&mut self, element: &BindingElement<'a>) -> Option<TypeId> {
        if let Some(initializer) = element.initializer {
            if !binding_default_is_context_independent(initializer) {
                return None;
            }
            if let Some(BindingName::BindingPattern(pattern)) = element.name
                && !self.binding_default_pattern_context_available(pattern, initializer)
            {
                return None;
            }
            let declaration = element.node_id?;
            let checked = self.check_expression(initializer);
            if checked == self.intrinsics.error {
                return None;
            }
            let checked = if let Some(BindingName::BindingPattern(pattern)) = element.name {
                self.pad_binding_pattern_initializer(declaration, checked, pattern)?
            } else {
                checked
            };
            let widened = self.get_widened_literal_type_for_initializer(declaration, checked);
            let widened = self.widen_object_literal_freshness(widened);
            return Some(if self.strict_null_checks {
                self.get_optional_type(widened, false)
            } else {
                widened
            });
        }
        match element.name? {
            BindingName::BindingPattern(pattern) => self.binding_pattern_implied_type(pattern),
            BindingName::Identifier(_) => Some(self.intrinsics.any),
        }
    }

    pub(crate) fn check_binding_pattern_default(
        &mut self,
        declaration: NodeId,
        initializer: Expression<'a>,
        pattern: &BindingPattern<'a>,
    ) -> Option<TypeId> {
        if !binding_default_is_context_independent(initializer)
            || !self.binding_default_pattern_context_available(pattern, initializer)
        {
            return None;
        }
        let checked = self.check_expression(initializer);
        if checked == self.intrinsics.error {
            return None;
        }
        self.pad_binding_pattern_initializer(declaration, checked, pattern)
    }

    /// Both parameter consumers enter after their existing annotation/context
    /// admission. Padding reads the checked initializer before widening removes
    /// `ObjectLiteral` metadata; a no-op retains their computed type identity.
    pub(crate) fn pad_binding_parameter_type(
        &mut self,
        declaration: NodeId,
        computed: TypeId,
        pattern: &BindingPattern<'a>,
    ) -> Option<TypeId> {
        let initializer = self.initializer_of(declaration)?;
        let source = self.check_expression(initializer);
        if source == self.intrinsics.error {
            return None;
        }
        let padded = self.pad_binding_pattern_initializer(declaration, source, pattern)?;
        if padded == source {
            return Some(computed);
        }
        let widened = self.widen_type_inferred_from_initializer(declaration, padded);
        Some(self.widen_object_literal_freshness(widened))
    }

    pub(crate) fn pad_binding_pattern_initializer(
        &mut self,
        declaration: NodeId,
        source: TypeId,
        pattern: &BindingPattern<'a>,
    ) -> Option<TypeId> {
        if self.nodes.kind(self.root_declaration_of(declaration)) != SyntaxKind::Parameter {
            return Some(source);
        }
        match self.nodes.kind(pattern.node_id?) {
            SyntaxKind::ObjectBindingPattern if self.is_object_literal_type(source) => {
                let mut missing = Vec::new();
                for element in
                    pattern.elements.iter().filter(|element| element.initializer.is_some())
                {
                    let name = self.binding_pattern_property_name(element)?;
                    if self.get_type_of_property_of_type(source, &name).is_none() {
                        missing.push((name, element));
                    }
                }
                if missing.is_empty() {
                    return Some(source);
                }
                let (mut properties, _) = self.spread_properties(source, false)?;
                let indexes = self.get_index_infos_of_type(source)?;
                for (name, element) in missing {
                    let ty = self.binding_element_implied_type(element)?;
                    properties.push(self.binding_pattern_property(name, ty, true));
                }
                let owner = match self.store.get(source).data {
                    TypeData::Named { members, .. } => members,
                    _ => None,
                };
                let result = self.binding_pattern_object(properties, indexes, owner)?;
                if self.fresh_object_literal_types.contains(&source) {
                    self.fresh_object_literal_types.insert(result);
                }
                if let Some(&spread) = self.object_literal_spread_flags.get(&source) {
                    self.object_literal_spread_flags.insert(result, spread);
                }
                Some(result)
            }
            SyntaxKind::ArrayBindingPattern => {
                if self.variadic_tuple_elements.contains_key(&source) {
                    return Some(source);
                }
                let Some((types, readonly)) = self.tuple_element_lists.get(&source).cloned() else {
                    return Some(source);
                };
                if types.len() >= pattern.elements.len() {
                    return Some(source);
                }
                let mask = self.tuple_optional_masks.get(&source);
                let labels = self.tuple_labels.get(&source);
                let mut elements = types
                    .into_iter()
                    .enumerate()
                    .map(|(index, ty)| TupleElement {
                        r#type: ty,
                        spread: false,
                        optional: mask.and_then(|mask| mask.get(index)).copied().unwrap_or(false),
                        label: labels.and_then(|labels| labels.get(index)).cloned().flatten(),
                    })
                    .collect::<Vec<_>>();
                for (index, element) in pattern.elements.iter().enumerate().skip(elements.len()) {
                    if index + 1 == pattern.elements.len() && element.dot_dot_dot_token.is_some() {
                        continue;
                    }
                    let ty = if element.name.is_some() && element.initializer.is_some() {
                        self.binding_element_implied_type(element)?
                    } else {
                        self.intrinsics.any
                    };
                    elements.push(TupleElement {
                        r#type: ty,
                        spread: false,
                        optional: true,
                        label: None,
                    });
                }
                let result = self.normalize_variadic_tuple(elements, readonly);
                (result != self.intrinsics.error).then_some(result)
            }
            _ => Some(source),
        }
    }

    fn binding_pattern_property_name(&mut self, element: &BindingElement<'a>) -> Option<String> {
        match element.property_name {
            Some(PropertyName::Identifier(name)) => Some(name.text.to_owned()),
            Some(PropertyName::StringLiteral(name)) => Some(name.text.to_owned()),
            Some(PropertyName::NumericLiteral(name)) => {
                Some(crate::printing::normalise_number(name.text))
            }
            Some(PropertyName::ComputedPropertyName(name)) => {
                let expression = name.expression?;
                if !binding_default_is_context_independent(expression) {
                    return None;
                }
                let ty = self.check_expression(expression);
                self.property_name_from_index(ty)
            }
            None => match element.name? {
                BindingName::Identifier(name) => Some(name.text.to_owned()),
                BindingName::BindingPattern(_) => None,
            },
            _ => None,
        }
    }

    /// Certify only default shapes whose value suppliers need no explicit
    /// pattern context. The object-literal optional-property supplier consumes
    /// this same prerequisite, independently of contextual scheduling.
    pub(crate) fn binding_default_pattern_context_available(
        &self,
        pattern: &BindingPattern<'a>,
        initializer: Expression<'a>,
    ) -> bool {
        match initializer {
            Expression::ParenthesizedExpression(node) => node.expression.is_some_and(|expression| {
                // The existing syntactic pattern suppliers do not walk
                // parentheses. An explicit pattern context is needed there.
                !matches!(expression, Expression::ArrayLiteralExpression(_) | Expression::ObjectLiteralExpression(_))
                    && self.binding_default_pattern_context_available(pattern, expression)
            }),
            Expression::ArrayLiteralExpression(literal) if pattern.node_id.is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ArrayBindingPattern) => {
                !pattern.elements.iter().any(|element| element.dot_dot_dot_token.is_some())
                    // Tuple shape is available, per-element pattern context is
                    // not. Missing slots still consume the native padding road.
                    && literal.elements.iter().enumerate().all(|(index, _)| {
                        pattern.elements.get(index).is_none_or(|element| {
                            element.initializer.is_none()
                                && !matches!(element.name, Some(BindingName::BindingPattern(_)))
                        })
                    })
            },
            Expression::ObjectLiteralExpression(literal) if pattern.node_id.is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ObjectBindingPattern) => literal.properties.iter().all(|property| {
                let tsr_ast::ObjectLiteralElementLike::PropertyAssignment(property) = property else { return false };
                let name = match property.name { PropertyName::Identifier(name) => name.text, PropertyName::StringLiteral(name) => name.text, _ => return false };
                !pattern.elements.iter().any(|element| {
                    // A computed key might name this present property. Without
                    // explicit context, do not assume it names an absent slot.
                    let matches = match element.property_name { Some(PropertyName::Identifier(key)) => key.text == name, Some(PropertyName::StringLiteral(key)) => key.text == name, Some(PropertyName::ComputedPropertyName(_)) => true, None => matches!(element.name, Some(BindingName::Identifier(key)) if key.text == name), _ => false };
                    matches && (matches!(element.name, Some(BindingName::BindingPattern(_)))
                        || element.initializer.is_some_and(|default| {
                            matches!(element.property_name, Some(PropertyName::ComputedPropertyName(_)))
                                || !primitive_binding_literal(default)
                                || !property.initializer.is_some_and(primitive_binding_literal)
                        }))
                })
            }),
            _ => true,
        }
    }

    fn binding_pattern_property(
        &mut self,
        name: String,
        ty: TypeId,
        optional: bool,
    ) -> AnonymousProperty {
        let printed_name = if crate::symbols::is_identifier_text(&name)
            || name.parse::<u64>().is_ok_and(|number| number.to_string() == name)
        {
            name.clone()
        } else {
            crate::printing::quote_ascii(&name)
        };
        AnonymousProperty {
            accessor_write: None,
            method: false,
            origin: None,
            checked_declaration: None,
            printed_name,
            name,
            printed_type: self.type_to_string(ty),
            optional,
            readonly: false,
            r#type: ty,
        }
    }

    fn binding_pattern_object(
        &mut self,
        mut properties: Vec<AnonymousProperty>,
        indexes: Vec<IndexInfo>,
        owner: Option<SymbolId>,
    ) -> Option<TypeId> {
        // Native synthetic binding properties have no declarations; ordinary
        // source properties retain their declaration order ahead of them.
        properties.sort_by(|left, right| match (left.origin, right.origin) {
            (Some(left), Some(right)) => self.compare_symbols(left, right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => left.name.cmp(&right.name),
        });
        let mut members = Vec::new();
        for index in &indexes {
            members.extend(self.index_info_members(index)?);
        }
        members.extend(crate::callable_expandos::property_members(&properties));
        let ty = self.store.new_named(
            TypeFlags::OBJECT,
            crate::objects::render_object_type(&members),
            owner,
        );
        self.anonymous_properties.insert(ty, (properties, true));
        self.object_literal_index_infos.insert(ty, indexes);
        self.object_literal_members.insert(ty, members);
        self.object_literal_spread_flags.insert(ty, false);
        Some(ty)
    }
}

fn binding_default_is_context_independent(expression: Expression<'_>) -> bool {
    match expression {
        Expression::Identifier(_) | Expression::StringLiteral(_) | Expression::NumericLiteral(_) | Expression::BigIntLiteral(_) | Expression::NoSubstitutionTemplateLiteral(_) => true,
        Expression::KeywordExpression(keyword) => matches!(keyword.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword | SyntaxKind::NullKeyword),
        Expression::ParenthesizedExpression(node) => node.expression.is_some_and(binding_default_is_context_independent),
        Expression::PrefixUnaryExpression(node) => node.operand.is_some_and(binding_default_is_context_independent),
        Expression::PropertyAccessExpression(node) => node.expression.is_some_and(binding_default_is_context_independent),
        Expression::CallExpression(node) => node.expression.is_some_and(binding_default_is_context_independent) && node.arguments.iter().copied().all(binding_default_is_context_independent),
        Expression::YieldExpression(node) => node.expression.is_none_or(binding_default_is_context_independent),
        Expression::ArrayLiteralExpression(node) => node.elements.iter().copied().all(binding_default_is_context_independent),
        Expression::ObjectLiteralExpression(node) => node.properties.iter().all(|property| matches!(property, tsr_ast::ObjectLiteralElementLike::PropertyAssignment(property) if property.initializer.is_some_and(binding_default_is_context_independent))),
        _ => false,
    }
}

fn primitive_binding_literal(expression: Expression<'_>) -> bool {
    match expression {
        Expression::StringLiteral(_) | Expression::NumericLiteral(_) => true,
        Expression::KeywordExpression(keyword) => {
            matches!(keyword.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
        }
        Expression::ParenthesizedExpression(node) => {
            node.expression.is_some_and(primitive_binding_literal)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_ast::Node;
    use tsr_core::{Arena, CompilerOptions, Tristate};

    const SOURCE: &str = include_str!("../tests/binding_pattern_controls.ts");

    fn with_checker(strict: bool, test: impl FnOnce(&mut Checker<'_, '_>, Vec<(String, NodeId)>)) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, SOURCE);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "control.ts", text: SOURCE },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.apply_compiler_options(&CompilerOptions {
            strict: if strict { Tristate::True } else { Tristate::False },
            ..Default::default()
        });
        let queries = (0..parsed.nodes.len())
            .filter_map(|index| {
                let id = NodeId::new(u32::try_from(index).unwrap());
                let name = match parsed.node_map.get(id)? {
                    Node::FunctionDeclaration(node) => node.name?.text,
                    Node::BindingElement(element) => match element.name? {
                        BindingName::Identifier(name) => name.text,
                        BindingName::BindingPattern(_) => return None,
                    },
                    _ => return None,
                };
                Some((name.to_owned(), id))
            })
            .collect();
        test(&mut checker, queries);
    }

    fn type_of(c: &mut Checker<'_, '_>, queries: &[(String, NodeId)], name: &str) -> TypeId {
        let id = queries.iter().find(|(held, _)| held == name).unwrap().1;
        c.get_type_of_symbol(c.binder.symbol_of(id).unwrap())
    }

    fn parameter(c: &Checker<'_, '_>, queries: &[(String, NodeId)], name: &str) -> NodeId {
        let id = queries.iter().find(|(held, _)| held == name).unwrap().1;
        let Node::FunctionDeclaration(function) = c.node_map.get(id).unwrap() else {
            panic!("function")
        };
        function.parameters[0].node_id.unwrap()
    }

    #[test]
    fn native_default_projections_keep_semantic_suppliers_and_admission() {
        for strict in [false, true] {
            with_checker(strict, |c, queries| {
                for (name, expected) in [
                    ("word", "string"),
                    ("amount", "number"),
                    ("shade", "Hue"),
                    ("first", "string"),
                    ("required", "any"),
                    ("trailing", "number"),
                    ("middle", "boolean"),
                    ("last", "any"),
                    ("child", "string"),
                    ("left", "number"),
                    ("right", "any"),
                    ("head", "number"),
                    ("objectTail", "{ [x: string]: any; }"),
                    ("start", "number"),
                    ("arrayTail", "any[]"),
                    ("named", "boolean"),
                    ("numeric", "string"),
                    ("position", "string"),
                    ("padded", "number"),
                    ("added", "string"),
                    ("count", "number"),
                    ("kept", "string"),
                    ("extra", "number"),
                    ("keptTuple", "number"),
                    ("extraTuple", "string"),
                    ("absent", "any"),
                    ("single", "number"),
                    ("annotatedValue", "1 | 2"),
                    ("fixed", "string"),
                    ("source", "any"),
                    ("copied", "any"),
                    ("yieldedDefault", "any"),
                    ("presentPrimitive", "number"),
                    ("copiedPrimitive", "number"),
                ] {
                    let ty = type_of(c, &queries, name);
                    assert_eq!(c.type_to_string(ty), expected, "strict={strict}: {name}");
                }
            });
        }
    }

    #[test]
    fn native_tuple_flags_and_nested_padding_are_asymmetric() {
        for strict in [false, true] {
            with_checker(strict, |c, queries| {
                for (name, expected) in [
                    (
                        "objectDefaults",
                        if strict {
                            "{ amount?: number | undefined; shade?: Hue | undefined; word?: string | undefined; }"
                        } else {
                            "{ amount?: number; shade?: Hue; word?: string; }"
                        },
                    ),
                    (
                        "arrayDefaults",
                        if strict {
                            "[string | undefined, any, (number | undefined)?]"
                        } else {
                            "[string, any, number?]"
                        },
                    ),
                    (
                        "omitted",
                        if strict {
                            "[any, boolean | undefined, any]"
                        } else {
                            "[any, boolean, any]"
                        },
                    ),
                    (
                        "arrayRest",
                        if strict {
                            "[(number | undefined)?, ...any[]]"
                        } else {
                            "[number?, ...any[]]"
                        },
                    ),
                    (
                        "nestedArrayDefault",
                        if strict {
                            "{ row?: [string, (number | undefined)?] | undefined; }"
                        } else {
                            "{ row?: [string, number?]; }"
                        },
                    ),
                ] {
                    let parameter = parameter(c, &queries, name);
                    let ty = c.get_widened_type_for_variable_like_declaration(parameter);
                    assert_eq!(c.type_to_string(ty), expected, "strict={strict}: {name}");
                }
                let parameter = parameter(c, &queries, "rootTupleDefault");
                let Node::ParameterDeclaration(node) = c.node_map.get(parameter).unwrap() else {
                    panic!("parameter")
                };
                let Some(BindingName::BindingPattern(pattern)) = node.name else {
                    panic!("pattern")
                };
                let source = c.check_expression(node.initializer.unwrap());
                let readonly = c.create_tuple_type(vec![c.intrinsics.number], true);
                let padded =
                    c.pad_binding_pattern_initializer(parameter, readonly, pattern).unwrap();
                assert!(c.tuple_element_lists[&padded].1);
                assert_eq!(c.tuple_optional_masks[&padded], [false, true, true, true]);
                assert_eq!(c.tuple_element_lists[&source].0.len(), 1, "padding mutated its source");
                let longer = c.create_tuple_type(vec![c.intrinsics.number; 5], false);
                assert_eq!(
                    c.pad_binding_pattern_initializer(parameter, longer, pattern),
                    Some(longer)
                );
            });
        }
    }

    #[test]
    fn literal_optionality_suppliers_preserve_annotation_precedence() {
        for strict in [false, true] {
            with_checker(strict, |c, queries| {
                for (name, property, expected_optional) in [
                    ("primitiveSource", "copiedPrimitive", true),
                    ("annotatedSource", "stamped", false),
                ] {
                    let parameter = parameter(c, &queries, name);
                    let initializer = c.initializer_of(parameter).unwrap();
                    let checked = c.check_expression(initializer);
                    let properties = &c.anonymous_properties[&checked].0;
                    assert_eq!(
                        properties.iter().find(|held| held.name == property).unwrap().optional,
                        expected_optional,
                        "strict={strict}: {name}"
                    );
                    assert_eq!(
                        c.check_expression(initializer),
                        checked,
                        "literal owner was rebuilt"
                    );
                }
                let present_parameter = parameter(c, &queries, "presentObject");
                let Node::ParameterDeclaration(node) = c.node_map.get(present_parameter).unwrap()
                else {
                    panic!("parameter")
                };
                let Some(BindingName::BindingPattern(pattern)) = node.name else {
                    panic!("pattern")
                };
                let initializer = pattern.elements[0].initializer.unwrap();
                let checked = c.check_expression(initializer);
                assert!(c.anonymous_properties[&checked].0[0].optional);
                assert_eq!(c.check_expression(initializer), checked);
                for name in ["annotatedNested", "initializedNested"] {
                    let parameter = parameter(c, &queries, name);
                    let Node::ParameterDeclaration(node) = c.node_map.get(parameter).unwrap()
                    else {
                        panic!("parameter")
                    };
                    let Some(BindingName::BindingPattern(pattern)) = node.name else {
                        panic!("pattern")
                    };
                    let initializer = pattern.elements[0].initializer.unwrap();
                    let checked = c.check_expression(initializer);
                    assert!(
                        c.anonymous_properties[&checked]
                            .0
                            .iter()
                            .all(|property| !property.optional),
                        "a present parent source was replaced by the implied pattern: {name}"
                    );
                }
                for index in 0..c.nodes.len() {
                    let id = NodeId::new(u32::try_from(index).unwrap());
                    let Some(Node::ObjectLiteralExpression(literal)) = c.node_map.get(id) else {
                        continue;
                    };
                    let Some(tsr_ast::ObjectLiteralElementLike::PropertyAssignment(property)) =
                        literal.properties.first()
                    else {
                        continue;
                    };
                    if !matches!(property.name, PropertyName::Identifier(name) if name.text == "argumentDefault")
                    {
                        continue;
                    }
                    let checked = c.check_expression(Expression::ObjectLiteralExpression(literal));
                    assert!(!c.anonymous_properties[&checked].0[0].optional);
                }
            });
        }
    }

    #[test]
    fn warm_and_reversed_consumer_queries_keep_owner_identity() {
        for strict in [false, true] {
            let mut images = Vec::new();
            for reverse in [false, true] {
                with_checker(strict, |c, queries| {
                    let mut order = queries.clone();
                    if reverse {
                        order.reverse();
                    }
                    for (_, id) in &order {
                        c.get_type_of_symbol(c.binder.symbol_of(*id).unwrap());
                    }
                    let cold = queries
                        .iter()
                        .map(|(_, id)| c.get_type_of_symbol(c.binder.symbol_of(*id).unwrap()))
                        .collect::<Vec<_>>();
                    let count = c.store.len();
                    for ((_, id), ty) in queries.iter().zip(&cold) {
                        assert_eq!(c.get_type_of_symbol(c.binder.symbol_of(*id).unwrap()), *ty);
                    }
                    assert_eq!(c.store.len(), count, "warm consumer rebuilt a type");
                    images
                        .push(cold.into_iter().map(|ty| c.type_to_string(ty)).collect::<Vec<_>>());
                });
            }
            assert_eq!(images[0], images[1], "query order changed the native image");
        }
    }

    #[test]
    fn unsupported_explicit_context_is_not_a_completed_empty_image() {
        with_checker(true, |c, queries| {
            for name in [
                "unsupported",
                "unsupportedObject",
                "unsupportedArray",
                "unsupportedComputed",
                "unsupportedParenthesized",
                "unsupportedParenthesizedArray",
            ] {
                let parameter = parameter(c, &queries, name);
                let Node::ParameterDeclaration(node) = c.node_map.get(parameter).unwrap() else {
                    panic!("parameter")
                };
                let Some(BindingName::BindingPattern(pattern)) = node.name else {
                    panic!("pattern")
                };
                let images = c.anonymous_properties.len();
                assert_eq!(c.binding_pattern_implied_type(pattern), None);
                assert_eq!(
                    c.anonymous_properties.len(),
                    images,
                    "unsupported work published members"
                );
                assert_eq!(
                    c.get_widened_type_for_variable_like_declaration(parameter),
                    c.intrinsics.error
                );
            }
        });
    }
}
