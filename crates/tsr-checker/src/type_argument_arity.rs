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
//! | a `.js` class or interface reference | the JS leg: `noImplicitAny` off reports nothing, and a heritage reference asks for an `@extends` tag (TS8026/TS8027) — [`Checker::check_js_type_argument_arity`] |
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
                if reference.type_arguments.is_empty() {
                    self.jsdoc_augments_type_arguments(node).map_or(0, <[_]>::len)
                } else {
                    reference.type_arguments.len()
                },
            ),
            _ => return,
        };
        let Some(name) = name else { return };
        // `getTypeFromTypeReference` asks `getIntendedTypeFromJSDocTypeReference`
        // first; its `Object.<K, V>` arm answers without this rule.
        if self.is_jsdoc_record_object_reference(node) {
            return;
        }
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
        // A **class's** `extends` entry is a value: `resolveBaseTypesOfClass`
        // (`checker.go:19220`) reaches `getTypeFromClassOrInterfaceReference`
        // only when the base constructor's symbol is a class; any other
        // constructor value (`extends Array`, `extends Mup` for a declared
        // `var Mup: MupConstructor`) is instantiated through its construct
        // signatures, where this arity rule does not apply.
        if self.nodes.kind(node) == SyntaxKind::ExpressionWithTypeArguments
            && self.is_class_extends_entry(node)
            && !self.class_extends_entry_names_a_class(name)
        {
            return;
        }
        let Some((minimum, maximum, symbol)) = self.declared_type_parameter_arity(name) else {
            return;
        };
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
        if self.in_js_file(node)
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
        {
            self.check_js_type_argument_arity(node, symbol, minimum, maximum);
            return;
        }
        let message = if minimum < maximum {
            &messages::GENERIC_TYPE_0_REQUIRES_BETWEEN_1_AND_2_TYPE_ARGUMENTS
        } else {
            &messages::GENERIC_TYPE_0_REQUIRES_1_TYPE_ARGUMENT_S
        };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        // `typeStr`: the declared type, `C<T>`, not the written name.
        let declared = self.get_declared_type_of_symbol(symbol);
        let printed = self.type_to_string(declared);
        self.report(
            file,
            Diagnostic::with_args(
                message,
                span,
                [printed, minimum.to_string(), maximum.to_string()],
            ),
        );
    }
}

impl<'a> Checker<'a, '_> {
    /// The type arguments `reparseHosted`'s `KindJSDocAugmentsTag` arm
    /// (`parser/reparser.go`) copies onto a JS class's single `extends`
    /// reference that writes none: the first `@augments`/`@extends` tag whose
    /// class name has the same property-access name
    /// (`ast.HasSamePropertyAccessName`) and writes type arguments.
    ///
    /// This port keeps JSDoc in a side table instead of mutating the tree, so
    /// consumers of the heritage reference's arguments read them here.
    pub(crate) fn jsdoc_augments_type_arguments(
        &self,
        reference: NodeId,
    ) -> Option<&'a [tsr_ast::TypeNode<'a>]> {
        if !self.in_js_file(reference) {
            return None;
        }
        let Some(Node::ExpressionWithTypeArguments(target)) = self.node_map.get(reference) else {
            return None;
        };
        let clause = self.nodes.parent(reference)?;
        let Some(Node::HeritageClause(heritage)) = self.node_map.get(clause) else {
            return None;
        };
        if heritage.token.kind != SyntaxKind::ExtendsKeyword || heritage.types.len() != 1 {
            return None;
        }
        let class = self.nodes.parent(clause)?;
        for doc in self.jsdoc_entries.get(&class).copied().unwrap_or_default() {
            for tag in doc.tags {
                if let tsr_ast::JSDocTag::JSDocAugmentsTag(tag) = tag
                    && let Some(source) = tag.class_name
                    && let (Some(left), Some(right)) = (target.expression, source.expression)
                    && has_same_property_access_name(left, right)
                    && !source.type_arguments.is_empty()
                {
                    return Some(source.type_arguments);
                }
            }
        }
        None
    }
}

