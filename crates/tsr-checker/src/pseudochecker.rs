//! The pseudochecker: the type a declaration or expression has by syntax
//! alone, ported from `internal/pseudochecker/lookup.go` (pinned `5b1047d`).
//!
//! # What is ported, and how it is asked
//!
//! Upstream's pseudochecker maps a declaration to a `PseudoType`, a skeleton
//! of the type it *should* have built from syntax alone, and the node builder
//! reuses it when `pseudoTypeEquivalentToType` holds against the checker's
//! type (`checker/pseudotypenodebuilder.go:362`). Two answers are ported:
//!
//! - the DIRECT kind on its own ([`Checker::pseudo_direct_return_node`],
//!   [`Checker::pseudo_direct_declaration_node`]): `Some(node)` exactly where
//!   upstream answers `NewPseudoTypeDirect(node)`. Every printer, site-free
//!   ones included, asks these (`docs/parity/notes/r5-nodereuse.md` §3);
//! - the whole tree, [`PseudoType`] ([`Checker::pseudo_return_type`],
//!   [`Checker::pseudo_type_of_declaration`]), whose STRUCTURAL kinds (an
//!   object literal, a single call signature, a `const` tuple) print
//!   differently from the type: an annotated get/set pair keeps its
//!   accessors, and a returned function keeps its written nodes in the
//!   scope of the signature that returns it. It is built only where the
//!   Direct answer is none, and only for a printer with a site
//!   (`docs/parity/notes/r5-nodereuse2.md` §2).
//!
//! The Direct node is then handed to the existing reuse machinery
//! ([`crate::node_reuse`]): its type is `getTypeFromTypeNode(node)`
//! (`pseudoTypeToType`'s Direct arm, `pseudotypenodebuilder.go:694`), the
//! identity arm of the equivalence is asked at print time, and the visitor
//! re-emits the node.
//!
//! # Port-convention boundary (`docs/conventions.md`)
//!
//! - **Native operation:** `GetReturnTypeOfSignature` →
//!   `createReturnFromSignature` → `typeFromSingleReturnExpression`
//!   (`lookup.go:198`, `:211`), and `typeFromExpression`'s assertion arms
//!   (`:254`, `:510`), consumed by `serializeReturnTypeForSignature`
//!   (`nodebuilderimpl.go:2023`).
//! - **Key identity and owner:** none. No cache, no side table: the answer is
//!   a pure function of the syntax tree, as upstream's is. A [`PseudoType`]
//!   tree is built per print and dropped with it.
//! - **Publication states:** none; nothing is stored.
//! - **Receiver/alias context:** the node is asked of the signature's own
//!   declaration; an instantiated signature whose return type the
//!   instantiation moved fails the print-time identity gate, as upstream's
//!   equivalence does.
//! - **Expensive work boundary:** upstream asks at PRINT time, and so does this
//!   port ([`crate::Checker::reused_return_text`]): the body's return
//!   statements are walked once per printed unannotated signature, never on
//!   the checking path. The structural tree walks a returned function or an
//!   object-literal initializer once more, and only for a site-aware print:
//!   no baked type text builds it.

use crate::Checker;
use tsr_ast::{
    ConciseBody, Expression, FunctionBody, ModifierLike, Node, NodeId, SyntaxKind, TypeNode,
};

