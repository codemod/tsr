//! The JSDoc type nodes upstream's `checkSourceElement` visits.
//!
//! typescript-go reparses a JS file's JSDoc into ordinary AST
//! (`internal/parser/reparser.go`): a typedef becomes a
//! `JSTypeAliasDeclaration` statement (`reparseUnhosted`), and a hosted
//! `@type`/`@param`/`@returns` becomes the `Type` of the declaration,
//! parameter or function it documents (`reparseHosted`). The checker then
//! walks those nodes like written ones — `checkVariableLikeDeclaration`,
//! `checkParameter`, `checkSignatureDeclaration` and
//! `checkTypeAliasDeclaration` each `checkSourceElement` their type — so every
//! type-node rule (TS2344's `checkTypeArgumentConstraints`, arity, …) runs on
//! them.
//!
//! This port keeps JSDoc in the `jsdoc_entries` side table
//! ([ADR-0046](../../../docs/adr/0046-jsdoc-reparse-is-a-checker-query.md)),
//! so `check_node`'s child walk never reaches a comment. This module answers,
//! for one node, the reparsed type nodes the native walk would reach through
//! it, and `check_node` visits each with the same walk. Each is the node the
//! typing road already reads for the same slot (`jsdoc_type_annotation`,
//! [`Checker::jsdoc_reparsed_parameter_type`], the replay's `return_type`), so
//! the checked node and the typed node cannot disagree.
//!
//! Covered, each as native's reparser exposes it: a variable's, class
//! property's or parameter's own type; a function's reparsed `@returns`,
//! `@this` parameter and full-signature `@type`; a cast's type
//! (`makeNewCast` for `@type` on a parenthesized expression or `return`);
//! a `@satisfies` type; and, from every comment `node` hosts, the
//! `@typedef` and `@callback` aliases and the `@overload` signatures
//! `reparseUnhosted` appends after the host. Not covered: `@template`
//! constraints and defaults, `@augments`/`@implements` heritage, `@import`
//! declarations, and a typedef's `@property` types.
//! See `docs/parity/notes/r5-jsdoc2.md` §2, `r5-jsdoc3.md` §3 and
//! `r5-jsdoc4.md` §1.
//!
//! No cache or side table: one `jsdoc_entries` probe per node, and for a
//! parameter or function the replay [`Checker::jsdoc_reparsed_function`]
//! answers from hash lookups when no comment applies.

use tsr_ast::{JSDocTag, Node, NodeId, SyntaxKind, TypeNode};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::jsdoc_params::top_level_tags;

