//! Call signatures: what a function-like declaration declares, and how one is
//! printed.
//!
//! Ported from `Checker.getSignaturesOfSymbol` (`checker.go:19806`),
//! `Checker.getSignatureFromDeclaration` (`checker.go:19836`) and
//! `Checker.getReturnTypeOfSignature` (`checker.go:20001`), together with the
//! node builder's rendering of one —
//! `NodeBuilderImpl.signatureToSignatureDeclarationHelper`
//! (`nodebuilderimpl.go:1792`) and
//! `NodeBuilderImpl.symbolToParameterDeclaration` (`nodebuilderimpl.go:1654`).
//!
//! # Why the printed form is built here and not in `printing.rs`
//!
//! Upstream never renders a type from the type alone: `typeToString` builds a
//! type *node* and prints that, and for a function type the node is rebuilt from
//! the signature's parameter symbols and return type. This port keeps a named
//! type's printed form as a string computed once at creation
//! ([`crate::types::TypeData::Named`]), so the rendering has to happen where the
//! signature is — here — rather than in a printer that only sees the finished
//! string. That is a consequence of the divergence already recorded in
//! `docs/architecture/checker.md`, not a new one.
//!
//! # What is a gap, and why each one is a gap rather than a guess
//!
//! [`Signature`] is only built when every part of it can be answered exactly.
//! A `.types` baseline compares whole lines, so a signature that is right in
//! three places and plausible in the fourth fails identically to one that is
//! wrong everywhere — and, worse, is indistinguishable from an answer in the
//! failure histogram. The list is in [`Checker::get_signature_from_declaration`].

use tsr_ast::{
    ModifierLike, Node, NodeId, ParameterDeclaration, SyntaxKind, TypeNode,
    TypeParameterDeclaration,
};
use tsr_binder::SymbolId;

use crate::{checker::Checker, types::TypeId};

/// One parameter of a [`Signature`], reduced to what a printed line needs.
///
/// Upstream carries the parameter *symbol* and asks `getTypeOfSymbol` for its
/// type at print time (`nodebuilderimpl.go:1654`); the type is resolved eagerly
/// here for the reason given in the module docs.
#[derive(Debug, Clone)]
pub struct Parameter {
    /// The parameter's name, as written.
    pub name: String,
    /// Whether the node builder emits a `?` after the name.
    pub optional: bool,
    /// Whether the node builder emits a leading `...`.
    pub rest: bool,
    /// `getTypeOfSymbol` of the parameter symbol.
    pub r#type: TypeId,
}

/// One type parameter of a [`Signature`].
///
/// Ported from `NodeBuilderImpl.typeParameterToDeclarationWithConstraint`
/// (`nodebuilderimpl.go`), reduced to the three parts it prints: the name, an
/// `extends` constraint, and a default.
#[derive(Debug, Clone)]
pub struct TypeParameter {
    /// The parameter's name, as written.
    pub name: String,
    /// The `extends` clause, if there is one.
    pub constraint: Option<TypeId>,
    /// The `= T` default, if there is one.
    pub default: Option<TypeId>,
}

/// A call signature.
///
/// Ported from `Signature` (`internal/checker/types.go`), reduced to the fields
/// this slice computes. Upstream's `resolvedReturnType` is lazy and this is not;
/// see the module docs.
#[derive(Debug, Clone)]
pub struct Signature {
    /// The declaration this signature came from.
    pub declaration: NodeId,
    /// Type parameters, in source order.
    pub type_parameters: Vec<TypeParameter>,
    /// The `this` parameter, which upstream keeps out of `parameters` and the
    /// node builder puts back at the front (`nodebuilderimpl.go:1792`).
    pub this_parameter: Option<Parameter>,
    /// The value parameters, in source order.
    pub parameters: Vec<Parameter>,
    /// The return type.
    pub r#type: TypeId,
}

/// The parts of a function-like declaration a signature is built from.
///
/// Upstream reaches these through `*ast.Node` accessors (`declaration.Parameters()`,
/// `declaration.Type()`, `declaration.Body()`), which exist for every function-like
/// kind. This port's `Node` is a union of concrete types, so the accessors are one
/// match instead.
struct SignatureParts<'a> {
    modifiers: &'a [ModifierLike<'a>],
    asterisk: bool,
    type_parameters: &'a [&'a TypeParameterDeclaration<'a>],
    parameters: &'a [&'a ParameterDeclaration<'a>],
    return_annotation: Option<TypeNode<'a>>,
    body: Option<NodeId>,
    /// `mayReturnNever` (`checker.go:20312`): true for a function expression, an
    /// arrow, and a method of an object literal.
    may_return_never: bool,
}

