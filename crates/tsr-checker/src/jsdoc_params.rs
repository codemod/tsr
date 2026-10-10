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

use tsr_ast::{EntityName, JSDocTag, Node, NodeId, SyntaxKind, TypeNode};
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
    top_level_tags(tags)
        .into_iter()
        .filter_map(|tag| match tag {
            JSDocTag::JSDocParameterOrPropertyTag(tag)
                if tag.kind.kind == SyntaxKind::JSDocParameterTag =>
            {
                Some(*tag)
            }
            _ => None,
        })
        .collect()
}

/// The comment's tags as upstream's `JSDoc.Tags` lists them: an `@overload`
/// signature's run is part of the overload tag, not the comment.
pub(crate) fn top_level_tags<'a>(tags: &'a [JSDocTag<'a>]) -> Vec<&'a JSDocTag<'a>> {
    let mut top = Vec::new();
    let mut index = 0;
    while let Some(tag) = tags.get(index) {
        index += 1;
        match tag {
            // A flat-parsed run: no signature folded into the tag.
            JSDocTag::JSDocOverloadTag(overload) if overload.type_expression.is_none() => {
                top.push(tag);
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
            other => top.push(other),
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

/// What `reparseHosted` (`parser/reparser.go:342`) would have written onto a
/// function-like node from its documenting comment — the parts of the
/// reparsed tree a checker read sees through `param.Type`,
/// `param.QuestionToken` and `FunctionLikeData().FullSignature`.
///
/// This port has no reparser ([ADR-0046](../../../docs/adr/0046-jsdoc-reparse-is-a-checker-query.md)):
/// JSDoc stays in the `jsdoc_entries` side table and this is the one replay
/// of the hosted arms over it, so every consumer asks the same question
/// upstream asks of the mutated node.
#[derive(Default)]
pub(crate) struct JSDocReparsedFunction<'a> {
    /// `FullSignature`: the `@type` the function-like takes as its whole
    /// signature.
    pub(crate) full_signature: Option<TypeNode<'a>>,
    /// One slot per written parameter, in order — or empty when no comment
    /// is reparsed onto the function (every slot would be default).
    pub(crate) parameters: Vec<JSDocReparsedParameter<'a>>,
    /// The reparsed `fun.Type`: the first typed `@returns` of an
    /// unannotated function without a full signature.
    pub(crate) return_type: Option<TypeNode<'a>>,
    /// The `@this` tag whose reparsed `this` parameter prefixes the list
    /// (`reparseHosted`'s `KindJSDocThisTag` arm, `parser/reparser.go:487`):
    /// the first one, when the written list does not start with `this`.
    pub(crate) this_tag: Option<&'a tsr_ast::JSDocThisTag<'a>>,
}

/// One parameter's reparsed half: the `@param` tag `findMatchingParameter`
/// chose, and whether `makeQuestionIfOptional` gave it a `?`.
#[derive(Clone, Copy, Default)]
pub(crate) struct JSDocReparsedParameter<'a> {
    /// The matched `@param` tag, when it wrote the parameter's type or `?`.
    pub(crate) tag: Option<&'a tsr_ast::JSDocParameterOrPropertyTag<'a>>,
    /// A reparsed `?` (`NodeFlagsReparsed`): the tag is bracketed or its type
    /// is `T=`, and the parameter had no written `?`.
    pub(crate) question: bool,
    /// The reparsed `param.Type`: the type expression of the `@param` that
    /// typed an unannotated parameter.
    pub(crate) r#type: Option<TypeNode<'a>>,
}

impl<'a> Checker<'a, '_> {
    /// `reparseTags`' hosted half for `function`: the comments whose host
    /// `getFunctionLikeHost` (`parser/reparser.go:653`) resolves to it,
    /// each replayed in tag order over its **last** comment only (`isLast`).
    ///
    /// No cache: the walk is the function's own parameter list and one or
    /// two comments' tags, and only in a JS file; a TypeScript file returns
    /// before any lookup.
    pub(crate) fn jsdoc_reparsed_function(&self, function: NodeId) -> JSDocReparsedFunction<'a> {
        let mut out = JSDocReparsedFunction::default();
        // Hash lookups before anything else: this runs for every parameter
        // list `checkGrammarParameterList` sees, and almost none has a
        // comment. `parameters` stays empty (every slot default) unless a
        // comment is replayed.
        let hosts = [Some(function), self.jsdoc_function_like_host_of(function)];
        if !hosts.iter().flatten().any(|host| self.jsdoc_entries.contains_key(host)) {
            return out;
        }
        let Some(parts) = self.function_like_parts(function) else { return out };
        if !self.in_js_file(function) {
            return out;
        }
        out.parameters = vec![JSDocReparsedParameter::default(); parts.parameters.len()];
        // A node's comments are reparsed when it is finished, so the
        // function's own comment precedes its outer host's.
        let mut state = ReplayState {
            has_type_parameters: !parts.type_parameters.is_empty(),
            has_return_type: parts.return_type,
            // A parameter's own `@type` was reparsed when the parameter
            // finished, before any comment of the function's.
            typed: parts
                .parameters
                .iter()
                .map(|p| {
                    p.r#type.is_some()
                        || p.node_id.is_some_and(|id| self.jsdoc_parameter_hosted_type(id).is_some())
                })
                .collect(),
            this: if parts.parameters.first().is_some_and(|p| {
                matches!(p.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
            }) {
                ThisParameter::Written
            } else {
                ThisParameter::None
            },
            host_takes_type: HostTypeArm::None,
        };
        for host in hosts.into_iter().flatten() {
            let Some(doc) = self.jsdoc_entries.get(&host).and_then(|docs| docs.last()) else {
                continue;
            };
            state.host_takes_type = if host == function {
                match self.nodes.kind(function) {
                    SyntaxKind::GetAccessor if !parts.return_type => HostTypeArm::OwnType,
                    _ => HostTypeArm::None,
                }
            } else {
                self.host_type_arm(host)
            };
            Self::replay_hosted_tags(doc, parts.parameters, &mut state, &mut out);
        }
        out
    }

    /// The type `reparseHosted`'s `KindJSDocTypeTag` arm writes onto an
    /// unannotated parameter from the parameter's **own** comment
    /// (`parser/reparser.go:363`, `case ast.KindParameter`):
    /// `function f(/** @type {string} */ message)`.
    ///
    /// Only the last comment is hosted (`reparseTags`' `isLast`), and the first
    /// `@type` tag with a type expression wins: once it has set `Type`, a later
    /// tag fails the `parent.Type() == nil` test and falls through to
    /// `getFunctionLikeHost`, which a parameter never is. A parameter is
    /// finished before its function, so this precedes every `@param` and
    /// `@type` the function's own comments reparse onto it.
    ///
    /// No cache: one hash lookup that almost always misses, then one comment's
    /// tags, and only in a JS file.
    pub(crate) fn jsdoc_parameter_hosted_type(&self, parameter: NodeId) -> Option<TypeNode<'a>> {
        let doc = self.jsdoc_entries.get(&parameter)?.last()?;
        if !self.in_js_file(parameter) {
            return None;
        }
        match self.node_map.get(parameter)? {
            Node::ParameterDeclaration(declaration) if declaration.r#type.is_none() => {}
            _ => return None,
        }
        doc.tags.iter().find_map(|tag| match tag {
            JSDocTag::JSDocTypeTag(tag) => match tag.type_expression {
                Some(Node::JSDocTypeExpression(expression)) => expression.r#type,
                _ => None,
            },
            _ => None,
        })
    }

    /// The `Type` `reparseHosted`'s `KindJSDocTypeTag` arm
    /// (`parser/reparser.go:356`) gives a declaration from its **own**
    /// comment, for the hosts `jsdoc_type_annotation`'s statement walk does
    /// not reach: an unannotated class property (`#private` and computed
    /// names included), an object-literal property assignment and a
    /// catch-clause variable. The first `@type` with a
    /// type expression in the last comment wins (`reparseTags`' `isLast`;
    /// a later tag fails `parent.Type() == nil`).
    ///
    /// No cache: one `jsdoc_entries` probe that almost always misses, then
    /// one comment's tags, and only in a JS file.
    pub(crate) fn jsdoc_self_hosted_type(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        let doc = self.jsdoc_entries.get(&declaration)?.last()?;
        if !self.in_js_file(declaration) {
            return None;
        }
        match self.node_map.get(declaration)? {
            Node::PropertyDeclaration(property) if property.r#type.is_none() => {}
            Node::PropertyAssignment(property) if property.r#type.is_none() => {}
            Node::VariableDeclaration(variable)
                if variable.r#type.is_none()
                    && self.nodes.parent(declaration).is_some_and(|parent| {
                        self.nodes.kind(parent) == SyntaxKind::CatchClause
                    }) => {}
            _ => return None,
        }
        doc.tags.iter().find_map(|tag| match tag {
            JSDocTag::JSDocTypeTag(tag) => match tag.type_expression {
                Some(Node::JSDocTypeExpression(expression)) => expression.r#type,
                _ => None,
            },
            _ => None,
        })
    }

    /// Whether `makeQuestionIfOptional` (`parser/reparser.go`) gave
    /// `parameter` a reparsed `?`: its function's last comment has a
    /// matching `@param [x]` or `@param {T=} x`, and nothing wrote a `?`.
    /// Upstream reads that token through `param.QuestionToken` everywhere
    /// it reads a written one (`isOptionalDeclaration`,
    /// `getSignatureFromDeclaration`'s `isOptionalParameter`).
    ///
    /// No cache: [`Self::jsdoc_reparsed_function`] answers with hash lookups
    /// for the common uncommented function.
    pub(crate) fn jsdoc_reparsed_parameter_question(&self, parameter: NodeId) -> bool {
        let Some(function) = self.nodes.parent(parameter) else { return false };
        let Some(parts) = self.function_like_parts(function) else { return false };
        let Some(index) = parts.parameters.iter().position(|p| p.node_id == Some(parameter)) else {
            return false;
        };
        self.jsdoc_reparsed_function(function)
            .parameters
            .get(index)
            .is_some_and(|slot| slot.question)
    }

    /// `param.Type()` as the reparser leaves it for an unannotated JS
    /// parameter: its own `@type` ([`Self::jsdoc_parameter_hosted_type`]),
    /// else the type of the `@param` its function's comment matched to it.
    /// `None` for a parameter with a written type, which needs no reparse.
    ///
    /// No cache, as for [`Self::jsdoc_reparsed_parameter_question`].
    pub(crate) fn jsdoc_reparsed_parameter_type(&self, parameter: NodeId) -> Option<TypeNode<'a>> {
        if let Some(hosted) = self.jsdoc_parameter_hosted_type(parameter) {
            return Some(hosted);
        }
        let function = self.nodes.parent(parameter)?;
        let parts = self.function_like_parts(function)?;
        let index = parts.parameters.iter().position(|p| p.node_id == Some(parameter))?;
        if parts.parameters[index].r#type.is_some() {
            return None;
        }
        self.jsdoc_reparsed_function(function).parameters.get(index)?.r#type
    }

    /// `getAnnotatedAccessorTypeNode` (`checker.go:20106`) over the reparsed
    /// tree, for an accessor with no written annotation: a getter's
    /// `node.Type()` (its own `@type`, else its `@returns`), and a setter's
    /// `getEffectiveSetAccessorTypeAnnotationNode` — the type of
    /// `GetSetAccessorValueParameter`, the first parameter that is not
    /// `this`, which in JS is its reparsed `@param` (or its own `@type`).
    ///
    /// No cache: the replay answers from hash lookups when no comment
    /// applies.
    pub(crate) fn jsdoc_accessor_annotation(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::GetAccessorDeclaration(getter) if getter.r#type.is_none() => {
                if !self.in_js_file(declaration) {
                    return None;
                }
                let reparsed = self.jsdoc_reparsed_function(declaration);
                if reparsed.full_signature.is_some() {
                    return None;
                }
                reparsed.return_type
            }
            Node::SetAccessorDeclaration(setter) => {
                // `GetSetAccessorValueParameter` (`ast/utilities.go`).
                let has_this = setter.parameters.len() == 2
                    && matches!(setter.parameters[0].name,
                        Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this");
                let value = setter.parameters.get(usize::from(has_this))?;
                if value.r#type.is_some() {
                    return None;
                }
                self.jsdoc_reparsed_parameter_type(value.node_id?)
            }
            _ => None,
        }
    }

    /// The written type-parameter list, parameters and return-annotation
    /// presence of a function-like declaration.
    pub(crate) fn function_like_parts(&self, node: NodeId) -> Option<FunctionLikeParts<'a>> {
        let (type_parameters, parameters, return_type) = match self.node_map.get(node)? {
            Node::FunctionDeclaration(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::FunctionExpression(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::ArrowFunction(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::MethodDeclaration(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::ConstructorDeclaration(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::GetAccessorDeclaration(n) => (n.type_parameters, n.parameters, n.r#type),
            Node::SetAccessorDeclaration(n) => (n.type_parameters, n.parameters, n.r#type),
            _ => return None,
        };
        Some(FunctionLikeParts { type_parameters, parameters, return_type: return_type.is_some() })
    }

    /// The inverse of `getFunctionLikeHost` for a host other than the
    /// function itself: the variable statement whose *first* declaration it
    /// initializes, the property it initializes, the export or return it is
    /// the expression of, or the expression statement whose right-most
    /// assigned expression it is — through any `satisfies` wrappers
    /// (`skipSatisfiesExpressions`), but not through parentheses.
    fn jsdoc_function_like_host_of(&self, function: NodeId) -> Option<NodeId> {
        let mut child = function;
        let mut parent = self.nodes.parent(child)?;
        while self.nodes.kind(parent) == SyntaxKind::SatisfiesExpression {
            child = parent;
            parent = self.nodes.parent(child)?;
        }
        let is = |expression: Option<tsr_ast::Expression<'_>>| {
            expression.and_then(|e| e.node_id()) == Some(child)
        };
        match self.node_map.get(parent)? {
            Node::VariableDeclaration(declaration) if is(declaration.initializer) => {
                let list = self.nodes.parent(parent)?;
                let Some(Node::VariableDeclarationList(declarations)) = self.node_map.get(list)
                else {
                    return None;
                };
                if declarations.declarations.first().and_then(|first| first.node_id) != Some(parent)
                {
                    return None;
                }
                let statement = self.nodes.parent(list)?;
                (self.nodes.kind(statement) == SyntaxKind::VariableStatement).then_some(statement)
            }
            Node::PropertyAssignment(property) if is(property.initializer) => Some(parent),
            Node::PropertyDeclaration(property) if is(property.initializer) => Some(parent),
            Node::ExportAssignment(export) if is(export.expression) => Some(parent),
            Node::ReturnStatement(statement) if is(statement.expression) => Some(parent),
            Node::BinaryExpression(_) => {
                // `GetRightMostAssignedExpression` from the statement's
                // expression down: climb assignments while `child` is the
                // right operand.
                let mut current = child;
                let mut up = parent;
                loop {
                    match self.node_map.get(up)? {
                        Node::BinaryExpression(binary)
                            if is_assignment(binary)
                                && binary.right.and_then(|e| e.node_id()) == Some(current) =>
                        {
                            current = up;
                            up = self.nodes.parent(up)?;
                        }
                        Node::ExpressionStatement(statement)
                            if current != child
                                && statement.expression.and_then(|e| e.node_id())
                                    == Some(current) =>
                        {
                            return Some(up);
                        }
                        _ => return None,
                    }
                }
            }
            _ => None,
        }
    }

    /// Which of `reparseHosted`'s `KindJSDocTypeTag` arms ahead of the
    /// function-like one a `@type` on `host` lands in.
    fn host_type_arm(&self, host: NodeId) -> HostTypeArm {
        match self.node_map.get(host) {
            Some(Node::VariableStatement(statement)) => {
                let untyped = statement.declaration_list.map_or(0, |list| {
                    list.declarations.iter().filter(|d| d.r#type.is_none()).count()
                });
                HostTypeArm::Times(untyped)
            }
            Some(Node::PropertyDeclaration(property)) if property.r#type.is_none() => {
                HostTypeArm::Once
            }
            // Neither carries a written annotation; the reparsed one makes
            // the next `@type` fall through.
            Some(Node::PropertyAssignment(_) | Node::ExportAssignment(_)) => HostTypeArm::Once,
            // `makeNewCast` wraps the expression every time.
            Some(Node::ReturnStatement(_)) => HostTypeArm::Always,
            Some(Node::ExpressionStatement(statement)) => {
                let declares = match statement.expression {
                    Some(tsr_ast::Expression::BinaryExpression(binary)) => {
                        self.is_assignment_declaration(binary)
                    }
                    _ => false,
                };
                if declares { HostTypeArm::Always } else { HostTypeArm::None }
            }
            _ => HostTypeArm::None,
        }
    }

    /// `BinaryExpression.Type` as `reparseHosted`'s `KindJSDocTypeTag` arm
    /// sets it (`parser/reparser.go:369`): in a JS file, an assignment
    /// declaration that is its statement's expression takes the statement's
    /// `@type`. Read by `getContextualTypeForBinaryOperand`'s first line
    /// (`checker.go:29811`).
    pub(crate) fn jsdoc_binary_type(
        &self,
        binary_id: NodeId,
        binary: &tsr_ast::BinaryExpression<'_>,
    ) -> Option<TypeNode<'a>> {
        let statement = self.nodes.parent(binary_id)?;
        if self.nodes.kind(statement) != SyntaxKind::ExpressionStatement
            || !self.in_js_file(binary_id)
            || !self.is_assignment_declaration(binary)
        {
            return None;
        }
        self.jsdoc_cast_annotation(statement)
    }

    /// An `ExportAssignment`'s `Type()`: only ever the `@type` that
    /// `reparseHosted`'s `KindJSDocTypeTag` arm (`parser/reparser.go:356`)
    /// copies onto it in a JS file — the type `checkExportAssignment`
    /// checks the expression against (`check_jsdoc_annotated_initializer`)
    /// and `getContextualType`'s `KindExportAssignment` arm answers.
    pub(crate) fn jsdoc_export_assignment_type(&self, node: NodeId) -> Option<TypeNode<'a>> {
        if !self.in_js_file(node) {
            return None;
        }
        self.jsdoc_cast_annotation(node)
    }

    /// The parent of the `JSImportDeclaration` that `reparseUnhosted`'s
    /// `KindJSDocImportTag` arm (`parser/reparser.go:119`) makes of an
    /// `@import` tag: `parseListIndex` (`parser.go:613`) propagates it out
    /// of every list but a source file's or a block's statements, so it is a
    /// statement of the nearest `SourceFile`, `Block` or `ModuleBlock`
    /// enclosing the comment's host. `getAnyImportSyntax`'s callers ask
    /// whether that parent is visible (`hasVisibleDeclarations`).
    pub(crate) fn jsdoc_import_declaration_parent(&self, import_tag: NodeId) -> Option<NodeId> {
        let mut current = import_tag;
        while let Some(parent) = self.nodes.parent(current) {
            current = parent;
        }
        let mut current = self.nodes.parent(*self.jsdoc_hosts.get(&current)?)?;
        loop {
            if matches!(
                self.nodes.kind(current),
                SyntaxKind::SourceFile | SyntaxKind::Block | SyntaxKind::ModuleBlock
            ) {
                return Some(current);
            }
            current = self.nodes.parent(current)?;
        }
    }

    /// `getFunctionLikeHost` (`parser/reparser.go:653`): the function-like
    /// node a comment on `host` reparses onto — the host itself, the first
    /// declaration's initializer of a variable statement, a property's
    /// initializer, an export or return expression, or an expression
    /// statement's right-most assigned expression, through `satisfies`.
    pub(crate) fn jsdoc_function_like_host(&self, host: NodeId) -> Option<NodeId> {
        let expression = match self.node_map.get(host)? {
            Node::VariableStatement(statement) => {
                statement.declaration_list?.declarations.first()?.initializer
            }
            Node::PropertyAssignment(property) => property.initializer,
            Node::PropertyDeclaration(property) => property.initializer,
            Node::ExportAssignment(export) => export.expression,
            Node::ReturnStatement(statement) => statement.expression,
            Node::ExpressionStatement(statement) => {
                let mut current = statement.expression?;
                while let tsr_ast::Expression::BinaryExpression(binary) = current
                    && binary.operator_token.map(|t| t.kind) == Some(SyntaxKind::EqualsToken)
                {
                    current = binary.right?;
                }
                Some(current)
            }
            _ => return self.function_like_parts(host).map(|_| host),
        };
        let mut current = expression?;
        while let tsr_ast::Expression::SatisfiesExpression(satisfies) = current {
            current = satisfies.expression?;
        }
        let id = current.node_id()?;
        self.function_like_parts(id).map(|_| id)
    }

    /// The function a reparsed `@returns` type node is the `Type` of
    /// (`reparseHosted`'s `KindJSDocReturnTag` arm, `parser/reparser.go:514`),
    /// for `getTypePredicateParent` (`checker.go:3099`): native parents the
    /// clone under the function, so a `@returns {x is T}` predicate is in
    /// return-type position. `None` for any other node in a comment.
    pub(crate) fn jsdoc_reparsed_return_owner(&self, type_node: NodeId) -> Option<NodeId> {
        let mut root = type_node;
        while let Some(parent) = self.nodes.parent(root) {
            root = parent;
        }
        let host = *self.jsdoc_hosts.get(&root)?;
        let function = self.jsdoc_function_like_host(host)?;
        let parts = self.function_like_parts(function)?;
        if parts.return_type {
            return None;
        }
        let reparsed = self.jsdoc_reparsed_function(function);
        if reparsed.full_signature.is_some() {
            return None;
        }
        // The replay keeps the tag's `{…}` wrapper; native's `Type` is the
        // clone of what it wraps (`tag.TypeExpression().Type()`).
        let returned = match reparsed.return_type? {
            TypeNode::JSDocTypeExpression(expression) => expression.r#type?,
            other => other,
        };
        (Node::from(returned).node_id() == Some(type_node)).then_some(function)
    }

    /// The type parameters `reparseHosted`'s `KindJSDocTemplateTag` class
    /// arms (`parser/reparser.go:459-470`) give an unparameterised JS class:
    /// `gatherTypeParameters(jsDoc, false)` (`:293`) over its last comment —
    /// every `@template` tag's parameters in order, none when the comment
    /// declares a typedef or callback. Empty for any other node.
    pub(crate) fn jsdoc_class_template_parameters(
        &self,
        class: NodeId,
    ) -> Vec<&'a tsr_ast::TypeParameterDeclaration<'a>> {
        let written = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(node)) => node.type_parameters,
            Some(Node::ClassExpression(node)) => node.type_parameters,
            _ => return Vec::new(),
        };
        if !written.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.jsdoc_entries.get(&class).and_then(|docs| docs.last()) else {
            return Vec::new();
        };
        if !self.in_js_file(class) {
            return Vec::new();
        }
        let mut parameters = Vec::new();
        for tag in doc.tags {
            match tag {
                JSDocTag::JSDocTypedefTag(_) | JSDocTag::JSDocCallbackTag(_) => return Vec::new(),
                JSDocTag::JSDocTemplateTag(template) => {
                    parameters.extend_from_slice(template.type_parameters);
                }
                _ => {}
            }
        }
        parameters
    }

    /// Whether `getIntendedTypeFromJSDocTypeReference` (`checker.go:23020`)
    /// answers `reference` through its `Object` arm with two type arguments
    /// (`Object.<K, V>`: a `Record` instantiation, or `any`) — a JSDoc
    /// reference that never reaches `getTypeFromClassOrInterfaceReference`,
    /// so neither its arity rule nor `checkNoTypeArguments` runs on it.
    pub(crate) fn is_jsdoc_record_object_reference(&self, reference: NodeId) -> bool {
        let Some(Node::TypeReferenceNode(node)) = self.node_map.get(reference) else {
            return false;
        };
        if node.type_arguments.len() != 2
            || !matches!(node.type_name, Some(EntityName::Identifier(name)) if name.text == "Object")
        {
            return false;
        }
        // `node.Flags & NodeFlagsJSDoc`: the reference sits inside a comment.
        let mut current = self.nodes.parent(reference);
        while let Some(ancestor) = current {
            if self.nodes.kind(ancestor) == SyntaxKind::JSDoc {
                return true;
            }
            current = self.nodes.parent(ancestor);
        }
        false
    }

    /// The constraint `gatherTypeParameters` (`parser/reparser.go:293`)
    /// gives a JSDoc type parameter: an `@template {C} T, U` tag's `{C}`
    /// becomes the reparsed constraint of its **first** parameter only.
    pub(crate) fn jsdoc_template_constraint(&self, parameter: NodeId) -> Option<TypeNode<'a>> {
        let tag = self.nodes.parent(parameter)?;
        let Some(Node::JSDocTemplateTag(template)) = self.node_map.get(tag) else {
            return None;
        };
        if template.type_parameters.first()?.node_id != Some(parameter) {
            return None;
        }
        match template.constraint? {
            Node::JSDocTypeExpression(expression) => expression.r#type,
            other => TypeNode::try_from(other).ok(),
        }
    }

    /// `ast.GetAssignmentDeclarationKind(bin) != JSDeclarationKindNone` for a
    /// binary expression in a JS file (`ast/utilities.go:1541`): `=` with an
    /// access-expression left whose object is `this`, `module.exports`,
    /// `exports` or an entity name.
    pub(crate) fn is_assignment_declaration(&self, binary: &tsr_ast::BinaryExpression<'_>) -> bool {
        if binary.operator_token.map(|t| t.kind) != Some(SyntaxKind::EqualsToken) {
            return false;
        }
        let object = match binary.left {
            Some(tsr_ast::Expression::PropertyAccessExpression(access)) => {
                if !matches!(access.name, Some(tsr_ast::MemberName::Identifier(_))) {
                    // `this.#x = …`: only the JS arms above the entity-name
                    // one accept a private name.
                    return is_this(access.expression);
                }
                access.expression
            }
            Some(tsr_ast::Expression::ElementAccessExpression(access)) => access.expression,
            _ => return false,
        };
        match object {
            Some(_) if is_this(object) => true,
            Some(expression) => {
                expression.node_id().is_some_and(|id| self.is_entity_name_expression(id))
            }
            None => false,
        }
    }

    /// The hosted arms that write a function-like's parameters, return type,
    /// type parameters and full signature, over one comment's tags in order.
    fn replay_hosted_tags(
        doc: &'a tsr_ast::JSDoc<'a>,
        parameters: &'a [&'a tsr_ast::ParameterDeclaration<'a>],
        state: &mut ReplayState,
        out: &mut JSDocReparsedFunction<'a>,
    ) {
        let tags = top_level_tags(doc.tags);
        let gathers_type_parameters = !tags
            .iter()
            .any(|tag| matches!(tag, JSDocTag::JSDocTypedefTag(_) | JSDocTag::JSDocCallbackTag(_)))
            && tags.iter().any(|tag| {
                matches!(tag, JSDocTag::JSDocTemplateTag(template)
                if !template.type_parameters.is_empty())
            });
        let mut parameter_tag_index = 0usize;
        for tag in &tags {
            match tag {
                JSDocTag::JSDocTypeTag(type_tag) => {
                    let annotation = match type_tag.type_expression {
                        Some(Node::JSDocTypeExpression(expression)) => expression.r#type,
                        _ => None,
                    };
                    if annotation.is_none() {
                        continue;
                    }
                    match state.host_takes_type {
                        HostTypeArm::Always => continue,
                        // The getter's own `Type`: its return annotation.
                        HostTypeArm::OwnType => {
                            state.host_takes_type = HostTypeArm::None;
                            state.has_return_type = true;
                            out.return_type = annotation;
                            continue;
                        }
                        HostTypeArm::Once => {
                            state.host_takes_type = HostTypeArm::None;
                            continue;
                        }
                        HostTypeArm::Times(n) if n > 0 => {
                            state.host_takes_type = HostTypeArm::Times(n - 1);
                            continue;
                        }
                        _ => {}
                    }
                    let untyped = state.this != ThisParameter::Reparsed { typed: true }
                        && !state.typed.contains(&true);
                    if !state.has_type_parameters && !state.has_return_type && untyped {
                        out.full_signature = annotation;
                    }
                }
                JSDocTag::JSDocTemplateTag(_)
                    if out.full_signature.is_none() && !state.has_type_parameters =>
                {
                    state.has_type_parameters = gathers_type_parameters;
                }
                JSDocTag::JSDocParameterOrPropertyTag(parameter_tag)
                    if parameter_tag.kind.kind == SyntaxKind::JSDocParameterTag =>
                {
                    let tag_index = parameter_tag_index;
                    parameter_tag_index += 1;
                    if out.full_signature.is_some() {
                        continue;
                    }
                    // `findMatchingParameter` indexes `fun.Parameters()`,
                    // which a reparsed `@this` has already prefixed.
                    let Some(index) = find_matching_parameter(
                        parameters,
                        parameter_tag,
                        tag_index,
                        matches!(state.this, ThisParameter::Reparsed { .. }),
                    ) else {
                        continue;
                    };
                    let parameter = parameters[index];
                    let slot = &mut out.parameters[index];
                    if parameter_tag.type_expression.is_some() && !state.typed[index] {
                        state.typed[index] = true;
                        slot.tag = Some(parameter_tag);
                        slot.r#type = parameter_tag.type_expression;
                    }
                    if parameter.question_token.is_none()
                        && !slot.question
                        && make_question_if_optional(parameter_tag)
                    {
                        slot.question = true;
                        slot.tag = Some(parameter_tag);
                    }
                }
                JSDocTag::JSDocThisTag(this_tag) => {
                    // A `this` parameter is prefixed unless the list
                    // already starts with one; typed when the tag is, which
                    // disqualifies a later `@type`.
                    if state.this == ThisParameter::None {
                        state.this =
                            ThisParameter::Reparsed { typed: this_tag.type_expression.is_some() };
                        out.this_tag = Some(this_tag);
                    }
                }
                JSDocTag::JSDocReturnTag(return_tag) if out.full_signature.is_none() => {
                    if !state.has_return_type {
                        out.return_type = return_tag.type_expression;
                    }
                    state.has_return_type |= return_tag.type_expression.is_some();
                }
                _ => {}
            }
        }
    }
}