impl<'a> Checker<'a, '_> {
    /// The reparsed type nodes `checkSourceElement` reaches through `node`,
    /// in upstream's order: a declaration's own type (for a function, its
    /// reparsed `this` parameter's type, then its full signature or return
    /// type), the casts `node`'s last comment wraps its expression in, then
    /// (for every comment `node` hosts) the typedef and callback aliases and
    /// overload signatures `reparseList` appends after the host statement.
    ///
    /// A cast reads native's hosting rule — each typed `@type` of the last
    /// comment — where the typing road ([`Checker::jsdoc_cast_annotation`])
    /// takes the first typed `@type` of any comment; they differ only for a
    /// parenthesized expression with two typed `@type` tags.
    pub(crate) fn jsdoc_reparsed_type_nodes(&self, node: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let kind = self.nodes.kind(node);
        let hosts_reparse = matches!(
            kind,
            SyntaxKind::VariableDeclaration
                | SyntaxKind::Parameter
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
        );
        let docs = self.jsdoc_entries.get(&node);
        if !hosts_reparse && docs.is_none() {
            return out;
        }
        if !self.in_js_file(node) {
            return out;
        }
        let mut push = |ty: Option<TypeNode<'a>>| {
            if let Some(id) = ty.and_then(|ty| Node::from(ty).node_id()) {
                out.push(id);
            }
        };
        match self.node_map.get(node) {
            // `checkVariableLikeDeclaration` → `checkSourceElement(node.Type())`.
            Some(Node::VariableDeclaration(declaration)) if declaration.r#type.is_none() => {
                push(self.jsdoc_type_annotation(node));
            }
            // `checkPropertyDeclaration` → `checkVariableLikeDeclaration`.
            Some(Node::PropertyDeclaration(property)) if property.r#type.is_none() => {
                push(self.jsdoc_self_hosted_type(node));
            }
            // `checkParameter` → `checkVariableLikeDeclaration`.
            Some(Node::ParameterDeclaration(parameter)) if parameter.r#type.is_none() => {
                push(self.jsdoc_reparsed_parameter_type(node));
            }
            // `checkExportAssignment` (`checker.go:5662`) resolves the
            // reparsed `Type` the expression is checked against.
            Some(Node::ExportAssignment(_)) => push(self.jsdoc_export_assignment_type(node)),
            // `checkClassLikeDeclaration` (`checker.go:4316`):
            // `checkSourceElements(baseTypeNode.TypeArguments())`, which
            // `reparseHosted`'s `KindJSDocAugmentsTag` arm copied from an
            // `@augments` tag onto an `extends` element that writes none.
            Some(
                Node::ClassDeclaration(tsr_ast::ClassDeclaration { heritage_clauses, .. })
                | Node::ClassExpression(tsr_ast::ClassExpression { heritage_clauses, .. }),
            ) => {
                if let Some(element) = heritage_clauses
                    .iter()
                    .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
                    .and_then(|clause| clause.types.first())
                    && element.type_arguments.is_empty()
                    && let Some(id) = element.node_id
                {
                    for &argument in self.jsdoc_augments_type_arguments(id).unwrap_or_default() {
                        push(Some(argument));
                    }
                }
            }
            Some(_)
                if hosts_reparse
                    && kind != SyntaxKind::VariableDeclaration
                    && kind != SyntaxKind::Parameter =>
            {
                let function = self.jsdoc_reparsed_function(node);
                // `checkSignatureDeclaration`'s parameter loop reaches the
                // reparsed `this` parameter first (`checkParameter` →
                // `checkVariableLikeDeclaration`).
                if let Some(this_tag) = function.this_tag {
                    push(this_tag.type_expression);
                }
                // `checkFunctionOrMethodDeclaration` (`checker.go:10143`) and
                // `checkFunctionExpressionOrObjectLiteralMethod` (`:3441`)
                // resolve the full signature; otherwise
                // `checkSignatureDeclaration` checks the reparsed `Type`.
                if function.full_signature.is_some() {
                    push(function.full_signature);
                } else if self.function_like_parts(node).is_some_and(|parts| !parts.return_type) {
                    push(function.return_type);
                }
            }
            _ => {}
        }
        let Some(docs) = docs else { return out };
        let Some(&last) = docs.last() else { return out };
        // `reparseHosted`'s cast arms (`makeNewCast`): `checkAssertion`
        // (`checker.go:12302`) and `checkSatisfiesExpression` (`:10743`)
        // `checkSourceElement` the type. Only the last comment is hosted,
        // and every typed tag wraps the expression again.
        let casts = match self.node_map.get(node) {
            Some(Node::ParenthesizedExpression(parenthesized)) => {
                parenthesized.expression.is_some()
            }
            Some(Node::ReturnStatement(statement)) => statement.expression.is_some(),
            _ => false,
        };
        let satisfies = self.jsdoc_satisfies_target(node).is_some();
        if casts || satisfies {
            for tag in top_level_tags(last.tags) {
                match tag {
                    JSDocTag::JSDocTypeTag(type_tag) if casts => {
                        if let Some(Node::JSDocTypeExpression(expression)) =
                            type_tag.type_expression
                        {
                            push(expression.r#type);
                        }
                    }
                    JSDocTag::JSDocSatisfiesTag(satisfies_tag) if satisfies => {
                        push(satisfies_tag.type_expression);
                    }
                    _ => {}
                }
            }
        }
        // `reparseUnhosted`'s typedef, callback and overload arms: every
        // comment, not only the last. A callback's tag holds the function
        // type its alias carries (`parse_callback_tag`), so
        // `checkTypeAliasDeclaration` → `checkSourceElement(type)` reaches
        // its parameters and return type.
        let overloads = self.jsdoc_hosts_overloads(node);
        for doc in *docs {
            let tags = doc.tags;
            for (index, tag) in tags.iter().enumerate() {
                match tag {
                    JSDocTag::JSDocTypedefTag(typedef) => {
                        if let Some(Node::JSDocTypeExpression(expression)) = typedef.type_expression
                        {
                            push(expression.r#type);
                        }
                    }
                    JSDocTag::JSDocCallbackTag(callback) => push(callback.type_expression),
                    // `reparseJSDocSignature` (`parser/reparser.go:150`):
                    // `checkFunctionDeclaration` → `checkSignatureDeclaration`
                    // reaches each reparsed parameter's type and the return
                    // type — the tag's function type (`jsdoc_overloads.rs`).
                    JSDocTag::JSDocOverloadTag(tsr_ast::JSDocOverloadTag {
                        type_expression: Some(TypeNode::FunctionTypeNode(signature)),
                        ..
                    }) if overloads => {
                        for parameter in signature.parameters {
                            push(parameter.r#type);
                        }
                        push(signature.r#type);
                    }
                    // A flat-parsed run (no signature folded into the tag):
                    // the same nodes, read off the tags that follow it.
                    JSDocTag::JSDocOverloadTag(_) if overloads => {
                        for child in overload_signature(&tags[index + 1..]) {
                            match child {
                                JSDocTag::JSDocThisTag(this_tag) => push(this_tag.type_expression),
                                JSDocTag::JSDocParameterOrPropertyTag(parameter)
                                    if matches!(
                                        parameter.name,
                                        Some(tsr_ast::EntityName::Identifier(_))
                                    ) =>
                                {
                                    push(parameter.type_expression);
                                }
                                JSDocTag::JSDocReturnTag(return_tag) => {
                                    push(return_tag.type_expression);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        // `checkTypeParameters` → `checkTypeParameter` (`checker.go:7002`,
        // `:2603`) on each list `gatherTypeParameters` reparses from these
        // comments: a typedef's or callback's alias, or the documented
        // function or class.
        for (_, list) in self.jsdoc_template_lists(node) {
            out.extend(list.iter().filter_map(|parameter| parameter.node_id));
        }
        out
    }

    /// The type-parameter lists `gatherTypeParameters`
    /// (`parser/reparser.go:293`) reparses from the comments `node` hosts,
    /// each with the kind of the declaration it becomes the list of:
    ///
    /// - `reparseUnhosted`'s typedef and callback arms (`:70`), every
    ///   comment: a `JSTypeAliasDeclaration`, whose list is every `@template`
    ///   of the comment (`typedefOrCallback`);
    /// - `reparseHosted`'s `KindJSDocTemplateTag` arm (`:453`), the last
    ///   comment only, when it declares no typedef or callback: the
    ///   `getFunctionLikeHost` function, if it has no written list and the
    ///   comment's `@type` has not made a full signature first, else a class
    ///   declaration or expression with no written list.
    ///
    /// An `@overload`'s reparsed declaration takes the same comment's list
    /// again (`reparseJSDocSignature`); its parameters' diagnostics land at the
    /// same nodes, which the diagnostics collection deduplicates.
    pub(crate) fn jsdoc_template_lists(
        &self,
        node: NodeId,
    ) -> Vec<(SyntaxKind, Vec<&'a tsr_ast::TypeParameterDeclaration<'a>>)> {
        let mut out = Vec::new();
        let Some(docs) = self.jsdoc_entries.get(&node) else { return out };
        if !self.in_js_file(node) {
            return out;
        }
        let gather = |doc: &'a tsr_ast::JSDoc<'a>| {
            let mut list = Vec::new();
            for tag in top_level_tags(doc.tags) {
                if let JSDocTag::JSDocTemplateTag(template) = tag {
                    list.extend(template.type_parameters.iter().copied());
                }
            }
            list
        };
        let declares_alias = |doc: &tsr_ast::JSDoc<'_>| {
            doc.tags.iter().any(|tag| {
                matches!(tag, JSDocTag::JSDocTypedefTag(_) | JSDocTag::JSDocCallbackTag(_))
            })
        };
        for doc in *docs {
            if declares_alias(doc) {
                let list = gather(doc);
                if !list.is_empty() {
                    out.push((SyntaxKind::TypeAliasDeclaration, list));
                }
            }
        }
        let Some(&last) = docs.last() else { return out };
        if declares_alias(last) {
            return out;
        }
        let hosted = gather(last);
        if hosted.is_empty() {
            return out;
        }
        if let Some(function) = self.jsdoc_function_like_host(node) {
            let parts = self.function_like_parts(function);
            if parts.is_some_and(|parts| parts.type_parameters.is_empty())
                && !self.jsdoc_type_tag_precedes_template(last, function)
            {
                out.push((self.nodes.kind(function), hosted));
            }
            return out;
        }
        let written = match self.node_map.get(node) {
            Some(Node::ClassDeclaration(class)) => Some(class.type_parameters),
            Some(Node::ClassExpression(class)) => Some(class.type_parameters),
            _ => None,
        };
        if written.is_some_and(<[_]>::is_empty) {
            out.push((self.nodes.kind(node), hosted));
        }
        out
    }

    /// Whether the replay made `function`'s `@type` its full signature before
    /// the comment's first `@template` reached it: the template arm then
    /// finds `FullSignature` set and leaves the list alone.
    fn jsdoc_type_tag_precedes_template(&self, doc: &tsr_ast::JSDoc<'_>, function: NodeId) -> bool {
        if self.jsdoc_reparsed_function(function).full_signature.is_none() {
            return false;
        }
        for tag in top_level_tags(doc.tags) {
            match tag {
                JSDocTag::JSDocTemplateTag(_) => return false,
                JSDocTag::JSDocTypeTag(tsr_ast::JSDocTypeTag {
                    type_expression: Some(_), ..
                }) => return true,
                _ => {}
            }
        }
        false
    }

    /// The kind of the declaration whose list a JSDoc `@template` parameter
    /// is reparsed into ([`Self::jsdoc_template_lists`]), which
    /// `checkGrammarModifiers` reads as the parameter's `Parent`; `None` for
    /// a written parameter or one no declaration takes.
    #[expect(
        dead_code,
        reason = "called from the hooks in docs/parity/notes/r6-jsdoc-template-grammar.diff"
    )]
    pub(crate) fn jsdoc_template_owner_kind(&self, parameter: NodeId) -> Option<SyntaxKind> {
        let tag = self.nodes.parent(parameter)?;
        if self.nodes.kind(tag) != SyntaxKind::JSDocTemplateTag {
            return None;
        }
        let mut root = tag;
        while let Some(parent) = self.nodes.parent(root) {
            root = parent;
        }
        let host = *self.jsdoc_hosts.get(&root)?;
        self.jsdoc_template_lists(host).into_iter().find_map(|(kind, list)| {
            list.iter().any(|candidate| candidate.node_id == Some(parameter)).then_some(kind)
        })
    }
}

/// The tags `parseJSDocSignature` (`parser/jsdoc.go:1121`) folds into an
/// `@overload` when the parse leaves them flat: the run of `@template`,
/// `@this` and `@param` tags after it, then one `@return` (the run
/// [`top_level_tags`] skips).
fn overload_signature<'t, 'a>(after: &'t [JSDocTag<'a>]) -> &'t [JSDocTag<'a>] {
    let mut end = 0;
    while let Some(tag) = after.get(end) {
        match tag {
            JSDocTag::JSDocTemplateTag(_) | JSDocTag::JSDocThisTag(_) => {}
            JSDocTag::JSDocParameterOrPropertyTag(parameter)
                if parameter.kind.kind == SyntaxKind::JSDocParameterTag => {}
            _ => break,
        }
        end += 1;
    }
    if matches!(after.get(end), Some(JSDocTag::JSDocReturnTag(_))) {
        end += 1;
    }
    &after[..end]
}

impl Checker<'_, '_> {
    /// `checkParameter`'s `this` arm (`checker.go:2680`) for the `this`
    /// parameter `reparseHosted`'s `KindJSDocThisTag` arm prefixes
    /// (`parser/reparser.go:487`): TS2681 on a constructor, TS2730 on an
    /// arrow function, TS2784 on an accessor, at the parameter — whose
    /// location is the tag's name (`finishReparsedNode(thisParam,
    /// tag.TagName())`). It is always first, so TS2680 never applies. The
    /// written `this` parameter is [`Checker::check_this_parameter_position`]'s.
    pub(crate) fn check_jsdoc_reparsed_this_parameter(&mut self, function: NodeId) {
        let message = match self.nodes.kind(function) {
            SyntaxKind::Constructor => &messages::A_CONSTRUCTOR_CANNOT_HAVE_A_THIS_PARAMETER,
            SyntaxKind::ArrowFunction => &messages::AN_ARROW_FUNCTION_CANNOT_HAVE_A_THIS_PARAMETER,
            SyntaxKind::GetAccessor | SyntaxKind::SetAccessor => {
                &messages::GET_AND_SET_ACCESSORS_CANNOT_DECLARE_THIS_PARAMETERS
            }
            _ => return,
        };
        let Some(this_tag) = self.jsdoc_reparsed_function(function).this_tag else { return };
        let Some(name) = this_tag.tag_name.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(function) else { return };
        let span = self.nodes.span(name);
        self.report(file, Diagnostic::new(message, span));
    }
}
