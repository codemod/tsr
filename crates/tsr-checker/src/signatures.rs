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
    /// The written annotation's own text, when the node builder would reuse
    /// the node instead of re-printing the computed type.
    ///
    /// `symbolToParameterDeclaration` reaches `serializeTypeForDeclaration`
    /// (`nodebuilderimpl.go:2216`), whose reuse branch
    /// (`tryReuseExistingTypeNode`, `nodebuilderimpl.go:2229` region) keeps the
    /// **written** node when the type it denotes equals the computed type —
    /// which holds by construction for the very annotation this signature's
    /// type was computed *from*. Carried only for a `TypeQueryNode` annotation
    /// (`typeof a`), the one form whose computed print differs from its
    /// written text: measured on the first `bd tsr-4sc.10` run, 1,341
    /// gap→wrong lines, 1,059 of them with `typeof` still in the wanted text
    /// (`docs/architecture/checker-notes-tquery.md` §5). Wider reuse — every
    /// annotation — stands refused at 9.1 lost-per-gained (`bd tsr-a2c`).
    pub written_text: Option<String>,
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
    /// The written constraint's text under the same node-reuse rule as
    /// [`Parameter::written_text`] — `<T extends [number] | [string]>` prints
    /// the written constituent order, not the comparator's.
    pub written_constraint: Option<String>,
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
    /// The written return annotation's text, under the same node-reuse rule as
    /// [`Parameter::written_text`] — `serializeReturnTypeForSignature` reuses
    /// the written node too, and `typeof a` in return position is the corpus's
    /// most common carrier (`subtypingWithCallSignatures2` et al.).
    pub written_return: Option<String>,
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
    /// - **An inferred return type whose returns yield more than one distinct
    ///   type.** That is `getUnionTypeEx(types, UnionReductionSubtype)`
    ///   (`checker.go:20191`), and subtype reduction is not ported
    ///   (`crate::unions`, "Subtype reduction"). A body whose returns yield
    ///   *one* distinct type needs no union and is answered — see
    ///   [`Checker::inferred_return_type`].
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

        let written_return =
            return_annotation.and_then(|annotation| self.written_annotation_text(annotation));
        Some(Signature {
            declaration,
            type_parameters,
            this_parameter,
            parameters,
            r#type,
            written_return,
        })
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
        self.return_type_from_body(declaration, body, modifiers, asterisk, may_return_never)
    }

    /// What a function-like **body** infers, for a caller that has already
    /// established there is no annotation to take instead.
    ///
    /// Ported from `Checker.getReturnTypeFromBody` (`checker.go:20126`), and it
    /// is upstream's own seam rather than one invented for this port: upstream
    /// splits the annotation test into `getReturnTypeFromAnnotation`
    /// (`:20058`) and reaches this function only when that answers nothing.
    /// `getTypeOfAccessors` (`:18511`) calls it directly for the same reason a
    /// signature does — case 4, a getter with no annotation whose type is
    /// whatever its body returns.
    ///
    /// # Why this is `pub(crate)` and [`Checker::return_type_of`] is not
    ///
    /// The signature path must consult the annotation first and must answer
    /// `anyType` for a body-less declaration (an overload signature, an
    /// interface method); an accessor caller wants neither. Exposing
    /// `return_type_of` would hand out those two decisions along with the
    /// inference, and a caller that already made them would have to work out
    /// which of six arguments suppress them. This takes the declaration and its
    /// body and nothing else.
    ///
    /// **`None` is every gap**, and they are the ones
    /// [`Checker::get_signature_from_declaration`] lists — an aggregate of two
    /// or more distinct types, an `async` or generator body, a return
    /// expression whose own type is a gap. A caller must keep answering
    /// `errorType` for `None` rather than substituting `anyType`: for an
    /// accessor that distinction is load-bearing, because
    /// `getTypeOfAccessors`' *fifth* case answers a computed `anyType` for an
    /// accessor with no annotation and no getter body, and an inferable getter
    /// that fell into it would print a plausible wrong `any` where
    /// `accessorBodyInTypeContext.types` records `>foo : number`.
    ///
    /// # The `expect` on this item is a handshake, not a suppression
    ///
    /// It has no caller yet — `getTypeOfAccessors`' case 4 is the one it was
    /// cut for, and that lives in `crate::symbols`. `#[expect]` rather than
    /// `#[allow]` because an *unfulfilled* expectation is itself a warning: the
    /// moment a caller lands, `-D warnings` fails until this attribute is
    /// deleted. So the scaffold cannot outlive its purpose by being forgotten,
    /// which is what `#[allow(dead_code)]` would have permitted.
    pub(crate) fn get_return_type_from_body(&mut self, declaration: NodeId) -> Option<TypeId> {
        let (body, modifiers, asterisk, may_return_never) =
            // A get accessor is function-like upstream and reaches
            // `getReturnTypeFromBody` through the same door, but it is
            // deliberately **not** added to [`Checker::signature_parts_of`]:
            // that function's domain is what `getSignaturesOfSymbol` iterates,
            // and widening it would make an accessor symbol contribute a call
            // signature — so a symbol merging a method with a getter
            // (`METHOD | GET_ACCESSOR`, the case `crate::symbols` documents at
            // its dispatch) would start printing as a function type. The parts
            // are read here instead, where only this caller sees them.
            if let Some(Node::GetAccessorDeclaration(node)) = self.node_map.get(declaration) {
                (
                    Body::Block(node.body.and_then(|body| body.node_id())?),
                    node.modifiers,
                    node.asterisk_token.is_some(),
                    // `mayReturnNever` (`checker.go:20312`) covers a function
                    // expression, an arrow and an object-literal method. An
                    // accessor is none of them, so a getter whose body never
                    // returns a value is `void` and not `never`.
                    false,
                )
            } else {
                let parts = self.signature_parts_of(declaration)?;
                (parts.body?, parts.modifiers, parts.asterisk, parts.may_return_never)
            };
        self.return_type_from_body(declaration, body, modifiers, asterisk, may_return_never)
    }

    /// `getReturnTypeFromBody`'s body, shared by the signature path and
    /// [`Checker::get_return_type_from_body`] so the two cannot drift.
    fn return_type_from_body(
        &mut self,
        declaration: NodeId,
        body: Body<'a>,
        modifiers: &[ModifierLike<'_>],
        asterisk: bool,
        may_return_never: bool,
    ) -> Option<TypeId> {
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
        let returns = self.return_expressions_of(block, declaration);
        // `checkAndAggregateReturnExpressionTypes` (`checker.go:20259`), reduced
        // to the aggregate that needs no union. Upstream appends with
        // `core.AppendIfUnique` (`:20295`), so *n* returns of the same type
        // aggregate to one — see [`Checker::inferred_return_type`] for why that
        // makes this a test of distinct types rather than of return statements.
        let mut types: Vec<TypeId> = Vec::new();
        let mut has_bare_return = false;
        for expression in returns {
            let Some(expression) = expression else {
                // `if expr == nil { hasReturnWithNoExpression = true }` (`:20266`).
                has_bare_return = true;
                continue;
            };
            let id = self.check_expression(expression);
            if id == self.intrinsics.error {
                return None;
            }
            if !types.contains(&id) {
                types.push(id);
            }
        }
        match types.as_slice() {
            [] if has_bare_return => {
                // Every return is bare. `hasReturnWithNoExpression` is then true,
                // which fails `:20298`'s never-returning guard whatever
                // `mayReturnNever` says, and `:20175`'s empty-aggregate arm
                // answers `voidType`. A bare `return;` is itself the proof that
                // the body's end is reachable, so this does not consult
                // [`Checker::block_completes_normally`].
                return Some(self.intrinsics.void);
            }
            [] => {}
            // A bare `return;` *beside* a valued one is the one configuration
            // where `strictNullChecks` changes the answer: `:20301` appends
            // `undefinedType` to the aggregate under it and not otherwise, so
            // `function f() { if (c) return 1; return; }` is `number | undefined`
            // strict and `number` non-strict. This port has no compiler options
            // and is uniformly non-strict (`crate::symbols`), and the corpus does
            // set `@strict: true` on cases, so answering either spelling here
            // would be a confident wrong answer on half of them. Gapped.
            [_] if has_bare_return => return None,
            [single] => return self.inferred_return_type(declaration, *single),
            // Two or more distinct types. `bd tsr-4sc.9`.
            _ => return None,
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
        self.inferred_return_type(declaration, id)
    }

    /// `getReturnTypeFromBody`'s tail (`checker.go:20193`–`:20232`) applied to an
    /// inferred return type, shared by the concise-body arm and the single-type
    /// block arm.
    ///
    /// Sharing it is the point rather than a tidiness: upstream runs *one* tail
    /// over whichever of the two produced the type (`:20140` and `:20191` both
    /// fall into it), so a block body and a concise body must widen alike. Two
    /// copies would drift, and the drift would be invisible — both spellings
    /// print a type either way.
    ///
    /// # Why the aggregate is a test of *distinct types*, not of return count
    ///
    /// `checkAndAggregateReturnExpressionTypes` appends with `core.AppendIfUnique`
    /// (`checker.go:20295`), and `getUnionTypeEx` of a one-element list is that
    /// element (`crate::unions`). So a body with three `return "a"` statements
    /// aggregates to a single type and reaches no union at all — it is answerable
    /// here exactly as a one-return body is, and restricting this to a literal
    /// single `return` would gap it for no reason upstream recognises. Identity
    /// is `TypeId` equality, which is exact because this port interns types.
    ///
    /// # The unit result widens unconditionally, and that is upstream's own answer
    ///
    /// `getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded` (`:20221`) hands
    /// the type to `getWidenedLiteralLikeTypeForContextualType`, which widens
    /// unless `isLiteralOfContextualType(t, contextualType)`
    /// (`checker.go:25522`). **That function's last line is `return false` when
    /// `contextualType` is `nil`** (`:25551`), and this port computes no
    /// contextual types at all — `getContextualSignatureForFunctionLikeDeclaration`
    /// (`:29711`) is unported. So `nil` is the only value the argument could take
    /// here, and widening unconditionally *is* running upstream's function on
    /// this port's inputs, rather than a policy chosen in place of it.
    ///
    /// Before this, the tail refused to answer whenever widening would change the
    /// type and the declaration could not be *shown* to lack a contextual type —
    /// which for a function expression or an arrow meant everything except
    /// `const f = …`. The cost of that caution is measured, not argued
    /// (`docs/architecture/checker-notes-fnexpr.md` §9, `bd tsr-4e1`), over the
    /// 4,913-line population of function expressions whose signature build had
    /// every syntactic gate clear:
    ///
    /// ```text
    ///           gapping   right   wrong
    ///   before     2,000   2,463     450
    ///   after      1,337   3,028     548
    ///   delta       -663    +565     +98
    /// ```
    ///
    /// **565 converted against 98 manufactured — 85.2% of the lines that stopped
    /// gapping match the baseline character for character**, and corpus-wide the
    /// same change is +1,188 right against +156 wrong, over 171 cases with
    /// **zero regressing** and 29 newly complete. The 98 are the price of the
    /// missing contextual type, and they are visible: 23 lines want `() => true`
    /// and 6 want `() => false` where this now answers `() => boolean`, because
    /// something upstream supplied a boolean-literal contextual type and
    /// `isLiteralOfContextualType` said yes.
    ///
    /// **How you would know this is wrong.** Those 98 growing, or the ratio
    /// falling below the 70% match rate the decision was registered on, means the
    /// corpus has more literal-contextual positions than this measurement found.
    /// The instrument that would say so is
    /// `crates/tsr-conformance/examples/fnexpr.rs` §6c, which prints the
    /// right/gap/wrong split of that same population on every run.
    ///
    /// # The one position that still refuses, and why it is not the old guard
    ///
    /// `const f: () => 1 = () => 1` prints `() => 1`: a written annotation is a
    /// contextual type, it is a literal, and `isLiteralOfContextualType` says
    /// yes. This port cannot read that annotation — a function **type node**
    /// reaches [`Checker::get_signature_from_declaration`] but nothing joins it
    /// to the initialiser — so where one is *written* the answer is still a gap.
    ///
    /// That is a much smaller set than the old guard's: it asks whether a
    /// contextual type is **visible in the source at this position**, not whether
    /// one could exist. `has_no_contextual_type` refuses unless the position is
    /// `const f = …`; this refuses only when the position is `const f: T = …`.
    /// Everything between — a call argument, an object-literal property, a
    /// `return` expression — widens, which is what upstream does there.
    fn inferred_return_type(&mut self, declaration: NodeId, id: TypeId) -> Option<TypeId> {
        if id == self.intrinsics.error {
            return None;
        }
        let widened = self.get_widened_literal_type(id);
        if widened != id && self.has_a_written_contextual_type(declaration) {
            return None;
        }
        Some(widened)
    }

    /// Whether a contextual type for this function is **written down** at its
    /// position — the exact complement of [`Checker::has_no_contextual_type`]
    /// over the one position either can see.
    ///
    /// Deliberately narrow. `getContextualType` (`checker.go:29344`) reaches a
    /// call argument, an object-literal property and a `return` expression as
    /// well, and this recognises none of them: at those positions the contextual
    /// return type is almost never a *literal*, so `isLiteralOfContextualType`
    /// answers `false` and widening is upstream's answer too. Widening there and
    /// refusing here is the split the corpus measures at 569 converted against
    /// 104 manufactured — see the sibling doc on
    /// [`Checker::inferred_return_type`].
    fn has_a_written_contextual_type(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        matches!(
            self.node_map.get(parent),
            Some(Node::VariableDeclaration(node))
                if node.r#type.is_some()
                    && node.initializer.and_then(|i| Node::from(i).node_id()) == Some(declaration)
        )
    }

    /// The expression of every `return` statement belonging to `owner`, with
    /// `None` for a bare `return;`.
    ///
    /// Ported from `ast.ForEachReturnStatement`, whose contract is that it does
    /// **not** descend into a nested function-like node — a `return` inside an
    /// inner arrow belongs to the arrow. Iterative rather than recursive, for the
    /// reason `push_children` states: tree depth is a function of the source.
    ///
    /// **Order is not upstream's**, and nothing may come to depend on that: the
    /// stack yields siblings back to front. The one caller aggregates into a set
    /// and answers only when the set holds a single type, so order cannot reach
    /// the result. A caller that built a union would have to sort this first,
    /// because upstream's union constituents keep insertion order.
    fn return_expressions_of(
        &self,
        body: NodeId,
        owner: NodeId,
    ) -> Vec<Option<tsr_ast::Expression<'a>>> {
        let Some(root) = self.node_map.get(body) else { return Vec::new() };
        let mut found = Vec::new();
        let mut stack = vec![root];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            let id = node.node_id();
            if id.is_some_and(|id| id != owner && self.signature_parts_of(id).is_some()) {
                continue;
            }
            if let Node::ReturnStatement(statement) = node {
                found.push(statement.expression);
                // A `return` has no statements under it, but it does have an
                // expression, and descending would find a `return` inside a
                // function *expression* there. `signature_parts_of` above already
                // stops that; not descending at all is simply cheaper.
                continue;
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
        found
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
        // `symbolToParameterDeclaration` (`nodebuilderimpl.go:1654`) takes
        // `getTypeOfSymbol` and hands it to `serializeTypeForDeclaration`
        // (`:2216`), which **reuses the written annotation node** rather than
        // re-printing the computed type. The two differ for an optional
        // parameter, and the corpus records both spellings for the same one:
        //
        // ```text
        // export function assertWeird(value?: string): asserts value {
        // >assertWeird : (value?: string) => asserts value
        // >value : string | undefined
        // ```
        //
        // (`compiler/assertionWithNoArgument.types`, a `@strict: true` case.)
        // The declaration line prints the symbol's type, which carries the
        // `| undefined` a `?` adds (see [`crate::optionality`]); the signature
        // prints the annotation as written. Taking the symbol's type here would
        // print `(value?: string | undefined)`, which appears nowhere.
        let r#type = match node.r#type {
            Some(annotation) => self.get_type_from_type_node(annotation),
            None => self.get_type_of_symbol(symbol),
        };
        if r#type == self.intrinsics.error {
            return None;
        }
        let written_text =
            node.r#type.and_then(|annotation| self.written_annotation_text(annotation));
        Some(Parameter {
            name: name.text.to_string(),
            // Filled in by the caller: optionality needs the whole list.
            optional: false,
            rest: node.dot_dot_dot_token.is_some(),
            r#type,
            written_text,
        })
    }

    /// The written text of an annotation whose node the builder would reuse,
    /// for the rule on [`Parameter::written_text`]. **Only a `TypeQueryNode`
    /// qualifies**, and the scope is a measured negative, not an oversight: a
    /// `UnionTypeNode` leg — print the written constituent order where the
    /// computed union's is sorted, guarded on the written constituents mapping
    /// one-to-one onto the computed text's pieces — was built and **measured
    /// net-negative** (+323 gained, −270 lost, `promiseTypeStrictNull` alone
    /// −244), because upstream renders lib signature declarations *sorted*
    /// where the guard said reuse: upstream's gate involves the builder's
    /// enclosing declaration, not only the node's type. That family — written
    /// unions in `<T extends [number] | [string]>` and rest-parameter
    /// annotations, 9 lines — belongs to `bd tsr-5o2`, which now carries this
    /// measurement.
    fn written_annotation_text(&mut self, annotation: TypeNode<'a>) -> Option<String> {
        let _ = &self;
        Self::type_query_written_text(annotation)
    }

    /// The written text of a `typeof x` annotation, for the node-reuse rule on
    /// [`Parameter::written_text`]. `None` for every other node kind, and for
    /// the forms `get_type_from_type_query_node` refuses anyway.
    fn type_query_written_text(annotation: TypeNode<'_>) -> Option<String> {
        let TypeNode::TypeQueryNode(query) = annotation else { return None };
        if !query.type_arguments.is_empty() {
            return None;
        }
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
        Some(format!("typeof {}", segments.join(".")))
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
        let written_constraint =
            node.constraint.and_then(|annotation| self.written_annotation_text(annotation));
        Some(TypeParameter { name, constraint, written_constraint, default })
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
            // `getSignatureFromDeclaration` treats a function *type node* like
            // any other function-like declaration (`checker.go:19836` — it
            // switches on `declaration.Parameters()`, not on the kind), so this
            // mirrors `MethodSignatureDeclaration`: no modifiers, no asterisk,
            // no body, and therefore a return type of `any` when the annotation
            // is absent.
            //
            // **`ConstructorTypeNode` is deliberately not folded in here**, even
            // though upstream builds its signature through the same function and
            // the parts would line up. The two diverge in the *printer*: a
            // construct signature emits `ast.KindConstructorType`
            // (`nodebuilderimpl.go:2712`), which prints `new (x: T) => U`, and
            // `abstract new (x: T) => U` when the declaration carries the
            // `abstract` modifier that `FunctionTypeNode` cannot have.
            // [`Signature`] has no construct flag and
            // [`Checker::signature_to_string`] always emits the call form, so
            // adding the arm alone would print every one of the corpus's 523
            // constructor-type lines without its `new` — a wrong answer on all
            // of them rather than a gap. It needs a `construct` flag on
            // `Signature`, the `new `/`abstract new ` prefix, and a test per
            // spelling; that is a slice, not an arm.
            Node::FunctionTypeNode(node) => Some(SignatureParts {
                modifiers: &[],
                asterisk: false,
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: None,
                may_return_never: false,
            }),
            // A call or construct signature **member** of a type literal or
            // interface. Both reach `getSignatureFromDeclaration` upstream by
            // the same route every other function-like kind does.
            //
            // # Why this is not the `ConstructorTypeNode` refusal above
            //
            // That refusal is real and stands: a *type node* `new () => T` has
            // to print its own `new `, and [`Signature`] carries no construct
            // flag to print it from. **The member form does not have that
            // problem**, because the `new ` is supplied by the caller —
            // `get_type_from_type_literal` (`crate::declared`) already writes
            //
            // ```text
            // TypeElement::ConstructSignatureDeclaration(construct) => {
            //     Some((construct.node_id, None, "new ", false))
            // }
            // ```
            //
            // and has done since it was written. The prefix was correct and
            // unreachable: without these two arms `signature_parts_of` answered
            // `None`, `get_signature_from_declaration` answered `None`, and
            // that function's all-or-nothing rule gapped the **whole** literal.
            //
            // That one omission was measured at ~3,500 corpus lines: ~1,200 on
            // a literal containing a construct signature, ~305 on one
            // containing a call signature, and 2,003 more reached indirectly —
            // `p: X[]` where `type X = { … }` is a non-generic alias whose body
            // is such a literal, which is 98.5% of every `Alias[]` annotation
            // in the corpus.
            //
            // `r#type` and not `full_signature`: both fields exist on these
            // nodes, and `r#type` is the return annotation every other arm here
            // reads. The two are easy to confuse — this was checked against
            // `MethodSignatureDeclaration` rather than assumed.
            Node::CallSignatureDeclaration(node) => Some(SignatureParts {
                modifiers: &[],
                asterisk: false,
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: None,
                may_return_never: false,
            }),
            Node::ConstructSignatureDeclaration(node) => Some(SignatureParts {
                modifiers: &[],
                asterisk: false,
                type_parameters: node.type_parameters,
                parameters: node.parameters,
                return_annotation: node.r#type,
                body: None,
                may_return_never: false,
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
                    match &parameter.written_constraint {
                        Some(written) => out.push_str(written),
                        None => out.push_str(&self.type_to_string(constraint)),
                    }
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
            match &parameter.written_text {
                Some(written) => out.push_str(written),
                None => out.push_str(&self.type_to_string(parameter.r#type)),
            }
        }
        out.push_str(") => ");
        match &signature.written_return {
            Some(written) => out.push_str(written),
            None => out.push_str(&self.type_to_string(signature.r#type)),
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use tsr_ast::SyntaxKind;

    /// The printed signature of the first `FunctionTypeNode` in `source`.
    ///
    /// In-crate because [`super::Checker::get_signature_from_declaration`] is
    /// `pub(crate)` — which is the point: its consumer is
    /// `getTypeFromTypeNode`'s function-type arm, inside this crate, and the
    /// whole reason this arm exists is so that arm does not rebuild a signature
    /// by hand.
    fn signature_of(source: &str) -> String {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        for index in 0..parsed.nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let id = tsr_ast::NodeId::new(index as u32);
            // Both kinds, so the constructor test below asks the real question
            // — "does this arm claim it?" — rather than the vacuous one, "is
            // there a function type node in a constructor fixture?". The first
            // version asked the second, and no mutation could turn it red.
            if !matches!(
                parsed.nodes.kind(id),
                SyntaxKind::FunctionType | SyntaxKind::ConstructorType
            ) {
                continue;
            }
            return match checker.get_signature_from_declaration(id) {
                Some(signature) => checker.signature_to_string(&signature),
                None => "error".to_string(),
            };
        }
        "<no signature-bearing type node>".to_string()
    }

    #[test]
    fn a_function_type_node_yields_a_signature() {
        // The arm exists so `getTypeFromTypeNode` can reach this machinery
        // instead of re-deriving optionality, rest parameters,
        // `minArgumentCount` and the `this`-parameter split by hand.
        assert_eq!(signature_of("declare const f: (x: number) => void;"), "(x: number) => void");
        assert_eq!(signature_of("declare const f: () => void;"), "() => void");
        assert_eq!(signature_of("declare const f: <T>(p: T) => T;"), "<T>(p: T) => T");
        // A `?` and a rest parameter come through the shared path, which is the
        // whole argument for routing here rather than rebuilding.
        assert_eq!(
            signature_of("declare const f: (x?: string, y: number) => void;"),
            "(x?: string, y: number) => void"
        );
        // No body and no annotation is `any`, exactly as for a method signature:
        // `getReturnTypeOfSignature`'s `NodeIsMissing(Body())` arm.
        assert_eq!(signature_of("declare const f: (x: number) => any;"), "(x: number) => any");
    }

    /// The printed type of the first `ArrowFunction` or `FunctionExpression` in
    /// `source`, through the same entry `crate::expressions` uses.
    ///
    /// Deliberately **not** keyed on a variable name: the whole point of these
    /// two tests is the positions that are *not* `const f = …`, and a helper that
    /// went looking for a declaration would only ever reach the position that
    /// already worked.
    fn function_expression_type(source: &str) -> String {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut checker = crate::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        for index in 0..parsed.nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let id = tsr_ast::NodeId::new(index as u32);
            if !matches!(
                parsed.nodes.kind(id),
                SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression
            ) {
                continue;
            }
            let type_id = checker.get_type_of_function_expression(id);
            return checker.type_to_string(type_id);
        }
        "<no function expression>".to_string()
    }

    /// A unit return widens even where a contextual type could exist, because
    /// `isLiteralOfContextualType(t, nil)` is `false` (`checker.go:25551`) and
    /// this port has no contextual types to pass.
    ///
    /// Both mutations were run rather than reasoned about. **M1** — `return None`
    /// whenever widening would change the type, which is the guard this commit
    /// removed — answers `error` here. **M2** — return `Some(id)` instead of the
    /// widened type — answers `() => 1`. Each was applied, seen red, and reverted.
    #[test]
    fn a_unit_return_widens_outside_a_const_initialiser() {
        // A call argument. Upstream *does* compute a contextual signature here;
        // its return type is `number`, which is not a literal, so
        // `isLiteralOfContextualType` says no and the result widens anyway.
        assert_eq!(
            function_expression_type("declare function foo(f: () => number): void; foo(() => 1);"),
            "() => number"
        );
        // An object-literal property value — `ast.IsObjectLiteralMethod`'s
        // neighbour, and the position `has_no_contextual_type` could never admit.
        assert_eq!(function_expression_type("var o = { m: () => \"a\" };"), "() => string");
    }

    /// The position that already worked keeps working, and a **block** body
    /// reaches the same tail as a concise one.
    ///
    /// Red under both M1 and M2, at `"() => 1"` and `"error"` respectively. It is
    /// a **regression** test rather than the one that pins the change: the first
    /// fixture is the position the removed guard already admitted, and the second
    /// is a block body, which reaches this tail through the other of its two
    /// callers. If a future narrowing of the widening rule breaks either, it has
    /// gone further than this commit did.
    #[test]
    fn a_const_initialiser_and_a_block_body_reach_the_same_tail() {
        assert_eq!(function_expression_type("const f = () => 1;"), "() => number");
        assert_eq!(
            function_expression_type("const f = function () { return 1; };"),
            "() => number"
        );
    }

    #[test]
    fn a_constructor_type_node_is_not_reached_by_this_arm() {
        // Deliberate: the parts line up but the *printer* does not. A construct
        // signature emits `ast.KindConstructorType` and prints `new (x: T) => U`;
        // [`super::Signature`] has no construct flag, so folding it in here would
        // print 523 corpus lines without their `new` — a wrong answer on every
        // one rather than a gap. This pins the absence so the next person adds
        // the flag and the prefix together.
        assert_eq!(signature_of("declare const c: new (x: number) => void;"), "error");
    }
}
