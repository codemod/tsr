//! The node builder's reuse of an instantiation expression's own `typeof`
//! node (`createAnonymousTypeNodeEx`, `nodebuilderimpl.go:2816`).
//!
//! `getInstantiationExpressionType` (`checker.go:10660`) records the node it
//! was computed for on the minted type
//! (`result.AsInstantiationExpressionType().node = node`). When that node is
//! a `TypeQueryNode` and `getTypeFromTypeNode(existing) == t`, the builder
//! reuses the written node (`tryReuseExistingNonParameterTypeNode`), so
//! `var xs3: typeof Array<number>` records `>xs3 : typeof Array<number>`
//! (`arrayTypeOfTypeOf`) rather than the instantiated `ArrayConstructor`
//! members.
//!
//! The identity test admits exactly the result of the whole computation for
//! that node: a constituent of a union or intersection result is not the
//! node's type, and neither is an alias instantiation, which upstream maps
//! from the frame-free result instead of recomputing. This port bakes text at
//! creation, so the reuse is applied once, to a result minted by the node's
//! own frame-free computation: the result is re-minted with the written
//! spelling.
//!
//! **Approximation, stated.** Upstream's reuse re-checks each entity name's
//! accessibility at the print site and falls back to the structural form;
//! the baked text cannot. Every corpus print of these types is in the
//! declaring file, where the written names resolve.
//!
//! `docs/parity/notes/r6-typesroots.md` §4.

use tsr_ast::{Node, NodeId, TypeNode};

use crate::checker::Checker;
use crate::types::{TypeData, TypeId};

impl<'a> Checker<'a, '_> {
    /// `result` re-minted with the written `typeof` spelling of `node` when
    /// the node builder would reuse that node for it; else `result`.
    ///
    /// `minted_from` is the type store's length before the computation, so a
    /// result minted by it is told from a pre-existing one (the expression
    /// type itself, a constraint).
    pub(crate) fn reuse_instantiation_type_query_node(
        &mut self,
        node: NodeId,
        expression_type: TypeId,
        result: TypeId,
        minted_from: usize,
    ) -> TypeId {
        if result == expression_type || result.index() < minted_from {
            return result;
        }
        let Some(Node::TypeQueryNode(query)) = self.node_map.get(node) else { return result };
        let Some(text) = self.written_type_query_text(query) else { return result };
        let flags = self.store.get(result).flags;
        let reused = match self.store.get(result).data {
            TypeData::Anonymous { symbol, .. } => {
                self.store.new_anonymous(flags, text, symbol, false)
            }
            TypeData::Named { members, .. } => self.store.new_named(flags, text, members),
            _ => return result,
        };
        if let Some(properties) = self.anonymous_properties.get(&result).cloned() {
            self.anonymous_properties.insert(reused, properties);
        }
        if let Some(signatures) = self.signature_types.get(&result).cloned() {
            self.signature_types.insert(reused, signatures);
        }
        if let Some(indexes) = self.object_literal_index_infos.get(&result).cloned() {
            self.object_literal_index_infos.insert(reused, indexes);
        }
        // The written spelling is the print; the site re-render of a
        // signature-bearing type must not replace it.
        self.alias_named_signature_types.insert(reused);
        reused
    }

    /// `typeof a.b<A, B>` as written: the entity name, then each argument as
    /// the builder reuses it (a nested argument-free `typeof` as written,
    /// anything else as its type prints).
    fn written_type_query_text(&mut self, query: &tsr_ast::TypeQueryNode<'a>) -> Option<String> {
        let mut segments = Vec::new();
        let mut current = query.expr_name?;
        loop {
            match current {
                tsr_ast::EntityName::Identifier(identifier) => {
                    segments.push(identifier.text);
                    break;
                }
                tsr_ast::EntityName::QualifiedName(qualified) => {
                    segments.push(qualified.right?.text);
                    current = qualified.left?;
                }
            }
        }
        segments.reverse();
        let mut text = format!("typeof {}", segments.join("."));
        if !query.type_arguments.is_empty() {
            let mut arguments = Vec::with_capacity(query.type_arguments.len());
            for &argument in query.type_arguments {
                let printed = match argument {
                    TypeNode::TypeQueryNode(inner) if inner.type_arguments.is_empty() => {
                        self.written_type_query_text(inner)?
                    }
                    _ => {
                        let id = self.get_type_from_type_node(argument);
                        if self.is_gap(id) {
                            return None;
                        }
                        self.type_to_string(id)
                    }
                };
                arguments.push(printed);
            }
            text.push('<');
            text.push_str(&arguments.join(", "));
            text.push('>');
        }
        Some(text)
    }
}
