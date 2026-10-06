//! A JS function's `@type` tag as its full signature.
//!
//! `reparseHosted`'s `KindJSDocTypeTag` arm (`parser/reparser.go:342`) ends by
//! storing the tag's type as `FullSignature` on the function-like host
//! (`getFunctionLikeHost`, `parser/reparser.go:653`) when that function has no
//! type parameters, no return annotation and no typed parameter at that point
//! of the comment's reparse. The checker reads it through
//! `getSignatureOfFullSignatureType` (`checker/checker.go:20072`): an
//! unannotated parameter takes the signature's type at its position
//! (`getParameterTypeOfFullSignature`).
//!
//! This port keeps JSDoc in a side table, so the reparse order is replayed
//! over the host's last comment instead of read off a mutated node — by the
//! one replay, [`Checker::jsdoc_reparsed_function`] (ADR-0046). No state is
//! cached: the walk is over one comment's tags and the function's own
//! parameter list, and the parameter type itself is memoized by the symbol
//! type that calls in here.

use tsr_ast::{Node, NodeId, SyntaxKind, TypeNode};

use crate::{checker::Checker, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// The `@type` node `reparseHosted` would store as `function`'s
    /// `FullSignature`, if any.
    fn jsdoc_full_signature_node(&self, function: NodeId) -> Option<TypeNode<'a>> {
        if !self.in_js_file(function) {
            return None;
        }
        // `getFunctionLikeHost` keeps a function or method host as itself.
        // The other function-like hosts are not read here yet: widening is
        // its own measured change (docs/parity/notes/js.md §3).
        if !matches!(
            self.nodes.kind(function),
            SyntaxKind::FunctionDeclaration | SyntaxKind::MethodDeclaration
        ) {
            return None;
        }
        self.jsdoc_reparsed_function(function).full_signature
    }

    /// `getParameterTypeOfFullSignature` (`checker/checker.go:20079`): the
    /// type an unannotated JS parameter takes from its function's `@type`.
    pub(crate) fn jsdoc_full_signature_parameter_type(
        &mut self,
        parameter: NodeId,
    ) -> Option<TypeId> {
        let function = self.nodes.parent(parameter)?;
        let annotation = self.jsdoc_full_signature_node(function)?;
        let parameters = match self.node_map.get(function)? {
            Node::FunctionDeclaration(node) => node.parameters,
            Node::MethodDeclaration(node) => node.parameters,
            _ => return None,
        };
        let position = parameters.iter().position(|node| node.node_id == Some(parameter))?;
        let is_rest = parameters[position].dot_dot_dot_token.is_some();
        // getSignatureOfFullSignatureType: getSingleCallSignature of the
        // annotation's type.
        let ty = self.get_type_from_type_node(annotation);
        let signature = self.single_call_signature(ty)?;
        Some(if is_rest {
            self.signature_rest_type_at_position(&signature, position)
        } else {
            self.signature_type_at_position(&signature, position).unwrap_or(self.intrinsics.any)
        })
    }
}
