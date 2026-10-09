//! Rendering a type as the string a `.types` baseline compares against.
//!
//! Ported from `Checker.typeToString` (`internal/checker/checker.go`), which
//! delegates to the node builder and then to the printer. Only the intrinsic and
//! literal cases are ported; everything else needs types that do not exist yet.
//!
//! # Why this is more delicate than it looks
//!
//! Every `.types` baseline compares **whole lines, verbatim**. So the rendering is
//! not a debugging convenience — it is the thing under test, and a difference of
//! one character fails a case just as surely as an outright wrong type. The two
//! places a port drifts here are string escaping and number formatting, and both
//! are handled explicitly below rather than left to `Display`.

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{Type, TypeData, TypeId},
};

/// Pinned tsgo 5b1047d: typeParameterToName / cloneNodeBuilderContext.
/// Private Checker display state, keyed by semantic `TypeId` in this store.
/// An absent entry is unallocated; only a completed name is published. No
/// provisional entry or failure is cached. The outer `type_to_string_at` call
/// clears allocations on success or refusal; signatures truncate to their
/// inherited allocation depth so nested slots share names but siblings do not.
/// Resolution uses the current print site and render scope, never a receiver
/// mapper or alias spelling. Stored semantic types are not rewritten. Allocation
/// scans only names in this print; union rendering walks existing constituents
/// under `rendering_composites`, without forcing members or adding semantic reuse.
#[derive(Default)]
pub(crate) struct TypeParameterNames {
    pub(crate) depth: usize,
    pub(crate) allocations: Vec<(TypeId, String)>,
}

impl Checker<'_, '_> {
    /// Pinned tsgo 5b1047d: getTypeOfVariableOrParameterOrPropertyWorker /
    /// createTypeNodesFromResolvedType. The synthetic `CommonJS` wrapper keeps
    /// its binder `MODULE_EXPORTS` symbol and completed members table. Only its
    /// published symbol type can be serialized; active members decline without
    /// marking a semantic cycle failed. Force an export's lazy semantic type
    /// through the ordinary getter, then name it at this viewer. No wrapper
    /// text, receiver image, print-site result or provisional value is cached.
    pub(crate) fn module_exports_text_at(
        &mut self,
        id: TypeId,
        symbol: tsr_binder::SymbolId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        if self.symbol_types.get(&symbol) != Some(&id)
            || self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
            || !self.rendering_composites.insert(id)
        {
            return None;
        }
        let mut properties: Vec<_> =
            self.binder.symbols().get(symbol).members.values().copied().collect();
        properties.sort_by_cached_key(|&symbol| self.compare_symbols_key(symbol));
        let result = properties
            .into_iter()
            .map(|property| {
                if self.resolutions.on_stack(property, crate::resolution::PropertyName::Type) {
                    return None;
                }
                let ty = self.get_type_of_symbol(property);
                let printed = self.type_to_string_at(ty, reference)?;
                Some(crate::objects::Member::Property {
                    name: self.binder.symbols().get(property).name.to_string(),
                    optional: false,
                    readonly: false,
                    printed,
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(|members| crate::objects::render_object_type(&members));
        self.rendering_composites.remove(&id);
        result
    }

    /// Pinned tsgo 5b1047d getFileSymbolIfFileSymbolExportEqualsContainer /
    /// getSpecifierForModuleSymbol. A completed original class constructor can
    /// be named by its file only when the file's export= resolves to that very
    /// symbol. Module-value clones retain their own naming route. Visibility
    /// starts at the assertion's parent, so a class expression's whole value
    /// cannot see its private self-name, but its name/body can. No type, alias,
    /// or per-site result is published here; existing semantic alias resolution
    /// certifies the container without forcing its export value type. Keep the
    /// current file-module renderer's rooted single-directory specifier boundary.
    pub(crate) fn export_equals_class_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        use tsr_binder::SymbolFlags;

        let TypeData::Anonymous { symbol, .. } = self.store.get(id).data else {
            return None;
        };
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS)
            || self.symbol_types.get(&symbol) != Some(&id)
            || self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type)
        {
            return None;
        }
        // `getContainersOfSymbol` (`symbolaccessibility.go:280`) offers the
        // file as a container for a declaration that is a direct child of it
        // (`hasNonGlobalAugmentationExternalModuleSymbol(d.Parent)`) or for a
        // class expression assigned to `module.exports` / `exports.x`. The
        // class expression of `export = class B {}` has the export
        // assignment as its parent and keeps its own name.
        let declarations = self.binder.symbols().get(symbol).declarations.to_vec();
        let module = declarations.into_iter().find_map(|declaration| {
            let file = self.source_file_of_for_diagnostics(declaration)?;
            let parent = self.nodes.parent(declaration)?;
            let commonjs_class = matches!(
                (self.node_map.get(declaration), self.node_map.get(parent)),
                (Some(tsr_ast::Node::ClassExpression(_)), Some(tsr_ast::Node::BinaryExpression(binary)))
                    if binary.operator_token.is_some_and(|token| token.kind == tsr_ast::SyntaxKind::EqualsToken)
                        && binary.left.is_some_and(|left| {
                            crate::assignment_declarations::is_module_exports_access(left)
                                || matches!(left,
                                    tsr_ast::Expression::PropertyAccessExpression(tsr_ast::PropertyAccessExpression {
                                        expression: Some(tsr_ast::Expression::Identifier(receiver)), ..
                                    }) | tsr_ast::Expression::ElementAccessExpression(tsr_ast::ElementAccessExpression {
                                        expression: Some(tsr_ast::Expression::Identifier(receiver)), ..
                                    }) if receiver.text == "exports")
                        })
            );
            if parent != file && !commonjs_class {
                return None;
            }
            let module = self.binder.symbol_of(file)?;
            let exported = self.resolve_external_module_symbol(module);
            (exported != module
                && self.binder.merged_symbol(self.resolve_alias_fully(exported)) == symbol)
                .then_some(module)
        })?;
        let enclosing = self.nodes.parent(reference).unwrap_or(reference);
        if let Some(name) = self.best_name(symbol, enclosing) {
            return Some(format!("typeof {name}"));
        }
        // `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1249`) through
        // `crate::module_specifiers` (r5-modules2 §4): the module symbol's
        // name keeps `index` and declaration suffixes (`./foo.d`) that
        // `processEnding` removes.
        let specifier = self.module_specifier_for_symbol(module, reference)?;
        Some(format!("typeof import({specifier})"))
    }

    pub(crate) fn allocate_type_parameter_name(
        &mut self,
        id: TypeId,
        symbol: tsr_binder::SymbolId,
        enclosing: tsr_ast::NodeId,
    ) -> String {
        if let Some((_, name)) =
            self.render_type_parameter_names.allocations.iter().find(|(owner, _)| *owner == id)
        {
            return name.clone();
        }
        if let Some((name, _)) =
            self.render_type_parameter_scope.iter().rev().find(|(_, owner)| *owner == symbol)
        {
            return name.clone();
        }
        let raw = self.type_to_string(id);
        let taken = |text: &str| {
            self.render_type_parameter_scope.iter().any(|(name, _)| name == text)
                || self.render_type_parameter_names.allocations.iter().any(|(_, name)| name == text)
                || self
                    .binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        enclosing,
                        text,
                        tsr_binder::SymbolFlags::TYPE,
                    )
                    .is_some_and(|found| {
                        found != symbol
                            && self
                                .binder
                                .symbols()
                                .get(found)
                                .flags
                                .contains(tsr_binder::SymbolFlags::TYPE_PARAMETER)
                    })
        };
        let mut text = raw.clone();
        let mut suffix = 0usize;
        while taken(&text) {
            suffix += 1;
            text = format!("{raw}_{suffix}");
        }
        self.render_type_parameter_names.allocations.push((id, text.clone()));
        text
    }

