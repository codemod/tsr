//! Decorator contextual types, pinned to typescript-go
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb checker.go:29801,30155-30562.
//!
//! `getContextualTypeForDecorator` consumes `signatureLinks[decorated node]`,
//! not expression links. This private Checker owns the cache for its lifetime
//! and fixed compiler options. Absent is uncomputed; Active is native's
//! anySignature sentinel; Complete(None) is native's completed nil for an
//! invalid target; Unsupported is an unresolved port prerequisite, never a
//! completed signature. Only the worker publishes completion. Cold resolution
//! forces class/member types and global templates through their existing owners;
//! warm reads do no declaration walk or copy of a signature.
//!
//! The override identity is (regular name `TypeId`, private, static), within this
//! Checker's type store. Global references retain ordered arguments and template
//! symbols through `create_type_reference`. Static/instance receiver identities
//! come from the decorated declaration, not the decorator expression's receiver;
//! no alias or mapper is reconstructed from rendered text. This is a correctness
//! port, not a speed claim. Cold/warm/reversed controls below check reuse bounds.

use rustc_hash::FxHashMap;
use tsr_ast::{BindingName, ModifierLike, Node, NodeId, PropertyName, SyntaxKind};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    objects::AnonymousProperty,
    signatures::{Parameter, Signature, SignatureKind},
    types::{TypeData, TypeId},
};

#[derive(Clone, Copy)]
enum DecoratorType {
    Active,
    Complete(Option<TypeId>),
    Unsupported,
}

#[derive(Default)]
pub(crate) struct DecoratorTypes {
    signatures: FxHashMap<NodeId, DecoratorType>,
    overrides: FxHashMap<(TypeId, bool, bool), TypeId>,
    #[cfg(test)]
    workers: usize,
}

