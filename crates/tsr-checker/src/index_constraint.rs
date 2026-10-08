//! TS2411 — `Property '{0}' of type '{1}' is not assignable to '{2}' index type
//! '{3}'.` and TS2413 — `'{0}' index type '{1}' is not assignable to '{2}'
//! index type '{3}'.`
//!
//! `checkIndexConstraints` (`checker.go:4786`), run from
//! `checkClassLikeDeclaration` on the instance and static sides and from
//! `checkInterfaceDeclaration` once per symbol: every property of the type,
//! own and inherited, must be assignable to each applicable index signature,
//! and each index signature to the others that apply to its key.
//!
//! The index infos, property names and property types are the type-level
//! queries (`get_index_infos_of_type`, `get_property_names_of_type`,
//! `get_type_of_property_of_type`), so inherited signatures and properties
//! come from the same tables member access reads. The error node follows
//! upstream's locality rule: the property's declaration when the type owns
//! it, else the index signature's when the type owns that, else — for an
//! interface whose bases never carry both — the interface declaration.
//! A negative is reported only when the relation decides `NotRelated`.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    check::{has_modifier, modifiers_of},
    checker::Checker,
    flags::TypeFlags,
    index_signatures::IndexInfo,
    relater::Relation,
    relater::Ternary,
    types::TypeId,
};