/// The written shape [`Checker::jsdoc_reparsed_function`] reads.
pub(crate) struct FunctionLikeParts<'a> {
    pub(crate) type_parameters: &'a [&'a tsr_ast::TypeParameterDeclaration<'a>],
    pub(crate) parameters: &'a [&'a tsr_ast::ParameterDeclaration<'a>],
    pub(crate) return_type: bool,
}

/// Where a `@type` on a non-function host goes before the function-like arm.
#[derive(Clone, Copy)]
enum HostTypeArm {
    /// Straight to the function-like arm.
    None,
    /// A getter's own comment: the first `@type` is its return type
    /// (`reparseHosted`'s `KindGetAccessor` case).
    OwnType,
    /// The host's own annotation, the first time only.
    Once,
    /// One untyped variable declaration per tag.
    Times(usize),
    /// Every time (a cast, or an assignment declaration's type).
    Always,
}

/// The function-like's reparse state between tags.
struct ReplayState {
    has_type_parameters: bool,
    has_return_type: bool,
    /// Per written parameter: has a type, written or reparsed.
    typed: Vec<bool>,
    this: ThisParameter,
    host_takes_type: HostTypeArm,
}

/// Whether the parameter list starts with `this`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ThisParameter {
    None,
    Written,
    /// A `@this` prefixed one; typed when the tag is.
    Reparsed {
        typed: bool,
    },
}

