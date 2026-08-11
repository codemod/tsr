//! `noImplicitAny` for **parameters** — TS7006 and TS7019.
//!
//! `reportImplicitAny` (`checker.go:18275`), reached from
//! `reportErrorsFromWidening` (`checker.go:20452`) when a declaration's widened
//! type still carries `ObjectFlagsContainsWideningType`.
//!
//! # Why this is a syntactic rule here and a widening rule upstream
//!
//! Upstream decides *"is this an implicit any"* from the type: the parameter has
//! no annotation, nothing supplied one, and what came out of widening still
//! contains the widening marker. This port has no widening marker, so the same
//! question is asked of the syntax — **and it can only be asked safely where the
//! answer cannot depend on a contextual type**, because contextual typing is the
//! one thing that supplies a parameter's type without an annotation and this
//! port's version of it is partial (`docs/architecture/checker-notes-*`, the
//! refused `ArrowFunction` row).
//!
//! So the rule fires only for parameters of declarations that **cannot** be
//! contextually typed: a `function` declaration, a class method, a constructor.
//! A function expression, an arrow, an object-literal method and an accessor are
//! each declined by construction, not by measurement — every one of them has a
//! route by which upstream supplies the type and this port does not.
//!
//! # The blast radius is the corpus — CORRECTED
//!
//! This section read *"like [`crate::unused`], nothing here fires unless the
//! case writes `@noImplicitAny` (or `@strict`), so a wrong answer can only reach
//! the cases that opted in."* **That was true of the harness and false of
//! upstream, and the harness was wrong.** `c.noImplicitAny =
//! GetStrictOptionValue(c.compilerOptions.NoImplicitAny)` (`checker.go:924`),
//! and `GetStrictOptionValue` (`core/compileroptions.go:294`) answers
//! `options.Strict != TSFalse` for an unset option — so **an unset
//! `noImplicitAny` is ON**. `checker-notes-diag2.md` §80.
//!
//! `noUnusedLocals` genuinely is opt-in (`unusedIsError` reads it as
//! `IsTrue()`), so the comparison to [`crate::unused`] does not transfer; the
//! two options are read by different upstream functions.
//!
//! The allow-list below was therefore tuned while nothing outside the opt-in
//! cases could reach it. It held when the option was turned on across the
//! corpus — 40 right lines for 3 wrong — with **one** arm wrong, the
//! `ReturnStatement` one, corrected in the same build.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::{check::has_modifier, checker::Checker};

