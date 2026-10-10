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
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{check::has_modifier, checker::Checker, contextual::ContextualSignature};

/// See [`Checker::implicit_any_parameter_owner`].
enum ParameterOwner {
    /// Parameters are never contextually typed.
    Uncontextual,
    /// Each parameter asks the function's context.
    Contextual,
    /// Not a parameter owner this rule walks.
    Other,
}

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

    /// Does a class static block assign `this.<name>`?
    ///
    /// `getFlowTypeInStaticBlocks` (`flow.go:2488`) — the static twin of
    /// [`Checker::constructor_assigns_this_member`], reached from the static
    /// arm of `getTypeForVariableLikeDeclaration` (`checker.go:16768`). In a
    /// static block `this` is the class, so `static x; static { this.x = 1 }`
    /// types `x` (`classStaticBlockUseBeforeDef1`).
    fn static_block_assigns_this_member(&mut self, member: NodeId, text: &str) -> bool {
        let Some(class) = self.nodes.parent(member) else { return false };
        let members = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(class)) => class.members,
            Some(Node::ClassExpression(class)) => class.members,
            _ => return false,
        };
        members.iter().any(|each| {
            let tsr_ast::ClassElement::ClassStaticBlockDeclaration(block) = each else {
                return false;
            };
            block.node_id.is_some_and(|block| self.subtree_assigns_this_member(block, text, 0))
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
    /// Every guard is [`Checker::check_implicit_any_parameters`]'. In JS the
    /// annotation is also the property's own reparsed `@type`.
    pub(crate) fn check_implicit_any_member(&mut self, node: NodeId, ambient: bool) {
        // **`isPrivateWithinAmbient`, which the `ambient` guard does *not*
        // answer.** `ambient` is *is it ambient*; upstream exempts *ambient
        // **and** private*. §582 fixed the identical defect in the parameter
        // rule and this comment used to assert the guard was sufficient. §664.
        // `IsPrivateIdentifierClassElementDeclaration` is the other half of
        // `isPrivateWithinAmbient` (`utilities.go:343`): `declare class A {
        // #prop; }` is private without the keyword
        // (`privateNameAmbientNoImplicitAny`).
        let is_private =
            self.node_map.get(node).and_then(crate::check::modifiers_of).is_some_and(|m| {
                tsr_ast::has_syntactic_modifier(m, tsr_ast::SyntaxKind::PrivateKeyword)
            }) || matches!(
                self.node_map.get(node),
                Some(Node::PropertyDeclaration(property))
                    if matches!(property.name, tsr_ast::PropertyName::PrivateIdentifier(_))
            );
        if !self.no_implicit_any || (ambient && is_private) {
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
        // In JS `node.Type()` is the reparsed `@type` (`reparseHosted`).
        if self.jsdoc_self_hosted_type(node).is_some() {
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
        if is_static && self.static_block_assigns_this_member(node, text) {
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
        if !self.no_implicit_any {
            return;
        }
        let contextual = match self.implicit_any_parameter_owner(node) {
            ParameterOwner::Uncontextual => false,
            ParameterOwner::Contextual => true,
            ParameterOwner::Other => return,
        };
        // `reportImplicitAny` (`checker.go:18276`) declines a `.js` file
        // without `checkJs`; the program drops those files' semantic
        // diagnostics already (`tsr_compiler::program_diagnostics`), so a JS
        // file is checked here like a TypeScript one. Its parameters' JSDoc
        // types are the reparsed `node.Type()` and its `@type` the full
        // signature, both read below (r6-jsdoc2 §2).
        let parameters: Vec<NodeId> = self.implicit_any_candidates(node);
        for (index, parameter) in parameters.into_iter().enumerate() {
            let Some(Node::ParameterDeclaration(declaration)) = self.node_map.get(parameter) else {
                continue;
            };
            if declaration.r#type.is_some() || declaration.initializer.is_some() {
                continue;
            }
            // `tryGetTypeFromTypeNode` over the reparsed `@param`/`@type`, then
            // `getParameterTypeOfFullSignature` (`checker.go:16726`).
            if self.file_is_js
                && (self.jsdoc_reparsed_parameter_type(parameter).is_some()
                    || self.jsdoc_full_signature_parameter_type(parameter).is_some()
                    || self.jsdoc_full_signature_undecided(node))
            {
                continue;
            }
            if contextual && !self.contextual_parameter_type_is_absent(node, parameter) {
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
            let Some(file) = self.source_file_of_for_diagnostics(parameter) else { continue };
            // The position is the **parameter declaration**, modifiers included:
            // `ParameterList4.ts(1,12)` for `function F(public A)` is the
            // `public`, not the `A`.
            let span = self.error_span(parameter);
            if self.parameter_name_is_probably_a_type(node, parameter, name.text) {
                // TS7051 — `reportImplicitAny`'s parameter arm
                // (`checker.go:18290`): the name is spelled like a type, so the
                // author most likely wrote `(string) => void` meaning a type.
                let type_name = format!("{}{}", name.text, if rest { "[]" } else { "" });
                self.report(
                    file,
                    Diagnostic::with_args(
                        &messages::PARAMETER_HAS_A_NAME_BUT_NO_TYPE_DID_YOU_MEAN_0_COLON_1,
                        span,
                        [format!("arg{index}"), type_name],
                    ),
                );
                continue;
            }
            let message = if rest {
                &messages::REST_PARAMETER_0_IMPLICITLY_HAS_AN_ANY_TYPE
            } else {
                &messages::PARAMETER_0_IMPLICITLY_HAS_AN_1_TYPE
            };
            let text = name.text.to_string();
            let diagnostic = if rest {
                Diagnostic::with_args(message, span, [text])
            } else {
                Diagnostic::with_args(message, span, [text, "any".to_string()])
            };
            self.report(file, diagnostic);
        }
    }

    /// The TS7051 condition of `reportImplicitAny`'s parameter arm
    /// (`checker.go:18290`): the parameter belongs to a call signature, a
    /// method signature or a function type, and its name is either a
    /// type keyword (`ast.IsTypeNodeKind(scanner.IdentifierToKeywordKind(name))`)
    /// or resolves with the `Type` meaning from the parameter
    /// (`resolveName(declaration, name, SymbolFlagsType, …, excludeGlobals=false)`).
    ///
    /// A construct signature and a constructor type are deliberately absent:
    /// upstream's kind test names exactly the three kinds above.
    fn parameter_name_is_probably_a_type(
        &self,
        owner: NodeId,
        parameter: NodeId,
        name: &str,
    ) -> bool {
        if !matches!(
            self.nodes.kind(owner),
            SyntaxKind::CallSignature | SyntaxKind::MethodSignature | SyntaxKind::FunctionType
        ) {
            return false;
        }
        // `IsTypeNodeKind` also admits the type-node range and the JSDoc type
        // kinds; `IdentifierToKeywordKind` can only answer a keyword, so the
        // keyword list is the whole of what can match here.
        let keyword_is_a_type = tsr_scanner::keyword_kind(name).is_some_and(|kind| {
            matches!(
                kind,
                SyntaxKind::AnyKeyword
                    | SyntaxKind::UnknownKeyword
                    | SyntaxKind::NumberKeyword
                    | SyntaxKind::BigIntKeyword
                    | SyntaxKind::ObjectKeyword
                    | SyntaxKind::BooleanKeyword
                    | SyntaxKind::StringKeyword
                    | SyntaxKind::SymbolKeyword
                    | SyntaxKind::VoidKeyword
                    | SyntaxKind::UndefinedKeyword
                    | SyntaxKind::NeverKeyword
                    | SyntaxKind::IntrinsicKeyword
            )
        });
        keyword_is_a_type
            || self
                .binder
                .resolve_name(self.nodes, self.node_map, parameter, name, SymbolFlags::TYPE)
                .is_some()
    }

    /// Which road a declaration's unannotated parameters take to their type.
    ///
    /// `getContextuallyTypedParameterType` (`checker.go:29458`) answers nil
    /// outright unless the function is
    /// `isContextSensitiveFunctionOrObjectLiteralMethod` — a function
    /// expression, an arrow, or an object-literal method. Every other
    /// function-like's unannotated parameter falls straight to the implicit
    /// `any` of `widenTypeForVariableLikeDeclaration` (`checker.go:18264`).
    /// The three context-sensitive forms ask the context, per parameter:
    /// [`Checker::contextual_parameter_type_is_absent`].
    fn implicit_any_parameter_owner(&self, node: NodeId) -> ParameterOwner {
        match self.node_map.get(node) {
            Some(
                Node::FunctionDeclaration(_)
                | Node::ConstructorDeclaration(_)
                | Node::FunctionTypeNode(_)
                | Node::ConstructorTypeNode(_)
                | Node::CallSignatureDeclaration(_)
                | Node::ConstructSignatureDeclaration(_)
                | Node::MethodSignatureDeclaration(_),
            ) => ParameterOwner::Uncontextual,
            Some(Node::MethodDeclaration(_)) => {
                if self.nodes.parent(node).is_some_and(|parent| {
                    self.nodes.kind(parent) == SyntaxKind::ObjectLiteralExpression
                }) {
                    ParameterOwner::Contextual
                } else {
                    ParameterOwner::Uncontextual
                }
            }
            Some(Node::FunctionExpression(_) | Node::ArrowFunction(_)) => {
                ParameterOwner::Contextual
            }
            _ => ParameterOwner::Other,
        }
    }

    /// Does `getTypeForVariableLikeDeclaration` (`checker.go:16652`) answer nil
    /// for this unannotated, initializer-less parameter of a context-sensitive
    /// function — i.e. does `getContextuallyTypedParameterType`
    /// (`checker.go:29458`) answer nil?
    ///
    /// Upstream's nil has three sources, and each is asked here of the data
    /// upstream reads, never of the syntax around the function:
    ///
    /// 1. `getContextualSignature` is nil because the function has **no
    ///    contextual type** — [`Checker::has_no_contextual_type`], the walk
    ///    over `getContextualType`'s nil-answering arms;
    /// 2. it is nil because the contextual type has **no usable signature**
    ///    (none, or a union whose members' signatures differ) —
    ///    [`ContextualSignature::Absent`];
    /// 3. the signature is present but **too short**: `tryGetTypeAtPosition`
    ///    answers nil past the last parameter when there is no rest.
    ///
    /// A port answer of "unknown" (the contextual machinery returns its outer
    /// `None`) is not upstream's nil and reports nothing. That decline is a
    /// gap in `crate::contextual`, recorded in
    /// `docs/parity/notes/implicit-any-widening.md` §3, not a judgement that
    /// the parameter is typed.
    fn contextual_parameter_type_is_absent(&mut self, function: NodeId, parameter: NodeId) -> bool {
        // The IIFE arm (`checker.go:29463`) answers from the call's arguments
        // before any contextual signature is consulted; whatever the port
        // answers there, it is not `getContextualSignature`'s nil.
        if self.immediately_invoked_call(function).is_some() {
            return false;
        }
        // Asked FIRST, in upstream's order: with no contextual type,
        // `getContextualSignature` is nil and so is the parameter's contextual
        // type, whatever this port's contextual machinery would answer.
        if self.has_no_reparsed_contextual_type(function) {
            return !self.returned_from_an_iife(function);
        }
        if self.retained_return_position_report(function) {
            return true;
        }
        // getContextuallyTypedParameterType's `getContextualSignature(func)`:
        // nil (`Absent`) leaves the parameter implicitly `any`.
        let signature = match self.contextual_signature_result(function) {
            Some(ContextualSignature::Present(signature)) => signature,
            Some(ContextualSignature::Absent) => {
                return !self.within_generic_call_argument(function);
            }
            None => return false,
        };
        // `slices.Index(fn.Parameters(), parameter)`, less one for a `this`
        // parameter (`checker.go:29489`).
        let parameters = self.implicit_any_candidates(function);
        let Some(mut index) = parameters.iter().position(|&each| each == parameter) else {
            return false;
        };
        if parameters.first().is_some_and(|&first| self.is_this_parameter_node(first)) {
            index -= 1;
        }
        // An own rest parameter reads `getRestTypeAtPosition`, which always
        // answers.
        let own_rest = matches!(
            self.node_map.get(parameter),
            Some(Node::ParameterDeclaration(declaration)) if declaration.dot_dot_dot_token.is_some()
        );
        !own_rest && self.signature_type_at_position(&signature, index).is_none()
    }

    /// Is `function` inside an argument of a call or `new` whose callee has a
    /// generic signature?
    ///
    /// A decline of [`ContextualSignature::Absent`] there, not an upstream
    /// branch: such a function's contextual type passes through
    /// `instantiateContextualType` with the call's inference mapper
    /// (`checker.go:30817`), which this port applies only in part (a
    /// defaulted type parameter's conditional parameter type,
    /// `contextualSignatureConditionalTypeInstantiationUsingDefault`, keeps
    /// both branches), so its `Absent` is not upstream's nil. The walk stops
    /// at the first enclosing call; syntax plus the callee's type.
    fn within_generic_call_argument(&mut self, function: NodeId) -> bool {
        let mut child = function;
        for ancestor in self.nodes.ancestors(function).collect::<Vec<_>>() {
            let (callee, arguments) = match self.node_map.get(ancestor) {
                Some(Node::CallExpression(call)) => (call.expression, call.arguments),
                Some(Node::NewExpression(call)) => (call.expression, call.arguments),
                _ => {
                    child = ancestor;
                    continue;
                }
            };
            if !arguments.iter().any(|argument| argument.node_id() == Some(child)) {
                return false;
            }
            let Some(callee) = callee else { return false };
            let callee_type = self.check_expression(callee);
            let kind = if self.nodes.kind(ancestor) == SyntaxKind::NewExpression {
                crate::signatures::SignatureKind::Construct
            } else {
                crate::signatures::SignatureKind::Call
            };
            return self.signatures_of_type_kind(callee_type, kind).is_some_and(|signatures| {
                signatures.iter().any(|signature| !signature.type_parameters.is_empty())
            });
        }
        false
    }

    /// Is `function` inside the return expression of an immediately invoked
    /// function?
    ///
    /// `getContextualReturnType` (`checker.go:29665`) ends with
    /// `if iife != nil { return c.getContextualType(iife, contextFlags) }`:
    /// a `return` inside an IIFE is contextually typed by the IIFE call's own
    /// context. [`Checker::has_no_contextual_type`]'s `ReturnStatement` arm
    /// stops at the owner's position instead (the owner is the call's callee,
    /// which has no context), so it answers "no context" where upstream may
    /// have one — `contextualReturnTypeOfIIFE3`, where
    /// `app.foo.bar = (function () { return { someFun(arg) {} }; })()` types
    /// `arg` from `app.foo.bar`. Until that arm is ported there, the absence
    /// proof is declined for every function under a return of an IIFE. The
    /// test is wider than the climb (it does not stop where the climb would),
    /// which can only withhold a report, never add one.
    fn returned_from_an_iife(&self, function: NodeId) -> bool {
        self.nodes.ancestors(function).any(|ancestor| {
            self.nodes.kind(ancestor) == SyntaxKind::ReturnStatement
                && self
                    .containing_function(ancestor)
                    .is_some_and(|owner| self.immediately_invoked_call(owner).is_some())
        })
    }

    /// **Retained, not ported** — the `ReturnStatement` arm of the allow-list
    /// this rule used before it asked the context (§3).
    ///
    /// It reports a function returned from a function with no return-type
    /// annotation. Upstream's reason is different: the owner's contextual
    /// signature yields no contextual signature for the returned function. In
    /// `subtypeReductionWithAnyFunctionType` (`return x => x.length > 0` inside
    /// the argument of `useMemo<T>(func: () => T)`) tsgo has none, while
    /// `get_contextually_typed_parameter_type` answers `any` — a producer
    /// defect in `crate::contextual` (tsr-2zk.31), and dropping this arm loses
    /// that RIGHT case. The arm stays until that producer is fixed; it is
    /// also what keeps five lane cases' TS7006 extra (§3 lists them).
    ///
    /// A JS `return` whose comment's `@type` is reparsed into a cast
    /// (`parser/reparser.go:378`) is the function's parent upstream, not the
    /// `ReturnStatement`, so the arm does not apply to it.
    fn retained_return_position_report(&self, function: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(function) else { return false };
        self.nodes.kind(parent) == SyntaxKind::ReturnStatement
            && !(self.file_is_js && self.jsdoc_reparse_gives_context(function))
            && !self
                .nodes
                .ancestors(function)
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

    fn is_this_parameter_node(&self, parameter: NodeId) -> bool {
        matches!(
            self.node_map.get(parameter),
            Some(Node::ParameterDeclaration(declaration))
                if matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        )
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
    /// [`Checker::contextual_parameter_type_is_absent`] asks the context
    /// because a contextual signature can supply a *parameter's* type. A **bodiless** declaration has no inferred return type for anything
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
        if ambient || !self.no_implicit_any {
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
        if !self.no_implicit_any {
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

    /// TS7031 for a `var`/`let`/`const` whose name is a binding pattern. §730.
    pub(crate) fn check_implicit_any_binding_pattern(&mut self, node: NodeId) {
        if !self.no_implicit_any {
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
        if !self.no_implicit_any {
            return;
        }
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(node) else { return };
        if variable.r#type.is_some() || variable.initializer.is_some() {
            return;
        }
        // In JS `node.Type()` is the reparsed `@type` (`reparseHosted`).
        if self.jsdoc_type_annotation(node).is_some() {
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

    /// TS7009 — `'new' expression, whose target lacks a construct signature,
    /// implicitly has an 'any' type.`
    ///
    /// `checkCallExpression`'s new-expression arm (`checker.go:8334`): the
    /// signature `resolveNewExpression` (`checker.go:8575`) resolved is a
    /// call signature — the callee's apparent type has no construct
    /// signatures but has call signatures — so the result is `any`, reported
    /// under `noImplicitAny`. Every signature that call-signature road can
    /// resolve has a declaration upstream, so the declaration test reduces to
    /// the signature lists.
    ///
    /// Declines what this port cannot certify: a callee needing
    /// `checkNonNullExpression`'s narrowing (nullable, `unknown`, `void`), an
    /// `any` or error apparent type (the untyped and error roads report
    /// nothing here), and an undecided signature list. A JS constructor
    /// function has no construct signature in tsgo (no `isJSConstructor`
    /// arm at the pinned commit), so `new F()` of one reports here as in TS.
    pub(crate) fn check_implicit_any_new_expression(&mut self, node: NodeId) {
        use crate::{flags::TypeFlags, signatures::SignatureKind};
        if !self.no_implicit_any || self.file_has_parse_errors {
            return;
        }
        let Some(Node::NewExpression(new)) = self.node_map.get(node) else { return };
        let Some(callee) = new.expression else { return };
        let callee_type = self.check_expression(callee);
        if self.is_error(callee_type)
            || self
                .store
                .get(callee_type)
                .flags
                .intersects(TypeFlags::NULLABLE | TypeFlags::UNKNOWN | TypeFlags::VOID)
        {
            return;
        }
        let apparent = self.apparent_type(callee_type);
        if self.is_error(apparent) || self.store.get(apparent).flags.intersects(TypeFlags::ANY) {
            return;
        }
        let Some(construct) =
            self.signature_shapes_of_type_kind(apparent, SignatureKind::Construct)
        else {
            return;
        };
        if !construct.is_empty() {
            return;
        }
        let Some(call) = self.signature_shapes_of_type_kind(apparent, SignatureKind::Call) else {
            return;
        };
        if call.is_empty() {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::NEW_EXPRESSION_WHOSE_TARGET_LACKS_A_CONSTRUCT_SIGNATURE_IMPLICITLY_HAS_AN_ANY_TYPE,
                span,
            ),
        );
    }

    /// TS7057 — `'yield' expression implicitly results in an 'any' type
    /// because its containing generator lacks a return-type annotation.`
    ///
    /// `checkYieldExpression`'s last arm (`checker.go:11005`): a non-`yield*`
    /// yield in a generator with no return-type annotation and no contextual
    /// NEXT iteration type (`getContextualIterationType`, the same
    /// [`Checker::get_contextual_iteration_type`] `check_yield_expression`
    /// answers from) is `anyType`, reported under `noImplicitAny` unless the
    /// result is unused (`expressionResultIsUnused`, `utilities.go:1159`) or
    /// the yield's own contextual type is present and not `any`. Where this
    /// port cannot finish either contextual lookup — an unsupported
    /// iteration lookup, or a yield position neither
    /// [`Checker::get_contextual_type`] answers nor
    /// [`Checker::has_no_contextual_type`] proves empty — nothing is
    /// reported.
    pub(crate) fn check_implicit_any_yield_expression(&mut self, node: NodeId) {
        if !self.no_implicit_any {
            return;
        }
        let Some(Node::YieldExpression(expression)) = self.node_map.get(node) else { return };
        if expression.asterisk_token.is_some() {
            return;
        }
        let Some(container) = self.containing_function(node) else { return };
        let (asterisk, annotation) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.r#type),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.r#type),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.r#type),
            _ => return,
        };
        if asterisk.is_none() || annotation.is_some() || self.expression_result_is_unused(node) {
            return;
        }
        if !matches!(
            self.get_contextual_iteration_type(
                crate::iteration::IterationTypeKind::Next,
                container
            ),
            Ok(None)
        ) {
            return;
        }
        let uncontextualised = self.has_no_contextual_type(node)
            || self.get_contextual_type(node).is_some_and(|contextual| {
                self.store.get(contextual).flags.contains(crate::flags::TypeFlags::ANY)
            });
        if !uncontextualised {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::new(
                &messages::YIELD_EXPRESSION_IMPLICITLY_RESULTS_IN_AN_ANY_TYPE_BECAUSE_ITS_CONTAINING_GENERATOR_LACKS_A_RETURN_TYPE_ANNOTATION,
                span,
            ),
        );
    }

    /// `expressionResultIsUnused` (`utilities.go:1159`).
    fn expression_result_is_unused(&self, mut node: NodeId) -> bool {
        while let Some(parent) = self.nodes.parent(node) {
            match self.node_map.get(parent) {
                Some(Node::ParenthesizedExpression(_)) => node = parent,
                Some(Node::ExpressionStatement(_) | Node::VoidExpression(_)) => return true,
                Some(Node::ForStatement(statement)) => {
                    return statement.initializer.and_then(|n| n.node_id()) == Some(node)
                        || statement.incrementor.and_then(|n| n.node_id()) == Some(node);
                }
                Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::CommaToken) =>
                {
                    if binary.left.and_then(|n| n.node_id()) == Some(node) {
                        return true;
                    }
                    node = parent;
                }
                _ => return false,
            }
        }
        false
    }
}
