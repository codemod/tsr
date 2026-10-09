//! Value exports on anonymous callable objects. `resolveAnonymousTypeMembers`
//! reads the function's exports; `createTypeNodeFromObjectType` emits its call
//! signatures followed by those properties (checker.go, nodebuilderimpl.go).

use tsr_ast::{Node, NodeId};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_core::Idx;

use crate::{
    Checker,
    objects::{AnonymousProperty, Member},
    types::TypeId,
};

impl Checker<'_, '_> {
    pub(crate) fn callable_export_properties(
        &mut self,
        symbol: SymbolId,
    ) -> Option<Vec<AnonymousProperty>> {
        let mut exports: Vec<_> = self
            .binder
            .symbols()
            .get(symbol)
            .exports
            .iter()
            .filter(|(_, member)| {
                self.binder.symbols().get(**member).flags.intersects(SymbolFlags::VALUE)
            })
            .map(|(name, &member)| {
                let declaration = *self.binder.symbols().get(member).declarations.first()?;
                Some((declaration.index(), (*name).to_owned(), member))
            })
            .collect::<Option<Vec<_>>>()?;
        // Registered declaration identities follow the program's source walk;
        // sorting restores insertion order from the binder's hash table.
        exports.sort_by_key(|(declaration, _, _)| *declaration);
        // combineSymbolTables(early, late) (5b1047d checker.go:15975): the
        // late-bound assignment members follow the early exports, in
        // declaration order, and an early name shadows a late one.
        let mut late = Vec::new();
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::FUNCTION) {
            for (name, member) in self.late_bound_static_members_of(symbol) {
                if exports.iter().any(|(_, early, _)| *early == name)
                    || late.iter().any(|(_, seen, _)| *seen == name)
                {
                    continue;
                }
                late.push((usize::MAX, name, member));
            }
        }
        let mut members = Vec::with_capacity(exports.len() + late.len());
        for (order, name, member) in exports.into_iter().chain(late) {
            let t = self.get_type_of_symbol(member);
            if t == self.intrinsics.error {
                return None;
            }
            let printed_name = if order == usize::MAX {
                self.late_bound_assignment_printed_name(member, &name)
            } else {
                self.callable_property_name(member, &name)
            };
            members.push(AnonymousProperty {
                accessor_write: None,
                method: false,
                origin: Some(member),
                checked_declaration: None,
                name,
                printed_name,
                optional: self.property_is_optional(member),
                readonly: self.is_readonly_symbol(member),
                printed_slot: crate::objects::PrintedSlot::printed(self.type_to_string(t)),
                slot: crate::objects::PropertySlot::resolved(t),
            });
        }
        Some(members)
    }

    /// The printed name of a late-bound assignment member: its name type
    /// decides (`getPropertyNameNodeForSymbol` reads the late symbol's
    /// `nameType`), so `foo[k] = 1` with `const k = "10"` prints `"10"`
    /// where a numeric `k` prints `10`. A unique symbol prints its bracketed
    /// entity name, which is already the member's name here.
    fn late_bound_assignment_printed_name(&mut self, member: SymbolId, name: &str) -> String {
        if name.starts_with('[') || crate::objects::is_identifier_text(name) {
            return name.to_owned();
        }
        let declaration = self.binder.symbols().get(member).value_declaration;
        let argument = match declaration.and_then(|declaration| self.node_map.get(declaration)) {
            Some(Node::BinaryExpression(binary)) => match binary.left {
                Some(tsr_ast::Expression::ElementAccessExpression(access)) => {
                    access.argument_expression
                }
                _ => None,
            },
            _ => None,
        };
        let string_named = argument.is_some_and(|argument| {
            let name_type = self.check_expression(argument);
            self.store.get(name_type).flags.intersects(crate::flags::TypeFlags::STRING_LIKE)
        });
        if !string_named && crate::index_signatures::is_numeric_literal_name(name) {
            return name.to_owned();
        }
        crate::printing::quote_ascii(name)
    }

    /// `getPropertyNameNodeForSymbol` / `classifyPropertyName`
    /// (internal/checker/nodebuilderimpl.go): numeric names retain the
    /// distinction between string-named and numeric declarations.
    pub(crate) fn callable_property_name(&self, symbol: SymbolId, name: &str) -> String {
        if crate::objects::is_identifier_text(name) {
            return name.to_owned();
        }
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let string_named = !declarations.is_empty()
            && declarations.iter().all(|&declaration| {
                let name = match self.node_map.get(declaration) {
                    Some(Node::BinaryExpression(binary)) => match binary.left {
                        Some(tsr_ast::Expression::ElementAccessExpression(access)) => {
                            access.argument_expression.and_then(|argument| argument.node_id())
                        }
                        _ => None,
                    },
                    Some(Node::CallExpression(call)) => {
                        call.arguments.get(1).and_then(tsr_ast::Expression::node_id)
                    }
                    _ => self.declaration_name_of(declaration),
                };
                matches!(
                    name.and_then(|name| self.node_map.get(name)),
                    Some(Node::StringLiteral(_))
                )
            });
        if !string_named
            && crate::index_signatures::is_numeric_literal_name(name)
            && !name.starts_with('-')
        {
            return name.to_owned();
        }
        crate::printing::quote_ascii(name)
    }

    pub(crate) fn callable_object_to_string_at(
        &mut self,
        id: TypeId,
        reference: NodeId,
    ) -> Option<String> {
        let signatures = self.signature_types.get(&id)?.clone();
        let properties = self.anonymous_properties.get(&id)?.0.clone();
        let mut claimed = rustc_hash::FxHashSet::default();
        let mut members = Vec::new();
        for signature in signatures {
            let signature =
                self.rename_type_parameters_for_site(signature, reference, &mut claimed);
            members.push(Member::Signature {
                printed: self.signature_member_text_at(&signature, reference),
            });
        }
        for property in properties {
            let ty = self.property_type(&property);
            members.push(Member::Property {
                name: property.printed_name,
                optional: property.optional,
                readonly: property.readonly,
                printed: self.type_to_string_at(ty, reference)?,
            });
        }
        Some(crate::objects::render_object_type(&members))
    }

    pub(crate) fn property_members(&mut self, properties: &[AnonymousProperty]) -> Vec<Member> {
        properties
            .iter()
            .map(|property| Member::Property {
                name: property.printed_name.clone(),
                optional: property.optional,
                readonly: property.readonly,
                printed: self.property_printed_type(property).into_owned(),
            })
            .collect()
    }
}