impl<'a> Checker<'a, '_> {
    /// `createReturnFromSignature` (`lookup.go:198`) restricted to its Direct
    /// answers: a function-like declaration's written return annotation,
    /// else, for a value-signature declaration, the body's
    /// `typeFromSingleReturnExpression` (`:211`).
    pub(crate) fn pseudo_direct_return_node(&self, declaration: NodeId) -> Option<TypeNode<'a>> {
        if let Some(annotation) = self.function_like_return_annotation(declaration) {
            return Some(annotation);
        }
        // `isValueSignatureDeclaration` (`lookup.go:194`), with the body.
        let (modifiers, asterisk, body): (&[ModifierLike<'a>], bool, _) =
            match self.node_map.get(declaration)? {
                Node::FunctionDeclaration(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::FunctionExpression(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::MethodDeclaration(node) if node.r#type.is_none() => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Node::ArrowFunction(node) if node.r#type.is_none() => {
                    (node.modifiers, false, node.body)
                }
                _ => return None,
            };
        let body = body?;
        // `FunctionFlagsAsyncGenerator`: an async generator is Inferred.
        let is_async = modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
        });
        if is_async && asterisk {
            return None;
        }
        let candidate = match body {
            ConciseBody::Block(block) => self.single_direct_return_expression(block)?,
            expression => Expression::try_from(Node::from(expression)).ok()?,
        };
        let candidate_id = Node::from(candidate).node_id()?;
        if self.pseudo_is_contextually_typed(candidate_id) {
            // A contextually typed candidate is Direct only when it is itself
            // a non-`const` assertion.
            let annotation = match candidate {
                Expression::AsExpression(node) => node.r#type?,
                Expression::TypeAssertion(node) => node.r#type?,
                _ => return None,
            };
            return (!crate::assertions::is_const_type_reference(annotation)).then_some(annotation);
        }
        self.pseudo_direct_type_of_expression(candidate)
    }

    /// `GetTypeOfDeclaration` (`lookup.go:34`) restricted to its Direct
    /// answers, for the declarations a printed property symbol can have:
    ///
    /// - a property signature or declaration: its annotation; an
    ///   unannotated property declaration's non-contextual initializer
    ///   (`typeFromProperty`, `:96`);
    /// - an object literal's property assignment: its initializer
    ///   (`typeFromPropertyAssignment`, `:66`);
    /// - a variable: its annotation, else a non-contextual initializer of a
    ///   variable with a single declaration (`typeFromVariable`, `:120`);
    /// - a parameter without an initializer: its annotation
    ///   (`typeFromParameter`'s fast path, `:620`). An initialized parameter
    ///   may need `| undefined` added (`typeFromParameterWorker`), which a
    ///   Direct node cannot express, so it answers none here.
    pub(crate) fn pseudo_direct_declaration_node(
        &self,
        declaration: NodeId,
    ) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::PropertySignatureDeclaration(node) => node.r#type,
            Node::PropertyDeclaration(node) => {
                if let Some(annotation) = node.r#type {
                    return Some(annotation);
                }
                let initializer = node.initializer?;
                if self.pseudo_is_contextually_typed(declaration) {
                    return None;
                }
                self.pseudo_direct_type_of_expression(initializer)
            }
            Node::PropertyAssignment(node) => {
                node.r#type.or_else(|| self.pseudo_direct_type_of_expression(node.initializer?))
            }
            Node::VariableDeclaration(node) => {
                if let Some(annotation) = node.r#type {
                    return Some(annotation);
                }
                let initializer = node.initializer?;
                let symbol = self.binder.symbol_of(declaration)?;
                let declarations = &self.binder.symbols().get(symbol).declarations;
                let single = declarations.len() == 1
                    || declarations
                        .iter()
                        .filter(|&&d| self.nodes.kind(d) == SyntaxKind::VariableDeclaration)
                        .count()
                        == 1;
                if !single || self.pseudo_is_contextually_typed(declaration) {
                    return None;
                }
                self.pseudo_direct_type_of_expression(initializer)
            }
            Node::ParameterDeclaration(node) if node.initializer.is_none() => {
                // A setter's parameter is typed by the accessor pair
                // (`GetTypeOfAccessor`), which is not ported here.
                if self
                    .nodes
                    .parent(declaration)
                    .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::SetAccessor)
                {
                    return None;
                }
                node.r#type
            }
            _ => None,
        }
    }

    /// The written return annotation (`FunctionLikeData().Type`) of a
    /// function-like declaration.
    pub(crate) fn function_like_return_annotation(
        &self,
        declaration: NodeId,
    ) -> Option<TypeNode<'a>> {
        match self.node_map.get(declaration)? {
            Node::FunctionDeclaration(node) => node.r#type,
            Node::FunctionExpression(node) => node.r#type,
            Node::ArrowFunction(node) => node.r#type,
            Node::MethodDeclaration(node) => node.r#type,
            Node::MethodSignatureDeclaration(node) => node.r#type,
            Node::CallSignatureDeclaration(node) => node.r#type,
            Node::ConstructSignatureDeclaration(node) => node.r#type,
            Node::FunctionTypeNode(node) => node.r#type,
            Node::ConstructorTypeNode(node) => node.r#type,
            Node::GetAccessorDeclaration(node) => node.r#type,
            Node::SetAccessorDeclaration(node) => node.r#type,
            Node::IndexSignatureDeclaration(node) => node.r#type,
            Node::ConstructorDeclaration(node) => node.r#type,
            _ => None,
        }
    }

    /// `ast.ForEachReturnStatement` with `typeFromSingleReturnExpression`'s
    /// callback (`lookup.go:221`): the expression of the block's ONLY return
    /// statement, provided it is a direct child of the block. A nested return
    /// (inside `if`, a loop, …) or a second return yields none, as does a
    /// bare `return;`. Only statement containers are entered, so a function
    /// or class never contributes its own returns.
    fn single_direct_return_expression(
        &self,
        block: &'a tsr_ast::Block<'a>,
    ) -> Option<Expression<'a>> {
        let block_id = block.node_id?;
        let mut candidate = None;
        let mut stack: Vec<Node<'a>> =
            block.statements.iter().rev().map(|&s| Node::from(s)).collect();
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            match node {
                Node::ReturnStatement(statement) => {
                    let id = statement.node_id?;
                    if self.nodes.parent(id) != Some(block_id) || candidate.is_some() {
                        return None;
                    }
                    candidate = Some(statement.expression);
                    continue;
                }
                // `ForEachReturnStatement`'s traversed kinds (`ast/utilities.go:1163`).
                Node::CaseBlock(_)
                | Node::Block(_)
                | Node::IfStatement(_)
                | Node::DoStatement(_)
                | Node::WhileStatement(_)
                | Node::ForStatement(_)
                | Node::ForInOrOfStatement(_)
                | Node::WithStatement(_)
                | Node::SwitchStatement(_)
                | Node::CaseOrDefaultClause(_)
                | Node::LabeledStatement(_)
                | Node::TryStatement(_)
                | Node::CatchClause(_) => {}
                _ => continue,
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().rev().copied());
        }
        candidate.flatten()
    }

    /// `typeFromExpression` (`lookup.go:254`) restricted to its Direct
    /// answers: a non-`const` type assertion (`typeFromTypeAssertion`,
    /// `:510`), through parentheses. A `const` assertion answers whatever its
    /// operand answers.
    pub(crate) fn pseudo_direct_type_of_expression(
        &self,
        expression: Expression<'a>,
    ) -> Option<TypeNode<'a>> {
        match expression {
            Expression::ParenthesizedExpression(node) => {
                self.pseudo_direct_type_of_expression(node.expression?)
            }
            Expression::AsExpression(node) => {
                self.pseudo_direct_type_of_assertion(node.expression?, node.r#type?)
            }
            Expression::TypeAssertion(node) => {
                self.pseudo_direct_type_of_assertion(node.expression?, node.r#type?)
            }
            _ => None,
        }
    }

    fn pseudo_direct_type_of_assertion(
        &self,
        expression: Expression<'a>,
        annotation: TypeNode<'a>,
    ) -> Option<TypeNode<'a>> {
        if crate::assertions::is_const_type_reference(annotation) {
            return self.pseudo_direct_type_of_expression(expression);
        }
        Some(annotation)
    }

    /// The pseudochecker's `isContextuallyTyped` (`lookup.go:713`): some
    /// ancestor (not stopping at function boundaries) is a call, a
    /// `satisfies`, a JSX element or expression, or an annotated variable,
    /// parameter, property or non-`const` assertion.
    pub(crate) fn pseudo_is_contextually_typed(&self, node: NodeId) -> bool {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let contextual = match self.node_map.get(id) {
                Some(
                    Node::CallExpression(_)
                    | Node::SatisfiesExpression(_)
                    | Node::JsxElement(_)
                    | Node::JsxExpression(_),
                ) => true,
                Some(Node::VariableDeclaration(node)) => node.r#type.is_some(),
                Some(Node::ParameterDeclaration(node)) => node.r#type.is_some(),
                Some(Node::PropertySignatureDeclaration(node)) => node.r#type.is_some(),
                Some(Node::PropertyDeclaration(node)) => node.r#type.is_some(),
                Some(Node::AsExpression(node)) => {
                    node.r#type.is_some_and(|t| !crate::assertions::is_const_type_reference(t))
                }
                Some(Node::TypeAssertion(node)) => {
                    node.r#type.is_some_and(|t| !crate::assertions::is_const_type_reference(t))
                }
                _ => false,
            };
            if contextual {
                return true;
            }
            current = self.nodes.parent(id);
        }
        false
    }
}

