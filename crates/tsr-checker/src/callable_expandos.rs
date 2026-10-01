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
        let mut members = Vec::with_capacity(exports.len());
        for (_, name, member) in exports {
            let t = self.get_type_of_symbol(member);
            if t == self.intrinsics.error {
                return None;
            }
            let printed_name = self.callable_property_name(member, &name);
            members.push(AnonymousProperty {
                name,
                printed_name,
                optional: self.property_is_optional(member),
                readonly: self.is_readonly_symbol(member),
                printed_type: self.type_to_string(t),
                r#type: t,
            });
        }
        Some(members)
    }

    /// `getPropertyNameNodeForSymbol` / `classifyPropertyName`
    /// (internal/checker/nodebuilderimpl.go): numeric names retain the
    /// distinction between string-named and numeric declarations.
    fn callable_property_name(&self, symbol: SymbolId, name: &str) -> String {
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
            members.push(Member::Property {
                name: property.printed_name,
                optional: property.optional,
                readonly: property.readonly,
                printed: self.type_to_string_at(property.r#type, reference)?,
            });
        }
        Some(crate::objects::render_object_type(&members))
    }
}

pub(crate) fn property_members(properties: &[AnonymousProperty]) -> Vec<Member> {
    properties
        .iter()
        .map(|property| Member::Property {
            name: property.printed_name.clone(),
            optional: property.optional,
            readonly: property.readonly,
            printed: property.printed_type.clone(),
        })
        .collect()
}
