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
            TypeNode::ArrayTypeNode(node) => self.get_type_from_array_type_node(node),
            TypeNode::FunctionTypeNode(node) => self.get_type_from_function_type_node(node),
            // `getTypeFromTypeOperatorNode` (`checker.go:22960`). Only the
            // `readonly` arm: it is transparent — the readonly-ness is carried by
            // the *target* the array node picks, not by a wrapper type — and it
            // is what lets `readonly T[]` reach the array arm at all. `keyof` and
            // `unique symbol` are unported and fall through to `errorType`.
            TypeNode::TypeOperatorNode(node)
                if node.operator.kind == SyntaxKind::ReadonlyKeyword =>
            {
                node.r#type
                    .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner))
            }
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
        // **A qualified name is minted only when its root does not resolve.**
        //
        // `resolveEntityName` is unported, so this port cannot type `M.I` where
        // `M` is a real namespace — upstream resolves that and prints `I`, not
        // `M.I`. Minting the written text unconditionally would turn every
        // *resolvable* qualified reference into a confident wrong line, and a
        // `matched`-count bar cannot see it: those lines gap today, so gap→wrong
        // moves nothing the bar watches. That is how this arm shipped and was
        // caught by a unit test rather than by the corpus.
        //
        // The discriminator is upstream's own control flow:
        // `getUnresolvedSymbolForEntityName` is reached *only* when
        // `resolveEntityName` failed, and `resolveEntityName` begins by
        // resolving the **leftmost** name as a namespace. So if the root
        // resolves, upstream had a real symbol and this port must keep gapping
        // until `resolveEntityName` lands; if the root does not resolve,
        // nothing downstream can, and the whole dotted path is unresolvable for
        // upstream too.
        let name = match node.type_name {
            Some(tsr_ast::EntityName::Identifier(name)) => name,
            Some(qualified @ tsr_ast::EntityName::QualifiedName(_)) => {
                let mut root = qualified;
                while let tsr_ast::EntityName::QualifiedName(inner) = root {
                    let Some(left) = inner.left else { return error };
                    root = left;
                }
                let tsr_ast::EntityName::Identifier(root) = root else { return error };
                let Some(root_id) = root.node_id else { return error };
                if self
                    .binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        root_id,
                        root.text,
                        SymbolFlags::NAMESPACE,
                    )
                    .is_some()
                {
                    return error;
                }
                return self.unresolved_type_reference(node);
            }
            None => return error,
        };
        let Some(id) = name.node_id else { return error };
        // `SymbolFlags::TYPE` is upstream's meaning for a type reference
        // (`resolveTypeReferenceName`). It is what lets the resolver consult an
        // enclosing class's or interface's `members` for a type parameter — see
        // `BindResult::resolve_name`.
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, id, name.text, SymbolFlags::TYPE)
        else {
            return self.unresolved_type_reference(node);
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
    /// Accessors and computed names remain unported. A literal containing one
    /// answers `errorType` rather than printing the members it *does*
    /// understand: a partial object type is a wrong answer that looks like a
    /// right one, and it would score as a mismatch either way. The same applies
    /// to a member whose own type is a gap.
    ///
    /// # Members are grouped, not printed in source order
    ///
    /// `createTypeNodesFromResolvedType` (`nodebuilderimpl.go:2627`) emits call
    /// signatures, then construct signatures, then index infos, then
    /// properties — so
    /// `{ a: string; b: string, [key: string]: string }` records
    /// `{ [key: string]: string; a: string; b: string; }`
    /// (`baselines/reference/submodule/conformance/noUncheckedIndexedAccess.types:377`).
    ///
    /// A **method** is a `Signature` in [`crate::objects::Member`] but a
    /// *property* upstream — `addPropertyToElementList` renders it from the
    /// property symbol — so it groups with the properties, not with the call
    /// signatures. That is why the grouping happens here, where the member kind
    /// is known, and not in the shared renderer.
    fn get_type_from_type_literal(&mut self, node: &tsr_ast::TypeLiteralNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let mut signatures = Vec::new();
        let mut indexes = Vec::new();
        let mut properties = Vec::with_capacity(node.members.len());
        for member in node.members {
            // A method, call or construct signature prints whole and has no
            // `name: type` shape at all — see `crate::objects::Member`. Its
            // three spellings differ only in what precedes the parameter list,
            // which is decided here rather than in the renderer.
            let signature = match member {
                tsr_ast::TypeElement::MethodSignatureDeclaration(method) => {
                    let tsr_ast::PropertyName::Identifier(name) = method.name else {
                        return error;
                    };
                    // A method groups with the properties: see the doc comment.
                    Some((method.node_id, Some(name.text.to_string()), "", true))
                }
                tsr_ast::TypeElement::CallSignatureDeclaration(call) => {
                    Some((call.node_id, None, "", false))
                }
                tsr_ast::TypeElement::ConstructSignatureDeclaration(construct) => {
                    Some((construct.node_id, None, "new ", false))
                }
                _ => None,
            };
            if let Some((id, name, prefix, is_property)) = signature {
                let Some(id) = id else { return error };
                // A signature this port cannot build is a gap for the *whole*
                // literal, on the rule this function has followed since it was
                // written: a partial object type is a wrong answer that looks
                // like a right one.
                let Some(signature) = self.get_signature_from_declaration(id) else {
                    return error;
                };
                let text = crate::objects::signature_member_text(self, &signature);
                let name = name.unwrap_or_default();
                let bucket = if is_property { &mut properties } else { &mut signatures };
                bucket.push(crate::objects::Member::Signature {
                    printed: format!("{prefix}{name}{text}"),
                });
                continue;
            }
            if let tsr_ast::TypeElement::IndexSignatureDeclaration(index) = member {
                let Some(rendered) = self.index_signature_member(index) else { return error };
                indexes.push(rendered);
                continue;
            }
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
            properties.push(crate::objects::Member::Property {
                name: name.text.to_string(),
                optional,
                readonly,
                printed: self.type_to_string(member_type),
            });
        }
        signatures.append(&mut indexes);
        signatures.append(&mut properties);
        let members = signatures;
        // `getAliasForTypeNode` (`checker.go:23711`) again, and the arms are
        // deliberately the *same three* [`Checker::get_type_from_union_type_node`]
        // takes — a literal under a non-generic alias prints the alias's name, a
        // generic one gaps rather than dropping its arguments, and an unaliased
        // literal renders structurally.
        //
        // Sharing the shape is the point. `type T8 = string | boolean` records
        // `>T8 : T8` while `var x8: string | boolean` records the constituents,
        // and a type *literal* body behaves identically — `type Obj = { … }`
        // used as `p: Obj[]` records `>arr : Obj[]`, never the expanded members.
        // Two spellings of one rule that disagreed would be worse than either.
        //
        // **The generic arm is unreachable today and is written anyway.**
        // [`Checker::get_declared_type_of_type_alias`] short-circuits a generic
        // alias before its body is resolved, so no generic host reaches here.
        // It is not defensive padding: if that short-circuit is ever replaced by
        // real instantiation, this arm is what stops `type A<T> = { x: T }` from
        // printing a bare `A` with its arguments silently dropped, and the union
        // arm it mirrors would otherwise be the only one guarding that.
        let printed = match node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)) {
            // Shared with `checkObjectLiteral`, so the two structural renderers
            // cannot drift apart — see `crate::objects::render_object_type`.
            None => crate::objects::render_object_type(&members),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                self.binder.symbols().get(alias).name.to_string()
            }
            Some(_) => return error,
        };
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

    /// Ported from `Checker.getTypeFromArrayOrTupleTypeNode` (`checker.go:24115`),
    /// **array half only**.
    ///
    /// # An array type *is* a reference to the global `Array`
    ///
    /// Not a type that prints `T[]`. `getArrayOrTupleTargetType`
    /// (`checker.go:24148`) picks `globalArrayType` and `createTypeReference`
    /// instantiates it, so `string[]` and `Array<string>` are **the same type**,
    /// interned on the same `(symbol, arguments)` key this port already uses for
    /// generic references. `checker.md` warned in as many words against answering
    /// arrays "with another type that merely prints alike"; reusing the reference
    /// machinery is what makes that warning satisfied rather than merely noted.
    ///
    /// The `T[]` spelling is a **printing** rule keyed on the target symbol, so
    /// `Array<Base>` prints `Base[]` too — upstream records exactly that
    /// (`generatedContextualTyping`).
    ///
    /// # `readonly T[]` is a different global, not a modifier
    ///
    /// `getArrayOrTupleTargetType` asks whether the *parent* is a `readonly`
    /// type operator and picks `globalReadonlyArrayType` if so. The two are
    /// distinct types that happen to share an element, and the operator node
    /// itself is transparent.
    ///
    /// # Tuples are a gap, deliberately
    ///
    /// Upstream reaches them through this same function, with `globalTupleType`
    /// and a per-element flags model (`optional`, `rest`, `variadic`) that has no
    /// counterpart here. `bd tsr-p3o`. The array half is where the 11,784 lines
    /// are; a half-ported tuple would answer plausible wrong lines for the rest.
    fn get_type_from_array_type_node(&mut self, node: &tsr_ast::ArrayTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(element_node) = node.element_type else { return error };
        let element = self.get_type_from_type_node(element_node);
        // A gap in the element is a gap in the array: `Unported[]` is not
        // `any[]`, the same call made for union constituents and type arguments.
        if element == error {
            return error;
        }
        let readonly = node
            .node_id
            .and_then(|id| self.nodes.parent(id))
            .is_some_and(|parent| self.is_readonly_type_operator(parent));
        let target = if readonly { "ReadonlyArray" } else { "Array" };
        let Some(target) = self.global_type_symbol(target) else { return error };
        self.create_type_reference(target, vec![element])
    }

    /// `isReadonlyTypeOperator` (`checker.go:24160`).
    fn is_readonly_type_operator(&self, node: tsr_ast::NodeId) -> bool {
        matches!(
            self.node_map.get(node),
            Some(Node::TypeOperatorNode(operator))
                if operator.operator.kind == SyntaxKind::ReadonlyKeyword
        )
    }

    /// The global type `name`, if the program has one of arity 1.
    ///
    /// Ported from `Checker.getGlobalType` (`checker.go`), reduced to the arity
    /// this slice needs. Upstream reports when the global is missing or has the
    /// wrong arity; without diagnostics the answer is a gap — which is also what
    /// happens when a file is checked with no lib files, as every unit test here
    /// is.
    pub(crate) fn global_type_symbol(&self, name: &str) -> Option<SymbolId> {
        let symbol = *self.binder.globals().get(name)?;
        (self.local_type_parameters_of(symbol).len() == 1).then_some(symbol)
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
        self.create_type_reference(symbol, arguments)
    }

    /// A type reference whose name **does not resolve**, printed as the name
    /// that was written.
    ///
    /// Ported from `getUnresolvedSymbolForEntityName` (`checker.go:23102`) and
    /// the `CheckFlagsUnresolved` branch of `getTypeFromTypeAliasReference`
    /// (`checker.go:23580`). Upstream mints a synthetic `TypeAlias` symbol
    /// named after the entity and one `errorType` per alias key carrying it, so
    /// the node builder writes a `TypeReference` to that name.
    ///
    /// # Why this is not "inventing an answer"
    ///
    /// It looks like the thing this port refuses everywhere else — answering
    /// something for a name it could not resolve. It is the opposite: upstream
    /// reports `TS2304 Cannot find name` **and prints the name anyway**.
    /// `conformance/parserRealSource11` carries 1,006 of those errors, records
    /// `>nodeType : NodeType` throughout, and contains **zero** ` : any` lines.
    /// Answering `errorType` there is the divergence.
    ///
    /// # The type still answers `is_error`, and that is the whole design
    ///
    /// [`Checker::is_error`] is identity-based throughout this crate so that
    /// `errorType` and `anyType` stay apart. The type minted here is added to
    /// [`Checker::unresolved_types`] and `is_error` consults that set, so every
    /// consumer — the arithmetic arm, `+`, the union worker, array elements —
    /// keeps treating it as a gap and keeps propagating. **Only the line that
    /// renders this node changes**, which is why the change can gain and cannot
    /// lose.
    ///
    /// Type arguments are rendered into the text, as upstream puts them on the
    /// alias, so `Foo<string>` prints `Foo<string>` rather than `Foo`.
    fn unresolved_type_reference(&mut self, node: &tsr_ast::TypeReferenceNode<'a>) -> TypeId {
        let Some(text) = Self::entity_name_text(node.type_name) else {
            return self.intrinsics.error;
        };
        let mut printed = text;
        if !node.type_arguments.is_empty() {
            let arguments: Vec<String> = node
                .type_arguments
                .iter()
                .map(|argument| {
                    let id = self.get_type_from_type_node(*argument);
                    self.type_to_string(id)
                })
                .collect();
            // A gap inside an argument is a gap in the whole reference:
            // printing `Foo<error>` would be a wrong line rather than a missing
            // one, and upstream's alias key is built from resolved arguments.
            if arguments.iter().any(|argument| argument == "error") {
                return self.intrinsics.error;
            }
            printed = format!("{printed}<{}>", arguments.join(", "));
        }
        let id = self.store.new_named(TypeFlags::ANY, printed, None);
        self.unresolved_types.insert(id);
        id
    }

    /// The source spelling of an entity name — `A`, or `A.B.C`.
    ///
    /// Upstream builds the same string with `getSymbolPath` over the chain of
    /// unresolved parent symbols (`checker.go:23139`).
    fn entity_name_text(name: Option<tsr_ast::EntityName<'a>>) -> Option<String> {
        match name? {
            tsr_ast::EntityName::Identifier(identifier) => Some(identifier.text.to_owned()),
            tsr_ast::EntityName::QualifiedName(qualified) => {
                let left = Self::entity_name_text(qualified.left)?;
                Some(format!("{left}.{}", qualified.right?.text))
            }
        }
    }

    /// `createTypeReference(target, typeArguments)` (`checker.go`).
    ///
    /// Interned on the `(target, arguments)` pair, which is what makes
    /// `string[]` and `Array<string>` one type rather than two that print alike.
    pub(crate) fn create_type_reference(
        &mut self,
        symbol: SymbolId,
        arguments: Vec<TypeId>,
    ) -> TypeId {
        if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
            return cached;
        }
        let printed = self.type_reference_text(symbol, &arguments);
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
        let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
        self.instantiations.insert((symbol, arguments.clone()), id);
        // The same pair, the other way round. Substitution starts from a
        // `TypeId` and needs the pair, which only exists here as a key — see
        // [`crate::checker::Checker::type_reference_targets`]. Written on the
        // miss path only, so it is one insert per distinct reference.
        self.type_reference_targets.insert(id, (symbol, arguments));
        id
    }

    /// How an instantiated reference prints: `C<number>`, or `T[]` when the
    /// target is the global `Array`.
    ///
    /// `typeReferenceToTypeNode` (`nodebuilderimpl.go:2977`) special-cases
    /// `globalArrayType` and `globalReadonlyArrayType` before anything else, so
    /// the shorthand is a property of the **target**, not of how the type was
    /// written. `Array<Base>` prints `Base[]`.
    fn type_reference_text(&mut self, symbol: SymbolId, arguments: &[TypeId]) -> String {
        if let [element] = arguments {
            let element = self.array_element_text(*element);
            if self.global_type_symbol("Array") == Some(symbol) {
                return format!("{element}[]");
            }
            if self.global_type_symbol("ReadonlyArray") == Some(symbol) {
                return format!("readonly {element}[]");
            }
        }
        let name = self.binder.symbols().get(symbol).name.to_string();
        let printed = arguments
            .iter()
            .map(|&argument| self.type_to_string(argument))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name}<{printed}>")
    }

    /// An array's element, parenthesised where the postfix `[]` would otherwise
    /// bind wrongly.
    ///
    /// Taken from the baselines rather than from a precedence table, because
    /// only some of the plausible cases are actually parenthesised:
    ///
    /// ```text
    /// (string | number)[]        (typeof Alpha)[]        (() => string)[]
    /// { (x: number): number; }[]        string[][]        string[]
    /// ```
    ///
    /// So a union, a `typeof`, and a signature are wrapped; an object type and a
    /// nested array are not.
    ///
    /// # The intersection clause was wrong, and its own comment said how
    ///
    /// This function used to wrap **every** union and intersection, with the
    /// note *"intersections are wrapped on the same precedence grounds and no
    /// baseline exercises one, which is stated rather than presented as
    /// verified."* One does: `compiler/inferTypePredicates` wants `Bar[]` where
    /// `Bar` is `type Bar = …&…`, and this printed `(Bar)[]`.
    ///
    /// A union or intersection that a type alias names prints as **that name**,
    /// and so does `boolean` — see
    /// [`crate::printing::prints_as_a_single_token`]. Neither is a
    /// `Union`/`IntersectionTypeNode` to upstream's builder, so neither is
    /// parenthesised anywhere.
    fn array_element_text(&self, element: TypeId) -> String {
        let ty = self.store.get(element);
        let text = crate::printing::type_to_string(ty);
        let wrap = (ty.flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
            && !crate::printing::prints_as_a_single_token(ty))
            || text.starts_with("typeof ")
            || has_top_level_arrow(&text);
        if wrap { format!("({text})") } else { text }
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
                // *fresh* form of its literal type.
                //
                // **This is now load-bearing, and the comment that said it was
                // unobservable is corrected rather than deleted.** It claimed
                // reaching this needed `E.A` in type position — a qualified name,
                // still unported — and that was true of the only route that
                // existed when it was written. A second route arrived:
                // `getWidenedLiteralType` widens an enum member only when it is
                // *fresh* (`checker.go:25488`), so `var e = E.A` prints `E`
                // because of this line and would print `E.A` without it. The
                // freshness gate is what `crate::literals`'s enum arm tests
                // first, and the control asserting the enum type itself does not
                // widen is pinning exactly this call.
                let fresh = self.get_fresh_type_of_literal_type(member_type);
                self.declared_types.insert(member_symbol, fresh);
                // The type -> enum back-edge that `getBaseTypeOfEnumLikeType`
                // (`checker.go:25470`) reads as `t.symbol`, which
                // `TypeData::Named` does not carry. Recorded here, at the one
                // place a member type is created, because anywhere else would
                // have to reconstruct which enum a type belongs to.
                //
                // **Both forms, not just the fresh one.** Upstream's symbol
                // lives on the type, not on its freshness, so both spellings of
                // the same literal answer `getBaseTypeOfEnumLikeType`
                // identically. Only the fresh one is reachable through
                // `get_widened_literal_type`, which returns early for a regular
                // type; the regular entry is what keeps the function a fact
                // about the type rather than about how it was reached.
                self.enum_member_owners.insert(member_type, symbol);
                self.enum_member_owners.insert(fresh, symbol);
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

/// Whether a printed type has a `=>` outside any brackets.
///
/// A function type is parenthesised as an array element; an object type with a
/// call signature — `{ (x: number): number; }` — is not, and has no top-level
/// `=>` either, so one test separates them.
fn has_top_level_arrow(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>'
                if !(*byte == b'>' && index > 0 && bytes[index - 1] == b'=') =>
            {
                depth -= 1;
            }
            _ => {}
        }
        if depth == 0 && bytes[index..].starts_with(b"=>") {
            return true;
        }
    }
    false
}