impl<'a> Checker<'a, '_> {
    pub(crate) fn contextual_type_for_decorator(&mut self, decorator: NodeId) -> Option<TypeId> {
        let declaration = self.nodes.parent(decorator)?;
        if let Some(cached) = self.decorator_types.signatures.get(&declaration) {
            return match cached {
                DecoratorType::Complete(result) => *result,
                DecoratorType::Active | DecoratorType::Unsupported => None,
            };
        }
        self.decorator_types.signatures.insert(declaration, DecoratorType::Active);
        #[cfg(test)]
        {
            self.decorator_types.workers += 1;
        }
        let signature = if self.legacy_decorators {
            self.legacy_decorator_call_signature(declaration)
        } else {
            self.es_decorator_call_signature(declaration)
        };
        let result = match signature {
            Ok(signature) => DecoratorType::Complete(
                signature.map(|signature| self.decorator_function_type(signature)),
            ),
            Err(()) => DecoratorType::Unsupported,
        };
        self.decorator_types.signatures.insert(declaration, result);
        match result {
            DecoratorType::Complete(result) => result,
            DecoratorType::Active | DecoratorType::Unsupported => None,
        }
    }

    fn decorator_function_type(&mut self, signature: Signature) -> TypeId {
        let text = self.signature_to_string(&signature);
        let ty = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.signature_types.insert(ty, vec![signature]);
        ty
    }

    fn decorator_global_type(&mut self, name: &str, arguments: Vec<TypeId>) -> TypeId {
        match self.global_type_symbol_with_arity(name, arguments.len()) {
            Some(symbol) => self.create_type_reference(symbol, arguments),
            // tryCreateTypeReference(emptyGenericType) returns unknown;
            // createTypeFromGenericGlobalType returns emptyObjectType instead.
            None if name == "TypedPropertyDescriptor" => self.intrinsics.empty_object,
            None => self.intrinsics.unknown,
        }
    }

    fn decorator_declaration_type(&mut self, declaration: NodeId) -> Result<TypeId, ()> {
        let symbol = self.binder.symbol_of(declaration).ok_or(())?;
        let ty = self.get_type_of_symbol(symbol);
        (ty != self.intrinsics.error).then_some(ty).ok_or(())
    }

    fn decorator_member_parts(
        &self,
        declaration: NodeId,
    ) -> Option<(PropertyName<'a>, &'a [ModifierLike<'a>])> {
        match self.node_map.get(declaration)? {
            Node::MethodDeclaration(node) => Some((node.name, node.modifiers)),
            Node::GetAccessorDeclaration(node) => Some((node.name, node.modifiers)),
            Node::SetAccessorDeclaration(node) => Some((node.name, node.modifiers)),
            Node::PropertyDeclaration(node) => Some((node.name, node.modifiers)),
            _ => None,
        }
    }

    fn decorator_class_side(&mut self, member: NodeId) -> Result<TypeId, ()> {
        let class = self.nodes.parent(member).ok_or(())?;
        let symbol = self.binder.symbol_of(class).ok_or(())?;
        let (_, modifiers) = self.decorator_member_parts(member).ok_or(())?;
        let ty = if crate::check::has_modifier(modifiers, SyntaxKind::StaticKeyword) {
            self.get_type_of_symbol(symbol)
        } else {
            self.get_declared_type_of_symbol(symbol)
        };
        (ty != self.intrinsics.error).then_some(ty).ok_or(())
    }

    fn is_decorator_class_member(&self, declaration: NodeId) -> bool {
        self.nodes.parent(declaration).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
            )
        })
    }

    fn legacy_decorator_call_signature(
        &mut self,
        declaration: NodeId,
    ) -> Result<Option<Signature>, ()> {
        let kind = self.nodes.kind(declaration);
        let mut parameters = Vec::new();
        let returned = match kind {
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                let target = self.decorator_declaration_type(declaration)?;
                parameters.push(decorator_parameter("target", target));
                target
            }
            SyntaxKind::Parameter => {
                let parent = self.nodes.parent(declaration).ok_or(())?;
                let (constructor, declarations) = match self.node_map.get(parent) {
                    Some(Node::ConstructorDeclaration(node)) => (true, node.parameters),
                    Some(Node::MethodDeclaration(node)) => (false, node.parameters),
                    Some(Node::SetAccessorDeclaration(node))
                        if self.is_decorator_class_member(parent) =>
                    {
                        (false, node.parameters)
                    }
                    _ => return Ok(None),
                };
                let has_this = declarations.first().is_some_and(|parameter| {
                    matches!(parameter.name, Some(BindingName::Identifier(name)) if name.text == "this")
                });
                let index = declarations
                    .iter()
                    .position(|parameter| parameter.node_id == Some(declaration))
                    .ok_or(())?;
                if has_this && index == 0 {
                    return Ok(None);
                }
                let (target, key) = if constructor {
                    (
                        self.decorator_declaration_type(self.nodes.parent(parent).ok_or(())?)?,
                        self.intrinsics.undefined,
                    )
                } else {
                    (self.decorator_class_side(parent)?, self.legacy_decorator_key(parent)?)
                };
                let index = self.store.intern_literal(
                    TypeFlags::NUMBER_LITERAL,
                    TypeData::NumberLiteral((index - usize::from(has_this)).to_string()),
                    false,
                );
                parameters.extend([
                    decorator_parameter("target", target),
                    decorator_parameter("propertyKey", key),
                    decorator_parameter("parameterIndex", index),
                ]);
                self.intrinsics.void
            }
            SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::PropertyDeclaration
                if self.is_decorator_class_member(declaration) =>
            {
                let target = self.decorator_class_side(declaration)?;
                let key = self.legacy_decorator_key(declaration)?;
                parameters.extend([
                    decorator_parameter("target", target),
                    decorator_parameter("propertyKey", key),
                ]);
                let (_, modifiers) = self.decorator_member_parts(declaration).ok_or(())?;
                if kind != SyntaxKind::PropertyDeclaration
                    || crate::check::has_modifier(modifiers, SyntaxKind::AccessorKeyword)
                {
                    let value = self.decorator_declaration_type(declaration)?;
                    let descriptor =
                        self.decorator_global_type("TypedPropertyDescriptor", vec![value]);
                    parameters.push(decorator_parameter("descriptor", descriptor));
                    if kind == SyntaxKind::PropertyDeclaration {
                        self.intrinsics.void
                    } else {
                        descriptor
                    }
                } else {
                    self.intrinsics.void
                }
            }
            _ => return Ok(None),
        };
        let returned = self.get_union_type(&[returned, self.intrinsics.void]);
        Ok(Some(decorator_signature(self.new_signature_id(), declaration, None, parameters, returned)))
    }

    fn legacy_decorator_key(&mut self, declaration: NodeId) -> Result<TypeId, ()> {
        let (name, _) = self.decorator_member_parts(declaration).ok_or(())?;
        match name {
            PropertyName::Identifier(name) => Ok(self.decorator_string_literal(name.text)),
            PropertyName::StringLiteral(name) => Ok(self.decorator_string_literal(name.text)),
            PropertyName::NumericLiteral(name) => Ok(self.decorator_string_literal(name.text)),
            PropertyName::ComputedPropertyName(name) => {
                let ty = self.check_expression(name.expression.ok_or(())?);
                if ty == self.intrinsics.error {
                    return Err(());
                }
                Ok(
                    if self
                        .type_of(ty)
                        .flags
                        .intersects(TypeFlags::ES_SYMBOL.union(TypeFlags::UNIQUE_ES_SYMBOL))
                        || self.is_type_assignable_to(ty, self.intrinsics.es_symbol)
                    {
                        ty
                    } else {
                        self.intrinsics.string
                    },
                )
            }
            _ => Ok(self.intrinsics.error),
        }
    }

    fn es_decorator_call_signature(
        &mut self,
        declaration: NodeId,
    ) -> Result<Option<Signature>, ()> {
        let kind = self.nodes.kind(declaration);
        let (target, context, returned) = match kind {
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                let target = self.decorator_declaration_type(declaration)?;
                let context = self.decorator_global_type("ClassDecoratorContext", vec![target]);
                (target, context, target)
            }
            SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::PropertyDeclaration
                if self.is_decorator_class_member(declaration) =>
            {
                let value = if kind == SyntaxKind::MethodDeclaration {
                    let signature = self.get_signature_from_declaration(declaration).ok_or(())?;
                    self.decorator_function_type(signature)
                } else {
                    self.decorator_declaration_type(declaration)?
                };
                let this_type = self.decorator_class_side(declaration)?;
                let (name, modifiers) = self.decorator_member_parts(declaration).ok_or(())?;
                let accessor = crate::check::has_modifier(modifiers, SyntaxKind::AccessorKeyword);
                let (template, target, returned) = match kind {
                    SyntaxKind::MethodDeclaration => ("ClassMethodDecoratorContext", value, value),
                    SyntaxKind::GetAccessor => {
                        let id = self.new_signature_id();
                        let target = self.decorator_function_type(decorator_signature(
                            id,
                            declaration,
                            None,
                            Vec::new(),
                            value,
                        ));
                        ("ClassGetterDecoratorContext", target, target)
                    }
                    SyntaxKind::SetAccessor => {
                        let id = self.new_signature_id();
                        let target = self.decorator_function_type(decorator_signature(
                            id,
                            declaration,
                            None,
                            vec![decorator_parameter("value", value)],
                            self.intrinsics.void,
                        ));
                        ("ClassSetterDecoratorContext", target, target)
                    }
                    _ if accessor => {
                        let target = self.decorator_global_type(
                            "ClassAccessorDecoratorTarget",
                            vec![this_type, value],
                        );
                        let returned = self.decorator_global_type(
                            "ClassAccessorDecoratorResult",
                            vec![this_type, value],
                        );
                        ("ClassAccessorDecoratorContext", target, returned)
                    }
                    _ => {
                        let id = self.new_signature_id();
                        let returned = self.decorator_function_type(decorator_signature(
                            id,
                            declaration,
                            Some(decorator_parameter("this", this_type)),
                            vec![decorator_parameter("value", value)],
                            value,
                        ));
                        ("ClassFieldDecoratorContext", self.intrinsics.undefined, returned)
                    }
                };
                let context = self.decorator_global_type(template, vec![this_type, value]);
                let private = matches!(name, PropertyName::PrivateIdentifier(_));
                let static_member =
                    crate::check::has_modifier(modifiers, SyntaxKind::StaticKeyword);
                let name_type = self.decorator_name_type(name)?;
                let overrides = self.decorator_context_override(name_type, private, static_member);
                let context = self.get_intersection_type(&[context, overrides], None);
                (target, context, returned)
            }
            _ => return Ok(None),
        };
        let returned = self.get_union_type(&[returned, self.intrinsics.void]);
        Ok(Some(decorator_signature(
            self.new_signature_id(),
            declaration,
            None,
            vec![decorator_parameter("target", target), decorator_parameter("context", context)],
            returned,
        )))
    }

    fn decorator_name_type(&mut self, name: PropertyName<'a>) -> Result<TypeId, ()> {
        Ok(match name {
            PropertyName::Identifier(name) => self.decorator_string_literal(name.text),
            PropertyName::PrivateIdentifier(name) => self.decorator_string_literal(name.text),
            PropertyName::StringLiteral(name) => self.decorator_string_literal(name.text),
            PropertyName::NumericLiteral(name) => self.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral(crate::printing::normalise_number(name.text)),
                false,
            ),
            PropertyName::ComputedPropertyName(name) => {
                let ty = self.check_expression(name.expression.ok_or(())?);
                if ty == self.intrinsics.error {
                    return Err(());
                }
                self.get_regular_type_of_literal_type(ty)
            }
            _ => self.intrinsics.never,
        })
    }

    fn decorator_string_literal(&mut self, text: &str) -> TypeId {
        self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral(text.to_owned()),
            false,
        )
    }

    fn decorator_context_override(
        &mut self,
        name: TypeId,
        private: bool,
        static_member: bool,
    ) -> TypeId {
        let key = (name, private, static_member);
        if let Some(&ty) = self.decorator_types.overrides.get(&key) {
            return ty;
        }
        let properties = [
            ("name", name),
            (
                "private",
                if private { self.intrinsics.true_type } else { self.intrinsics.false_type },
            ),
            (
                "static",
                if static_member { self.intrinsics.true_type } else { self.intrinsics.false_type },
            ),
        ]
        .into_iter()
        .map(|(name, ty)| AnonymousProperty {
            accessor_write: None,
            method: false,
            origin: None,
            checked_declaration: None,
            name: name.to_owned(),
            printed_name: name.to_owned(),
            printed_slot: crate::objects::PrintedSlot::printed(self.type_to_string(ty)),
            optional: false,
            readonly: false,
            slot: crate::objects::PropertySlot::resolved(ty),
        })
        .collect::<Vec<_>>();
        let members = self.property_members(&properties);
        let text = crate::objects::render_object_type(&members);
        let ty = self.store.new_named(TypeFlags::OBJECT, text, None);
        self.anonymous_properties.insert(ty, (properties, true));
        self.decorator_types.overrides.insert(key, ty);
        ty
    }
}