impl Checker<'_, '_> {
    /// Does the enclosing class's constructor assign `this.<name>`? §796.
    fn constructor_assigns_this_member(&mut self, member: NodeId, text: &str) -> bool {
        let Some(class) = self.nodes.parent(member) else { return false };
        let members = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(class)) => class.members,
            Some(Node::ClassExpression(class)) => class.members,
            _ => return false,
        };
        members.iter().any(|each| {
            let tsr_ast::ClassElement::ConstructorDeclaration(constructor) = each else {
                return false;
            };
            constructor
                .body
                .and_then(|body| body.node_id())
                .is_some_and(|body| self.subtree_assigns_this_member(body, text, 0))
        })
    }

    /// Every parameter of one function-like declaration that is an implicit
    /// `any`.
    ///
    /// Called from the walk at the *declaration*, not at the parameter, because
    /// the decision is about the declaration's kind and asking it once per
    /// parameter would ask it N times.
    /// TS7008 — `Member '{0}' implicitly has an '{1}' type.`
    ///
    /// `reportImplicitAny`'s first case (`checker.go:18283`), which covers
    /// `PropertyDeclaration` and `PropertySignature`.
    ///
    /// Upstream reports when the **widened** type is implicitly `any`. That is
    /// decidable without widening for the shape this corpus uses: a property
    /// with **no annotation and no initialiser** is `any` under any inference
    /// this port could have. One with an initialiser has a type to widen and is
    /// declined. §271.
    ///
    /// Every guard is [`Checker::check_implicit_any_parameters`]' — including
    /// the blanket `.js` decline, which exists because JSDoc supplies types
    /// this port does not parse into one.
    pub(crate) fn check_implicit_any_member(&mut self, node: NodeId, ambient: bool) {
        // **`isPrivateWithinAmbient`, which the `ambient` guard does *not*
        // answer.** `ambient` is *is it ambient*; upstream exempts *ambient
        // **and** private*. §582 fixed the identical defect in the parameter
        // rule and this comment used to assert the guard was sufficient. §664.
        let is_private =
            self.node_map.get(node).and_then(crate::check::modifiers_of).is_some_and(|m| {
                tsr_ast::has_syntactic_modifier(m, tsr_ast::SyntaxKind::PrivateKeyword)
            });
        if !self.no_implicit_any || self.file_has_parse_errors || (ambient && is_private) {
            return;
        }
        if self.in_js_file(node) {
            return;
        }
        let Some(typed_member) = self.node_map.get(node) else { return };
        let (name, annotation, initializer) = match self.node_map.get(node) {
            Some(Node::PropertyDeclaration(property)) => {
                (property.name, property.r#type, property.initializer)
            }
            Some(Node::PropertySignatureDeclaration(property)) => {
                (property.name, property.r#type, property.initializer)
            }
            _ => return,
        };
        if annotation.is_some() || initializer.is_some() {
            return;
        }
        // A **private name** is a separate `PropertyName` variant, and
        // `class B { #prop; }` is TS7008 exactly as `class B { prop; }` is.
        // Upstream reaches it through the same arm, exempting only
        // `isPrivateWithinAmbient` — which the `ambient` guard above already
        // answers. §379.
        // A **string-literal** name is a member name. `'string_named';` in an
        // interface is unannotated exactly as `foo;` is, and upstream prints it
        // through `declarationNameToString` without caring which spelling it
        // took. §379 added the private-identifier arm; this one was never
        // there, and §800's removal of the wrong lines above it is what made
        // the absence legible. §801.
        let text = match name {
            tsr_ast::PropertyName::Identifier(name) => name.text,
            tsr_ast::PropertyName::PrivateIdentifier(name) => name.text,
            tsr_ast::PropertyName::StringLiteral(name) => name.text,
            _ => return,
        };
        // **A constructor assignment gives an *instance* member its type.**
        // `getTypeOfPropertyDeclaration` infers from `this.<name> = …`, and
        // `this.sideLength = sideLength` types the instance member while saying
        // nothing about `static sideLength` — §797's `staticVisibility2`, where
        // both exist and only the static one reports. §800.
        let is_static = crate::check::modifiers_of(typed_member)
            .is_some_and(|m| tsr_ast::has_syntactic_modifier(m, SyntaxKind::StaticKeyword));
        if !is_static && self.constructor_assigns_this_member(node, text) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::MEMBER_0_IMPLICITLY_HAS_AN_1_TYPE,
                span,
                [text.to_string(), "any".to_string()],
            ),
        );
    }

    pub(crate) fn check_implicit_any_parameters(&mut self, node: NodeId, ambient: bool) {
        // `isPrivateWithinAmbient` (`checker.go:3449`) — computed once for the
        // whole signature, as upstream computes it per declaration. §582.
        let private_within_ambient =
            self.node_map.get(node).and_then(crate::check::modifiers_of).is_some_and(|modifiers| {
                tsr_ast::has_syntactic_modifier(modifiers, tsr_ast::SyntaxKind::PrivateKeyword)
            });
        if !self.no_implicit_any || self.file_has_parse_errors {
            return;
        }
        if !self.parameters_cannot_be_contextually_typed(node) {
            return;
        }
        // `reportImplicitAny` (`checker.go:18276`) already declines a `.js` file
        // without `checkJs`; this declines **every** `.js` file, and the reason
        // is JSDoc. A `@param {string} x` supplies the type upstream reads and
        // this port does not parse into one, so a checked JS file reports an
        // implicit any on every annotated parameter. It was the *whole* of the
        // first measurement's wrong column — 28 lines, 12 cases, all `.js`, and
        // `typedefOnStatements` alone was 15 of them.
        if self.in_js_file(node) {
            return;
        }
        let parameters: Vec<NodeId> = self.implicit_any_candidates(node);
        for parameter in parameters {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                continue;
            };
            if declaration.r#type.is_some() || declaration.initializer.is_some() {
                continue;
            }
            // A binding pattern parameter reports TS7031 **per element**, at
            // the element's name rather than at the pattern —
            // `function f1([a], {b})` wants columns 14 and 19, which are `a`
            // and `b`. The same code from a *variable* declaration
            // (`var [a, b] = [undefined, null]`) is triggered by the
            // initialiser widening rather than by the syntax, and is a
            // different owner. §373.
            let name = match declaration.name {
                Some(tsr_ast::BindingName::Identifier(name)) => name,
                Some(tsr_ast::BindingName::BindingPattern(_)) if !ambient => {
                    self.report_binding_pattern_elements(parameter);
                    continue;
                }
                _ => continue,
            };
            // `this` is not a parameter for this purpose — it has no symbol and
            // upstream's widening never reaches it.
            if name.text == "this" {
                continue;
            }
            // "Report an implicit any error if there is no body … and node is
            // not a private method in an ambient context" is the *return type*
            // arm; a parameter in an ambient context still reports, and
            // `ParameterList4` is an ordinary declaration. The ambient flag is
            // threaded here only so a `.d.ts` stays silent, matching every other
            // rule in `crate::check`.
            // **`isPrivateWithinAmbient`, which the comment above names and the
            // old guard did not test.** A `.d.ts` stays silent; so does a
            // `private` member in an ambient context. A *public* method in a
            // `declare class` reports, and the walk-threaded `ambient` was
            // skipping it along with them. §582.
            if self.file_is_ambient || (ambient && private_within_ambient) {
                continue;
            }
            let rest = declaration.dot_dot_dot_token.is_some();
            let message = if rest {
                &messages::REST_PARAMETER_0_IMPLICITLY_HAS_AN_ANY_TYPE
            } else {
                &messages::PARAMETER_0_IMPLICITLY_HAS_AN_1_TYPE
            };
            let Some(file) = self.source_file_of_for_diagnostics(parameter) else { continue };
            // The position is the **parameter declaration**, modifiers included:
            // `ParameterList4.ts(1,12)` for `function F(public A)` is the
            // `public`, not the `A`.
            let span = self.error_span(parameter);
            let text = name.text.to_string();
            let diagnostic = if rest {
                Diagnostic::with_args(message, span, [text])
            } else {
                Diagnostic::with_args(message, span, [text, "any".to_string()])
            };
            self.report(file, diagnostic);
        }
    }

    /// Can this declaration's parameters get their types from a contextual
    /// signature?
    ///
    /// `getContextualSignatureForFunctionLikeDeclaration` (`checker.go:20464`)
    /// answers for the shapes that *can*; the shapes below are the ones for
    /// which it structurally cannot, and they are the whole of what this rule
    /// admits.
    ///
    /// A method written inside an **object literal** is contextually typed by
    /// the literal's contextual type, so it is excluded even though it shares a
    /// node kind with a class method. An accessor is excluded because a `set`
    /// accessor's parameter type comes from its `get` counterpart.
    fn parameters_cannot_be_contextually_typed(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            // §333 — a *signature*'s parameters are declarations, not
            // expressions, so nothing can supply them a contextual type. The
            // kinds this predicate knew all have **bodies**; the signatures do
            // not, and they answer the same way a function declaration does.
            Some(
                Node::FunctionDeclaration(_)
                | Node::ConstructorDeclaration(_)
                | Node::FunctionTypeNode(_)
                | Node::ConstructorTypeNode(_)
                | Node::CallSignatureDeclaration(_)
                | Node::ConstructSignatureDeclaration(_)
                | Node::MethodSignatureDeclaration(_),
            ) => true,
            Some(Node::MethodDeclaration(_)) => self.nodes.parent(node).is_some_and(|parent| {
                self.nodes.kind(parent) != SyntaxKind::ObjectLiteralExpression
            }),
            // A function expression or arrow **can** be contextually typed, so
            // it is admitted only in the two positions where nothing can supply
            // a signature: an initialiser with no annotation on the variable,
            // and a bare expression statement. Every other position — an
            // argument, an annotated declaration, a property assignment, a
            // return, a JSX attribute, an `as` — has a contextual type this port
            // computes only partially, and that partiality is a *wrong* TS7006.
            Some(Node::FunctionExpression(_) | Node::ArrowFunction(_)) => {
                match self.nodes.parent(node).and_then(|parent| self.node_map.get(parent)) {
                    Some(Node::VariableDeclaration(declaration)) => declaration.r#type.is_none(),
                    Some(Node::ExpressionStatement(_)) => true,
                    // A `return` was admitted unconditionally on the theory
                    // that nothing there supplies a signature. **The enclosing
                    // function's written return-type annotation does** —
                    // `function <T>(…): React.StatelessComponent<T> { return
                    // (props) => … }` contextually types `props`, and
                    // `tsxGenericAttributesType1` was 3 of §80's wrong lines.
                    // Only a return inside a function with no return annotation
                    // is uncontextual.
                    Some(Node::ReturnStatement(_)) => {
                        !self.enclosing_function_has_a_return_annotation(node)
                    }
                    Some(Node::PropertyDeclaration(property)) => property.r#type.is_none(),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// TS7010 — `'{0}', which lacks return-type annotation, implicitly has an
    /// '{1}' return type.`
    ///
    /// `checkFunctionOrMethodDeclaration` (`checker.go:3446`), the arm the
    /// parameter loop above already quotes:
    ///
    /// ```go
    /// if node.Type() == nil {
    ///     // Report an implicit any error if there is no body, no explicit return
    ///     // type, and node is not a private method in an ambient context
    ///     if ast.NodeIsMissing(body) && !isPrivateWithinAmbient(node) {
    ///         c.reportImplicitAny(node, c.anyType, WideningKindNormal)
    ///     }
    /// }
    /// ```
    ///
    /// `reportImplicitAny`'s function arm (`checker.go:18320`) then picks
    /// `X_0_which_lacks_return_type_annotation_implicitly_has_an_1_return_type`
    /// when `noImplicitAny` holds and the declaration has a name.
    ///
    /// # No contextual-typing question, unlike the arm beside it
    ///
    /// [`Checker::parameters_cannot_be_contextually_typed`] is a fenced
    /// allow-list because a contextual signature can supply a *parameter's*
    /// type. A **bodiless** declaration has no inferred return type for anything
    /// to supply: upstream reaches `reportImplicitAny` at this site with no
    /// `shouldReportErrorsFromWideningWithContextualSignature` in the path. This
    /// arm is simpler than its neighbour, which is worth saying because the
    /// neighbouring code looks like it should be copied.
    ///
    /// # The ambient gate is the OPPOSITE way round
    ///
    /// The parameter loop skips everything ambient. This skips only
    /// `isPrivateWithinAmbient` — a **private** member in an ambient context —
    /// so `declare function f();` in a `.d.ts` does report. Both are upstream's
    /// and conflating them is the obvious mistake here
    /// (`checker-notes-diag2.md` §81).
    /// TS7013 / TS7011 — a **signature** with no return-type annotation.
    ///
    /// `checkSignatureDeclaration` (`checker.go:2749`) selects on *kind*, not on
    /// namelessness: a construct signature and a call signature each have their
    /// own code, in their own function, and neither is the function-expression
    /// form `check_implicit_any_return`'s closing comment names. §438.
    pub(crate) fn check_implicit_any_signature_return(&mut self, node: NodeId, ambient: bool) {
        if ambient || !self.no_implicit_any || self.file_has_parse_errors {
            return;
        }
        if self.in_js_file(node) {
            return;
        }
        let (annotation, message) = match self.node_map.get(node) {
            Some(Node::ConstructSignatureDeclaration(signature)) => (
                signature.r#type,
                &messages::CONSTRUCT_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_TYPE,
            ),
            Some(Node::CallSignatureDeclaration(signature)) => (
                signature.r#type,
                &messages::CALL_SIGNATURE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_ANY_RETURN_TYPE,
            ),
            _ => return,
        };
        if annotation.is_some() {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(message, span));
    }

    pub(crate) fn check_implicit_any_return(&mut self, node: NodeId, ambient: bool) {
        if !self.no_implicit_any || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let private_name = |name: tsr_ast::PropertyName<'_>| {
            matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_))
        };
        let (annotation, body_is_missing, modifiers, name, name_is_private_identifier) =
            match self.node_map.get(node) {
                Some(Node::FunctionDeclaration(n)) => (
                    n.r#type,
                    n.body.is_none(),
                    n.modifiers,
                    n.name.map(|name| name.text.to_string()),
                    false,
                ),
                Some(Node::MethodDeclaration(n)) => (
                    n.r#type,
                    n.body.is_none(),
                    n.modifiers,
                    crate::check::declaration_name_to_string(n.name),
                    private_name(n.name),
                ),
                // A `MethodSignatureDeclaration` has no body by construction, which
                // is why it has no `body` field to read. Upstream reaches this site
                // for it through `checkMethodDeclaration` (`checker.go:2806`), whose
                // own body guards the declaration-only arms with
                // `ast.IsMethodDeclaration(node)` and lets this one through.
                Some(Node::MethodSignatureDeclaration(n)) => (
                    n.r#type,
                    true,
                    n.modifiers,
                    crate::check::declaration_name_to_string(n.name),
                    private_name(n.name),
                ),
                _ => return,
            };
        if annotation.is_some() || !body_is_missing {
            return;
        }
        // `isPrivateWithinAmbient` (`utilities.go:343`) is
        // `(HasModifier(Private) || IsPrivateIdentifierClassElementDeclaration)
        //  && node.Flags&NodeFlagsAmbient != 0`, and **both halves are wider
        // than they first read**:
        //
        // - a `#name` class element is private without the keyword. `declare
        //   #whatMethod()` was this rule's only wrong line (§81,
        //   `privateNamesIncompatibleModifiers`);
        // - `NodeFlagsAmbient` is set by the parser for anything under a
        //   `declare`, including the member's own. This port's parser never sets
        //   that flag (it is one of the three declared-and-unset ones), so the
        //   walk threads an `ambient` bool — which the `MethodDeclaration` arm
        //   does not widen for a member-level `declare`. Reading the modifier
        //   here is what closes that gap.
        //
        // `declare function f();` is not private and does report, which is the
        // whole reason this gate is not the parameter loop's blanket
        // `if ambient { continue }`.
        let is_ambient = ambient || has_modifier(modifiers, SyntaxKind::DeclareKeyword);
        let is_private =
            has_modifier(modifiers, SyntaxKind::PrivateKeyword) || name_is_private_identifier;
        if is_ambient && is_private {
            return;
        }
        // `declaration.Name() == nil` takes a different message entirely
        // (TS7011, the function-expression form), which is not this rule.
        let Some(name) = name else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        // `errorOrSuggestion(c.noImplicitAny, declaration, …)` errors on the
        // **declaration**, and `GetErrorRangeForNode` narrows a named
        // function-like to its name: `FunctionDeclaration3.ts(1,10)` for
        // `function foo();` is the `foo`.
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_1_RETURN_TYPE,
                span,
                [name, "any".to_string()],
            ),
        );
    }

    /// Does the function-like enclosing this node write a return-type
    /// annotation?
    ///
    /// `getContextualReturnType` (`checker.go:20315`) reads exactly that
    /// annotation, so its presence is what makes a `return`'s expression a
    /// contextually typed position — see
    /// [`Checker::parameters_cannot_be_contextually_typed`]'s `ReturnStatement`
    /// arm.
    fn enclosing_function_has_a_return_annotation(&self, node: NodeId) -> bool {
        self.nodes
            .ancestors(node)
            .find_map(|ancestor| match self.node_map.get(ancestor) {
                Some(Node::FunctionDeclaration(n)) => Some(n.r#type.is_some()),
                Some(Node::FunctionExpression(n)) => Some(n.r#type.is_some()),
                Some(Node::ArrowFunction(n)) => Some(n.r#type.is_some()),
                Some(Node::MethodDeclaration(n)) => Some(n.r#type.is_some()),
                Some(Node::GetAccessorDeclaration(n)) => Some(n.r#type.is_some()),
                _ => None,
            })
            .unwrap_or(false)
    }

    /// TS7031 for a `var`/`let`/`const` whose name is a binding pattern. §730.
    pub(crate) fn check_implicit_any_binding_pattern(&mut self, node: NodeId) {
        if !self.no_implicit_any || self.file_has_parse_errors {
            return;
        }
        self.report_binding_pattern_elements(node);
    }

    /// One TS7031 per element of a parameter's binding pattern.
    ///
    /// `checkVariableLikeDeclaration`'s pattern arm — the error node is each
    /// element's **name**, and a nested pattern recurses. An element with its
    /// own initialiser is not implicitly `any`. §373.
    fn report_binding_pattern_elements(&mut self, parameter: NodeId) {
        let elements = match self.node_map.get(parameter) {
            Some(Node::ParameterDeclaration(declaration)) => match declaration.name {
                Some(tsr_ast::BindingName::BindingPattern(pattern)) => pattern.elements,
                _ => return,
            },
            // **A `var` whose name is a binding pattern is implicitly `any`
            // when nothing supplies its type**, and *nothing supplies it* is a
            // fact about the declaration's **parent**, not about which of its
            // fields are empty: a `for…of` head has neither an initializer nor
            // an annotation and takes its type from the iterable. §729 measured
            // the field-shaped bound at 39 wrong lines, every one a loop head.
            // §730.
            Some(Node::VariableDeclaration(declaration)) => {
                if declaration.initializer.is_some() || declaration.r#type.is_some() {
                    return;
                }
                let in_a_statement = self
                    .nodes
                    .parent(parameter)
                    .and_then(|list| self.nodes.parent(list))
                    .is_some_and(|owner| {
                        self.nodes.kind(owner) == tsr_ast::SyntaxKind::VariableStatement
                    });
                if !in_a_statement {
                    return;
                }
                match declaration.name {
                    Some(tsr_ast::BindingName::BindingPattern(pattern)) => pattern.elements,
                    _ => return,
                }
            }
            Some(Node::BindingElement(element)) => match element.name {
                Some(tsr_ast::BindingName::BindingPattern(pattern)) => pattern.elements,
                _ => return,
            },
            _ => return,
        };
        for element in elements {
            let Some(id) = element.node_id else { continue };
            if element.initializer.is_some() {
                continue;
            }
            match element.name {
                Some(tsr_ast::BindingName::Identifier(name)) => {
                    let Some(name_id) = name.node_id else { continue };
                    let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
                    let span = self.error_span(name_id);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::BINDING_ELEMENT_0_IMPLICITLY_HAS_AN_1_TYPE,
                            span,
                            [name.text.to_string(), "any".to_string()],
                        ),
                    );
                }
                Some(tsr_ast::BindingName::BindingPattern(_)) => {
                    self.report_binding_pattern_elements(id);
                }
                None => {}
            }
        }
    }

    /// The parameter nodes of a declaration this rule admits.
    fn implicit_any_candidates(&self, node: NodeId) -> Vec<NodeId> {
        let collect = |parameters: &[&tsr_ast::ParameterDeclaration<'_>]| -> Vec<NodeId> {
            parameters.iter().filter_map(|parameter| parameter.node_id).collect()
        };
        match self.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => collect(n.parameters),
            Some(Node::MethodDeclaration(n)) => collect(n.parameters),
            Some(Node::ConstructorDeclaration(n)) => collect(n.parameters),
            Some(Node::FunctionExpression(n)) => collect(n.parameters),
            Some(Node::ArrowFunction(n)) => collect(n.parameters),
            // §333 — the five above all have bodies; a parameter in a
            // *signature* is implicitly `any` exactly as one in a declaration
            // is. `ParameterList6` is `constructor(C: (public A) => any)`.
            Some(Node::FunctionTypeNode(n)) => collect(n.parameters),
            Some(Node::ConstructorTypeNode(n)) => collect(n.parameters),
            Some(Node::CallSignatureDeclaration(n)) => collect(n.parameters),
            Some(Node::ConstructSignatureDeclaration(n)) => collect(n.parameters),
            Some(Node::MethodSignatureDeclaration(n)) => collect(n.parameters),
            _ => Vec::new(),
        }
    }

    /// TS7005 — `Variable '{0}' implicitly has an '{1}' type.`
    ///
    /// The variable arm of `reportImplicitAny`. Restricted to declarations
    /// where the **evolving any** cannot apply: a `const`, which can never be
    /// assigned again, or an ambient declaration, which has no control flow to
    /// evolve through. `let x;` in an ordinary file is silent upstream and the
    /// flow analysis that decides so is not ported.
    ///
    /// `docs/architecture/checker-notes-diag2.md` §978.
    pub(crate) fn check_implicit_any_variable(&mut self, node: NodeId, ambient: bool) {
        if !self.no_implicit_any || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(node) else { return };
        if variable.r#type.is_some() || variable.initializer.is_some() {
            return;
        }
        // Falsifier 3: a binding pattern has its own reporter.
        let Some(tsr_ast::BindingName::Identifier(name)) = variable.name else { return };
        let Some(name_id) = name.node_id else { return };
        // **A `catch (e)` binding is not an implicit-any site.** It is a
        // `VariableDeclaration` with neither annotation nor initializer, and in
        // a `.d.ts` it satisfied the ambient arm — `parserTryStatement1.d` was
        // §978's one wrong line. §979.
        if self
            .nodes
            .parent(node)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::CatchClause)
        {
            return;
        }
        // Falsifier 2: `for (const x of …)` is bound by the iteration.
        if self.nodes.parent(node).and_then(|list| self.nodes.parent(list)).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
            )
        }) {
            return;
        }
        let is_const = self
            .nodes
            .parent(node)
            .is_some_and(|list| self.nodes.flags(list).contains(tsr_ast::NodeFlags::CONST));
        if !is_const && !ambient && !self.file_is_ambient {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.nodes.span(name_id);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::VARIABLE_0_IMPLICITLY_HAS_AN_1_TYPE,
                span,
                [name.text.to_string(), "any".to_string()],
            ),
        );
    }
}
