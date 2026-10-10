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

use crate::{
    checker::Checker,
    flags::TypeFlags,
    signatures::{Signature, SignatureKind},
    types::TypeId,
};

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

    /// `getSignatureOfFullSignatureType` (`checker/checker.go:20072`): the
    /// single call signature of `function`'s `@type`, type parameters
    /// included. `getSignaturesOfSymbol` (`:19827`) and
    /// `getTypeParametersFromDeclaration` (`:19913`) take it in place of the
    /// declaration's own signature.
    pub(crate) fn jsdoc_full_signature(&mut self, function: NodeId) -> Option<Signature> {
        let annotation = self.jsdoc_full_signature_node(function)?;
        let ty = self.get_type_from_type_node(annotation);
        self.single_call_signature_of(ty)
    }

    /// `getParameterTypeOfFullSignature` (`checker/checker.go:20079`): the
    /// type an unannotated JS parameter takes from its function's `@type`.
    pub(crate) fn jsdoc_full_signature_parameter_type(
        &mut self,
        parameter: NodeId,
    ) -> Option<TypeId> {
        let function = self.nodes.parent(parameter)?;
        let parameters = match self.node_map.get(function)? {
            Node::FunctionDeclaration(node) => node.parameters,
            Node::MethodDeclaration(node) => node.parameters,
            _ => return None,
        };
        let signature = self.jsdoc_full_signature(function)?;
        let position = parameters.iter().position(|node| node.node_id == Some(parameter))?;
        Some(if parameters[position].dot_dot_dot_token.is_some() {
            self.signature_rest_type_at_position(&signature, position)
        } else {
            self.signature_type_at_position(&signature, position).unwrap_or(self.intrinsics.any)
        })
    }

    /// Whether `function` has a `@type` full signature this port cannot
    /// read: its type is the error type (a gap, `tsr-2zk.31`) or one of the
    /// lists `getSingleCallSignature` reads is undecided. Upstream reads a
    /// signature or none; the caller declines rather than answer "none" for
    /// a type this port did not compute.
    #[expect(dead_code, reason = "called by docs/parity/notes/r6-jsdoc2-js-implicit-any.diff")]
    pub(crate) fn jsdoc_full_signature_undecided(&mut self, function: NodeId) -> bool {
        let Some(annotation) = self.jsdoc_full_signature_node(function) else { return false };
        let ty = self.get_type_from_type_node(annotation);
        if self.is_error(ty) {
            return true;
        }
        if !self.store.get(ty).flags.contains(TypeFlags::OBJECT) {
            return false;
        }
        self.get_property_names_of_type(ty).is_none()
            || self.get_index_infos_of_type(ty).is_none()
            || self.signatures_of_type_kind(ty, SignatureKind::Call).is_none()
            || self.signatures_of_type_kind(ty, SignatureKind::Construct).is_none()
    }

    /// `getReturnTypeOfFullSignature` (`checker/checker.go:20089`), the last
    /// arm of `getReturnTypeFromAnnotation`: a JS function whose `@type` tag
    /// is its full signature returns that signature's return type.
    pub(crate) fn jsdoc_full_signature_return_type(&mut self, function: NodeId) -> Option<TypeId> {
        let signature = self.jsdoc_full_signature(function)?;
        self.get_return_type_of_signature(&signature)
    }

    /// `getSingleCallSignature` → `getSingleSignature(t, SignatureKindCall,
    /// false)` (`checker/checker.go:19353`): exactly one call signature, no
    /// construct signature, and no property or index signature. Unlike the
    /// contextual `single_call_signature`, a generic signature is kept.
    fn single_call_signature_of(&mut self, ty: TypeId) -> Option<Signature> {
        if !self.store.get(ty).flags.contains(TypeFlags::OBJECT) {
            return None;
        }
        if !self.get_property_names_of_type(ty)?.is_empty()
            || !self.get_index_infos_of_type(ty)?.is_empty()
        {
            return None;
        }
        let mut calls = self.signatures_of_type_kind(ty, SignatureKind::Call)?;
        if calls.len() != 1
            || !self.signatures_of_type_kind(ty, SignatureKind::Construct)?.is_empty()
        {
            return None;
        }
        calls.pop()
    }
}
