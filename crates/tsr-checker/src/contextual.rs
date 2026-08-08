//! Contextual typing: what an *unannotated* parameter's type comes from when
//! the function it belongs to is written where a function type is expected.
//!
//! Ported from `Checker.getContextuallyTypedParameterType` (`checker.go:29458`),
//! reduced to the three corpus arms that need no type inference: a function
//! expression or arrow passed as a **call argument**, typed from the
//! corresponding parameter of the callee's signature; one that is the
//! initialiser of a variable carrying a **type annotation**, typed from the
//! annotation; and one that is the value of a **member of an object literal**
//! which itself has a contextual type, typed from the matching property of it.
//!
//! ```text
//! declare function each(callback: (item: string) => void): void;
//! each(item => item.length);
//! //   ^^^^ `string`, from `callback`, not the implicit `any`
//! ```
//!
//! # Why this is worth a module of its own, and what it is expected to move
//!
//! Contextual typing did not exist in this port at all, and it is the named
//! owner of the largest *known-wrong* population the checker has: the `any`
//! receiver arm in [`crate::members`] carries a measured **+555 wrong
//! property-access lines**, every one of them an unannotated parameter that
//! answered the implicit `any` here and then answered `any` again on the member.
//! Those lines cost no gradient — the parameter's own line was already wrong —
//! but they cost diagnostic separability, which is what this project's method
//! rests on.
//!
//! So the expected shape of the win is unusual and worth stating before the
//! measurement rather than after: **this turns wrong lines into right ones
//! without moving the gap column.** A flat gradient with an improved right/wrong
//! split is the success case, not the failure case.
//!
//! # Concentration: measured before writing, and it is diffuse
//!
//! Counted over `vendor/typescript-go/testdata/baselines/reference/submodule`
//! at the pinned commit, by the signature a contextually typed function leaves
//! in a `.types` baseline — an assertion whose subject is a function expression
//! with **unannotated** parameters and whose type has **typed** parameters, at
//! least one of them not `any`:
//!
//! - **925** such function expressions, in **349** files. The largest single
//!   file holds 42 (4.5%); the top 25 files hold 36%.
//! - Their assertion-line footprint — lines whose subject roots at one of those
//!   parameters — is **5,230 lines across 325 files**, 0.88% of the corpus's
//!   594,122 assertion lines, top 10 files 30%.
//!
//! That check exists because this project has been burned three times by a large
//! row that turned out to be one file. This one is not: it is genuinely spread,
//! which is what makes it worth porting and also what makes any single test
//! fixture a poor proxy for it.
//!
//! # What is reachable without inference, measured rather than assumed
//!
//! The obvious shape — `arr.map(x => x.foo)` — is **not** reachable. `map` is
//! generic and `every` is an overload set, and this port has no
//! `inferTypeArguments`. Classifying the 925 by what their callee's printed type
//! is:
//!
//! | callee | count | reachable |
//! |---|---|---|
//! | one non-generic signature | 548 (59%) | yes — this module |
//! | generic | 183 (20%) | no: needs inference |
//! | overload set | 102 (11%) | no: needs the full `resolveCall` |
//! | not classified by the counter | 92 (10%) | — |
//!
//! The reachable bucket is itself spread over 206 files, largest 37 (6.7%). It
//! is the first arm this module ports, and the generic and overloaded callees
//! are gaps that stay gaps.
//!
//! The second arm — an arrow initialising an **annotated variable**,
//! `const f: (x: T) => U = x => …` — is smaller but cheaper still: **75
//! functions in 32 files**, largest 8 (10.7%), and it needs no call machinery at
//! all, because `getContextualTypeForVariableLikeDeclaration`
//! (`checker.go:29438`) returns the annotation's type directly.
//!
//! The third arm — a **member of an object literal** that itself has a
//! contextual type — is the largest remaining one: 297 functions in 98 files, of
//! which **66 in 33 files** sit in an object literal whose own context this port
//! can already answer. The per-arm measurement that ranked it above `return`
//! (26), array element (33) and parenthesised expression (101, but ~32
//! reachable in 7 files) is in
//! [`Checker::contextual_type_for_object_literal_element`] and in
//! `docs/architecture/checker-notes-ctx.md`.
//!
//! # A gap here is the implicit `any`, and that is not a violated rule
//!
//! Everywhere else in this checker an unported form answers `errorType`, because
//! a confident wrong answer is worse than an honest "don't know". This module
//! answers `None`, and one level up in
//! `Checker::get_type_for_variable_like_declaration` that becomes `anyType`.
//!
//! That is correct rather than a lapse: for a parameter, `anyType` **is**
//! upstream's answer when no contextual type is available (`checker.go:18264`,
//! reached from a nil `getContextuallyTypedParameterType`). `function f(x) {}`
//! types `x` as `any` in TypeScript too. Returning `errorType` from an
//! unreachable arm here would convert lines that are currently *right* into
//! wrong ones. The rule this module is actually bound by is the stronger one it
//! is derived from: **never invent an answer**. `None` invents nothing — it
//! leaves behaviour exactly as it was before this module existed, so the only
//! lines this module can move are ones it types deliberately.
//!
//! # The recursion upstream guards, and how this avoids needing the guard
//!
//! `getContextualTypeForArgumentAtIndex` (`checker.go:29772`) resolves the
//! call's signature to find the argument's contextual type — while it is being
//! reached *from* checking that same call's argument. Upstream breaks the cycle
//! with signature links: it parks `anySignature`/`resolvingSignature` in
//! `signatureLinks` for the duration (`checker.go:29785`).
//!
//! This port has no such cache, so it cannot use that guard, and it must not
//! call [`crate::calls`]'s `resolve_call_signature`: that function now takes the
//! argument list and hands it to `choose_overload`, which **checks every
//! argument** — including the very arrow whose parameter this is trying to type.
//! Instead this module reads the callee's signatures directly and requires there
//! to be exactly one, non-generic. Selecting that signature needs no argument
//! checked at all, so the cycle is not broken, it is never entered. The cost is
//! the 102 overloaded callees in the table above, and paying it is what makes
//! this arm safe without a links table (`bd` follow-up: signature links).

