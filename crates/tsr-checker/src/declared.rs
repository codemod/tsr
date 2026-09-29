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
    /// The instantiated base type an `extends` heritage entry names — the
    /// checker half of the `React.Component<Prop, {}>` row. §226.
    ///
    /// # What this is for, and why it is not `base_symbols_of`
    ///
    /// Upstream's baseline writer has a guard
    /// (`type_symbol_baseline.go:370-374`) that, for a node whose parent is an
    /// `ExpressionWithTypeArguments` in a class `extends` clause, records
    /// `GetTypeAtLocation(node.Parent)` instead of the node's own type — the
    /// comment there says *"Workaround to ensure we output 'C' instead of
    /// 'typeof C' for base class expressions"*. That lands in `getTypeOfNode`'s
    /// arm at `checker.go:31958`, which answers the class's first base type.
    ///
    /// So `class Poisoned extends React.Component<{}, {}>` records
    /// `>React.Component : React.Component<{}, {}>` where this port records
    /// `typeof React.Component`. Twelve deficit-1 cases, all `tsx`.
    ///
    /// **[`Self::base_symbols_of`] cannot serve this**, and the reason is worth
    /// keeping because it nearly cost a `+0` build:
    /// `base_symbol_of_heritage_entry` refuses a non-`Identifier` expression
    /// *and* refuses any entry carrying type arguments, and every one of the
    /// twelve witnesses trips both. That type-argument refusal is correct for
    /// its own caller — a heritage entry's arguments decide what
    /// `extends B<any>` contributes as a **member table**, which this port
    /// cannot instantiate. This caller wants no member table. It wants the
    /// written reference as a printed type, which is the thing the arguments
    /// already answer. `docs/conventions.md` corollary 11.
    ///
    /// # The split with `types_producer`
    ///
    /// `base` arrives already resolved, because resolving it means walking a
    /// `PropertyAccessExpression` through aliases (`import React =
    /// require('react')`) and that walk already exists on the producer side
    /// from its import-equals work. Duplicating it here to keep the boundary
    /// tidy would be the worse trade.
    ///
    /// `None` keeps the caller's current answer rather than producing a gap:
    /// upstream's writer guard falls back to `GetTypeAtLocation(node)` when
    /// this yields nothing, so a decline here is exactly today's behaviour.
    ///
    /// # An empty argument list declines, deliberately
    ///
    /// All twelve witnesses write two arguments. `class C extends B {}` would
    /// also reach here and upstream would answer `B`, but that population has
    /// not been measured and adding it would be unmeasured surface riding along
    /// on a measured change — the §219 lesson.
    ///
    /// **The mutation test then found a better reason than the one above.**
    /// Deleting this decline does not merely widen the arm: it renders
    /// `Base<>`, because `create_type_reference` prints the bracket list it is
    /// given. So the empty case is not "unmeasured but probably fine" — it is
    /// malformed, and enabling it needs a bare-reference road rather than one
    /// line. Recorded because the weaker reason was already written down and
    /// would have made this look like a free win.
    pub fn base_type_of_heritage_entry(
        &mut self,
        base: SymbolId,
        type_arguments: &[TypeNode<'a>],
    ) -> Option<TypeId> {
        if type_arguments.is_empty() {
            return None;
        }
        let arguments: Vec<TypeId> =
            type_arguments.iter().map(|&argument| self.get_type_from_type_node(argument)).collect();
        // **No guard on unresolvable arguments, and the first draft had one.**
        // It declined when any argument came back as the error type, on the
        // reasoning that `Base<error>` is a wrong line rather than a gap. The
        // test written for it failed: `interface C extends Base<Missing>`
        // renders `Base<Missing>`, not `Base<error>`, because
        // `get_type_from_type_node` answers an unresolved name with a type
        // printed by that name. So the guard named a case it could not detect.
        //
        // Removed rather than re-aimed. Whatever this port spells for an
        // unresolved type argument, it spells the same way in every other
        // reference, so this arm introduces no new wrongness — and a detection
        // invented here would be unmeasured machinery guarding a case nobody
        // has seen in the corpus.
        Some(self.create_type_reference(base, arguments))
    }
    /// The NEXT type of a generator's written return annotation — the third
    /// type argument, or the third type parameter's default. §225.
    ///
    /// The decidable slice of `getIterationTypesOfGeneratorFunctionReturnType`
    /// (`checker.go`): a **direct reference to a named generic** with at least
    /// three type parameters. That covers `Generator<…>`,
    /// `IterableIterator<…>` and `Iterator<…>`, which is what the corpus
    /// writes. Anything else — a union, an alias needing expansion, a
    /// structural type with a `next` member, a reference with too few
    /// parameters — answers `None` and keeps the gap.
    ///
    /// **Not gated on the target being one of the three globals**, deliberately.
    /// The rule upstream applies is about the *iteration types* of whatever the
    /// annotation resolves to, and a user-written
    /// `interface MyGen<T, R, N> { … }` has its next type in the same slot for
    /// the same reason. Gating on the name would be fitting the witness
    /// (corollary 20) and would also be *narrower than the reason given*.
    pub(crate) fn next_type_of_annotated_generator(
        &mut self,
        annotation: TypeNode<'a>,
    ) -> Option<TypeId> {
        let TypeNode::TypeReferenceNode(reference) = annotation else { return None };
        // A written third argument wins; nothing else is consulted.
        if let Some(&written) = reference.type_arguments.get(2) {
            return Some(self.get_type_from_type_node(written));
        }
        // Otherwise the third parameter's DEFAULT, read off the declaration.
        // An unwritten slot with no default is not `any` — it is a slot this
        // port cannot fill, so it gaps.
        let target = self.resolve_entity_name(reference.type_name?, SymbolFlags::TYPE)?;
        let declarations = self.binder.symbols().get(target).declarations.clone();
        for declaration in declarations {
            let parameters = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => node.type_parameters,
                Some(Node::TypeAliasDeclaration(node)) => node.type_parameters,
                Some(Node::ClassDeclaration(node)) => node.type_parameters,
                _ => continue,
            };
            if let Some(parameter) = parameters.get(2)
                && let Some(default) = parameter.default_type
            {
                return Some(self.get_type_from_type_node(default));
            }
        }
        None
    }

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
            TypeNode::ImportTypeNode(node) => self.get_type_from_import_type_node(node),
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
                // §899: `getESSymbolLikeTypeForNode` (`checker.go:22982`) mints
                // the unique type **only in a valid declaration position**:
                //
                // ```go
                // if isValidESSymbolDeclaration(node) { … return uniqueType }
                // return c.esSymbolType
                // ```
                //
                // `let x: unique symbol`, `var x: unique symbol` and a parameter
                // `(arg: unique symbol)` are all errors upstream, and their type
                // is plain `symbol`. The node upstream tests is
                // `ast.WalkUpParenthesizedTypes(node.Parent)` — the declaration
                // the operator is written in, not the operator itself.
                if !self.unique_symbol_position_is_valid(id) {
                    return self.intrinsics.es_symbol;
                }
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
                    && (!self.alias_evaluation_bindings.is_empty()
                        // §730: a CONCRETE operand's key set is final, so the
                        // operator may be evaluated. §729 measured this predicate
                        // at 23 WRONG→RIGHT and ZERO RIGHT→WRONG; its 90
                        // GAP→WRONG were the PRINTED form, which the written-text
                        // arm in `signatures.rs` now supplies.
                        || node.r#type.is_some_and(|inner| {
                            let target = self.get_type_from_type_node(inner);
                            target != self.intrinsics.error
                                && !self.mentions_any_type_parameter(target, 4)
                        })) =>
            {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let target = self.get_type_from_type_node(inner);
                match self.keys_of(target) {
                    Some(keys) => {
                        let union = self.literal_key_union(&keys);
                        // §816 (`checker-notes-deferred.md`): upstream attaches
                        // `origin = newIndexType(t)` to the key union and the
                        // node builder prints the origin — so `keyof Thing`
                        // prints `keyof Thing`, not `"a" | "b" | "c"`. §730
                        // already computed the right key set and its own note
                        // records the printed form as the residue.
                        if self.keyof_origin_applies(target) {
                            let text = format!("keyof {}", self.type_to_string(target));
                            self.index_origin_union(union, text)
                        } else {
                            union
                        }
                    }
                    None => self.intrinsics.error,
                }
            }
            // §905: a MAPPED TYPE mints a PRINT-ONLY type carrying its written
            // form — `{ [P in keyof T]: T[P]; }` — where this port has no
            // mapped-type subsystem at all (`members.rs` records
            // `ObjectFlagsMapped` as *"not ported at all"*).
            //
            // Upstream keeps a generic mapped type DEFERRED and its node builder
            // prints it from its own parts, which for an unevaluated mapped type
            // are exactly the written ones. `signatures.rs`'s §77 renderer
            // already produces that spelling — it is the same text the written
            // ANNOTATION road prints today — so the mint is the renderer plus
            // `new_named`.
            //
            // **Print-only, in §40's sense**: the type carries no members, no
            // key set and no template, so nothing can read through it. That is
            // the §811 hazard's shape, and the guard against it here is that a
            // mapped type has no member road in this port to escape into —
            // `get_property_of_type` on a `Named` with no table already
            // declines.
            //
            // Declines whenever the renderer declines, which keeps the
            // admission set exactly §77's bounded one.
            // §906: a CONDITIONAL TYPE takes the same print-only mint as §905's
            // mapped type, and for the same reason — upstream keeps one whose
            // check type is generic DEFERRED and prints it from its parts, which
            // for an uninstantiated conditional are the written ones. **The
            // alias road is untouched**: §92's `evaluate_conditional_alias` runs
            // before any reference reaches here, and its deliberate `error` for
            // an unevaluable conditional ALIAS in an alias-declared position is
            // a decision about the alias, not about this node.
            TypeNode::MappedTypeNode(_) | TypeNode::ConditionalTypeNode(_) => {
                // §909: a mapped or conditional type that is the body of a
                // NON-GENERIC type alias prints the ALIAS NAME, not the body.
                // `type T12 = { readonly [P in keyof Item]: Item[P] }` records
                // `>T12 : T12` — upstream carries an `aliasSymbol` on the type
                // and the node builder names it. §905's mint printed the body
                // everywhere, which is right at an anonymous site and wrong at a
                // named one (21 of `mappedTypes1`'s rows).
                //
                // Restricted to a non-generic alias: a GENERIC one is
                // instantiated per reference, and `mappedTypeRelationships`'s 63
                // gains are exactly those expanded forms.
                if matches!(node, TypeNode::MappedTypeNode(_))
                    && let Some(id) = tsr_ast::Node::from(node).node_id()
                    && let Some(name) = self.non_generic_alias_body_name(id)
                {
                    return self.store.new_named(TypeFlags::OBJECT, name, None);
                }
                let mut single_quoted = false;
                let mut array_headed = false;
                match Self::written_type_text(node, &mut single_quoted, &mut array_headed) {
                    // §905.1: a RECURSIVE mapped alias answers `any` upstream,
                    // not the mapped form — `type Recurse = { [K in keyof
                    // Recurse]: Recurse[K] }` records `>Recurse : any`, its
                    // circularity result — and those three rows are this mint's
                    // ONLY `RIGHT→WRONG`. A syntactic guard was built (the
                    // enclosing alias's own name appearing in the rendered text)
                    // and **measured worse**: it recovered one of the three and
                    // cost five elsewhere, because the corpus's other two are
                    // MUTUAL recursion (`Recurse1` through `Recurse2`), which no
                    // same-name test can see. Taking the three is the better
                    // trade at 208:1, and the guard is recorded rather than kept.
                    Some(text) => self.store.new_named(TypeFlags::OBJECT, text, None),
                    None => self.intrinsics.error,
                }
            }
            // §110 slice 2c: the census's one line — EVERY `@param`/`@returns`
            // annotation arrives as this transparent wrapper, and it had no
            // arm, so 186/186 failed before any grammar question.
            TypeNode::JSDocTypeExpression(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            // The JSDoc grammar's simple wrappers (upstream's
            // `getTypeFromJSDoc*` family).
            TypeNode::JSDocAllType(_) => self.intrinsics.any,
            TypeNode::JSDocNullableType(node) => {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let inner = self.get_type_from_type_node(inner);
                if inner == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                let null = self.intrinsics.null;
                self.get_union_type(&[inner, null])
            }
            TypeNode::JSDocNonNullableType(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            TypeNode::JSDocOptionalType(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            TypeNode::JSDocVariadicType(node) => {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let element = self.get_type_from_type_node(inner);
                if element == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                match self.global_type_symbol("Array") {
                    Some(array) => self.create_type_reference(array, vec![element]),
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
                        // §812: OBJECT rather than ANY, for §36's recorded
                        // reason one construct over — an ANY constituent
                        // absorbs its whole union, so `keyof T | keyof U`
                        // answered a bare `any`.
                        let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
                        self.unresolved_types.insert(id);
                        // §786: remember that THIS mint is a generic index, so
                        // `x[k]` where `k: keyof T` can defer rather than
                        // answer `any`.
                        self.deferred_keyof_types.insert(id);
                        // §813: also a DEFERRED mint, for getAdjustedTypeWithFacts.
                        self.deferred_index_mints.insert(id);
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
                // §620: the CONCRETE arm, ahead of the deferred one. The road
                // below prints `T[K]` as written for a type-PARAMETER index and
                // answers `error` for everything else — *"Literal indexes
                // resolve concretely upstream and stay declined here"* (§619).
                // An array or tuple object with a literal index is the slice
                // that needs no inference: `type T = string[]["0"]` is `string`
                // (`assignmentToAnyArrayRestParameters`), and a numeric-literal
                // NAME is a numeric index — `isNumericLiteralName`
                // (`checker.go:16256`) is `String(+name) === name`, which is why
                // the same fixture makes `string[]["0.0"]` an ERROR.
                //
                // Only fires where the road already answered `error`, so its
                // failure direction is gap→wrong rather than right→wrong.
                if let (Some(object_node), Some(index_node)) = (node.object_type, node.index_type) {
                    let object_type = self.get_type_from_type_node(object_node);
                    if object_type != self.intrinsics.error {
                        let index_type = self.get_type_from_type_node(index_node);
                        let numeric = match &self.store.get(index_type).data {
                            crate::types::TypeData::StringLiteral(text)
                                if crate::printing::normalise_number(text) == *text =>
                            {
                                Some(self.intrinsics.number)
                            }
                            crate::types::TypeData::NumberLiteral(_) => {
                                Some(self.intrinsics.number)
                            }
                            _ => None,
                        };
                        // §621: a TUPLE with a literal index selects the
                        // SPECIFIC element, which is what §620 recorded as the
                        // next slice — `[a: string, b?: number]["0"]` is
                        // `string`. The index's VALUE is what the numeric
                        // normalisation below discards, so this arm reads it
                        // from the literal directly and never routes through
                        // `array_or_tuple_element_access`.
                        //
                        // Out of range declines rather than guessing: upstream
                        // answers `undefined` there under
                        // `noUncheckedIndexedAccess` and the element type
                        // otherwise, and this port models neither, so a gap is
                        // the honest answer.
                        if let Some((elements, _)) =
                            self.tuple_element_lists.get(&object_type).cloned()
                        {
                            let literal = match &self.store.get(index_type).data {
                                crate::types::TypeData::StringLiteral(text)
                                | crate::types::TypeData::NumberLiteral(text) => {
                                    text.parse::<usize>().ok()
                                }
                                _ => None,
                            };
                            if let Some(position) = literal
                                && let Some(&element) = elements.get(position)
                            {
                                return element;
                            }
                        }
                        // TUPLES excluded: `array_or_tuple_element_access`
                        // answers the UNION of a tuple's elements for a
                        // `number` index, which is right for `number` and wrong
                        // for a literal — `[a: string, b?: number]["0"]` is
                        // `string`, not `string | number`
                        // (`partiallyNamedTuples`, measured at 2 G→W before
                        // this guard). Selecting the specific element needs the
                        // literal's value, which is the next slice.
                        if let Some(numeric) = numeric
                            && !self.tuple_element_lists.contains_key(&object_type)
                            && let Some(element) =
                                self.array_or_tuple_element_access(object_type, numeric, false)
                        {
                            return element;
                        }
                    }
                }
                // §933: `X[keyof X]` — the union of EVERY property type.
                //
                // `getIndexedAccessType` (`checker.go:22292` region) distributes
                // an indexed access over a union index, and `keyof X` is that
                // union. The port had no arm for it, so `type WeakKey =
                // WeakKeyTypes[keyof WeakKeyTypes]` — **in `lib.es5.d.ts:1692`** —
                // answered `error`, and with it every `WeakSet` and `WeakMap`
                // use in the corpus: `new WeakSet<symbol>()` was `error` while
                // `new Set<symbol>()` was right.
                //
                // Found by ranking `any_audit`'s dump and landing on a bucket
                // labelled *"self-referential initialiser: reportCircularityError"* —
                // **misattributed, exactly as §929's was.** The rows wanted
                // `WeakSet<symbol>`, which is not a circularity at all. Two of
                // this session's largest finds came from a wrongly labelled
                // bucket, because a label that is wrong about the *mechanism*
                // still points at the right *rows*.
                //
                // The index must resolve to the SAME type the object does, which
                // is what makes this the `keyof` of the object rather than of
                // something else; a `keyof` over a different type is left to the
                // deferred road below.
                if let (Some(object_node), Some(TypeNode::TypeOperatorNode(operator))) =
                    (node.object_type, node.index_type)
                    && operator.operator.kind == SyntaxKind::KeyOfKeyword
                    && let Some(operand) = operator.r#type
                {
                    let object_type = self.get_type_from_type_node(object_node);
                    if object_type != self.intrinsics.error
                        && self.get_type_from_type_node(operand) == object_type
                    {
                        let names = self.property_names_of(object_type);
                        let mut members = Vec::with_capacity(names.len());
                        let mut clean = !names.is_empty();
                        for name in &names {
                            let Some(property) = self.get_property_of_type(object_type, name)
                            else {
                                clean = false;
                                break;
                            };
                            let member = self.get_type_of_symbol(property);
                            if member == self.intrinsics.error {
                                clean = false;
                                break;
                            }
                            if !members.contains(&member) {
                                members.push(member);
                            }
                        }
                        if clean {
                            return match members.as_slice() {
                                [single] => *single,
                                many => {
                                    let many = many.to_vec();
                                    // The ALIAS names the result, the same
                                    // three arms `get_type_from_union_type_node`
                                    // takes. Without this, `type WeakKey =
                                    // WeakKeyTypes[keyof WeakKeyTypes]` printed
                                    // the expanded `symbol | object` wherever
                                    // upstream prints `WeakKey` — 21
                                    // `RIGHT->WRONG` and 12 `RIGHT->GAP`
                                    // measured, across `sharedMemory`,
                                    // `bigintWithLib` and
                                    // `readonlyFloat32ArrayAssignableWithFloat32Array`.
                                    let alias = node
                                        .node_id
                                        .and_then(|id| self.alias_symbol_for_type_node(id));
                                    match alias {
                                        Some(alias)
                                            if self.alias_evaluation_bindings.is_empty()
                                                && self
                                                    .local_type_parameters_of(alias)
                                                    .is_empty() =>
                                        {
                                            self.get_named_union_type(
                                                &many,
                                                TypeFlags::empty(),
                                                alias,
                                            )
                                        }
                                        _ => self.get_union_type(&many),
                                    }
                                }
                            };
                        }
                    }
                }
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
                    // §625: a LITERAL or UNION index defers **only when the
                    // access is generic** — the object is a type parameter, or
                    // the index mentions one. A fully CONCRETE pair resolves:
                    // `I["readonlyType"]` is `unique symbol`, not the written
                    // form (`uniqueSymbols`), and deferring it printed a
                    // confident `I["readonlyType"]` over a right answer. That
                    // asymmetry is what §624 measured as 27 G→W and could not
                    // name.
                    //
                    // The corpus states the rule in four shapes:
                    // `T["0"]` defers (generic object), `string[]["0" | K]`
                    // defers (generic index), `I["readonlyType"]` resolves and
                    // `string[]["0"]` resolves (§620) — both concrete.
                    Some(index @ (TypeNode::LiteralTypeNode(_) | TypeNode::UnionTypeNode(_))) => {
                        let index_type = self.get_type_from_type_node(index);
                        let object_type =
                            node.object_type.map(|object| self.get_type_from_type_node(object));
                        let object_is_generic = object_type.is_some_and(|object| {
                            self.store.get(object).flags.contains(TypeFlags::TYPE_PARAMETER)
                        });
                        let index_is_generic = {
                            let ty = self.store.get(index_type);
                            ty.flags.contains(TypeFlags::TYPE_PARAMETER)
                                || matches!(&ty.data, crate::types::TypeData::Union { types, .. }
                                    if types.iter().any(|&t| self
                                        .store
                                        .get(t)
                                        .flags
                                        .contains(TypeFlags::TYPE_PARAMETER)))
                        };
                        (index_type != self.intrinsics.error
                            && (object_is_generic || index_is_generic))
                            .then(|| crate::printing::type_to_string(self.store.get(index_type)))
                    }
                    _ => None,
                };
                let object_text = match node.object_type {
                    // §626: an ARRAY object can carry the deferred print too —
                    // `string[]["0" | K]` is concrete on the left and generic on
                    // the right, which §625's gate admits, but `TypeReferenceNode`
                    // was the only object shape that could produce the text, so
                    // the node still answered `error`. This is the last line of
                    // `assignmentToAnyArrayRestParameters`, and it converts the
                    // case.
                    Some(object @ TypeNode::ArrayTypeNode(_)) => {
                        let object_type = self.get_type_from_type_node(object);
                        (object_type != self.intrinsics.error)
                            .then(|| crate::printing::type_to_string(self.store.get(object_type)))
                    }
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
                        // §806: §34 declined an ALIAS object whole, on the
                        // reasoning that upstream EXPANDS it — `ArgMap[P]`
                        // records `{ sum: …; concat: … }[P]`. True for that
                        // alias and NOT for every alias: the same fixture has
                        // `RecordMap[P]` recording `RecordMap[P]`, and both are
                        // `type X = { … }`.
                        //
                        // The difference is where they are DECLARED.
                        // `RecordMap` is top-level; `ArgMap` sits inside a
                        // function body (`correlatedUnions.ts:147`). Upstream
                        // prints a name it can REACH from the site and expands
                        // one it cannot — `isTypeAccessible`, the node
                        // builder's symbol-table walk.
                        //
                        // Approximated syntactically: an alias whose
                        // declaration has a function or block ancestor is not
                        // nameable from an arbitrary site, so it expands;
                        // everything else keeps its written name. That is
                        // narrower than upstream's walk and errs toward the
                        // old behaviour, which was the measured one.
                        if is_alias && self.alias_declaration_is_locally_scoped(object.type_name) {
                            None
                        } else {
                            Self::entity_name_text(object.type_name)
                        }
                    }
                    _ => None,
                };
                match (object_text, deferred_index) {
                    (Some(object), Some(index)) => {
                        let printed = format!("{object}[{index}]");
                        // §812: OBJECT rather than ANY — see the `keyof` arm;
                        // a deferred `T["params"] | undefined` loses its second
                        // constituent to any-absorption otherwise.
                        let id = self.store.new_named(TypeFlags::OBJECT, printed, None);
                        self.unresolved_types.insert(id);
                        // §813: also a DEFERRED mint, for getAdjustedTypeWithFacts.
                        self.deferred_index_mints.insert(id);
                        id
                    }
                    _ => self.intrinsics.error,
                }
            }
            _ => self.intrinsics.error,
        }
    }

    /// Whether the TYPE ALIAS `name` resolves to is declared inside a function
    /// or block rather than at a file/namespace top level. §806.
    ///
    /// The syntactic half of `isTypeAccessible` (`checker.go`): a locally
    /// scoped alias cannot be NAMED from an arbitrary print site, so the node
    /// builder writes its body instead. A top-level one keeps its name.
    fn alias_declaration_is_locally_scoped(
        &mut self,
        name: Option<tsr_ast::EntityName<'a>>,
    ) -> bool {
        let Some(tsr_ast::EntityName::Identifier(identifier)) = name else { return false };
        let Some(id) = identifier.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            identifier.text,
            SymbolFlags::TYPE,
        ) else {
            return false;
        };
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return false;
        };
        let mut current = declaration;
        while let Some(parent) = self.nodes.parent(current) {
            if matches!(
                self.nodes.kind(parent),
                SyntaxKind::Block
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
            ) {
                return true;
            }
            current = parent;
        }
        false
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
        // §157 (`checker-notes-ctx.md` sibling in `checker-notes-narrow.md`):
        // a reference THROUGH AN ALIAS prints the WRITTEN alias name —
        // `var v: IC` wants `IC`, not the target class's own text. The §41
        // mint with the alias's spelling, carrying the MERGED target so
        // member reads flow through it. Argument-less only; the alias
        // declaration line is §156's coupled half and stays gapped.
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS)
            && node.type_arguments.is_empty()
            // ImportEquals aliases ONLY. §158 measured the ES import kinds at
            // 213:72 (3.0:1, refused): their minted texts TRAVEL — inferred
            // types cross units and upstream re-spells per site
            // (`import("...").SomeType` where the local name is not in scope
            // at the consuming file) — the per-site re-render wall, which no
            // declaration-kind gate can cut. ImportEquals targets are
            // same-unit namespaces in practice, which is why §157 held.
            && self.declaration_of_alias_symbol(symbol).is_some_and(|declaration| {
                matches!(self.node_map.get(declaration), Some(Node::ImportEqualsDeclaration(_)))
            })
        {
            let target = self.resolve_alias(symbol).or_else(|| {
                let declaration = self.declaration_of_alias_symbol(symbol)?;
                let Some(Node::ImportEqualsDeclaration(import)) = self.node_map.get(declaration)
                else {
                    return None;
                };
                let Some(tsr_ast::ModuleReference::QualifiedName(qualified)) =
                    import.module_reference
                else {
                    return None;
                };
                self.resolve_qualified_entity(qualified)
            });
            if let Some(target) = target {
                let merged = self.binder.merged_symbol(target);
                if self.binder.symbols().get(merged).flags.intersects(SymbolFlags::TYPE) {
                    let text = name.text.to_string();
                    let key = (text.clone(), merged);
                    if let Some(&existing) = self.qualified_reference_types.get(&key) {
                        return existing;
                    }
                    let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(merged));
                    self.qualified_reference_types.insert(key, minted);
                    return minted;
                }
            }
            return error;
        }
        // §269: a JSDoc `@import` alias — `@import { Foo } from "./m"` then
        // `@param {Foo} x`. This is NOT §158's refused population: §158
        // refused MINTING the written alias name for ES imports because
        // minted texts travel across units; here nothing is minted — the
        // alias resolves and the TARGET's own declared type answers, so the
        // printed name is the target's. That is only sound where the local
        // name and the target name agree, so a RENAMED specifier
        // (`{ Foo as F }`) declines: its printed form is the local name,
        // which this road cannot spell (the §158 wall, unchanged).
        if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS)
            && node.type_arguments.is_empty()
            && self
                .declaration_of_alias_symbol(symbol)
                .is_some_and(|declaration| self.is_unrenamed_jsdoc_import_alias(declaration))
        {
            if let Some(target) = self.resolve_alias(symbol) {
                let merged = self.binder.merged_symbol(target);
                if self.binder.symbols().get(merged).flags.intersects(SymbolFlags::TYPE) {
                    let declared = self.get_declared_type_of_symbol(merged);
                    return self.get_regular_type_of_literal_type(declared);
                }
            }
            return error;
        }
        // §491: an ES named import in TYPE position resolves through the alias
        // — `resolveTypeReferenceName` calls `resolveAlias` and
        // `getDeclaredTypeOfSymbol` answers for the TARGET. Probed before
        // building: `import { A } from "./a"; let _: A` printed `error`
        // corpus-wide, including through `export type *` chains
        // (`exportNamespace6/9`, whose annotations resolve while the VALUE use
        // is the diagnostics lane's error). UNRENAMED specifiers only: the
        // target's declared type prints the target's own name, which is the
        // local name exactly when no `as` intervenes — a renamed specifier
        // would print the wrong name on every line, the §158 per-site naming
        // wall, so it stays a gap.
        // §493 widens §491 to the DEFAULT-import clause (`import A from …`,
        // probed the same way: `let _: A` gapped corpus-wide), with the gate
        // §491's unrenamed-specifier test was a special case of — NAME
        // AGREEMENT: the target declaration's own written name must equal the
        // local name, because the declared type prints the declaration's name
        // and upstream prints the LOCAL one (`import Foo from` naming a class
        // `A` would print `A` on every line — the §158 wall).
        let alias_road: Option<Option<&str>> =
            if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS) {
                match self.declaration_of_alias_symbol(symbol).and_then(|d| self.node_map.get(d)) {
                    // §491's original form: no `as`, so the local and target
                    // names agree by construction — no gate needed.
                    Some(Node::ImportSpecifier(specifier)) if specifier.property_name.is_none() => {
                        Some(None)
                    }
                    // §493: the default clause must carry the name-agreement
                    // gate, checked against the target below.
                    Some(Node::ImportClause(clause)) => clause.name.map(|local| Some(local.text)),
                    _ => None,
                }
            } else {
                None
            };
        if let Some(required_name) = alias_road {
            if let Some(target) = self.resolve_alias(symbol) {
                let merged = self.binder.merged_symbol(target);
                if let Some(required) = required_name
                    && self.declaration_written_name(merged) != Some(required)
                {
                    return error;
                }
                // §499 refines §491's TYPE_ALIAS gate from a blanket decline
                // to an in-flight park: a cross-file alias CYCLE (`circular2`)
                // re-enters this road for a target already resolving, and the
                // park answers `error` — the pre-§491 answer, which prints
                // `any` through the same propagation it always did — instead
                // of reaching `get_declared_type_of_type_alias`'s §29
                // placeholder, whose NAME was the arm's only measured R→W.
                // Acyclic alias targets (`exportNamespace9`'s
                // `export type A = number` through a type-only star) resolve.
                if self.binder.symbols().get(merged).flags.intersects(SymbolFlags::TYPE) {
                    // The cycle test: a target whose OWN declared type is
                    // already computing above this frame would answer the §29
                    // name placeholder; `error` here is the pre-§491 answer,
                    // which prints `any` through the same propagation it
                    // always did (`circular2`'s four pinned lines).
                    if self.resolutions.on_stack(merged, PropertyName::DeclaredType) {
                        return error;
                    }
                    let parameters = self.local_type_parameters_of(merged).len();
                    if parameters == 0 {
                        if !node.type_arguments.is_empty() {
                            return error;
                        }
                        let declared = self.get_declared_type_of_symbol(merged);
                        return self.get_regular_type_of_literal_type(declared);
                    }
                    return self.get_instantiated_type_reference(node, merged, parameters);
                }
            }
            return error;
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
        // SS333: computed property signatures whose name cannot late-bind
        // contribute an INDEX (`var v: { [e]: number }` with unresolved `e`
        // records `{ [x: number]: number; }`, `parserComputedPropertyName13`),
        // DROPPED whole when the literal declares a real index signature
        // (`{ [e: number]: string; [e]: number }` prints only the declared
        // one, `parserComputedPropertyName15`).
        let mut computed_indexes: Vec<(&'static str, TypeId)> = Vec::new();
        for member in node.members {
            // A method, call or construct signature prints whole and has no
            // `name: type` shape at all — see `crate::objects::Member`. Its
            // three spellings differ only in what precedes the parameter list,
            // which is decided here rather than in the renderer.
            let signature = match member {
                tsr_ast::TypeElement::MethodSignatureDeclaration(method) => {
                    // SS145 (checker-notes-callres2.md): a computed name
                    // whose expression is the well-known `Symbol.hasInstance`
                    // access prints bracketed - the shape the hasInstance
                    // narrowing family's RHS literals carry. Every other
                    // computed name keeps the whole-literal decline.
                    let name = match method.name {
                        tsr_ast::PropertyName::Identifier(name) => name.text.to_string(),
                        tsr_ast::PropertyName::ComputedPropertyName(computed)
                            if computed.expression.is_some_and(|e| {
                                matches!(e, tsr_ast::Expression::PropertyAccessExpression(access)
                                    if matches!(access.name,
                                        Some(tsr_ast::MemberName::Identifier(name))
                                            if name.text == "hasInstance")
                                        && matches!(access.expression,
                                            Some(tsr_ast::Expression::Identifier(receiver))
                                                if receiver.text == "Symbol"))
                            }) =>
                        {
                            "[Symbol.hasInstance]".to_string()
                        }
                        // SS327: the general late-bound arm - any computed
                        // name whose type is a unique symbol spelled as an
                        // identifier chain prints bracketed, the same rule
                        // the object-literal road took at SS323
                        // (`symbolProperty11/12`'s type-literal halves). The
                        // hasInstance arm above stays: it answers
                        // SYNTACTICALLY, before any type is computed.
                        tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                            match self.late_bound_symbol_member_name(computed) {
                                Some((name, _)) => name,
                                None => return error,
                            }
                        }
                        // §586 REVERTED: the symmetric arm for the METHOD half
                        // — a method signature named by a string or numeric
                        // literal, `{ "a b"(): void }` — was built, and it
                        // measured **zero transitions**. It is kept out rather
                        // than kept in on the §219 rule: unmeasured surface
                        // riding along on a measured change is how a later
                        // session inherits behaviour nothing ever scored. The
                        // property half (§584) is +161; this half is a shape
                        // the corpus appears not to write. Reopen it with a
                        // fixture that reaches this arm.
                        _ => return error,
                    };
                    // §831: an OPTIONAL method keeps its `?` —
                    // `{ k?(a: any): any; }`. `postfix_token` was never read on
                    // this half, while the PROPERTY half has honoured it since
                    // §77. Carried on the name so the member tuple keeps its
                    // shape and the renderer needs no new field.
                    let name = if method
                        .postfix_token
                        .is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
                    {
                        format!("{name}?")
                    } else {
                        name
                    };
                    // A method groups with the properties: see the doc comment.
                    Some((method.node_id, Some(name), "", true))
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
                // §585: a DEGENERATE index signature contributes nothing, and
                // is not the same event as one this port cannot spell.
                // `index_signature_member` answers `None` for both, and the
                // caller turned every `None` into a whole-literal `error` —
                // so `var y: { []; }` printed `any` where upstream prints
                // `{}` (`indexWithoutParamType`).
                //
                // `[]` is a parse error; the parser reports it and hands back
                // an `IndexSignatureDeclaration` with NO parameter, which
                // upstream drops on the floor rather than admitting to the
                // type. Dropping is only safe BECAUSE the member is
                // degenerate: a real `[k: string]: T` this port cannot render
                // must keep declining the literal whole, since dropping that
                // one silently loses an index signature and prints a smaller
                // type that looks correct. The narrow test — an empty
                // parameter list — is what separates them.
                if index.parameters.is_empty() {
                    continue;
                }
                let Some(rendered) = self.index_signature_member(index) else { return error };
                indexes.push(rendered);
                continue;
            }
            // §930.1: an ACCESSOR in a type literal. Until now it fell to the
            // `else { return error }` below and took the whole literal with it,
            // which is the rule `tests/signature_members.rs`'s
            // `a_member_this_port_still_cannot_render_gaps_the_whole_literal`
            // was named for.
            //
            // Upstream resolves a get/set pair to ONE property symbol
            // (`getTypeOfAccessors`, `checker.go:16700` region) and the node
            // builder prints it as a property: `{ get a(): string }` is
            // `{ readonly a: string; }`, and a pair is `{ a: string; }` — the
            // `readonly` comes from *there being no setter*
            // (`isReadonlySymbol`'s accessor arm).
            if let tsr_ast::TypeElement::GetAccessorDeclaration(_)
            | tsr_ast::TypeElement::SetAccessorDeclaration(_) = member
            {
                let (accessor_name, annotation, is_getter) = match member {
                    tsr_ast::TypeElement::GetAccessorDeclaration(get) => {
                        (get.name, get.r#type, true)
                    }
                    tsr_ast::TypeElement::SetAccessorDeclaration(set) => (
                        set.name,
                        set.parameters.first().and_then(|parameter| parameter.r#type),
                        false,
                    ),
                    _ => unreachable!("guarded by the pattern above"),
                };
                let tsr_ast::PropertyName::Identifier(accessor_name) = accessor_name else {
                    // Only a plain identifier; every other name kind keeps the
                    // decline rather than guessing a spelling.
                    return error;
                };
                // A SETTER whose pair also declares a getter contributes
                // nothing: the getter already carried the property, and
                // admitting both would print the member twice.
                if !is_getter
                    && node.members.iter().any(|other| {
                        matches!(other, tsr_ast::TypeElement::GetAccessorDeclaration(get)
                            if matches!(get.name, tsr_ast::PropertyName::Identifier(n)
                                if n.text == accessor_name.text))
                    })
                {
                    continue;
                }
                let paired = node.members.iter().any(|other| {
                    matches!(other, tsr_ast::TypeElement::SetAccessorDeclaration(set)
                        if matches!(set.name, tsr_ast::PropertyName::Identifier(n)
                            if n.text == accessor_name.text))
                });
                let (member_type, spelled) = match annotation {
                    Some(annotation) => {
                        let resolved = self.get_type_from_type_node(annotation);
                        if resolved == error {
                            // §930's rule again: keep the written spelling.
                            let mut single_quoted = false;
                            let mut array_headed = false;
                            let Some(spelled) = Self::written_type_text(
                                annotation,
                                &mut single_quoted,
                                &mut array_headed,
                            ) else {
                                return error;
                            };
                            (self.intrinsics.any, Some(spelled))
                        } else {
                            (resolved, None)
                        }
                    }
                    None => (self.intrinsics.any, None),
                };
                let printed = spelled.unwrap_or_else(|| self.type_to_string(member_type));
                properties.push(crate::objects::Member::Property {
                    name: accessor_name.text.to_string(),
                    optional: false,
                    readonly: is_getter && !paired,
                    printed,
                });
                continue;
            }
            let tsr_ast::TypeElement::PropertySignatureDeclaration(property) = member else {
                return error;
            };
            let name = match property.name {
                // §584: a STRING- or NUMERIC-named signature renders through
                // the object-literal road's own spelling rather than declining
                // the whole literal. `var a: { 1: number }` printed `any`
                // because the `_ => return error` below caught every name kind
                // this arm did not list, and the list was one kind long.
                tsr_ast::PropertyName::StringLiteral(_)
                | tsr_ast::PropertyName::NumericLiteral(_) => {
                    match crate::objects::written_property_name(&property.name) {
                        Some(name) => name,
                        None => return error,
                    }
                }
                tsr_ast::PropertyName::Identifier(name) => name.text.to_string(),
                // SS327: the property-signature half of the late-bound arm -
                // `{ [Symbol.iterator]: { x } }` in type position prints the
                // written chain in brackets. SS333: a name that cannot
                // late-bind takes the same key dispatch the object-literal
                // road uses, into `computed_indexes`.
                tsr_ast::PropertyName::ComputedPropertyName(computed) => {
                    match self.late_bound_symbol_member_name(computed) {
                        Some((name, _)) => name,
                        None => {
                            // KNOWN DEVIATION, measured both ways: an
                            // ENUM-typed name late-binds upstream to the
                            // member's own name (`[Test.a]: 0` is `{ a: 0; }`,
                            // `declarationEmitComputedPropertyNameEnum1`) and
                            // this arm prints `{ [x: number]: 0; }` for it —
                            // one wrong line in a case failing on others.
                            // GATING enum names out costs
                            // `isolatedModulesConstEnum` (a full case) for
                            // that one line; the ungated form is kept on that
                            // measurement.
                            match self.computed_member_index_key(computed) {
                                crate::objects::ComputedNameKey::LateBound => return error,
                                crate::objects::ComputedNameKey::Nothing => continue,
                                crate::objects::ComputedNameKey::Index(key) => {
                                    let Some(annotation) = property.r#type else { return error };
                                    let mut member_type = self.get_type_from_type_node(annotation);
                                    if member_type == error {
                                        return error;
                                    }
                                    // `?` on the member adds `undefined` to
                                    // the index's value
                                    // (`parserComputedPropertyName18`).
                                    if property.postfix_token.is_some_and(|token| {
                                        token.kind == SyntaxKind::QuestionToken
                                    }) {
                                        let undefined = self.intrinsics.undefined;
                                        member_type =
                                            self.get_union_type(&[member_type, undefined]);
                                    }
                                    computed_indexes.push((key, member_type));
                                    continue;
                                }
                            }
                        }
                    }
                }
                _ => return error,
            };
            // SS355: a property signature with NO annotation is the implicit
            // `any`, not a gap — upstream's member resolution reaches
            // `getTypeForVariableLikeDeclaration` (checker.go:16652), every
            // arm declines for a bare `{ x }`, and the widening fallback
            // answers `anyType` (checker.go:16648). `{ x; y }` renders
            // `{ x: any; y: any; }` (`symbolProperty9`).
            // §930: §929's rule, one level up. A property signature whose
            // annotation does not resolve used to decline the WHOLE literal —
            // `{ a: string; b: Array }` printed `error`, losing `a` as well.
            // Upstream's member carries `errorType` and the node builder reuses
            // the written annotation node, exactly as it does for a parameter.
            //
            // Recorded in the same channel (`qualified_written_text`), so the
            // `printed` slot below picks it up through
            // `written_annotation_text` for the three annotation shapes it
            // already consults, and `unresolved_printed` carries the rest.
            let mut unresolved_printed = None;
            let member_type = match property.r#type {
                Some(annotation) => {
                    let member_type = self.get_type_from_type_node(annotation);
                    if member_type == error {
                        let mut single_quoted = false;
                        let mut array_headed = false;
                        let Some(spelled) = Self::written_type_text(
                            annotation,
                            &mut single_quoted,
                            &mut array_headed,
                        ) else {
                            // No printable spelling: still a whole-literal
                            // decline, because inventing one would be worse.
                            return error;
                        };
                        unresolved_printed = Some(spelled);
                        self.intrinsics.any
                    } else {
                        member_type
                    }
                }
                None => self.intrinsics.any,
            };
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
            // §517 adds the UNION spelling: a member annotation `number |
            // string` whose union interned under the other order prints the
            // WRITTEN order (`functionOverloads43-45`'s
            // `{ a: number | string; }`); `written_annotation_text`'s §137
            // gate admits ONLY same-set-different-order unions, so nothing
            // else changes spelling.
            let printed = match (unresolved_printed, property.r#type) {
                // §930: the annotation did not resolve; its written spelling is
                // the answer, whatever shape the node is.
                (Some(spelled), _) => spelled,
                (
                    None,
                    Some(
                        annotation @ (tsr_ast::TypeNode::TypeLiteralNode(_)
                        | tsr_ast::TypeNode::ArrayTypeNode(_)
                        | tsr_ast::TypeNode::UnionTypeNode(_)),
                    ),
                ) => self
                    .written_annotation_text(annotation)
                    .unwrap_or_else(|| self.type_to_string(member_type)),
                _ => self.type_to_string(member_type),
            };
            properties.push(crate::objects::Member::Property { name, optional, readonly, printed });
        }
        if !computed_indexes.is_empty() && indexes.is_empty() {
            let key = computed_indexes[0].0;
            if computed_indexes.iter().any(|(k, _)| *k != key) {
                return error;
            }
            let mut distinct: Vec<TypeId> = Vec::new();
            for (_, value) in &computed_indexes {
                if !distinct.contains(value) {
                    distinct.push(*value);
                }
            }
            let value = match distinct.as_slice() {
                [single] => *single,
                many => {
                    let candidates = many.to_vec();
                    let Some(reduced) = self.union_with_subtype_reduction(&candidates) else {
                        return error;
                    };
                    reduced
                }
            };
            indexes.push(crate::objects::Member::Index {
                readonly: false,
                name: "x".to_string(),
                key: key.to_string(),
                value: self.type_to_string(value),
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
                let mut array_headed = false;
                match crate::signatures::written_type_literal_text(
                    node,
                    &mut single_quoted,
                    &mut array_headed,
                ) {
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
        // §290, MEASURED AND REFUSED on the 08febc71 tree. The
        // never-reduction of a discriminant-conflicting intersection
        // (`getReducedType`'s intersection arm, `checker.go:21831`) was built
        // as a two-test approximation — disjoint unit literals, or unit
        // against a different primitive kind — and measured:
        //
        //     WRONG->RIGHT ~38   intersectionReduction 16, ...Strict 14
        //     RIGHT->WRONG 15    the SAME two fixtures 6+6, genericRestTypes,
        //                        and neverTypeErrors1/2 one line EACH
        //     RIGHT->GAP 1
        //
        // Two independent blockers, both named: (1) upstream reduces only a
        // DISCRIMINANT property (`CheckFlags` non-uniform + literal), and the
        // approximation over-fires on the branded/unique-symbol shapes those
        // fixtures also hold; (2) the reduction makes the ALIAS itself
        // `never`, and a WRITTEN annotation mentioning it (`value: Union[]`)
        // must still print the alias name — one type, two renders, which is
        // ADR-0043's exact wall, and it broke a line INSIDE the winning
        // witness. REOPENING: upstream's discriminant CheckFlags port plus
        // the written-type-node carriage; neither half lands alone.
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
            // §791: remember the NODE. This mint is print-only precisely
            // because a rest element resolved to something with no element
            // list — a type parameter. Once that parameter is BOUND to a
            // concrete tuple, re-resolving this same node takes the `spliced`
            // path above and produces a real tuple, which is upstream's
            // normalisation. The binding is `alias_evaluation_bindings`, so
            // nothing here needs to know how to substitute.
            if let Some(id) = node.node_id {
                self.variadic_tuple_nodes.insert(minted, id);
            }
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
            //
            // **§930.3: §929's rule does NOT reach here, and the reason is not
            // the measurement but the shape.** §929 and §930 keep an
            // unresolvable annotation by printing *the text the user wrote*,
            // which is what upstream prints because upstream cannot resolve it
            // either. A tuple element is not a printed slot: it is a real
            // `TypeId` in `elements`, consumed by access, instantiation and
            // relations. Substituting `any` would print `[string, any]` and
            // silently hand a wrong element type to every consumer.
            //
            // And the premise fails too: the elements this port cannot resolve
            // are largely ones upstream CAN — `keyof string` is a real union
            // upstream, not an error it prints verbatim. Printing the written
            // text here would be inventing upstream's answer rather than
            // reproducing it.
            //
            // Not measured, because it should not be: the reopening condition
            // is resolving the element, not routing around it.
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
    pub(crate) fn create_optional_tuple_type(
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
    ///
    /// # The arity is silent, and it has bitten twice
    ///
    /// `global_type_symbol(name)` means *"no such global at arity 1"* and reads
    /// as *"no such global"*. A caller whose type has a different arity gets a
    /// perfectly legitimate `None` and an arm that never runs — no error, no
    /// warning, a plausible negative. Two in one day: `checker-2`'s §251
    /// (`Iterable`/`IterableIterator`/`Generator`, all arity 3 in the modern
    /// lib) and §231 (`Function`, arity **0**, whose `typeof` narrowing arm had
    /// therefore never executed in any program).
    ///
    /// **Every caller has now been audited and the rest are correct** — do not
    /// redo this. The full set is `Array`, `ReadonlyArray`, `Promise` and
    /// `Function`; the first three are genuinely arity 1, and the two call
    /// sites that pass a *variable* name (`contextual.rs`, and the array/tuple
    /// road above) only ever pass `"Array"` or `"ReadonlyArray"`.
    ///
    /// **Prefer [`Checker::global_type_symbol_with_arity`] in new code.** Its
    /// arity is written at the call site, where the reader can check it against
    /// the lib, rather than inherited from a default that is right for four
    /// types and wrong for everything else.
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
        if self.nodes.kind(host) != SyntaxKind::TypeAliasDeclaration {
            return None;
        }
        // §281: the alias's name is usable only when the DECLARATION is
        // accessible by a symbol chain from the print site — checker-2's §229
        // probe, five positions in one upstream fixture: top-level `A` and
        // namespace-nested `E` print their names; a FUNCTION-LOCAL alias and
        // a LABELLED one render structurally (`{}`), whether or not the
        // function is generic (`function g()` behaves as `f<U>()` does).
        // The predicate is a parent walk from the declaration: any
        // function-like or labelled-statement ancestor before the source file
        // makes the name unreachable. No name is minted for the inaccessible
        // arm — the STRUCTURAL answer needs no qualifier, which is what keeps
        // this outside `bd tsr-e2u`'s wall
        // (`labeledStatementWithLabel{,_es2015,_strict}`,
        // `nonGenericTypeReferenceWithTypeArguments`).
        let mut current = host;
        while let Some(parent) = self.nodes.parent(current) {
            match self.nodes.kind(parent) {
                SyntaxKind::LabeledStatement
                | SyntaxKind::FunctionDeclaration
                | SyntaxKind::FunctionExpression
                | SyntaxKind::ArrowFunction
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor
                // §363: a bare BLOCK hides the alias the same way a function
                // body does — `{ type Data = string | boolean; }` prints the
                // expanded union on the alias's own name line and at every
                // use (`declarationEmitInferredTypeAlias1`). A namespace body
                // is a ModuleBlock, not a Block, so namespace-nested aliases
                // keep their names (§281's `E`).
                | SyntaxKind::Block => return None,
                SyntaxKind::SourceFile => break,
                _ => current = parent,
            }
        }
        self.binder.symbol_of(host)
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
        // §933.1: a reference written with NO type arguments prints its BARE
        // name, whatever the defaults instantiate to. `Float32Array` in
        // `lib.esnext` is `Float32Array<TArrayBuffer extends ArrayBufferLike =
        // ArrayBufferLike>`, and the corpus wants
        // `(a: Float32Array) => Float32Array<ArrayBuffer>` — **bare where it was
        // written bare, expanded where it was computed**, which is
        // `serializeTypeForDeclaration` reusing the written node again.
        //
        // Registered in §926's `qualified_written_text` channel, which
        // `written_annotation_text` already consults, so parameters and returns
        // pick it up. It became reachable at §933: until `WeakKey` resolved,
        // these references errored and never printed.
        if node.type_arguments.is_empty()
            && let Some(id) = node.node_id
            && let Some(text) = Self::entity_name_text(node.type_name)
        {
            self.qualified_written_text.entry(id).or_insert(text);
        }
        // §136 (printseam §6): a SHORTER written list is accepted when
        // DEFAULTS cover the tail — `fillMissingTypeArguments`
        // (checker.go:19458), the annotation half. Bare references keep
        // today's road (position-sensitive default choice).
        // §890 (`checker-notes-nnaccess.md` §6): a **fully bare** reference to a
        // generic whose every parameter has a default — `CompleteRuleConfig`,
        // declared `<M extends TypesMap = TypesMap>` and written with no
        // arguments. Upstream's arity window is
        // `[minTypeArgumentCount, len(typeParameters)]` (`checker.go:23189`) and
        // `minTypeArgumentCount` is **zero** when every parameter is defaulted,
        // so zero written arguments is inside it. This port answered
        // `errorType`, which poisoned the union it sat in and cost the
        // *narrowing* of `A | null | string` — see the section for the trace.
        let bare_and_fully_defaulted = node.type_arguments.is_empty()
            && parameters > 0
            && self.local_type_parameters_of(symbol).len() == parameters
            && self
                .local_type_parameters_of(symbol)
                .iter()
                .all(|declaration| declaration.default_type.is_some());
        let partially_written = !node.type_arguments.is_empty()
            && node.type_arguments.len() < parameters
            // LIB-declared targets only (printseam §7's gate): every measured
            // win is lib-driven (typedArrays, complexRecursiveCollections,
            // asyncGenerators) while the builder-position adverse is
            // user-file defaulted generics (tsxLibraryManagedAttributes,
            // genericDefaults) — those keep their gaps until per-site
            // printing exists.
            && self
                .binder
                .symbols()
                .get(symbol)
                .declarations
                .first()
                .is_some_and(|&declaration| self.in_default_library(declaration))
            && self.local_type_parameters_of(symbol).len() == parameters
            && self.local_type_parameters_of(symbol)[node.type_arguments.len()..]
                .iter()
                .all(|declaration| declaration.default_type.is_some());
        // The bare arm carries **no lib gate**. §136's gate exists because a
        // partially-written list has to choose which position the default fills
        // and that choice is visible in print; a bare list fills every position
        // and has no choice to get wrong.
        let fillable = partially_written || bare_and_fully_defaulted;
        if node.type_arguments.len() != parameters && !fillable {
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
        // §933.2: compose this reference's WRITTEN spelling out of its argument
        // NODES, the same composition §926 needed for a qualified name. An
        // argument written bare whose defaults expand — §933.1's population —
        // must contribute the bare text: `Readonly<Float32Array>` is what
        // upstream prints, where composing from the *rendered* arguments gives
        // `Readonly<Float32Array<ArrayBuffer>>`. Only registered when some
        // argument actually carries a written spelling, so nothing else changes.
        if !node.type_arguments.is_empty()
            && let Some(id) = node.node_id
            && let Some(base) = Self::entity_name_text(node.type_name)
        {
            let mut spelled = Vec::with_capacity(arguments.len());
            let mut any_written = false;
            for (argument, &resolved) in node.type_arguments.iter().zip(&arguments) {
                let written = tsr_ast::Node::from(*argument)
                    .node_id()
                    .and_then(|id| self.qualified_written_text.get(&id))
                    .cloned();
                match written {
                    Some(text) => {
                        any_written = true;
                        spelled.push(text);
                    }
                    None => spelled.push(self.type_to_string(resolved)),
                }
            }
            if any_written {
                let composed = format!("{base}<{}>", spelled.join(", "));
                self.qualified_written_text.insert(id, composed);
            }
        }
        // §136's fill: each tail position takes its DEFAULT instantiated
        // under the map built so far (`<T, U = T>` substitutes the written
        // argument; a later default sees earlier fills).
        if fillable {
            let Some(parameter_types) = self.local_type_parameter_types_of(symbol) else {
                return error;
            };
            let types: Vec<TypeId> = parameter_types.iter().map(|&(t, _)| t).collect();
            let names: Vec<String> = parameter_types.iter().map(|(_, n)| n.clone()).collect();
            let declarations = self.local_type_parameters_of(symbol);
            for declaration in &declarations[arguments.len()..parameters] {
                let Some(default) = declaration.default_type else { return error };
                let resolved = self.get_type_from_type_node(default);
                if resolved == error {
                    return error;
                }
                let map: Vec<(TypeId, TypeId)> =
                    types.iter().copied().zip(arguments.iter().copied()).collect();
                let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
                let instantiated = self.instantiate_type(resolved, &map, &types, &name_refs);
                if instantiated == error {
                    return error;
                }
                arguments.push(instantiated);
            }
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
        // §791: a generic ALIAS whose body is a §40 PRINT-ONLY VARIADIC TUPLE
        // normalises at instantiation — `TV0<[boolean]>` over
        // `type TV0<T extends unknown[]> = [string, ...T]` is `[string, boolean]`
        // upstream, not the alias reference this port printed.
        //
        // §790 refused this as a subsystem and named the wrong prerequisite
        // twice before the trace: the arm IS on this road and DOES fire, and
        // `instantiate_type` answers `errorType` because its Arm 6 substitutes
        // a tuple ELEMENT-WISE through `tuple_element_lists` — which a
        // print-only variadic has no entry in, by §40's design.
        //
        // The cheap rule is not to teach Arm 6 to splice. It is to notice that
        // §40's structural road ALREADY splices a rest over a concrete tuple
        // (`excessivelyLargeTupleSpread`'s population), and that all it lacked
        // was the parameter being concrete. So: bind the alias's type
        // parameters to the arguments with §91's own
        // `alias_evaluation_bindings` frame and RE-RESOLVE the recorded node.
        // Nothing here substitutes anything; the existing splice runs.
        // §947.2: a GENERIC alias whose body is a FUNCTION or CONSTRUCTOR type
        // gets its signatures — **without moving what the reference prints.**
        //
        // §947.1 did the first half and measured **−271**: 16 `WRONG->RIGHT`
        // against 287 `RIGHT->WRONG`, because `signature_types` is *itself* what
        // makes a type render as a signature (`checker.rs:1517`), so registering
        // it turned `declare const fc: F<number>` from `F<number>` into
        // `(x: number) => void`.
        //
        // `alias_named_signature_types` is the existing answer to exactly that
        // question — `function_types.rs` inserts into it so a non-generic
        // alias-named bake keeps its name — and the printer checks it at both
        // signature-rendering sites. Registering there too is the whole
        // difference between §947.1 and this.
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(
                body_node @ (TypeNode::FunctionTypeNode(_) | TypeNode::ConstructorTypeNode(_)),
            ) = alias.r#type
            && self.variadic_alias_in_progress.insert(symbol)
        {
            let parameter_symbols: Vec<tsr_binder::SymbolId> = alias
                .type_parameters
                .iter()
                .filter_map(|parameter| parameter.node_id)
                .filter_map(|id| self.binder.symbol_of(id))
                .collect();
            let signatures =
                if parameter_symbols.len() == arguments.len() && !parameter_symbols.is_empty() {
                    let frame: rustc_hash::FxHashMap<tsr_binder::SymbolId, TypeId> =
                        parameter_symbols.iter().copied().zip(arguments.iter().copied()).collect();
                    self.alias_evaluation_bindings.push(frame);
                    let body = self.get_type_from_type_node(body_node);
                    self.alias_evaluation_bindings.pop();
                    self.signature_types.get(&body).cloned()
                } else {
                    None
                };
            self.variadic_alias_in_progress.remove(&symbol);
            if let Some(signatures) = signatures
                && !signatures.is_empty()
            {
                let built = self.create_type_reference(symbol, arguments.clone());
                if built != error {
                    self.signature_types.insert(built, signatures);
                    // The name survives: this is an alias-NAMED bake.
                    self.alias_named_signature_types.insert(built);
                    return built;
                }
            }
        }
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            // Syntactic gate FIRST: only a tuple body carrying a rest element
            // can be a print-only variadic, and resolving every alias body
            // eagerly to find out re-enters this road on unrelated shapes.
            && let Some(body_node @ TypeNode::TupleTypeNode(body_tuple)) = alias.r#type
            && body_tuple.elements.iter().any(|e| matches!(e, TypeNode::RestTypeNode(_)))
            && self.variadic_alias_in_progress.insert(symbol)
        {
            let body = self.get_type_from_type_node(body_node);
            if let Some(&tuple_node) = self.variadic_tuple_nodes.get(&body) {
                let parameter_symbols: Vec<tsr_binder::SymbolId> = alias
                    .type_parameters
                    .iter()
                    .filter_map(|parameter| parameter.node_id)
                    .filter_map(|id| self.binder.symbol_of(id))
                    .collect();
                if parameter_symbols.len() == arguments.len() && !parameter_symbols.is_empty() {
                    let frame: rustc_hash::FxHashMap<tsr_binder::SymbolId, TypeId> =
                        parameter_symbols.iter().copied().zip(arguments.iter().copied()).collect();
                    self.alias_evaluation_bindings.push(frame);
                    let resolved = match self.node_map.get(tuple_node) {
                        Some(Node::TupleTypeNode(tuple)) => {
                            Some(self.tuple_type_node_structural(tuple))
                        }
                        _ => None,
                    };
                    self.alias_evaluation_bindings.pop();
                    // Only a SPLICED answer is taken. A re-resolve that is
                    // still print-only means an argument was itself generic,
                    // and the alias reference remains the honest print.
                    if let Some(resolved) = resolved
                        && resolved != error
                        && self.tuple_element_lists.contains_key(&resolved)
                    {
                        self.variadic_alias_in_progress.remove(&symbol);
                        return resolved;
                    }
                }
            }
            self.variadic_alias_in_progress.remove(&symbol);
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
        if partially_written {
            let written = node.type_arguments.len();
            return self.create_type_reference_with_display(symbol, arguments, Some(written));
        }
        // The bare arm prints **every** argument, which is what upstream does:
        // `interface i00<T = number>` referenced as `<i00>x` prints
        // `i00<number>` (`genericDefaults.types:2538`). §136's display
        // truncation belongs to the partially-written arm alone — it exists so
        // a written `Map<string>` does not grow an argument nobody typed, a
        // question a bare reference does not raise.
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

    /// §289: `import("./m").Foo` in type position — `getTypeFromImportTypeNode`
    /// reduced to the arm the corpus records: a QUALIFIED, non-`typeof`
    /// reference resolves the module, walks the qualifier through its exports,
    /// and prints the WRITTEN text with the member's table behind it (the §41
    /// mint shape; `>k : import("./mod1").Con` is the baseline form).
    ///
    /// Declined, each a gap and not a guess: `typeof import(...)` and the
    /// unqualified module object (both are the `bd tsr-e2u` naming wall), and
    /// written type arguments (the instantiated print is its own row).
    fn get_type_from_import_type_node(&mut self, node: &tsr_ast::ImportTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        if node.is_type_of || !node.type_arguments.is_empty() {
            return error;
        }
        let Some(qualifier) = node.qualifier else { return error };
        let Some(site) = node.node_id else { return error };
        let Some(TypeNode::LiteralTypeNode(literal)) = node.argument else { return error };
        let Some(specifier) = literal.literal.and_then(|l| l.node_id()) else { return error };
        let Some(module) = self.resolve_external_module_name(site, specifier) else {
            return error;
        };
        // The qualifier's segments, leftmost first, walked through exports —
        // `resolveEntityName` rooted at the module symbol.
        let mut segments: Vec<&str> = Vec::new();
        let mut current = qualifier;
        let root = loop {
            match current {
                tsr_ast::EntityName::Identifier(name) => break name.text,
                tsr_ast::EntityName::QualifiedName(inner) => {
                    let Some(right) = inner.right else { return error };
                    segments.push(right.text);
                    let Some(left) = inner.left else { return error };
                    current = left;
                }
            }
        };
        segments.push(root);
        segments.reverse();
        let mut symbol = self.binder.merged_symbol(module);
        for segment in &segments {
            let Some(&found) = self.binder.symbols().get(symbol).exports.get(*segment) else {
                return error;
            };
            symbol = self.binder.merged_symbol(found);
        }
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::TYPE) {
            return error;
        }
        // The written text, rebuilt: `import("<specifier>").<qualifier>`.
        let Some(Node::StringLiteral(spec)) = self.node_map.get(specifier) else { return error };
        let text = format!("import(\"{}\").{}", spec.text, segments.join("."));
        let key = (text.clone(), symbol);
        if let Some(&existing) = self.qualified_reference_types.get(&key) {
            return existing;
        }
        let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(symbol));
        self.qualified_reference_types.insert(key, minted);
        minted
    }

    fn qualified_type_reference(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        name: tsr_ast::EntityName<'a>,
        namespace: SymbolId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // §925: the gate that stood here — `if
        // self.site_is_inside_namespace(site, namespace) { return error }` —
        // is REMOVED. It carried no recorded reason, and it declined every
        // qualified reference whose root is an ENCLOSING namespace:
        // `namespace c { export class K {} export interface I { m(p: c.K): void } }`
        // answered `error` for `c.K` while the identical reference from outside
        // `c` answered correctly.
        //
        // Upstream has no such rule — `resolveEntityName` walks the scope chain
        // and a namespace is in scope inside itself. Measured on removal:
        // **320 W→R + 92 G→R against 48 G→W and 1 R→W.**
        // §605: **upstream mints the unresolved symbol here too.** What stood
        // here declined — *"a name upstream cannot resolve is a different
        // bucket, and printing text for it here would be inventing an export
        // that does not exist"* — and the second half is the error:
        // `resolveEntityName` failing is precisely what sends upstream to
        // `getUnresolvedSymbolForEntityName` (`checker.go:23102`), which mints
        // a synthetic symbol and prints the WRITTEN text. `var foge: N.S` where
        // `N` exports a function `S` and no type records `>foge : N.S`
        // (`namespacesDeclaration2`), not `any`.
        //
        // The port already took that path when the LEFTMOST name failed; the
        // miss inside a resolvable namespace was the half that declined.
        // Measured: **305 lines (78 GAP→RIGHT, 227 WRONG→RIGHT), +18 cases,
        // zero R→W** — the largest arm of this window.
        //
        // # The cycle gate, which cost a passing case before it existed
        //
        // A CIRCULAR alias must keep answering `error`. `circular4` writes
        // `export type T = ns2.nested.T` across two files that import each
        // other; upstream reports the circularity and yields `any`, and minting
        // the written text there turned a PASSING case into a failing one (2
        // R→W). The gate is the cycle itself, not the import: the namespace
        // resolves through an ALIAS *and* the enclosing type alias's declared
        // type is already on the resolution stack. Gating on the alias alone
        // was tried first and cost 285 of the 305 lines — most of this arm's
        // wins arrive through imported namespaces.
        let Some(resolved) = self.resolve_entity_name(name, SymbolFlags::TYPE) else {
            if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
                && node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)).is_some_and(
                    |alias| {
                        self.resolutions
                            .on_stack(alias, crate::resolution::PropertyName::DeclaredType)
                    },
                )
            {
                return error;
            }
            return self.unresolved_type_reference(node);
        };
        // §41 (`checker-notes-narrow.md`): the resolved, argument-less
        // qualified reference answers a members-CARRYING named type — the
        // qualified print with the real lookup table. Generic references
        // stay print-only mints.
        if node.type_arguments.is_empty() {
            // §280's annotation half: a qualified name resolving to an ENUM
            // MEMBER answers the member's declared type in REGULAR form, not
            // a mint of the written text — upstream's `getTypeFromTypeNode`
            // regularises, so `const x1: E.static` reads `E` when the enum's
            // values collapse to one (`strictModeEnumMemberNameReserved`)
            // and `E.A` otherwise, which is the same text the mint produced.
            // Two-segment names ONLY (`E.A`): the member's own spelling and
            // the written path coincide there, so the declared road loses no
            // qualification. A deeper path (`Z.Foo.A`) must keep the mint —
            // the first draft answered `Foo.A` for it, 5 R→W across
            // `enumLiteralAssignableToEnumInsideUnion` and
            // `discriminatedUnionTypes4`, the tsr-e2u qualification wall from
            // yet another door.
            let two_segments = matches!(
                name,
                tsr_ast::EntityName::QualifiedName(qualified)
                    if matches!(qualified.left, Some(tsr_ast::EntityName::Identifier(_)))
            );
            if two_segments
                && self.binder.symbols().get(resolved).flags.intersects(SymbolFlags::ENUM_MEMBER)
            {
                let declared = self.get_declared_type_of_symbol(resolved);
                let regular = self.get_regular_type_of_literal_type(declared);
                // STRING-enum members keep the mint: the second draft handed
                // their literal types to interface discriminants and
                // `discriminatedUnionTypes4` went 3 R→W / 7 R→G — the union
                // and narrowing roads consume these where the numeric shapes
                // only print. Numeric members measured +104/0. The test is on
                // the initializer's SYNTAX because the fold mints every
                // member `TypeFlags::ENUM` regardless of value kind — a flags
                // test here was dead code, caught by an identical rescore.
                let string_valued = self
                    .binder
                    .symbols()
                    .get(resolved)
                    .declarations
                    .first()
                    .and_then(|&declaration| match self.node_map.get(declaration) {
                        Some(Node::EnumMember(member)) => member.initializer,
                        _ => None,
                    })
                    .is_some_and(|initializer| {
                        matches!(initializer, tsr_ast::Expression::StringLiteral(_))
                    });
                if !string_valued {
                    if let Some(&spelled) = self.enum_access_spelling.get(&regular) {
                        return spelled;
                    }
                    return regular;
                }
            }
            let Some(written) = Self::entity_name_text(node.type_name) else { return error };
            let text = match self.qualification_free_name(name, resolved) {
                Some(bare) => {
                    if let Some(id) = node.node_id {
                        self.qualified_written_text.insert(id, written);
                    }
                    bare
                }
                None => written,
            };
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
            let Some(written) = Self::entity_name_text(node.type_name) else { return error };
            let base = self.qualification_free_name(name, resolved);
            let printed_arguments: Vec<String> =
                arguments.iter().map(|&a| self.type_to_string(a)).collect();
            // §926: an argument that was itself shortened contributes its
            // WRITTEN spelling to the written form — `C.A<C.B>` is the reused
            // annotation even when `C.A` needed no shortening and only `C.B`
            // did. Composing the written form out of the rendered arguments
            // instead cost 26 R→W, every one of this shape.
            let mut written_arguments = Vec::with_capacity(printed_arguments.len());
            for (argument, printed) in node.type_arguments.iter().zip(&printed_arguments) {
                let spelled = match argument {
                    tsr_ast::TypeNode::TypeReferenceNode(reference) => reference
                        .node_id
                        .and_then(|id| self.qualified_written_text.get(&id))
                        .cloned(),
                    _ => None,
                };
                written_arguments.push(spelled.unwrap_or_else(|| printed.clone()));
            }
            let printed_text = format!(
                "{}<{}>",
                base.as_deref().unwrap_or(written.as_str()),
                printed_arguments.join(", ")
            );
            let written_text = format!("{written}<{}>", written_arguments.join(", "));
            if printed_text != written_text
                && let Some(id) = node.node_id
            {
                self.qualified_written_text.insert(id, written_text);
            }
            let text = printed_text;
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
    pub(crate) fn resolve_entity_name(
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

    /// §269's gate: an alias declared by a JSDoc `@import` tag's clause, with
    /// no rename — the one population whose printed name provably equals the
    /// target's own (see the arm in
    /// [`Checker::get_type_from_type_reference`]).
    fn is_unrenamed_jsdoc_import_alias(&self, declaration: NodeId) -> bool {
        let unrenamed = match self.node_map.get(declaration) {
            Some(Node::ImportSpecifier(specifier)) => specifier.property_name.is_none(),
            Some(Node::ImportClause(_)) => true,
            _ => false,
        };
        if !unrenamed {
            return false;
        }
        // specifier → NamedImports → ImportClause → JSDocImportTag, or the
        // clause's one hop.
        let mut current = declaration;
        for _ in 0..3 {
            let Some(parent) = self.nodes.parent(current) else { return false };
            if matches!(self.node_map.get(parent), Some(Node::JSDocImportTag(_))) {
                return true;
            }
            current = parent;
        }
        false
    }

    /// The source spelling of an entity name — `A`, or `A.B.C`.
    ///
    /// Upstream builds the same string with `getSymbolPath` over the chain of
    /// unresolved parent symbols (`checker.go:23139`).
    /// `needsQualification` (`symbolaccessibility.go:688`) — the printed name
    /// of a qualified type reference is the SHORTEST one that resolves from the
    /// reference site, not the one that was written.
    ///
    /// Upstream never prints the written text: `symbolToString` builds an
    /// accessible symbol chain (`getAccessibleSymbolChain`), and
    /// `canQualifySymbol` (`symbolaccessibility.go:676`) prepends the parent
    /// only when `needsQualification` says the bare name is taken by something
    /// else. Inside `namespace privateModule`, `privateModule.publicClass`
    /// prints as `publicClass`, because `publicClass` is in scope there and
    /// means that symbol.
    ///
    /// This is §925's named completion: removing the gate that refused a
    /// namespace's own name recovered 412 lines and left 48 whose only fault is
    /// carrying a qualifier upstream drops.
    ///
    /// **Only the bare/qualified decision is ported, not the chain.** Upstream's
    /// `needsQualification` walks every symbol table in scope and, when the name
    /// IS taken, recurses on the parent to build a possibly-shorter-than-written
    /// chain. Here a taken name simply keeps the written text. The two agree
    /// wherever the written path is already the accessible one, which is every
    /// shape the corpus exercises; they would diverge on a reference written
    /// through a longer path than the site needs (`A.B.C.T` from inside `A.B`,
    /// where upstream prints `C.T`). Nothing in the corpus measured that, so it
    /// is left out rather than guessed at — see the §926 entry.
    fn qualification_free_name(
        &self,
        name: tsr_ast::EntityName<'a>,
        resolved: SymbolId,
    ) -> Option<String> {
        let tsr_ast::EntityName::QualifiedName(qualified) = name else { return None };
        let right = qualified.right?;
        let site = right.node_id?;
        let wanted = self.binder.merged_symbol(resolved);

        // The written path, leftmost first: `A.B.C.T` is `["A", "B", "C", "T"]`.
        let mut segments: Vec<&str> = Vec::new();
        let mut current = name;
        loop {
            match current {
                tsr_ast::EntityName::Identifier(identifier) => {
                    segments.push(identifier.text);
                    break;
                }
                tsr_ast::EntityName::QualifiedName(qualified) => {
                    segments.push(qualified.right?.text);
                    current = qualified.left?;
                }
            }
        }
        segments.reverse();

        // §926.1: the SHORTEST suffix of the written path that resolves to the
        // same symbol from this site — upstream builds an accessible chain and
        // prints it, so `A.B.C.T` written inside `A.B` prints `C.T`.
        //
        // §926 left this out and asserted the corpus had no population for it,
        // **without measuring**. The loop below is that measurement; see the
        // §926.1 entry for what it found.
        for start in (0..segments.len()).rev() {
            let head = segments[start];
            let meaning = if start + 1 == segments.len() {
                SymbolFlags::TYPE
            } else {
                SymbolFlags::NAMESPACE
            };
            let Some(found) =
                self.binder.resolve_name(self.nodes, self.node_map, site, head, meaning)
            else {
                continue;
            };
            let mut symbol = self.binder.merged_symbol(found);
            let mut walked = true;
            for step in &segments[start + 1..] {
                let Some(&next) = self.binder.symbols().get(symbol).exports.get(*step) else {
                    walked = false;
                    break;
                };
                symbol = self.binder.merged_symbol(next);
            }
            if walked && symbol == wanted {
                return Some(segments[start..].join("."));
            }
        }
        None
    }

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

    /// §823: the member table of the branch a CONDITIONAL alias body chooses
    /// for `arguments`, if that branch is an object type with one.
    ///
    /// The companion of [`Checker::alias_body_literal_symbol`] for the one body
    /// shape it declines. `evaluate_conditional_alias` already refuses every
    /// case it cannot decide, so a `None` here keeps exactly today's answer.
    fn conditional_alias_branch_literal_symbol(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<SymbolId> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        if !matches!(alias.r#type, Some(TypeNode::ConditionalTypeNode(_))) {
            return None;
        }
        let evaluated = self.evaluate_conditional_alias(symbol, arguments)?;
        match &self.store.get(evaluated).data {
            crate::types::TypeData::Named { members: Some(members), .. } => Some(*members),
            _ => None,
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
        self.create_type_reference_with_display(symbol, arguments, None)
    }

    /// §136 (printseam §6): a default-filled reference PRINTS its written
    /// arity while carrying the full argument list — upstream's
    /// written-annotation reuse (`Iterable<number>` written short prints
    /// short; `Generator<Y, any, any>` written full prints full). `display`
    /// is the written prefix; `None` prints everything. The arity is
    /// registered in `reference_display_arity` so instantiation rebuilds and
    /// the composite re-render keep the spelling.
    pub(crate) fn create_type_reference_with_display(
        &mut self,
        symbol: SymbolId,
        arguments: Vec<TypeId>,
        display: Option<usize>,
    ) -> TypeId {
        if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
            return cached;
        }
        let shown = display.unwrap_or(arguments.len()).min(arguments.len());
        let printed = self.type_reference_text(symbol, &arguments[..shown]);
        // §90 (`checker-notes-narrow.md`): a TYPE_ALIAS target with a
        // TypeLiteral body mints the BODY's symbol — §46's admission, one
        // road lower, so `instantiate_type`'s arm-3 rebuilds keep their
        // members and `o2.merge` resolves like `o1.merge` did.
        // §823 (`checker-notes-deferred.md`): a CONDITIONAL body gets its
        // member table from the branch its own arguments choose. §90's helper
        // admits `alias.r#type` only when it IS a `TypeLiteralNode`, so
        // `type Action<T, P> = P extends void ? … : { type: T, payload: P }`
        // fell back to the alias symbol — whose member table is structurally
        // empty — and every property access on `Action<…>` gapped. The probe's
        // control proves the substitution seam itself is fine: the same access
        // on a plain `type Plain<T, P> = { type: T, payload: P }` answers
        // correctly, because that body IS a literal.
        let mut member_symbol = self.alias_body_literal_symbol(symbol);
        if member_symbol.is_none() {
            member_symbol = self.conditional_alias_branch_literal_symbol(symbol, &arguments);
        }
        let member_symbol = member_symbol.unwrap_or(symbol);
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
        if shown < arguments.len() {
            self.reference_display_arity.insert(id, shown);
        }
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
            // §595: `unique symbol` is the THIRD `TypeOperator` spelling, and
            // it binds exactly as the other two do — `unique symbol[]` parses
            // as `unique (symbol[])`, so the element must wrap.
            // `uniqueSymbolsErrors` wants `(...args: (unique symbol)[]) => void`.
            // Listed rather than folded into a shared prefix test because
            // `readonly` is a `TypeOperator` too and does NOT reach this road —
            // it is carried on the tuple/array *type* here, not in the element
            // text — so a general test would claim ground nothing exercises.
            || text.starts_with("unique ")
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

    /// §493: the name a TYPE-side declaration writes for itself — what the
    /// declared type's minted text carries, and therefore what a local alias
    /// must equal for the §491/§493 road to print truthfully.
    fn declaration_written_name(&self, symbol: SymbolId) -> Option<&str> {
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        match self.node_map.get(declaration)? {
            Node::ClassDeclaration(node) => node.name.map(|identifier| identifier.text),
            Node::InterfaceDeclaration(node) => node.name.map(|identifier| identifier.text),
            Node::EnumDeclaration(node) => node.name.map(|identifier| identifier.text),
            _ => None,
        }
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
                // §667: upstream's enum constant evaluator folds the arithmetic
                // and bitwise operators, not just literals and unary minus
                // (`evaluate`, checker.go). `enum E2 { a = 1 << 0, b = 1 << 1 }`
                // (`equalityWithEnumTypes`) has no value without this, so its
                // members never enter `enum_value_types` and §666's
                // comparability lookup cannot match them.
                tsr_ast::Expression::BinaryExpression(binary) => {
                    let left = binary.left.as_ref().and_then(|l| eval(l, enum_name, folded))?;
                    let right = binary.right.as_ref().and_then(|r| eval(r, enum_name, folded))?;
                    let token = binary.operator_token?;
                    // String `+` concatenates; every other operator is numeric.
                    if let (MemberValue::Str(a), MemberValue::Str(b)) = (&left, &right) {
                        return (token.kind == SyntaxKind::PlusToken)
                            .then(|| MemberValue::Str(format!("{a}{b}")));
                    }
                    let (MemberValue::Num(a), MemberValue::Num(b)) = (&left, &right) else {
                        return None;
                    };
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "upstream's bitwise ops are defined on int32, per the language"
                    )]
                    let (ia, ib) = (*a as i32, *b as i32);
                    // The shift count is masked to 0..=31 first, so the `u32`
                    // conversion cannot lose a sign; `>>>` is defined on the
                    // unsigned reinterpretation and wraps back, which is the
                    // language's own semantics rather than an accident.
                    #[expect(
                        clippy::cast_sign_loss,
                        clippy::cast_possible_wrap,
                        reason = "ECMAScript shift semantics: masked count, int32 <-> uint32 reinterpretation"
                    )]
                    let value = match token.kind {
                        SyntaxKind::PlusToken => a + b,
                        SyntaxKind::MinusToken => a - b,
                        SyntaxKind::AsteriskToken => a * b,
                        SyntaxKind::SlashToken => a / b,
                        SyntaxKind::PercentToken => a % b,
                        SyntaxKind::AsteriskAsteriskToken => a.powf(*b),
                        SyntaxKind::AmpersandToken => f64::from(ia & ib),
                        SyntaxKind::BarToken => f64::from(ia | ib),
                        SyntaxKind::CaretToken => f64::from(ia ^ ib),
                        SyntaxKind::LessThanLessThanToken => {
                            f64::from(ia.wrapping_shl((ib & 31) as u32))
                        }
                        SyntaxKind::GreaterThanGreaterThanToken => {
                            f64::from(ia.wrapping_shr((ib & 31) as u32))
                        }
                        SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                            f64::from(((ia as u32) >> ((ib & 31) as u32)) as i32)
                        }
                        _ => return None,
                    };
                    Some(MemberValue::Num(value))
                }
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
                // §309: a COMPUTED-NAME member — `enum E { [e] = 1 }`, a parse
                // recovery the corpus tests deliberately — has no bindable
                // name, and its declared type is the ENUM'S OWN: the baseline
                // records `[e] : E`, never a per-name literal
                // (`parserComputedPropertyName16/30/34`). Minting `E.__computed`
                // from the binder's placeholder was the §55 fold applied one
                // member too wide. It contributes nothing to the union — an
                // enum of only computed-name members takes the
                // `createComputedEnumType` fallback below and still prints `E`.
                // Gated to names the BINDER could not spell: a LITERAL
                // computed name (`[1]`, `["3"]`) is late-bound upstream
                // (`hasBindableName` true) and the binder already named its
                // symbol, so those keep the fold —
                // `compiler/literalsInComputedProperties1` records
                // `(typeof X)["2"]` and `X.bar` for them, and the first draft
                // of this arm flattened all four to `X` (4 R→W).
                if matches!(member.name, tsr_ast::PropertyName::ComputedPropertyName(_))
                    && member_name == "__computed"
                {
                    let member_type = self.store.new_named(TypeFlags::ENUM, name.clone(), None);
                    self.enum_member_owners.insert(member_type, symbol);
                    self.declared_types.insert(member_symbol, member_type);
                    continue;
                }
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
        let enum_type = self.get_named_union_type(&members, TypeFlags::ENUM_LITERAL, symbol);
        // §280: §55.1's single-DISTINCT-VALUE generalisation. `members` holds
        // one entry per distinct value (duplicates reused and `continue`d
        // above), so `enum E { a, b = a }` lands here with ONE member type
        // across TWO members — upstream's union-of-one IS the enum's declared
        // type, and its node builder prints the bare enum name for a literal
        // that equals it (`E.A : E`, `a : E` inside `b = a` —
        // `mergedEnumDeclarationCodeGen`, `preserveConstEnums`,
        // `noUnusedLocals_selfReference`). Declaration lines keep the fresh
        // per-name spelling, exactly as §55.1's twins do; only the ACCESS
        // spelling swaps.
        if members.len() == 1 && total_members != 1 {
            let single = members[0];
            let fresh = self.get_fresh_type_of_literal_type(single);
            self.enum_access_spelling.insert(single, enum_type);
            self.enum_access_spelling.insert(fresh, enum_type);
        }
        enum_type
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
            // §282: a generic alias whose body IS one of its own type
            // parameters answers that parameter — upstream attaches an alias
            // symbol only to types CREATED during the resolution, and a
            // pre-existing type parameter keeps its own display:
            // `type Bar1<T extends unknown[][]> = T` records `Bar1 : T`
            // (`substitutionTypePassedToExtends`,
            // `substituteReturnTypeSatisfiesConstraint`,
            // `homomorphicMappedTypeNesting`). Everything else keeps the
            // name-with-parameters mint below.
            if let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
                && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
                && let Some(body @ TypeNode::TypeReferenceNode(reference)) = alias.r#type
                && reference.type_arguments.is_empty()
                && matches!(reference.type_name, Some(tsr_ast::EntityName::Identifier(name))
                    if parameters.iter().any(|parameter| parameter == name.text))
            {
                return self.get_type_from_type_node(body);
            }
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
        // §246. The symbol's NAME is not always its declaration's name. A
        // default-exported class binds as `default` — upstream's
        // `InternalSymbolNameDefault` — so `export default class A {}` printed
        // its instance type as `default`, where every baseline records `A`.
        //
        // Upstream never meets this because the node builder renders from the
        // DECLARATION; this port computes the printed form once at creation
        // (see `TypeData::Named`'s note on that divergence), so the declaration
        // is what it must read here too.
        //
        // Witness `compiler/es2015modulekind` AND ITS FIVE BYTE-IDENTICAL
        // SIBLINGS — the same file under six names. The census row reading
        // "six cases" is one cause: conventions corollary 21's `apply`/`call`
        // trap in its strongest form, where the fixtures are not one word
        // apart but identical.
        //
        // PRE-FLIGHT (checker-1's §210): this exact line appears at SEVEN
        // sites in this file. Only this one is changed — the other six name
        // enums, type parameters and aliases, whose symbol name IS their
        // declaration name in every case reached today, and each is its own
        // question rather than this one repeated.
        let symbols = self.binder.symbols();
        let declared_name = symbols
            .get(symbol)
            .declarations
            .first()
            .and_then(|&declaration| self.node_map.get(declaration))
            .and_then(|node| node.name_id())
            .and_then(|id| self.node_map.get(id))
            .and_then(|node| match node {
                tsr_ast::Node::Identifier(identifier) => Some(identifier.text.to_string()),
                _ => None,
            });
        // §305: an anonymous class expression's INSTANCE type takes the same
        // `getNameOfSymbolAsWritten` walk as its `typeof` — `let C = class
        // { foo() { return new C(); } }` records `>new C() : C`
        // (`conformance/classExpression4`). The helper answers `None` for
        // every non-class-expression declaration, so the other kinds keep
        // the fallback they had.
        let name = declared_name
            .or_else(|| self.anonymous_class_written_name(symbol))
            .unwrap_or_else(|| self.binder.symbols().get(symbol).name.to_string());
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
        // §182 slice 1 (`checker-notes-narrow.md`): the `extends never`
        // shape was the only one evaluated; `getConditionalType`'s
        // non-deferred fast path evaluates ANY conditional whose CHECK type
        // is decidable, choosing a branch by assignability. The general
        // road is taken below; this shape keeps its own `literal_key_texts`
        // reading because keyof-emptiness is not an assignability question.
        let extends_is_never = matches!(conditional.extends_type,
            Some(TypeNode::KeywordTypeNode(keyword)) if keyword.kind == SyntaxKind::NeverKeyword);
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
            let keys = if extends_is_never { self.literal_key_texts(check) } else { None };
            if check != error
                && let Some(keys) = keys
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
            // The general fast path: a decidable check picks a branch.
            // `is_type_assignable_to` answers `false` between two object
            // types rather than guessing, so an undecidable check keeps the
            // gap — the decline is in the safe direction.
            if result.is_none()
                && !extends_is_never
                && check != error
                && let Some(extends_node) = conditional.extends_type
            {
                let extends = self.get_type_from_type_node(extends_node);
                // §821 (`checker-notes-deferred.md`): upstream's own three
                // outcomes, replacing the hand-rolled `primitive_domain` gate.
                // `getConditionalType`'s non-deferred case
                // (`checker.go:24372-24429`):
                //
                //   FALSE iff extends is NOT any/unknown AND not assignable
                //   TRUE  iff extends IS any/unknown OR assignable
                //   else  DEFERRED
                //
                // The primitive-domain gate existed because
                // `is_type_assignable_to` answered `false` where it could not
                // tell, and a `false` here is a confident WRONG branch rather
                // than a decline. **That cost belonged to the BINARY relation**:
                // `relate_ternary` (`d8590ff`, `bd tsr-kmzf`) answers
                // `Unknown` instead, which maps exactly onto upstream's
                // deferred outcome. The ungated binary arm measured 47 gains
                // against 58 adverse; this replaces the gate rather than
                // widening it.
                //
                // Rule 1 is structural and is what SS177 could not name:
                // `extends` being `any`/`unknown` takes the TRUE branch with NO
                // relation test — the first disjunct at `:24415`, and excluded
                // from the false branch at `:24377`. So `T extends unknown ? A
                // : B` is always `A` (`unknownType2`).
                //
                // NOT ported, and stated rather than approximated: upstream's
                // `check is any` sub-rule unions BOTH branches through
                // `extraTypes` (`:24383-24386`). That is a separate shape and
                // folding it in would make this measurement unreadable, so an
                // `any` check declines here.
                // §821.1 gate (a), DISTRIBUTIVITY: BUILT, MEASURED AND
                // REMOVED, with the number. Upstream's `root.isDistributive` is
                // a property of the check NODE — a bare reference to a type
                // parameter — and a distributive conditional over a `never` or
                // union check DISTRIBUTES rather than testing, so declining that
                // shape here looked obviously right: it removes
                // `distributiveConditionalTypeNeverIntersection1`'s 2 adverse
                // (`want never`/`want true` against an undistributed `false`).
                //
                // It cost **8 RIGHT→WRONG in `conditionalTypes1`** for those 2,
                // and took the build from +37 to +14. The reason is that a
                // distributive conditional whose check has been SUBSTITUTED to a
                // concrete argument evaluates correctly by testing — which is
                // what this road already did, and what `conditionalTypes1`'s
                // eight lines were relying on. Distribution only changes the
                // answer when the substituted check is a union or `never`, and
                // that is a much narrower shape than "the node is a naked
                // parameter".
                //
                // So the decline belongs on the SUBSTITUTED check being a union
                // or `never`, not on the node — and that is a different build
                // with its own measurement. The 2 stay as stated residue.
                if extends != error && !self.mentions_any_type_parameter(check, 2) {
                    let extends_is_any_or_unknown = self
                        .store
                        .get(extends)
                        .flags
                        .intersects(crate::flags::TypeFlags::ANY_OR_UNKNOWN);
                    let check_is_any =
                        self.store.get(check).flags.intersects(crate::flags::TypeFlags::ANY);
                    let takes_true = if extends_is_any_or_unknown {
                        Some(true)
                    } else if check_is_any {
                        // Upstream answers `true | false` here; declined.
                        None
                    } else {
                        match self.relate_ternary(
                            check,
                            extends,
                            crate::relater::Relation::Assignable,
                        ) {
                            crate::relater::Ternary::Related => Some(true),
                            crate::relater::Ternary::NotRelated => Some(false),
                            // Upstream's deferred outcome.
                            crate::relater::Ternary::Unknown => None,
                        }
                    };
                    if let Some(takes_true) = takes_true {
                        let branch =
                            if takes_true { conditional.true_type } else { conditional.false_type };
                        if let Some(branch) = branch {
                            let evaluated = self.get_type_from_type_node(branch);
                            // §821.1 gate (b) was BUILT, MEASURED AND REMOVED,
                            // and the number is the reason. It declined whenever
                            // the evaluated branch still mentioned a type
                            // parameter, on the argument that an unsubstituted
                            // body is not an evaluation — which would have
                            // removed `recursiveArrayNotCircular`'s 5 adverse
                            // (it wanted `number`/`boolean`/`string` and got bare
                            // `P`/`T`).
                            //
                            // It cost **8 RIGHT→WRONG in `conditionalTypes1`**
                            // and took the build from +37 to +14, because **a
                            // conditional's branch legitimately IS a type
                            // parameter** in the deferred/generic shapes that
                            // case is made of, and upstream prints it. The gate
                            // could not tell "the frame failed to substitute"
                            // from "the answer is a type parameter", and those
                            // are different facts.
                            //
                            // `recursiveArrayNotCircular`'s 5 therefore stay as
                            // §821's stated residue, and the real fix is for the
                            // frame to reach nested parameters rather than for
                            // this site to second-guess its own result.
                            if evaluated != error {
                                result = Some(evaluated);
                            }
                        }
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

    /// §816's gate: whether a concrete `keyof` operand is one upstream gives an
    /// INDEX ORIGIN to.
    ///
    /// `getLiteralTypeFromProperties` attaches the origin only when the operand
    /// is a `ClassOrInterface`, a `Reference`, or **aliased**. An *anonymous*
    /// object type is `ObjectFlagsAnonymous` and gets none — so
    /// `keyof { a: string }` prints its expansion, and attaching the origin
    /// unconditionally would break every such line. That is this predicate's
    /// whole job, and it is registered as §816's third bar leg.
    fn keyof_origin_applies(&mut self, target: TypeId) -> bool {
        // An operand whose printed form is STRUCTURAL is upstream's anonymous
        // object type, which takes no origin: `keyof { x: number; y: number; }`
        // prints `"x" | "y"`. This port stores a JS object-literal type as
        // `Named` with a structural text rather than as `Anonymous`, so the
        // variant alone cannot tell them apart and the text has to
        // (`checkJsObjectLiteralHasCheckedKeyof`, leg 3's first firing).
        if self.type_to_string(target).starts_with('{') {
            return false;
        }
        if let Some((symbol, _)) = self.type_reference_targets.get(&target) {
            // An alias reference whose KEYS came from evaluating its body is a
            // mapped type upstream — `Omit`, `Pick`, `Partial` and friends —
            // and `getIndexTypeEx` routes those to `getIndexTypeForMappedType`
            // instead of `getLiteralTypeFromProperties`, so no origin is ever
            // attached and the expansion prints. This port has no mapped types
            // to test for, and "we had to evaluate an alias body to find the
            // keys" is the signal it does have
            // (`divideAndConquerIntersections`, leg 3's second firing).
            return !self.binder.symbols().get(*symbol).flags.contains(SymbolFlags::TYPE_ALIAS);
        }
        matches!(&self.store.get(target).data, crate::types::TypeData::Named { members, .. } if members.is_some())
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

impl Checker<'_, '_> {
    /// Whether a written `unique symbol` sits in a position that may carry one.
    ///
    /// `isValidESSymbolDeclaration` (`checker/utilities.go:961`):
    ///
    /// ```go
    /// if ast.IsVariableDeclaration(node) {
    ///     return ast.IsVarConst(node) && ast.IsIdentifier(node.Name()) && isVariableDeclarationInVariableStatement(node)
    /// }
    /// if ast.IsPropertyDeclaration(node) {
    ///     return hasReadonlyModifier(node) && ast.HasStaticModifier(node)
    /// }
    /// return ast.IsPropertySignatureDeclaration(node) && hasReadonlyModifier(node)
    /// ```
    ///
    /// Reached from the operator node, so the walk is upstream's
    /// `WalkUpParenthesizedTypes(node.Parent)`: `(unique symbol)` in a `const`
    /// is still valid. §899.
    fn unique_symbol_position_is_valid(&self, operator: tsr_ast::NodeId) -> bool {
        let mut current = operator;
        let declaration = loop {
            let Some(parent) = self.nodes.parent(current) else { return false };
            if self.nodes.kind(parent) == SyntaxKind::ParenthesizedType {
                current = parent;
                continue;
            }
            break parent;
        };
        let has = |modifiers: &[tsr_ast::ModifierLike<'_>], kind: SyntaxKind| {
            modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == kind)
            })
        };
        // **Decidable positions only.** Upstream answers `false` for everything
        // it does not recognise, but its `node` is always the real declaration;
        // this walk climbs a syntactic parent chain that a JSDoc `@type` does
        // not share — `/** @type {unique symbol} */ const x = Symbol()` puts a
        // `JSDocTypeExpression` between the operator and the declaration, and
        // treating "not recognised" as invalid answered `symbol` there where
        // upstream answers `unique symbol` (6 `RIGHT→WRONG` in
        // `compiler/uniqueSymbolJs2`). So this declines only where it can SEE an
        // invalid declaration, and an unrecognised shape keeps the unique type —
        // the tri-state discipline the relater and
        // [`Checker::is_literal_of_contextual_type`] already use.
        match self.node_map.get(declaration) {
            Some(Node::VariableDeclaration(node)) => {
                if !matches!(node.name, Some(tsr_ast::BindingName::Identifier(_))) {
                    return false;
                }
                // `isVariableDeclarationInVariableStatement` AND `IsVarConst`,
                // both read off the list: a `for (const x of …)` head is a
                // declaration list whose parent is not a `VariableStatement`.
                let Some(list) = self.nodes.parent(declaration) else { return false };
                self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST)
                    && self
                        .nodes
                        .parent(list)
                        .is_some_and(|s| self.nodes.kind(s) == SyntaxKind::VariableStatement)
            }
            Some(Node::PropertyDeclaration(node)) => {
                has(node.modifiers, SyntaxKind::ReadonlyKeyword)
                    && has(node.modifiers, SyntaxKind::StaticKeyword)
            }
            Some(Node::PropertySignatureDeclaration(node)) => {
                has(node.modifiers, SyntaxKind::ReadonlyKeyword)
            }
            // **A PARAMETER keeps the unique type**, which is not what
            // `isValidESSymbolDeclaration` answers — it returns `false` there —
            // and the baselines are unambiguous:
            // `conformance/uniqueSymbolsErrors` records
            // `>invalidArgType : (arg: unique symbol) => void`. Upstream errors
            // on the position and still PRINTS the written form, because the
            // signature's text comes from the node builder reusing the written
            // annotation rather than from the computed type. Declining here
            // measured 10 `RIGHT→WRONG`, all of them parameter or `this`
            // positions in that one case.
            //
            // This is the ADR-0006 rule in miniature: the oracle is the
            // generated baseline, not a reading of the checker source.
            _ => true,
        }
    }
}

impl Checker<'_, '_> {
    /// §909: the name of the NON-GENERIC type alias this node is the body of.
    ///
    /// Upstream attaches an `aliasSymbol` to a type minted from an alias body
    /// and its node builder prints that name. This port's §905 mint has no alias
    /// link, so the body was printed everywhere.
    ///
    /// Non-generic only: a generic alias is instantiated per reference and
    /// upstream prints the instantiated body, which is what §905's 63-row gain
    /// in `mappedTypeRelationships` is made of.
    fn non_generic_alias_body_name(&self, node: tsr_ast::NodeId) -> Option<String> {
        let parent = self.nodes.parent(node)?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(parent) else {
            return None;
        };
        if !alias.type_parameters.is_empty() {
            return None;
        }
        Some(alias.name?.text.to_string())
    }
}
