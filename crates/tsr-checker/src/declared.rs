//! Type nodes, and what a *type* symbol declares.
//!
//! Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`) and
//! `Checker.getDeclaredTypeOfSymbol` (`checker.go:23670`). These are one module
//! because they call each other on every type reference: a `TypeReferenceNode`
//! resolves a name and asks the symbol what it declares.
//!
//! Not to be confused with [`crate::symbols`], which answers what type a
//! *value* symbol has.

use tsr_ast::{Expression, Node, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{checker::Checker, flags::TypeFlags, resolution::PropertyName, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// The type a type node denotes.
    ///
    /// Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`),
    /// restricted to the forms this slice covers: the keyword types, literal
    /// types, and parenthesised types. Everything else — type references, arrays,
    /// unions, intersections, conditionals, mapped types, and the rest — yields
    /// `errorType`, because none of those type *shapes* exists yet.
    pub fn get_type_from_type_node(&mut self, node: TypeNode<'a>) -> TypeId {
        match node {
            TypeNode::KeywordTypeNode(keyword) => match keyword.kind {
                SyntaxKind::AnyKeyword => self.intrinsics.any,
                SyntaxKind::UnknownKeyword => self.intrinsics.unknown,
                SyntaxKind::StringKeyword => self.intrinsics.string,
                SyntaxKind::NumberKeyword => self.intrinsics.number,
                SyntaxKind::BigIntKeyword => self.intrinsics.bigint,
                SyntaxKind::BooleanKeyword => self.intrinsics.boolean,
                SyntaxKind::SymbolKeyword => self.intrinsics.es_symbol,
                SyntaxKind::VoidKeyword => self.intrinsics.void,
                SyntaxKind::UndefinedKeyword => self.intrinsics.undefined,
                SyntaxKind::NeverKeyword => self.intrinsics.never,
                SyntaxKind::ObjectKeyword => self.intrinsics.non_primitive,
                _ => self.intrinsics.error,
            },
            // `Checker.getTypeFromLiteralTypeNode`: the literal's type, made
            // **regular**. A literal in a type position is not fresh, which is
            // what keeps `let x: "a"` from widening to `string`.
            TypeNode::LiteralTypeNode(literal) => {
                // `literal` is a `Node`, not an `Expression`: a literal type's
                // payload can be `null`, or a prefixed `-1`, which are not the
                // same alias. Anything that is not an expression we can type is
                // an unported form.
                let Some(node) = literal.literal else { return self.intrinsics.error };
                let Ok(expression) = Expression::try_from(node) else {
                    return self.intrinsics.error;
                };
                let id = self.check_expression(expression);
                self.get_regular_type_of_literal_type(id)
            }
            TypeNode::ParenthesizedTypeNode(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            TypeNode::TypeReferenceNode(node) => self.get_type_from_type_reference(node),
            TypeNode::TypeLiteralNode(node) => self.get_type_from_type_literal(node),
            TypeNode::UnionTypeNode(node) => self.get_type_from_union_type_node(node),
            TypeNode::IntersectionTypeNode(node) => self.get_type_from_intersection_type_node(node),
            _ => self.intrinsics.error,
        }
    }

    /// Ported from `Checker.getTypeFromTypeReference` into
    /// `getTypeReferenceType` (`checker.go:23146`).
    ///
    /// Resolves the name and asks the symbol what type it declares. Two things
    /// are deliberately left as gaps rather than approximated:
    ///
    /// - **A qualified name** (`M.I`) needs `resolveEntityName` walking module
    ///   exports, which the binder does not expose yet.
    /// - **Type arguments** (`C<number>`) need instantiation, the machinery
    ///   upstream guards with a depth of 100 and a count of 5 million
    ///   (`checker.go:22111`, `bd tsr-el3.2`). Half of it — substituting names
    ///   without the guards — is exactly the kind of port that works on the
    ///   corpus and hangs on a real program.
    fn get_type_from_type_reference(&mut self, node: &tsr_ast::TypeReferenceNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(tsr_ast::EntityName::Identifier(name)) = node.type_name else {
            return error;
        };
        let Some(id) = name.node_id else { return error };
        // `SymbolFlags::TYPE` is upstream's meaning for a type reference
        // (`resolveTypeReferenceName`). It is what lets the resolver consult an
        // enclosing class's or interface's `members` for a type parameter — see
        // `BindResult::resolve_name`.
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, id, name.text, SymbolFlags::TYPE)
        else {
            return error;
        };
        let parameters = self.local_type_parameters_of(symbol).len();
        if parameters == 0 {
            // `checkNoTypeArguments` (`checker.go:23157`): arguments on a type
            // that takes none is an error, and answering the bare declared type
            // would quietly drop them.
            if !node.type_arguments.is_empty() {
                return error;
            }
            let declared = self.get_declared_type_of_symbol(symbol);
            return self.get_regular_type_of_literal_type(declared);
        }
        self.get_instantiated_type_reference(node, symbol, parameters)
    }

    /// Ported from `Checker.getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`
    /// (`checker.go`), for the type-literal half only.
    ///
    /// # An anonymous object type, printed structurally
    ///
    /// `{ a: string }` has no symbol to be named by, so unlike a class or an
    /// interface it prints its members: `{ a: string; }`, with the trailing
    /// semicolon and the surrounding spaces upstream\'s printer emits, and `{}`
    /// when there are none. That form is not a style choice — it is compared
    /// character for character against 26,686 corpus lines.
    ///
    /// # Any member this port cannot render makes the whole type a gap
    ///
    /// Methods, call and construct signatures, index signatures, accessors and
    /// computed names all remain unported. A literal containing one answers
    /// `errorType` rather than printing the members it *does* understand: a
    /// partial object type is a wrong answer that looks like a right one, and
    /// it would score as a mismatch either way. The same applies to a member
    /// whose own type is a gap.
    fn get_type_from_type_literal(&mut self, node: &tsr_ast::TypeLiteralNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let mut members = Vec::with_capacity(node.members.len());
        for member in node.members {
            let tsr_ast::TypeElement::PropertySignatureDeclaration(property) = member else {
                return error;
            };
            let tsr_ast::PropertyName::Identifier(name) = property.name else {
                return error;
            };
            let Some(annotation) = property.r#type else { return error };
            let member_type = self.get_type_from_type_node(annotation);
            if member_type == error {
                return error;
            }
            // `?` on a property signature; `!` cannot appear on one, so the
            // token\'s presence is enough to distinguish it.
            let optional =
                property.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken);
            let readonly = property.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(m) if m.kind == SyntaxKind::ReadonlyKeyword)
            });
            members.push(crate::objects::Member {
                name: name.text.to_string(),
                optional,
                readonly,
                printed: self.type_to_string(member_type),
            });
        }
        // Shared with `checkObjectLiteral`, so the two structural renderers
        // cannot drift apart — see `crate::objects::render_object_type`.
        let printed = crate::objects::render_object_type(&members);
        // The binder gives a type literal its own anonymous `__type` symbol,
        // whose members table is where a property access on this type looks.
        let members = node.node_id.and_then(|id| self.binder.symbol_of(id));
        self.store.new_named(TypeFlags::OBJECT, printed, members)
    }

    /// Ported from `Checker.getTypeFromUnionTypeNode` (`checker.go:24209`).
    ///
    /// The constituents in source order, then [`Checker::get_union_type`], which
    /// sorts, deduplicates and reduces them. `type T = number | string` prints
    /// `string | number`: source order is not the answer.
    ///
    /// # A gap in a constituent is a gap in the union — upstream's own rule
    ///
    /// `A | Unported` is not `A | any`, which is the call this port already made
    /// for a generic reference's type arguments. Here **no deviation is needed**:
    /// `errorType` carries `TypeFlagsAny`, so `getUnionTypeWorker` reduces any
    /// union containing one to `errorType` (`checker.go:25659`). An early return
    /// guarding the constituent loop was written first and then removed — it
    /// could not be made observable, because the reduction answers identically.
    ///
    /// # The alias
    ///
    /// `getAliasForTypeNode` (`checker.go:23711`) attaches the enclosing type
    /// alias's symbol to the union, which is what makes `type T8 = string | boolean`
    /// record `>T8 : T8` while `var x8: string | boolean` records the constituents
    /// (`baselines/reference/submodule/conformance/typeAliases.types:76`). A
    /// *generic* alias would print its type arguments too — `Tree<T>` — so it is
    /// a gap rather than a `Tree` that drops them.
    fn get_type_from_union_type_node(&mut self, node: &tsr_ast::UnionTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let types = node
            .types
            .iter()
            .map(|constituent| self.get_type_from_type_node(*constituent))
            .collect::<Vec<_>>();
        match node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)) {
            None => self.get_union_type(&types),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                self.get_named_union_type(&types, TypeFlags::empty(), alias)
            }
            Some(_) => error,
        }
    }

    /// Ported from `Checker.getTypeFromIntersectionTypeNode` (`checker.go:24218`).
    ///
    /// The constituents in **source order**, which unlike a union's is the order
    /// they are printed in — see [`crate::intersections`]. Everything else
    /// mirrors the union node: a gap in a constituent is a gap in the whole
    /// (`errorType` carries `ANY`, so upstream's own reduction answers
    /// `errorType` at `checker.go:26092`), and an enclosing non-generic type
    /// alias names the result.
    fn get_type_from_intersection_type_node(
        &mut self,
        node: &tsr_ast::IntersectionTypeNode<'a>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let types = node
            .types
            .iter()
            .map(|constituent| self.get_type_from_type_node(*constituent))
            .collect::<Vec<_>>();
        match node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)) {
            None => self.get_intersection_type(&types, None),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                self.get_intersection_type(&types, Some(alias))
            }
            Some(_) => error,
        }
    }

    /// Ported from `Checker.getAliasSymbolForTypeNode` (`checker.go:23719`).
    ///
    /// The host is the nearest ancestor that is not a parenthesised type or a
    /// `readonly` type operator, and it names the type only when it is a type
    /// alias declaration.
    fn alias_symbol_for_type_node(&self, node: tsr_ast::NodeId) -> Option<SymbolId> {
        let mut host = self.nodes.parent(node)?;
        loop {
            let kind = self.nodes.kind(host);
            let transparent = kind == SyntaxKind::ParenthesizedType
                || kind == SyntaxKind::TypeOperator
                    && matches!(
                        self.node_map.get(host),
                        Some(Node::TypeOperatorNode(operator))
                            if operator.operator.kind == SyntaxKind::ReadonlyKeyword
                    );
            if !transparent {
                break;
            }
            host = self.nodes.parent(host)?;
        }
        if self.nodes.kind(host) == SyntaxKind::TypeAliasDeclaration {
            self.binder.symbol_of(host)
        } else {
            None
        }
    }

    /// A reference to a generic type: `C<number>`, `Tree<T>`.
    ///
    /// Ported from `getTypeFromClassOrInterfaceReference` and
    /// `getTypeFromTypeAliasReference` (`checker.go:23168`, `:23222`), reduced to
    /// what a printed line needs — the target and its arguments — and interned
    /// on that pair so `C<number>` written twice is one type.
    ///
    /// # There is no substitution here, and therefore no depth limit
    ///
    /// Upstream instantiates by *substituting* the arguments through the target's
    /// members, and guards that with an instantiation depth of 100 and a count of
    /// 5,000,000 (`checker.go:22111`) because self-referential generics generate
    /// new type identities forever. This port has no members to substitute into
    /// (see `getDeclaredTypeOfClassOrInterface`), so the recursion those limits
    /// exist to stop does not happen yet: the only recursion is over the *source*
    /// nesting of the argument type nodes, which is finite in a parsed file.
    ///
    /// **The limits are therefore not ported here, deliberately, and they are not
    /// optional later.** `bd tsr-el3.2` records that they belong with
    /// `instantiateType` — the code that actually recurses — and porting them now
    /// would be a guard around a loop that does not exist, which reads as
    /// coverage and provides none.
    ///
    /// # Arity
    ///
    /// Upstream reports and answers `errorType` when the count is outside
    /// `[minTypeArgumentCount, len(typeParameters)]` (`checker.go:23189`). Fewer
    /// arguments than parameters *within* that range is legal and fills from the
    /// parameters\' defaults (`fillMissingTypeArguments`), and a default may
    /// reference an earlier parameter — which is substitution, so that case is a
    /// gap rather than a guess.
    fn get_instantiated_type_reference(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        symbol: SymbolId,
        parameters: usize,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if node.type_arguments.len() != parameters {
            return error;
        }
        let mut arguments = Vec::with_capacity(parameters);
        for argument in node.type_arguments {
            let resolved = self.get_type_from_type_node(*argument);
            // A gap in an argument is a gap in the reference: `C<Unported>` is
            // not `C<any>`, and printing it as though the argument were known
            // would be a wrong line rather than a missing one.
            if resolved == error {
                return error;
            }
            arguments.push(resolved);
        }
        if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
            return cached;
        }
        let name = self.binder.symbols().get(symbol).name.to_string();
        let printed = arguments
            .iter()
            .map(|&argument| self.type_to_string(argument))
            .collect::<Vec<_>>()
            .join(", ");
        // `OBJECT` even when the target is a type alias, where upstream\'s
        // instantiated type carries the flags of the alias\'s *body*. The flags
        // are consulted by the arithmetic and `+` arms, and claiming
        // `Alias<number>` is string- or number-like would be worse than claiming
        // it is an object: `object` is the one answer those arms treat as
        // neither.
        // No members: see `TypeData::Named`. `C<number>`'s properties would be
        // `C`'s uninstantiated ones, so `c.a` would answer `T` where upstream
        // answers `number`.
        //
        // **This is blunter than upstream and it costs answers.** A member whose
        // type does not mention a type parameter — `class C<T> { a: string }` —
        // is the same before and after instantiation, and upstream answers it;
        // this gaps it. The correct rule is `couldContainTypeVariables`, which
        // arrives with real instantiation. There is deliberately **no test
        // pinning the current answer**, because the current answer is the worse
        // of the two and a test would cement it — and the obvious test cannot
        // tell the two apart anyway while `bd tsr-y4u.21` keeps a class's type
        // parameters out of every scope.
        let id = self.store.new_named(TypeFlags::OBJECT, format!("{name}<{printed}>"), None);
        self.instantiations.insert((symbol, arguments), id);
        id
    }

    /// The type a *type* symbol declares.
    ///
    /// Ported from `Checker.getDeclaredTypeOfSymbol` / `tryGetDeclaredTypeOfSymbol`
    /// (`checker.go:23670`), in upstream's dispatch order. Enum members and
    /// aliases (`import X = ...`) are unported and answer `errorType`.
    ///
    /// **This is not `getTypeOfSymbol`.** A class `C` *declares* the instance
    /// type `C` and *has* the type `typeof C`; asking the wrong one is how a
    /// baseline line ends up plausible and wrong.
    pub fn get_declared_type_of_symbol(&mut self, symbol: SymbolId) -> TypeId {
        if let Some(&cached) = self.declared_types.get(&symbol) {
            return cached;
        }
        let flags = self.binder.symbols().get(symbol).flags;
        let computed = if flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            self.get_declared_type_of_class_or_interface(symbol)
        } else if flags.contains(SymbolFlags::TYPE_PARAMETER) {
            self.new_named_type(symbol, TypeFlags::TYPE_PARAMETER, false)
        } else if flags.contains(SymbolFlags::TYPE_ALIAS) {
            self.get_declared_type_of_type_alias(symbol)
        } else if flags.intersects(SymbolFlags::ENUM) {
            self.get_declared_type_of_enum(symbol)
        } else {
            self.intrinsics.error
        };
        self.declared_types.insert(symbol, computed);
        computed
    }

    /// Ported from `Checker.getDeclaredTypeOfEnum` (`checker.go:23874`).
    ///
    /// **This replaces a divergence rather than extending it.** Until unions
    /// existed, an enum's declared type here was a named type that printed the
    /// enum's name and had none of a union's behaviour. It is now what upstream
    /// builds: the union of the members' types, printing as the enum's name
    /// because the node builder renders an enum-like type from its symbol
    /// (`nodebuilderimpl.go:3260`) rather than from its constituents.
    ///
    /// # The member values are not evaluated, and that is the remaining gap
    ///
    /// Upstream asks `getEnumMemberValue` for each member and builds an
    /// *enum literal* type from the value — `getEnumLiteralType`
    /// (`checker.go:25362`) — falling back to `createComputedEnumType` when the
    /// evaluator cannot produce a constant. This port has no constant evaluator
    /// (`bd tsr-8pz`), so **every** member takes the fallback: a distinct type per member
    /// symbol, flagged `ENUM`, printing `E.A`.
    ///
    /// The consequence is narrow and worth stating: the enum type's *printed*
    /// form, its constituent count and its per-member identities are all
    /// upstream's, and two members that share a value are two types here where
    /// upstream interns them into one. Nothing observable depends on that yet
    /// because nothing compares enum members for value equality.
    fn get_declared_type_of_enum(&mut self, symbol: SymbolId) -> TypeId {
        let declarations =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect::<Vec<_>>();
        let name = self.binder.symbols().get(symbol).name.to_string();
        let mut members = Vec::new();
        for declaration in declarations {
            let Some(Node::EnumDeclaration(node)) = self.node_map.get(declaration) else {
                continue;
            };
            for member in node.members {
                // `hasBindableName` (`checker.go:23879`), asked of the binder: a
                // member the binder gave no symbol to is one whose name is a
                // non-literal computed expression, and upstream skips it too.
                let Some(member_symbol) = member.node_id.and_then(|id| self.binder.symbol_of(id))
                else {
                    continue;
                };
                let member_name = self.binder.symbols().get(member_symbol).name.to_string();
                let member_type =
                    self.store.new_named(TypeFlags::ENUM, format!("{name}.{member_name}"), None);
                // `checker.go:23890`: the member's own declared type is the
                // *fresh* form of its literal type. Currently unobservable —
                // reaching it needs `E.A` in type position, which is a qualified
                // name and unported — and written because it is upstream's line
                // and because it is what keeps the member types created once.
                let fresh = self.get_fresh_type_of_literal_type(member_type);
                self.declared_types.insert(member_symbol, fresh);
                members.push(member_type);
            }
        }
        if members.is_empty() {
            // `createComputedEnumType(symbol)` (`checker.go:23901`): an enum with
            // no bindable members is not a union at all.
            return self.new_named_type(symbol, TypeFlags::ENUM, false);
        }
        // `checker.go:23904`: a union enum type carries `ENUM_LITERAL` and the
        // enum's symbol, which is what it prints as.
        self.get_named_union_type(&members, TypeFlags::ENUM_LITERAL, symbol)
    }

    /// Ported from `Checker.getDeclaredTypeOfClassOrInterface`
    /// (`checker.go:17319`).
    ///
    /// Upstream builds an object type with members, base types and a `this`
    /// type. This builds the *identity* and the printed form only: one type per
    /// symbol, printing `C` or `C<T>`. Members are `bd tsr-4sc.7`\'s second
    /// slice and nothing here depends on them, because no relation is computed
    /// yet — a type this port cannot look inside is still the right answer to
    /// "what type is this".
    fn get_declared_type_of_class_or_interface(&mut self, symbol: SymbolId) -> TypeId {
        self.new_named_type(symbol, TypeFlags::OBJECT, true)
    }

    /// Ported from `Checker.getDeclaredTypeOfTypeAlias` (`checker.go:23837`).
    ///
    /// A type alias is **transparent**: `type T = number` declares `number`, and
    /// upstream\'s baselines print it that way — `var x: T` reads `>x : number`
    /// (`conformance/typeAliases.types`). The alias name survives in the printed
    /// form only for *generic* aliases, which are a gap here.
    ///
    /// The circularity guard is upstream\'s and is not optional: `type T = T`
    /// resolves through this function forever without it, and the corpus
    /// contains such cases deliberately.
    fn get_declared_type_of_type_alias(&mut self, symbol: SymbolId) -> TypeId {
        let error = self.intrinsics.error;
        // A generic alias keeps its own name in the printed form:
        // `type Tree<T> = T | { left: Tree<T> }` records `>Tree : Tree<T>`
        // (`conformance/genericTypeAliases.types`), because upstream\'s declared
        // type for one is the body instantiated with the alias\'s own parameters
        // and carrying it as an alias symbol. The body is not expanded here —
        // which is also why a self-referential alias like `Tree` terminates
        // rather than needing the guard below.
        let parameters = self.local_type_parameter_names_of(symbol);
        if !parameters.is_empty() {
            let name = self.binder.symbols().get(symbol).name.to_string();
            return self.store.new_named(
                TypeFlags::OBJECT,
                format!("{name}<{}>", parameters.join(", ")),
                None,
            );
        }
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return error;
        };
        let Some(annotation) = self.node_map.get(declaration).and_then(|node| node.type_id())
        else {
            return error;
        };
        let Some(type_node) =
            self.node_map.get(annotation).and_then(|n| TypeNode::try_from(n).ok())
        else {
            return error;
        };
        if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
            return error;
        }
        let resolved = self.get_type_from_type_node(type_node);
        if !self.resolutions.pop() {
            // A cycle closed below this frame, so the answer above was built on a
            // partial one. Upstream reports "Type alias 0 circularly references
            // itself" and answers `errorType`; the diagnostic is `bd tsr-5e7.6`.
            return error;
        }
        resolved
    }

    /// A named type for `symbol`, printed as `C` or `C<T, U>`.
    ///
    /// `with_type_parameters` is upstream\'s distinction between a type that can
    /// be generic and one that cannot: an enum or a type parameter never carries
    /// type parameters of its own, and asking for them would print `E<T>` for an
    /// enum declared inside a generic class.
    fn new_named_type(
        &mut self,
        symbol: SymbolId,
        flags: TypeFlags,
        with_type_parameters: bool,
    ) -> TypeId {
        let name = self.binder.symbols().get(symbol).name.to_string();
        let printed = if with_type_parameters {
            let parameters = self.local_type_parameter_names_of(symbol);
            if parameters.is_empty() { name } else { format!("{name}<{}>", parameters.join(", ")) }
        } else {
            name
        };
        // A class or interface owns its members; a type parameter and an enum do
        // not, and pointing them at a members table they do not have would be a
        // lookup that silently succeeds against the wrong symbol.
        let members = with_type_parameters.then_some(symbol);
        self.store.new_named(flags, printed, members)
    }

    /// The type parameters declared *on* a symbol\'s own declaration.
    ///
    /// Ported from `getLocalTypeParametersOfClassOrInterfaceOrTypeAlias`
    /// (`checker.go`), without the merging across declarations: a symbol with two
    /// declarations takes the first, which is where upstream would find the same
    /// list in every case this slice reaches.
    fn local_type_parameters_of(
        &self,
        symbol: SymbolId,
    ) -> &'a [&'a tsr_ast::TypeParameterDeclaration<'a>] {
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return &[];
        };
        match self.node_map.get(declaration) {
            Some(Node::ClassDeclaration(node)) => node.type_parameters,
            Some(Node::ClassExpression(node)) => node.type_parameters,
            Some(Node::InterfaceDeclaration(node)) => node.type_parameters,
            Some(Node::TypeAliasDeclaration(node)) => node.type_parameters,
            _ => &[],
        }
    }

    /// The names of those type parameters, in order.
    fn local_type_parameter_names_of(&self, symbol: SymbolId) -> Vec<String> {
        self.local_type_parameters_of(symbol)
            .iter()
            .map(|parameter| {
                parameter.name.map_or_else(|| "?".to_string(), |name| name.text.to_string())
            })
            .collect()
    }
}