impl Checker<'_, '_> {
    /// The JS leg of `getTypeFromClassOrInterfaceReference`'s arity arm
    /// (`checker.go:23169`): `isJsImplicitAny` reports nothing, and a heritage
    /// `ExpressionWithTypeArguments` outside an `@augments` tag asks for an
    /// `@extends` tag (TS8026/TS8027) instead of TS2314/TS2707.
    ///
    /// Upstream passes `typeStr` — the declared type printed with
    /// `TypeFormatFlagsWriteArrayAsGenericType` — as `{0}` and the counts as
    /// `{1}`/`{2}`, so the message reads `Expected Foo<T> type arguments`; the
    /// arguments are kept in that order.
    fn check_js_type_argument_arity(
        &mut self,
        node: NodeId,
        symbol: tsr_binder::SymbolId,
        minimum: usize,
        maximum: usize,
    ) {
        if !self.no_implicit_any {
            return;
        }
        // Every heritage reference reaching here is an `ExpressionWithTypeArguments`
        // under a `HeritageClause`; an `@augments` tag's own reference is not.
        let missing_augments_tag = self.nodes.kind(node) == SyntaxKind::ExpressionWithTypeArguments;
        let message = match (missing_augments_tag, minimum < maximum) {
            (true, false) => &messages::EXPECTED_0_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG,
            (true, true) => {
                &messages::EXPECTED_0_1_TYPE_ARGUMENTS_PROVIDE_THESE_WITH_AN_EXTENDS_TAG
            }
            (false, false) => &messages::GENERIC_TYPE_0_REQUIRES_1_TYPE_ARGUMENT_S,
            (false, true) => &messages::GENERIC_TYPE_0_REQUIRES_BETWEEN_1_AND_2_TYPE_ARGUMENTS,
        };
        let declared = self.get_declared_type_of_symbol(symbol);
        let printed = self.type_to_string(declared);
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
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
        if self.in_js_file(node) {
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
    fn declared_type_parameter_arity(
        &mut self,
        name: NodeId,
    ) -> Option<(usize, usize, tsr_binder::SymbolId)> {
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
            Node::QualifiedName(_) | Node::PropertyAccessExpression(_) => {
                self.qualified_type_name_symbol(name)?
            }
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
        let declarations = entry.declarations.clone();
        for declaration in &declarations {
            let reparsed;
            let parameters = match self.node_map.get(*declaration) {
                // An unparameterised JS class takes its `@template` tags'
                // parameters (`reparseHosted`, `parser/reparser.go:459`).
                Some(Node::ClassDeclaration(class)) if class.type_parameters.is_empty() => {
                    reparsed = self.jsdoc_class_template_parameters(*declaration);
                    &reparsed[..]
                }
                Some(Node::ClassExpression(class)) if class.type_parameters.is_empty() => {
                    reparsed = self.jsdoc_class_template_parameters(*declaration);
                    &reparsed[..]
                }
                Some(Node::ClassDeclaration(class)) => class.type_parameters,
                Some(Node::ClassExpression(class)) => class.type_parameters,
                Some(Node::InterfaceDeclaration(interface)) => interface.type_parameters,
                Some(Node::TypeAliasDeclaration(alias)) => alias.type_parameters,
                // A `@typedef`/`@callback` is a reparsed `JSTypeAliasDeclaration`
                // whose parameters are its comment's `@template` tags
                // (`gatherTypeParameters(jsDoc, true)`, `parser/reparser.go:293`).
                Some(Node::JSDocTypedefTag(_) | Node::JSDocCallbackTag(_)) => {
                    reparsed = self.local_type_parameters_of(symbol).into_owned();
                    &reparsed[..]
                }
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
        arity.map(|(minimum, maximum)| (minimum, maximum, symbol))
    }

    /// Is `node` an entry of a class declaration's or expression's `extends`
    /// clause?
    pub(crate) fn is_class_extends_entry(&self, node: NodeId) -> bool {
        let Some(clause) = self.nodes.parent(node) else { return false };
        let Some(Node::HeritageClause(heritage)) = self.node_map.get(clause) else {
            return false;
        };
        heritage.token.kind == SyntaxKind::ExtendsKeyword
            && self.nodes.parent(clause).is_some_and(|class| {
                matches!(
                    self.nodes.kind(class),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            })
    }

    /// Does a class `extends` expression resolve, as a value, to a class
    /// symbol? An identifier with no value answers `false`; a
    /// non-identifier expression answers `true`, which leaves the existing
    /// type-side resolution to decide.
    pub(crate) fn class_extends_entry_names_a_class(&mut self, name: NodeId) -> bool {
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return true };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            // No value: `checkExpression` reports it (TS2304/TS2689) and the
            // base constructor type is the error type, so
            // `resolveBaseTypesOfClass` never reaches the arity arm.
            return false;
        };
        let mut symbol = self.binder.merged_symbol(symbol);
        for _ in 0..8u8 {
            if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
                break;
            }
            let Some(target) = self.resolve_alias(symbol) else { return true };
            symbol = self.binder.merged_symbol(target);
        }
        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::CLASS)
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
        // **Three node kinds, one question.** `M.E` is a `QualifiedName` in the
        // *type* grammar and a `PropertyAccessExpression` in the *expression*
        // grammar — a heritage clause's base uses the second. The port needs
        // both arms because the grammar chose the kind, not the meaning. §1039.
        let (left, right) = match self.node_map.get(name)? {
            Node::QualifiedName(qualified) => {
                (qualified.left?.node_id()?, qualified.right?.node_id?)
            }
            Node::PropertyAccessExpression(access) => {
                (access.expression?.node_id()?, access.name?.node_id()?)
            }
            _ => return None,
        };
        let left_text = self.identifier_text(left)?.to_string();
        let member = self.identifier_text(right)?.to_string();
        let namespace = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            left,
            &left_text,
            // **Not `| VALUE`.** Widening the meaning does resolve the
            // namespace — `have` reaches `want` at 174 — but five of the eight
            // lines it adds are wrong and `extraonly` rises by one, with **no
            // case moving either way**. Measured and declined; §1040.
            SymbolFlags::NAMESPACE,
        )?;
        let namespace = self.binder.merged_symbol(namespace);
        let exported = *self.binder.symbols().get(namespace).exports.get(member.as_str())?;
        Some(self.binder.merged_symbol(exported))
    }
}

/// Ported from typescript-go's `ast.HasSamePropertyAccessName`
/// (`internal/ast/utilities.go`).
fn has_same_property_access_name(
    left: tsr_ast::Expression<'_>,
    right: tsr_ast::Expression<'_>,
) -> bool {
    use tsr_ast::{Expression, MemberName};
    match (left, right) {
        (Expression::Identifier(left), Expression::Identifier(right)) => left.text == right.text,
        (
            Expression::PropertyAccessExpression(left),
            Expression::PropertyAccessExpression(right),
        ) => {
            let same_name = match (left.name, right.name) {
                (Some(MemberName::Identifier(l)), Some(MemberName::Identifier(r))) => {
                    l.text == r.text
                }
                (
                    Some(MemberName::PrivateIdentifier(l)),
                    Some(MemberName::PrivateIdentifier(r)),
                ) => l.text == r.text,
                _ => false,
            };
            same_name
                && matches!((left.expression, right.expression), (Some(l), Some(r)) if has_same_property_access_name(l, r))
        }
        _ => false,
    }
}