impl<'a> Checker<'a, '_> {
    /// The call signatures a symbol has.
    ///
    /// Ported from `Checker.getSignaturesOfSymbol` (`checker.go:19806`),
    /// including its **implementation-exclusion rule**: a declaration with a body
    /// that immediately follows a same-kind sibling with the same parent is the
    /// implementation of an overload set and contributes no signature. That is
    /// what makes
    ///
    /// ```text
    /// function f(x?: number, y: string);
    /// function f() { }
    /// ```
    ///
    /// print `(x?: number, y: string) => any` and not the implementation's empty
    /// signature — a line the corpus records 20 times over in
    /// `conformance/functionOverloadErrorsSyntax.types` alone.
    ///
    /// `None` if any declaration is one this slice cannot answer exactly.
    pub(crate) fn get_signatures_of_symbol(&mut self, symbol: SymbolId) -> Option<Vec<Signature>> {
        let declarations: Vec<NodeId> =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect();
        let mut result = Vec::new();
        for (index, &declaration) in declarations.iter().enumerate() {
            if self.signature_parts_of(declaration).is_none() {
                continue;
            }
            if index > 0 && self.is_overload_implementation(declaration, declarations[index - 1]) {
                continue;
            }
            result.push(self.get_signature_from_declaration(declaration)?);
        }
        Some(result)
    }

    /// The second half of `getSignaturesOfSymbol`'s loop (`checker.go:19814`):
    /// *"a node is considered an implementation node if it has a body and the
    /// previous node is of the same kind and immediately precedes it"*.
    ///
    /// The `Reparsed` half of upstream's test is JSDoc-only and is not ported.
    ///
    /// **"Immediately precedes" is a sibling test here, not a position test.**
    /// Upstream writes `decl.Pos() == previous.End()`, where `Pos()` is the
    /// *full* start — the offset just past the previous token, trivia included —
    /// so the comparison means "with nothing but trivia between them". This
    /// port's spans start *after* leading trivia
    /// (`docs/architecture/checker-oracle.md`, "the line text is the raw source
    /// slice"), so the same equality would be false for every overload set that
    /// spans two lines, and it was: `function f(); function f() {}` printed
    /// `error` until this was corrected. Asking whether `declaration` is the very
    /// next child of the shared parent answers the same question from the data
    /// this port does have.
    fn is_overload_implementation(&self, declaration: NodeId, previous: NodeId) -> bool {
        if self.signature_parts_of(declaration).and_then(|parts| parts.body).is_none()
            || self.nodes.kind(declaration) != self.nodes.kind(previous)
        {
            return false;
        }
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        if self.nodes.parent(previous) != Some(parent) {
            return false;
        }
        let Some(parent) = self.node_map.get(parent) else { return false };
        let mut last = None;
        let mut adjacent = false;
        tsr_ast::for_each_child_id(parent, |child| {
            if child == declaration && last == Some(previous) {
                adjacent = true;
            }
            last = Some(child);
        });
        adjacent
    }

    /// The signature a function-like declaration declares.
    ///
    /// Ported from `Checker.getSignatureFromDeclaration` (`checker.go:19836`)
    /// and `Checker.getReturnTypeOfSignature` (`checker.go:20001`), which
    /// upstream keeps apart because the return type is lazy there.
    ///
    /// # The forms that answer `None`, and why none of them is guessed at
    ///
    /// - **A destructuring parameter.** `parameterToParameterDeclarationName`
    ///   invents a name for a binding pattern under rules this port has no
    ///   equivalent of, and the invented name is compared verbatim.
    /// - **A parameter whose own type is a gap**, and likewise a constraint,
    ///   default or return annotation. `(x: Unported) => void` is not
    ///   `(x: any) => void`.
    /// - **A type parameter carrying a modifier** (`const`, `in`, `out`):
    ///   `getTypeParameterModifiers` reads them off *every* declaration of the
    ///   parameter's symbol, which is a merge this port does not do.
    /// - **An inferred return type from a body containing a `return`.** That is
    ///   `checkAndAggregateReturnExpressionTypes` (`checker.go:20259`) — a union
    ///   of the return expressions, subtype-reduced. Unions are `bd tsr-4sc.9`.
    /// - **An inferred return type of an `async` or generator function**, which
    ///   is `Promise<T>` or `Generator<...>` — a reference to a global that does
    ///   not exist here (`bd tsr-9or.1`).
    /// - **An inferred return type where `mayReturnNever` holds** — a function
    ///   expression, an arrow, or a method of an object literal. Upstream
    ///   answers `never` or `void` there depending on whether the end of the body
    ///   is reachable (`functionHasImplicitReturn`), and reachability is the flow
    ///   graph, which the binder builds and nothing reads.
    pub(crate) fn get_signature_from_declaration(
        &mut self,
        declaration: NodeId,
    ) -> Option<Signature> {
        let parts = self.signature_parts_of(declaration)?;
        let (modifiers, asterisk) = (parts.modifiers, parts.asterisk);
        let type_parameter_nodes = parts.type_parameters;
        let parameter_nodes = parts.parameters;
        let return_annotation = parts.return_annotation;
        let body = parts.body;
        let may_return_never = parts.may_return_never;

        let mut type_parameters = Vec::with_capacity(type_parameter_nodes.len());
        for node in type_parameter_nodes {
            type_parameters.push(self.type_parameter_of(node)?);
        }

        // Upstream's loop, including the order of its two effects: the parameter
        // joins `parameters` *before* the optionality test, so
        // `minArgumentCount` is a count and not an index.
        let mut this_parameter = None;
        let mut parameters: Vec<Parameter> = Vec::with_capacity(parameter_nodes.len());
        let mut min_argument_count = 0;
        for (index, node) in parameter_nodes.iter().enumerate() {
            let parameter = self.parameter_of(node)?;
            if index == 0 && parameter.name == "this" {
                this_parameter = Some(parameter);
                continue;
            }
            let syntactically_optional = node.question_token.is_some()
                || node.initializer.is_some()
                || node.dot_dot_dot_token.is_some();
            parameters.push(parameter);
            if !syntactically_optional {
                min_argument_count = parameters.len();
            }
        }

        // `isOptionalParameter` (`utilities.go:303`). A `?` is optional outright;
        // an initialiser makes the parameter optional only from
        // `minArgumentCount` onward, so `function f(x = 1, y: number)` prints
        // `(x: number, y: number)` and `function f(x = 1)` prints `(x?: number)`.
        // The index upstream compares is the one in the *declaration's* list,
        // which includes any `this` parameter.
        let offset = usize::from(this_parameter.is_some());
        for (index, node) in parameter_nodes.iter().enumerate().skip(offset) {
            let Some(slot) = parameters.get_mut(index - offset) else { continue };
            if node.question_token.is_some() {
                slot.optional = true;
            } else if node.initializer.is_some() {
                slot.optional = index >= min_argument_count;
            }
        }

        let r#type = self.return_type_of(
            declaration,
            return_annotation,
            body,
            modifiers,
            asterisk,
            may_return_never,
        )?;

        Some(Signature { declaration, type_parameters, this_parameter, parameters, r#type })
    }

    /// `getReturnTypeOfSignature`'s `default` arm (`checker.go:20013`) and the
    /// part of `getReturnTypeFromBody` (`checker.go:20126`) that can be answered
    /// without unions or a flow graph.
    fn return_type_of(
        &mut self,
        declaration: NodeId,
        annotation: Option<TypeNode<'a>>,
        body: Option<NodeId>,
        modifiers: &[ModifierLike<'_>],
        asterisk: bool,
        may_return_never: bool,
    ) -> Option<TypeId> {
        // `getReturnTypeFromAnnotation` (`checker.go:20058`) wins outright.
        if let Some(annotation) = annotation {
            let id = self.get_type_from_type_node(annotation);
            return (id != self.intrinsics.error).then_some(id);
        }
        // `ast.NodeIsMissing(sig.declaration.Body())` (`checker.go:20016`): an
        // ambient declaration, an interface method, or an overload signature has
        // no body and its return type is `any`. That is a computed answer and not
        // a gap, which is why it is `anyType` here and `errorType` below.
        let Some(body) = body else { return Some(self.intrinsics.any) };
        if asterisk
            || modifiers.iter().any(|modifier| {
                matches!(modifier, ModifierLike::Token(token) if token.kind == SyntaxKind::AsyncKeyword)
            })
        {
            return None;
        }
        if may_return_never || self.body_has_return_statement(body, declaration) {
            return None;
        }
        // Zero return statements, no implicit-return question to ask: upstream's
        // `checkAndAggregateReturnExpressionTypes` yields no types and is not
        // never-returning, so `getReturnTypeFromBody` answers `voidType`
        // (`checker.go:20200`). This holds even for a body that only throws,
        // because `mayReturnNever` is false for a function declaration and a
        // class method.
        Some(self.intrinsics.void)
    }

    /// Whether `body` contains a `return` statement belonging to `owner`.
    ///
    /// Ported from `ast.ForEachReturnStatement`, whose contract is that it does
    /// **not** descend into a nested function-like node — a `return` inside an
    /// inner arrow belongs to the arrow. Iterative rather than recursive, for the
    /// reason `push_children` states: tree depth is a function of the source.
    fn body_has_return_statement(&self, body: NodeId, owner: NodeId) -> bool {
        let Some(root) = self.node_map.get(body) else { return false };
        let mut stack = vec![root];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            let id = node.node_id();
            if id.is_some_and(|id| id != owner && self.signature_parts_of(id).is_some()) {
                continue;
            }
            if id.is_some_and(|id| self.nodes.kind(id) == SyntaxKind::ReturnStatement) {
                return true;
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
        false
    }

    /// One parameter, or `None` for a form whose printed name this port cannot
    /// reproduce.
    fn parameter_of(&mut self, node: &ParameterDeclaration<'a>) -> Option<Parameter> {
        let Some(tsr_ast::BindingName::Identifier(name)) = node.name else {
            // A binding pattern. `parameterToParameterDeclarationName` renders one
            // from the pattern, and a generated name compared verbatim against a
            // baseline is a guess.
            return None;
        };
        let id = node.node_id?;
        let symbol = self.binder.symbol_of(id)?;
        let r#type = self.get_type_of_symbol(symbol);
        if r#type == self.intrinsics.error {
            return None;
        }
        Some(Parameter {
            name: name.text.to_string(),
            // Filled in by the caller: optionality needs the whole list.
            optional: false,
            rest: node.dot_dot_dot_token.is_some(),
            r#type,
        })
    }

    /// One type parameter, or `None` for a form this port cannot print exactly.
    fn type_parameter_of(&mut self, node: &TypeParameterDeclaration<'a>) -> Option<TypeParameter> {
        if !node.modifiers.is_empty() {
            return None;
        }
        let name = node.name?.text.to_string();
        let error = self.intrinsics.error;
        let resolve = |checker: &mut Self, annotation: Option<TypeNode<'a>>| match annotation {
            None => Some(None),
            Some(annotation) => {
                let id = checker.get_type_from_type_node(annotation);
                (id != error).then_some(Some(id))
            }
        };
        let constraint = resolve(self, node.constraint)?;
        let default = resolve(self, node.default_type)?;
        Some(TypeParameter { name, constraint, default })
    }

    /// The signature-shaped parts of a node, or `None` if it is not one of the
    /// function-like kinds this slice reaches.
    ///
    /// Accessors, constructors, class static blocks, and the call/construct/index
    /// signature members are function-like upstream and are deliberately absent:
    /// an accessor's type comes from `getTypeOfAccessors`, a constructor's from
    /// the class, and the three signature members are reached through a type
    /// literal rather than through a symbol's type.
    fn signature_parts_of(&self, id: NodeId) -> Option<SignatureParts<'a>> {
        match self.node_map.get(id)? {
            Node::FunctionDeclaration(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: node.asterisk_token.is_some(),
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.and_then(|body| body.node_id()),
                may_return_never: false,
            }),
            Node::MethodDeclaration(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: node.asterisk_token.is_some(),
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.and_then(|body| body.node_id()),
                // `mayReturnNever` (`checker.go:20312`) is true for a method of an
                // *object literal* only.
                may_return_never: self.nodes.parent(id).is_some_and(|parent| {
                    self.nodes.kind(parent) == SyntaxKind::ObjectLiteralExpression
                }),
            }),
            Node::MethodSignatureDeclaration(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: false,
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: None,
                may_return_never: false,
            }),
            Node::FunctionExpression(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: node.asterisk_token.is_some(),
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.and_then(|body| body.node_id()),
                may_return_never: true,
            }),
            Node::ArrowFunction(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: false,
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.and_then(|body| body.node_id()),
                may_return_never: true,
            }),
            _ => None,
        }
    }

    /// Render a signature as a `FunctionTypeNode` is printed: `<T>(x?: A, ...r: B[]) => C`.
    ///
    /// Ported from `NodeBuilderImpl.signatureToSignatureDeclarationHelper` with
    /// `kind == ast.KindFunctionType` (`nodebuilderimpl.go:1792`) and the printer
    /// that emits the resulting node. The spacing is not a style choice — the
    /// whole line is compared verbatim.
    pub(crate) fn signature_to_string(&self, signature: &Signature) -> String {
        let mut out = String::new();
        if !signature.type_parameters.is_empty() {
            out.push('<');
            for (index, parameter) in signature.type_parameters.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&parameter.name);
                if let Some(constraint) = parameter.constraint {
                    out.push_str(" extends ");
                    out.push_str(&self.type_to_string(constraint));
                }
                if let Some(default) = parameter.default {
                    out.push_str(" = ");
                    out.push_str(&self.type_to_string(default));
                }
            }
            out.push('>');
        }
        out.push('(');
        for (index, parameter) in
            signature.this_parameter.iter().chain(signature.parameters.iter()).enumerate()
        {
            if index > 0 {
                out.push_str(", ");
            }
            if parameter.rest {
                out.push_str("...");
            }
            out.push_str(&parameter.name);
            out.push_str(if parameter.optional { "?: " } else { ": " });
            out.push_str(&self.type_to_string(parameter.r#type));
        }
        out.push_str(") => ");
        out.push_str(&self.type_to_string(signature.r#type));
        out
    }
}
