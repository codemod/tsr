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

/// A function-like declaration's body.
///
/// Upstream reaches this through one accessor, `declaration.Body()`, and asks
/// `ast.IsBlock` about the result (`checker.go:20135`); only an arrow can carry
/// the other shape. The distinction is in the type here because it decides which
/// arm of `getReturnTypeFromBody` runs.
#[derive(Debug, Clone, Copy)]
enum Body<'a> {
    /// A `{ … }` body.
    Block(NodeId),
    /// A concise arrow body, `x => x + 1`.
    Expression(tsr_ast::Expression<'a>),
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
    body: Option<Body<'a>>,
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
    /// - **An inferred return type where `mayReturnNever` holds and the body's
    ///   end cannot be decided syntactically** — see
    ///   [`Checker::block_completes_normally`], which answers `void`/`never`
    ///   where the grammar forces it and refuses otherwise.
    /// - **A concise arrow body whose type is a literal, in a position that could
    ///   supply a contextual return type** — see
    ///   [`Checker::has_no_contextual_type`].
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
    /// without unions.
    fn return_type_of(
        &mut self,
        declaration: NodeId,
        annotation: Option<TypeNode<'a>>,
        body: Option<Body<'a>>,
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
        let block = match body {
            // `getReturnTypeFromBody`'s first arm, `!ast.IsBlock(body)`
            // (`checker.go:20135`): a concise arrow body is simply its
            // expression's type.
            Body::Expression(expression) => {
                return self.concise_return_type(declaration, expression);
            }
            Body::Block(block) => block,
        };
        if self.body_has_return_statement(block, declaration) {
            return None;
        }
        if !may_return_never {
            // Zero return statements and `mayReturnNever` false, so upstream's
            // `checkAndAggregateReturnExpressionTypes` yields no types and is not
            // never-returning: `getReturnTypeFromBody` answers `voidType`
            // (`checker.go:20200`). This holds even for a body that only throws.
            return Some(self.intrinsics.void);
        }
        // A function expression, an arrow, or an object-literal method with no
        // `return`. Upstream separates `never` from `void` here by asking whether
        // the **end of the body is reachable** (`functionHasImplicitReturn`,
        // `checker.go:20255`), which reads the flow graph. See
        // [`Checker::block_completes_normally`] for the conservative syntactic
        // stand-in and what it refuses to decide.
        match self.block_completes_normally(block, declaration) {
            Some(true) => Some(self.intrinsics.void),
            Some(false) => Some(self.intrinsics.never),
            None => None,
        }
    }

    /// The type of a concise arrow body, `x => x + 1`.
    ///
    /// Ported from `getReturnTypeFromBody`'s non-block arm (`checker.go:20135`)
    /// together with the widening its tail applies
    /// (`getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded`,
    /// `checker.go:20221`). A **unit** result is where the two part company:
    /// `const f = () => 1` is `() => number` because nothing supplied a
    /// contextual return type, and `const f: () => 1 = () => 1` is `() => 1`
    /// because something did. So a literal result is answered only where the
    /// absence of a contextual type can be *shown* — see
    /// [`Checker::has_no_contextual_type`] — and gapped otherwise.
    fn concise_return_type(
        &mut self,
        declaration: NodeId,
        expression: tsr_ast::Expression<'a>,
    ) -> Option<TypeId> {
        let id = self.check_expression(expression);
        if id == self.intrinsics.error {
            return None;
        }
        let widened = self.get_widened_literal_type(id);
        if widened != id && !self.has_no_contextual_type(declaration) {
            return None;
        }
        Some(widened)
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

    /// Whether the end of a block is reachable — `Some(true)` yes, `Some(false)`
    /// no, `None` **refuses to say**.
    ///
    /// A conservative syntactic stand-in for `functionHasImplicitReturn`
    /// (`checker.go:20255`), which asks the flow graph whether a function body's
    /// end flow node is reachable. The binder builds that graph and nothing reads
    /// it yet, so rather than approximate it this answers only the shapes where
    /// the answer is forced by the grammar, and gaps the rest.
    ///
    /// It is called only for a body with **no `return` statement**, which is what
    /// makes the enumeration short. In that situation a block's end is
    /// unreachable exactly when control cannot leave it normally, and the only
    /// syntactic ways to arrange that are `throw`, a loop with no exit, and a
    /// call to something typed `never`. So:
    ///
    /// - `throw` ends the block — `Some(false)`, and everything after it is dead.
    /// - `if`/`else` where **both** halves end the block ends it too.
    /// - a declaration, an empty statement or a `debugger` always completes.
    /// - an expression statement completes **unless it contains a call**, since
    ///   a `never`-returning call ends the block and this port cannot yet type
    ///   most calls — `None`.
    /// - a loop, a `switch`, a `try`, a labelled statement: `None`. Each *can*
    ///   be decided, and each needs the real analysis to decide correctly.
    ///
    /// The cost is measurable and deliberate: `() => { console.log(1); }` is a
    /// gap until calls can be typed, where upstream says `() => void`. The
    /// alternative — assuming a call completes — turns every `never`-returning
    /// helper into a wrong `void`, and a wrong answer here is worse than a gap
    /// because it is indistinguishable from a result in the histogram.
    fn block_completes_normally(&self, block: NodeId, owner: NodeId) -> Option<bool> {
        let node = self.node_map.get(block)?;
        let mut children = Vec::new();
        tsr_ast::push_children(node, &mut children);
        for child in children {
            let Some(id) = child.node_id() else { continue };
            match self.statement_completes_normally(id, owner) {
                // Unreachable from here on, which is the answer for the block.
                Some(false) => return Some(false),
                Some(true) => {}
                None => return None,
            }
        }
        Some(true)
    }

    /// One statement's contribution to [`Checker::block_completes_normally`].
    fn statement_completes_normally(&self, id: NodeId, owner: NodeId) -> Option<bool> {
        match self.nodes.kind(id) {
            SyntaxKind::ThrowStatement => Some(false),
            SyntaxKind::Block => self.block_completes_normally(id, owner),
            SyntaxKind::IfStatement => {
                let Some(Node::IfStatement(node)) = self.node_map.get(id) else { return None };
                let then = node
                    .then_statement
                    .and_then(|s| s.node_id())
                    .map_or(Some(true), |s| self.statement_completes_normally(s, owner))?;
                // No `else` means the `if` can always be skipped.
                let Some(otherwise) = node.else_statement else { return Some(true) };
                let otherwise = otherwise
                    .node_id()
                    .map_or(Some(true), |s| self.statement_completes_normally(s, owner))?;
                Some(then || otherwise)
            }
            SyntaxKind::VariableStatement
            | SyntaxKind::EmptyStatement
            | SyntaxKind::DebuggerStatement
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::ModuleDeclaration
            | SyntaxKind::ImportDeclaration
            | SyntaxKind::ImportEqualsDeclaration => Some(true),
            // `Some(true)` when nothing in it can fail to return; `None` when a
            // call is in the way, because a `never`-returning call ends the block.
            SyntaxKind::ExpressionStatement => (!self.contains_a_call(id)).then_some(true),
            // Every remaining statement form — loops, `switch`, `try`, labels,
            // `with`, `for…of` — can be decided and needs the real analysis to be
            // decided correctly.
            _ => None,
        }
    }

    /// Whether a subtree contains a call or `new`, without entering a nested
    /// function.
    ///
    /// The one thing standing between an expression statement and "this
    /// completes": a call to a `never`-returning function ends the block.
    fn contains_a_call(&self, root: NodeId) -> bool {
        let Some(node) = self.node_map.get(root) else { return true };
        let mut stack = vec![node];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            if let Some(id) = node.node_id() {
                if id != root && self.signature_parts_of(id).is_some() {
                    continue;
                }
                if matches!(
                    self.nodes.kind(id),
                    SyntaxKind::CallExpression
                        | SyntaxKind::NewExpression
                        | SyntaxKind::TaggedTemplateExpression
                ) {
                    return true;
                }
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
        false
    }

    /// Whether this function-like node demonstrably has **no** contextual type.
    ///
    /// A conservative stand-in for the absence of
    /// `getContextualSignatureForFunctionLikeDeclaration` (`checker.go:20226`).
    /// It decides two things that would otherwise be wrong answers rather than
    /// gaps: whether an unannotated parameter is really `any`, and whether a
    /// literal return widens.
    ///
    /// Only one shape is recognised — the initialiser of a `var`/`let`/`const`
    /// with **no type annotation**, which is `const f = …` and is where most
    /// function expressions in the corpus live. Every other position (a call
    /// argument, an annotated declaration, an object-literal property, a
    /// `return` expression, an `as`) can supply a contextual type, and this
    /// refuses to guess which.
    fn has_no_contextual_type(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        matches!(
            self.node_map.get(parent),
            Some(Node::VariableDeclaration(node))
                if node.r#type.is_none()
                    && node.initializer.and_then(|i| Node::from(i).node_id()) == Some(declaration)
        )
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
                body: node.body.and_then(|body| body.node_id()).map(Body::Block),
                may_return_never: false,
            }),
            Node::MethodDeclaration(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                asterisk: node.asterisk_token.is_some(),
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.and_then(|body| body.node_id()).map(Body::Block),
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
                body: node.body.and_then(|body| body.node_id()).map(Body::Block),
                may_return_never: true,
            }),
            Node::ArrowFunction(node) => Some(SignatureParts {
                modifiers: node.modifiers,
                // An arrow cannot be a generator, but the parser accepts
                // `async *() =>` in error recovery, so the token is read rather
                // than assumed absent.
                asterisk: node.asterisk_token.is_some(),
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: node.body.map(|body| match tsr_ast::Expression::try_from(Node::from(body)) {
                    // A concise body is an *expression*; a block is not, which is
                    // exactly the test `getReturnTypeFromBody` makes with
                    // `ast.IsBlock` (`checker.go:20135`).
                    Ok(expression) => Body::Expression(expression),
                    Err(_) => {
                        Body::Block(body.node_id().expect("a parsed arrow body is registered"))
                    }
                }),
                may_return_never: true,
            }),
            _ => None,
        }
    }

    /// The type of a function expression or arrow function.
    ///
    /// Ported from `Checker.checkFunctionExpressionOrObjectLiteralMethod`
    /// (`checker.go:9077`) into `getTypeOfSymbol` on the function's own symbol —
    /// the binder gives every function expression and arrow one (`__function`,
    /// or its name where it has one), and its type is the anonymous object type
    /// carrying that one signature. So this is the same arm
    /// `getTypeOfFuncClassEnumModule` already provides, reached from an
    /// expression instead of from a name.
    ///
    /// # An unannotated parameter is a wrong answer waiting to happen
    ///
    /// This is the one place in this port where the implicit `any` is not safe.
    /// `getTypeOfSymbol` on an unannotated parameter answers `anyType`, which is
    /// upstream's answer *only when nothing supplies a contextual type*. In
    ///
    /// ```text
    /// const f: (x: number) => void = x => {};
    /// >x : number
    /// ```
    ///
    /// upstream types `x` from the contextual signature and this port would print
    /// `any` — a wrong line dressed as a computed one. So a function expression
    /// with any unannotated parameter is answered only where
    /// [`Checker::has_no_contextual_type`] can *show* there is no contextual type,
    /// and gapped otherwise. Contextual typing itself is the next item here; it
    /// needs function **type nodes**, which `getTypeFromTypeNode` does not yet
    /// have.
    pub(crate) fn get_type_of_function_expression(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        let Some(parts) = self.signature_parts_of(node) else { return error };
        let unannotated = parts.parameters.iter().any(|parameter| parameter.r#type.is_none());
        if unannotated && !self.has_no_contextual_type(node) {
            return error;
        }
        let Some(symbol) = self.binder.symbol_of(node) else { return error };
        self.get_type_of_symbol(symbol)
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
