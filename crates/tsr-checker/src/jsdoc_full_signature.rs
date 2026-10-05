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
//! over the host's last comment instead of read off a mutated node. No state
//! is cached: the walk is over one comment's tags and the function's own
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
        // `getFunctionLikeHost` keeps a function or method host as itself;
        // the variable, property, export and return hosts take the tag for
        // their own declaration first (`reparseHosted`'s earlier arms).
        let (type_parameters, parameters, return_type) = match self.node_map.get(function)? {
            Node::FunctionDeclaration(node) => (node.type_parameters, node.parameters, node.r#type),
            Node::MethodDeclaration(node) => (node.type_parameters, node.parameters, node.r#type),
            _ => return None,
        };
        // `reparseTags` runs `reparseHosted` for the last comment only.
        let doc = self.jsdoc_entries.get(&function)?.last()?;
        let mut has_type_parameters = !type_parameters.is_empty();
        let mut has_return_type = return_type.is_some();
        let mut typed: Vec<bool> =
            parameters.iter().map(|parameter| parameter.r#type.is_some()).collect();
        let mut has_typed_this = false;
        let gathers_type_parameters = !doc.tags.iter().any(|tag| {
            matches!(
                tag,
                tsr_ast::JSDocTag::JSDocTypedefTag(_) | tsr_ast::JSDocTag::JSDocCallbackTag(_)
            )
        });
        let mut full_signature = None;
        let mut parameter_tag_index = 0usize;
        for tag in doc.tags {
            match tag {
                // `KindJSDocTemplateTag`: `gatherTypeParameters(jsDoc, false)`
                // collects every `@template` of the comment, unless a full
                // signature is already set.
                tsr_ast::JSDocTag::JSDocTemplateTag(_)
                    if full_signature.is_none() && !has_type_parameters =>
                {
                    has_type_parameters = gathers_type_parameters
                        && doc.tags.iter().any(|tag| {
                            matches!(tag, tsr_ast::JSDocTag::JSDocTemplateTag(template)
                                if !template.type_parameters.is_empty())
                        });
                }
                // `KindJSDocParameterTag`: `findMatchingParameter` types the
                // parameter of the same name (or position, for an empty name).
                tsr_ast::JSDocTag::JSDocParameterOrPropertyTag(parameter_tag)
                    if parameter_tag.kind.kind == SyntaxKind::JSDocParameterTag =>
                {
                    let tag_index = parameter_tag_index;
                    parameter_tag_index += 1;
                    if full_signature.is_some() || parameter_tag.type_expression.is_none() {
                        continue;
                    }
                    let tag_name = match parameter_tag.name {
                        Some(tsr_ast::EntityName::Identifier(name)) => Some(name.text),
                        _ => None,
                    };
                    let matched =
                        parameters.iter().enumerate().position(
                            |(index, parameter)| match parameter.name {
                                Some(tsr_ast::BindingName::Identifier(name)) => tag_name
                                    .is_some_and(|text| {
                                        name.text == text || (index == tag_index && text.is_empty())
                                    }),
                                _ => index == tag_index,
                            },
                        );
                    if let Some(index) = matched {
                        typed[index] = true;
                    }
                }
                tsr_ast::JSDocTag::JSDocThisTag(this_tag) => {
                    has_typed_this |= this_tag.type_expression.is_some();
                }
                tsr_ast::JSDocTag::JSDocReturnTag(return_tag) if full_signature.is_none() => {
                    has_return_type |= return_tag.type_expression.is_some();
                }
                tsr_ast::JSDocTag::JSDocTypeTag(type_tag) => {
                    let no_typed_parameters = !has_typed_this && !typed.contains(&true);
                    if !has_type_parameters && !has_return_type && no_typed_parameters {
                        let annotation = match type_tag.type_expression {
                            Some(Node::JSDocTypeExpression(expression)) => expression.r#type,
                            _ => None,
                        };
                        if annotation.is_some() {
                            full_signature = annotation;
                        }
                    }
                }
                _ => {}
            }
        }
        full_signature
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