impl<'a> Checker<'a, '_> {
    /// TS2374 — `Duplicate index signature for type '{0}'.`
    ///
    /// `checkTypeForDuplicateIndexSignatures` (`checker.go:4904`), reached from
    /// `checkTypeLiteral` and, once per symbol, from
    /// `checkClassOrInterfaceForDuplicateIndexSignatures` (`checker.go:4896`).
    /// The signatures are the declarations of the symbol's `__index` **member**
    /// (`getIndexSymbol` = `getMembersOfSymbol(symbol)["__index"]`), so every
    /// merged class/interface declaration contributes and a class's `static`
    /// signatures (bound into its exports) do not. Each signature with exactly
    /// one typed parameter files itself under every constituent of the written
    /// key type (`getTypeFromTypeNode(...).Distributed()`), and every signature
    /// of a key type filed more than once is reported, with the key printed by
    /// `typeToString`. Late-bound `__index` declarations are not
    /// `IndexSignatureDeclaration`s and never take part.
    ///
    /// The once-per-symbol flag (`links.indexSignaturesChecked`) is replaced by
    /// locality: every declaration computes the symbol-wide grouping and
    /// reports only the signatures it owns, which yields each native
    /// diagnostic exactly once without a side table, including for a
    /// declaration merged into a default-library interface that is never
    /// checked itself. `docs/parity/notes/r4-index2.md` §1.
    pub(crate) fn check_duplicate_index_signatures(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let carriers: Vec<NodeId> = match self.nodes.kind(node) {
            SyntaxKind::TypeLiteral => vec![node],
            SyntaxKind::ClassDeclaration
            | SyntaxKind::ClassExpression
            | SyntaxKind::InterfaceDeclaration => {
                let Some(symbol) = self.binder.symbol_of(node) else { return };
                let symbol = self.binder.merged_symbol(symbol);
                self.binder
                    .symbols()
                    .get(symbol)
                    .declarations
                    .iter()
                    .copied()
                    .filter(|&declaration| {
                        matches!(
                            self.nodes.kind(declaration),
                            SyntaxKind::ClassDeclaration
                                | SyntaxKind::ClassExpression
                                | SyntaxKind::InterfaceDeclaration
                        )
                    })
                    .collect()
            }
            _ => return,
        };
        let mut signatures: Vec<(NodeId, bool, tsr_ast::TypeNode<'a>)> = Vec::new();
        for carrier in carriers {
            let owned = carrier == node;
            let mut push = |signature: &tsr_ast::IndexSignatureDeclaration<'a>| {
                if let (Some(at), [parameter]) = (signature.node_id, signature.parameters)
                    && let Some(key) = parameter.r#type
                {
                    signatures.push((at, owned, key));
                }
            };
            let (class_members, type_members) = match self.node_map.get(carrier) {
                Some(Node::ClassDeclaration(class)) => (class.members, &[][..]),
                Some(Node::ClassExpression(class)) => (class.members, &[][..]),
                Some(Node::InterfaceDeclaration(interface)) => (&[][..], interface.members),
                Some(Node::TypeLiteralNode(literal)) => (&[][..], literal.members),
                _ => continue,
            };
            for member in class_members {
                if let tsr_ast::ClassElement::IndexSignatureDeclaration(signature) = member
                    && !has_modifier(signature.modifiers, SyntaxKind::StaticKeyword)
                {
                    push(signature);
                }
            }
            for member in type_members {
                if let tsr_ast::TypeElement::IndexSignatureDeclaration(signature) = member {
                    push(signature);
                }
            }
        }
        if signatures.len() < 2 || !signatures.iter().any(|&(_, owned, _)| owned) {
            return;
        }
        // `indexSignatureMap`, keyed by the distributed key type. A key this
        // port could not compute (`error`) is declined, not filed as `any`.
        let mut groups: Vec<(TypeId, Vec<(NodeId, bool)>)> = Vec::new();
        for (at, owned, key) in signatures {
            let key = self.get_type_from_type_node(key);
            let keys = match &self.store.get(key).data {
                crate::types::TypeData::Union { types, .. } => types.clone(),
                _ => vec![key],
            };
            for key in keys {
                if key == self.intrinsics.error {
                    continue;
                }
                match groups.iter_mut().find(|(seen, _)| *seen == key) {
                    Some((_, declarations)) => declarations.push((at, owned)),
                    None => groups.push((key, vec![(at, owned)])),
                }
            }
        }
        for (key, declarations) in groups {
            if declarations.len() < 2 {
                continue;
            }
            let key_text = self.type_to_string(key);
            for (at, owned) in declarations {
                if !owned {
                    continue;
                }
                let Some(file) = self.source_file_of_for_diagnostics(at) else { continue };
                let span = self.error_span(at);
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE_0,
                        span,
                        [key_text.clone()],
                    ),
                );
            }
        }
    }

    /// `checkIndexConstraints`' four call sites (`checker.go:4786`):
    /// `checkClassLikeDeclaration` runs it on the instance type and on the
    /// static side (`checker.go:4387`), `checkInterfaceDeclaration` once per
    /// symbol on the declared type (`checker.go:5013`), `checkTypeLiteral` on
    /// the literal's type (`checker.go:3137`).
    ///
    /// The interface arm is guarded by `links.interfaceChecked`, a once-per-
    /// symbol flag set by whichever declaration is checked first; this runs on
    /// the symbol's first interface declaration, which is the one a file-order
    /// check reaches first.
    pub(crate) fn check_index_constraints(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return };
        let symbol = self.binder.merged_symbol(symbol);
        match self.nodes.kind(node) {
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                let instance = self.get_declared_type_of_class_or_interface(symbol);
                self.check_index_constraints_of_type(instance, symbol, false);
                let static_side = self.get_type_of_symbol(symbol);
                self.check_index_constraints_of_type(static_side, symbol, true);
            }
            SyntaxKind::InterfaceDeclaration => {
                let first = self.binder.symbols().get(symbol).declarations.iter().copied().find(
                    |&declaration| self.nodes.kind(declaration) == SyntaxKind::InterfaceDeclaration,
                );
                if first != Some(node) {
                    return;
                }
                let declared = self.get_declared_type_of_class_or_interface(symbol);
                self.check_index_constraints_of_type(declared, symbol, false);
            }
            // `checkTypeLiteral` (`checker.go:3134`): the literal's own type,
            // owned by its `__type` symbol.
            SyntaxKind::TypeLiteral => {
                let Some(Node::TypeLiteralNode(literal)) = self.node_map.get(node) else { return };
                // `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode` runs
                // unconditionally upstream; resolving it is where a circular
                // `typeof` member reports TS2502 (`recursiveTypesWithTypeof`).
                let ty = self.get_type_from_type_node(tsr_ast::TypeNode::TypeLiteralNode(literal));
                // A literal has no base types, so its index infos come only
                // from its own index signatures and non-bindable computed
                // members; without either there is nothing to check, and its
                // properties need not be enumerated.
                if !literal.members.iter().any(|member| {
                    matches!(member, tsr_ast::TypeElement::IndexSignatureDeclaration(_))
                        || member.node_id().and_then(|id| self.declaration_name_of(id)).is_some_and(
                            |name| self.nodes.kind(name) == SyntaxKind::ComputedPropertyName,
                        )
                }) {
                    return;
                }
                self.check_index_constraints_of_type(ty, symbol, false);
            }
            _ => {}
        }
    }

    /// `checkIndexConstraints` (`checker.go:4786`) for one type: every property
    /// of the type, own and inherited, against each applicable index signature;
    /// a class's members with non-bindable computed names; and, where the type
    /// has more than one index signature, each signature against the others.
    ///
    /// No cache or side table: the property names, index infos and property
    /// types are the existing `get_property_names_of_type`,
    /// `get_index_infos_of_type` and `get_type_of_property_of_type` queries,
    /// run once per checked declaration. A type whose property table cannot be
    /// enumerated (`None`) is declined rather than checked partially.
    fn check_index_constraints_of_type(&mut self, ty: TypeId, owner: SymbolId, is_static: bool) {
        let Some(infos) = self.get_index_infos_of_type(ty) else { return };
        if infos.is_empty() {
            return;
        }
        let Some(names) = self.get_property_names_of_type(ty) else { return };
        for name in names {
            let Some(property) = self.get_property_of_type(ty, &name) else { continue };
            // `prop.Flags&ast.SymbolFlagsPrototype`: the class value's
            // synthetic `prototype`, which this port enumerates by name on the
            // static side without a binder symbol of its own.
            if is_static
                && (name == "prototype" && self.class_static_symbol(ty).is_some()
                    || self.binder.symbols().get(property).flags.intersects(SymbolFlags::PROTOTYPE))
            {
                continue;
            }
            let name_type = self.literal_type_from_property(property, &name);
            let Some(property_type) = self.get_type_of_property_of_type(ty, &name) else {
                continue;
            };
            let property_type = self.remove_missing_type(property_type);
            self.check_index_constraint_for_property(
                ty,
                owner,
                &infos,
                property,
                &name,
                name_type,
                property_type,
            );
        }
        let value_declaration = self.binder.symbols().get(owner).value_declaration;
        if let Some(class) = value_declaration {
            let members: Vec<NodeId> = match self.node_map.get(class) {
                Some(Node::ClassDeclaration(class)) => {
                    class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                Some(Node::ClassExpression(class)) => {
                    class.members.iter().filter_map(|m| Node::from(*m).node_id()).collect()
                }
                _ => Vec::new(),
            };
            for member in members {
                // `ast.IsStatic(member) == isStaticIndex && !c.hasBindableName(member)`.
                let member_is_static =
                    self.node_map.get(member).and_then(modifiers_of).is_some_and(|modifiers| {
                        has_modifier(modifiers, SyntaxKind::StaticKeyword)
                    });
                if member_is_static != is_static {
                    continue;
                }
                let Some(expression) = self.non_bindable_computed_name(member) else { continue };
                let Some(member_symbol) = self.binder.symbol_of(member) else { continue };
                let name_type = self.check_expression(expression);
                let member_type = self.get_type_of_symbol(member_symbol);
                let member_type = self.remove_missing_type(member_type);
                // `symbolToString(prop)` of an anonymous `__computed` symbol
                // is its declaration name as written (`getNameOfSymbolAsWritten`
                // -> `getTextOfNode`): `Property '[+s]' of type ...`.
                let Some(name) = self.computed_member_name_text(member) else { continue };
                self.check_index_constraint_for_property(
                    ty,
                    owner,
                    &infos,
                    member_symbol,
                    &name,
                    name_type,
                    member_type,
                );
            }
        }
        if infos.len() > 1 {
            for info in &infos {
                self.check_index_constraint_for_index_signature(ty, owner, &infos, info);
            }
        }
    }

    /// The computed-name expression of a member whose name is **not** bindable
    /// — `!c.hasBindableName(member)` (`checker.go:19941`): a dynamic name
    /// (`ast.HasDynamicName`, anything but a string or numeric literal) that is
    /// not late-bindable either (`isLateBindableName`, `checker.go:19961`: an
    /// entity name whose type is usable as a property name).
    pub(crate) fn non_bindable_computed_name(
        &mut self,
        member: NodeId,
    ) -> Option<tsr_ast::Expression<'a>> {
        let name = self.declaration_name_of(member)?;
        let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name) else {
            return None;
        };
        let expression = computed.expression?;
        match expression {
            tsr_ast::Expression::StringLiteral(_)
            | tsr_ast::Expression::NumericLiteral(_)
            | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_) => return None,
            tsr_ast::Expression::PrefixUnaryExpression(prefix)
                if matches!(
                    prefix.operator.kind,
                    SyntaxKind::PlusToken | SyntaxKind::MinusToken
                ) && matches!(prefix.operand, Some(tsr_ast::Expression::NumericLiteral(_))) =>
            {
                return None;
            }
            tsr_ast::Expression::Identifier(_)
            | tsr_ast::Expression::PropertyAccessExpression(_) => {
                let name_type = self.check_expression(expression);
                if self.type_of(name_type).flags.intersects(
                    TypeFlags::STRING_LITERAL
                        | TypeFlags::NUMBER_LITERAL
                        | TypeFlags::UNIQUE_ES_SYMBOL,
                ) {
                    return None;
                }
            }
            _ => {}
        }
        Some(expression)
    }

    /// The source text of a member's computed name, brackets included.
    fn computed_member_name_text(&self, member: NodeId) -> Option<String> {
        let name = self.declaration_name_of(member)?;
        let span = self.error_span(name);
        let text = self
            .source_file_of_for_diagnostics(name)
            .and_then(|file| self.module_host?.source_text(file, self.nodes))?;
        Some(text.get(span.start as usize..span.end as usize)?.to_string())
    }

    /// `getLiteralTypeFromProperty(prop, TypeFlagsStringOrNumberLiteralOrUnique,
    /// true)` (`checker.go`): the late-bound name type of a computed member,
    /// else the string literal of the property's name.
    fn literal_type_from_property(&mut self, property: SymbolId, name: &str) -> TypeId {
        let declaration = self.binder.symbols().get(property).value_declaration;
        if let Some(name_node) = declaration.and_then(|d| self.declaration_name_of(d))
            && let Some(Node::ComputedPropertyName(computed)) = self.node_map.get(name_node)
            && let Some(expression) = computed.expression
        {
            let name_type = self.check_expression(expression);
            let name_type = self.get_regular_type_of_literal_type(name_type);
            if self.type_of(name_type).flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL | TypeFlags::UNIQUE_ES_SYMBOL,
            ) {
                return name_type;
            }
        }
        self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral(name.to_owned()),
            false,
        )
    }

    /// Is `declaration` a member of the type named by `owner` —
    /// `c.getParentOfSymbol(c.getSymbolOfDeclaration(declaration)) == t.symbol`
    /// read off the declaration's containing class, interface or type literal.
    fn declared_in_owner(&self, declaration: NodeId, owner: SymbolId) -> bool {
        self.nodes
            .parent(declaration)
            .and_then(|parent| self.binder.symbol_of(parent))
            .is_some_and(|parent| self.binder.merged_symbol(parent) == owner)
    }

    /// `getApplicableIndexInfos` (`checker.go`): the infos whose key the
    /// property name type is applicable to.
    ///
    /// The numeric-name arm of `isApplicableIndexType` (`checker.go:19054`) —
    /// a string literal against a `number` key — is decided by this module's
    /// `isNumericLiteralName` round-trip, which accepts `"Infinity"` and
    /// rejects `"-0"` as upstream does; the shared
    /// `index_signatures::is_numeric_literal_name` does not yet.
    fn applicable_index_infos(&mut self, infos: &[IndexInfo], key: TypeId) -> Vec<IndexInfo> {
        let numeric_name = match &self.store.get(key).data {
            crate::types::TypeData::StringLiteral(value) => Some(is_numeric_literal_name(value)),
            _ => None,
        };
        infos
            .iter()
            .copied()
            .filter(|info| match numeric_name {
                Some(numeric) if info.key == self.intrinsics.number => numeric,
                _ => self.is_applicable_index_type(key, info.key),
            })
            .collect()
    }

    /// The interface fallback shared by both constraint checks: `t` is an
    /// interface and no base type carries both halves of the pair.
    fn interface_error_node(
        &mut self,
        owner: SymbolId,
        is_interface: bool,
        base_has_both: &mut dyn FnMut(&mut Self, TypeId) -> bool,
    ) -> Option<NodeId> {
        if !is_interface {
            return None;
        }
        let interface = self
            .binder
            .symbols()
            .get(owner)
            .declarations
            .iter()
            .copied()
            .find(|&d| self.nodes.kind(d) == SyntaxKind::InterfaceDeclaration)?;
        let bases = self.base_symbols_of_ex(owner, false)?;
        for base in bases {
            let base_type = self.get_declared_type_of_class_or_interface(base);
            if base_has_both(self, base_type) {
                return None;
            }
        }
        Some(interface)
    }

    /// `checkIndexConstraintForProperty` (`checker.go`).
    #[allow(clippy::too_many_arguments)]
    fn check_index_constraint_for_property(
        &mut self,
        ty: TypeId,
        owner: SymbolId,
        infos: &[IndexInfo],
        property: SymbolId,
        name: &str,
        name_type: TypeId,
        property_type: TypeId,
    ) {
        let entry = self.binder.symbols().get(property);
        let declaration = entry.value_declaration;
        let parent = entry.parent;
        if let Some(name_node) = declaration.and_then(|d| self.declaration_name_of(d))
            && self.nodes.kind(name_node) == SyntaxKind::PrivateIdentifier
        {
            return;
        }
        let applicable = self.applicable_index_infos(infos, name_type);
        if applicable.is_empty() {
            return;
        }
        let is_interface = self.is_interface_type(ty, owner);
        let local_property =
            declaration.filter(|_| parent.is_some_and(|p| self.binder.merged_symbol(p) == owner));
        for info in applicable {
            let local_index = info.declaration.filter(|&d| self.declared_in_owner(d, owner));
            let error_node = match local_property.or(local_index) {
                Some(node) => Some(node),
                None => self.interface_error_node(owner, is_interface, &mut |checker, base| {
                    checker.get_property_of_type(base, name).is_some()
                        && checker
                            .get_index_infos_of_type(base)
                            .is_some_and(|infos| infos.iter().any(|i| i.key == info.key))
                }),
            };
            let Some(error_node) = error_node else { continue };
            if !self.assignability_pair_is_reportable(property_type, info.value)
                || self.relate_ternary(property_type, info.value, Relation::Assignable)
                    != Ternary::NotRelated
            {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(error_node) else { continue };
            let span = self.error_span(error_node);
            let property_text = self.type_to_string(property_type);
            let key_text = self.type_to_string(info.key);
            let value_text = self.type_to_string(info.value);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_OF_TYPE_1_IS_NOT_ASSIGNABLE_TO_2_INDEX_TYPE_3,
                    span,
                    [name.to_string(), property_text, key_text, value_text],
                ),
            );
        }
    }

    /// `checkIndexConstraintForIndexSignature` (`checker.go`).
    fn check_index_constraint_for_index_signature(
        &mut self,
        ty: TypeId,
        owner: SymbolId,
        infos: &[IndexInfo],
        check: &IndexInfo,
    ) {
        let applicable = self.applicable_index_infos(infos, check.key);
        if applicable.is_empty() {
            return;
        }
        let is_interface = self.is_interface_type(ty, owner);
        let local_check = check.declaration.filter(|&d| self.declared_in_owner(d, owner));
        for info in applicable {
            if info == *check {
                continue;
            }
            let local_index = info.declaration.filter(|&d| self.declared_in_owner(d, owner));
            let error_node = match local_check.or(local_index) {
                Some(node) => Some(node),
                None => self.interface_error_node(owner, is_interface, &mut |checker, base| {
                    checker.get_index_infos_of_type(base).is_some_and(|infos| {
                        infos.iter().any(|i| i.key == check.key)
                            && infos.iter().any(|i| i.key == info.key)
                    })
                }),
            };
            let Some(error_node) = error_node else { continue };
            if !self.assignability_pair_is_reportable(check.value, info.value)
                || self.relate_ternary(check.value, info.value, Relation::Assignable)
                    != Ternary::NotRelated
            {
                continue;
            }
            let Some(file) = self.source_file_of_for_diagnostics(error_node) else { continue };
            let span = self.error_span(error_node);
            let check_key = self.type_to_string(check.key);
            let check_value = self.type_to_string(check.value);
            let key_text = self.type_to_string(info.key);
            let value_text = self.type_to_string(info.value);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_INDEX_TYPE_1_IS_NOT_ASSIGNABLE_TO_2_INDEX_TYPE_3,
                    span,
                    [check_key, check_value, key_text, value_text],
                ),
            );
        }
    }

    /// `t.objectFlags&ObjectFlagsInterface != 0`: the declared type of a class
    /// or interface, not its static side or a type literal.
    fn is_interface_type(&self, ty: TypeId, owner: SymbolId) -> bool {
        matches!(self.store.get(ty).data, crate::types::TypeData::Named { .. }) && {
            let flags = self.binder.symbols().get(owner).flags;
            flags.contains(SymbolFlags::INTERFACE) && !flags.intersects(SymbolFlags::CLASS)
        }
    }
}
/// `isNumericLiteralName` (`utilities.go:898`) — *"we test whether
/// `ToString(ToNumber(name))` is exactly equal to `name`"*.
///
/// The round-trip is the point, and it is why `"0xF00D"` is **not** a numeric
/// name: indexing with `0xF00D` indexes with `"61453"`. `Infinity`,
/// `-Infinity` and `NaN` are accepted deliberately — upstream's comment says so
/// — because indexing with them really does index with those strings.
///
/// `parse::<f64>()` is not this predicate: it accepts `"+1"`, `"inf"` and
/// `"nan"`, none of which round-trips. Every character outside `[0-9.eE+-]` is
/// rejected up front, which covers the hex, octal and whitespace spellings the
/// corpus tests without needing JS's full `ToNumber`.
fn is_numeric_literal_name(name: &str) -> bool {
    if matches!(name, "Infinity" | "-Infinity" | "NaN") {
        return true;
    }
    if name.is_empty()
        || !name.bytes().all(|byte| byte.is_ascii_digit() || b".eE+-".contains(&byte))
    {
        return false;
    }
    name.parse::<f64>().is_ok_and(|value| js_number_to_string(value) == name)
}

/// `Number#toString` for a finite value — decimal in JS's `[1e-6, 1e21)` band
/// and exponential outside it, with the `+` JS writes on a non-negative
/// exponent.
fn js_number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs();
    if !(1e-6..1e21).contains(&magnitude) {
        let exponential = format!("{value:e}");
        return match exponential.split_once('e') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => {
                format!("{mantissa}e+{exponent}")
            }
            _ => exponential,
        };
    }
    format!("{value}")
}
