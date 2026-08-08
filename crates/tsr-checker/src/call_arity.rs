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

use crate::checker::Checker;

impl<'a> Checker<'a, '_> {
    /// The argument-count check for one call expression.
    pub(crate) fn check_call_arity(&mut self, node: NodeId) {
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
        if !call.type_arguments.is_empty() {
            return;
        }
        let arguments = call.arguments.len();
        // The argument *types* are checked at the same gate as the count,
        // because both need the same thing: exactly one signature, known from
        // the declaration. `checkApplicableSignature` (`checker.go`) runs after
        // arity and only for a candidate that survived it, so the ordering here
        // is upstream's too.
        self.check_argument_types(call, callee);
        let Some((minimum, maximum)) = self.sole_signature_arity(callee) else { return };
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
            let Some(annotation) = *parameter else { continue };
            let Some(argument_id) = argument.node_id() else { continue };
            let target = self.get_type_from_type_node(annotation);
            // An object literal at an argument position is an excess-property
            // site exactly as one at a declaration is — the contextual type is
            // the parameter's. `check_excess_properties` is the same function
            // §23 wrote; only the target changes.
            self.check_excess_properties(target, argument_id);
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
    pub(crate) fn check_new_arity(&mut self, node: NodeId) {
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
        let Some((parameters, minimum)) = self.sole_constructor_parameters(callee) else { return };
        // Argument types first, exactly as the call arm orders them.
        for (index, argument) in call.arguments.iter().enumerate() {
            let Some(annotation) = parameters.get(index).copied().flatten() else { break };
            let Some(argument_id) = argument.node_id() else { continue };
            let Some(target) = self.type_from_annotation_id(annotation) else { continue };
            let source = self.check_expression(*argument);
            if self.report_argument_failure(argument_id, source, target) {
                break;
            }
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
        let expected = parameters.len();
        if arguments >= minimum && arguments <= expected {
            return;
        }
        let message = &messages::EXPECTED_0_ARGUMENTS_BUT_GOT_1;
        let at = if arguments > expected {
            call.arguments.get(expected).and_then(tsr_ast::Expression::node_id).unwrap_or(node)
        } else {
            node
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        self.report(
            file,
            Diagnostic::with_args(message, span, [expected.to_string(), arguments.to_string()]),
        );
    }

    /// The written parameter annotations of a class's **sole** constructor.
    ///
    /// Declined for a generic class, a class with more than one declaration, an
    /// overloaded constructor (more than one, or one without a body), and a class
    /// with **no** constructor at all — the last because its signature comes from
    /// the base class, which is `getBaseConstructorTypeOfClass`.
    fn sole_constructor_parameters(
        &mut self,
        callee: NodeId,
    ) -> Option<(Vec<Option<NodeId>>, usize)> {
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
        if !class.type_parameters.is_empty() {
            return None;
        }
        let mut class = class;
        // `getSignaturesOfType` on a class with no constructor of its own
        // resolves the **base**'s (`classWithBaseClassButNoConstructor`). One
        // `extends` link is the whole of what that family wants; a deeper
        // chain, a generic base, or a base this port cannot resolve declines.
        let mut hops = 0u32;
        while !class.members.iter().any(|member| {
            matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(constructor)
                if constructor.body.is_some())
        }) {
            hops += 1;
            if hops > 8 {
                return None;
            }
            let base = self.sole_extends_class_declaration(class)?;
            class = base;
        }
        let mut constructors = class.members.iter().filter_map(|member| match member {
            tsr_ast::ClassElement::ConstructorDeclaration(constructor) => Some(*constructor),
            _ => None,
        });
        let constructor = constructors.next()?;
        if constructors.next().is_some() || constructor.body.is_none() {
            return None;
        }
        let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = constructor
            .parameters
            .iter()
            .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
            .take_while(|parameter| parameter.dot_dot_dot_token.is_none())
            .copied()
            .collect();
        // `getMinArgumentCount`: the index of the first parameter that is
        // optional or carries an initialiser.
        let minimum = parameters
            .iter()
            .position(|parameter| {
                parameter.question_token.is_some() || parameter.initializer.is_some()
            })
            .unwrap_or(parameters.len());
        Some((
            parameters
                .iter()
                .map(|parameter| parameter.r#type.and_then(|annotation| annotation.node_id()))
                .collect(),
            minimum,
        ))
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
        if !base.type_arguments.is_empty() {
            return None;
        }
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
            Node::ClassDeclaration(declaration) if declaration.type_parameters.is_empty() => {
                Some(declaration)
            }
            _ => None,
        }
    }

    /// `get_type_from_type_node` reached from a [`NodeId`] — ADR-0013's
    /// read-drop-recurse.
    fn type_from_annotation_id(&mut self, node: NodeId) -> Option<crate::types::TypeId> {
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
    fn call_error_node(&self, callee: NodeId) -> NodeId {
        match self.node_map.get(callee) {
            Some(Node::PropertyAccessExpression(access)) => {
                access.name.and_then(|name| name.node_id()).unwrap_or(callee)
            }
            _ => callee,
        }
    }

    /// `(getMinArgumentCount, getParameterCount)` where the callee names exactly
    /// one function-like declaration.
    ///
    /// `None` for every other shape, and each `None` is a decline with a reason:
    ///
    /// - **not a bare identifier** — a property access needs the receiver's type
    ///   to find the member, which is [`crate::members`]' road;
    /// - **more than one declaration** — an overload set, whose selection is
    ///   `resolveCall`'s and whose arity error upstream computes across *all*
    ///   candidates (`checker.go:9715`);
    /// - **not a function or method declaration** — a class is a construct
    ///   signature, a variable holds a function *type* whose parameter list is
    ///   not on the declaration.
    fn sole_signature_arity(&mut self, callee: NodeId) -> Option<(usize, Option<usize>)> {
        let symbol = self.callee_symbol(callee)?;
        let entry = self.binder.symbols().get(symbol);
        if !entry.flags.intersects(SymbolFlags::FUNCTION) || entry.declarations.len() != 1 {
            return None;
        }
        let Some(Node::FunctionDeclaration(declaration)) = self.node_map.get(entry.declarations[0])
        else {
            return None;
        };
        // An overload set of one — a declaration with no body — is still an
        // overload set as far as the corpus is concerned, and its implementation
        // may be in another file this rule has not looked at.
        declaration.body?;
        let parameters: Vec<&tsr_ast::ParameterDeclaration<'_>> = declaration
            .parameters
            .iter()
            .copied()
            // `this` is not an argument (`getParameterCount` skips it).
            .filter(|parameter| !Self::is_this_parameter_declaration(parameter))
            .collect();
        if parameters.iter().any(|parameter| parameter.dot_dot_dot_token.is_some()) {
            let minimum = parameters
                .iter()
                .position(|parameter| {
                    parameter.question_token.is_some()
                        || parameter.initializer.is_some()
                        || parameter.dot_dot_dot_token.is_some()
                })
                .unwrap_or(parameters.len());
            return Some((minimum, None));
        }
        let maximum = parameters.len();
        let mut minimum = parameters
            .iter()
            .position(|parameter| {
                parameter.question_token.is_some() || parameter.initializer.is_some()
            })
            .unwrap_or(maximum);
        // `getMinArgumentCountEx` (`relater.go:1737`) walks back from the
        // minimum and drops every trailing parameter whose type contains
        // `void` — a `void` parameter may be omitted. `callWithMissingVoid` is
        // the whole of what this costs, and it was 4 wrong lines.
        while minimum > 0 && self.parameter_annotation_is_void(parameters[minimum - 1]) {
            minimum -= 1;
        }
        Some((minimum, Some(maximum)))
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
    fn parameter_annotation_is_void(&self, parameter: &tsr_ast::ParameterDeclaration<'_>) -> bool {
        let Some(annotation) = parameter.r#type else { return false };
        let Some(id) = annotation.node_id() else { return false };
        match self.node_map.get(id) {
            Some(Node::KeywordTypeNode(keyword)) => keyword.kind == SyntaxKind::VoidKeyword,
            Some(Node::UnionTypeNode(union)) => union.types.iter().any(|member| {
                member.node_id().and_then(|id| self.node_map.get(id)).is_some_and(|node| {
                    matches!(node, Node::KeywordTypeNode(keyword)
                        if keyword.kind == SyntaxKind::VoidKeyword)
                })
            }),
            _ => false,
        }
    }

    /// The written parameter annotations of the sole signature a callee names,
    /// in order — `None` where [`Checker::sole_signature_arity`] declines, or
    /// where the function is generic.
    fn sole_signature_parameters(
        &mut self,
        callee: NodeId,
    ) -> Option<Vec<Option<tsr_ast::TypeNode<'a>>>> {
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
                .map(|parameter| parameter.r#type)
                .collect(),
        )
    }

    /// Is this the `this` parameter — the one that is a type annotation wearing
    /// a parameter's syntax?
    fn is_this_parameter_declaration(parameter: &tsr_ast::ParameterDeclaration<'_>) -> bool {
        matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
    }
}