use tsr_ast::{
    BindingName, CallExpression, Expression, Node, NodeId, ParameterDeclaration,
    PropertyAssignment, PropertyName,
};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    /// The type an unannotated parameter takes from its context, if any.
    ///
    /// Ported from `Checker.getContextuallyTypedParameterType`
    /// (`checker.go:29458`), for the arms [`Checker::get_contextual_type`]
    /// supplies.
    ///
    /// `None` is upstream's nil: no contextual type is available, and the caller
    /// turns that into the implicit `any`. See the module documentation for why
    /// `None` rather than `errorType` is the honest answer *here specifically*.
    ///
    /// Not ported, each answering `None`:
    ///
    /// - **An immediately invoked function expression** (`(x => x)(1)`), which
    ///   upstream types from the *argument expressions* rather than from a
    ///   signature (`checker.go:29463`). It is a separate mechanism — the
    ///   corpus's `contextuallyTypedIife` cases are 28 of the 925 — and folding
    ///   it in here would share none of the code below.
    /// - **Every context beyond the three above**: a `return`, a JSX attribute,
    ///   a binary operand, an array element, a parenthesised expression. Each
    ///   needs its own arm of `getContextualType` (`checker.go:29343`); the
    ///   populations and the concentration that rejected them are in
    ///   [`Checker::get_contextual_type`].
    /// - **A rest parameter or a `this` parameter** on the contextually typed
    ///   function, which shift what "the parameter at this index" means —
    ///   upstream calls `getRestTypeAtPosition` for the first and subtracts the
    ///   second from the index. Both are answerable later; neither is guessed.
    /// - **A generic or overloaded callee**, and a callee that is not a call at
    ///   all. See the module documentation.
    pub(crate) fn get_contextually_typed_parameter_type(
        &mut self,
        parameter: NodeId,
    ) -> Option<TypeId> {
        let function = self.nodes.parent(parameter)?;
        let parameters = self.contextualisable_parameters(function)?;

        // `slices.Index(fn.Parameters(), parameter)` (`checker.go:29489`). A
        // `this` parameter would make this index disagree with the signature's,
        // which is exactly what upstream's `GetThisParameter` subtraction on the
        // same line repairs; here it is a gap instead, so the two indices are
        // the same index or there is no answer.
        let index = parameters.iter().position(|p| p.node_id == Some(parameter))?;
        if parameters.iter().any(|p| p.dot_dot_dot_token.is_some() || is_this_parameter(p)) {
            return None;
        }

        let signature = self.contextual_signature(function)?;
        let contextual = signature.parameters.get(index)?;
        // A contextual parameter that is itself optional or rest carries a type
        // whose relationship to the position is not the plain one — upstream
        // reaches those through `getRestTypeAtPosition` and the optionality
        // rules in `crate::optionality`. Neither is asserted from here.
        if contextual.optional || contextual.rest {
            return None;
        }
        Some(contextual.r#type)
    }

    /// The parameter list of `function`, if it is a form that can be
    /// contextually typed.
    ///
    /// Ported from `Checker.isContextSensitiveFunctionOrObjectLiteralMethod`
    /// (`checker.go:29495`), minus the object-literal method — which needs the
    /// object literal's own contextual type, an arm this module does not have.
    ///
    /// The `isContextSensitiveFunctionLikeDeclaration` half of upstream's test
    /// holds by construction rather than by a check: the only caller reaches
    /// here for a parameter that has no type annotation, which is what makes a
    /// function context-sensitive in the first place.
    fn contextualisable_parameters(
        &self,
        function: NodeId,
    ) -> Option<&'a [&'a ParameterDeclaration<'a>]> {
        match self.node_map.get(function)? {
            Node::ArrowFunction(node) => Some(node.parameters),
            Node::FunctionExpression(node) => Some(node.parameters),
            _ => None,
        }
    }

    /// The signature `function` is contextually typed by, whichever context
    /// supplies it.
    ///
    /// Ported from `Checker.getContextualSignature` (`checker.go:10264`), which
    /// reaches every context through one `getContextualType` switch — here
    /// [`Checker::get_contextual_type`].
    ///
    /// Upstream's union handling (`getContextualSignature`'s
    /// `compareSignaturesIdentical` loop) is not ported: a union-typed context
    /// answers `None`, because picking one member's signature is a guess and
    /// building the combined one needs `createUnionSignature`.
    fn contextual_signature(&mut self, function: NodeId) -> Option<Signature> {
        let contextual = self.get_contextual_type(function)?;
        self.single_call_signature(contextual)
    }

    /// The type `node` is expected to have, from where it is written.
    ///
    /// Ported from `Checker.getContextualType` (`checker.go:29343`) — the
    /// dispatch every context reaches, a `switch` on the **parent's** kind. Three
    /// of its twenty arms are here.
    ///
    /// # Why this is a dispatch on an expression and not a helper on a function
    ///
    /// The first version of this module had no such function: it asked "is this
    /// *function* a call argument, or the initialiser of an annotated variable?"
    /// and answered a `Signature` directly. That shape cannot express the
    /// object-literal arm, whose subject is the enclosing **object literal** —
    /// an expression that is not a function and has no signature. Upstream's own
    /// structure is the fix and it is a strict generalisation: each of the two
    /// existing arms was already `single_call_signature(<a type>)`, so factoring
    /// the type out changes no answer. The object-literal arm is then a
    /// recursive call rather than a fourth special case, which is exactly how
    /// `checker.go:29925`'s `getApparentTypeOfContextualType(objectLiteral)`
    /// reaches it.
    ///
    /// # The seventeen arms that are not here
    ///
    /// Each was measured over
    /// `vendor/typescript-go/testdata/baselines/reference/submodule` before being
    /// left out; the counts and the concentration are in
    /// `docs/architecture/checker-notes-ctx.md`. The three largest omissions:
    ///
    /// - **`ParenthesizedExpression`** (`checker.go:29392`), a one-line recursion
    ///   — 101 contextually typed functions, but only ~32 of them in a context
    ///   this port can already answer, and those sit in 7 files with
    ///   `parenthesizedContexualTyping{1,2,3}` holding most of them. Rejected on
    ///   concentration, not on cost.
    /// - **`ReturnStatement`** (`checker.go:29621`) — 26 functions in 15 files,
    ///   top 10 files 81%.
    /// - **`ArrayLiteralExpression`** (`checker.go:29380`, via
    ///   `getContextualTypeForElementExpression`, `checker.go:29972`) — 33
    ///   functions in 15 files, top 10 files 85%, and every one of them needs
    ///   tuple types this port does not have.
    ///
    /// A `NewExpression` shares upstream's `CallExpression` arm and would cost
    /// one pattern here, but it is not measured separately, so it is a gap.
    fn get_contextual_type(&mut self, node: NodeId) -> Option<TypeId> {
        let parent = self.nodes.parent(node)?;
        match self.node_map.get(parent)? {
            // `getContextualTypeForInitializerExpression` (`checker.go:29423`) →
            // `getContextualTypeForVariableLikeDeclaration` (`checker.go:29438`),
            // whose first three lines are the whole of this arm: if the
            // declaration has a type node, the contextual type *is* that type.
            //
            // Restricted to a `VariableDeclaration`. The other variable-like
            // carriers of an annotation — a `PropertyDeclaration`, a
            // `PropertySignature`, a parameter with a function-typed annotation
            // and a function initialiser — reach the same upstream function, but
            // each is a separate corpus shape and none is measured, so each is a
            // gap rather than an untested generalisation.
            //
            // No check that `node` is the *initialiser*. A `VariableDeclaration`
            // has three children — name, type annotation, initialiser — and only
            // the initialiser is an expression, so the parent test already
            // establishes it. An earlier draft asserted it anyway; no mutation
            // could make the assertion observable, including the one that looked
            // most dangerous — typing the annotation's own parameter from the
            // signature it belongs to — because such a parameter's parent is the
            // `FunctionType` node and [`Checker::contextualisable_parameters`]
            // turns it away one level up. Removed rather than kept as decoration,
            // which is the call `crate::members` records making for the same
            // reason.
            Node::VariableDeclaration(declaration) => {
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            Node::CallExpression(call) => self.contextual_type_for_argument(call, node),
            Node::PropertyAssignment(element) => {
                self.contextual_type_for_object_literal_element(parent, element)
            }
            // §68 (`checker-notes-narrow.md`): `getContextualTypeForReturnExpression`
            // (`checker.go:29621`), the WRITTEN-annotation half — a returned
            // expression's contextual type is the enclosing function's
            // declared return type. Rejected once at 26 functions;
            // `generatedContextualTyping` alone holds 62 aligned lines now.
            // §68.1: `ParenthesizedExpression` (`checker.go:29392`) — the
            // one-line recursion, un-rejected with §68's own argument.
            Node::ParenthesizedExpression(_) => self.get_contextual_type(parent),
            // §68.2: an ARRAY LITERAL element's contextual type is the
            // array's contextual ELEMENT type
            // (`getContextualTypeForElementExpression`, `checker.go:29972`) —
            // the Array-reference and tuple halves this port can read.
            Node::ArrayLiteralExpression(literal) => {
                let contextual = self.get_contextual_type(parent)?;
                if let Some((target, arguments)) =
                    self.type_reference_targets.get(&contextual).cloned()
                    && arguments.len() == 1
                    && ["Array", "ReadonlyArray"].iter().any(|name| {
                        self.global_type_symbol(name).map(|s| self.binder.merged_symbol(s))
                            == Some(self.binder.merged_symbol(target))
                    })
                {
                    return Some(arguments[0]);
                }
                if let Some((elements, _)) = self.tuple_element_lists.get(&contextual) {
                    let index = literal
                        .elements
                        .iter()
                        .position(|element| element.node_id() == Some(node))?;
                    return elements.get(index).copied();
                }
                None
            }
            // §68.3: a CONCISE arrow body's contextual type is the arrow's
            // own contextual signature's return
            // (`getContextualReturnType`, `checker.go:29648` region).
            Node::ArrowFunction(arrow)
                if arrow.body.and_then(|b| tsr_ast::Node::from(b).node_id()) == Some(node)
                    && self.nodes.kind(node) != tsr_ast::SyntaxKind::Block =>
            {
                let signature = self.contextual_signature(parent)?;
                (signature.r#type != self.intrinsics.error).then_some(signature.r#type)
            }
            Node::ReturnStatement(_) => {
                let mut function = self.nodes.parent(parent)?;
                loop {
                    match self.nodes.kind(function) {
                        tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::FunctionExpression
                        | tsr_ast::SyntaxKind::ArrowFunction
                        | tsr_ast::SyntaxKind::MethodDeclaration => break,
                        tsr_ast::SyntaxKind::SourceFile => return None,
                        _ => function = self.nodes.parent(function)?,
                    }
                }
                let annotation = match self.node_map.get(function)? {
                    Node::FunctionDeclaration(f) => f.r#type,
                    Node::FunctionExpression(f) => f.r#type,
                    Node::ArrowFunction(f) => f.r#type,
                    Node::MethodDeclaration(f) => f.r#type,
                    _ => None,
                }?;
                Some(self.get_type_from_type_node(annotation))
            }
            _ => None,
        }
    }

    /// The type an object literal's property assignment is expected to have.
    ///
    /// ```text
    /// interface Handlers { onTick: (tick: string) => void }
    /// const handlers: Handlers = { onTick: value => value };
    /// //                                   ^^^^^ `string`, via `Handlers.onTick`
    /// ```
    ///
    /// Ported from `Checker.getContextualTypeForObjectLiteralElement`
    /// (`checker.go:29920`), which is two steps: the object literal's own
    /// contextual type, then the matching property of it.
    ///
    /// # Measured, and why this arm and not another
    ///
    /// 297 contextually typed function expressions in 98 files sit in an object
    /// literal member — the largest arm after the call argument, and the second
    /// largest of the twenty. But the population that matters is the one whose
    /// **prerequisite is met**, because this arm answers nothing unless the
    /// enclosing object literal already has a contextual type. Splitting the 297
    /// by what supplies the object literal's own context:
    ///
    /// | the object literal is… | count | reachable today |
    /// |---|---|---|
    /// | an argument to a generic callee | 82 | no — needs inference |
    /// | initialiser of a simply annotated declaration | 47 | **yes** |
    /// | initialiser of an unannotated declaration | 28 | no |
    /// | a member of another object literal | 23 | no — needs this arm to nest through an index signature |
    /// | an array/tuple element | 22 | no — needs tuple types |
    /// | initialiser of a **union**-annotated declaration | 20 | no — `getApparentTypeOfContextualType` |
    /// | an argument to a one-signature callee | 19 | **yes** |
    /// | parenthesised, in a block, a `return`, other | 56 | no |
    ///
    /// **66 in 33 files**, footprint 259 assertion lines, largest file 21%, top
    /// 10 files 61%. That is the same order as the annotated-variable arm's 75,
    /// and it costs less, because everything below already exists.
    ///
    /// # Not ported
    ///
    /// - **A computed property name.** Upstream checks the name expression and
    ///   uses it if it is usable as a property name (`checker.go:29934`); here a
    ///   `ComputedPropertyName` answers `None`. So do a numeric, bigint and
    ///   template-literal name: [`Checker::get_property_of_type`] is keyed by the
    ///   source text and the literal-to-name mapping is upstream's
    ///   `getLiteralTypeFromPropertyName`, which is not ported.
    /// - **The index-signature fallback** (`checker.go:29946`), which is what
    ///   upstream answers when no property matches. Without it a member whose
    ///   name is absent from the contextual type is a gap, and gapping it is why
    ///   the 23 nested-object-literal cases above are counted as unreachable
    ///   rather than as an untested claim.
    /// - **`ShorthandPropertyAssignment`**, which shares upstream's arm
    ///   (`checker.go:29376`) but cannot hold a function expression, so it could
    ///   never reach [`Checker::get_contextually_typed_parameter_type`].
    /// - **A `SpreadAssignment` or an object-literal method**, each a separate
    ///   upstream branch (`checker.go:29378`, `checker.go:29964`).
    fn contextual_type_for_object_literal_element(
        &mut self,
        element: NodeId,
        assignment: &'a PropertyAssignment<'a>,
    ) -> Option<TypeId> {
        // `if t := element.Type(); t != nil && !ast.IsObjectLiteralMethod(element)`
        // (`checker.go:29921`). A `PropertyAssignment` cannot be an object
        // literal method, so the second half of upstream's test is the pattern
        // match above rather than a check here.
        if let Some(annotation) = assignment.r#type {
            return Some(self.get_type_from_type_node(annotation));
        }
        // `c.hasBindableName(element)` (`checker.go:29927`) reduced to the names
        // `get_property_of_type` can be keyed by. See "Not ported" above.
        let name = match assignment.name {
            PropertyName::Identifier(name) => name.text,
            PropertyName::StringLiteral(name) => name.text,
            _ => return None,
        };
        // `objectLiteral := element.Parent` (`checker.go:29924`). No check that
        // the parent *is* an `ObjectLiteralExpression`: a `PropertyAssignment`
        // has no other possible parent in this AST, so a guard here could not be
        // reddened by any mutation and would read as evidence for a decision that
        // was never made. Same call, same reason, as the initialiser guard above.
        let object_literal = self.nodes.parent(element)?;
        let contextual = self.get_contextual_type(object_literal)?;
        // `getTypeOfPropertyOfContextualTypeEx` (`checker.go:29932`). Upstream
        // maps over a union here; this port does not, so a union-typed context
        // finds nothing and gaps — which is the 20 rows in the table above.
        let property = self.get_property_of_type(contextual, name)?;
        Some(self.get_type_of_symbol(property))
    }

    /// The type an expression is expected to have when it sits directly in the
    /// argument list of a call.
    ///
    /// Ported from `Checker.getContextualTypeForArgumentAtIndex`
    /// (`checker.go:29772`), collapsed to the one path that needs no argument
    /// checked — see the module documentation on the recursion this avoids.
    fn contextual_type_for_argument(
        &mut self,
        call: &'a CallExpression<'a>,
        argument: NodeId,
    ) -> Option<TypeId> {
        // `getEffectiveCallArguments` (`checker.go:29772`'s `argIndex` domain).
        // A spread anywhere before this argument makes the index meaningless, so
        // it is a gap rather than an off-by-one waiting to happen.
        if call.arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_))) {
            return None;
        }
        // `None` when the expression is the call's *callee* rather than one of
        // its arguments, which is the immediately invoked function expression
        // upstream handles at `checker.go:29463` from the argument expressions.
        let index = call.arguments.iter().position(|a| a.node_id() == Some(argument))?;

        let callee = call.expression?;
        let callee_type = self.check_expression(callee);
        if let Some(parameter) = self
            .single_call_signature(callee_type)
            .and_then(|s| s.parameters.into_iter().nth(index))
        {
            return Some(parameter.r#type);
        }
        // §70 (`checker-notes-narrow.md`): OVERLOADED/GENERIC callees whose
        // every candidate AGREES on the parameter's type at this index — the
        // agreement is what upstream's per-candidate contextual pass
        // converges to when the position's type mentions no type parameter
        // (`parenthesizedContexualTyping2`'s FuncType callbacks, 73 lines).
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data else {
            return None;
        };
        let candidates = self.get_signatures_of_symbol(symbol)?;
        let mut agreed: Option<TypeId> = None;
        for candidate in &candidates {
            let parameter = candidate.parameters.get(index)?;
            if parameter.rest || parameter.optional {
                return None;
            }
            // A parameter whose type MENTIONS the candidate's own type
            // parameters (by ID, walked two levels through signatures and
            // reference arguments — a TEXT test collided the callback's own
            // `<T>` with the candidate's and killed the wins) is not
            // position-stable — decline.
            if self.mentions_any_type_parameter(parameter.r#type, 2) {
                return None;
            }
            match agreed {
                None => agreed = Some(parameter.r#type),
                Some(t) if t == parameter.r#type => {}
                Some(_) => return None,
            }
        }
        agreed
    }

    /// The one call signature of `id`, or `None` if it has any other number of
    /// them or is generic.
    ///
    /// Ported from `Checker.getSignaturesOfType` (`checker.go:18959` via
    /// [`Checker::get_signatures_of_symbol`]) with the two reductions the module
    /// documentation justifies: an overload set is `None` because choosing needs
    /// arguments checked, and a generic signature is `None` because its
    /// parameter types would print as their own type parameters — `T`, not the
    /// type the caller supplied.
    /// §70's mention walk: whether `id` is, or transitively contains (to
    /// `depth` levels through signature parameters/returns and reference
    /// arguments), a type-parameter type. Conservative: ANY type-parameter
    /// mention declines, not just the candidate's own — a callback's inner
    /// generic re-binds its names and cannot leak the outer parameter.
    fn mentions_any_type_parameter(&mut self, id: TypeId, depth: u8) -> bool {
        if self.type_parameter_symbols.contains_key(&id) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&id).cloned()
            && arguments.iter().any(|&a| self.mentions_any_type_parameter(a, depth - 1))
        {
            return true;
        }
        if let Some(signatures) = self.signatures_of_type(id) {
            let signatures = signatures.clone();
            for signature in &signatures {
                if signature.type_parameters.is_empty()
                    && (signature
                        .parameters
                        .iter()
                        .any(|p| self.mentions_any_type_parameter(p.r#type, depth - 1))
                        || self.mentions_any_type_parameter(signature.r#type, depth - 1))
                {
                    return true;
                }
            }
        }
        false
    }

    fn single_call_signature(&mut self, id: TypeId) -> Option<Signature> {
        let TypeData::Anonymous { symbol, .. } = self.store.get(id).data else {
            return None;
        };
        match self.get_signatures_of_symbol(symbol)?.as_slice() {
            [signature] if signature.type_parameters.is_empty() => Some(signature.clone()),
            _ => None,
        }
    }
}

/// Whether `parameter` is the `this` parameter.
///
/// Ported from `ast.GetThisParameter` (`utilities.go`), which upstream reaches
/// on `checker.go:29489` to correct the parameter index. Here it only decides a
/// gap, so the identifier text is the whole test — a `this` parameter cannot be
/// a binding pattern.
fn is_this_parameter(parameter: &ParameterDeclaration<'_>) -> bool {
    matches!(parameter.name, Some(BindingName::Identifier(name)) if name.text == "this")
}
