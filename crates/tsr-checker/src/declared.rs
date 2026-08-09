//! Type nodes, and what a *type* symbol declares.
//!
//! Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`) and
//! `Checker.getDeclaredTypeOfSymbol` (`checker.go:23670`). These are one module
//! because they call each other on every type reference: a `TypeReferenceNode`
//! resolves a name and asks the symbol what it declares.
//!
//! Not to be confused with [`crate::symbols`], which answers what type a
//! *value* symbol has.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind, TypeNode};
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
            TypeNode::TupleTypeNode(node) => self.get_type_from_tuple_type_node(node),
            TypeNode::FunctionTypeNode(node) => self.get_type_from_function_type_node(node),
            TypeNode::ConstructorTypeNode(node) => self.get_type_from_constructor_type_node(node),
            TypeNode::TypeQueryNode(node) => self.get_type_from_type_query_node(node),
            // `getTypeFromTypeNodeWorker`'s `ast.KindTypePredicate` case
            // (`checker.go:22858`). A predicate's *type* is `void` under an
            // `asserts` modifier and `boolean` otherwise; the predicate itself
            // is not a type and appears only in a signature's return position,
            // where [`Checker::signature_to_string`] prints it instead of this.
            //
            // The pair is visible in one corpus case
            // (`conformance/typeGuardOfFormIsType`): `>isFunction : (x: any) =>
            // x is Function` for the declaration and `>isFunction(x) : boolean`
            // for a call to it. `docs/architecture/checker-notes-typepred.md`
            // §1 is why those are two halves of one arm and not two items.
            TypeNode::TypePredicateNode(node) => {
                if node.asserts_modifier.is_some() {
                    self.intrinsics.void
                } else {
                    self.intrinsics.boolean
                }
            }
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
            // The ESSymbol arm of `getTypeFromTypeOperatorNode`
            // (`checker.go:22960`): a WRITTEN `unique symbol` mints one type
            // per node (`checker-notes-callres.md` §27).
            TypeNode::TypeOperatorNode(node) if node.operator.kind == SyntaxKind::UniqueKeyword => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                if let Some(&existing) = self.unique_symbol_nodes.get(&id) {
                    return existing;
                }
                let minted = self.store.new_named(
                    TypeFlags::UNIQUE_ES_SYMBOL,
                    "unique symbol".to_string(),
                    None,
                );
                self.unique_symbol_nodes.insert(id, minted);
                minted
            }
            // §91: `keyof T` EVALUATES — but only inside a conditional-alias
            // evaluation (the env gate); everywhere else the §35 deferred
            // print and the error fall-through keep their old answers.
            TypeNode::TypeOperatorNode(node)
                if node.operator.kind == SyntaxKind::KeyOfKeyword
                    && !self.alias_evaluation_bindings.is_empty() =>
            {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let target = self.get_type_from_type_node(inner);
                match self.keys_of(target) {
                    Some(keys) => self.literal_key_union(&keys),
                    None => self.intrinsics.error,
                }
            }
            // §28 (`checker-notes-callres.md`): `this` in type position is
            // the enclosing class/interface declaration's one `this` type.
            TypeNode::ThisTypeNode(node) => {
                let Some(id) = node.node_id else { return self.intrinsics.error };
                let mut current = self.nodes.parent(id);
                while let Some(parent) = current {
                    if matches!(self.nodes.kind(parent), SyntaxKind::InterfaceDeclaration) {
                        if let Some(&existing) = self.this_type_nodes.get(&parent) {
                            return existing;
                        }
                        let minted =
                            self.store.new_named(TypeFlags::OBJECT, "this".to_string(), None);
                        self.this_type_nodes.insert(parent, minted);
                        return minted;
                    }
                    current = self.nodes.parent(parent);
                }
                self.intrinsics.error
            }
            // §35 (`checker-notes-callres.md`): DEFERRED `keyof` over a type
            // parameter prints as written — the §34 mint, the §31
            // registration. Concrete operands resolve upstream and decline.
            TypeNode::TypeOperatorNode(node) if node.operator.kind == SyntaxKind::KeyOfKeyword => {
                let deferred = match node.r#type {
                    Some(TypeNode::TypeReferenceNode(operand))
                        if operand.type_arguments.is_empty() =>
                    {
                        let is_type_parameter = operand
                            .type_name
                            .and_then(|name| match name {
                                tsr_ast::EntityName::Identifier(identifier) => {
                                    identifier.node_id.and_then(|id| {
                                        self.binder.resolve_name(
                                            self.nodes,
                                            self.node_map,
                                            id,
                                            identifier.text,
                                            SymbolFlags::TYPE,
                                        )
                                    })
                                }
                                tsr_ast::EntityName::QualifiedName(_) => None,
                            })
                            .is_some_and(|symbol| {
                                self.binder
                                    .symbols()
                                    .get(symbol)
                                    .flags
                                    .contains(SymbolFlags::TYPE_PARAMETER)
                            });
                        if is_type_parameter {
                            Self::entity_name_text(operand.type_name)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                match deferred {
                    Some(operand) => {
                        let printed = format!("keyof {operand}");
                        let id = self.store.new_named(TypeFlags::ANY, printed, None);
                        self.unresolved_types.insert(id);
                        id
                    }
                    None => self.intrinsics.error,
                }
            }
            // §36 (`checker-notes-callres.md`): a template-literal type
            // prints as written — the §31 mint from the node's parts.
            // Literal-typed holes decline (upstream RESOLVES those to plain
            // literals).
            TypeNode::TemplateLiteralTypeNode(node) => {
                let Some(head) = node.head else { return self.intrinsics.error };
                let mut printed = format!("`{}", head.text);
                for span in node.template_spans {
                    let Some(hole) = span.r#type else { return self.intrinsics.error };
                    let hole_type = self.get_type_from_type_node(hole);
                    if hole_type == self.intrinsics.error
                        || self
                            .store
                            .get(hole_type)
                            .flags
                            .intersects(TypeFlags::UNIT | TypeFlags::UNION)
                    {
                        return self.intrinsics.error;
                    }
                    let rendered = self.type_to_string(hole_type);
                    let literal_text = match span.literal {
                        Some(tsr_ast::TemplateMiddleOrTail::TemplateMiddle(middle)) => middle.text,
                        Some(tsr_ast::TemplateMiddleOrTail::TemplateTail(tail)) => tail.text,
                        None => return self.intrinsics.error,
                    };
                    printed.push_str("${");
                    printed.push_str(&rendered);
                    printed.push('}');
                    printed.push_str(literal_text);
                }
                printed.push('`');
                // OBJECT rather than the §31 mints' ANY: a template mint in
                // a UNION must not trip any-absorption or string-literal
                // reduction (`"bar" | \`foo-${string}\`` keeps both).
                let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
                self.unresolved_types.insert(id);
                id
            }
            // §34 (`checker-notes-callres.md`): a DEFERRED indexed access —
            // the index is a type parameter, upstream cannot resolve it until
            // instantiation — prints as written via the §31 mint; is_error
            // stays true through `unresolved_types`, so only the printed
            // line changes. Literal indexes resolve concretely upstream and
            // stay declined here.
            TypeNode::IndexedAccessTypeNode(node) => {
                let deferred_index = match node.index_type {
                    Some(TypeNode::TypeReferenceNode(index)) => {
                        let text = Self::entity_name_text(index.type_name);
                        let is_type_parameter = index
                            .type_name
                            .and_then(|name| match name {
                                tsr_ast::EntityName::Identifier(identifier) => {
                                    identifier.node_id.and_then(|id| {
                                        self.binder.resolve_name(
                                            self.nodes,
                                            self.node_map,
                                            id,
                                            identifier.text,
                                            SymbolFlags::TYPE,
                                        )
                                    })
                                }
                                tsr_ast::EntityName::QualifiedName(_) => None,
                            })
                            .is_some_and(|symbol| {
                                self.binder
                                    .symbols()
                                    .get(symbol)
                                    .flags
                                    .contains(SymbolFlags::TYPE_PARAMETER)
                            });
                        if index.type_arguments.is_empty() && is_type_parameter {
                            text
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                let object_text = match node.object_type {
                    Some(TypeNode::TypeReferenceNode(object))
                        if object.type_arguments.is_empty() =>
                    {
                        // §34's narrowing (29 G→W in the first pair): a TYPE
                        // ALIAS object EXPANDS in upstream's deferred print
                        // (`ArgMap[P]` wants `{ sum: ...; concat: ... }[P]`);
                        // only a non-alias object keeps its written name.
                        let is_alias = object
                            .type_name
                            .and_then(|name| match name {
                                tsr_ast::EntityName::Identifier(identifier) => {
                                    identifier.node_id.and_then(|id| {
                                        self.binder.resolve_name(
                                            self.nodes,
                                            self.node_map,
                                            id,
                                            identifier.text,
                                            SymbolFlags::TYPE,
                                        )
                                    })
                                }
                                tsr_ast::EntityName::QualifiedName(_) => None,
                            })
                            .is_some_and(|symbol| {
                                self.binder
                                    .symbols()
                                    .get(symbol)
                                    .flags
                                    .contains(SymbolFlags::TYPE_ALIAS)
                            });
                        if is_alias { None } else { Self::entity_name_text(object.type_name) }
                    }
                    _ => None,
                };
                match (object_text, deferred_index) {
                    (Some(object), Some(index)) => {
                        let printed = format!("{object}[{index}]");
                        let id = self.store.new_named(TypeFlags::ANY, printed, None);
                        self.unresolved_types.insert(id);
                        id
                    }
                    _ => self.intrinsics.error,
                }
            }
            _ => self.intrinsics.error,
        }
    }

    /// Ported from `Checker.getTypeFromTypeQueryNode` (`checker.go:24102`) via
    /// `checkExpressionWithTypeArguments` (`checker.go:10637`): `typeof x` in
    /// type position is the *expression* type of the entity name, then
    /// `getRegularTypeOfLiteralType(getWidenedType(t))`.
    ///
    /// Two refusals, whole-construct rather than approximated
    /// (`docs/architecture/checker-notes-tquery.md` §4 sizes both):
    ///
    /// - **`typeof this`** (20 corpus lines): upstream routes it through
    ///   `checkThisExpression` (`checker.go:10652`), whose per-container answer
    ///   this port only partially has.
    /// - **Instantiation expressions** `typeof f<string>` (10 lines): with type
    ///   arguments present, `getInstantiationExpressionType`
    ///   (`checker.go:10660`) filters signatures by arity and instantiates
    ///   each; without them it returns the expression type unchanged, which is
    ///   the only half ported here.
    ///
    /// Divergence, stated: this port has no general `getWidenedType`
    /// (`checker.go:18355`). Entity-name expression types come from
    /// `get_type_of_symbol`, which widens variable-like declarations at the
    /// declaration, so the residual work here is fresh-literal regularisation —
    /// `get_regular_type_of_literal_type`. A `typeof` line needing
    /// object-literal widening *at the query* is owned by the notes page §2.
    fn get_type_from_type_query_node(&mut self, node: &tsr_ast::TypeQueryNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        if !node.type_arguments.is_empty() {
            return error;
        }
        let Some(name) = node.expr_name else { return error };
        // The first run of this arm refused `typeof` over a parameter symbol
        // here, after the registered bar fired (+1,341 gap→wrong,
        // `checker-notes-tquery.md` §5). That narrowing is superseded by the
        // faithful mechanism: signature rendering reuses the written
        // `typeof a` node ([`crate::signatures::Parameter::written_text`]),
        // which is where every one of those wrong lines was printed. The
        // computation below is upstream's for every entity-name form.
        let id = match name {
            // Upstream's `isThisIdentifier` dispatch (`checker.go:10651`). The
            // guard does not currently bite under mutation — `check_expression`
            // answers `errorType` for a `this` identifier anyway, because
            // `resolve_name` finds no such value — and it stays because it is
            // upstream's dispatch, with the same standing as the
            // `SymbolFlags::VALUE` test `checker-notes-symbols.md` §5 records:
            // it becomes observable the moment `check_expression` learns
            // `this`, and whoever adds that should re-run the mutation and
            // expect `type_query.rs` red.
            tsr_ast::EntityName::Identifier(identifier) if identifier.text == "this" => {
                return error;
            }
            tsr_ast::EntityName::Identifier(identifier) => {
                self.check_expression(Expression::Identifier(identifier))
            }
            tsr_ast::EntityName::QualifiedName(qualified) => self.check_qualified_name(qualified),
        };
        self.get_regular_type_of_literal_type(id)
    }

    /// Ported from `Checker.getTypeFromTypeReference` into
    /// `getTypeReferenceType` (`checker.go:23146`).
    ///
    /// Resolves the name and asks the symbol what type it declares. Two things
    /// are deliberately left as gaps rather than approximated:
    ///
    /// - **A qualified name** (`M.I`) goes to
    ///   [`Checker::qualified_type_reference`], which reprints the written
    ///   entity name rather than building upstream's symbol chain.
    /// - **Type arguments** (`C<number>`) need instantiation, the machinery
    ///   upstream guards with a depth of 100 and a count of 5 million
    ///   (`checker.go:22111`, `bd tsr-el3.2`). Half of it — substituting names
    ///   without the guards — is exactly the kind of port that works on the
    ///   corpus and hangs on a real program.
    fn get_type_from_type_reference(&mut self, node: &tsr_ast::TypeReferenceNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        // **A qualified name splits on whether its root resolves as a
        // namespace**, which is upstream's own control flow:
        // `getUnresolvedSymbolForEntityName` is reached *only* when
        // `resolveEntityName` failed, and `resolveEntityName` begins by
        // resolving the **leftmost** name as a namespace
        // (`resolveQualifiedName`, `checker.go:15829`).
        //
        // - The root does **not** resolve — nothing downstream can, the whole
        //   dotted path is unresolvable for upstream too, and upstream mints the
        //   synthetic symbol and prints the written text.
        //   [`Checker::unresolved_type_reference`].
        // - The root **does** resolve — upstream had a real symbol, and this
        //   port answers it through [`Checker::qualified_type_reference`], whose
        //   doc comment carries the design and the one refusal inside it.
        //
        // Until `2a7a03f` the second arm was an unconditional `errorType`,
        // because minting the written text *unconditionally* turns every
        // resolvable qualified reference into a confident wrong line and a
        // `matched`-count bar cannot see it — those lines gap today, so gap→wrong
        // moves nothing the bar watches. That refusal was of an unrefined design;
        // see `docs/architecture/checker-notes-qualname.md`.
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
                let Some(namespace) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    root_id,
                    root.text,
                    SymbolFlags::NAMESPACE,
                ) else {
                    return self.unresolved_type_reference(node);
                };
                return self.qualified_type_reference(node, qualified, namespace);
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
        // §91: inside a conditional-alias evaluation, a bound type parameter
        // answers its binding — the node-level substitution the evaluator
        // runs on the alias body.
        if let Some(bound) = self
            .alias_evaluation_bindings
            .iter()
            .rev()
            .find_map(|frame| frame.get(&symbol).copied())
        {
            return bound;
        }
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
        // **The single-signature collapse** (`checker-notes-modobj.md` §10.15,
        // `bd tsr-d4li`): upstream renders an anonymous type whose only member
        // is one call or construct signature as the arrow form —
        // `{ new (): Base }` prints `new () => Base`. Sized at 1,093 root
        // converts / 0 root at-risk. The 209-line embedded population is held
        // by the written carriage in `written_annotation_text`: a parameter
        // *written* with this literal keeps its braces text through node
        // reuse, which is what the first build of this collapse lacked when
        // its leg 4 fired at 112 and it was reverted (`dbc1ae9`).
        if let [
            tsr_ast::TypeElement::CallSignatureDeclaration(_)
            | tsr_ast::TypeElement::ConstructSignatureDeclaration(_),
        ] = node.members
        {
            let member_id = match node.members[0] {
                tsr_ast::TypeElement::CallSignatureDeclaration(member) => member.node_id,
                tsr_ast::TypeElement::ConstructSignatureDeclaration(member) => member.node_id,
                _ => unreachable!(),
            };
            let Some(member_id) = member_id else { return error };
            let Some(signature) = self.get_signature_from_declaration(member_id) else {
                return error;
            };
            let text = self.signature_to_string(&signature);
            let Some(symbol) = node.node_id.and_then(|id| self.binder.symbol_of(id)) else {
                return error;
            };
            let built = self.store.new_anonymous(TypeFlags::OBJECT, text, symbol, true);
            self.signature_types.insert(built, vec![signature]);
            return built;
        }
        // §33's second containment (`checker-notes-callres.md`): overloaded
        // literals whose signatures REUSE a type-parameter name print
        // upstream's site-sensitive `_1` renames (the §19/§20 refusal); when
        // any member also carries `const` — the shape this build newly
        // admits — the literal declines whole rather than printing the
        // un-renamed collision.
        {
            let mut any_const = false;
            let mut seen = std::collections::HashSet::new();
            let mut collision = false;
            for member in node.members {
                let parameters = match member {
                    tsr_ast::TypeElement::CallSignatureDeclaration(m) => m.type_parameters,
                    tsr_ast::TypeElement::ConstructSignatureDeclaration(m) => m.type_parameters,
                    _ => continue,
                };
                for parameter in parameters {
                    if parameter.modifiers.iter().any(|modifier| {
                        matches!(modifier, tsr_ast::ModifierLike::Token(token)
                            if token.kind == SyntaxKind::ConstKeyword)
                    }) {
                        any_const = true;
                    }
                    if let Some(name) = parameter.name
                        && !seen.insert(name.text)
                    {
                        collision = true;
                    }
                }
            }
            if any_const && collision {
                return error;
            }
        }
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
                // No `"new "` here: it comes from the signature's
                // [`crate::signatures::SignatureKind`] inside
                // `signature_member_text`, so a construct signature member and
                // a `new () => T` type node take their prefix from one place.
                tsr_ast::TypeElement::ConstructSignatureDeclaration(construct) => {
                    Some((construct.node_id, None, "", false))
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
            // A property *written* with a single-member literal keeps its
            // braces text, the same carriage the parameter slot takes —
            // `bd tsr-d4li`; the second measurement's 55 residual losses were
            // exactly this slot. Restricted to the literal shape so nothing
            // else changes spelling here.
            let printed = match annotation {
                tsr_ast::TypeNode::TypeLiteralNode(_) | tsr_ast::TypeNode::ArrayTypeNode(_) => self
                    .written_annotation_text(annotation)
                    .unwrap_or_else(|| self.type_to_string(member_type)),
                _ => self.type_to_string(member_type),
            };
            properties.push(crate::objects::Member::Property {
                name: name.text.to_string(),
                optional,
                readonly,
                printed,
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
            // §77.1 (`checker-notes-narrow.md`): a literal whose subtree holds
            // a SINGLE-QUOTED string literal type keeps its written spelling —
            // the same gate as the parameter carriage, at the mint.
            None => {
                let mut single_quoted = false;
                match crate::signatures::written_type_literal_text(node, &mut single_quoted) {
                    Some(text) if single_quoted => text,
                    _ => crate::objects::render_object_type(&members),
                }
            }
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
        // A degenerate node (`type U3 = | () => number`, the leading-bar
        // parse keeps it — §89.1) answers its constituent BEFORE the alias
        // attaches: `getUnionTypeEx`'s `len(types) == 1` early return
        // (`checker.go:25632`) runs ahead of every alias consumer, which is
        // why upstream prints `U3 : () => number` structurally.
        if let [single] = types[..] {
            return single;
        }
        // §92: under evaluation bindings the node is an alias BODY being
        // instantiated — its own alias attribution (parent = the generic
        // alias declaration) must not fire the generic-alias error arm.
        if !self.alias_evaluation_bindings.is_empty() {
            return self.get_union_type(&types);
        }
        match node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)) {
            None => self.get_union_type(&types),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                self.get_named_union_type(&types, TypeFlags::empty(), alias)
            }
            Some(_) => error,
        }
    }

    /// [`Checker::get_type_from_type_node`] for a consumer that will never
    /// **print** the result — see [`Checker::get_union_type_unprinted`].
    ///
    /// Only an un-aliased `UnionTypeNode` differs, and only when a constituent
    /// is *named*: `get_type_from_union_type_node` routes it through
    /// `get_union_type`, whose worker answers `errorType` there because this
    /// port computes printed text at type-creation time and upstream's `origin`
    /// denormalisation (`checker.go:25705`) is unported. `var c: E | F` — two
    /// enums — therefore *declares* `errorType`, and every rule that gates on
    /// the declared type is silenced on it.
    ///
    /// §42.1 found this at the wrapper (`getOptionalType`'s `T | undefined`)
    /// and fixed it there; the **annotation itself** was still on the printing
    /// road, which is `checker-notes-diag2.md` §76.
    ///
    /// An *aliased* union is deliberately not rerouted. It goes to
    /// `get_named_union_type`, which is a different question — the alias's own
    /// printing — and a generic alias is `errorType` for a third reason.
    ///
    /// **How this would be shown wrong:** a caller printing a type obtained
    /// through here would emit expanded constituents. Nothing may call it from
    /// the query road, which is what keeps the `checker_types` gradient
    /// unmoved.
    pub(crate) fn get_type_from_type_node_unprinted(&mut self, node: TypeNode<'a>) -> TypeId {
        let TypeNode::UnionTypeNode(union) = node else {
            return self.get_type_from_type_node(node);
        };
        if union.node_id.and_then(|id| self.alias_symbol_for_type_node(id)).is_some() {
            return self.get_type_from_type_node(node);
        }
        let types = union
            .types
            .iter()
            .map(|constituent| self.get_type_from_type_node(*constituent))
            .collect::<Vec<_>>();
        self.get_union_type_unprinted(&types)
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
        // The degenerate leading-`&` node answers its constituent un-aliased,
        // like the union above — `getIntersectionTypeEx`'s
        // `len(typeSet) == 1` return (`checker.go:26128`) precedes the alias
        // consumers (§89.1).
        if let [single] = types[..] {
            return single;
        }
        // §91, env-gated: an intersection of literal-key unions reduces by
        // set intersection — upstream's `intersectUnionsOfPrimitiveTypes` +
        // the two-unit-types-are-never rule, applied only where the
        // conditional-alias evaluator needs it (`keyof base & keyof props`).
        if !self.alias_evaluation_bindings.is_empty() {
            if let Some(reduced) = self.intersect_literal_key_unions(&types) {
                return reduced;
            }
            // §92: same alias-body rule as the union arm above.
            return self.get_intersection_type(&types, None);
        }
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
    /// # Tuples are the sibling arm
    ///
    /// Upstream reaches them through this same function, with `globalTupleType`
    /// and a per-element flags model (`optional`, `rest`, `variadic`). That
    /// model is still unported and still refuses — see
    /// [`Checker::get_type_from_tuple_type_node`], which gaps on every element
    /// carrying one. This comment used to say tuples were a gap outright, on
    /// the grounds that *"a half-ported tuple would answer plausible wrong
    /// lines for the rest"*; that argument is against porting the modifiers and
    /// not against the plain form, which is **74.9%** of every tuple the
    /// baselines print (`docs/architecture/checker-notes-tuple.md` §2).
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

    /// `Checker.getTypeFromArrayOrTupleTypeNode` (`checker.go:24115`), **tuple
    /// half, plain elements only**.
    ///
    /// # What "plain" means, and why the rest refuses
    ///
    /// Upstream builds a tuple as a reference to a target synthesised from a
    /// per-element flags model — `optional`, `rest`, `variadic`
    /// (`getArrayOrTupleTargetType`, `checker.go:24148`). None of that model
    /// exists here, so an element written `a?: T`, `...T` or `name: T` gaps the
    /// **whole tuple** rather than being approximated: `[string, ...number[]]`
    /// rendered as `[string, number[]]` would be a wrong line where there is a
    /// missing one today.
    ///
    /// The plain form is separable and is most of the population — 966 of the
    /// 1,289 tuples the baselines print, 74.9%
    /// (`docs/architecture/checker-notes-tuple.md`). Same shape as `&&`
    /// separating from `||`: the machinery this refuses is real and it guards a
    /// minority.
    ///
    /// # It carries no members, and that is the safety property
    ///
    /// Upstream's tuple has numeric properties, a `length`, and the `Array`
    /// interface behind it. This type has none, so `t[0]`, `t.length` and
    /// destructuring stay exactly the gaps they are today. Only the line that
    /// *renders* the tuple can move, which is what makes the change able to
    /// gain and almost unable to lose — the same argument
    /// [`Checker::unresolved_type_reference`] makes for its own row.
    ///
    /// # Interned on the element list
    ///
    /// `[number, string]` written twice must be one type, or a union of the two
    /// spellings prints both. Upstream gets that from `createTypeReference` on
    /// the tuple target; there is no target symbol here, so the key is the
    /// element list itself — see [`Checker::tuple_types`](crate::checker).
    fn get_type_from_tuple_type_node(&mut self, node: &tsr_ast::TupleTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        // §79.1: `getAliasForTypeNode`'s three arms, the §72 rule at the
        // tuple mint — `type T2 = [number, string, boolean?]` prints `T2`
        // (`optionalTupleElements1` priced this at 99 G→W without it: the
        // §79 structural prints replaced prior `error` gaps at every
        // alias-wanting position). The named copy carries the element list
        // and optional mask so contexts and index reads work through it.
        if let Some(alias) = node.node_id.and_then(|id| self.alias_symbol_for_type_node(id))
            && self.local_type_parameters_of(alias).is_empty()
            && !node.elements.is_empty()
            && !node.elements.iter().any(|e| matches!(e, TypeNode::RestTypeNode(_)))
        {
            // (`type foo = []` prints `[]`, not `foo` —
            // `typeAliasDeclarationEmit3`'s 3 R→W named the empty gate.)
            // A GENERIC alias falls to the structural road (its instantiated
            // positions print structurally — `destructureTupleWithVariableElement`
            // measured 3 R→W under §72's gap rule here), and a REST-bearing
            // body keeps the §40 variadic road untouched (the depth-guarded
            // giant of `excessivelyLargeTupleSpread` printed `any` through
            // it, 13 R→W when the alias arm intercepted).
            let structural = self.tuple_type_node_structural(node);
            if structural == error {
                return error;
            }
            let name = self.binder.symbols().get(alias).name.to_string();
            let named = self.store.new_named(TypeFlags::OBJECT, name, None);
            if let Some(entry) = self.tuple_element_lists.get(&structural).cloned() {
                self.tuple_element_lists.insert(named, entry);
            }
            if let Some(mask) = self.tuple_optional_masks.get(&structural).cloned() {
                self.tuple_optional_masks.insert(named, mask);
            }
            return named;
        }
        self.tuple_type_node_structural(node)
    }

    /// The structural mint behind [`Checker::get_type_from_tuple_type_node`].
    fn tuple_type_node_structural(&mut self, node: &tsr_ast::TupleTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let mut elements = Vec::with_capacity(node.elements.len());
        // §40 (`checker-notes-narrow.md`): REST elements make the tuple a
        // PRINT-ONLY variadic — the text composed from resolved element
        // prints, minted with no element-list entry so access,
        // instantiation, and relations keep declining.
        if node.elements.iter().any(|element| matches!(element, TypeNode::RestTypeNode(_)))
            && !node.elements.iter().any(|element| {
                matches!(element, TypeNode::NamedTupleMember(_) | TypeNode::OptionalTypeNode(_))
            })
        {
            let mut pieces = Vec::with_capacity(node.elements.len());
            let mut spliced: Option<Vec<TypeId>> = Some(Vec::new());
            for element in node.elements {
                let (prefix, inner) = match element {
                    TypeNode::RestTypeNode(rest) => {
                        let Some(inner) = rest.r#type else { return error };
                        ("...", inner)
                    }
                    other => ("", *other),
                };
                let resolved = self.get_type_from_type_node(inner);
                if resolved == error {
                    return error;
                }
                // A rest over a CONCRETE tuple splices — upstream expands it
                // flat (`excessivelyLargeTupleSpread`, the §40 falsifier's
                // population); any other rest keeps the whole print-only.
                if let Some(flat) = spliced.as_mut() {
                    if prefix.is_empty() {
                        flat.push(resolved);
                    } else if let Some((inner_elements, _)) =
                        self.tuple_element_lists.get(&resolved).cloned()
                    {
                        flat.extend(inner_elements);
                    } else {
                        spliced = None;
                    }
                }
                pieces.push(format!("{prefix}{}", self.type_to_string(resolved)));
            }
            if let Some(flat) = spliced {
                let readonly = node
                    .node_id
                    .and_then(|id| self.nodes.parent(id))
                    .is_some_and(|parent| self.is_readonly_type_operator(parent));
                return self.create_tuple_type(flat, readonly);
            }
            let readonly = node
                .node_id
                .and_then(|id| self.nodes.parent(id))
                .is_some_and(|parent| self.is_readonly_type_operator(parent));
            let text =
                format!("{}[{}]", if readonly { "readonly " } else { "" }, pieces.join(", "));
            let minted = self.store.new_named(TypeFlags::OBJECT, text, None);
            // §87 (`checker-notes-narrow.md`): a variadic whose ONLY rest is
            // one TRAILING `...T[]` records its NODE so positional consumers
            // (§86's contextual expansion) can resolve it AT CONSUMPTION —
            // resolving eagerly here perturbed an unrelated JSX case's whole
            // alignment (16 lines, `unicodeEscapesInJsxtags`); the print-only
            // road stays lazy.
            if let [prefix @ .., TypeNode::RestTypeNode(rest)] = node.elements
                && !prefix.iter().any(|e| matches!(e, TypeNode::RestTypeNode(_)))
                && matches!(rest.r#type, Some(TypeNode::ArrayTypeNode(_)))
                && let Some(id) = node.node_id
            {
                self.tuple_rest_tails.insert(minted, id);
            }
            return minted;
        }
        let mut any_marked = false;
        let mut labels: Vec<Option<String>> = Vec::with_capacity(node.elements.len());
        for element in node.elements {
            // Rests stay refused whole. §79 (`checker-notes-narrow.md`): an
            // OPTIONAL element resolves its inner type and marks the
            // position — the print carries the `?` (`[number, string?,
            // boolean?]`), the element list carries the members, and index
            // reads consult the mask. §80: a LABELED member (`[first:
            // string]`) carries its label into the print the same way; a
            // labeled REST keeps the decline.
            let (inner, optional, label) = match element {
                TypeNode::RestTypeNode(_) => return error,
                TypeNode::NamedTupleMember(member) => {
                    if member.dot_dot_dot_token.is_some() {
                        return error;
                    }
                    let (Some(inner), Some(name)) = (member.r#type, member.name) else {
                        return error;
                    };
                    any_marked = true;
                    (inner, member.question_token.is_some(), Some(name.text.to_string()))
                }
                TypeNode::OptionalTypeNode(optional) => {
                    let Some(inner) = optional.r#type else { return error };
                    any_marked = true;
                    (inner, true, None)
                }
                other => (*other, false, None),
            };
            let resolved = self.get_type_from_type_node(inner);
            // A gap in an element is a gap in the tuple, the rule the array arm
            // and `get_instantiated_type_reference` both use.
            if resolved == error {
                return error;
            }
            elements.push((resolved, optional));
            labels.push(label);
        }
        if any_marked {
            let readonly = node
                .node_id
                .and_then(|id| self.nodes.parent(id))
                .is_some_and(|parent| self.is_readonly_type_operator(parent));
            return self.create_optional_tuple_type(&elements, &labels, readonly);
        }
        let elements: Vec<TypeId> = elements.into_iter().map(|(t, _)| t).collect();
        let readonly = node
            .node_id
            .and_then(|id| self.nodes.parent(id))
            .is_some_and(|parent| self.is_readonly_type_operator(parent));
        self.create_tuple_type(elements, readonly)
    }

    /// Mint (or reuse) the tuple type for an element list.
    ///
    /// Extracted from [`Checker::get_type_from_tuple_type_node`] when
    /// `bd tsr-84iz` needed a tuple built from an **array literal's** element
    /// types rather than from a type node. Shared rather than copied, and that
    /// is load-bearing: `tsr-5ll`'s comparator and `tsr-o00`'s element lookup
    /// both key on `tuple_element_lists`, and the interning on
    /// `(elements, readonly)` is what makes `[number, string]` written twice —
    /// once as an annotation, once inferred from `[1, "x"]` — **one** type.
    /// Two minting sites would produce two ids that print alike and compare
    /// unequal.
    /// §79: a tuple with OPTIONAL elements — printed with `?` markers,
    /// registered in `tuple_element_lists` on the PLAIN member list so
    /// context tests and access see the members. Interned separately from
    /// the all-required spelling.
    fn create_optional_tuple_type(
        &mut self,
        elements: &[(TypeId, bool)],
        labels: &[Option<String>],
        readonly: bool,
    ) -> TypeId {
        let key = (elements.to_vec(), labels.to_vec(), readonly);
        if let Some(&cached) = self.optional_tuple_types.get(&key) {
            return cached;
        }
        let printed = elements
            .iter()
            .zip(labels)
            .map(|(&(element, optional), label)| {
                let text = self.type_to_string(element);
                match label {
                    // §80: the label owns the `?` — `[first?: string]`,
                    // never `[first: string?]`.
                    Some(label) if optional => format!("{label}?: {text}"),
                    Some(label) => format!("{label}: {text}"),
                    None if optional => format!("{text}?"),
                    None => text,
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let printed =
            if readonly { format!("readonly [{printed}]") } else { format!("[{printed}]") };
        let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
        let plain: Vec<TypeId> = elements.iter().map(|&(t, _)| t).collect();
        let mask: Vec<bool> = elements.iter().map(|&(_, optional)| optional).collect();
        self.tuple_element_lists.insert(id, (plain, readonly));
        self.tuple_optional_masks.insert(id, mask);
        self.optional_tuple_types.insert(key, id);
        id
    }

    pub(crate) fn create_tuple_type(&mut self, elements: Vec<TypeId>, readonly: bool) -> TypeId {
        if let Some(&cached) = self.tuple_types.get(&(elements.clone(), readonly)) {
            return cached;
        }
        let printed = elements
            .iter()
            .map(|&element| self.type_to_string(element))
            .collect::<Vec<_>>()
            .join(", ");
        // `[]` for the empty tuple, which is what the baselines record — 90
        // instances, the second most common tuple text in the corpus.
        let printed =
            if readonly { format!("readonly [{printed}]") } else { format!("[{printed}]") };
        let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
        self.tuple_element_lists.insert(id, (elements.clone(), readonly));
        self.tuple_types.insert((elements, readonly), id);
        id
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
        self.global_type_symbol_with_arity(name, 1)
    }

    /// [`Checker::global_type_symbol`] at an explicit arity — `getGlobalType`'s
    /// real signature (`checker.go`, `arity int`); `Generator` is the first
    /// caller to need one other than 1.
    pub(crate) fn global_type_symbol_with_arity(
        &self,
        name: &str,
        arity: usize,
    ) -> Option<SymbolId> {
        let symbol = *self.binder.globals().get(name)?;
        (self.local_type_parameters_of(symbol).len() == arity).then_some(symbol)
    }

    /// Ported from `Checker.getAliasSymbolForTypeNode` (`checker.go:23719`).
    ///
    /// The host is the nearest ancestor that is not a parenthesised type or a
    /// `readonly` type operator, and it names the type only when it is a type
    /// alias declaration.
    pub(crate) fn alias_symbol_for_type_node(&self, node: tsr_ast::NodeId) -> Option<SymbolId> {
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
    /// new type identities forever. This function only *interns* the pair; the
    /// recursion those limits exist to stop lives in
    /// [`Checker::instantiate_type`](crate::Checker::instantiate_type), which is
    /// where they are now ported (`bd tsr-el3.2`). Putting a second copy here
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
        // §36's second contained leg: an alias whose body is a CONDITIONAL
        // type over CONCRETE arguments is EVALUATED upstream (`Foo1<"*x*">`
        // answers the branch, `templateLiteralTypes3`); the written
        // reference is a wrong line there. Deferred arguments keep it.
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && (matches!(alias.r#type, Some(TypeNode::ConditionalTypeNode(_)))
                || matches!(alias.r#type, Some(TypeNode::KeywordTypeNode(keyword))
                    if keyword.kind == SyntaxKind::IntrinsicKeyword))
            && self.in_alias_declared_position(node.node_id)
        {
            // §92.1: §91's evaluator now answers the computable slice of
            // this decline (extends-never conditionals over concrete
            // arguments); everything it refuses keeps the honest gap.
            if let Some(evaluated) = self.evaluate_conditional_alias(symbol, &arguments) {
                return evaluated;
            }
            // Upstream evaluates conditional aliases in alias-declared
            // positions even through type-parameter arguments
            // (`PrefixData<P>` answers `\`${P}:baz\``).
            return error;
        }
        // §46 (`checker-notes-narrow.md`): a generic ALIAS reference whose
        // body is a type literal answers the §41 shape — name+args print,
        // the body's member symbol, the seam registration.
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(TypeNode::TypeLiteralNode(literal)) = alias.r#type
            && let Some(literal_id) = literal.node_id
            && let Some(body_symbol) = self.binder.symbol_of(literal_id)
        {
            let printed_arguments: Vec<String> =
                arguments.iter().map(|&a| self.type_to_string(a)).collect();
            let name = self.binder.symbols().get(symbol).name.to_string();
            let text = format!("{name}<{}>", printed_arguments.join(", "));
            let key = (text.clone(), symbol);
            if let Some(&existing) = self.qualified_reference_types.get(&key) {
                return existing;
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(body_symbol));
            self.qualified_reference_types.insert(key, minted);
            self.type_reference_targets.insert(minted, (symbol, arguments));
            return minted;
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
        // §36's first contained leg: an INTRINSIC string mapping over
        // CONCRETE arguments is EVALUATED upstream (`Uppercase<"aA">` is
        // `"AA"`), so printing the written call is a wrong line; deferred
        // arguments (type parameters, mints) keep the written print.
        if matches!(text.as_str(), "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize")
            && self.in_alias_declared_position(node.node_id)
        {
            // Upstream EVALUATES string mappings in alias-declared positions
            // whatever the argument — including through patterns and even
            // idempotence (`Uppercase<Uppercase<string>>` reduces). No
            // written print survives there.
            return self.intrinsics.error;
        }
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

    /// §36's positional gate: upstream's node builder REUSES written
    /// annotation nodes, so a reference in an ANNOTATION prints as written
    /// whatever it would evaluate to; only the DECLARED type of an alias
    /// (`type B = Uppercase<A>` — `B`'s own line) shows the evaluation.
    /// True when the node sits under a `TypeAliasDeclaration` with only
    /// type-node ancestry between.
    fn in_alias_declared_position(&self, node: Option<tsr_ast::NodeId>) -> bool {
        let Some(mut current) = node else { return false };
        loop {
            let Some(parent) = self.nodes.parent(current) else { return false };
            if self.nodes.kind(parent) == SyntaxKind::TypeAliasDeclaration {
                return true;
            }
            let is_type_node =
                self.node_map.get(parent).is_some_and(|node| TypeNode::try_from(node).is_ok());
            if !is_type_node {
                return false;
            }
            current = parent;
        }
    }

    /// A type reference `M.I` whose leftmost name **does** resolve as a
    /// namespace, printed as the entity name that was written.
    ///
    /// Anchored to `resolveQualifiedName` (`checker.go:15828`) for the
    /// resolution and to `needsQualification` (`symbolaccessibility.go:688`) for
    /// the refusal. **Design W** of
    /// [`docs/architecture/checker-notes-qualname.md`](../../../docs/architecture/checker-notes-qualname.md),
    /// with that page's positional refusal.
    ///
    /// # Why the *written* text and not the symbol's name
    ///
    /// Upstream resolves `M.I` to `I`'s symbol and then hands the printing to
    /// `getSymbolChain` (`nodebuilderimpl.go:1087`), which re-derives a
    /// qualifier from the reference site. That chain is unported, and the
    /// obvious substitute — resolve and print the symbol's bare name — is
    /// **design R** on that page and is refused there with a number: 384
    /// conversions against 1,025 newly wrong lines, because upstream's baselines
    /// want the qualifier wherever the source bothered to write one. Reprinting
    /// the written text is the design whose forecast is 1,702 conversions
    /// against 20.
    ///
    /// # The one refusal, and it is upstream's rule rather than a knob
    ///
    /// `needsQualification` (`symbolaccessibility.go:688`) answers *no qualifier
    /// needed* the moment a symbol table in scope holds the symbol itself
    /// (`symbolaccessibility.go:701`). Inside `namespace M`, `I` is in scope, so
    /// upstream prints `I` and the written `M.I` is over-qualified by
    /// construction. 79 of design W's 99 would-be-wrong lines come from exactly
    /// that position against only 68 conversions, so the site being inside the
    /// namespace it qualifies is refused.
    ///
    /// [`Checker::site_is_inside_namespace`] is a deliberate **over**-approximation
    /// of `needsQualification`: it also refuses a site inside the namespace's own
    /// enclosing namespace, where upstream might still qualify. Over-approximating
    /// a refusal can only cost conversions, never add wrong lines, and it is the
    /// predicate the counterfactual's 1,702/20 was measured with — a build that
    /// narrowed it would be reporting under a forecast it did not compute.
    ///
    /// # The answer is still a gap, and that is what makes this unable to lose
    ///
    /// The type minted here is [`Checker::unresolved_type_reference`]'s, so
    /// [`Checker::is_error`] stays true and every consumer downstream — the
    /// arithmetic arm, `+`, the union worker, property access — keeps
    /// propagating a gap. **Only the line that renders this node changes.** That
    /// is a stated deviation from upstream, which has a real type here: a
    /// resolved `M.I` participates in assignability and property lookup, and
    /// this port declines all of that rather than approximating it from a
    /// symbol whose declared type it has not asked for. The registered bar was
    /// `lost == 0`, and a mechanism that cannot move a line it does not render
    /// is how that is met by construction rather than by measurement.
    /// §60: the heritage-EXPRESSION twin of the §42-v2 qualified mint — a
    /// `class B extends N.C<A>` base prints the QUALIFIED instantiated
    /// spelling. Public for the types producer's extends compensation.
    pub fn qualified_heritage_reference(
        &mut self,
        text: String,
        symbol: SymbolId,
        arguments: Vec<TypeId>,
    ) -> TypeId {
        let printed_arguments: Vec<String> =
            arguments.iter().map(|&a| self.type_to_string(a)).collect();
        let text = if arguments.is_empty() {
            text
        } else {
            format!("{text}<{}>", printed_arguments.join(", "))
        };
        let key = (text.clone(), symbol);
        if let Some(&existing) = self.qualified_reference_types.get(&key) {
            return existing;
        }
        let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(symbol));
        self.qualified_reference_types.insert(key, minted);
        if !arguments.is_empty() {
            self.type_reference_targets.insert(minted, (symbol, arguments));
        }
        minted
    }

    fn qualified_type_reference(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        name: tsr_ast::EntityName<'a>,
        namespace: SymbolId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(site) = node.node_id else { return error };
        if self.site_is_inside_namespace(site, namespace) {
            return error;
        }
        // W still has to *resolve* to answer at all: a name upstream cannot
        // resolve is a different bucket, and printing text for it here would be
        // inventing an export that does not exist.
        let Some(resolved) = self.resolve_entity_name(name, SymbolFlags::TYPE) else {
            return error;
        };
        // §41 (`checker-notes-narrow.md`): the resolved, argument-less
        // qualified reference answers a members-CARRYING named type — the
        // qualified print with the real lookup table. Generic references
        // stay print-only mints.
        if node.type_arguments.is_empty() {
            let Some(text) = Self::entity_name_text(node.type_name) else { return error };
            let key = (text.clone(), resolved);
            if let Some(&existing) = self.qualified_reference_types.get(&key) {
                return existing;
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(resolved));
            self.qualified_reference_types.insert(key, minted);
            return minted;
        }
        // §42 v2 (`checker-notes-narrow.md`): the GENERIC qualified
        // reference builds arity-checked arguments, prints the QUALIFIED
        // spelling, and registers in `type_reference_targets` so the
        // tsr-4qx seam and instantiation both see a real reference.
        let parameters = self.local_type_parameters_of(resolved).len();
        if parameters > 0 && node.type_arguments.len() == parameters {
            let mut arguments = Vec::with_capacity(parameters);
            for argument in node.type_arguments {
                let image = self.get_type_from_type_node(*argument);
                if image == error {
                    return error;
                }
                arguments.push(image);
            }
            let Some(base) = Self::entity_name_text(node.type_name) else { return error };
            let printed_arguments: Vec<String> =
                arguments.iter().map(|&a| self.type_to_string(a)).collect();
            let text = format!("{base}<{}>", printed_arguments.join(", "));
            let key = (text.clone(), resolved);
            if let Some(&existing) = self.qualified_reference_types.get(&key) {
                return existing;
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(resolved));
            self.qualified_reference_types.insert(key, minted);
            self.type_reference_targets.insert(minted, (resolved, arguments));
            return minted;
        }
        self.unresolved_type_reference(node)
    }

    /// `resolveEntityName` (`checker.go:15772`) for the two arms a type
    /// reference can take.
    ///
    /// The qualified arm is `resolveQualifiedName` (`checker.go:15828`):
    /// resolve the left with meaning `SymbolFlagsNamespace`, then look the
    /// right-hand text up in `getExportsOfSymbol(namespace)`
    /// (`checker.go:15851`). The resolution site is the name's own node, which
    /// is upstream's default — `resolveEntityName` falls back to `name` when its
    /// `location` is nil (`checker.go:15789`), and every call reaching a type
    /// reference passes nil.
    ///
    /// **Three of upstream's arms are not ported and each is a `None` rather
    /// than an approximation**: the `export =` re-resolution through
    /// `resolveAlias` when the export lookup misses (`checker.go:15855`), the
    /// `CommonJS` `require` redirect (`checker.go:15836`), and the alias chain
    /// upstream walks when the found symbol lacks the wanted meaning
    /// (`checker.go:15820`). An `ALIAS` is accepted here without being resolved,
    /// which is what `BindResult::resolve_name`'s own `lookup_scoped` already
    /// does for the leftmost name; resolving it needs `bd tsr-4jk`'s machinery.
    fn resolve_entity_name(
        &self,
        name: tsr_ast::EntityName<'a>,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        match name {
            tsr_ast::EntityName::Identifier(identifier) => self.binder.resolve_name(
                self.nodes,
                self.node_map,
                identifier.node_id?,
                identifier.text,
                meaning,
            ),
            tsr_ast::EntityName::QualifiedName(qualified) => {
                let namespace =
                    self.resolve_entity_name(qualified.left?, SymbolFlags::NAMESPACE)?;
                let right = qualified.right?;
                let found = *self.binder.symbols().get(namespace).exports.get(right.text)?;
                let found = self.binder.merged_symbol(found);
                let flags = self.binder.symbols().get(found).flags;
                (flags.intersects(meaning) || flags.intersects(SymbolFlags::ALIAS)).then_some(found)
            }
        }
    }

    /// Whether the reference site sits inside the namespace it is qualifying —
    /// the position `needsQualification` (`symbolaccessibility.go:688`) answers
    /// *no qualifier needed* for.
    ///
    /// Two tests, and the second is the over-approximation
    /// [`Checker::qualified_type_reference`] documents: the site is inside one
    /// of the namespace's own declarations, **or** inside the nearest
    /// `ModuleDeclaration` enclosing one of them. Upstream recovers containers
    /// the same way — `getContainersOfSymbol` (`symbolaccessibility.go:280`)
    /// walks declarations, because `Symbol.Parent` is left off for locals.
    fn site_is_inside_namespace(&self, site: NodeId, namespace: SymbolId) -> bool {
        let declarations = &self.binder.symbols().get(namespace).declarations;
        if declarations.iter().any(|&declaration| Self::is_inside(self.nodes, site, declaration)) {
            return true;
        }
        for &declaration in declarations {
            let mut current = self.nodes.parent(declaration);
            while let Some(id) = current {
                if matches!(self.node_map.get(id), Some(Node::ModuleDeclaration(_))) {
                    return Self::is_inside(self.nodes, site, id);
                }
                current = self.nodes.parent(id);
            }
        }
        false
    }

    /// Whether `node` is `container` or sits beneath it.
    fn is_inside(nodes: &tsr_ast::NodeTable, node: NodeId, container: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            if id == container {
                return true;
            }
            current = nodes.parent(id);
        }
        false
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

    /// [`Checker::create_type_reference`] for callers outside the crate — the
    /// conformance producer's heritage-instantiation branch is the one
    /// consumer (`checker-notes-jsx.md`, the ts-slice bar).
    pub fn create_type_reference_public(
        &mut self,
        target: tsr_binder::SymbolId,
        arguments: Vec<crate::types::TypeId>,
    ) -> crate::types::TypeId {
        self.create_type_reference(target, arguments)
    }

    /// §46/§90's shared admission: the symbol of a `TYPE_ALIAS`'s `TypeLiteral`
    /// body, if it has one. The member table an instantiated alias reference
    /// answers property lookups from.
    fn alias_body_literal_symbol(&self, symbol: SymbolId) -> Option<SymbolId> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(TypeNode::TypeLiteralNode(literal)) = alias.r#type else { return None };
        self.binder.symbol_of(literal.node_id?)
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
        // §90 (`checker-notes-narrow.md`): a TYPE_ALIAS target with a
        // TypeLiteral body mints the BODY's symbol — §46's admission, one
        // road lower, so `instantiate_type`'s arm-3 rebuilds keep their
        // members and `o2.merge` resolves like `o1.merge` did.
        let member_symbol = self.alias_body_literal_symbol(symbol).unwrap_or(symbol);
        // `OBJECT` even when the target is a type alias, where upstream\'s
        // instantiated type carries the flags of the alias\'s *body*. The flags
        // are consulted by the arithmetic and `+` arms, and claiming
        // `Alias<number>` is string- or number-like would be worse than claiming
        // it is an object: `object` is the one answer those arms treat as
        // neither.
        // `Some(symbol)`: the reference's properties are looked up in the
        // target's members table. **This is only safe because every consumer
        // that turns a found property into a type goes through
        // `get_type_of_property_of_type`** (`crate::members`), which
        // instantiates the property's declared type through
        // `type_reference_targets` — otherwise `c.a` on a `C<number>` whose
        // member is declared `a: T` would answer `T` where upstream answers
        // `number`, which is why this field was `None` from this type's
        // creation until `bd tsr-4qx` step 4 flipped it. The routing of the
        // three consumers (property access, element access, the relater) landed
        // first and separately (`8fa6a3e`) so that this flip and the seam's
        // instantiation could be one commit.
        let id = self.store.new_named(TypeFlags::OBJECT, printed, Some(member_symbol));
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
        let text = crate::printing::type_to_string(self.store.get(element));
        self.wrap_array_element_text(element, &text)
    }

    /// The parenthesisation half of [`Checker::array_element_text`], shared
    /// with §95's site-aware reference rebuild so both roads wrap by the same
    /// rules whatever text the element rendered as.
    pub(crate) fn wrap_array_element_text(&self, element: TypeId, text: &str) -> String {
        let ty = self.store.get(element);
        let wrap = (ty.flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION)
            && !crate::printing::prints_as_a_single_token(ty))
            || text.starts_with("typeof ")
            // §35: a deferred `keyof T` mint under `[]` binds wrongly
            // unwrapped — `keyof T[]` is keyof-of-array
            // (`keyofIsLiteralContexualType` wants `(keyof T)[]`).
            || text.starts_with("keyof ")
            || has_top_level_arrow(text);
        if wrap { format!("({text})") } else { text.to_string() }
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
            {
                // Record the symbol behind the type parameter, which
                // `new_named_type` deliberately does not put in `members`.
                // `getApparentType`'s head reads the `extends` constraint back
                // through this index (`bd tsr-rppd`,
                // `Checker::type_parameter_symbols`).
                let id = self.new_named_type(symbol, TypeFlags::TYPE_PARAMETER, false);
                self.type_parameter_symbols.insert(id, symbol);
                id
            }
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
        // §55 (`checker-notes-narrow.md`): the sequential constant folder.
        // `None` = computed; auto-increment dies after a string or computed
        // predecessor, per the language.
        #[derive(Clone, PartialEq)]
        enum MemberValue {
            Num(f64),
            Str(String),
        }
        // §55: fold the member's value. Identifier and same-enum
        // qualified references reach PRIOR members only.
        fn eval(
            expr: &tsr_ast::Expression<'_>,
            enum_name: &str,
            folded: &[(String, Option<MemberValue>)],
        ) -> Option<MemberValue> {
            match expr {
                tsr_ast::Expression::NumericLiteral(n) => {
                    n.text.parse::<f64>().ok().map(MemberValue::Num)
                }
                tsr_ast::Expression::StringLiteral(s) => Some(MemberValue::Str(s.text.to_string())),
                tsr_ast::Expression::PrefixUnaryExpression(u) => {
                    let inner =
                        u.operand.as_ref().and_then(|operand| eval(operand, enum_name, folded))?;
                    match (&inner, u.operator.kind) {
                        (MemberValue::Num(n), SyntaxKind::MinusToken) => Some(MemberValue::Num(-n)),
                        (MemberValue::Num(n), SyntaxKind::PlusToken) => Some(MemberValue::Num(*n)),
                        _ => None,
                    }
                }
                tsr_ast::Expression::Identifier(identifier) => folded
                    .iter()
                    .rev()
                    .find(|(n, _)| n == identifier.text)
                    .and_then(|(_, v)| v.clone()),
                tsr_ast::Expression::PropertyAccessExpression(access) => {
                    let receiver = match access.expression {
                        Some(tsr_ast::Expression::Identifier(r)) => r.text,
                        _ => return None,
                    };
                    if receiver != enum_name {
                        return None;
                    }
                    let member_name = match access.name {
                        Some(tsr_ast::MemberName::Identifier(n)) => n.text,
                        _ => return None,
                    };
                    folded.iter().rev().find(|(n, _)| n == member_name).and_then(|(_, v)| v.clone())
                }
                _ => None,
            }
        }
        let declarations =
            self.binder.symbols().get(symbol).declarations.iter().copied().collect::<Vec<_>>();
        let name = self.binder.symbols().get(symbol).name.to_string();
        let mut members = Vec::new();
        // §55.1: a single-MEMBER enum's one literal prints as the ENUM
        // itself (`Enum.A : Enum`, `classStaticInitializersUseProperties…`,
        // `enumAssignabilityInInheritance` — 261 corpus lines); multi-member
        // enums keep per-value names (`E9.A`). Counted across merged
        // declarations, bindable members only.
        let total_members: usize = declarations
            .iter()
            .filter_map(|&declaration| match self.node_map.get(declaration) {
                Some(Node::EnumDeclaration(node)) => Some(
                    node.members
                        .iter()
                        .filter(|member| {
                            member.node_id.and_then(|id| self.binder.symbol_of(id)).is_some()
                        })
                        .count(),
                ),
                _ => None,
            })
            .sum();
        let canonical = |value: &MemberValue| match value {
            MemberValue::Num(n) => format!("n:{n}"),
            MemberValue::Str(s) => format!("s:{s}"),
        };
        let mut folded: Vec<(String, Option<MemberValue>)> = Vec::new();
        for declaration in declarations {
            let Some(Node::EnumDeclaration(node)) = self.node_map.get(declaration) else {
                continue;
            };
            // An AMBIENT non-const enum has NO auto-increment: its
            // initializer-less members are opaque upstream
            // (`ambientDeclarations`' E2 — auto `b` beside `c = 2` stays
            // two members because `b` never folds to 2).
            let is_ambient = node.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                    if token.kind == SyntaxKind::DeclareKeyword)
            });
            let is_const = node.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                    if token.kind == SyntaxKind::ConstKeyword)
            });
            let no_auto = is_ambient && !is_const;
            let mut auto: Option<f64> = Some(0.0);
            for member in node.members {
                // `hasBindableName` (`checker.go:23879`), asked of the binder: a
                // member the binder gave no symbol to is one whose name is a
                // non-literal computed expression, and upstream skips it too.
                let Some(member_symbol) = member.node_id.and_then(|id| self.binder.symbol_of(id))
                else {
                    continue;
                };
                let member_name = self.binder.symbols().get(member_symbol).name.to_string();
                // `symbol: None` is LOAD-BEARING, measured (§10.16 of
                // `checker-notes-modobj.md`): carrying the member symbol here
                // let `qualified_name_at` rename the baked `{enum}.` prefix —
                // 6 wanted lines in `exportAssignmentEnum` — and broke the
                // enum-union collapse on 100+ others (`enumOperations` printed
                // `Enum.None` where `Enum` is wanted, 2 cases regressed). The
                // collapse distinguishes the enum type from a member by this
                // very field; the §10.16 build kept mechanism (a) and reverted
                // this one on that measurement.
                // A member whose name is not identifier text spells as an
                // indexed access — `>"" : (typeof ENUM1)[""]`
                // (`negateOperatorWithEnumType.types:13`;
                // `checker-notes-narrow.md` §11). The dotted form would print
                // `ENUM1.` with an empty right side.
                // ASCII first, then a Unicode approximation: `Ϳ` is
                // identifier text upstream (`enumMemberNameNonIdentifier`
                // records `E.Ϳ` — the §11 bar's falsifier caught the
                // ASCII-only test costing 4 such lines), and `char`'s
                // alphabetic/alphanumeric classes are close enough to
                // ID_Start/ID_Continue for every name the corpus holds.
                let identifier_like = crate::symbols::is_identifier_text(&member_name)
                    || (!member_name.is_empty()
                        && member_name.chars().enumerate().all(|(index, c)| {
                            if index == 0 {
                                c.is_alphabetic() || c == '_' || c == '$'
                            } else {
                                c.is_alphanumeric() || c == '_' || c == '$'
                            }
                        }));
                // §55.1: the single-member split mints DIVERGENT twins —
                // regular spelled as the enum, fresh spelled per-name — and
                // registers the access-road swap.
                if total_members == 1 {
                    let regular = self.store.new_named(TypeFlags::ENUM, name.clone(), None);
                    let per_name = if identifier_like {
                        format!("{name}.{member_name}")
                    } else {
                        format!("(typeof {name})[{}]", crate::printing::quote(&member_name))
                    };
                    let fresh = self.store.intern_literal(
                        TypeFlags::ENUM,
                        crate::types::TypeData::Named { text: per_name, members: None },
                        true,
                    );
                    self.enum_member_owners.insert(regular, symbol);
                    self.enum_member_owners.insert(fresh, symbol);
                    self.enum_member_regular.insert(fresh, regular);
                    self.enum_access_spelling.insert(fresh, regular);
                    self.declared_types.insert(member_symbol, fresh);
                    members.push(regular);
                    continue;
                }
                let member_text = if identifier_like {
                    format!("{name}.{member_name}")
                } else {
                    format!("(typeof {name})[{}]", crate::printing::quote(&member_name))
                };
                let value: Option<MemberValue> = match member.initializer {
                    None if no_auto => None,
                    None => auto.map(MemberValue::Num),
                    Some(ref expr) => eval(expr, &name, &folded),
                };
                auto = match &value {
                    Some(MemberValue::Num(n)) => Some(n + 1.0),
                    _ => None,
                };
                folded.push((member_name.clone(), value.clone()));
                // Value-keyed interning: a later member with a seen value
                // REUSES the first member's type (`B = A` prints `E9.A`);
                // a computed member's type IS the enum
                // (`createComputedEnumType`, `E8.B : E8`).
                if let Some(value) = &value {
                    let key = (symbol, canonical(value));
                    if let Some(&existing) = self.enum_value_types.get(&key) {
                        let fresh = self.get_fresh_type_of_literal_type(existing);
                        self.declared_types.insert(member_symbol, fresh);
                        continue;
                    }
                }
                // §55's second split, from `enumBasics2`: a VALID computed
                // member (`'foo'.length`) is the enum's own type, but an
                // ERROR-VALUED one (`a.b` where `a` is a number-typed
                // member) keeps its per-name literal — upstream's evaluator
                // error path. Classified by checking the initializer, which
                // is safe mid-fold because prior members' declared types are
                // already inserted.
                // A COMPUTED member keeps its per-name literal in this
                // slice: `enumBasics2` wants `Bar.a` for `(1).valueOf()`.
                // The single-distinct-value spelling split (`E8.B : E8`
                // beside `B : E8.A`-style declaration prints) is the
                // recorded residue — it needs fresh/regular SPELLING
                // divergence, §55's postscript.
                let member_type = self.store.new_named(TypeFlags::ENUM, member_text, None);
                if let Some(value) = &value {
                    self.enum_value_types.insert((symbol, canonical(value)), member_type);
                }
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
                // The §18 back-link: the fresh form's REGULAR twin is the
                // union's own constituent, not an interned lookalike.
                self.enum_member_regular.insert(fresh, member_type);
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
    pub(crate) fn get_declared_type_of_class_or_interface(&mut self, symbol: SymbolId) -> TypeId {
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
        // §29: a mention of the alias inside its own resolution answers the
        // memoized NAME placeholder — upstream's laziness, at the one seam
        // print-at-creation permits. No failure marking, so the outer
        // resolution completes.
        if self.resolutions.on_stack(symbol, PropertyName::DeclaredType) {
            if let Some(&placeholder) = self.alias_placeholders.get(&symbol) {
                return placeholder;
            }
            let name = self.binder.symbols().get(symbol).name.to_string();
            let placeholder = self.store.new_named(TypeFlags::OBJECT, name, None);
            self.alias_placeholders.insert(symbol, placeholder);
            return placeholder;
        }
        if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
            return error;
        }
        // §33's containment (`checker-notes-callres.md`): a body whose
        // signatures carry a CONST type parameter is a shape this build
        // newly admits; upstream's declared type keeps the ALIAS's own name
        // (`>T2 : T2`), and expanding it printed the signature — 14 G→W in
        // the first pair. Scoped to the const shape: the broader
        // alias-name-on-anonymous-body question is its own bar.
        if Self::alias_body_has_const_type_parameter(type_node) {
            if !self.resolutions.pop() {
                return error;
            }
            let name = self.binder.symbols().get(symbol).name.to_string();
            let members = type_node.node_id().and_then(|id| self.binder.symbol_of(id));
            return self.store.new_named(TypeFlags::OBJECT, name, members);
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

    /// §33: whether an alias body is a function type, constructor type, or
    /// type literal whose signature members carry a `const` type parameter.
    fn alias_body_has_const_type_parameter(type_node: TypeNode<'a>) -> bool {
        let has_const = |parameters: &[&tsr_ast::TypeParameterDeclaration<'_>]| {
            parameters.iter().any(|parameter| {
                parameter.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token)
                        if token.kind == SyntaxKind::ConstKeyword)
                })
            })
        };
        match type_node {
            TypeNode::FunctionTypeNode(node) => has_const(node.type_parameters),
            TypeNode::ConstructorTypeNode(node) => has_const(node.type_parameters),
            TypeNode::TypeLiteralNode(literal) => {
                literal.members.iter().any(|member| match member {
                    tsr_ast::TypeElement::CallSignatureDeclaration(node) => {
                        has_const(node.type_parameters)
                    }
                    tsr_ast::TypeElement::ConstructSignatureDeclaration(node) => {
                        has_const(node.type_parameters)
                    }
                    _ => false,
                })
            }
            _ => false,
        }
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
    /// §91 (`checker-notes-narrow.md`): evaluate a CONDITIONAL alias body
    /// over the given arguments, or `None` when any part is not computable —
    /// the caller falls back to the named reference, so a refusal here costs
    /// a name print, never a wrong line.
    ///
    /// Only the `extends never` form is admitted; the check must evaluate to
    /// a literal-key union (empty → true branch, upstream's
    /// `getConditionalTypeInstantiation` resolution for a concrete check).
    pub(crate) fn evaluate_conditional_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(TypeNode::ConditionalTypeNode(conditional)) = alias.r#type else { return None };
        if !matches!(conditional.extends_type, Some(TypeNode::KeywordTypeNode(keyword))
            if keyword.kind == SyntaxKind::NeverKeyword)
        {
            return None;
        }
        let parameters = self.local_type_parameters_of(symbol);
        if parameters.len() != arguments.len() {
            return None;
        }
        let mut frame = rustc_hash::FxHashMap::default();
        for (parameter, &argument) in parameters.iter().zip(arguments) {
            let parameter = parameter.node_id.and_then(|id| self.binder.symbol_of(id))?;
            frame.insert(parameter, argument);
        }
        // The same guard as `instantiate_type`: a self-recursive conditional
        // alias re-enters here through the branch's own references.
        if self.instantiation_depth == 100 {
            return None;
        }
        self.instantiation_depth += 1;
        self.alias_evaluation_bindings.push(frame);
        let error = self.intrinsics.error;
        let mut result = None;
        if let Some(check_node) = conditional.check_type {
            let check = self.get_type_from_type_node(check_node);
            if check != error
                && let Some(keys) = self.literal_key_texts(check)
            {
                let branch =
                    if keys.is_empty() { conditional.true_type } else { conditional.false_type };
                if let Some(branch) = branch {
                    let evaluated = self.get_type_from_type_node(branch);
                    if evaluated != error {
                        result = Some(evaluated);
                    }
                }
            }
        }
        self.alias_evaluation_bindings.pop();
        self.instantiation_depth -= 1;
        if let Some(evaluated) = result {
            self.alias_evaluated_types.insert(evaluated);
        }
        result
    }

    /// §92: evaluate ANY generic alias body under bindings — the
    /// non-conditional generalization of [`Checker::evaluate_conditional_alias`],
    /// used by the property road to see through `merge<X, Y>` when the body is
    /// an intersection. Cached per (symbol, arguments); `None` when the body
    /// does not evaluate, which keeps the named reference as the answer.
    pub(crate) fn evaluate_alias_body(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let key = (symbol, arguments.to_vec());
        if let Some(&cached) = self.alias_body_evaluations.get(&key) {
            return (cached != self.intrinsics.error).then_some(cached);
        }
        if let Some(evaluated) = self.evaluate_conditional_alias(symbol, arguments) {
            self.alias_body_evaluations.insert(key, evaluated);
            self.alias_evaluated_types.insert(evaluated);
            return Some(evaluated);
        }
        let error = self.intrinsics.error;
        let mut result = None;
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(body) = alias.r#type
            && !matches!(body, TypeNode::ConditionalTypeNode(_))
            // A TypeLiteral anywhere in the body's structural spine belongs
            // to the §90 symbol road, whose member reads instantiate;
            // evaluating one here mints WRITTEN member types (`T | undefined`
            // for a bound T — 19 G→W in controlFlowAliasedDiscriminants,
            // whose `UseQueryResult<T>` is a union of two literals).
            && !Self::body_carries_type_literal(body)
            && self.instantiation_depth < 100
        {
            let parameters = self.local_type_parameters_of(symbol);
            if parameters.len() == arguments.len() && !parameters.is_empty() {
                let mut frame = rustc_hash::FxHashMap::default();
                let mut complete = true;
                for (parameter, &argument) in parameters.iter().zip(arguments) {
                    match parameter.node_id.and_then(|id| self.binder.symbol_of(id)) {
                        Some(parameter) => {
                            frame.insert(parameter, argument);
                        }
                        None => complete = false,
                    }
                }
                if complete {
                    self.instantiation_depth += 1;
                    self.alias_evaluation_bindings.push(frame);
                    let evaluated = self.get_type_from_type_node(body);
                    self.alias_evaluation_bindings.pop();
                    self.instantiation_depth -= 1;
                    if evaluated != error {
                        result = Some(evaluated);
                    }
                }
            }
        }
        self.alias_body_evaluations.insert(key, result.unwrap_or(error));
        if let Some(evaluated) = result {
            self.alias_evaluated_types.insert(evaluated);
        }
        result
    }

    /// §92's admission walk: whether a `TypeLiteral` sits on the body's
    /// structural spine (through unions, intersections, parentheses).
    fn body_carries_type_literal(node: TypeNode<'_>) -> bool {
        match node {
            TypeNode::TypeLiteralNode(_) => true,
            TypeNode::UnionTypeNode(union) => {
                union.types.iter().any(|&t| Self::body_carries_type_literal(t))
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                intersection.types.iter().any(|&t| Self::body_carries_type_literal(t))
            }
            TypeNode::ParenthesizedTypeNode(parenthesized) => {
                parenthesized.r#type.is_some_and(Self::body_carries_type_literal)
            }
            _ => false,
        }
    }

    /// The literal-key texts of a string-literal union (or single literal, or
    /// `never` = empty), `None` for anything else.
    pub(crate) fn literal_key_texts(&self, id: TypeId) -> Option<Vec<String>> {
        let ty = self.store.get(id);
        if ty.flags.contains(TypeFlags::NEVER) {
            return Some(Vec::new());
        }
        match &ty.data {
            crate::types::TypeData::StringLiteral(text) => Some(vec![text.clone()]),
            crate::types::TypeData::Union { types, .. } => {
                let mut keys = Vec::with_capacity(types.len());
                for &constituent in types {
                    let crate::types::TypeData::StringLiteral(text) =
                        &self.store.get(constituent).data
                    else {
                        return None;
                    };
                    keys.push(text.clone());
                }
                Some(keys)
            }
            _ => None,
        }
    }

    /// §91: the set intersection of literal-key unions, in the FIRST
    /// operand's order; `None` when any constituent is not a literal-key
    /// union, which sends the caller to the ordinary intersection.
    fn intersect_literal_key_unions(&mut self, types: &[TypeId]) -> Option<TypeId> {
        let mut sets = Vec::with_capacity(types.len());
        for &id in types {
            sets.push(self.literal_key_texts(id)?);
        }
        let (first, rest) = sets.split_first()?;
        let surviving: Vec<String> =
            first.iter().filter(|key| rest.iter().all(|set| set.contains(key))).cloned().collect();
        Some(self.literal_key_union(&surviving))
    }

    /// A union of REGULAR string-literal types over `keys` — `never` when
    /// empty, the single literal when one.
    fn literal_key_union(&mut self, keys: &[String]) -> TypeId {
        let literals: Vec<TypeId> = keys
            .iter()
            .map(|key| {
                self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(key.clone()),
                    false,
                )
            })
            .collect();
        match literals.as_slice() {
            [] => self.intrinsics.never,
            [one] => *one,
            many => self.get_union_type(many),
        }
    }

    /// §91: the property-name set of a type, in declaration order, or `None`
    /// where enumeration is not computable. Covers member-table types,
    /// intersections (the union of both sides' keys), and `Omit<T, K>` by its
    /// global symbol (keys of `T` minus `K`'s literals — the §45 Record
    /// precedent for special-casing one lib alias).
    fn keys_of(&mut self, id: TypeId) -> Option<Vec<String>> {
        if id == self.intrinsics.error {
            return None;
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned() {
            if self.global_type_symbol_with_arity("Omit", 2) == Some(target) && arguments.len() == 2
            {
                let base = self.keys_of(arguments[0])?;
                let removed = self.literal_key_texts(arguments[1])?;
                return Some(base.into_iter().filter(|key| !removed.contains(key)).collect());
            }
            // §92: a NAMED alias reference's keys are its evaluated body's —
            // the alias symbol's own member table is empty and must not be
            // read as "no keys" (chain1's deep reads).
            if self.binder.symbols().get(target).flags.contains(SymbolFlags::TYPE_ALIAS) {
                let evaluated = self.evaluate_alias_body(target, &arguments)?;
                if evaluated == id {
                    return None;
                }
                return self.keys_of(evaluated);
            }
        }
        if let crate::types::TypeData::Intersection { types, .. } = &self.store.get(id).data {
            let types = types.clone();
            let mut keys: Vec<String> = Vec::new();
            for constituent in types {
                for key in self.keys_of(constituent)? {
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
            return Some(keys);
        }
        let owner = match &self.store.get(id).data {
            crate::types::TypeData::Named { members: Some(owner), .. } => *owner,
            crate::types::TypeData::Anonymous { symbol, .. } => *symbol,
            _ => return None,
        };
        // A TYPE_ALIAS owner's member table is structurally empty — reading
        // it as "no keys" is the §92 hazard the alias arm above exists for.
        if self.binder.symbols().get(owner).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        // Declaration order, not table order: the members table is an
        // unordered map, and a printed key union must be deterministic.
        let mut named: Vec<(Option<tsr_ast::NodeId>, String)> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .map(|(name, &member)| {
                (
                    self.binder.symbols().get(member).declarations.first().copied(),
                    (*name).to_string(),
                )
            })
            .collect();
        named.sort();
        Some(named.into_iter().map(|(_, name)| name).collect())
    }

    pub(crate) fn local_type_parameters_of(
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

    /// The **declared types** of a symbol's own type parameters, with their
    /// names, in order — the substitution domain for instantiating a member of
    /// `C<number>`.
    ///
    /// The class/interface/alias sibling of `inference.rs`'s
    /// `type_parameter_types`, and built the same way: through each parameter's
    /// **declaration symbol**, because two type parameters can print `T` and be
    /// different types. Upstream reads the same list off
    /// `getLocalTypeParametersOfClassOrInterfaceOrTypeAlias` results as `*Type`s
    /// directly (`checker.go:23168`).
    ///
    /// `None` when any parameter has no symbol or no declared type, which keeps
    /// a partial list from producing a partial substitution — the same rule
    /// `type_parameter_types` states.
    pub(crate) fn local_type_parameter_types_of(
        &mut self,
        symbol: SymbolId,
    ) -> Option<Vec<(TypeId, String)>> {
        let declarations = self.local_type_parameters_of(symbol);
        let mut parameters = Vec::with_capacity(declarations.len());
        for declaration in declarations {
            let name = declaration.name?.text.to_string();
            let parameter = declaration.node_id.and_then(|id| self.binder.symbol_of(id))?;
            let declared = self.get_declared_type_of_symbol(parameter);
            if declared == self.intrinsics.error {
                return None;
            }
            parameters.push((declared, name));
        }
        Some(parameters)
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