    /// Native createAnonymousTypeNodeEx / createTypeNodesFromResolvedType.
    /// `check_object_literal_members` publishes the original fresh image only
    /// after checking members and indices; regularization/widening explicitly
    /// transfer that image to a distinct `TypeId`. These identities own the
    /// plan, not arbitrary `anonymous_properties` or baked placeholders.
    /// All three completed maps and source-only property syntax are required.
    /// Reuse the writer's actual member order/flags/names and replace only its
    /// typed slots. The per-print visiting set elides anonymous re-entry to any
    /// and resets on success/refusal. Canonical signature returns are forced
    /// only after image publication; no semantic owner/member/type is rewritten.
    /// A certified inline initializer follows native pseudoTypeToNode instead:
    /// it writes source properties/signatures without visiting their type id.
    pub(crate) fn object_literal_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
        visit_identity: bool,
    ) -> Option<String> {
        self.certified_object_literal_text_at(id, reference, visit_identity)
            .or_else(|| self.deferred_accessor_text_at(id, reference))
    }

    /// An object-literal image whose member plan the certified renderer
    /// declines still carries the placeholder an on-demand accessor slot baked
    /// at the mint (`DEFERRED_ACCESSOR_TEXT`). Native serializes that member
    /// from the accessor symbol's type (createTypeNodesFromResolvedType), so
    /// the baked plan is kept and only those slots are printed from the type
    /// read here. While a variable enclosing the literal is still resolving,
    /// that read is the re-entry native never makes: keep the baked text.
    fn deferred_accessor_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let properties = &self.anonymous_properties.get(&id)?.0;
        if !properties.iter().any(crate::objects::AnonymousProperty::reads_on_demand) {
            return None;
        }
        let deferred: Vec<_> =
            properties.iter().filter(|property| property.reads_on_demand()).cloned().collect();
        // createAnonymousTypeNode's visited check: re-entry elides to `any`.
        if self.rendering_composites.contains(&id) {
            return Some("any".to_string());
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        if self
            .binder
            .symbols()
            .get(owner)
            .declarations
            .iter()
            .any(|&declaration| self.enclosing_variable_resolving(declaration))
        {
            return None;
        }
        let mut members = self.object_literal_members.get(&id)?.clone();
        self.rendering_composites.insert(id);
        let result = deferred.iter().try_for_each(|property| {
            let Some(crate::objects::Member::Property { printed, .. }) =
                members.iter_mut().find(|member| {
                    matches!(member, crate::objects::Member::Property { name, .. }
                        if *name == property.printed_name)
                })
            else {
                return Some(());
            };
            let property_type = self.property_type(property);
            *printed = self.type_to_string_at(property_type, reference)?;
            Some(())
        });
        self.rendering_composites.remove(&id);
        result.map(|()| crate::objects::render_object_type(&members))
    }

    fn certified_object_literal_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
        visit_identity: bool,
    ) -> Option<String> {
        let mut source = id;
        while !self.fresh_object_literal_types.contains(&source) {
            // Widening's temporary source -> source entry is not completion.
            // A distinct target is allocated and published only after its
            // member transformation finishes. Follow existing transfers back
            // to the original image, without resolving or constructing types.
            // `object_type_transfer_origins` holds exactly the entries this
            // scan can match. A sole original is the scan's answer; several
            // distinct ones keep the scan, whose table order picks one.
            let origins = self.object_type_transfer_origins.get(&source)?;
            let &(first, _) = origins.first()?;
            source = if origins.iter().all(|&(original, _)| original == first) {
                first
            } else {
                self.regular_object_literal_types
                    .iter()
                    .chain(self.widened_object_types.iter())
                    .find_map(|(&original, &target)| {
                        (target == source && original.index() < target.index()).then_some(original)
                    })?
            };
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(source).data else {
            return None;
        };
        if !matches!(self.store.get(id).data, TypeData::Named { members: Some(current), .. } if current == owner)
        {
            return None;
        }
        let [declaration] = self.binder.symbols().get(owner).declarations.as_slice() else {
            return None;
        };
        let tsr_ast::Node::ObjectLiteralExpression(literal) = self.node_map.get(*declaration)?
        else {
            return None;
        };
        // A completed inner image can still be baked into an active enclosing
        // variable's value. Node building is not part of member collection;
        // demanding a captured return here would fail that variable's type
        // resolution before normal publication. Decline without forcing it.
        if self.enclosing_variable_resolving(*declaration) {
            return None;
        }
        if !self.object_literal_index_infos.get(&source)?.is_empty()
            // Widening omits an empty index table. Its completed transfer
            // plus the original empty table certifies that absence; a real
            // index or an untracked image still cannot enter this renderer.
            || self.object_literal_index_infos.get(&id).is_some_and(|indices| !indices.is_empty())
            || !self.anonymous_properties.contains_key(&source)
            || !self.object_literal_members.contains_key(&source)
        {
            return None;
        }
        let properties = self.anonymous_properties.get(&id)?.0.clone();
        let mut members = self.object_literal_members.get(&id)?.clone();
        if !self.object_member_plan_matches_source(literal, &members, &properties) {
            return None;
        }
        if visit_identity && !self.rendering_composites.insert(id) {
            return Some("any".to_string());
        }
        let result = members
            .iter_mut()
            .zip(properties)
            .try_for_each(|(member, property)| {
                // An on-demand accessor slot baked a placeholder at the mint
                // (`DEFERRED_ACCESSOR_TEXT`); native serializes the accessor
                // symbol's type read here (createTypeNodesFromResolvedType).
                if property.reads_on_demand() {
                    let crate::objects::Member::Property { printed, .. } = member else {
                        return None;
                    };
                    let property_type = self.property_type(&property);
                    *printed = self.type_to_string_at(property_type, reference)?;
                    return Some(());
                }
                // Method/accessor overrides retain their producer's display.
                // Only original property assignments and shorthand slots are
                // admitted to the dynamic property serializer below.
                if property.checked_declaration.is_none()
                    && self.binder.symbols().get(property.origin?).flags.intersects(
                        tsr_binder::SymbolFlags::METHOD | tsr_binder::SymbolFlags::ACCESSOR,
                    )
                {
                    return Some(());
                }
                let crate::objects::Member::Property { printed, .. } = member else {
                    unreachable!("certified original property slot");
                };
                let property_type = self.property_type(&property);
                // Primitive/literal slots have no site-dependent symbol.
                // Keep the actual writer's source-reuse display, including
                // single-quoted const literals, rather than reformatting it.
                if matches!(
                    self.store.get(property_type).data,
                    TypeData::Intrinsic { .. }
                        | TypeData::StringLiteral(_)
                        | TypeData::NumberLiteral(_)
                        | TypeData::BigIntLiteral(_)
                        | TypeData::BooleanLiteral(_)
                ) {
                    return Some(());
                }
                // Native serializeTypeForDeclaration can reuse an original
                // SingleCallSignature initializer. pseudoTypeToNode builds
                // its signature directly, without visiting the callable
                // object identity; the return still visits semantic objects.
                // Certify that exact source, not an arbitrary callable slot
                // or a mapper's image, before taking the same print route.
                let initializer = property.checked_declaration.and_then(|declaration| {
                    // serializeTypeForDeclaration (nodebuilderimpl.go:2181)
                    // requires source/semantic pseudo-type equivalence. A
                    // merged symbol's surviving type need not match its source
                    // reuse declaration. Keep the final checked slot, but use
                    // ordinary visited TypeId serialization for such symbols.
                    if self.binder.symbols().get(property.origin?).declarations.len() != 1 {
                        return None;
                    }
                    let tsr_ast::Node::PropertyAssignment(assignment) =
                        self.node_map.get(declaration)?
                    else {
                        return None;
                    };
                    assignment.initializer
                });
                let source_signature = initializer.and_then(|initializer| {
                    if !matches!(
                        initializer,
                        tsr_ast::Expression::ArrowFunction(_)
                            | tsr_ast::Expression::FunctionExpression(_)
                    ) {
                        return None;
                    }
                    let symbol = self.completed_callable_symbol(property_type)?;
                    if self.binder.symbol_of(initializer.node_id()?) != Some(symbol) {
                        return None;
                    }
                    let [signature] = self.signature_types.get(&property_type)?.as_slice() else {
                        return None;
                    };
                    (signature.declaration == initializer.node_id()?).then(|| signature.clone())
                });
                let source_object = initializer.is_some_and(|initializer| {
                    matches!(initializer, tsr_ast::Expression::ObjectLiteralExpression(_))
                        && matches!(self.store.get(property_type).data,
                            TypeData::Named { members: Some(owner), .. }
                                if initializer.node_id().and_then(|node| self.binder.symbol_of(node)) == Some(owner))
                });
                *printed = if let Some(signature) = source_signature {
                    self.signature_to_string_at(&signature, reference)
                } else if source_object {
                    self.object_literal_text_at(property_type, reference, false)
                        .or_else(|| self.type_to_string_at(property_type, reference))?
                } else {
                    self.type_to_string_at(property_type, reference)?
                };
                Some(())
            })
            .map(|()| crate::objects::render_object_type(&members));
        if visit_identity {
            self.rendering_composites.remove(&id);
        }
        result
    }

    /// Certify the existing producer plan's semantic slot association, not its
    /// rendered type text. Source keys retain producer order in both published
    /// tables. A replaced assignment must carry the producer's exact surviving
    /// checked declaration; a merged symbol's first declaration cannot certify
    /// it. Only one complementary get/set pair may merge without that key.
    /// Private names, unresolved computed keys and mixed-kind collisions decline.
    /// Stored printed names are an additional association check after matching
    /// source `SymbolId`, canonical member key and method/property kind.
    fn object_member_plan_matches_source(
        &self,
        literal: &tsr_ast::ObjectLiteralExpression<'_>,
        members: &[crate::objects::Member],
        properties: &[crate::objects::AnonymousProperty],
    ) -> bool {
        const ASSIGNMENT: u8 = 1;
        const SHORTHAND: u8 = 2;
        const METHOD: u8 = 4;
        const GET: u8 = 8;
        const SET: u8 = 16;
        let mut source_slots = Vec::new();
        for element in literal.properties {
            let (name, kind) = match element {
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(node) => {
                    (node.name, ASSIGNMENT)
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(node) => {
                    (node.name, SHORTHAND)
                }
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(node) => (node.name, METHOD),
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(node) => (node.name, GET),
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(node) => (node.name, SET),
                tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => return false,
            };
            if matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_))
                || matches!(name, tsr_ast::PropertyName::ComputedPropertyName(computed)
                    if !matches!(computed.expression,
                        Some(tsr_ast::Expression::StringLiteral(_) | tsr_ast::Expression::NumericLiteral(_))))
            {
                return false;
            }
            let Some(declaration) = element.node_id() else {
                return false;
            };
            let Some(symbol) = self.binder.symbol_of(declaration) else {
                return false;
            };
            let key = self.binder.symbols().get(symbol).name;
            if let Some((previous, previous_kind, checked_declaration)) = source_slots
                .iter_mut()
                .find(|(previous, _, _)| self.binder.symbols().get(*previous).name == key)
            {
                if *previous != symbol {
                    return false;
                }
                match (*previous_kind, kind) {
                    (ASSIGNMENT, ASSIGNMENT) => *checked_declaration = Some(declaration),
                    (GET, SET) | (SET, GET) => *previous_kind |= kind,
                    _ => return false,
                }
            } else {
                source_slots.push((symbol, kind, (kind == ASSIGNMENT).then_some(declaration)));
            }
        }
        if source_slots.len() != properties.len() || members.len() != properties.len() {
            return false;
        }
        source_slots.iter().zip(properties).zip(members).all(
            |((&(symbol, kind, checked_declaration), property), member)| {
                if property.origin != Some(symbol)
                    || property.name != self.binder.symbols().get(symbol).name
                    || property.method != (kind == METHOD)
                    || property.checked_declaration != checked_declaration
                {
                    return false;
                }
                match member {
                    crate::objects::Member::Property { name, .. } => name == &property.printed_name,
                    crate::objects::Member::Method { name, .. } => {
                        kind == METHOD && name == &property.printed_name
                    }
                    _ => false,
                }
            },
        )
    }

    pub(crate) fn union_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let TypeData::Union { types, symbol: None, .. } = &self.store.get(id).data else {
            return None;
        };
        // Native typeToTypeNode formats the union before recursively naming
        // its slots. Keep the existing nullable order and boolean collapse;
        // aliases and written origins are handled before this inferred arm.
        // The mint's completed provenance certifies this display plan. Opaque
        // overrides cannot be reconstructed from the semantic list alone.
        if !self.store.has_union_display_plan(id) {
            return None;
        }
        if !self.rendering_composites.insert(id) {
            return None;
        }
        let parts = crate::unions::union_print_parts(&self.store, types);
        let result = parts
            .into_iter()
            .map(|part| {
                let member = match part {
                    crate::unions::UnionPrintPart::Type(member) => member,
                    crate::unions::UnionPrintPart::Keyword(text) => return Some(text.to_string()),
                };
                let text = self.type_to_string_at(member, reference)?;
                let parentheses =
                    crate::unions::union_constituent_needs_parentheses(&self.store, member);
                Some(if parentheses { format!("({text})") } else { text })
            })
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join(" | "));
        self.rendering_composites.remove(&id);
        result
    }

    /// Pinned tsgo 5b1047d: createTypeNodesFromResolvedType /
    /// addPropertyToElementList. Consume the already-completed type-literal
    /// image, keyed by `TypeId` in this Checker, without resolving members again
    /// or changing its mapper/alias identity. Nothing is cached by spelling or
    /// site. The existing per-print guard is removed after success or refusal;
    /// each signature inherits the naming scope and isolates its allocations
    /// from siblings. Indices and whole written-node overrides keep their
    /// existing renderer until their complete display structure is available.
    pub(crate) fn type_literal_text_at(
        &mut self,
        id: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let origin = *self.type_literal_origins.get(&id)?;
        let tsr_ast::Node::TypeLiteralNode(node) = self.node_map.get(origin)? else {
            return None;
        };
        if node
            .members
            .iter()
            .any(|member| matches!(member, tsr_ast::TypeElement::IndexSignatureDeclaration(_)))
            || self.object_literal_index_infos.get(&id).is_some_and(|infos| !infos.is_empty())
        {
            return None;
        }
        let mut single_quoted = false;
        crate::signatures::written_type_literal_text(node, &mut single_quoted, &mut false);
        if single_quoted {
            return None;
        }
        let properties = self.anonymous_properties.get(&id)?.0.clone();
        let signatures = self.signature_types.get(&id).cloned().unwrap_or_default();
        // Preserve the existing node kind of single call/construct literals.
        if properties.is_empty() && signatures.len() == 1 {
            return None;
        }
        if !self.rendering_composites.insert(id) {
            return None;
        }
        let mut members = Vec::new();
        let mut claimed = rustc_hash::FxHashSet::default();
        for signature in signatures {
            members.push(crate::objects::Member::Signature {
                printed: self.type_literal_signature_at(signature, reference, &mut claimed),
            });
        }
        for property in properties {
            let property_type = self.property_type(&property);
            if property.method {
                let Some(signatures) = self.signature_types.get(&property_type).cloned() else {
                    self.rendering_composites.remove(&id);
                    return None;
                };
                // A method's overload set has its own name claims, not those
                // of a sibling method or the object's call signatures.
                let mut claimed = rustc_hash::FxHashSet::default();
                for signature in signatures {
                    let text = self.type_literal_signature_at(signature, reference, &mut claimed);
                    members.push(crate::objects::Member::Signature {
                        printed: format!(
                            "{}{}{text}",
                            property.printed_name,
                            if property.optional { "?" } else { "" },
                        ),
                    });
                }
            } else if let Some([getter, setter]) =
                self.type_literal_accessor_pair_at(&property, property_type, reference)
            {
                members.push(crate::objects::Member::Signature { printed: getter });
                members.push(crate::objects::Member::Signature { printed: setter });
            } else {
                let annotation = property.origin.and_then(|symbol| {
                    let declaration = *self.binder.symbols().get(symbol).declarations.first()?;
                    match self.node_map.get(declaration)? {
                        tsr_ast::Node::PropertySignatureDeclaration(node) => node.r#type,
                        tsr_ast::Node::GetAccessorDeclaration(node) => node.r#type,
                        _ => None,
                    }
                });
                // `serializeTypeForDeclaration`'s reuse arm first: an
                // equivalent annotation is re-emitted at the site. The two
                // arms after it keep their earlier spellings where the
                // visitor refuses the node: an alias the annotation names,
                // and the producer's written-node precedence, read from the
                // declaration rather than by comparing two rendered strings
                // (an unresolved annotation keeps its published spelling).
                let printed = if let Some(text) = annotation.and_then(|annotation| {
                    self.reused_property_annotation_text_at(
                        annotation,
                        property_type,
                        property.optional,
                        reference,
                    )
                }) {
                    text
                } else if let Some(alias) = annotation.and_then(|annotation| {
                    self.annotation_alias_text_at(annotation, property_type, reference)
                }) {
                    alias
                } else if annotation.is_some_and(|annotation| {
                    let semantic = self.get_type_from_type_node(annotation);
                    self.is_error(semantic)
                        || (matches!(
                            annotation,
                            tsr_ast::TypeNode::TypeLiteralNode(_)
                                | tsr_ast::TypeNode::ArrayTypeNode(_)
                                | tsr_ast::TypeNode::UnionTypeNode(_)
                        ) && self.written_annotation_text(annotation).is_some())
                }) {
                    self.property_printed_type(&property).into_owned()
                } else if let Some(text) = self.type_to_string_at(property_type, reference) {
                    text
                } else {
                    self.property_printed_type(&property).into_owned()
                };
                members.push(crate::objects::Member::Property {
                    name: property.printed_name,
                    optional: property.optional,
                    readonly: property.readonly,
                    printed,
                });
            }
        }
        self.rendering_composites.remove(&id);
        Some(crate::objects::render_object_type(&members))
    }

    fn type_literal_signature_at(
        &mut self,
        signature: crate::signatures::Signature,
        reference: tsr_ast::NodeId,
        claimed: &mut rustc_hash::FxHashSet<String>,
    ) -> String {
        let names_depth = self.render_type_parameter_names.allocations.len();
        let signature = self.rename_type_parameters_for_site(signature, reference, claimed);
        let scope_depth = self.render_type_parameter_scope.len();
        self.push_render_type_parameter_scope(&signature);
        let text = self.signature_member_text_at(&signature, reference);
        self.render_type_parameter_scope.truncate(scope_depth);
        self.render_type_parameter_names.allocations.truncate(names_depth);
        text
    }

    /// `computeModuleSpecifiers`' first arm (`modulespecifiers/specifiers.go:369`):
    /// the first entry of `importing`'s `Imports()` that resolves to `target`
    /// names it with its own text — unless its usage mode and the file's
    /// default resolution mode are both set and differ, in which case no
    /// existing import is used at all (upstream `continue`s the module-path
    /// loop rather than trying a later import).
    ///
    /// `Imports()` order is statement-level specifiers (import, export and
    /// `import x = require` declarations) in source order, then dynamic
    /// `import()` calls and literal import types by position
    /// (`collectExternalModuleReferences`, mirrored by
    /// `tsr_parser::collect_external_module_references`). The dynamic half
    /// is a tree walk, taken only when no statement-level import matched; no
    /// cache, since the question is asked only for a module the reference
    /// cannot name through an alias. JS `require()` calls and JSDoc import
    /// types are not read (a JS target reached only that way falls through to
    /// the computed specifier). r5-modules §4.
    pub(crate) fn existing_import_specifier(
        &self,
        importing: tsr_ast::NodeId,
        target: tsr_ast::NodeId,
        override_mode: tsr_core::ModuleKind,
    ) -> Option<String> {
        use tsr_ast::{Expression, ModuleReference, Node, Statement};
        let host = self.module_host?;
        let Some(Node::SourceFile(source)) = self.node_map.get(importing) else {
            return None;
        };
        let resolves = |usage: tsr_ast::NodeId, text: &str| {
            let mode = host.mode_for_usage_location(importing, usage);
            (host.resolved_module_in_mode(importing, text, mode) == Some(target)).then_some(mode)
        };
        let statement_level = source.statements.iter().find_map(|statement| {
            let specifier = match statement {
                Statement::ImportDeclaration(node) => node.module_specifier,
                Statement::ExportDeclaration(node) => node.module_specifier,
                Statement::ImportEqualsDeclaration(node) => match node.module_reference {
                    Some(ModuleReference::ExternalModuleReference(external)) => external.expression,
                    _ => None,
                },
                _ => None,
            };
            let Some(Expression::StringLiteral(literal)) = specifier else { return None };
            let usage = literal.node_id?;
            resolves(usage, literal.text).map(|mode| (literal.text, mode))
        });
        let found = statement_level.or_else(|| {
            let mut dynamic: Vec<(u32, tsr_ast::NodeId, &str)> = Vec::new();
            let mut stack = vec![Node::from(source)];
            let mut children = Vec::new();
            while let Some(node) = stack.pop() {
                let literal = match node {
                    Node::CallExpression(call)
                        if matches!(call.expression,
                            Some(Expression::KeywordExpression(keyword))
                                if keyword.kind == tsr_ast::SyntaxKind::ImportKeyword) =>
                    {
                        match call.arguments.first().map(|argument| Node::from(*argument)) {
                            Some(Node::StringLiteral(literal)) => {
                                literal.node_id.map(|id| (id, literal.text))
                            }
                            Some(Node::NoSubstitutionTemplateLiteral(literal)) => {
                                literal.node_id.map(|id| (id, literal.text))
                            }
                            _ => None,
                        }
                    }
                    Node::ImportTypeNode(import) => match import.argument {
                        Some(tsr_ast::TypeNode::LiteralTypeNode(literal_type)) => {
                            match literal_type.literal {
                                Some(Node::StringLiteral(literal)) => {
                                    literal.node_id.map(|id| (id, literal.text))
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((id, text)) = literal {
                    dynamic.push((self.nodes.span(id).start, id, text));
                }
                children.clear();
                tsr_ast::push_children(node, &mut children);
                stack.extend(children.iter().copied());
            }
            dynamic.sort_by_key(|&(position, _, _)| position);
            dynamic
                .into_iter()
                .find_map(|(_, usage, text)| resolves(usage, text).map(|mode| (text, mode)))
        });
        let (text, mode) = found?;
        let none = tsr_core::ModuleKind::None;
        let target_mode = if override_mode == none {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        if mode != target_mode && mode != none && target_mode != none {
            return None;
        }
        Some(text.to_string())
    }

    /// `module.TryGetJSExtensionForFile` (`module/util.go:178`): the
    /// extension a `.js`-ending specifier gives a file.
    pub(crate) fn js_extension_for_file(&self, path: &str) -> Option<(&'static str, &'static str)> {
        // (input extension, output extension), longest input first.
        const TABLE: [(&str, &str); 11] = [
            (".d.mts", ".mjs"),
            (".d.cts", ".cjs"),
            (".d.ts", ".js"),
            (".mts", ".mjs"),
            (".cts", ".cjs"),
            (".tsx", ""),
            (".ts", ".js"),
            (".mjs", ".mjs"),
            (".cjs", ".cjs"),
            (".jsx", ".jsx"),
            (".js", ".js"),
        ];
        let &(input, output) = TABLE.iter().find(|(input, _)| path.ends_with(input))?;
        if input == ".tsx" {
            let output = if self.jsx_emit == tsr_core::JsxEmit::Preserve { ".jsx" } else { ".js" };
            return Some((input, output));
        }
        Some((input, output))
    }
}

impl<'a> Checker<'a, '_> {
    /// `addPropertyToElementList`'s accessor arm (`nodebuilderimpl.go:2524`):
    /// an accessor property whose read type differs from its write type
    /// (`getWriteTypeOfSymbol`) prints as its getter and setter signatures,
    /// `{ get foo(): number; set foo(v: number | string); }`, each built by
    /// `signatureToSignatureDeclarationHelper` from the accessor's own
    /// declaration, so the written annotations are reused. Error types on
    /// either side keep the property form. A type literal's accessors have
    /// no class parent, so the class-only arms do not apply.
    fn type_literal_accessor_pair_at(
        &mut self,
        property: &crate::objects::AnonymousProperty,
        read: TypeId,
        reference: tsr_ast::NodeId,
    ) -> Option<[String; 2]> {
        let symbol = property.origin?;
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let getter = declarations.iter().copied().find(|&id| {
            matches!(self.node_map.get(id), Some(tsr_ast::Node::GetAccessorDeclaration(_)))
        })?;
        let setter = declarations.iter().copied().find(|&id| {
            matches!(self.node_map.get(id), Some(tsr_ast::Node::SetAccessorDeclaration(_)))
        })?;
        let write_parameter = self.accessor_write_parameter(symbol)?;
        let write = self.parameter_type(&write_parameter);
        if read == write || self.is_error(read) || self.is_error(write) {
            return None;
        }
        let name = &property.printed_name;
        let getter_signature = self.get_signature_from_declaration(getter)?;
        let getter_return = getter_signature
            .written_return
            .and_then(|written| {
                let current = self
                    .get_return_type_of_signature(&getter_signature)
                    .unwrap_or(getter_signature.r#type);
                self.written_annotation_text_at(written, current, reference)
            })
            .or_else(|| self.type_to_string_at(read, reference))
            .unwrap_or_else(|| self.type_to_string(read));
        let setter_signature = self.get_signature_from_declaration(setter)?;
        let parameter = setter_signature.parameters.first()?;
        let parameter_type = self.parameter_type(parameter);
        let parameter_text = parameter
            .written_text
            .and_then(|written| self.written_annotation_text_at(written, parameter_type, reference))
            .or_else(|| self.type_to_string_at(parameter_type, reference))
            .unwrap_or_else(|| self.type_to_string(parameter_type));
        Some([
            format!("get {name}(): {getter_return}"),
            format!("set {name}({}: {parameter_text})", parameter.name),
        ])
    }

    /// `serializeTypeForDeclaration`'s reuse arm for a property signature
    /// (`nodebuilderimpl.go:2231`, reached from `addPropertyToElementList`):
    /// the written annotation is re-emitted when `pseudoTypeEquivalentToType`
    /// holds — its type is the property's, or, for an optional property,
    /// the property's once the optionality is forgiven (`:2249`). Emitted at
    /// `reference` by the existing-node visitor, so an unannotated parameter
    /// of a written function type gains `: any` (`nodecopy.go:660`) where
    /// the type's own serialization prints the parameter's type.
    fn reused_property_annotation_text_at(
        &mut self,
        annotation: tsr_ast::TypeNode<'a>,
        property_type: TypeId,
        optional: bool,
        reference: tsr_ast::NodeId,
    ) -> Option<String> {
        let semantic = self.get_type_from_type_node(annotation);
        if self.is_error(semantic) {
            return None;
        }
        let equivalent = semantic == property_type
            || (optional
                && self.strict_null_checks
                && self.get_optional_type(semantic, true) == property_type);
        if !equivalent {
            return None;
        }
        let written = self.reuse_annotation(annotation, property_type)?;
        self.written_annotation_text_at(written, property_type, reference)
    }
}

/// Render a type.
///
/// Ported from `Checker.typeToString`.
#[must_use]
pub fn type_to_string(ty: &Type) -> String {
    match &ty.data {
        TypeData::Intrinsic { name } => (*name).to_string(),
        TypeData::StringLiteral(value) => quote(value),
        TypeData::BigIntLiteral(text) => format!("{text}n"),
        TypeData::BooleanLiteral(value) => value.to_string(),
        // One body, three unrelated reasons — kept as one arm because clippy's
        // `match_same_arms` is a workspace gate and splitting them to hold three
        // comments would fail it:
        //
        // - a **number** prints its normalised text, since `1.0` and `0x1` are
        //   both the type `1` and source text cannot be used;
        // - a **named** type prints the form computed when it was created — see
        //   `TypeData::Named` for why that is a renderer divergence rather than a
        //   data-model one;
        // - a **union** prints a form computed when it was *built*, because that
        //   form depends on the constituents and this function takes a single
        //   `Type` with no way back to the store. See `TypeData::Union::text`.
        TypeData::NumberLiteral(text)
        | TypeData::Named { text, .. }
        | TypeData::EnumLiteral { text, .. }
        // - an **anonymous** object type prints `typeof C` or its call
        //   signature, computed at creation for the same reason: rendering a
        //   signature needs the parameter and return *types*, and this function
        //   has only a `Type`.
        | TypeData::Union { text, .. }
        | TypeData::Intersection { text, .. }
        | TypeData::Anonymous { text, .. } => text.clone(),
    }
}

/// Whether a type prints as a **single token** rather than as its own
/// structure — a name, or a keyword.
///
/// The question every parenthesiser here is really asking. Upstream never asks
/// it, because it decides on the *node kind* the builder emitted
/// (`GetTypeNodePrecedence`, `ast/precedence.go:655`) and a named union is a
/// `TypeReferenceNode` while an anonymous one is a `UnionTypeNode` — two kinds,
/// two precedences, no predicate needed. This port computes text at creation and
/// has only the type, so the same distinction has to be recovered from what the
/// type carries.
///
/// Two ways a `Union` or `Intersection` prints as one token:
///
/// - **A type alias names it.** `type Tagged = A & B` prints `Tagged`, which the
///   node builder emits as a `TypeReferenceNode` — `NonArray`, the highest
///   precedence, never parenthesised.
/// - **It is `boolean`.** `booleanType` is the union `false | true`
///   ([`crate::unions::create_boolean_type`]) and prints as the keyword, which
///   the builder emits as `KindBooleanKeyword`.
///
/// Both were found by parenthesising without them: the first cost 19 lines
/// across three cases (`(TaggedString1) | (TaggedString2)` for
/// `TaggedString1 | TaggedString2`), the second is pinned by a test rather than
/// by the corpus.
pub(crate) fn prints_as_a_single_token(ty: &Type) -> bool {
    match &ty.data {
        TypeData::Union { symbol, .. } | TypeData::Intersection { symbol, .. } => {
            symbol.is_some() || ty.flags.contains(TypeFlags::BOOLEAN)
        }
        _ => false,
    }
}

/// Render a string literal type's value the way TypeScript prints one.
///
/// Double quotes, with `\`, `"`, and the C0 controls escaped. TypeScript's own
/// `escapeString` is what this mirrors; the set here covers what appears in the
/// corpus and deliberately stops short of the full table — a character outside it
/// is emitted raw, which is visible as a baseline mismatch rather than as silent
/// corruption. `bd tsr-4sc.1`.
///
/// `pub(crate)` so an object literal's non-identifier property name — `{ "a-b": 1 }`
/// printing `{ "a-b": number; }` — quotes through *this* table rather than a second
/// copy of it. That matters precisely **because** the table is incomplete: two
/// copies would both have to be corrected when `bd tsr-4sc.1` lands, and nothing
/// would fail if only one were. Same argument `render_object_type` records for
/// keeping one object renderer — separate ones are how a port ends up printing
/// `{ a: string }` in one position and `{ a: string; }` in another.
pub(crate) fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // escapeStringWorker (`printer/utilities.go:85`) escapes these
            // three under every quote and flag set, NeverAsciiEscape
            // included, so a string literal type that holds one prints its
            // escape (`allowUnescapedParagraphAndLineSeparatorsInStringLiteral`).
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            '\u{0085}' => out.push_str("\\u0085"),
            // The rest of `escapedCharsMap` (`printer/utilities.go:41`) plus
            // the NUL rule (`:150`): `\0` prints `\0` unless a digit follows —
            // then `\x00`, so the result cannot re-parse as an octal. The
            // corpus pinned these as the `templateString*Escapes` W2 rows
            // (`"\t\n\v\f\r"` wanted where `` printed).
            '\u{0B}' => out.push_str("\\v"),
            '\u{0C}' => out.push_str("\\f"),
            '\u{08}' => out.push_str("\\b"),
            '\0' => {
                if chars.peek().is_some_and(char::is_ascii_digit) {
                    out.push_str("\\x00");
                } else {
                    out.push_str("\\0");
                }
            }
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `escapeNonAsciiString` (internal/printer/utilities.go) for synthesized
/// property names. Supplementary characters use UTF-16 surrogate escapes.
pub(crate) fn quote_ascii(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for character in quote(value).chars() {
        if character.is_ascii() {
            out.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                let _ = write!(out, "\\u{unit:04X}");
            }
        }
    }
    out
}

/// Normalise a numeric literal's source text to the form TypeScript prints.
///
/// TypeScript prints a numeric literal *type* as the ECMAScript `Number::toString`
/// of its value, not as it was written: `1.0`, `1e0` and `0x1` are all the type
/// `1`. So the source text cannot be used directly.
///
/// Decimal integers and fractions, radix literals, numeric separators, and the
/// ECMAScript exponential-notation boundaries are ported. Rust and ECMAScript
/// use the same shortest round-tripping digits but different notation cutoffs,
/// so [`number_to_ecmascript_string`] adjusts only that presentation choice.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "ECMAScript numbers *are* f64, so narrowing an integer literal to f64 \
              is the specified behaviour rather than a defect: `0x20000000000001` is \
              genuinely the same number as `0x20000000000000` in TypeScript, and \
              upstream prints it that way."
)]
pub fn normalise_number(text: &str) -> String {
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();

    // §276: a radix prefix with NO digits — `0x`, `0b`, `0o` — is upstream's
    // scanner-recovery ZERO: `scanNumber` reports "Hexadecimal digit
    // expected" and keeps value 0, so `0x` records `>0x : 0`
    // (`scannerS7.8.3_A6.1_T1`). An empty digit run parses as 0 rather than
    // falling to the keep-the-source arm below, which exists for literals
    // whose VALUE is unrepresentable, not unreadable.
    let radix_value = |rest: &str, radix: u32| {
        if rest.is_empty() {
            Some(0.0)
        } else {
            u128::from_str_radix(rest, radix)
                .ok()
                .map(|v| v as f64)
                .or_else(|| tsr_core::jsnum::wide_radix_value(rest, radix))
        }
    };
    let value = if let Some(rest) =
        cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X"))
    {
        radix_value(rest, 16)
    } else if let Some(rest) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        radix_value(rest, 8)
    } else if let Some(rest) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B")) {
        radix_value(rest, 2)
    } else if cleaned.len() > 1
        && cleaned.starts_with('0')
        && cleaned.bytes().all(|b| b.is_ascii_digit())
        && !cleaned.contains(['8', '9'])
    {
        // §150 rider (`checker-notes-narrow.md`): the LEGACY octal literal —
        // leading zero, every digit octal — reads base 8 (`055` is `45`),
        // the same branch `tsr_core::jsnum::numeric_value` already carries.
        // A non-octal digit (`08`, `09`) or a `.` falls through to decimal.
        u128::from_str_radix(&cleaned[1..], 8).ok().map(|v| v as f64)
    } else {
        // §295, §276's exponent sibling: a DANGLING exponent — `1e`, `1e+`,
        // `1.0e_` (the separator strips to `1.0e`) — is upstream's scanner
        // recovery keeping the mantissa: "Digit expected" reports and the
        // value is the mantissa's (`scannerES3NumericLiteral4/6`,
        // `parser.numericSeparators.decmialNegative` 49/50 all record `1`).
        let trimmed = cleaned
            .strip_suffix(['+', '-'])
            .unwrap_or(&cleaned)
            .strip_suffix(['e', 'E'])
            .map(str::to_string);
        match trimmed {
            Some(mantissa) if !mantissa.is_empty() => mantissa.parse::<f64>().ok(),
            _ => cleaned.parse::<f64>().ok(),
        }
    };

    // An unparseable literal keeps its source text: the scanner already reported
    // it, and inventing a value here would turn a syntax error into a wrong type.
    let Some(value) = value else { return cleaned };

    // Exact integrality is the question, so an epsilon would be wrong: `1.0` must
    // print `1` and `1.0000000000000002` must not.
    #[allow(clippy::float_cmp, reason = "exact integrality is the intended test")]
    let is_integral = value == value.trunc();
    if is_integral && value.abs() < 1e21 {
        // Integral values print without a fractional part, which `{}` on f64 also
        // does — but only for values that fit; the guard above is what keeps this
        // inside the agreed range.
        format!("{value:.0}")
    } else {
        number_to_ecmascript_string(value)
    }
}