fn decorator_parameter(name: &str, ty: TypeId) -> Parameter {
    Parameter::new(name.to_owned(), false, false, ty, None)
}

fn decorator_signature(
    id: u32,
    declaration: NodeId,
    this_parameter: Option<Parameter>,
    parameters: Vec<Parameter>,
    returned: TypeId,
) -> Signature {
    Signature { id, mapper: None, declaration,
    target: None,
    union_contains_abstract: false,
    non_inferrable: false,
    kind: SignatureKind::Call,
    type_parameters: Vec::new(),
    this_parameter,
    parameters,
    r#type: returned,
    written_return: None,
    predicate: None, }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::Arena;

    const SOURCE: &str = include_str!("../tests/decorator_contexts.ts");

    fn with_checker(
        source: &str,
        legacy: bool,
        test: impl FnOnce(&mut Checker<'_, '_>, Vec<NodeId>),
    ) {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "control.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        checker.apply_compiler_options(&tsr_core::CompilerOptions {
            strict: tsr_core::Tristate::True,
            experimental_decorators: if legacy {
                tsr_core::Tristate::True
            } else {
                tsr_core::Tristate::False
            },
            ..Default::default()
        });
        let mut decorators = (0..parsed.nodes.len())
            .map(|i| NodeId::new(u32::try_from(i).unwrap()))
            .filter(|&id| parsed.nodes.kind(id) == SyntaxKind::Decorator)
            .collect::<Vec<_>>();
        decorators.sort_by_key(|&id| parsed.nodes.span(id).start);
        test(&mut checker, decorators);
    }

    fn signature(checker: &mut Checker<'_, '_>, decorator: NodeId) -> Signature {
        let ty = checker.contextual_type_for_decorator(decorator).expect("decorator signature");
        checker.signature_types[&ty][0].clone()
    }

    fn property(checker: &mut Checker<'_, '_>, ty: TypeId, name: &str) -> String {
        let property = checker.get_type_of_property_of_type(ty, name).expect("context property");
        checker.type_to_string(property)
    }

    #[test]
    fn es_targets_context_receivers_and_initializer_returns_are_native() {
        with_checker(SOURCE, false, |c, decorators| {
            assert_eq!(decorators.len(), 16);
            let class = signature(c, decorators[0]);
            let class_type = c.parameter_type(&class.parameters[0]);
            assert_eq!(c.type_to_string(class_type), "typeof Vessel");
            let context_type = c.parameter_type(&class.parameters[1]);
            assert_eq!(c.type_to_string(context_type), "ClassDecoratorContext<typeof Vessel>");
            assert!(
                c.contextual_type_for_decorator(decorators[1]).is_none(),
                "ES parameter decorator has no signature"
            );
            for (index, target, receiver, value, name, private, static_member) in [
                (
                    2,
                    "(input: string) => boolean",
                    "typeof Vessel",
                    "(input: string) => boolean",
                    "\"shared\"",
                    "false",
                    "true",
                ),
                (
                    3,
                    "(input: number) => string",
                    "Vessel",
                    "(input: number) => string",
                    "\"shared\"",
                    "false",
                    "false",
                ),
                (
                    5,
                    "(input: boolean) => number",
                    "typeof Vessel",
                    "(input: boolean) => number",
                    "\"#secret\"",
                    "true",
                    "true",
                ),
                (6, "() => number", "Vessel", "number", "\"reading\"", "false", "false"),
                (7, "(value: number) => void", "Vessel", "number", "\"reading\"", "false", "false"),
                (
                    8,
                    "ClassAccessorDecoratorTarget<Vessel, string>",
                    "Vessel",
                    "string",
                    "\"entry\"",
                    "false",
                    "false",
                ),
                (9, "undefined", "Vessel", "boolean", "\"slot\"", "false", "false"),
                (10, "undefined", "Vessel", "string", "17", "false", "false"),
                (11, "undefined", "Vessel", "number", "\"computed\"", "false", "false"),
                (12, "undefined", "Vessel", "boolean", "unique symbol", "false", "false"),
                (14, "undefined", "Parcel<T>", "Wrapped<T>", "\"item\"", "false", "false"),
            ] {
                let sig = signature(c, decorators[index]);
                let target_type = c.parameter_type(&sig.parameters[0]);
                assert_eq!(c.type_to_string(target_type), target);
                let context = c.parameter_type(&sig.parameters[1]);
                assert_eq!(property(c, context, "receiver"), receiver);
                assert_eq!(property(c, context, "value"), value);
                assert_eq!(property(c, context, "name"), name);
                assert_eq!(property(c, context, "private"), private);
                assert_eq!(property(c, context, "static"), static_member);
            }
            let field = signature(c, decorators[9]);
            let TypeData::Union { types: parts, .. } = &c.type_of(field.r#type).data else {
                panic!("void union")
            };
            let mutator =
                parts.iter().find_map(|part| c.signature_types.get(part)).unwrap()[0].clone();
            assert_eq!(
                c.parameter_type(mutator.this_parameter.as_ref().unwrap()),
                c.get_declared_type_of_symbol(
                    c.binder
                        .symbol_of(c.nodes.parent(c.nodes.parent(decorators[9]).unwrap()).unwrap())
                        .unwrap()
                )
            );
            assert_eq!(c.parameter_type(&mutator.parameters[0]), c.intrinsics.boolean);
            assert_eq!(mutator.r#type, c.intrinsics.boolean);
        });
    }

    #[test]
    fn legacy_descriptors_keys_and_parameter_positions_are_native() {
        with_checker(SOURCE, true, |c, decorators| {
            for (index, target, key, descriptor) in [
                (1, "typeof Vessel", "undefined", Some("1")),
                (
                    2,
                    "typeof Vessel",
                    "\"shared\"",
                    Some("TypedPropertyDescriptor<(input: string) => boolean>"),
                ),
                (
                    3,
                    "Vessel",
                    "\"shared\"",
                    Some("TypedPropertyDescriptor<(input: number) => string>"),
                ),
                (6, "Vessel", "\"reading\"", Some("TypedPropertyDescriptor<number>")),
                (7, "Vessel", "\"reading\"", Some("TypedPropertyDescriptor<number>")),
                (8, "Vessel", "\"entry\"", Some("TypedPropertyDescriptor<string>")),
                (9, "Vessel", "\"slot\"", None),
                (10, "Vessel", "\"17\"", None),
                (11, "Vessel", "string", None),
                (12, "Vessel", "unique symbol", None),
                (13, "Vessel", "\"indexed\"", Some("1")),
                (14, "Parcel<T>", "\"item\"", None),
            ] {
                let sig = signature(c, decorators[index]);
                let target_type = c.parameter_type(&sig.parameters[0]);
                assert_eq!(c.type_to_string(target_type), target);
                let key_type = c.parameter_type(&sig.parameters[1]);
                assert_eq!(c.type_to_string(key_type), key);
                assert_eq!(sig.parameters.len(), if descriptor.is_some() { 3 } else { 2 });
                if let Some(descriptor) = descriptor {
                    let descriptor_type = c.parameter_type(&sig.parameters[2]);
                    assert_eq!(c.type_to_string(descriptor_type), descriptor);
                }
            }
            assert_eq!(
                signature(c, decorators[8]).r#type,
                c.intrinsics.void,
                "legacy auto accessor cannot replace value"
            );
        });
    }

    #[test]
    fn cold_warm_reversed_queries_preserve_owner_and_context_identity() {
        for legacy in [false, true] {
            let mut images = Vec::new();
            for reverse in [false, true] {
                with_checker(SOURCE, legacy, |c, decorators| {
                    let mut order = decorators.clone();
                    if reverse {
                        order.reverse();
                    }
                    for decorator in order {
                        c.contextual_type_for_decorator(decorator);
                    }
                    let workers = c.decorator_types.workers;
                    assert_eq!(workers, 15, "two decorators share one declaration owner");
                    let cold = decorators
                        .iter()
                        .map(|&d| c.contextual_type_for_decorator(d))
                        .collect::<Vec<_>>();
                    let count = c.store.len();
                    for (index, &decorator) in decorators.iter().enumerate() {
                        assert_eq!(c.contextual_type_for_decorator(decorator), cold[index]);
                    }
                    assert_eq!(c.store.len(), count, "warm read must not rebuild types");
                    assert_eq!(c.decorator_types.workers, workers);
                    assert_eq!(cold[3], cold[4], "declaration-owned signature");
                    assert_ne!(cold[2], cold[3], "static and instance owners stay distinct");
                    images.push(
                        cold.into_iter()
                            .map(|ty| ty.map(|ty| c.type_to_string(ty)))
                            .collect::<Vec<_>>(),
                    );
                    // Exercise the consumer, not just the synthetic supplier.
                    for (parameter, expected) in [
                        // Native getContextualCallSignature rejects the legacy
                        // one-argument context for a two-required-parameter arrow.
                        ("classTarget", if legacy { "any" } else { "typeof Vessel" }),
                        ("oneTarget", "typeof Single"),
                        (
                            "staticTarget",
                            if legacy { "typeof Vessel" } else { "(input: string) => boolean" },
                        ),
                        (
                            "instanceTarget",
                            if legacy { "Vessel" } else { "(input: number) => string" },
                        ),
                        ("fieldTarget", if legacy { "Vessel" } else { "undefined" }),
                    ] {
                        let (symbol, node) = (0..c.nodes.len()).find_map(|i| {
                            let node = NodeId::new(u32::try_from(i).unwrap());
                            let Node::ParameterDeclaration(p) = c.node_map.get(node)? else { return None };
                            if matches!(p.name, Some(BindingName::Identifier(name)) if name.text == parameter) { c.binder.symbol_of(node).map(|symbol| (symbol, node)) } else { None }
                        }).unwrap();
                        let ty = c.get_type_of_symbol(symbol);
                        assert_eq!(c.type_to_string(ty), expected, "legacy={legacy}: {parameter}");
                        assert_eq!(
                            c.type_to_string_at(ty, node).as_deref(),
                            Some(expected),
                            "site serialization: {parameter}"
                        );
                    }
                });
            }
            assert_eq!(images[0], images[1], "query order changed context");
        }
    }

    #[test]
    fn active_nil_unsupported_and_override_keys_remain_distinct() {
        with_checker(SOURCE, false, |c, decorators| {
            let declaration = c.nodes.parent(decorators[0]).unwrap();
            c.decorator_types.signatures.insert(declaration, DecoratorType::Active);
            assert!(c.contextual_type_for_decorator(decorators[0]).is_none());
            assert!(matches!(c.decorator_types.signatures[&declaration], DecoratorType::Active));
            assert_eq!(c.decorator_types.workers, 0);
            c.decorator_types.signatures.remove(&declaration);
            assert!(c.contextual_type_for_decorator(decorators[0]).is_some());
            let invalid = c.nodes.parent(decorators[1]).unwrap();
            assert!(c.contextual_type_for_decorator(decorators[1]).is_none());
            assert!(matches!(
                c.decorator_types.signatures[&invalid],
                DecoratorType::Complete(None)
            ));
            c.decorator_types.signatures.insert(invalid, DecoratorType::Unsupported);
            assert!(c.contextual_type_for_decorator(decorators[1]).is_none());
            assert!(matches!(c.decorator_types.signatures[&invalid], DecoratorType::Unsupported));
            let name = c.decorator_string_literal("same");
            let ordinary = c.decorator_context_override(name, false, false);
            assert_eq!(ordinary, c.decorator_context_override(name, false, false));
            assert_ne!(ordinary, c.decorator_context_override(name, true, false));
            assert_ne!(ordinary, c.decorator_context_override(name, false, true));
            let number = c.store.intern_literal(
                TypeFlags::NUMBER_LITERAL,
                TypeData::NumberLiteral("17".to_owned()),
                false,
            );
            let string = c.decorator_string_literal("17");
            assert_ne!(
                c.decorator_context_override(number, false, false),
                c.decorator_context_override(string, false, false)
            );
        });
    }
}