/// `findMatchingParameter` (`parser/reparser.go:609`): the parameter named
/// like the tag, or the one at the tag's position among the comment's
/// `@param` tags when the parameter is a binding pattern or the tag's name is
/// empty. Indices are upstream's, over a list a reparsed `this` may prefix;
/// matching that synthetic parameter yields no written one.
fn find_matching_parameter(
    parameters: &[&tsr_ast::ParameterDeclaration<'_>],
    tag: &tsr_ast::JSDocParameterOrPropertyTag<'_>,
    tag_index: usize,
    this_prepended: bool,
) -> Option<usize> {
    let tag_name = match tag.name {
        Some(EntityName::Identifier(name)) => Some(name.text),
        _ => None,
    };
    let matches_identifier = |text: &str, index: usize| {
        tag_name
            .is_some_and(|tag_text| text == tag_text || (index == tag_index && tag_text.is_empty()))
    };
    let offset = usize::from(this_prepended);
    if this_prepended && matches_identifier("this", 0) {
        return None;
    }
    parameters.iter().enumerate().position(|(written, parameter)| {
        let index = written + offset;
        match parameter.name {
            Some(tsr_ast::BindingName::Identifier(name)) => matches_identifier(name.text, index),
            _ => index == tag_index,
        }
    })
}

/// `makeQuestionIfOptional` (`parser/reparser.go:597`): a bracketed name or
/// a `T=` type.
pub(crate) fn make_question_if_optional(tag: &tsr_ast::JSDocParameterOrPropertyTag<'_>) -> bool {
    tag.is_bracketed
        || matches!(tag.type_expression,
            Some(TypeNode::JSDocTypeExpression(expression))
                if matches!(expression.r#type, Some(TypeNode::JSDocOptionalType(_))))
}

fn is_this(expression: Option<tsr_ast::Expression<'_>>) -> bool {
    matches!(expression, Some(tsr_ast::Expression::KeywordExpression(keyword))
        if keyword.kind == SyntaxKind::ThisKeyword)
}

/// `IsAssignmentExpression(node, false)`: any assignment operator.
fn is_assignment(binary: &tsr_ast::BinaryExpression<'_>) -> bool {
    binary.operator_token.is_some_and(|token| {
        matches!(
            token.kind,
            SyntaxKind::EqualsToken
                | SyntaxKind::PlusEqualsToken
                | SyntaxKind::MinusEqualsToken
                | SyntaxKind::AsteriskEqualsToken
                | SyntaxKind::AsteriskAsteriskEqualsToken
                | SyntaxKind::SlashEqualsToken
                | SyntaxKind::PercentEqualsToken
                | SyntaxKind::LessThanLessThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanEqualsToken
                | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                | SyntaxKind::AmpersandEqualsToken
                | SyntaxKind::BarEqualsToken
                | SyntaxKind::CaretEqualsToken
                | SyntaxKind::BarBarEqualsToken
                | SyntaxKind::AmpersandAmpersandEqualsToken
                | SyntaxKind::QuestionQuestionEqualsToken
        )
    })
}