/// Apply ECMAScript `Number::toString`'s notation boundaries to Rust's shortest
/// round-tripping decimal digits.
fn number_to_ecmascript_string(value: f64) -> String {
    if !value.is_finite() {
        return tsr_core::jsnum::format_number(value);
    }
    let raw = value.to_string();
    let absolute = value.abs();
    if absolute == 0.0 || (1e-6..1e21).contains(&absolute) {
        return raw;
    }

    let (sign, magnitude) = raw.strip_prefix('-').map_or(("", raw.as_str()), |rest| ("-", rest));
    let Some((first, exponent, tail)) = scientific_parts(magnitude) else { return raw };
    let tail = tail.trim_end_matches('0');
    let mantissa = if tail.is_empty() { first.to_string() } else { format!("{first}.{tail}") };
    let exponent_sign = if exponent >= 0 { "+" } else { "" };
    format!("{sign}{mantissa}e{exponent_sign}{exponent}")
}

/// Split a non-zero, non-exponential decimal into scientific components.
fn scientific_parts(decimal: &str) -> Option<(char, isize, String)> {
    if let Some(fraction) = decimal.strip_prefix("0.") {
        let first_index = fraction.find(|character| character != '0')?;
        let exponent = -isize::try_from(first_index).ok()? - 1;
        let mut digits = fraction[first_index..].chars();
        Some((digits.next()?, exponent, digits.collect()))
    } else {
        let digits: String = decimal.chars().filter(|character| *character != '.').collect();
        let mut digits = digits.chars();
        let first = digits.next()?;
        let tail: String = digits.collect();
        let exponent = isize::try_from(tail.len()).ok()?;
        Some((first, exponent, tail))
    }
}

