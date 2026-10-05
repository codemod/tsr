//! `@param` tags that name no parameter — TS8024, TS8029, TS8032.
//!
//! Ported from typescript-go's `Checker.checkUnmatchedJSDocParameters`
//! (`internal/checker/jsdoc.go:9`), which `checkSignatureDeclaration` runs for
//! every function-like declaration, with `getAllJSDocTags`
//! (`internal/checker/jsdoc.go:87`), `ast.GetNextJSDocCommentLocation`
//! (`internal/ast/utilities.go:4058`) and `Checker.containsArgumentsReference`
//! (`checker.go:32118`).
//!
//! `containsArgumentsReference` caches per node upstream; here it runs at most
//! once per function-like whose JSDoc carries a `@param` tag, which is the only
//! caller, so there is nothing to share and no cache.

use tsr_ast::{EntityName, JSDocTag, Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// `checkUnmatchedJSDocParameters` for one function-like node.
    pub(crate) fn check_unmatched_jsdoc_parameters(&mut self, node: NodeId) {
        // The kinds `checkSignatureDeclaration` is reached for.
        if !matches!(
            self.nodes.kind(node),
            SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::MethodSignature
                | SyntaxKind::CallSignature
                | SyntaxKind::ConstructSignature
                | SyntaxKind::IndexSignature
                | SyntaxKind::FunctionType
                | SyntaxKind::ConstructorType
        ) {
            return;
        }
        let Some(tags) = self.all_jsdoc_tags(node) else { return };
        let jsdoc_parameters: Vec<&'a tsr_ast::JSDocParameterOrPropertyTag<'a>> =
            top_level_parameter_tags(tags)
                .into_iter()
                .filter(|tag| !matches!(tag.name, Some(EntityName::Identifier(name)) if name.text.is_empty()))
                .collect();
        if jsdoc_parameters.is_empty() {
            return;
        }
        let is_js = self.in_js_file(node);
        let mut parameters: Vec<&str> = Vec::new();
        let mut excluded: Vec<usize> = Vec::new();
        for (index, parameter) in self.parameters_of(node).into_iter().enumerate() {
            let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(parameter) else {
                continue;
            };
            match parameter.name {
                Some(tsr_ast::BindingName::Identifier(name)) => parameters.push(name.text),
                Some(_) => excluded.push(index),
                None => {}
            }
        }
        if self.contains_arguments_reference(node) {
            if !is_js {
                return;
            }
            let last_index = jsdoc_parameters.len() - 1;
            let last = jsdoc_parameters[last_index];
            let Some(EntityName::Identifier(name)) = last.name else { return };
            if excluded.contains(&last_index) || parameters.contains(&name.text) {
                return;
            }
            let Some(annotation) = last.type_expression else { return };
            let annotation = match annotation {
                tsr_ast::TypeNode::JSDocTypeExpression(expression) => {
                    let Some(inner) = expression.r#type else { return };
                    inner
                }
                other => other,
            };
            let ty = self.get_type_from_type_node(annotation);
            if self.signature_array_element(ty).is_some() {
                return;
            }
            let Some(name_id) = name.node_id else { return };
            self.report_jsdoc_parameter(
                node,
                name_id,
                &messages::JSDOC_PARAM_TAG_HAS_NAME_0_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME_IT_WOULD_MATCH_ARGUMENTS_IF_IT_HAD_AN_ARRAY_TYPE,
                vec![name.text.to_string()],
            );
            return;
        }
        for (index, tag) in jsdoc_parameters.iter().enumerate() {
            let Some(name) = tag.name else { continue };
            if excluded.contains(&index)
                || matches!(name, EntityName::Identifier(identifier) if parameters.contains(&identifier.text))
            {
                continue;
            }
            match name {
                EntityName::QualifiedName(qualified) => {
                    if !is_js {
                        continue;
                    }
                    let (Some(id), Some(left)) = (qualified.node_id, qualified.left) else {
                        continue;
                    };
                    self.report_jsdoc_parameter(
                        node,
                        id,
                        &messages::QUALIFIED_NAME_0_IS_NOT_ALLOWED_WITHOUT_A_LEADING_PARAM_OBJECT_1,
                        vec![entity_name_text(name), entity_name_text(left)],
                    );
                }
                EntityName::Identifier(identifier) => {
                    // `errorOrSuggestion(isJs, …)`: a suggestion in a TS file,
                    // which no diagnostics list here carries.
                    if tag.is_name_first || !is_js {
                        continue;
                    }
                    let Some(id) = identifier.node_id else { continue };
                    self.report_jsdoc_parameter(
                        node,
                        id,
                        &messages::JSDOC_PARAM_TAG_HAS_NAME_0_BUT_THERE_IS_NO_PARAMETER_WITH_THAT_NAME,
                        vec![identifier.text.to_string()],
                    );
                }
            }
        }
    }

    fn report_jsdoc_parameter(
        &mut self,
        host: NodeId,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        args: Vec<String>,
    ) {
        // The JSDoc tree keeps no parent edge to its host, so the file is the
        // host function's.
        let Some(file) = self.source_file_of_for_diagnostics(host) else {
            return;
        };
        let span = self.nodes.span(at);
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// `getAllJSDocTags`: the tags of the last JSDoc comment at the first
    /// location in the `GetNextJSDocCommentLocation` chain that has any.
    fn all_jsdoc_tags(&self, node: NodeId) -> Option<&'a [JSDocTag<'a>]> {
        let mut current = Some(node);
        while let Some(location) = current {
            if let Some(docs) = self.jsdoc_entries.get(&location).copied()
                && let Some(last) = docs.last()
            {
                return (!last.tags.is_empty()).then_some(last.tags);
            }
            current = self.next_jsdoc_comment_location(location);
        }
        None
    }

    /// `ast.GetNextJSDocCommentLocation`.
    fn next_jsdoc_comment_location(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.nodes.parent(node)?;
        match self.nodes.kind(parent) {
            SyntaxKind::PropertyAssignment
            | SyntaxKind::ExportAssignment
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::VariableDeclaration
            | SyntaxKind::SatisfiesExpression
            | SyntaxKind::ReturnStatement
            | SyntaxKind::VariableStatement
            | SyntaxKind::ExpressionStatement => Some(parent),
            SyntaxKind::VariableDeclarationList => match self.node_map.get(parent) {
                Some(Node::VariableDeclarationList(list))
                    if list.declarations.first().and_then(|first| first.node_id) == Some(node) =>
                {
                    Some(parent)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// `containsArgumentsReference`: an `arguments` identifier that resolves to
    /// the implicit arguments object, outside nested lexical environments and
    /// type nodes.
    fn contains_arguments_reference(&self, node: NodeId) -> bool {
        let body = match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::FunctionExpression(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::MethodDeclaration(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::ConstructorDeclaration(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::GetAccessorDeclaration(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::SetAccessorDeclaration(n)) => n.body.and_then(|b| b.node_id()),
            Some(Node::ArrowFunction(n)) => n.body.and_then(|b| b.node_id()),
            _ => None,
        };
        body.is_some_and(|body| self.visit_arguments_reference(body))
    }

    fn visit_arguments_reference(&self, node: NodeId) -> bool {
        let Some(typed) = self.node_map.get(node) else { return false };
        match typed {
            Node::Identifier(identifier) => {
                return identifier.text == "arguments"
                    && self
                        .binder
                        .resolve_name(
                            self.nodes,
                            self.node_map,
                            node,
                            identifier.text,
                            SymbolFlags::VALUE,
                        )
                        .is_none();
            }
            Node::PropertyDeclaration(n) => {
                if let tsr_ast::PropertyName::ComputedPropertyName(name) = n.name {
                    return name.node_id.is_some_and(|id| self.visit_arguments_reference(id));
                }
            }
            Node::MethodDeclaration(n) => {
                if let tsr_ast::PropertyName::ComputedPropertyName(name) = n.name {
                    return name.node_id.is_some_and(|id| self.visit_arguments_reference(id));
                }
            }
            Node::GetAccessorDeclaration(n) => {
                if let tsr_ast::PropertyName::ComputedPropertyName(name) = n.name {
                    return name.node_id.is_some_and(|id| self.visit_arguments_reference(id));
                }
            }
            Node::SetAccessorDeclaration(n) => {
                if let tsr_ast::PropertyName::ComputedPropertyName(name) = n.name {
                    return name.node_id.is_some_and(|id| self.visit_arguments_reference(id));
                }
            }
            Node::PropertyAccessExpression(n) => {
                return n
                    .expression
                    .and_then(|e| e.node_id())
                    .is_some_and(|id| self.visit_arguments_reference(id));
            }
            Node::ElementAccessExpression(n) => {
                return n
                    .expression
                    .and_then(|e| e.node_id())
                    .is_some_and(|id| self.visit_arguments_reference(id));
            }
            Node::PropertyAssignment(n) => {
                return n
                    .initializer
                    .and_then(|e| e.node_id())
                    .is_some_and(|id| self.visit_arguments_reference(id));
            }
            _ => {}
        }
        // `nodeStartsNewLexicalEnvironment` (`utilities.go:1709`).
        if matches!(
            self.nodes.kind(node),
            SyntaxKind::Constructor
                | SyntaxKind::FunctionExpression
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                | SyntaxKind::ModuleDeclaration
                | SyntaxKind::SourceFile
        ) || tsr_ast::predicates::is_part_of_type_node(
            node,
            tsr_ast::predicates::Tree { nodes: self.nodes, map: self.node_map },
        ) {
            return false;
        }
        let mut found = false;
        tsr_ast::for_each_child_id(typed, |child| {
            if !found {
                found = self.visit_arguments_reference(child);
            }
        });
        found
    }
}

/// The `@param` tags upstream's JSDoc parser leaves at the top level of a
/// comment.
///
/// The parser already folds a callback's parameters
/// (`parseCallbackTagParameters`) and an `Object` parameter's `x.y` children
/// (`parseNestedTypeLiteral`) into their parent tag. `@overload` still parses
/// flat, so its signature's run — `@param`/`@template`/`@this` tags, then one
/// `@return` (`parseJSDocSignature`) — is skipped here.
fn top_level_parameter_tags<'a>(
    tags: &'a [JSDocTag<'a>],
) -> Vec<&'a tsr_ast::JSDocParameterOrPropertyTag<'a>> {
    let mut top = Vec::new();
    let mut index = 0;
    while let Some(tag) = tags.get(index) {
        index += 1;
        match tag {
            JSDocTag::JSDocOverloadTag(_) => {
                while matches!(
                    tags.get(index),
                    Some(
                        JSDocTag::JSDocTemplateTag(_)
                            | JSDocTag::JSDocThisTag(_)
                            | JSDocTag::JSDocParameterOrPropertyTag(_)
                    )
                ) && !matches!(tags.get(index),
                    Some(JSDocTag::JSDocParameterOrPropertyTag(child))
                        if child.kind.kind != SyntaxKind::JSDocParameterTag)
                {
                    index += 1;
                }
                if matches!(tags.get(index), Some(JSDocTag::JSDocReturnTag(_))) {
                    index += 1;
                }
            }
            JSDocTag::JSDocParameterOrPropertyTag(tag)
                if tag.kind.kind == SyntaxKind::JSDocParameterTag =>
            {
                top.push(*tag);
            }
            _ => {}
        }
    }
    top
}

/// `entityNameToString` for a JSDoc parameter name.
fn entity_name_text(name: EntityName<'_>) -> String {
    match name {
        EntityName::Identifier(identifier) => identifier.text.to_string(),
        EntityName::QualifiedName(qualified) => {
            let left = qualified.left.map(entity_name_text).unwrap_or_default();
            let right = qualified.right.map_or("", |right| right.text);
            format!("{left}.{right}")
        }
    }
}