/// A pseudochecker answer (`pseudochecker.PseudoType`, `type.go:28`) as the
/// node builder's STRUCTURAL arms need it: the type a declaration or
/// expression has by syntax alone. [`Checker::pseudo_direct_return_node`] and
/// [`Checker::pseudo_direct_declaration_node`] keep answering the Direct
/// kind on its own; this tree is built only where they answer none, for the
/// structural kinds and the leaves those contain.
///
/// `Inferred` stands for both `PseudoTypeKindInferred` and
/// `PseudoTypeKindNoResult`: the node builder's equivalence of either holds
/// only through error-type charity in this port (`docs/parity/notes/r5-nodereuse2.md`
/// §2), so the two need not be told apart.
#[derive(Debug, Clone)]
pub(crate) enum PseudoType<'a> {
    Direct(TypeNode<'a>),
    Inferred,
    Undefined,
    Null,
    String,
    Number,
    BigInt,
    Boolean,
    True,
    False,
    /// A string, numeric or bigint literal (`PseudoTypeLiteral`): the
    /// expression whose regular literal type it denotes, and which the node
    /// builder re-emits as written.
    Literal(Expression<'a>),
    /// `PseudoTypeMaybeConstLocation`: `constant` in a const context, else
    /// `regular`.
    MaybeConst {
        node: NodeId,
        constant: Box<PseudoType<'a>>,
        regular: Box<PseudoType<'a>>,
    },
    Union(Vec<PseudoType<'a>>),
    SingleCallSignature {
        type_parameters: &'a [&'a tsr_ast::TypeParameterDeclaration<'a>],
        parameters: Vec<PseudoParameter<'a>>,
        return_type: Box<PseudoType<'a>>,
    },
    ObjectLiteral {
        literal: NodeId,
        elements: Vec<PseudoObjectElement<'a>>,
    },
    Tuple(Vec<PseudoType<'a>>),
}

/// `pseudochecker.PseudoParameter` (`type.go:170`).
#[derive(Debug, Clone)]
pub(crate) struct PseudoParameter<'a> {
    pub(crate) declaration: &'a tsr_ast::ParameterDeclaration<'a>,
    pub(crate) rest: bool,
    pub(crate) optional: bool,
    pub(crate) r#type: PseudoType<'a>,
}

/// `pseudochecker.PseudoObjectElement` (`type.go:218`): the element's
/// declaration (`e.Name.Parent`), its written name, and its kind.
#[derive(Debug, Clone)]
pub(crate) struct PseudoObjectElement<'a> {
    pub(crate) declaration: NodeId,
    pub(crate) name: tsr_ast::PropertyName<'a>,
    pub(crate) optional: bool,
    pub(crate) kind: PseudoObjectElementKind<'a>,
}

#[derive(Debug, Clone)]
pub(crate) enum PseudoObjectElementKind<'a> {
    Method {
        type_parameters: &'a [&'a tsr_ast::TypeParameterDeclaration<'a>],
        parameters: Vec<PseudoParameter<'a>>,
        return_type: PseudoType<'a>,
    },
    PropertyAssignment {
        readonly: bool,
        r#type: PseudoType<'a>,
    },
    GetAccessor(PseudoType<'a>),
    SetAccessor(PseudoParameter<'a>),
}

impl PseudoType<'_> {
    /// `isStructuralPseudoType` (`pseudotypenodebuilder.go:632`).
    pub(crate) fn is_structural(&self) -> bool {
        match self {
            Self::ObjectLiteral { .. } | Self::Tuple(_) | Self::SingleCallSignature { .. } => true,
            Self::MaybeConst { constant, regular, .. } => {
                constant.is_structural() || regular.is_structural()
            }
            _ => false,
        }
    }

    /// `CouldAlreadyReferToUndefinedType` (`lookup.go:578`).
    fn could_already_refer_to_undefined(&self) -> bool {
        match self {
            Self::Inferred | Self::Undefined => true,
            Self::MaybeConst { constant, regular, .. } => {
                matches!(**constant, Self::Undefined) || regular.could_already_refer_to_undefined()
            }
            Self::Direct(node) => type_node_could_refer_to_undefined(*node),
            Self::Union(types) => types.iter().any(Self::could_already_refer_to_undefined),
            _ => false,
        }
    }
}

/// `typeNodeCouldReferToUndefined` (`lookup.go:550`).
fn type_node_could_refer_to_undefined(node: TypeNode<'_>) -> bool {
    match node {
        TypeNode::ParenthesizedTypeNode(parenthesized) => {
            parenthesized.r#type.is_some_and(type_node_could_refer_to_undefined)
        }
        TypeNode::TypeReferenceNode(_)
        | TypeNode::IndexedAccessTypeNode(_)
        | TypeNode::TypeQueryNode(_)
        | TypeNode::OptionalTypeNode(_)
        | TypeNode::RestTypeNode(_)
        | TypeNode::ImportTypeNode(_)
        | TypeNode::ConditionalTypeNode(_)
        | TypeNode::TypeOperatorNode(_)
        | TypeNode::TypePredicateNode(_) => true,
        TypeNode::IntersectionTypeNode(intersection) => {
            intersection.types.iter().copied().any(type_node_could_refer_to_undefined)
        }
        TypeNode::UnionTypeNode(union) => {
            union.types.iter().copied().any(type_node_could_refer_to_undefined)
        }
        TypeNode::KeywordTypeNode(keyword) => keyword.kind == SyntaxKind::UndefinedKeyword,
        _ => false,
    }
}

/// `addUndefinedIfDefinitelyRequired` (`lookup.go:619`).
fn add_undefined_if_definitely_required(expression: PseudoType<'_>) -> PseudoType<'_> {
    if expression.could_already_refer_to_undefined() {
        return expression;
    }
    PseudoType::Union(vec![expression, PseudoType::Undefined])
}

/// `isOptionalInitializedOrRestParameter` (`lookup.go:597`).
fn is_optional_initialized_or_rest_parameter(
    parameter: &tsr_ast::ParameterDeclaration<'_>,
) -> bool {
    parameter.dot_dot_dot_token.is_some()
        || parameter.initializer.is_some()
        || parameter.question_token.is_some()
}

/// `lastRequiredParamIndex` (`lookup.go:610`).
fn last_required_parameter_index(parameters: &[&tsr_ast::ParameterDeclaration<'_>]) -> usize {
    parameters
        .iter()
        .rposition(|parameter| !is_optional_initialized_or_rest_parameter(parameter))
        .map_or(0, |index| index + 1)
}

impl<'a> Checker<'a, '_> {
    /// `createReturnFromSignature` (`lookup.go:201`), every kind.
    pub(crate) fn pseudo_return_type(&self, declaration: NodeId) -> PseudoType<'a> {
        if let Some(annotation) = self.function_like_return_annotation(declaration) {
            return PseudoType::Direct(annotation);
        }
        // `isValueSignatureDeclaration` (`lookup.go:196`), with the body.
        let (modifiers, asterisk, body): (&[ModifierLike<'a>], bool, _) =
            match self.node_map.get(declaration) {
                Some(Node::FunctionDeclaration(node)) => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::FunctionExpression(node)) => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::MethodDeclaration(node)) => (
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::GetAccessorDeclaration(node)) => (
                    node.modifiers,
                    false,
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::SetAccessorDeclaration(node)) => (
                    node.modifiers,
                    false,
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::ConstructorDeclaration(node)) => (
                    node.modifiers,
                    false,
                    node.body.map(|FunctionBody::Block(block)| ConciseBody::Block(block)),
                ),
                Some(Node::ArrowFunction(node)) => (node.modifiers, false, node.body),
                _ => return PseudoType::Inferred,
            };
        let Some(body) = body else { return PseudoType::Inferred };
        let is_async = modifiers.iter().any(|modifier| {
            matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
        });
        if is_async && asterisk {
            return PseudoType::Inferred;
        }
        let candidate = match body {
            ConciseBody::Block(block) => self.single_direct_return_expression(block),
            expression => Expression::try_from(Node::from(expression)).ok(),
        };
        let Some(candidate) = candidate else { return PseudoType::Inferred };
        let Some(candidate_id) = Node::from(candidate).node_id() else {
            return PseudoType::Inferred;
        };
        if self.pseudo_is_contextually_typed(candidate_id) {
            let annotation = match candidate {
                Expression::AsExpression(node) => node.r#type,
                Expression::TypeAssertion(node) => node.r#type,
                _ => None,
            };
            return match annotation {
                Some(annotation) if !crate::assertions::is_const_type_reference(annotation) => {
                    PseudoType::Direct(annotation)
                }
                _ => PseudoType::Inferred,
            };
        }
        self.pseudo_type_of_expression(candidate)
    }

    /// `GetTypeOfDeclaration` (`lookup.go:34`) for the declarations a printed
    /// property symbol can have, every kind.
    pub(crate) fn pseudo_type_of_declaration(&self, declaration: NodeId) -> PseudoType<'a> {
        let keep = |expression: PseudoType<'a>| match expression {
            // "fallback to NoResult if PseudoTypeKindInferred without error
            // nodes"; both are `Inferred` here.
            PseudoType::Inferred => PseudoType::Inferred,
            other => other,
        };
        match self.node_map.get(declaration) {
            Some(Node::ParameterDeclaration(node)) => {
                let Some(parent) = self.nodes.parent(declaration) else {
                    return PseudoType::Inferred;
                };
                let parameters = self.signature_parts_parameters(parent);
                let index = parameters
                    .iter()
                    .position(|parameter| parameter.node_id == Some(declaration))
                    .unwrap_or(0);
                self.pseudo_type_of_parameter(
                    node,
                    index,
                    last_required_parameter_index(parameters),
                )
            }
            Some(Node::PropertySignatureDeclaration(node)) => {
                node.r#type.map_or(PseudoType::Inferred, PseudoType::Direct)
            }
            // `typeFromProperty` (`lookup.go:98`).
            Some(Node::PropertyDeclaration(node)) => {
                if let Some(annotation) = node.r#type {
                    return PseudoType::Direct(annotation);
                }
                let Some(initializer) = node.initializer else { return PseudoType::Inferred };
                if self.pseudo_is_contextually_typed(declaration) {
                    return PseudoType::Inferred;
                }
                let readonly = node.modifiers.iter().any(|modifier| {
                    matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::ReadonlyKeyword)
                });
                if readonly && matches!(initializer, Expression::TemplateExpression(_)) {
                    return PseudoType::Inferred;
                }
                let expression = keep(self.pseudo_type_of_expression(initializer));
                let optional =
                    node.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken);
                if !matches!(expression, PseudoType::Inferred | PseudoType::Direct(_)) && optional {
                    return add_undefined_if_definitely_required(expression);
                }
                expression
            }
            // `typeFromPropertyAssignment` (`lookup.go:69`).
            Some(Node::PropertyAssignment(node)) => {
                if let Some(annotation) = node.r#type {
                    return PseudoType::Direct(annotation);
                }
                node.initializer
                    .map_or(PseudoType::Inferred, |init| keep(self.pseudo_type_of_expression(init)))
            }
            // `typeFromVariable` (`lookup.go:124`).
            Some(Node::VariableDeclaration(node)) => {
                if let Some(annotation) = node.r#type {
                    return PseudoType::Direct(annotation);
                }
                let Some(initializer) = node.initializer else { return PseudoType::Inferred };
                let Some(symbol) = self.binder.symbol_of(declaration) else {
                    return PseudoType::Inferred;
                };
                let declarations = &self.binder.symbols().get(symbol).declarations;
                let single = declarations.len() == 1
                    || declarations
                        .iter()
                        .filter(|&&d| self.nodes.kind(d) == SyntaxKind::VariableDeclaration)
                        .count()
                        == 1;
                if !single || self.pseudo_is_contextually_typed(declaration) {
                    return PseudoType::Inferred;
                }
                if matches!(initializer, Expression::TemplateExpression(_))
                    && self.nodes.parent(declaration).is_some_and(|list| {
                        self.nodes.flags(list).contains(tsr_ast::NodeFlags::CONST)
                    })
                {
                    return PseudoType::Inferred;
                }
                keep(self.pseudo_type_of_expression(initializer))
            }
            _ => PseudoType::Inferred,
        }
    }

    /// A function-like declaration's parameter list.
    fn signature_parts_parameters(
        &self,
        declaration: NodeId,
    ) -> &'a [&'a tsr_ast::ParameterDeclaration<'a>] {
        match self.node_map.get(declaration) {
            Some(Node::FunctionDeclaration(node)) => node.parameters,
            Some(Node::FunctionExpression(node)) => node.parameters,
            Some(Node::ArrowFunction(node)) => node.parameters,
            Some(Node::MethodDeclaration(node)) => node.parameters,
            Some(Node::MethodSignatureDeclaration(node)) => node.parameters,
            Some(Node::ConstructorDeclaration(node)) => node.parameters,
            Some(Node::GetAccessorDeclaration(node)) => node.parameters,
            Some(Node::SetAccessorDeclaration(node)) => node.parameters,
            Some(Node::CallSignatureDeclaration(node)) => node.parameters,
            Some(Node::ConstructSignatureDeclaration(node)) => node.parameters,
            Some(Node::FunctionTypeNode(node)) => node.parameters,
            Some(Node::ConstructorTypeNode(node)) => node.parameters,
            _ => &[],
        }
    }

    /// `typeFromParameterWorker` (`lookup.go:649`).
    fn pseudo_type_of_parameter(
        &self,
        parameter: &'a tsr_ast::ParameterDeclaration<'a>,
        index: usize,
        last_required: usize,
    ) -> PseudoType<'a> {
        let Some(id) = parameter.node_id else { return PseudoType::Inferred };
        if let Some(parent) = self.nodes.parent(id)
            && self.nodes.kind(parent) == SyntaxKind::SetAccessor
        {
            return self.pseudo_type_of_accessor(parent);
        }
        let has_required_after = index + 1 < last_required;
        if let Some(annotation) = parameter.r#type {
            let result = PseudoType::Direct(annotation);
            if self.strict_null_checks && parameter.initializer.is_some() && has_required_after {
                return add_undefined_if_definitely_required(result);
            }
            return result;
        }
        if let Some(initializer) = parameter.initializer
            && matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(_)))
            && !self.pseudo_is_contextually_typed(id)
        {
            let expression = self.pseudo_type_of_expression(initializer);
            if !self.strict_null_checks || !has_required_after {
                return expression;
            }
            return add_undefined_if_definitely_required(expression);
        }
        PseudoType::Inferred
    }

    /// `cloneParameters` (`lookup.go:687`).
    fn pseudo_parameters(
        &self,
        parameters: &'a [&'a tsr_ast::ParameterDeclaration<'a>],
    ) -> Vec<PseudoParameter<'a>> {
        let last_required = last_required_parameter_index(parameters);
        parameters
            .iter()
            .enumerate()
            .map(|(index, &parameter)| {
                let mut optional = parameter.question_token.is_some();
                if !optional && parameter.initializer.is_some() {
                    optional = index + 1 >= last_required;
                }
                PseudoParameter {
                    declaration: parameter,
                    rest: parameter.dot_dot_dot_token.is_some(),
                    optional,
                    r#type: self.pseudo_type_of_parameter(parameter, index, last_required),
                }
            })
            .collect()
    }

    /// `GetTypeOfAccessor` → `typeFromAccessor` (`lookup.go:146`).
    pub(crate) fn pseudo_type_of_accessor(&self, accessor: NodeId) -> PseudoType<'a> {
        let (first, second, getter) = self.all_accessor_declarations(accessor);
        let mut annotation = self.accessor_type_annotation(accessor);
        if annotation.is_none() && first != Some(accessor) {
            annotation = first.and_then(|first| self.accessor_type_annotation(first));
        }
        if annotation.is_none()
            && let Some(second) = second
            && second != accessor
        {
            annotation = self.accessor_type_annotation(second);
        }
        if let Some(annotation) = annotation
            && !matches!(annotation, TypeNode::TypePredicateNode(_))
        {
            return PseudoType::Direct(annotation);
        }
        match getter {
            Some(getter) => self.pseudo_return_type(getter),
            None => PseudoType::Inferred,
        }
    }

    /// `ast.GetAllAccessorDeclarationsForDeclaration`: the first and second
    /// accessor of the pair the accessor's symbol declares, in source order,
    /// and its getter.
    fn all_accessor_declarations(
        &self,
        accessor: NodeId,
    ) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
        let accessors: Vec<NodeId> = self.binder.symbol_of(accessor).map_or_else(
            || vec![accessor],
            |symbol| {
                self.binder
                    .symbols()
                    .get(symbol)
                    .declarations
                    .iter()
                    .copied()
                    .filter(|&declaration| {
                        matches!(
                            self.nodes.kind(declaration),
                            SyntaxKind::GetAccessor | SyntaxKind::SetAccessor
                        )
                    })
                    .collect()
            },
        );
        let getter = accessors
            .iter()
            .copied()
            .find(|&declaration| self.nodes.kind(declaration) == SyntaxKind::GetAccessor);
        (accessors.first().copied(), accessors.get(1).copied(), getter)
    }

    /// `getTypeAnnotationFromAccessor` (`lookup.go:177`).
    fn accessor_type_annotation(&self, accessor: NodeId) -> Option<TypeNode<'a>> {
        match self.node_map.get(accessor)? {
            Node::GetAccessorDeclaration(node) => node.r#type,
            Node::SetAccessorDeclaration(node) => node.parameters.first()?.r#type,
            _ => None,
        }
    }

    /// `typeFromExpression` (`lookup.go:262`), every kind.
    pub(crate) fn pseudo_type_of_expression(&self, expression: Expression<'a>) -> PseudoType<'a> {
        let maybe_const =
            |constant: PseudoType<'a>, regular: PseudoType<'a>| match Node::from(expression)
                .node_id()
            {
                Some(node) => PseudoType::MaybeConst {
                    node,
                    constant: Box::new(constant),
                    regular: Box::new(regular),
                },
                None => PseudoType::Inferred,
            };
        match expression {
            Expression::OmittedExpression(_) => PseudoType::Undefined,
            Expression::ParenthesizedExpression(node) => {
                node.expression.map_or(PseudoType::Inferred, |e| self.pseudo_type_of_expression(e))
            }
            Expression::Identifier(identifier) if identifier.text == "undefined" => {
                PseudoType::Undefined
            }
            Expression::KeywordExpression(keyword) => match keyword.kind {
                SyntaxKind::NullKeyword => PseudoType::Null,
                SyntaxKind::TrueKeyword => maybe_const(PseudoType::True, PseudoType::Boolean),
                SyntaxKind::FalseKeyword => maybe_const(PseudoType::False, PseudoType::Boolean),
                _ => PseudoType::Inferred,
            },
            Expression::ArrowFunction(_) | Expression::FunctionExpression(_) => {
                self.pseudo_type_of_function_like_expression(expression)
            }
            Expression::AsExpression(node) => match (node.expression, node.r#type) {
                (Some(operand), Some(annotation)) => {
                    self.pseudo_type_of_assertion(operand, annotation)
                }
                _ => PseudoType::Inferred,
            },
            Expression::TypeAssertion(node) => match (node.expression, node.r#type) {
                (Some(operand), Some(annotation)) => {
                    self.pseudo_type_of_assertion(operand, annotation)
                }
                _ => PseudoType::Inferred,
            },
            // `typeFromPrimitiveLiteralPrefix` (`lookup.go:494`), behind
            // `IsPrimitiveLiteralValue(node, true)`.
            Expression::PrefixUnaryExpression(prefix)
                if prefix.operator.kind == SyntaxKind::MinusToken
                    || prefix.operator.kind == SyntaxKind::PlusToken =>
            {
                match prefix.operand {
                    Some(Expression::NumericLiteral(_)) => {
                        let literal = if prefix.operator.kind == SyntaxKind::PlusToken {
                            prefix.operand.unwrap_or(expression)
                        } else {
                            expression
                        };
                        maybe_const(PseudoType::Literal(literal), PseudoType::Number)
                    }
                    Some(Expression::BigIntLiteral(_))
                        if prefix.operator.kind == SyntaxKind::MinusToken =>
                    {
                        maybe_const(PseudoType::Literal(expression), PseudoType::BigInt)
                    }
                    _ => PseudoType::Inferred,
                }
            }
            Expression::ArrayLiteralExpression(array) => self.pseudo_type_of_array_literal(array),
            Expression::ObjectLiteralExpression(object) => {
                self.pseudo_type_of_object_literal(object)
            }
            Expression::TemplateExpression(_) => {
                let in_const = Node::from(expression)
                    .node_id()
                    .is_some_and(|node| self.pseudo_is_in_const_context(node));
                if in_const {
                    PseudoType::Inferred
                } else {
                    maybe_const(PseudoType::Inferred, PseudoType::String)
                }
            }
            Expression::NumericLiteral(_) => {
                maybe_const(PseudoType::Literal(expression), PseudoType::Number)
            }
            Expression::NoSubstitutionTemplateLiteral(_) | Expression::StringLiteral(_) => {
                maybe_const(PseudoType::Literal(expression), PseudoType::String)
            }
            Expression::BigIntLiteral(_) => {
                maybe_const(PseudoType::Literal(expression), PseudoType::BigInt)
            }
            _ => PseudoType::Inferred,
        }
    }

    /// `typeFromTypeAssertion` (`lookup.go:510`).
    fn pseudo_type_of_assertion(
        &self,
        expression: Expression<'a>,
        annotation: TypeNode<'a>,
    ) -> PseudoType<'a> {
        if crate::assertions::is_const_type_reference(annotation) {
            return self.pseudo_type_of_expression(expression);
        }
        PseudoType::Direct(annotation)
    }

    /// `typeFromFunctionLikeExpression` (`lookup.go:517`).
    fn pseudo_type_of_function_like_expression(
        &self,
        expression: Expression<'a>,
    ) -> PseudoType<'a> {
        let (full_signature, type_parameters, parameters) = match expression {
            Expression::ArrowFunction(node) => {
                (node.full_signature, node.type_parameters, node.parameters)
            }
            Expression::FunctionExpression(node) => {
                (node.full_signature, node.type_parameters, node.parameters)
            }
            _ => return PseudoType::Inferred,
        };
        if let Some(full_signature) = full_signature {
            return PseudoType::Direct(full_signature);
        }
        let Some(declaration) = Node::from(expression).node_id() else {
            return PseudoType::Inferred;
        };
        PseudoType::SingleCallSignature {
            type_parameters,
            parameters: self.pseudo_parameters(parameters),
            return_type: Box::new(self.pseudo_return_type(declaration)),
        }
    }

    /// `typeFromArrayLiteral` (`lookup.go:438`).
    fn pseudo_type_of_array_literal(
        &self,
        array: &'a tsr_ast::ArrayLiteralExpression<'a>,
    ) -> PseudoType<'a> {
        let Some(id) = array.node_id else { return PseudoType::Inferred };
        // `canGetTypeFromArrayLiteral`.
        if !self.pseudo_is_in_const_context(id)
            || array.elements.iter().any(|element| matches!(element, Expression::SpreadElement(_)))
        {
            return PseudoType::Inferred;
        }
        if self.pseudo_is_contextually_typed(id) {
            return PseudoType::Inferred;
        }
        PseudoType::Tuple(
            array.elements.iter().map(|&element| self.pseudo_type_of_expression(element)).collect(),
        )
    }

    /// `typeFromObjectLiteral` (`lookup.go:315`).
    fn pseudo_type_of_object_literal(
        &self,
        object: &'a tsr_ast::ObjectLiteralExpression<'a>,
    ) -> PseudoType<'a> {
        let Some(literal) = object.node_id else { return PseudoType::Inferred };
        if !self.can_get_pseudo_type_of_object_literal(object) {
            return PseudoType::Inferred;
        }
        let mut elements = Vec::with_capacity(object.properties.len());
        for &element in object.properties {
            let Some(declaration) = element.node_id() else { return PseudoType::Inferred };
            match element {
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(method) => {
                    let optional = method
                        .postfix_token
                        .is_some_and(|token| token.kind == SyntaxKind::QuestionToken);
                    let kind = if let Some(full_signature) = method.full_signature {
                        PseudoObjectElementKind::PropertyAssignment {
                            readonly: false,
                            r#type: PseudoType::Direct(full_signature),
                        }
                    } else {
                        PseudoObjectElementKind::Method {
                            type_parameters: method.type_parameters,
                            parameters: self.pseudo_parameters(method.parameters),
                            return_type: self.pseudo_return_type(declaration),
                        }
                    };
                    elements.push(PseudoObjectElement {
                        declaration,
                        name: method.name,
                        optional,
                        kind,
                    });
                }
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    elements.push(PseudoObjectElement {
                        declaration,
                        name: assignment.name,
                        optional: assignment
                            .postfix_token
                            .is_some_and(|token| token.kind == SyntaxKind::QuestionToken),
                        kind: PseudoObjectElementKind::PropertyAssignment {
                            readonly: false,
                            r#type: assignment.initializer.map_or(PseudoType::Inferred, |init| {
                                self.pseudo_type_of_expression(init)
                            }),
                        },
                    });
                }
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    if let Some(member) = self.pseudo_accessor_member(declaration, accessor.name) {
                        elements.push(member);
                    }
                }
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    if let Some(member) = self.pseudo_accessor_member(declaration, accessor.name) {
                        elements.push(member);
                    }
                }
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_)
                | tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => {
                    return PseudoType::Inferred;
                }
            }
        }
        PseudoType::ObjectLiteral { literal, elements }
    }

    /// `canGetTypeFromObjectLiteral` (`lookup.go:406`): no error node.
    fn can_get_pseudo_type_of_object_literal(
        &self,
        object: &tsr_ast::ObjectLiteralExpression<'a>,
    ) -> bool {
        let has_error =
            |id: NodeId| self.nodes.flags(id).contains(tsr_ast::NodeFlags::THIS_NODE_HAS_ERROR);
        object.properties.iter().all(|element| {
            let name = match element {
                tsr_ast::ObjectLiteralElementLike::ShorthandPropertyAssignment(_)
                | tsr_ast::ObjectLiteralElementLike::SpreadAssignment(_) => return false,
                tsr_ast::ObjectLiteralElementLike::MethodDeclaration(node) => node.name,
                tsr_ast::ObjectLiteralElementLike::PropertyAssignment(node) => node.name,
                tsr_ast::ObjectLiteralElementLike::GetAccessorDeclaration(node) => node.name,
                tsr_ast::ObjectLiteralElementLike::SetAccessorDeclaration(node) => node.name,
            };
            if element.node_id().is_some_and(has_error)
                || Node::from(name).node_id().is_some_and(has_error)
            {
                return false;
            }
            match name {
                tsr_ast::PropertyName::PrivateIdentifier(_) => false,
                tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                    computed.expression.is_some_and(is_primitive_literal_value)
                }
                _ => true,
            }
        })
    }

    /// `getAccessorMember` (`lookup.go:363`).
    fn pseudo_accessor_member(
        &self,
        accessor: NodeId,
        name: tsr_ast::PropertyName<'a>,
    ) -> Option<PseudoObjectElement<'a>> {
        let (first, second, _) = self.all_accessor_declarations(accessor);
        let [getter, setter] = [SyntaxKind::GetAccessor, SyntaxKind::SetAccessor].map(|kind| {
            [first, second].into_iter().flatten().find(|&d| self.nodes.kind(d) == kind)
        });
        let annotated = |declaration: Option<NodeId>| {
            declaration
                .is_some_and(|declaration| self.accessor_type_annotation(declaration).is_some())
        };
        if annotated(getter) && annotated(setter) {
            let kind = match self.node_map.get(accessor)? {
                Node::GetAccessorDeclaration(_) => {
                    PseudoObjectElementKind::GetAccessor(self.pseudo_type_of_accessor(accessor))
                }
                Node::SetAccessorDeclaration(node) => PseudoObjectElementKind::SetAccessor(
                    self.pseudo_parameters(node.parameters).into_iter().next()?,
                ),
                _ => return None,
            };
            return Some(PseudoObjectElement {
                declaration: accessor,
                name,
                optional: false,
                kind,
            });
        }
        if first == Some(accessor) {
            let readonly = self.nodes.kind(accessor) == SyntaxKind::GetAccessor && second.is_none();
            return Some(PseudoObjectElement {
                declaration: accessor,
                name,
                optional: false,
                kind: PseudoObjectElementKind::PropertyAssignment {
                    readonly,
                    r#type: self.pseudo_type_of_accessor(accessor),
                },
            });
        }
        None
    }

    /// The pseudochecker's `IsInConstContext` (`lookup.go:482`): the nearest
    /// ancestor that is an assertion or not an array/object-literal
    /// propagating kind is a const assertion.
    pub(crate) fn pseudo_is_in_const_context(&self, node: NodeId) -> bool {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            match self.node_map.get(id) {
                Some(Node::AsExpression(assertion)) => {
                    return assertion
                        .r#type
                        .is_some_and(crate::assertions::is_const_type_reference);
                }
                Some(Node::TypeAssertion(assertion)) => {
                    return assertion
                        .r#type
                        .is_some_and(crate::assertions::is_const_type_reference);
                }
                // `isConstContextPropagatingKind` (`lookup.go:470`).
                Some(
                    Node::ArrayLiteralExpression(_)
                    | Node::ObjectLiteralExpression(_)
                    | Node::ParenthesizedExpression(_)
                    | Node::PropertyAssignment(_)
                    | Node::ShorthandPropertyAssignment(_)
                    | Node::SpreadElement(_)
                    | Node::TemplateSpan(_)
                    | Node::PrefixUnaryExpression(_),
                ) => {}
                _ => return false,
            }
            current = self.nodes.parent(id);
        }
        false
    }
}

/// `ast.IsPrimitiveLiteralValue(node, false)`.
fn is_primitive_literal_value(expression: Expression<'_>) -> bool {
    match expression {
        Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::NoSubstitutionTemplateLiteral(_) => true,
        Expression::KeywordExpression(keyword) => {
            matches!(keyword.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
        }
        Expression::PrefixUnaryExpression(prefix) => {
            matches!(prefix.operator.kind, SyntaxKind::MinusToken | SyntaxKind::PlusToken)
                && matches!(prefix.operand, Some(Expression::NumericLiteral(_)))
        }
        _ => false,
    }
}