/// Convert a bigint literal spelling to the decimal value identity TypeScript
/// stores in a bigint literal type.
///
/// This ports `jsnum.ParsePseudoBigInt` (`internal/jsnum/pseudobigint.go`) rather
/// than parsing through a machine integer. Decimal multiply/add makes the result
/// arbitrary-precision without adding a checker-wide bigint dependency.
#[must_use]
pub fn normalise_bigint(text: &str) -> String {
    let (negative, text) = text.strip_prefix('-').map_or((false, text), |rest| (true, rest));
    let cleaned: String = text
        .strip_suffix('n')
        .unwrap_or(text)
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (radix, digits) = if let Some(digits) =
        cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B"))
    {
        (2, digits)
    } else if let Some(digits) = cleaned.strip_prefix("0o").or_else(|| cleaned.strip_prefix("0O")) {
        (8, digits)
    } else if let Some(digits) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        (16, digits)
    } else {
        (10, cleaned.as_str())
    };

    // Little-endian decimal digits. Each source digit multiplies the current
    // value by at most 16, so every intermediate fits comfortably in u16.
    let mut decimal = vec![0_u8];
    for character in digits.chars() {
        let Some(value) = character.to_digit(radix) else {
            // Scanner recovery can leave malformed source text. Preserve the
            // old spelling in that case instead of manufacturing a value.
            return text.strip_suffix('n').unwrap_or(text).to_string();
        };
        let mut carry = value;
        for digit in &mut decimal {
            let next = u32::from(*digit) * radix + carry;
            *digit = (next % 10) as u8;
            carry = next / 10;
        }
        while carry != 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    while decimal.len() > 1 && decimal.last() == Some(&0) {
        decimal.pop();
    }
    let value: String = decimal.iter().rev().map(|digit| char::from(b'0' + digit)).collect();
    if negative && value != "0" { format!("-{value}") } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_literal_type_is_double_quoted_and_escaped() {
        assert_eq!(quote("a"), "\"a\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("a\\b"), "\"a\\\\b\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
        // escapedCharsMap's line terminators other than LF/CR.
        assert_eq!(quote("\u{2028}x\u{2029}\u{0085}"), "\"\\u2028x\\u2029\\u0085\"");
    }

    #[test]
    fn a_numeric_literal_type_prints_its_value_not_its_spelling() {
        // The whole reason source text cannot be used directly.
        assert_eq!(normalise_number("1"), "1");
        assert_eq!(normalise_number("1.0"), "1");
        assert_eq!(normalise_number("1e0"), "1");
        assert_eq!(normalise_number("0x1"), "1");
        assert_eq!(normalise_number("0b101"), "5");
        assert_eq!(normalise_number("0o17"), "15");
        assert_eq!(normalise_number("055"), "45");
        assert_eq!(normalise_number("08"), "8");
        assert_eq!(normalise_number("0.5"), "0.5");
        assert_eq!(normalise_number("1_000"), "1000");
        assert_eq!(normalise_number("1.5"), "1.5");
    }

    #[test]
    fn number_notation_uses_ecmascript_boundaries() {
        assert_eq!(normalise_number("1e20"), "100000000000000000000");
        assert_eq!(normalise_number("1e21"), "1e+21");
        assert_eq!(normalise_number("1.2e35"), "1.2e+35");
        assert_eq!(normalise_number("0.000001"), "0.000001");
        assert_eq!(normalise_number("0.0000001"), "1e-7");
        assert_eq!(normalise_number("-0.00000012"), "-1.2e-7");
    }

    #[test]
    fn bigint_normalisation_is_radix_independent_and_arbitrary_precision() {
        assert_eq!(normalise_bigint("0xC0Bn"), "3083");
        assert_eq!(normalise_bigint("0b010_10_1n"), "21");
        assert_eq!(normalise_bigint("0o1234_567n"), "342391");
        assert_eq!(normalise_bigint("123_456_789n"), "123456789");
        assert_eq!(normalise_bigint("-0x000n"), "0");
        assert_eq!(normalise_bigint("0xn"), "0");
        // One followed by 32 hexadecimal zeroes is 16^32 = 2^128, one
        // greater than u128::MAX. The decimal expectation is derived from
        // that boundary rather than from the implementation under test.
        assert_eq!(
            normalise_bigint("0x100000000000000000000000000000000n"),
            "340282366920938463463374607431768211456"
        );
    }

    #[test]
    fn an_unparseable_literal_keeps_its_text_rather_than_inventing_a_value() {
        assert_eq!(normalise_number("not-a-number"), "not-a-number");
    }

    #[test]
    fn a_duplicate_slot_requires_its_actual_checked_declaration_not_the_merged_first_one() {
        let source = "const value = { item: 17, item: 'last',
            callback: () => 23, callback: () => true };";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/survivor.ts", text: source },
        );
        let mut nodes = vec![parsed.node_map.get(root).unwrap()];
        let mut literal = None;
        while let Some(node) = nodes.pop() {
            if let tsr_ast::Node::ObjectLiteralExpression(node) = node {
                literal = Some(node);
            }
            tsr_ast::push_children(node, &mut nodes);
        }
        let literal = literal.unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let id = checker.check_expression(tsr_ast::Expression::ObjectLiteralExpression(literal));
        let data = checker.store.get(id).data.clone();
        let members = &checker.object_literal_members[&id];
        let mut properties = checker.anonymous_properties[&id].0.clone();
        assert!(checker.object_member_plan_matches_source(literal, members, &properties));
        let slot = properties.iter().position(|property| property.name == "callback").unwrap();
        let surviving = properties[slot].checked_declaration;
        let first = bound.symbols().get(properties[slot].origin.unwrap()).value_declaration;
        assert_ne!(surviving, first);
        properties[slot].checked_declaration = first;
        assert!(!checker.object_member_plan_matches_source(literal, members, &properties));
        properties[slot].checked_declaration = None;
        assert!(!checker.object_member_plan_matches_source(literal, members, &properties));
        properties[slot].checked_declaration = surviving;
        properties[slot].name = "item".to_string();
        assert!(!checker.object_member_plan_matches_source(literal, members, &properties));
        assert_eq!(checker.store.get(id).data, data);
        let regular = checker.get_regular_type_of_object_literal(id);
        let widened = checker.widen_object_literal_freshness(id);
        assert_ne!(id, regular);
        assert_ne!(id, widened);
        assert_ne!(regular, widened);
        let mut images =
            [id, regular, widened].map(|image| (image, checker.store.get(image).data.clone()));
        for _ in 0..4 {
            images.reverse();
            for (image, original) in &images {
                assert_eq!(
                    checker.type_to_string_at(*image, literal.node_id.unwrap()).as_deref(),
                    Some("{ item: string; callback: () => boolean; }")
                );
                assert_eq!(&checker.store.get(*image).data, original);
            }
            assert_eq!(checker.get_regular_type_of_object_literal(id), regular);
            assert_eq!(checker.widen_object_literal_freshness(id), widened);
        }
    }
}
