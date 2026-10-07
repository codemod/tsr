//! Instantiation expressions, pinned to typescript-go 5b1047d checker.go:10660-10738.
//! The node/source cache publishes only a completed transformation, before errors
//! are rendered. Source links share members/indexes; only signatures are mapped.

use tsr_ast::{Node, NodeId, TypeNode};
use tsr_binder::SymbolId;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{Checker, calls::InstantiationExpressionSignature, flags::TypeFlags,
    signatures::{Signature, SignatureKind}, types::{TypeData, TypeId}};

#[derive(Default)]
struct Applicability {
    any_applicable: bool,
    first_inapplicable: Option<TypeId>,
}

#[derive(Default)]
struct PartApplicability {
    has_signatures: bool,
    applicable: bool,
}

impl<'a> Checker<'a, '_> {
    pub(crate) fn get_instantiation_expression_type(&mut self, source: TypeId, node: NodeId) -> TypeId {
        if Some(source) == self.silent_never_type || self.is_error(source)
            || self.store.get(source).flags.intersects(TypeFlags::ANY) && self.alias_of.contains_key(&source)
            || self.nodes.type_argument_list_span(node).is_none()
        {
            return source;
        }
        let key = (self.type_literal_key(node), source);
        if let Some(&image) = self.instantiation_expression_types.get(&key) {
            return image;
        }
        let Some(arguments) = self.instantiation_argument_nodes(node) else {
            return self.intrinsics.error;
        };
        let mut applicability = Applicability::default();
        let Some(result) = self.instantiate_expression_type_scope(source, node, arguments, &mut applicability) else {
            // Unsupported structural/signature work must not publish completion.
            return self.intrinsics.error;
        };
        self.instantiation_expression_types.insert(key, result);
        let error_type = if applicability.any_applicable { applicability.first_inapplicable } else { Some(source) };
        if let Some(error_type) = error_type
            && let Some(file) = self.source_file_of_for_diagnostics(node)
            && let Some(list) = self.nodes.type_argument_list_span(node)
            && let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        {
            let span = tsr_core::Span::new(tsr_scanner::skip_trivia(text, list.start), list.end);
            let printed = self.type_to_string(error_type);
            self.report(file, Diagnostic::with_args(
                &messages::TYPE_0_HAS_NO_SIGNATURES_FOR_WHICH_THE_TYPE_ARGUMENT_LIST_IS_APPLICABLE,
                span, [printed],
            ));
        }
        result
    }
    /// checkGrammarExpressionWithTypeArguments/checkGrammarTypeArguments
    /// (5b1047d grammarchecks.go:845-889). Native grammar still checks before
    /// the silent-never/error source early-out of the semantic worker.
    pub(crate) fn check_instantiation_expression_grammar(&mut self, node: NodeId) {
        if self.file_has_parse_errors { return; }
        let Some(list) = self.nodes.type_argument_list_span(node) else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        if let Some(Node::ExpressionWithTypeArguments(expression)) = self.node_map.get(node)
            && expression.expression.is_some_and(|expression| matches!(expression,
                tsr_ast::Expression::KeywordExpression(keyword) if keyword.kind == tsr_ast::SyntaxKind::ImportKeyword))
        {
            self.grammar_error_on_node(node,
                &messages::THIS_USE_OF_IMPORT_IS_INVALID_IMPORT_CALLS_CAN_BE_WRITTEN_BUT_THEY_MUST_HAVE_PARENTHESES_AND_CANNOT_HAVE_TYPE_ARGUMENTS);
            return;
        }
        let Some(arguments) = self.instantiation_argument_nodes(node) else { return };
        let message_span = if self.nodes.flags(node).contains(tsr_ast::NodeFlags::HAS_TRAILING_COMMA) {
            Some((&messages::TRAILING_COMMA_NOT_ALLOWED, tsr_core::Span::new(list.end - 1, list.end)))
        } else if arguments.is_empty() {
            let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes)) else { return };
            let end = tsr_scanner::skip_trivia(text, list.end) + 1;
            Some((&messages::TYPE_ARGUMENT_LIST_CANNOT_BE_EMPTY, tsr_core::Span::new(list.start - 1, end)))
        } else {
            None
        };
        if let Some((message, span)) = message_span {
            self.report(file, Diagnostic::new(message, span));
        }
    }

    /// checkExpressionWithTypeArguments's instanceof operand rule (:10640).
    pub(crate) fn check_instantiation_instanceof_operand(&mut self, node: NodeId) {
        let mut parent = self.nodes.parent(node);
        while let Some(current) = parent
            && matches!(self.node_map.get(current), Some(Node::ParenthesizedExpression(_)))
        {
            parent = self.nodes.parent(current);
        }
        let Some(parent) = parent else { return };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else { return };
        if binary.operator_token.is_none_or(|token| token.kind != tsr_ast::SyntaxKind::InstanceOfKeyword) {
            return;
        }
        let Some(right) = binary.right.and_then(|expression| expression.node_id()) else { return };
        if node == right || self.nodes.ancestors(node).any(|ancestor| ancestor == right) {
            self.grammar_error_on_node(node,
                &messages::THE_RIGHT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_NOT_BE_AN_INSTANTIATION_EXPRESSION);
        }
    }


    fn instantiation_argument_nodes(&self, node: NodeId) -> Option<&'a [TypeNode<'a>]> {
        match self.node_map.get(node)? {
            Node::ExpressionWithTypeArguments(expression) => Some(expression.type_arguments),
            Node::TypeQueryNode(query) => Some(query.type_arguments),
            Node::ImportTypeNode(import) => Some(import.type_arguments),
            _ => None,
        }
    }

    fn instantiate_expression_type_scope(&mut self, source: TypeId, node: NodeId,
        arguments: &[TypeNode<'a>], all: &mut Applicability) -> Option<TypeId> {
        let mut part = PartApplicability::default();
        let result = self.instantiate_expression_type_part(source, node, arguments, all, &mut part)?;
        all.any_applicable |= part.applicable;
        if part.has_signatures && !part.applicable && all.first_inapplicable.is_none() {
            all.first_inapplicable = Some(source);
        }
        Some(result)
    }

    fn instantiate_expression_type_part(&mut self, source: TypeId, node: NodeId,
        arguments: &[TypeNode<'a>], all: &mut Applicability, part: &mut PartApplicability) -> Option<TypeId> {
        let flags = self.store.get(source).flags;
        if flags.intersects(TypeFlags::OBJECT) {
            // resolveStructuredTypeMembers also demands member/index preparation;
            // none is unsupported, never completed absence.
            let property_names = self.get_property_names_of_type(source)?;
            let indexes = self.get_index_infos_of_type(source)?;
            let calls = self.head_signatures(source, SignatureKind::Call)?;
            let constructs = self.head_signatures(source, SignatureKind::Construct)?;
            part.has_signatures |= !calls.is_empty() || !constructs.is_empty();
            let (mut images, calls_changed) = self.instantiate_expression_signatures(&calls, arguments)?;
            let (construct_images, constructs_changed) = self.instantiate_expression_signatures(&constructs, arguments)?;
            part.applicable |= !images.is_empty() || !construct_images.is_empty();
            if !calls_changed && !constructs_changed {
                return Some(source);
            }
            images.extend(construct_images);
            let symbol = self.instantiation_source_symbol(source)?;
            // Text is not evaluated here: native leaves return/predicate lazy.
            // Canonical rendering uses this image's signatures and source edges.
            let empty_members = property_names.is_empty() && indexes.is_empty();
            let signature_node = empty_members && images.len() == 1;
            let image = self.store.new_anonymous(TypeFlags::OBJECT, String::new(), symbol, signature_node);
            self.signature_types.insert(image, images);
            self.instantiation_expression_sources.insert(image, source);
            self.instantiation_expression_nodes.insert(image, node);
            if empty_members {
                self.anonymous_properties.insert(image, (Vec::new(), true));
                self.object_literal_index_infos.insert(image, Vec::new());
            }
            return Some(image);
        }
        if flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) {
            if let Some(constraint) = self.base_constraint_of_type(source) {
                let image = self.instantiate_expression_type_part(constraint, node, arguments, all, part)?;
                if image != constraint { return Some(image); }
            }
            return Some(source);
        }
        if flags.intersects(TypeFlags::UNION) {
            // mapTypeEx traverses a denormalized union origin if present, with
            // a separate applicability scope for each non-union constituent.
            let original = if let Some(origin) = self.union_origin.get(&source) {
                if origin.len() != 1 || self.store.get(origin[0]).flags.contains(TypeFlags::UNION) {
                    origin.clone()
                } else {
                    let TypeData::Union { types, .. } = &self.store.get(source).data else { return None };
                    types.clone()
                }
            } else {
                let TypeData::Union { types, .. } = &self.store.get(source).data else { return None };
                types.clone()
            };
            let mut images = Vec::with_capacity(original.len());
            let mut changed = false;
            for constituent in original {
                let image = if self.store.get(constituent).flags.contains(TypeFlags::UNION) {
                    self.instantiate_expression_type_part(constituent, node, arguments, all, part)?
                } else {
                    self.instantiate_expression_type_scope(constituent, node, arguments, all)?
                };
                changed |= image != constituent;
                images.push(image);
            }
            let result = if changed { self.get_union_type(&images) } else { source };
            if changed && self.store.get(result).flags.contains(TypeFlags::UNION) {
                self.instantiation_expression_composites.insert(result);
            }
            return (!self.is_error(result)).then_some(result);
        }
        if flags.intersects(TypeFlags::INTERSECTION) {
            let TypeData::Intersection { types, .. } = &self.store.get(source).data else { return None };
            let original = types.clone();
            let mut images = Vec::with_capacity(original.len());
            let mut changed = false;
            for constituent in original {
                let image = self.instantiate_expression_type_part(constituent, node, arguments, all, part)?;
                changed |= image != constituent;
                images.push(image);
            }
            let result = if changed { self.get_intersection_type(&images, None) } else { source };
            if changed && self.store.get(result).flags.contains(TypeFlags::INTERSECTION) {
                self.instantiation_expression_composites.insert(result);
            }
            return (!self.is_error(result)).then_some(result);
        }
        Some(source)
    }

    fn instantiate_expression_signatures(&mut self, signatures: &[Signature], arguments: &[TypeNode<'a>])
        -> Option<(Vec<Signature>, bool)> {
        let mut images = Vec::with_capacity(signatures.len());
        let mut changed = false;
        for signature in signatures {
            match self.get_instantiation_expression_signature(signature, arguments)? {
                InstantiationExpressionSignature::Inapplicable => changed = true,
                InstantiationExpressionSignature::ConstraintRejected => images.push(signature.clone()),
                InstantiationExpressionSignature::Instantiated(image) => {
                    changed = true;
                    images.push(image);
                }
            }
        }
        Some((images, changed))
    }

    fn instantiation_source_symbol(&self, source: TypeId) -> Option<SymbolId> {
        match self.store.get(source).data {
            TypeData::Anonymous { symbol, .. } | TypeData::Named { members: Some(symbol), .. } => Some(symbol),
            _ => None,
        }
    }

    /// instantiateAnonymousType/resolveAnonymousTypeMembers (5b1047d).
    /// Reuse the existing object-instantiation cache by source and ordered
    /// mapper identities. Publish only the completed input-signature image;
    /// member/index semantics remain in the mapped source supplier.
    pub(crate) fn instantiate_expression_image(
        &mut self, id: TypeId, map: &[(TypeId, TypeId)],
        parameters: &[TypeId], names: &[&str],
    ) -> Option<TypeId> {
        let key = (id, map.to_vec());
        if let Some(&image) = self.instantiated_objects.get(&key) { return Some(image); }
        let source = *self.instantiation_expression_sources.get(&id)?;
        let node = *self.instantiation_expression_nodes.get(&id)?;
        let mapped_source = self.instantiate_type(source, map, parameters, names);
        if self.is_error(mapped_source) { return None; }
        let signatures = self.signature_types.get(&id)?.clone();
        let mut images = Vec::with_capacity(signatures.len());
        for signature in signatures {
            images.push(self.instantiate_signature_with_fresh_parameters_worker(
                signature, map, parameters, names, true,
            )?);
        }
        let symbol = self.instantiation_source_symbol(mapped_source)?;
        let empty_members = self.get_property_names_of_type(mapped_source)?.is_empty()
            && self.get_index_infos_of_type(mapped_source)?.is_empty();
        let signature_node = empty_members && images.len() == 1;
        let image = self.store.new_anonymous(TypeFlags::OBJECT, String::new(), symbol, signature_node);
        self.signature_types.insert(image, images);
        self.instantiation_expression_sources.insert(image, mapped_source);
        self.instantiation_expression_nodes.insert(image, node);
        if empty_members {
            self.anonymous_properties.insert(image, (Vec::new(), true));
            self.object_literal_index_infos.insert(image, Vec::new());
        }
        self.instantiated_objects.insert(key, image);
        Some(image)
    }

    /// Native typeToTypeNode recursively serializes semantic constituents;
    /// wrapper text is deliberately absent until this display boundary.
    pub(crate) fn instantiation_composite_text(
        &mut self, id: TypeId, reference: Option<NodeId>,
    ) -> Option<String> {
        if !self.rendering_composites.insert(id) {
            return Some("any".to_owned());
        }
        let result = (|| {
            let (parts, separator, union) = match &self.store.get(id).data {
                TypeData::Union { types, symbol: None, .. } => (
                    crate::unions::union_print_parts(&self.store, types), " | ", true,
                ),
                TypeData::Intersection { types, symbol: None, .. } => (
                    types.iter().copied().map(crate::unions::UnionPrintPart::Type).collect(),
                    " & ", false,
                ),
                _ => return None,
            };
            let mut text = String::new();
            for part in parts {
                if !text.is_empty() { text.push_str(separator); }
                let member = match part {
                    crate::unions::UnionPrintPart::Keyword(keyword) => {
                        text.push_str(keyword);
                        continue;
                    }
                    crate::unions::UnionPrintPart::Type(member) => member,
                };
                let printed = self.instantiation_slot_text(member, reference)?;
                let ty = self.store.get(member);
                let needs_parentheses = !crate::printing::prints_as_a_single_token(ty)
                    && (matches!(ty.data, TypeData::Anonymous { signature: true, .. })
                        || if union { ty.flags.contains(TypeFlags::INTERSECTION) }
                        else { ty.flags.contains(TypeFlags::UNION) && !ty.flags.contains(TypeFlags::BOOLEAN) });
                if needs_parentheses { text.push('('); }
                text.push_str(&printed);
                if needs_parentheses { text.push(')'); }
            }
            Some(text)
        })();
        self.rendering_composites.remove(&id);
        result
    }

    /// createAnonymousTypeNodeEx/createTypeNodeFromObjectType (5b1047d).
    /// Presentation demands returns, indexes and members only at the print site;
    /// neither signature filtering nor reference identity construction prints.
    pub(crate) fn instantiation_expression_text(
        &mut self, image: TypeId, reference: Option<NodeId>,
    ) -> Option<String> {
        let source = *self.instantiation_expression_sources.get(&image)?;
        if !self.rendering_composites.insert(image) {
            return Some("any".to_owned());
        }
        let result = (|| {
            if let Some(&node) = self.instantiation_expression_nodes.get(&image)
                && let Some(Node::TypeQueryNode(query)) = self.node_map.get(node)
                && self.get_type_from_type_node(TypeNode::TypeQueryNode(query)) == image
                && let Some(written) = self.reuse_annotation(TypeNode::TypeQueryNode(query), image)
            {
                let text = match reference {
                    Some(reference) => self.written_annotation_text_at(written, image, reference),
                    None => self.site_free_annotation_text(written, image),
                };
                if text.is_some() { return text; }
            }
            let signatures = self.signature_types.get(&image)?.clone();
            let names = self.get_property_names_of_type(source)?;
            let indexes = self.get_index_infos_of_type(source)?;
            if names.is_empty() && indexes.is_empty() && signatures.len() == 1 {
                let signature = self.complete_signature_return(signatures[0].clone())?;
                return Some(match reference {
                    Some(reference) => self.signature_to_string_at(&signature, reference),
                    None => self.signature_to_string(&signature),
                });
            }
            let mut members = Vec::with_capacity(signatures.len() + indexes.len() + names.len());
            let mut abstract_parts = Vec::new();
            let mut claimed = rustc_hash::FxHashSet::default();
            for signature in signatures {
                let signature = self.complete_signature_return(signature)?;
                if signature.kind == SignatureKind::AbstractConstruct {
                    abstract_parts.push(match reference {
                        Some(reference) => self.signature_to_string_at(&signature, reference),
                        None => self.signature_to_string(&signature),
                    });
                    continue;
                }
                let printed = match reference {
                    Some(reference) => self.type_literal_signature_at(signature, reference, &mut claimed),
                    None => crate::objects::signature_member_text(self, &signature),
                };
                members.push(crate::objects::Member::Signature { printed });
            }
            for index in indexes {
                let name = index.declaration.and_then(|declaration| match self.node_map.get(declaration) {
                    Some(Node::IndexSignatureDeclaration(signature)) => signature.parameters.first()
                        .and_then(|parameter| parameter.name).and_then(|name| match name {
                            tsr_ast::BindingName::Identifier(name) => Some(name.text.to_owned()),
                            tsr_ast::BindingName::BindingPattern(_) => None,
                        }),
                    _ => None,
                }).unwrap_or_else(|| "x".to_owned());
                let key = self.instantiation_slot_text(index.key, reference)?;
                let value = self.instantiation_slot_text(index.value, reference)?;
                members.push(crate::objects::Member::Index { readonly: index.readonly, name, key, value });
            }
            for name in names {
                let ty = self.get_type_of_property_of_type(source, &name)?;
                let property = self.get_property_of_type(source, &name);
                let optional = property.is_some_and(|symbol| self.property_is_optional(symbol));
                let readonly = self.is_readonly_property_of_type(source, &name);
                let printed_name = property.map_or_else(
                    || if crate::objects::is_identifier_text(&name) { name.clone() }
                        else { crate::printing::quote_ascii(&name) },
                    |symbol| self.callable_property_name(symbol, &name),
                );
                if let Some(property) = property
                    && self.binder.symbols().get(property).flags.contains(tsr_binder::SymbolFlags::ACCESSOR)
                {
                    let write = self.write_type_of_property_of_type(source, &name).unwrap_or(ty);
                    let owner = self.binder.symbols().get(property).parent;
                    let class_owner = owner.is_some_and(|owner| self.binder.symbols().get(owner)
                        .flags.contains(tsr_binder::SymbolFlags::CLASS));
                    if !self.is_error(ty) && !self.is_error(write) && (ty != write || class_owner) {
                        let declarations = self.binder.symbols().get(property).declarations.clone();
                        for kind in [tsr_ast::SyntaxKind::GetAccessor, tsr_ast::SyntaxKind::SetAccessor] {
                            let Some(declaration) = declarations.iter().copied()
                                .find(|&declaration| self.nodes.kind(declaration) == kind) else { continue };
                            let signature = self.get_signature_from_declaration(declaration)?;
                            let printed = if kind == tsr_ast::SyntaxKind::GetAccessor {
                                format!("get {printed_name}(): {}", self.instantiation_slot_text(ty, reference)?)
                            } else {
                                let parameter = signature.parameters.first()?;
                                format!("set {printed_name}({}: {})", parameter.name,
                                    self.instantiation_slot_text(write, reference)?)
                            };
                            members.push(crate::objects::Member::Signature { printed });
                        }
                        continue;
                    }
                }
                if property.is_some_and(|symbol| self.binder.symbols().get(symbol).flags
                    .intersects(tsr_binder::SymbolFlags::FUNCTION | tsr_binder::SymbolFlags::METHOD))
                    && !readonly && self.get_property_names_of_type(ty)?.is_empty()
                {
                    let signatures = self.signatures_of_type_kind(ty, SignatureKind::Call)?;
                    if !signatures.is_empty() {
                        let mut method_claimed = rustc_hash::FxHashSet::default();
                        for signature in signatures {
                            let printed = match reference {
                                Some(reference) => self.type_literal_signature_at(signature, reference, &mut method_claimed),
                                None => crate::objects::signature_member_text(self, &signature),
                            };
                            let name = if printed_name == "new" { "\"new\"" } else { &printed_name };
                            members.push(crate::objects::Member::Signature {
                                printed: format!("{name}{}{printed}", if optional { "?" } else { "" }),
                            });
                        }
                        continue;
                    }
                }
                let printed = self.instantiation_slot_text(ty, reference)?;
                members.push(crate::objects::Member::Property {
                    name: printed_name, optional, readonly, printed,
                });
            }
            let object = crate::objects::render_object_type(&members);
            if abstract_parts.is_empty() { return Some(object); }
            let has_object = !members.is_empty();
            let multiple = abstract_parts.len() + usize::from(has_object) > 1;
            let mut text = abstract_parts.into_iter().map(|part| {
                if multiple { format!("({part})") } else { part }
            }).collect::<Vec<_>>().join(" & ");
            if has_object {
                text.push_str(" & ");
                // getResolvedTypeWithoutAbstractConstructSignatures creates an
                // ordinary anonymous type: its original class symbol may name
                // the residual through native shouldEmitTypeOfSymbol.
                let symbol = self.instantiation_source_symbol(source)?;
                if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::CLASS) {
                    text.push_str(&self.instantiation_slot_text(source, reference)?);
                } else {
                    text.push_str(&object);
                }
            }
            Some(text)
        })();
        self.rendering_composites.remove(&image);
        result
    }

    fn instantiation_slot_text(&mut self, ty: TypeId, reference: Option<NodeId>) -> Option<String> {
        match reference {
            Some(reference) => self.type_to_string_at(ty, reference),
            None => Some(self.type_to_string(ty)),
        }
    }
}
