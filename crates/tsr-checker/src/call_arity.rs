//! TS2554 / TS2555 — a call written with the wrong number of arguments.
//!
//! `getArgumentArityError` (`checker.go:9705`), reached from
//! `reportCallResolutionErrors` when no candidate signature accepts the argument
//! count.
//!
//! # The same trick as [`crate::type_argument_arity`]
//!
//! Upstream asks a *`Signature`* for `getMinArgumentCount` and
//! `getParameterCount`. Both are syntactic properties of the parameter list: the
//! minimum is the index of the first parameter that is optional, has an
//! initialiser, or is a rest element, and the maximum is the list's length. So
//! the rule reads the declaration and counts, and — like the arity rule for type
//! arguments — reports on a syntactic fact with no incompleteness to leak.
//!
//! What it cannot do syntactically is decide **which** signature a call resolves
//! to, which is why the rule is confined to a callee naming a symbol with
//! exactly one function-like declaration. An overload set is
//! `crate::calls`' business and `docs/architecture/checker-notes-callres.md`'s
//! row.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};
use tsr_diagnostics::{Diagnostic, messages};

use crate::types::TypeId;

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// The argument-count check for one call expression.
    /// `syntactic_arity` false: only the argument-type half runs, because
    /// the caller decided arity from the callee type's signatures
    /// ([`Checker::check_argument_arity_of_signatures`]).
    pub(crate) fn check_call_arity(&mut self, node: NodeId, syntactic_arity: bool) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::CallExpression(call)) = self.node_map.get(node) else { return };
        // A type-argument list means the callee is generic and the resolution
        // upstream performs is not the one this rule models.
        let Some(callee) = call.expression.and_then(|expression| expression.node_id()) else {
            return;
        };
        // `getSpreadArgumentIndex` (`checker.go:9706`) short-circuits to a
        // different message entirely; a spread also makes the count unknowable
        // without a tuple type.
        if call.arguments.iter().any(|argument| {
            argument.node_id().is_some_and(|id| self.nodes.kind(id) == SyntaxKind::SpreadElement)
        }) {
            return;
        }
        // A written type-argument list makes the call a *generic* one, and
        // `f < A, B > 7` parses as a call here and as a comparison chain
        // upstream (`grammarAmbiguities1`). Both are declined.
        //
        // **The decline is about the parse, not about resolution.** A call
        // written with type arguments can only match a *generic* signature, so
        // the overload path stays open with its candidates filtered to the
        // generic ones; only the sole-signature path — where the ambiguity
        // actually bites — is still declined. §970.
        let explicit_type_arguments = !call.type_arguments.is_empty();
        let arguments = call.arguments.len();
        // The argument *types* are checked at the same gate as the count,
        // because both need the same thing: exactly one signature, known from
        // the declaration. `checkApplicableSignature` (`checker.go`) runs after
        // arity and only for a candidate that survived it, so the ordering here
        // is upstream's too.
        self.check_argument_types(call, callee);
        if !syntactic_arity {
            return;
        }
        let Some((minimum, maximum)) = (if explicit_type_arguments {
            self.overload_set_arity_filtered(callee, true)
        } else {
            self.sole_signature_arity(callee).or_else(|| self.overload_set_arity(callee))
        }) else {
            return;
        };
        let unbounded = maximum.is_none();
        if arguments >= minimum && maximum.is_none_or(|maximum| arguments <= maximum) {
            return;
        }
        let message = if unbounded {
            &messages::EXPECTED_AT_LEAST_0_ARGUMENTS_BUT_GOT_1
        } else {
            &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1
        };
        // The error node is **not the same for the two directions**
        // (`checker.go:9770` and `:9804`). Too few arguments reports on the call
        // — `getErrorNodeForCallNode` (`:9843`), the callee, or its `.name` when
        // the callee is a property access. Too many reports on the *first excess
        // argument*: `functionCall6.ts(4,12)` for `foo('foo', 'bar')` is the
        // `'bar'`, and reporting at the callee there was 16 of this rule's first
        // 24 wrong lines.
        let at = match maximum {
            Some(maximum) if arguments > maximum => call
                .arguments
                .get(maximum)
                .and_then(tsr_ast::Expression::node_id)
                .unwrap_or_else(|| self.call_error_node(callee)),
            _ => self.call_error_node(callee),
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        let range = match maximum {
            Some(maximum) if maximum > minimum => format!("{minimum}-{maximum}"),
            _ => minimum.to_string(),
        };
        self.report(file, Diagnostic::with_args(message, span, [range, arguments.to_string()]));
    }

    /// TS2345 — `Argument of type '{0}' is not assignable to parameter of type
    /// '{1}'.`
    ///
    /// `checkApplicableSignature` → `getSignatureApplicabilityError`
    /// (`checker.go`), error node the **argument**:
    /// `arrayAssignmentTest3.ts(12,16)` is the `null` of `new a(null, 7, …)`.
    ///
    /// # The same two ideas as everything that has worked here
    ///
    /// The signature comes from the *declaration* — one callee, one
    /// non-generic function, written annotations — which is
    /// [`Checker::sole_signature_arity`]'s gate reused. The verdict comes from
    /// `relate_ternary`, so an undecidable pair is a silence rather than an
    /// error (§25). Nothing else is needed: TS2345's message arguments are not
    /// compared by the suite.
    ///
    /// A **generic** callee is declined whole. Its parameter types are written
    /// in terms of type parameters that inference substitutes, and this port's
    /// inference is `checker-notes-infer2.md`'s refused row — comparing an
    /// argument against an uninstantiated `T` is a confident wrong answer.
    fn check_argument_types(&mut self, call: &tsr_ast::CallExpression<'_>, callee: NodeId) {
        let Some(parameters) = self.sole_signature_parameters(callee) else { return };
        for (index, argument) in call.arguments.iter().enumerate() {
            let Some(parameter) = parameters.get(index) else { break };
            let Some((annotation, optional)) = *parameter else { continue };
            let Some(argument_id) = argument.node_id() else { continue };
            let declared = self.get_type_from_type_node(annotation);
            let target = self.add_optionality(declared, optional);
            // An object literal at an argument position is an excess-property
            // site exactly as one at a declaration is — the contextual type is
            // the parameter's. `check_excess_properties` is the same function
            // §23 wrote; only the target changes.
            // **If the elaboration spoke, the outer code does not.**
            // `checkTypeRelatedToAndOptionallyElaborate` walks an object
            // literal's members and reports the offending *member*; the outer
            // `Argument of type … is not assignable` is what it says when
            // there is nothing finer to point at.
            // `process({ a: true, b: "y" })` is one TS2322 upstream and was a
            // TS2322 **and** a TS2345 here — §59's standing loss, diagnosed in
            // §72.
            let before = self.diagnostics.len();
            self.check_excess_properties(target, argument_id);
            if self.diagnostics.len() != before {
                return;
            }
            let source = self.check_expression(*argument);
            // `getSignatureApplicabilityError` **returns on the first
            // failure** — a signature that fails is not asked about its
            // remaining arguments. `foo(1, 'bar')` against
            // `foo(a: string, b?: number)` is one TS2345 upstream and was two
            // here (`checker-notes-diag2.md` §59).
            if self.report_argument_failure(argument_id, source, target) {
                return;
            }
        }
    }

    /// The same two checks for a `new` expression.
    ///
    /// `resolveNewExpression` reaches the same `getArgumentArityError` and
    /// `checkApplicableSignature`; the only differences are where the signature
    /// comes from — the class's sole constructor — and that
    /// `getErrorNodeForCallNode` (`checker.go:9843`) unwraps **only** a
    /// `CallExpression`, so a too-few-arguments error on `new C()` reports on the
    /// whole `new` expression rather than on `C`.
    /// `syntactic_arity` as for [`Checker::check_call_arity`].
    pub(crate) fn check_new_arity(&mut self, node: NodeId, syntactic_arity: bool) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::NewExpression(call)) = self.node_map.get(node) else { return };
        if !call.type_arguments.is_empty() {
            return;
        }
        let Some(callee) = call.expression.and_then(|expression| expression.node_id()) else {
            return;
        };
        if call.arguments.iter().any(|argument| {
            argument.node_id().is_some_and(|id| self.nodes.kind(id) == SyntaxKind::SpreadElement)
        }) {
            return;
        }
        let Some(arity) = self.sole_constructor_parameters(callee) else { return };
        let ConstructorArity { annotations, minimum, maximum, check_argument_types } = arity;
        // Argument types first, exactly as the call arm orders them — and only
        // where the signature is a single non-generic one, §90.
        for (index, argument) in call.arguments.iter().enumerate().take(if check_argument_types {
            usize::MAX
        } else {
            0
        }) {
            let Some((annotation, optional)) = annotations.get(index).copied().flatten() else {
                break;
            };
            let Some(argument_id) = argument.node_id() else { continue };
            let Some(target) = self.parameter_target_type(annotation, optional) else { continue };
            let before = self.diagnostics.len();
            self.check_excess_properties(target, argument_id);
            if self.diagnostics.len() != before {
                break;
            }
            let source = self.check_expression(*argument);
            if self.report_argument_failure(argument_id, source, target) {
                break;
            }
        }
        if !syntactic_arity {
            return;
        }
        // `sole_constructor_parameters` stops at the first rest parameter and
        // keeps every position in order, so the count is exact and the unbounded
        // case cannot arise — a rest constructor is declined, not widened.
        // The (minimum, maximum) pair, exactly as the call arm computes it:
        // the minimum is the index of the first parameter that is optional or
        // has an initialiser, and the maximum is the list's length.
        // Comparing against the length alone read `constructor(x?: string)` as
        // requiring one — §56.
        let arguments = call.arguments.len();
        if arguments >= minimum && arguments <= maximum {
            return;
        }
        let message = &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1;
        let at = if arguments > maximum {
            call.arguments.get(maximum).and_then(tsr_ast::Expression::node_id).unwrap_or(node)
        } else {
            node
        };
        // `Expected 1-2 arguments, but got 0.` is the SAME message and the same
        // code: upstream composes the range into `{0}` rather than selecting a
        // second diagnostic. There is no `Expected_0_1_arguments_but_got_2` in
        // `messages.rs` to reach for — §90.
        let expected =
            if minimum == maximum { maximum.to_string() } else { format!("{minimum}-{maximum}") };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(file, Diagnostic::with_args(message, span, [expected, arguments.to_string()]));
    }

    /// The written parameter annotations of a class's **sole** constructor.
    ///
    /// Declined for a generic class, a class with more than one declaration, an
    /// overloaded constructor (more than one, or one without a body), and a class
    /// with **no** constructor at all — the last because its signature comes from
    /// the base class, which is `getBaseConstructorTypeOfClass`.
    pub(crate) fn sole_constructor_parameters(
        &mut self,
        callee: NodeId,
    ) -> Option<ConstructorArity> {
        let Some(Node::Identifier(identifier)) = self.node_map.get(callee) else { return None };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            callee,
            identifier.text,
            SymbolFlags::VALUE,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::CLASS) || entry.declarations.len() != 1 {
            return None;
        }
        let Some(Node::ClassDeclaration(class)) = self.node_map.get(entry.declarations[0]) else {
            return None;
        };
        // A generic class is NOT declined: `constructor(x: T)` takes one
        // argument whether or not `T` is inferred, and arity is the only
        // question this rule needs answered. What generics do rule out is the
        // argument-TYPE half, which needs the instantiated parameter type — so
        // the flag travels with the arity rather than the whole rule declining.
        // §90.
        let mut check_argument_types = class.type_parameters.is_empty();
        let mut class = class;
        // `getSignaturesOfType` on a class with no constructor of its own
        // resolves the **base**'s (`classWithBaseClassButNoConstructor`). One
        // `extends` link is the whole of what that family wants; a deeper
        // chain, a generic base, or a base this port cannot resolve declines.
        let mut hops = 0u32;
        // The hop stops at a class that DECLARES a constructor, with or without
        // a body. `getSignaturesOfType` takes a class's own construct signatures
        // whenever it has any, and an ambient overload set
        // (`declare class BaseBase<T, U> { constructor(x: T, ...y: U[]); … }`)
        // is a set of signatures. Testing for a body walked straight past it
        // into `BaseBase2` and priced every `new Derived(…)` against the wrong
        // constructor — the six wrong lines of `inheritedConstructorWithRestParams2`.
        while !class
            .members
            .iter()
            .any(|member| matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(_)))
        {
            hops += 1;
            if hops > 8 {
                return None;
            }
            // **Two endings, not one.** `sole_extends_class_declaration`
            // returns `None` both for *there is no `extends` clause* and for
            // *there is one this port cannot resolve*, and `?` merges them. The
            // first is not a failure: a class with no constructor and no base
            // has the implicit **zero-argument** constructor, which is what
            // `getSignaturesOfType` yields and what upstream reports `new
            // Bar(0)` against. The second is genuinely unknown and still
            // declines. §343.
            let base = match self.sole_extends_class_declaration(class) {
                Some(base) => base,
                None if !class
                    .heritage_clauses
                    .iter()
                    .any(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword) =>
                {
                    // No annotations exist to check arguments against, and a
                    // zero-parameter signature makes every argument an arity
                    // error rather than a type error.
                    return Some(ConstructorArity {
                        annotations: Vec::new(),
                        minimum: 0,
                        maximum: 0,
                        check_argument_types: false,
                    });
                }
                None => return None,
            };
            if !base.type_parameters.is_empty() {
                check_argument_types = false;
            }
            class = base;
        }
        let constructors: Vec<&tsr_ast::ConstructorDeclaration<'_>> = class
            .members
            .iter()
            .filter_map(|member| match member {
                tsr_ast::ClassElement::ConstructorDeclaration(constructor) => Some(*constructor),
                _ => None,
            })
            .collect();
        // `getSignaturesOfSymbol`: where a constructor is overloaded, the call
        // signatures are the **overloads** and the implementation is not one of
        // them. With no overloads the single implementation is the signature.
        let overloaded = constructors.len() > 1;
        let signatures: Vec<&tsr_ast::ConstructorDeclaration<'_>> = if overloaded {
            constructors.iter().filter(|c| c.body.is_none()).copied().collect()
        } else {
            constructors.clone()
        };
        let [first, ..] = signatures.as_slice() else { return None };
        if !overloaded && first.body.is_none() {
            return None;
        }
        if overloaded {
            check_argument_types = false;
        }
        // Each signature's (minimum, length). A rest parameter makes the
        // maximum unbounded, which this rule has never reported on — decline
        // the whole callee rather than guess a bound.
        let mut ranges = Vec::with_capacity(signatures.len());
        for signature in &signatures {
            let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = signature
                .parameters
                .iter()
                .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
                .copied()
                .collect();
            if parameters.iter().any(|parameter| parameter.dot_dot_dot_token.is_some()) {
                return None;
            }
            ranges.push((Self::minimum_argument_count(&parameters), parameters.len()));
        }
        let minimum = ranges.iter().map(|(minimum, _)| *minimum).min()?;
        let maximum = ranges.iter().map(|(_, length)| *length).max()?;
        // The written annotations belong to the sole signature; with overloads
        // there is no single list and `check_argument_types` is already false.
        let annotations = first
            .parameters
            .iter()
            .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
            .map(|parameter| {
                parameter
                    .r#type
                    .and_then(|annotation| annotation.node_id())
                    .map(|annotation| (annotation, Self::parameter_is_optional(parameter)))
            })
            .collect();
        Some(ConstructorArity { annotations, minimum, maximum, check_argument_types })
    }

    /// The class declaration a class `extends`, when the heritage names a
    /// single non-generic class this port can resolve.
    fn sole_extends_class_declaration(
        &mut self,
        class: &'a tsr_ast::ClassDeclaration<'a>,
    ) -> Option<&'a tsr_ast::ClassDeclaration<'a>> {
        let clause = class
            .heritage_clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)?;
        let [base] = clause.types else { return None };
        // `extends C2<T, U>` is not declined: the base's type ARGUMENTS change
        // what its constructor's parameters mean, never how many there are, and
        // the caller turns off argument-type checking for a generic base
        // anyway — §90.
        let expression = base.expression?.node_id()?;
        let Some(Node::Identifier(name)) = self.node_map.get(expression) else { return None };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            expression,
            name.text,
            SymbolFlags::VALUE,
        )?;
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::CLASS) || entry.declarations.len() != 1 {
            return None;
        }
        match self.node_map.get(entry.declarations[0])? {
            Node::ClassDeclaration(declaration) => Some(declaration),
            _ => None,
        }
    }

    /// `get_type_from_type_node` reached from a [`NodeId`] — ADR-0013's
    /// read-drop-recurse.
    pub(crate) fn type_from_annotation_id(&mut self, node: NodeId) -> Option<crate::types::TypeId> {
        let typed = tsr_ast::TypeNode::try_from(self.node_map.get(node)?).ok()?;
        Some(self.get_type_from_type_node(typed))
    }

    /// The symbol a callee names, for the two shapes whose signature this
    /// module can reach.
    ///
    /// A bare identifier resolves through the scope. A **property access**
    /// resolves through the receiver's type, and is admitted only where
    /// [`Checker::declared_members_are_complete`] certifies that table — which
    /// §35 made true for a namespace or enum receiver, so `N.f(…)` is now
    /// reachable and `obj.method(…)` on a class still is not (its table is
    /// complete only when nothing inherits, and a method call's receiver is
    /// usually an instance).
    fn callee_symbol(&mut self, callee: NodeId) -> Option<SymbolId> {
        match self.node_map.get(callee)? {
            Node::Identifier(identifier) => {
                let text = identifier.text;
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    callee,
                    text,
                    SymbolFlags::VALUE,
                )?;
                Some(self.binder.merged_symbol(symbol))
            }
            Node::PropertyAccessExpression(access) => {
                if access.question_dot_token.is_some() {
                    return None;
                }
                let receiver = access.expression?.node_id()?;
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                let name = name.text;
                let receiver_type = self.check_expression_at_node(receiver);
                if !self.declared_members_are_complete(receiver_type) {
                    return None;
                }
                let symbol = self.get_property_of_type(receiver_type, name)?;
                Some(self.binder.merged_symbol(symbol))
            }
            _ => None,
        }
    }

    /// `getErrorNodeForCallNode` (`checker.go:9843`).
    pub(crate) fn call_error_node(&self, callee: NodeId) -> NodeId {
        match self.node_map.get(callee) {
            Some(Node::PropertyAccessExpression(access)) => {
                access.name.and_then(|name| name.node_id()).unwrap_or(callee)
            }
            _ => callee,
        }
    }

    /// The **sole** signature declaration a callee names, whatever kind it is
    /// spelled as, together with whether it is generic.
    ///
    /// Upstream has no four-way choice here: `getSignatureFromDeclaration`
    /// (`checker.go:19902`) accepts any `SignatureDeclaration`, and
    /// `getMinArgumentCount` / `getParameterCount` read the `Signature` it
    /// builds. The four kinds below are that set restricted to the ones a
    /// *callee* can name — §78.
    ///
    /// `None` for every other shape, and each `None` is a decline with a reason:
    ///
    /// - **more than one declaration** — an overload set, whose selection is
    ///   `resolveCall`'s and whose arity error upstream computes across *all*
    ///   candidates (`checker.go:9715`);
    /// - **a bodiless `FunctionDeclaration` or `MethodDeclaration`** — an
    ///   overload set of one is still an overload set as far as the corpus is
    ///   concerned, and its implementation may be in another file this rule has
    ///   not looked at. A **`MethodSignatureDeclaration`** never has a body and
    ///   is not an overload set, so the test is per-kind rather than universal;
    /// - **a variable with no function-valued initialiser** — it holds a
    ///   function *type*, whose parameter list is not on the declaration;
    /// - **a class** — that is a construct signature, and `new` is a different
    ///   node kind this rule does not visit.
    fn sole_signature_declaration(
        &mut self,
        callee: NodeId,
    ) -> Option<(&'a [&'a tsr_ast::ParameterDeclaration<'a>], bool)> {
        let symbol = self.callee_symbol(callee)?;
        let entry = self.binder.symbols().get(symbol);
        if !entry
            .flags
            .intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD | SymbolFlags::VARIABLE)
            || entry.declarations.len() != 1
        {
            return None;
        }
        let declaration = entry.declarations[0];
        // A variable is the *indirect* spelling: `var f = function () {}` and
        // `const f = (a) => a` declare a symbol whose declaration carries no
        // parameter list of its own. The initialiser does, and only when it is
        // written as a function right there — an initialiser that is a *call*,
        // or a reference to another function, is a type this rule cannot read
        // syntactically.
        let signature = match self.node_map.get(declaration)? {
            Node::VariableDeclaration(variable) => {
                // **A written annotation IS the signature**, and the initialiser
                // is contextually typed by it — so the initialiser's parameter
                // list may be shorter than the type's and says nothing about
                // arity. `var Component: C = () => {}` where `C` is a call
                // signature taking one argument reported *Expected 0, got 1* on
                // every call to it (`thislessFunctionsNotContextSensitive1`,
                // §78's second and third wrong lines). Reading the annotation
                // instead is `crate::signatures`' road, not a syntactic
                // parameter count. Owner: TS2554's own next slice.
                if variable.r#type.is_some() {
                    return None;
                }
                variable.initializer.and_then(|initializer| initializer.node_id())?
            }
            _ => declaration,
        };
        let (parameters, type_parameters) = match self.node_map.get(signature)? {
            Node::FunctionDeclaration(node) => {
                node.body?;
                (node.parameters, node.type_parameters)
            }
            Node::MethodDeclaration(node) => {
                node.body?;
                (node.parameters, node.type_parameters)
            }
            Node::MethodSignatureDeclaration(node) => (node.parameters, node.type_parameters),
            Node::FunctionExpression(node) => {
                node.body?;
                (node.parameters, node.type_parameters)
            }
            Node::ArrowFunction(node) => {
                node.body?;
                (node.parameters, node.type_parameters)
            }
            _ => return None,
        };
        Some((parameters, type_parameters.is_empty()))
    }

    /// `(min, max)` for a callee naming an **overload set**.
    ///
    /// `getArgumentArityError` (`checker.go:9715`) computes its range across
    /// *all* candidate signatures, so the arity question has an answer even
    /// though "which signature does this call resolve to" does not — the same
    /// split §90 made for `new`, transplanted. `sole_signature_arity` declines
    /// this shape and always will: it is about one signature.
    ///
    /// The signatures are the **bodiless** declarations; the implementation is
    /// not a call signature (`getSignaturesOfSymbol`). Declines a set with any
    /// rest parameter, and a set whose declarations are not all function-like
    /// with the same shape — the argument-TYPE half is not attempted at all,
    /// because there is no single parameter list to attempt it against.
    fn overload_set_arity(&mut self, callee: NodeId) -> Option<(usize, Option<usize>)> {
        self.overload_set_arity_filtered(callee, false)
    }

    /// The overload set's arity range, optionally restricted to the **generic**
    /// overloads — the candidates a call with explicit type arguments can
    /// select. §970.
    fn overload_set_arity_filtered(
        &mut self,
        callee: NodeId,
        generic_only: bool,
    ) -> Option<(usize, Option<usize>)> {
        let symbol = self.callee_symbol(callee)?;
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD)
            || entry.declarations.len() < 2
        {
            return None;
        }
        let declarations = entry.declarations.clone();
        let mut ranges = Vec::new();
        for declaration in declarations {
            if generic_only {
                let generic = match self.node_map.get(declaration)? {
                    Node::FunctionDeclaration(node) => !node.type_parameters.is_empty(),
                    Node::MethodDeclaration(node) => !node.type_parameters.is_empty(),
                    _ => false,
                };
                if !generic {
                    continue;
                }
            }
            let parameters = match self.node_map.get(declaration)? {
                Node::FunctionDeclaration(node) if node.body.is_none() => node.parameters,
                Node::MethodDeclaration(node) if node.body.is_none() => node.parameters,
                // The implementation signature, or a merged declaration of some
                // other kind — neither is a call signature.
                Node::FunctionDeclaration(_) | Node::MethodDeclaration(_) => continue,
                _ => return None,
            };
            let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = parameters
                .iter()
                .copied()
                .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
                .collect();
            if parameters.iter().any(|parameter| parameter.dot_dot_dot_token.is_some()) {
                return None;
            }
            ranges.push((Self::minimum_argument_count(&parameters), parameters.len()));
        }
        // **Two ranges are required only when every overload is a candidate.**
        // Filtering to the generic ones routinely leaves exactly one, and
        // rejecting that was §970's first measurement: +0, because
        // `functionCall18` has precisely one generic overload and the whole
        // point of the filter is that it is the only candidate. §971.
        if ranges.len() < if generic_only { 1 } else { 2 } {
            return None;
        }
        let minimum = ranges.iter().map(|(minimum, _)| *minimum).min()?;
        let maximum = ranges.iter().map(|(_, length)| *length).max()?;
        Some((minimum, Some(maximum)))
    }

    /// `(getMinArgumentCount, getParameterCount)` where the callee names exactly
    /// one signature declaration — see [`Checker::sole_signature_declaration`]
    /// for which shapes those are and why each other one declines.
    fn sole_signature_arity(&mut self, callee: NodeId) -> Option<(usize, Option<usize>)> {
        let (declaration_parameters, _) = self.sole_signature_declaration(callee)?;
        let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = declaration_parameters
            .iter()
            .copied()
            // `this` is not an argument (`getParameterCount` skips it).
            .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
            .collect();
        if parameters.iter().any(|parameter| parameter.dot_dot_dot_token.is_some()) {
            // **`None` is unbounded, which is right for `...args: T[]` and
            // wrong for `...[a, b]`.** A rest parameter named by an array
            // binding pattern destructures a fixed number of arguments, and the
            // guard above reads the `...` and never the name. §972.
            if let Some(fixed) = Self::destructured_rest_arity(&parameters) {
                return Some(fixed);
            }
            return Some((Self::minimum_argument_count(&parameters), None));
        }
        let maximum = parameters.len();
        let mut minimum = Self::minimum_argument_count(&parameters);
        // `getMinArgumentCountEx` (`relater.go:1737`) walks back from the
        // minimum and drops every trailing parameter whose type contains
        // `void` — a `void` parameter may be omitted. `callWithMissingVoid` is
        // the whole of what this costs, and it was 4 wrong lines.
        while minimum > 0 && self.parameter_annotation_is_void(parameters[minimum - 1]) {
            minimum -= 1;
        }
        Some((minimum, Some(maximum)))
    }

    /// `minArgumentCount` as `getSignatureFromDeclaration` builds it
    /// (`checker.go:19872-19879`): the count is reset to the running parameter
    /// count at **every non-optional parameter**, so the answer is the position
    /// after the **last** required one — not the position of the first optional
    /// one. The two agree on every well-formed signature and disagree on
    /// `function f(a, b = 0, c)`, which upstream requires **three** arguments
    /// for. `requiredInitializedParameter1` is that case, and the first-optional
    /// reading silently accepted `f(0, 1)`.
    /// The arity of a signature whose **last** parameter is a rest named by an
    /// array binding pattern: `(...[a, b])` takes exactly two arguments.
    ///
    /// `None` when the shape does not apply — an ordinary rest, an object
    /// pattern, a nested rest element, or a rest that is not last. §972.
    fn destructured_rest_arity(
        parameters: &[&tsr_ast::ParameterDeclaration<'_>],
    ) -> Option<(usize, Option<usize>)> {
        let (rest, leading) = parameters.split_last()?;
        if rest.dot_dot_dot_token.is_none()
            || leading.iter().any(|parameter| parameter.dot_dot_dot_token.is_some())
        {
            return None;
        }
        let Some(tsr_ast::BindingName::BindingPattern(pattern)) = rest.name else { return None };
        // **`BindingPattern::kind` is the opening *token*, not the node's
        // `SyntaxKind`** — `[` for an array pattern and `{` for an object one.
        // Comparing it against `SyntaxKind::ArrayBindingPattern` was §972's
        // first measurement: +0, with the arm reached and returning `None`.
        if pattern.kind.kind != SyntaxKind::OpenBracketToken {
            return None;
        }
        if pattern.elements.iter().any(|element| element.dot_dot_dot_token.is_some()) {
            return None;
        }
        let required = pattern
            .elements
            .iter()
            .rposition(|element| element.initializer.is_none())
            .map_or(0, |index| index + 1);
        let leading_minimum = Self::minimum_argument_count(leading);
        Some((leading_minimum + required, Some(leading.len() + pattern.elements.len())))
    }

    fn minimum_argument_count(parameters: &[&tsr_ast::ParameterDeclaration<'_>]) -> usize {
        parameters
            .iter()
            .rposition(|parameter| {
                parameter.question_token.is_none()
                    && parameter.initializer.is_none()
                    && parameter.dot_dot_dot_token.is_none()
            })
            .map_or(0, |index| index + 1)
    }

    /// Is this parameter's **written** annotation the `void` keyword?
    ///
    /// Upstream asks the computed type whether any constituent carries
    /// `TypeFlagsVoid`; this asks the annotation, which answers the same for
    /// `void` and for `T | void` and declines every alias and generic
    /// instantiation that would need the type. A decline here raises the
    /// minimum, so it costs a *wrong* diagnostic rather than a missing one —
    /// which is why the shapes it does not cover are named rather than assumed:
    /// an alias for `void`, and a type parameter instantiated with it.
    fn parameter_annotation_is_void(
        &mut self,
        parameter: &tsr_ast::ParameterDeclaration<'a>,
    ) -> bool {
        let Some(annotation) = parameter.r#type else { return false };
        // **The type, not the spelling.** The comment above records what the
        // syntactic form declined — aliases and generic instantiations — and
        // that a decline here costs a *wrong* diagnostic. §854 measured the
        // same substitution on TS2355 at +4/−0, so this one is measured too.
        // §855.
        let annotated = self.get_type_from_type_node_unprinted(annotation);
        if self.type_of(annotated).flags.intersects(crate::flags::TypeFlags::VOID) {
            return true;
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(annotated).data {
            let members = types.clone();
            return members.iter().any(|&member| {
                self.type_of(member).flags.intersects(crate::flags::TypeFlags::VOID)
            });
        }
        false
    }

    /// `getTypeOfParameter` (`checker.go:17042`) — the parameter's declared type
    /// with **optionality added**.
    ///
    /// ```go
    /// declaration := symbol.ValueDeclaration
    /// return c.addOptionalityEx(c.getTypeOfSymbol(symbol), false,
    ///     declaration != nil && (declaration.Initializer() != nil || isOptionalDeclaration(declaration)))
    /// ```
    ///
    /// `addOptionalityEx` (`:18633`) unions in `undefined` under
    /// `strictNullChecks`, and `isOptionalDeclaration` (`utilities.go:299`) is
    /// `HasQuestionToken`. So **both** spellings of an optional parameter widen
    /// its type:
    ///
    /// ```ts
    /// function f(a: string, b?: string) {}        // b: string | undefined
    /// function g(a: string, b: string = "d") {}   // b: string | undefined
    /// ```
    ///
    /// Taking the annotation alone made every `string | undefined` argument at
    /// such a position a TS2345, which is upstream's answer for a *required*
    /// parameter and nobody's for an optional one. `getFileLanguage(language,
    /// fileName)` against `fileName?: string` is the shape; it is what a
    /// component library is written in.
    ///
    /// **`is_property: false`**, upstream's argument here, so the added member
    /// is `undefined` and not the `missing` type an optional *property* gets.
    fn parameter_target_type(&mut self, annotation: NodeId, optional: bool) -> Option<TypeId> {
        let declared = self.type_from_annotation_id(annotation)?;
        Some(self.add_optionality(declared, optional))
    }

    /// `addOptionalityEx(t, isProperty: false, isOptional)` (`checker.go:18633`).
    ///
    /// `getOptionalType` (`:18640`) short-circuits when the type already leads
    /// with `undefined`, which `get_union_type` reaches anyway by deduplicating.
    fn add_optionality(&mut self, declared: TypeId, optional: bool) -> TypeId {
        if !optional || !self.strict_null_checks {
            return declared;
        }
        let undefined = self.intrinsics.undefined;
        self.get_union_type(&[declared, undefined])
    }

    /// Whether a parameter declaration is optional, for
    /// [`Checker::parameter_target_type`].
    ///
    /// `declaration.Initializer() != nil || isOptionalDeclaration(declaration)`.
    /// A rest parameter never reaches here — both callers stop at the first
    /// `...`, because its annotation is the array rather than the element.
    fn parameter_is_optional(parameter: &tsr_ast::ParameterDeclaration<'_>) -> bool {
        parameter.question_token.is_some() || parameter.initializer.is_some()
    }

    /// The written parameter annotations of the sole signature a callee names,
    /// in order — `None` where [`Checker::sole_signature_arity`] declines, or
    /// where the function is generic.
    fn sole_signature_parameters(
        &mut self,
        callee: NodeId,
    ) -> Option<Vec<Option<(tsr_ast::TypeNode<'a>, bool)>>> {
        let symbol = self.callee_symbol(callee)?;
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::FUNCTION) || entry.declarations.len() != 1 {
            return None;
        }
        let Some(Node::FunctionDeclaration(declaration)) = self.node_map.get(entry.declarations[0])
        else {
            return None;
        };
        declaration.body?;
        // Inference is `checker-notes-infer2.md`'s refused row; an argument
        // compared against an uninstantiated `T` is a confident wrong answer.
        if !declaration.type_parameters.is_empty() {
            return None;
        }
        Some(
            declaration
                .parameters
                .iter()
                .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
                // A rest parameter's annotation is the *array*, not the element,
                // so position `i` no longer names parameter `i`.
                .take_while(|parameter| parameter.dot_dot_dot_token.is_none())
                .map(|parameter| {
                    parameter
                        .r#type
                        .map(|annotation| (annotation, Self::parameter_is_optional(parameter)))
                })
                .collect(),
        )
    }

    /// Is this the `this` parameter — the one that is a type annotation wearing
    /// a parameter's syntax?
    pub(crate) fn is_this_parameter_declaration(
        parameter: &tsr_ast::ParameterDeclaration<'_>,
    ) -> bool {
        matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
    }
}

/// What `sole_constructor_parameters` knows about a `new` callee.
///
/// Arity and argument types are separate answers: a generic or overloaded
/// constructor has a perfectly well-defined `(minimum, maximum)` and no single
/// instantiated parameter list, so the second half declines on its own —
/// `checker-notes-diag2.md` §90.
pub(crate) struct ConstructorArity {
    /// The written annotations of the sole signature, positionally, each with
    /// whether its parameter is optional — see
    /// [`Checker::parameter_target_type`].
    pub(crate) annotations: Vec<Option<(NodeId, bool)>>,
    /// `getMinArgumentCount`, minimised over the overload set.
    minimum: usize,
    /// The parameter count, maximised over the overload set.
    maximum: usize,
    /// Whether the arguments may also be checked against `annotations`.
    pub(crate) check_argument_types: bool,
}
