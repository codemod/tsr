//! TS2314 / TS2707 — a generic type reference written with the wrong number of
//! type arguments.
//!
//! `getTypeFromClassOrInterfaceReference` (`checker.go:23169`), the arity arm.
//!
//! # A type rule this port can answer without computing a type
//!
//! Upstream asks the *declared type* for its local type parameters. Everything
//! that decision needs, though, is on the **declaration**: a class or interface
//! writes its type parameters syntactically, and a parameter with a default is
//! exactly the one that may be omitted (`getMinTypeArgumentCount`). So the rule
//! reads the declaration list and counts, which puts it in the same family as
//! the grammar rules in [`crate::check`] rather than in
//! [`crate::assignreport`]'s: it reports on a syntactic fact and has no
//! incompleteness to leak.
//!
//! The one thing it must get right is *which* declaration, and that is a name
//! resolution — the same one [`crate::check::Checker::check_value_identifier`]
//! makes, asked in type space.
//!
//! # What it declines
//!
//! | decline | why |
//! |---|---|
//! | a `.js` file | upstream substitutes `Expected_0_type_arguments_provide_these_with_an_extends_tag` when a JSDoc `@augments` tag is missing (`checker.go:23181`), and with `noImplicitAny` off it reports nothing at all |
//! | a type **alias** | its arity error is TS2315 / TS2558 from a different function |
//! | a name that resolves to more than one kind of declaration | a class merged with an interface or a namespace has type parameters upstream reads off the merged symbol, and this port's merge is not upstream's |
//! | a name that does not resolve | TS2304 / TS2552 territory, already a row |

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The arity check for one written type reference.
    pub(crate) fn check_type_argument_arity(&mut self, node: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let (name, written) = match self.node_map.get(node) {
            Some(Node::TypeReferenceNode(reference)) => (
                reference.type_name.and_then(|name| name.node_id()),
                reference.type_arguments.len(),
            ),
            Some(Node::ExpressionWithTypeArguments(reference)) => (
                reference.expression.and_then(|expression| expression.node_id()),
                reference.type_arguments.len(),
            ),
            _ => return,
        };
        let Some(name) = name else { return };
        // An `ExpressionWithTypeArguments` is also the syntax of an
        // *instantiation expression* (`f<number>`), which is a value position
        // and has nothing to do with this rule. Only a heritage clause's is a
        // type reference.
        if self.nodes.kind(node) == SyntaxKind::ExpressionWithTypeArguments
            && self
                .nodes
                .parent(node)
                .is_none_or(|parent| self.nodes.kind(parent) != SyntaxKind::HeritageClause)
        {
            return;
        }
        let Some((minimum, maximum)) = self.declared_type_parameter_arity(name) else { return };
        // `maximum == 0` is an **answer**, not a failure to compute one: the
        // class or interface resolved and declares no type parameters. Upstream
        // does not treat it as an arity mismatch at all — `checkNoTypeArguments`
        // (`checker.go:23220`) issues a different diagnostic, on the same error
        // node this rule already uses. §345.
        if maximum == 0 {
            if written == 0 {
                return;
            }
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            let span = self.error_span(node);
            let printed = self.written_type_name(name);
            self.report(
                file,
                Diagnostic::with_args(&messages::TYPE_0_IS_NOT_GENERIC, span, [printed]),
            );
            return;
        }
        if written >= minimum && written <= maximum {
            return;
        }
        let message = if minimum < maximum {
            &messages::GENERIC_TYPE_0_REQUIRES_BETWEEN_1_AND_2_TYPE_ARGUMENTS
        } else {
            &messages::GENERIC_TYPE_0_REQUIRES_1_TYPE_ARGUMENT_S
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let printed = self.written_type_name(name);
        self.report(
            file,
            Diagnostic::with_args(
                message,
                span,
                [printed, minimum.to_string(), maximum.to_string()],
            ),
        );
    }

    /// The same arity question for a **call**: `f<number>()`, `new D<number>()`.
    ///
    /// `getTypeArgumentArityError` (`checker.go:9852`). Written type arguments
    /// make both value-arity arms decline (`if !call.type_arguments.is_empty()`)
    /// and nothing took over, so this position had no owner at all. §347.
    ///
    /// The error node is upstream's `loc`: the type-argument **list**, whose
    /// `Pos` is just past the `<`, so after `SkipTrivia` the span starts at the
    /// first type argument rather than at the callee.
    ///
    /// Confined to upstream's own `len(signatures) == 1` branch — one
    /// declaration, a function or a class. The overload arm needs a signature
    /// set this port does not build and carries a different message.
    pub(crate) fn check_call_type_argument_arity(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let (callee, type_arguments) = match self.node_map.get(node) {
            Some(Node::CallExpression(call)) => {
                (call.expression.and_then(|e| e.node_id()), call.type_arguments)
            }
            Some(Node::NewExpression(call)) => {
                (call.expression.and_then(|e| e.node_id()), call.type_arguments)
            }
            _ => return,
        };
        let (Some(callee), [first, ..]) = (callee, type_arguments) else { return };
        let Some(Node::Identifier(identifier)) = self.node_map.get(callee) else { return };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            callee,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            return;
        };
        let symbol = self.binder.merged_symbol(symbol);
        let entry = self.binder.symbols().get(symbol);
        // `len(signatures) == 1`. An overload set is upstream's other branch.
        let [declaration] = entry.declarations.as_slice() else { return };
        let parameters = match self.node_map.get(*declaration) {
            Some(Node::FunctionDeclaration(function)) => function.type_parameters,
            Some(Node::ClassDeclaration(class)) => class.type_parameters,
            // **A variable holding a function literal** carries the signature's
            // type parameters on the literal, and `var f2 = (x: number) => …;
            // f2<string>(1)` is TS2558 exactly as a declared function is. The
            // annotation and property-access forms need the *type's* signature
            // list and are the type side's; this one is syntactic. §942.
            Some(Node::VariableDeclaration(variable)) => {
                match variable
                    .initializer
                    .and_then(|e| e.node_id())
                    .and_then(|id| self.node_map.get(id))
                {
                    Some(Node::ArrowFunction(arrow)) => arrow.type_parameters,
                    Some(Node::FunctionExpression(function)) => function.type_parameters,
                    _ => return,
                }
            }
            _ => return,
        };
        // A generic constructor of its own would supply the signature's type
        // parameters instead of the class's, so a class is only asked when its
        // constructors declare none.
        if let Some(Node::ClassDeclaration(class)) = self.node_map.get(*declaration)
            && class.members.iter().any(|member| {
                matches!(member, tsr_ast::ClassElement::ConstructorDeclaration(constructor)
                    if !constructor.type_parameters.is_empty())
            })
        {
            return;
        }
        // **A non-generic callee is `Expected 0 type arguments, but got N`**,
        // which is the message's own wording and what
        // `callNonGenericFunctionWithTypeArguments` is about — its header reads
        // *"it is always illegal to provide type arguments to a non-generic
        // function"*. The guard that stood here returned on `maximum == 0` with
        // no comment, no anchor and no measurement, while every other guard in
        // this function carries a reason; `written >= 0 && written <= 0` below
        // is exactly the test that should decide it. §942.
        let maximum = parameters.len();
        let minimum = parameters
            .iter()
            .position(|parameter| parameter.default_type.is_some())
            .unwrap_or(maximum);
        let written = type_arguments.len();
        if written >= minimum && written <= maximum {
            return;
        }
        let Some(argument) = first.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(argument);
        let expected =
            if minimum < maximum { format!("{minimum}-{maximum}") } else { minimum.to_string() };
        self.report(
            file,
            Diagnostic::with_args(
                &messages::EXPECTED_0_TYPE_ARGUMENTS_BUT_GOT_1,
                span,
                [expected, written.to_string()],
            ),
        );
    }

    /// `(getMinTypeArgumentCount(typeParameters), len(typeParameters))` for the
    /// class or interface an entity name refers to.
    ///
    /// `getMinTypeArgumentCount` (`checker.go`) counts the parameters up to the
    /// first one carrying a default: a parameter with a default may be omitted,
    /// and so may every parameter after it.
    fn declared_type_parameter_arity(&mut self, name: NodeId) -> Option<(usize, usize)> {
        // **`M.E` is a `QualifiedName`, not an `Identifier`.** §942 built this
        // file for call-site arity and every fixture it had used a bare name,
        // so a namespace-qualified generic type declined here — the comment
        // that stood in this place called it *"a different failure surface"*
        // and it is two `resolve_name` calls. §1037.
        let symbol = match self.node_map.get(name)? {
            Node::Identifier(identifier) => self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name,
                identifier.text,
                SymbolFlags::TYPE,
            )?,
            Node::QualifiedName(_) => self.qualified_type_name_symbol(name)?,
            _ => return None,
        };
        let mut symbol = self.binder.merged_symbol(symbol);
        // **An alias carries none of the countable kinds.** `import a =
        // require('./m')` where the module is `export = C<T>` answers `None`
        // here and silences the whole ladder, so the chain is followed to its
        // target — the same walk §692 measured, through
        // `qualified_alias_target` when `resolve_alias` declines for the
        // printer's sake (§686), and unresolvable is `None`. §710.
        for _ in 0..8u8 {
            if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
                break;
            }
            let target =
                self.resolve_alias(symbol).or_else(|| self.qualified_alias_target(symbol))?;
            symbol = self.binder.merged_symbol(target);
        }
        let entry = self.binder.symbols().get(symbol);
        // `checkNoTypeArguments` (`checker.go:23220`) asks nothing about the
        // kind at all — **any** symbol with no type parameters and a written
        // argument list is TS2315, a type parameter included: `function f<U>()
        // { var v: U<string> }` is upstream's own fixture line. So the kinds
        // here are a list of what this helper can *count*, and every one of
        // them that declares no list answers `(0, 0)` rather than refusing.
        // §683.
        if !entry.flags.intersects(
            SymbolFlags::CLASS
                | SymbolFlags::INTERFACE
                | SymbolFlags::TYPE_ALIAS
                | SymbolFlags::TYPE_PARAMETER
                | SymbolFlags::REGULAR_ENUM
                | SymbolFlags::CONST_ENUM,
        ) {
            return None;
        }
        let mut arity: Option<(usize, usize)> = None;
        for declaration in &entry.declarations {
            let parameters = match self.node_map.get(*declaration) {
                Some(Node::ClassDeclaration(class)) => class.type_parameters,
                Some(Node::ClassExpression(class)) => class.type_parameters,
                Some(Node::InterfaceDeclaration(interface)) => interface.type_parameters,
                Some(Node::TypeAliasDeclaration(alias)) => alias.type_parameters,
                // Neither an enum nor a type parameter declares a parameter
                // list, so each answers `(0, 0)` — an answer, not a refusal.
                // §681, §683.
                Some(Node::EnumDeclaration(_) | Node::TypeParameterDeclaration(_)) => &[],
                // **"I have nothing to add" is not "nobody can answer."** A
                // merged *value* declaration contributes no type parameters, so
                // it is skipped rather than treated as evidence the answer is
                // unknown. `Array` is the standard library merge — `interface
                // Array<T>` and `declare var Array: ArrayConstructor` — and
                // returning `None` here made every bare reference to a lib
                // generic silent. The parameters are read off the *type*
                // declarations, and the check below requires there to be one.
                // §364.
                Some(
                    Node::VariableDeclaration(_)
                    | Node::FunctionDeclaration(_)
                    | Node::ModuleDeclaration(_),
                ) => continue,
                _ => return None,
            };
            let maximum = parameters.len();
            let minimum = parameters
                .iter()
                .position(|parameter| parameter.default_type.is_some())
                .unwrap_or(maximum);
            match arity {
                // Every declaration of one interface must repeat the same type
                // parameter list; if this port sees two that disagree, it is
                // seeing something upstream would have merged differently.
                Some(seen) if seen != (minimum, maximum) => return None,
                _ => arity = Some((minimum, maximum)),
            }
        }
        arity
    }

    /// The written text of a type reference's name, for the message argument.
    ///
    /// Named apart from `declared::entity_name_text`, which answers a different
    /// question (the printed *chain*) and lives on the same impl.
    fn written_type_name(&self, name: NodeId) -> String {
        match self.node_map.get(name) {
            Some(Node::Identifier(identifier)) => identifier.text.to_string(),
            _ => String::new(),
        }
    }

    /// `M.E` in a type position: resolve the left as a namespace, take the
    /// right from its exports.
    ///
    /// Deliberately not `qualified_member_of_namespace`, which walks **property
    /// access expressions** — a different node with different fields. §1030's
    /// lesson: the walk is not the thing. §1037.
    fn qualified_type_name_symbol(&mut self, name: NodeId) -> Option<tsr_binder::SymbolId> {
        let Node::QualifiedName(qualified) = self.node_map.get(name)? else { return None };
        let left = qualified.left?.node_id()?;
        let right = qualified.right?.node_id?;
        let left_text = self.identifier_text(left)?.to_string();
        let member = self.identifier_text(right)?.to_string();
        let namespace = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            left,
            &left_text,
            SymbolFlags::NAMESPACE_MODULE | SymbolFlags::TYPE,
        )?;
        let namespace = self.binder.merged_symbol(namespace);
        let exported = *self.binder.symbols().get(namespace).exports.get(member.as_str())?;
        Some(self.binder.merged_symbol(exported))
    }
}
