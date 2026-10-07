//! Type nodes, and what a *type* symbol declares.
//!
//! Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`) and
//! `Checker.getDeclaredTypeOfSymbol` (`checker.go:23670`). These are one module
//! because they call each other on every type reference: a `TypeReferenceNode`
//! resolves a name and asks the symbol what it declares.
//!
//! Not to be confused with [`crate::symbols`], which answers what type a
//! *value* symbol has.

use tsr_ast::{Expression, Node, NodeId, Statement, SyntaxKind, TypeNode};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{checker::Checker, flags::TypeFlags, resolution::PropertyName, types::TypeId};

/// Native resolvedDefaultType belongs to a private type-parameter identity.
/// This port also evaluates AST nodes under outer alias frames and mapped
/// templates, so those contexts cannot share a completed default.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeParameterDefaultKey {
    parameter: TypeId,
    bindings: Vec<(SymbolId, TypeId)>,
    mapped_template: bool,
}

/// Absent storage is uncomputed; active recursion marks only the re-entered
/// parameter circular. A gap is unsupported and is never retained as completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeParameterDefaultState {
    Resolving,
    Circular,
    Resolved(Option<TypeId>),
    Unsupported(TypeId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeLiteralKey {
    pub(crate) node: tsr_ast::NodeId,
    bindings: Vec<(SymbolId, TypeId)>,
    mapped_template: bool,
}

impl TypeLiteralKey {
    pub(crate) fn is_unmapped(&self) -> bool {
        self.bindings.is_empty() && !self.mapped_template
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConditionalInferenceNode {
    declaration: NodeId,
    bindings: rustc_hash::FxHashMap<SymbolId, TypeId>,
}

impl<'a> Checker<'a, '_> {
    /// Ported from typescript-go's `getResolvedTypeParameterDefault`
    /// (`internal/checker/checker.go:22007`), pinned at 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
    /// The private Checker owns the key, result and captured mapper identities
    /// for its Program lifetime. Publish Resolving before evaluating the AST;
    /// a recursive read changes that exact slot to Circular, and the outer
    /// computation must not overwrite it. Completed absence differs from an
    /// unsupported Rust evaluation, whose exact unresolved identity still flows
    /// to the caller without completed reuse. Receiver/alias substitutions remain
    /// in the outer bindings or the instantiated parameter's existing target mapper.
    /// The worker is default-node evaluation (or target-default instantiation);
    /// this correctness state is not a measured member-reuse optimization.
    pub(crate) fn get_resolved_type_parameter_default(
        &mut self,
        parameter: TypeId,
    ) -> TypeParameterDefaultState {
        if !self.store.get(parameter).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return TypeParameterDefaultState::Resolved(None);
        }
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        let key = TypeParameterDefaultKey {
            parameter,
            bindings,
            mapped_template: self.mapped_template_depth > 0,
        };
        if let Some(state) = self.type_parameter_default_cache.get(&key).copied() {
            if state == TypeParameterDefaultState::Resolving {
                self.type_parameter_default_cache.insert(key, TypeParameterDefaultState::Circular);
                return TypeParameterDefaultState::Circular;
            }
            return state;
        }
        self.type_parameter_default_cache.insert(key.clone(), TypeParameterDefaultState::Resolving);
        let computed = if let Some(instance) =
            self.instantiated_type_parameters.get(&parameter).cloned()
        {
            // The target owns its declaration default. Resolve it before
            // applying the captured mapper, without an unrelated caller frame.
            let frames = std::mem::take(&mut self.alias_evaluation_bindings);
            let target_default = self.get_resolved_type_parameter_default(instance.target);
            self.alias_evaluation_bindings = frames;
            match target_default {
                TypeParameterDefaultState::Resolved(Some(default))
                | TypeParameterDefaultState::Unsupported(default) => {
                    let names: Vec<_> = instance.names.iter().map(String::as_str).collect();
                    let image =
                        self.instantiate_type(default, &instance.map, &instance.parameters, &names);
                    if matches!(target_default, TypeParameterDefaultState::Unsupported(_))
                        || self.is_error(image)
                    {
                        TypeParameterDefaultState::Unsupported(image)
                    } else {
                        TypeParameterDefaultState::Resolved(Some(image))
                    }
                }
                state => state,
            }
        } else {
            let node = self.type_parameter_symbols.get(&parameter).and_then(|&symbol| {
                self.binder.symbols().get(symbol).declarations.iter().find_map(|&declaration| {
                    match self.node_map.get(declaration) {
                        Some(Node::TypeParameterDeclaration(declaration)) => {
                            declaration.default_type
                        }
                        _ => None,
                    }
                })
            });
            match node {
                Some(node) => {
                    let default = self.get_type_from_type_node(node);
                    if self.is_error(default) {
                        TypeParameterDefaultState::Unsupported(default)
                    } else {
                        TypeParameterDefaultState::Resolved(Some(default))
                    }
                }
                None => TypeParameterDefaultState::Resolved(None),
            }
        };
        if self.type_parameter_default_cache.get(&key) == Some(&TypeParameterDefaultState::Circular)
        {
            return TypeParameterDefaultState::Circular;
        }
        if matches!(computed, TypeParameterDefaultState::Unsupported(_)) {
            self.type_parameter_default_cache.remove(&key);
        } else {
            self.type_parameter_default_cache.insert(key, computed);
        }
        computed
    }

    /// Ported from typescript-go's `getDefaultFromTypeParameter`
    /// (`internal/checker/checker.go:21996`). Circular and absent defaults use
    /// the caller's ordinary fallback; a Rust gap retains its unresolved identity
    /// without being retained as a completed default.
    pub(crate) fn get_default_from_type_parameter(&mut self, parameter: TypeId) -> Option<TypeId> {
        match self.get_resolved_type_parameter_default(parameter) {
            TypeParameterDefaultState::Resolved(default) => default,
            TypeParameterDefaultState::Unsupported(default) => Some(default),
            TypeParameterDefaultState::Circular | TypeParameterDefaultState::Resolving => None,
        }
    }

    /// getTypeFromClassOrInterfaceReference / fillMissingTypeArguments for a
    /// heritage member lookup. Defaults see the arguments already supplied.
    pub(crate) fn instantiated_heritage_base(
        &mut self,
        base: SymbolId,
        written_arguments: &[TypeNode<'a>],
        location: Option<NodeId>,
    ) -> Option<TypeId> {
        let is_js = location.is_some_and(|node| self.in_js_file(node));
        let declarations = self.local_type_parameters_of(base);
        if declarations.is_empty() {
            return written_arguments.is_empty().then(|| self.get_declared_type_of_symbol(base));
        }
        let minimum = declarations
            .iter()
            .rposition(|p| p.default_type.is_none())
            .map_or(0, |index| index + 1);
        if written_arguments.len() > declarations.len()
            || (!is_js && written_arguments.len() < minimum)
        {
            return None;
        }
        let parameter_types = self.local_type_parameter_types_of(base)?;
        let ids: Vec<_> = parameter_types.iter().map(|&(id, _)| id).collect();
        let names: Vec<_> = parameter_types.iter().map(|(_, name)| name.as_str()).collect();
        let mut arguments: Vec<_> =
            written_arguments.iter().map(|&node| self.get_type_from_type_node(node)).collect();
        let written_count = arguments.len();
        // Native fills unresolved slots before instantiating any default so
        // an invalid forward reference cannot escape as a free parameter.
        arguments.resize(declarations.len(), self.intrinsics.error);
        for index in written_count..declarations.len() {
            let mut default = match self.get_default_from_type_parameter(ids[index]) {
                Some(default) => default,
                None => {
                    if is_js {
                        self.intrinsics.any
                    } else {
                        self.intrinsics.unknown
                    }
                }
            };
            // fillMissingTypeArguments uses implicit any for unknown/empty
            // object defaults in JavaScript heritage references.
            if is_js
                && (default == self.intrinsics.unknown
                    || self.is_empty_anonymous_object_type(default))
            {
                default = self.intrinsics.any;
            }
            let map: Vec<_> = ids.iter().copied().zip(arguments.iter().copied()).collect();
            arguments[index] = self.instantiate_type(default, &map, &ids, &names);
        }
        if arguments.iter().any(|&argument| self.is_error(argument)) {
            return None;
        }
        Some(self.create_type_reference(base, arguments))
    }

    /// The type a type node denotes.
    ///
    /// Ported from `Checker.getTypeFromTypeNodeWorker` (`checker.go:22811`),
    /// restricted to the forms this slice covers: the keyword types, literal
    /// types, and parenthesised types. Everything else — type references, arrays,
    /// unions, intersections, conditionals, mapped types, and the rest — yields
    /// `errorType`, because none of those type *shapes* exists yet.
    pub fn get_type_from_type_node(&mut self, node: TypeNode<'a>) -> TypeId {
        if self.resolutions.has_property_frame(PropertyName::DeclaredType)
            && self.native_resolves_lazily(node)
        {
            self.resolutions.enter_deferred();
            let ty = self.get_type_from_type_node_worker(node);
            self.resolutions.exit_deferred();
            return ty;
        }
        self.get_type_from_type_node_worker(node)
    }

    /// Whether native builds `node`'s type without resolving its constituents
    /// now, so a type alias it mentions is not reached while the enclosing
    /// alias resolves: an anonymous type literal, function, constructor or
    /// mapped type (`getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`,
    /// `getTypeFromMappedTypeNode`, members resolved on demand), or a deferred
    /// array/tuple/class-or-interface reference (`isDeferredTypeReferenceNode`,
    /// `checker.go:23236`). Consulted only while a declared type is resolving,
    /// to mark [`crate::resolution::Resolutions::enter_deferred`] boundaries.
    fn native_resolves_lazily(&mut self, node: TypeNode<'a>) -> bool {
        // A BRANCH of a conditional type: getConditionalType resolves
        // `root.node.TrueType`/`FalseType` only once the conditional is no
        // longer deferred (checker.go:24300), so a deferred conditional — the
        // shape of every generic alias body over its own parameters, lib
        // `Awaited<T>` recursing through `Awaited<V>` — never reaches the alias
        // from a branch while its declared type resolves. This port does not
        // know at this point whether the conditional defers, so every branch
        // is a boundary: for a fully concrete conditional that is lazier than
        // upstream, and the cost is a missed TS2456, never a false one.
        if let Some(id) = tsr_ast::Node::from(node).node_id()
            && let Some(parent) = self.nodes.parent(id)
            && let Some(Node::ConditionalTypeNode(conditional)) = self.node_map.get(parent)
            && [conditional.true_type, conditional.false_type]
                .into_iter()
                .flatten()
                .any(|branch| tsr_ast::Node::from(branch).node_id() == Some(id))
        {
            return true;
        }
        match node {
            TypeNode::TypeLiteralNode(_)
            | TypeNode::FunctionTypeNode(_)
            | TypeNode::ConstructorTypeNode(_)
            | TypeNode::MappedTypeNode(_) => true,
            TypeNode::ArrayTypeNode(array) => {
                self.is_deferred_type_reference_node(array.node_id, false, |checker| {
                    array
                        .element_type
                        .is_some_and(|element| checker.may_resolve_type_alias(element))
                })
            }
            TypeNode::TupleTypeNode(tuple) => {
                self.is_deferred_type_reference_node(tuple.node_id, false, |checker| {
                    tuple.elements.iter().any(|&element| checker.may_resolve_type_alias(element))
                })
            }
            TypeNode::TypeReferenceNode(reference) => {
                // getTypeFromClassOrInterfaceReference's generic arm only;
                // an alias reference resolves its arguments eagerly.
                let Some(name) = reference.type_name else { return false };
                let Some(mut symbol) = self.resolve_entity_name(name, SymbolFlags::TYPE) else {
                    return false;
                };
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::ALIAS) {
                    let Some(target) = self.resolve_alias(symbol) else { return false };
                    symbol = self.binder.merged_symbol(target);
                }
                if !self
                    .binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                {
                    return false;
                }
                let parameters = self.local_type_parameters_of(symbol).len();
                if parameters == 0 {
                    return false;
                }
                let has_default_type_arguments = reference.type_arguments.len() != parameters;
                self.is_deferred_type_reference_node(
                    reference.node_id,
                    has_default_type_arguments,
                    |checker| {
                        reference
                            .type_arguments
                            .iter()
                            .any(|&argument| checker.may_resolve_type_alias(argument))
                    },
                )
            }
            _ => false,
        }
    }

    /// `isDeferredTypeReferenceNode` (`checker.go:23236`); `constituents` is
    /// its kind-specific `mayResolveTypeAlias` test.
    fn is_deferred_type_reference_node(
        &mut self,
        node: Option<NodeId>,
        has_default_type_arguments: bool,
        constituents: impl FnOnce(&mut Self) -> bool,
    ) -> bool {
        let Some(node) = node else { return false };
        if self.type_alias_host_for_type_node(node).is_some() {
            return true;
        }
        self.is_resolved_by_type_alias(node) && (has_default_type_arguments || constituents(self))
    }

    /// `isResolvedByTypeAlias` (`checker.go:23257`): `node` is transitively
    /// contained in type constructs that eagerly resolve their constituents,
    /// up to a type alias declaration.
    fn is_resolved_by_type_alias(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            match self.nodes.kind(parent) {
                SyntaxKind::ParenthesizedType
                | SyntaxKind::NamedTupleMember
                | SyntaxKind::TypeReference
                | SyntaxKind::UnionType
                | SyntaxKind::IntersectionType
                | SyntaxKind::IndexedAccessType
                | SyntaxKind::ConditionalType
                | SyntaxKind::TypeOperator
                | SyntaxKind::ArrayType
                | SyntaxKind::TupleType => current = parent,
                SyntaxKind::TypeAliasDeclaration => return true,
                _ => return false,
            }
        }
        false
    }

    /// `mayResolveTypeAlias` (`checker.go:23271`): whether resolving `node`
    /// possibly resolves a type alias.
    fn may_resolve_type_alias(&mut self, node: TypeNode<'a>) -> bool {
        match node {
            TypeNode::TypeReferenceNode(reference) => {
                let Some(name) = reference.type_name else { return false };
                let Some(mut symbol) = self.resolve_entity_name(name, SymbolFlags::TYPE) else {
                    return false;
                };
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::ALIAS) {
                    let Some(target) = self.resolve_alias(symbol) else { return false };
                    symbol = self.binder.merged_symbol(target);
                }
                self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            }
            TypeNode::TypeQueryNode(_) => true,
            TypeNode::TypeOperatorNode(operator) => {
                operator.operator.kind != SyntaxKind::UniqueKeyword
                    && operator.r#type.is_some_and(|inner| self.may_resolve_type_alias(inner))
            }
            TypeNode::ParenthesizedTypeNode(inner) => {
                inner.r#type.is_some_and(|inner| self.may_resolve_type_alias(inner))
            }
            TypeNode::OptionalTypeNode(inner) => {
                inner.r#type.is_some_and(|inner| self.may_resolve_type_alias(inner))
            }
            TypeNode::NamedTupleMember(member) => {
                member.r#type.is_some_and(|inner| self.may_resolve_type_alias(inner))
            }
            TypeNode::RestTypeNode(rest) => match rest.r#type {
                Some(TypeNode::ArrayTypeNode(array)) => {
                    array.element_type.is_some_and(|element| self.may_resolve_type_alias(element))
                }
                Some(_) => true,
                None => false,
            },
            TypeNode::UnionTypeNode(union) => {
                union.types.iter().any(|&member| self.may_resolve_type_alias(member))
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                intersection.types.iter().any(|&member| self.may_resolve_type_alias(member))
            }
            TypeNode::IndexedAccessTypeNode(access) => {
                access.object_type.is_some_and(|object| self.may_resolve_type_alias(object))
                    || access.index_type.is_some_and(|index| self.may_resolve_type_alias(index))
            }
            TypeNode::ConditionalTypeNode(conditional) => [
                conditional.check_type,
                conditional.extends_type,
                conditional.true_type,
                conditional.false_type,
            ]
            .into_iter()
            .flatten()
            .any(|part| self.may_resolve_type_alias(part)),
            _ => false,
        }
    }

    fn get_type_from_type_node_worker(&mut self, node: TypeNode<'a>) -> TypeId {
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
                // `checker.go` getTypeFromLiteralTypeNode: a `null` literal type
                // is `nullType`, not the expression's `nullWideningType`.
                if matches!(node, Node::KeywordExpression(keyword) if keyword.kind == SyntaxKind::NullKeyword)
                {
                    return self.intrinsics.null;
                }
                let Ok(expression) = Expression::try_from(node) else {
                    return self.intrinsics.error;
                };
                let id = self.check_expression(expression);
                self.get_regular_type_of_literal_type(id)
            }
            TypeNode::ParenthesizedTypeNode(node) => node
                .r#type
                .map_or(self.intrinsics.error, |inner| self.get_type_from_type_node(inner)),
            TypeNode::TypeReferenceNode(node) => {
                let ty = self.get_type_from_type_reference(node);
                if self.type_reference_targets.get(&ty).is_some_and(|(symbol, _)| {
                    self.binder
                        .symbols()
                        .get(*symbol)
                        .flags
                        .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
                }) {
                    self.reference_types_from_nodes.insert(ty);
                }
                ty
            }
            TypeNode::ImportTypeNode(node) => self.get_type_from_import_type_node(node),
            TypeNode::TypeLiteralNode(node) => self.get_type_from_type_literal(node),
            TypeNode::UnionTypeNode(node) => self.get_type_from_union_type_node(node),
            TypeNode::IntersectionTypeNode(node) => self.get_type_from_intersection_type_node(node),
            TypeNode::ArrayTypeNode(node) => self.get_type_from_array_type_node(node),
            TypeNode::TupleTypeNode(node) => self.get_type_from_tuple_type_node(node),
            TypeNode::FunctionTypeNode(node) => self.get_type_from_function_type_node(node),
            TypeNode::ConstructorTypeNode(node) => self.get_type_from_constructor_type_node(node),
            TypeNode::TypeQueryNode(node) => self.get_type_from_type_query_node(node),
            // getTypeFromInferTypeNode reads the declared parameter identity.
            TypeNode::InferTypeNode(node) => node
                .type_parameter
                .and_then(|parameter| parameter.node_id)
                .and_then(|id| self.binder.symbol_of(id))
                .map_or(self.intrinsics.error, |symbol| self.get_declared_type_of_symbol(symbol)),
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
            // Resolve mapped/concrete operands and operands under alias
            // bindings. Polymorphic this needs the same semantic index mint;
            // ordinary written keyof parameters retain their legacy metadata.
            TypeNode::TypeOperatorNode(node)
                if node.operator.kind == SyntaxKind::KeyOfKeyword
                    && !node.r#type.is_some_and(|mut operand| {
                        while let TypeNode::ParenthesizedTypeNode(parenthesized) = operand {
                            let Some(inner) = parenthesized.r#type else { return false };
                            operand = inner;
                        }
                        matches!(
                            operand,
                            TypeNode::UnionTypeNode(_) | TypeNode::IntersectionTypeNode(_)
                        )
                    })
                    && (!self.alias_evaluation_bindings.is_empty()
                        // §730: a CONCRETE operand's key set is final, so the
                        // operator may be evaluated. §729 measured this predicate
                        // at 23 WRONG→RIGHT and ZERO RIGHT→WRONG; its 90
                        // GAP→WRONG were the PRINTED form, which the written-text
                        // arm in `signatures.rs` now supplies.
                        || node.r#type.is_some_and(|inner| {
                            let target = self.get_type_from_type_node(inner);
                            target != self.intrinsics.error
                                && (matches!(inner, TypeNode::ThisTypeNode(_))
                                    || !self.mentions_any_type_parameter(target, 4))
                        })) =>
            {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let target = self.get_type_from_type_node(inner);
                if self.mapped_types.contains_key(&target)
                    || self.store.get(target).flags.contains(TypeFlags::TYPE_PARAMETER)
                {
                    return self.resolved_keyof_type(target).unwrap_or(self.intrinsics.error);
                }
                // getIndexType over a concrete object operand: the semantic
                // key union, so a numeric-literal property name contributes a
                // NUMBER literal key (`getLiteralTypeFromPropertyName`,
                // `checker.go:26773`) where `keys_of` spells every key as a
                // string. Generic, unresolved and non-object operands keep
                // the legacy road below.
                if self.store.get(target).flags.contains(TypeFlags::OBJECT)
                    && !self.unresolved_types.contains(&target)
                    && !self.mentions_any_type_parameter(target, 4)
                    && let Some(keys) = self.resolved_keyof_type(target)
                {
                    return keys;
                }
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
                            self.union_with_origin_text(union, text)
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
                // The historical print-only mint below now retains mapped
                // constraint/template metadata for reverse inference. General
                // forward mapped members remain a separate port.
                // Immediately nested conditionals continue under the current
                // mapper instead of minting their uninstantiated written form.
                // getTypeFromConditionalTypeNode (checker.go:24269) resolves
                // every conditional node through getConditionalType with no
                // mapper, so a non-deferred check also evaluates outside an
                // alias frame. A deferred one keeps the written mint below.
                if let TypeNode::ConditionalTypeNode(conditional) = node
                    && (!self.alias_evaluation_bindings.is_empty()
                        || self.mapped_template_depth == 0)
                {
                    if self.instantiation_depth == 100 {
                        return self.intrinsics.error;
                    }
                    self.instantiation_depth += 1;
                    let evaluated = self.evaluate_conditional_node(conditional, None);
                    self.instantiation_depth -= 1;
                    if let Some(evaluated) = evaluated {
                        return evaluated;
                    }
                }
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
                    let id = self.store.new_named(TypeFlags::OBJECT, name, None);
                    if let TypeNode::MappedTypeNode(mapped) = node {
                        self.capture_mapped_type(id, mapped);
                    }
                    return id;
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
                    Some(text) => {
                        let flags = if matches!(node, TypeNode::ConditionalTypeNode(_))
                        {
                            TypeFlags::CONDITIONAL
                        } else {
                            TypeFlags::OBJECT
                        };
                        let id = self.store.new_named(flags, text, None);
                        if let TypeNode::MappedTypeNode(mapped) = node {
                            self.capture_mapped_type(id, mapped);
                            if let Some(alias) = mapped.node_id.and_then(|node| self.alias_symbol_for_type_node(node)) {
                                let arguments = self.local_type_parameter_types_of(alias)
                                    .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                                self.alias_of.insert(id, (alias, arguments));
                            }
                        } else if let TypeNode::ConditionalTypeNode(conditional) = node
                            && self.mapped_template_depth > 0
                            && let (Some(true_type), Some(false_type)) =
                                (conditional.true_type, conditional.false_type)
                        {
                            let true_type = self.get_type_from_type_node(true_type);
                            let false_type = self.get_type_from_type_node(false_type);
                            if true_type != self.intrinsics.error
                                && false_type != self.intrinsics.error
                            {
                                self.mapped_conditional_branches
                                    .insert(id, (true_type, false_type));
                            }
                            // The root and mapper remain useful when a branch
                            // is not yet computable. Re-evaluate it only after
                            // substituting the check operand (getConditionalTypeInstantiation).
                            if let (Some(declaration), Some(check), Some(extends)) = (
                                conditional.node_id,
                                conditional.check_type,
                                conditional.extends_type,
                            ) {
                                let check = self.get_type_from_type_node(check);
                                let extends = self.get_type_from_type_node(extends);
                                let bindings = self
                                    .alias_evaluation_bindings
                                    .iter()
                                    .flat_map(|frame| {
                                        frame.iter().map(|(&symbol, &ty)| (symbol, ty))
                                    })
                                    .collect();
                                self.mapped_conditionals.insert(
                                    id,
                                    crate::mapped::MappedConditionalInfo {
                                        declaration,
                                        bindings,
                                        operands: [check, extends, true_type, false_type],
                                    },
                                );
                            }
                        } else if let TypeNode::ConditionalTypeNode(conditional) = node
                            && let Some(declaration) = conditional.node_id
                        {
                            let bindings = self
                                .alias_evaluation_bindings
                                .iter()
                                .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
                                .collect();
                            self.conditional_inference_nodes
                                .insert(id, ConditionalInferenceNode { declaration, bindings });
                        }
                        if matches!(node, TypeNode::ConditionalTypeNode(_))
                            && let Some(alias) = Node::from(node).node_id().and_then(|node| self.alias_symbol_for_type_node(node)) {
                            let arguments = self.local_type_parameter_types_of(alias)
                                .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                            self.alias_of.insert(id, (alias, arguments));
                        }
                        id
                    }
                    None => match node {
                        TypeNode::MappedTypeNode(mapped) => self
                            .create_semantic_mapped_type(mapped)
                            .unwrap_or(self.intrinsics.error),
                        _ => self.intrinsics.error,
                    },
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
            // `getTypeFromTypeNode`'s `KindJSDocOptionalType` arm
            // (`checker.go:22826`): `addOptionality` of the inner type.
            TypeNode::JSDocOptionalType(node) => {
                let Some(inner) = node.r#type else { return self.intrinsics.error };
                let inner = self.get_type_from_type_node(inner);
                if inner == self.intrinsics.error || !self.strict_null_checks {
                    return inner;
                }
                self.get_optional_type(inner, false)
            }
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
            TypeNode::ThisTypeNode(node) => self.get_type_from_this_type_node(node),
            // §35 (`checker-notes-callres.md`): DEFERRED `keyof` over a type
            // parameter prints as written — the §34 mint, the §31
            // registration. Unions and intersections follow
            // `getIndexTypeEx`'s semantic distribution: the keys of a union are
            // intersected and the keys of an intersection are unioned.
            // Alias consumers independently retain their written form.
            TypeNode::TypeOperatorNode(node) if node.operator.kind == SyntaxKind::KeyOfKeyword => {
                let mut direct_operand = node.r#type;
                while let Some(TypeNode::ParenthesizedTypeNode(parenthesized)) = direct_operand {
                    direct_operand = parenthesized.r#type;
                }
                // getIndexTypeEx consumes an alias's instantiated body, not
                // its display identity. Resolve the whole operand first so
                // absorbing any/never constituents reduce before distribution.
                if let Some(operand @ TypeNode::TypeReferenceNode(_)) = direct_operand {
                    let target = self.get_type_from_type_node(operand);
                    let alias_reference =
                        self.type_reference_targets.get(&target).is_some_and(|(symbol, _)| {
                            self.binder
                                .symbols()
                                .get(*symbol)
                                .flags
                                .contains(SymbolFlags::TYPE_ALIAS)
                        });
                    if alias_reference && let Some(keys) = self.resolved_keyof_type(target) {
                        return keys;
                    }
                }
                let (compound, operands, separator) = match direct_operand {
                    Some(TypeNode::UnionTypeNode(union)) => {
                        (Some(TypeNode::UnionTypeNode(union)), Some(union.types), " | ")
                    }
                    Some(TypeNode::IntersectionTypeNode(intersection)) => (
                        Some(TypeNode::IntersectionTypeNode(intersection)),
                        Some(intersection.types),
                        " & ",
                    ),
                    _ => (None, None, ""),
                };
                if let (Some(compound), Some(operands)) = (compound, operands) {
                    let target = self.get_type_from_type_node(compound);
                    if let Some(keys) = self.resolved_keyof_type(target) {
                        let written_operands: Vec<_> = operands
                            .iter()
                            .map(|&operand| self.get_type_from_type_node(operand))
                            .collect();
                        // The semantic type is distributed, while a declaration
                        // signature reuses the written operator node. Keep that
                        // spelling in the existing node-reuse channel instead
                        // of baking it into the semantic union/intersection.
                        if let Some(id) = node.node_id {
                            let written = format!(
                                "keyof ({})",
                                written_operands
                                    .iter()
                                    .map(|&operand| self.type_to_string(operand))
                                    .collect::<Vec<_>>()
                                    .join(separator)
                            );
                            self.qualified_written_text.entry(id).or_insert(written);
                        }
                        return keys;
                    }
                }
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
                        if let Some(operand) = node.r#type {
                            let operand = self.get_type_from_type_node(operand);
                            if operand != self.intrinsics.error {
                                self.deferred_keyof_operands.insert(id, operand);
                            }
                        }
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
                let mut texts = vec![head.text.to_owned()];
                let mut types = Vec::new();
                for span in node.template_spans {
                    let Some(hole) = span.r#type else { return self.intrinsics.error };
                    types.push(self.get_type_from_type_node(hole));
                    texts.push(match span.literal {
                        Some(tsr_ast::TemplateMiddleOrTail::TemplateMiddle(middle)) => {
                            middle.text.to_owned()
                        }
                        Some(tsr_ast::TemplateMiddleOrTail::TemplateTail(tail)) => {
                            tail.text.to_owned()
                        }
                        None => return self.intrinsics.error,
                    });
                }
                self.get_template_literal_type(&texts, &types)
            }
            // §34 (`checker-notes-callres.md`): a DEFERRED indexed access —
            // the index is a type parameter, upstream cannot resolve it until
            // instantiation — prints as written via the §31 mint; is_error
            // stays true through `unresolved_types`, so only the printed
            // line changes. Literal indexes resolve concretely upstream and
            // stay declined here.
            TypeNode::IndexedAccessTypeNode(node) => {
                if let (Some(object), Some(index)) = (node.object_type, node.index_type) {
                    let object = self.get_type_from_type_node(object);
                    let index = self.get_type_from_type_node(index);
                    if let Some(t) = self.resolved_indexed_access_type(object, index, false) {
                        if self.store.get(index).flags.contains(TypeFlags::UNION)
                            && let crate::types::TypeData::Union { types, .. } =
                                self.store.get(t).data.clone()
                            && let Some(alias) =
                                node.node_id.and_then(|id| self.alias_symbol_for_type_node(id))
                            && self.alias_evaluation_bindings.is_empty()
                            && self.local_type_parameters_of(alias).is_empty()
                        {
                            return self.get_named_union_type(&types, TypeFlags::empty(), alias);
                        }
                        if self.deferred_indexed_access_types.contains_key(&t)
                            && let Some(alias) = node.node_id.and_then(|node| self.alias_symbol_for_type_node(node)) {
                            let arguments = self.local_type_parameter_types_of(alias)
                                .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                            return self.alias_object_image(t, alias, arguments);
                        }
                        return t;
                    }
                    if self.variadic_tuple_elements.contains_key(&object)
                        && let Some(t) = self.tuple_index_type(object, index, false)
                    {
                        return t;
                    }
                }
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
                // §952.2: a STRING-LITERAL index naming a PROPERTY answers that
                // property's type. §619 recorded *"literal indexes resolve
                // concretely upstream and stay declined here"* as a limitation
                // and it was never measured; upstream's `getIndexedAccessType`
                // resolves exactly this, and the numeric and tuple arms above are
                // the two special cases of it that were already ported.
                //
                // Reached only where the road answered `error`, so the failure
                // direction is gap→wrong rather than right→wrong — the same
                // argument §620 makes for the arm above it.
                //
                // This is what makes §952's mapped members reachable from the
                // TYPE side: `Partial<O>["x"]` is `string | undefined` because
                // `get_type_of_property_of_type` applies the mapping's modifier,
                // so the arm needs no mapped-specific knowledge of its own.
                if let (Some(object_node), Some(index_node)) = (node.object_type, node.index_type) {
                    let object_type = self.get_type_from_type_node(object_node);
                    if object_type != self.intrinsics.error {
                        let index_type = self.get_type_from_type_node(index_node);
                        if let crate::types::TypeData::StringLiteral(name) =
                            self.store.get(index_type).data.clone()
                            // A numeric-literal NAME is a numeric index and the
                            // arm above owns it (`isNumericLiteralName`).
                            //
                            // `normalise_number` is NOT the test: it returns its
                            // input unchanged for text whose value it cannot
                            // read, so `normalise_number("x") == "x"` and a
                            // first draft using it excluded every ordinary
                            // property name — `O["x"]` stayed `error` and the arm
                            // looked unreachable. A failed parse is the test that
                            // actually separates a name from an index.
                            && name.parse::<f64>().is_err()
                            && let Some(member) =
                                self.get_type_of_property_of_type(object_type, &name)
                            && member != self.intrinsics.error
                        {
                            return member;
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
                        if let (Some(object), Some(index)) = (node.object_type, node.index_type) {
                            let object = self.get_type_from_type_node(object);
                            let index = self.get_type_from_type_node(index);
                            if object != self.intrinsics.error && index != self.intrinsics.error {
                                self.deferred_indexed_access_types
                                    .insert(id, (object, index, false));
                            }
                        }
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
    /// `getInstantiationExpressionType` (`checker.go:10660`) consumes the
    /// original node's argument-list presence, types and span through the shared
    /// expression worker before widening/regularization. No caller-side arity
    /// check or argument-free adapter duplicates that worker.
    ///
    /// Divergence, stated: this port has no general `getWidenedType`
    /// (`checker.go:18355`). Entity-name expression types come from
    /// `get_type_of_symbol`, which widens variable-like declarations at the
    /// declaration, so the residual work here is fresh-literal regularisation —
    /// `get_regular_type_of_literal_type`. A `typeof` line needing
    /// object-literal widening *at the query* is owned by the notes page §2.
    fn get_type_from_type_query_node(&mut self, node: &tsr_ast::TypeQueryNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        if let Some(id) = node.node_id { self.check_instantiation_expression_grammar(id); }
        let Some(name) = node.expr_name else { return error };
        // The first run of this arm refused `typeof` over a parameter symbol
        // here, after the registered bar fired (+1,341 gap→wrong,
        // `checker-notes-tquery.md` §5). That narrowing is superseded by the
        // faithful mechanism: signature rendering reuses the written
        // `typeof a` node ([`crate::signatures::Parameter::written_text`]),
        // which is where every one of those wrong lines was printed. The
        // computation below is upstream's for every entity-name form.
        let id = match name {
            // checkExpressionWithTypeArguments dispatches the query identifier
            // through checkThisExpression, retaining its original flow location
            // and receiver identity rather than resolving a value named "this".
            tsr_ast::EntityName::Identifier(identifier) if identifier.text == "this" => {
                let Some(location) = identifier.node_id else { return error };
                self.check_this_expression(location)
            }
            tsr_ast::EntityName::Identifier(identifier) => {
                if self.typeof_is_parameter_projection(node)
                    && let Some(location) = identifier.node_id
                    && let Some(symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        location,
                        identifier.text,
                        SymbolFlags::VALUE,
                    )
                {
                    self.defer_typeof_function_return(symbol);
                }
                self.check_expression(Expression::Identifier(identifier))
            }
            tsr_ast::EntityName::QualifiedName(qualified) => self.check_qualified_name(qualified),
        };
        // Ported from typescript-go's checkExpressionWithTypeArguments and
        // getTypeFromTypeQueryNode (internal/checker/checker.go): retain the
        // original expression/receiver and raw argument-list metadata. The
        // shared worker owns arity, constraints, diagnostics and publication.
        let Some(origin) = node.node_id else { return error };
        let id = self.get_instantiation_expression_type(id, origin);
        // getTypeFromTypeQueryNode widens before regularizing. Native seeds
        // the global undefined symbol with undefinedWideningType; this port
        // shares its ordinary undefined identity, so retain that provenance
        // through symbol resolution here. Written undefined annotations and
        // shadowing declarations remain non-widening.
        if !self.strict_null_checks
            && id == self.intrinsics.undefined
            && let tsr_ast::EntityName::Identifier(identifier) = name
            && let Some(location) = identifier.node_id
            && let Some(symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                location,
                identifier.text,
                SymbolFlags::VALUE,
            )
            && Some(symbol) == self.binder.undefined_symbol()
        {
            return self.intrinsics.any;
        }
        self.get_regular_type_of_literal_type(id)
    }

    /// Certify the source entry for parameter-only conditional inference
    /// (native inference.go:838–907 / relater.go:1595–1603). Read the original
    /// alias AST and binder identities, never a rendered signature or return.
    /// Only the direct `F extends (...args: infer P) => any/void ? P : never`
    /// projection can defer this typeof source. Return-demanding or wrapped
    /// shapes retain the existing eager route. This writes no alias image.
    fn typeof_is_parameter_projection(&self, query: &tsr_ast::TypeQueryNode<'a>) -> bool {
        let referenced_symbol = |reference: &tsr_ast::TypeReferenceNode<'a>| {
            if !reference.type_arguments.is_empty() {
                return None;
            }
            let tsr_ast::EntityName::Identifier(name) = reference.type_name? else { return None };
            self.binder
                .resolve_name(
                    self.nodes,
                    self.node_map,
                    name.node_id?,
                    name.text,
                    SymbolFlags::TYPE,
                )
                .map(|symbol| self.binder.merged_symbol(symbol))
        };
        let parameter_only_return = |function: &tsr_ast::FunctionTypeNode<'a>| {
            function.type_parameters.is_empty()
                && function.full_signature.is_none()
                && matches!(function.r#type, Some(TypeNode::KeywordTypeNode(keyword))
                    if matches!(keyword.kind, SyntaxKind::AnyKeyword | SyntaxKind::VoidKeyword))
        };
        (|| {
            let id = query.node_id?;
            let Node::TypeReferenceNode(application) = self.node_map.get(self.nodes.parent(id)?)?
            else {
                return None;
            };
            let [TypeNode::TypeQueryNode(argument)] = application.type_arguments else {
                return None;
            };
            if argument.node_id != Some(id) {
                return None;
            }
            let tsr_ast::EntityName::Identifier(name) = application.type_name? else {
                return None;
            };
            let alias = self.binder.merged_symbol(self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name.node_id?,
                name.text,
                SymbolFlags::TYPE,
            )?);
            let owner = self.binder.symbols().get(alias);
            if !owner.flags.contains(SymbolFlags::TYPE_ALIAS) {
                return None;
            }
            let [declaration] = owner.declarations.as_slice() else { return None };
            let Node::TypeAliasDeclaration(declaration) = self.node_map.get(*declaration)? else {
                return None;
            };
            let [parameter] = declaration.type_parameters else { return None };
            if let Some(constraint) = parameter.constraint {
                let TypeNode::FunctionTypeNode(function) = constraint else { return None };
                if !parameter_only_return(function) {
                    return None;
                }
            }
            let TypeNode::ConditionalTypeNode(conditional) = declaration.r#type? else {
                return None;
            };
            let TypeNode::TypeReferenceNode(check) = conditional.check_type? else { return None };
            let parameter_symbol = self.binder.symbol_of(parameter.node_id?)?;
            if referenced_symbol(check) != Some(parameter_symbol) {
                return None;
            }
            let TypeNode::FunctionTypeNode(function) = conditional.extends_type? else {
                return None;
            };
            if !parameter_only_return(function) {
                return None;
            }
            let [rest] = function.parameters else { return None };
            let TypeNode::InferTypeNode(infer) = rest.r#type? else { return None };
            let inferred = infer.type_parameter?;
            if rest.dot_dot_dot_token.is_none()
                || rest.question_token.is_some()
                || rest.initializer.is_some()
                || inferred.constraint.is_some()
            {
                return None;
            }
            let TypeNode::TypeReferenceNode(yes) = conditional.true_type? else { return None };
            let inferred_symbol = self.binder.symbol_of(inferred.node_id?)?;
            if referenced_symbol(yes) != Some(inferred_symbol)
                || !matches!(conditional.false_type, Some(TypeNode::KeywordTypeNode(keyword))
                    if keyword.kind == SyntaxKind::NeverKeyword)
            {
                return None;
            }
            Some(())
        })()
        .is_some()
    }

    /// `getIntendedTypeFromJSDocTypeReference` (`checker.go:23020`): inside
    /// JSDoc, `String`/`Number`/`BigInt`/`Boolean`/`Void`/`Undefined`/`Null`
    /// name the primitives, `Function`/`function` the global `Function` type,
    /// and (without `noImplicitAny`) bare `array`/`promise`/`Object` are
    /// `any[]`/`Promise<any>`/`any`. `None` everywhere else, including
    /// outside JSDoc. Upstream's `NodeFlagsJSDoc` is a parse flag this
    /// parser does not set; a JSDoc type is the one with a JSDoc ancestor,
    /// asked only after the name gate so ordinary references pay nothing.
    ///
    /// Not ported: the `Object.<K, V>` arm, which answers
    /// `getTypeAliasInstantiation(Record, [K, V])`. This port has no alias
    /// instantiation by type list, so that arm declines and the reference
    /// resolves as written, as it did before this function existed.
    fn get_intended_type_from_jsdoc_type_reference(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        name: &str,
        id: NodeId,
    ) -> Option<TypeId> {
        let arguments = node.type_arguments.len();
        let intended = match name {
            "String" | "Number" | "BigInt" | "Boolean" | "Void" | "Undefined" | "Null"
            | "Function" | "function" => true,
            "array" | "promise" => arguments == 0 && !self.no_implicit_any,
            "Object" => arguments != 2 && !self.no_implicit_any,
            _ => false,
        };
        if !intended {
            return None;
        }
        let mut current = self.nodes.parent(id);
        loop {
            let ancestor = current?;
            let kind = self.nodes.kind(ancestor);
            if (SyntaxKind::JSDocTypeExpression..=SyntaxKind::JSDocImportTag).contains(&kind) {
                break;
            }
            if kind == SyntaxKind::SourceFile {
                return None;
            }
            current = self.nodes.parent(ancestor);
        }
        // `checkNoTypeArguments` on the primitive arms reports TS2315 and
        // still answers the primitive; the diagnostic is not reported here.
        let intrinsics = &self.intrinsics;
        Some(match name {
            "String" => intrinsics.string,
            "Number" => intrinsics.number,
            "BigInt" => intrinsics.bigint,
            "Boolean" => intrinsics.boolean,
            "Void" => intrinsics.void,
            "Undefined" => intrinsics.undefined,
            "Null" => intrinsics.null,
            "Function" | "function" => {
                let function = self.global_type_symbol_with_arity("Function", 0)?;
                self.get_declared_type_of_symbol(function)
            }
            "array" => {
                let array = self.global_type_symbol_with_arity("Array", 1)?;
                let any = self.intrinsics.any;
                self.create_type_reference(array, vec![any])
            }
            "promise" => {
                let promise = self.global_type_symbol_with_arity("Promise", 1)?;
                let any = self.intrinsics.any;
                self.create_type_reference(promise, vec![any])
            }
            _ => self.intrinsics.any,
        })
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
                let Some(namespace) =
                    self.resolve_name_with_export_alias(root_id, root.text, SymbolFlags::NAMESPACE)
                else {
                    return self.unresolved_type_reference(node);
                };
                return self.qualified_type_reference(node, qualified, namespace);
            }
            None => return error,
        };
        let Some(id) = name.node_id else { return error };
        // `getTypeFromTypeReference` (`checker.go:23003`) asks
        // `getIntendedTypeFromJSDocTypeReference` before resolving the name.
        if let Some(intended) =
            self.get_intended_type_from_jsdoc_type_reference(node, name.text, id)
        {
            return intended;
        }
        // `SymbolFlags::TYPE` is upstream's meaning for a type reference
        // (`resolveTypeReferenceName`). It is what lets the resolver consult an
        // enclosing class's or interface's `members` for a type parameter — see
        // `BindResult::resolve_name`.
        let Some(symbol) = self.resolve_name_with_export_alias(id, name.text, SymbolFlags::TYPE)
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
                    // The park: a target whose OWN declared type is already
                    // computing above a lazily resolved construct would
                    // answer the §29 name placeholder; `error` here is the
                    // pre-§491 answer. A direct re-entry (`circular2`'s
                    // `type A = B` / `type B = A`) is native's cycle and
                    // reaches `getDeclaredTypeOfTypeAlias`'s failing push.
                    if self.resolutions.deferred_since(merged, PropertyName::DeclaredType) {
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
                    let instantiated =
                        self.get_instantiated_type_reference(node, merged, parameters);
                    return if self.is_error(instantiated) {
                        self.unresolved_type_reference(node)
                    } else {
                        instantiated
                    };
                }
                return error;
            }
            return if self
                .declaration_of_alias_symbol(symbol)
                .and_then(|declaration| {
                    self.external_module_name(self.import_or_export_declaration_of(declaration)?)
                })
                .is_some_and(|specifier| self.module_specifier_unfindable(specifier))
            {
                error
            } else {
                self.unresolved_type_reference(node)
            };
        }
        self.get_type_reference_type(node, symbol)
    }

    /// The resolved half of `Checker.getTypeReferenceType`
    /// (`checker.go:23146`) as this port has it: a non-generic symbol answers
    /// its declared type in regular form (`checkNoTypeArguments` first), a
    /// generic one goes through [`Checker::get_instantiated_type_reference`]'s
    /// arity window and default filling. Shared by the identifier arm of
    /// [`Checker::get_type_from_type_reference`] and the qualified arm
    /// ([`Checker::qualified_type_reference`]), which upstream does not
    /// distinguish once `resolveTypeReferenceName` has answered.
    fn get_type_reference_type(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        symbol: SymbolId,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let parameters = self.local_type_parameters_of(symbol).len();
        // getTypeFromTypeAliasReference (checker.go:23580) reads the declared
        // type BEFORE its arity window, and a circular alias publishes no type
        // parameters, so `type T1<in in> = T1` answers `errorType` rather than
        // a TS2314 arity failure.
        if parameters > 0
            && self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && self.get_declared_type_of_symbol(symbol) == error
        {
            return error;
        }
        if parameters == 0 {
            // `checkNoTypeArguments` (`checker.go:23157`): arguments on a type
            // that takes none is an error, and answering the bare declared type
            // would quietly drop them.
            if !node.type_arguments.is_empty() {
                return error;
            }
            let declared = self.get_declared_type_of_symbol(symbol);
            // The alias branch of native's node builder is gated by
            // `IsTypeSymbolAccessible` (`nodebuilderimpl.go:3362`). Its
            // declaration-visibility predicate admits a JSDoc typedef only
            // when its comment host is directly under a SourceFile
            // (`emitresolver.go:128-135`). This is a lexical ownership test,
            // not a comparison of file-relative spans: `typedefOnStatements`
            // keeps top-level A..Q named but expands function-local Alpha.
            if self.is_visible_jsdoc_type_alias(symbol)
                && matches!(self.store.get(declared).data, crate::types::TypeData::Named { .. })
            {
                let key = (symbol, Vec::new());
                if let Some(&cached) = self.instantiations.get(&key) {
                    return cached;
                }
                let name = self.binder.symbols().get(symbol).name.to_string();
                let members = self.alias_body_literal_symbol(symbol);
                let named = self.store.new_named(self.store.get(declared).flags, name, members);
                self.instantiations.insert(key, named);
                return named;
            }
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
    pub(crate) fn type_literal_key(&self, node: tsr_ast::NodeId) -> TypeLiteralKey {
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        TypeLiteralKey { node, bindings, mapped_template: self.mapped_template_depth > 0 }
    }

    pub(crate) fn cached_type_literal(&self, node: tsr_ast::NodeId) -> Option<TypeId> {
        self.type_literal_types.get(&self.type_literal_key(node)).copied()
    }

    fn get_type_from_type_literal(&mut self, node: &tsr_ast::TypeLiteralNode<'a>) -> TypeId {
        let Some(node_id) = node.node_id else { return self.build_type_literal(node) };
        let key = self.type_literal_key(node_id);
        if let Some(&ty) = self.type_literal_types.get(&key) {
            return ty;
        }
        // getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode installs the
        // object identity before member resolution. The current printer is
        // eager, so the reserved object carries the written recursive spelling.
        let mut single_quoted = false;
        let mut array_headed = false;
        let text = crate::signatures::written_type_literal_text(
            node,
            &mut single_quoted,
            &mut array_headed,
        )
        .unwrap_or_else(|| "{}".to_string());
        let reserved =
            self.store.new_named(TypeFlags::OBJECT, text, self.binder.symbol_of(node_id));
        self.type_literal_types.insert(key.clone(), reserved);
        let resolved = self.build_type_literal(node);
        if resolved == self.intrinsics.error {
            self.type_literal_types.insert(key, resolved);
            return resolved;
        }
        self.store.complete_object(reserved, resolved);
        self.type_literal_origins.insert(reserved, node_id);
        if let Some(properties) = self.anonymous_properties.remove(&resolved) {
            self.anonymous_properties.insert(reserved, properties);
        }
        if let Some(signatures) = self.signature_types.remove(&resolved) {
            self.signature_types.insert(reserved, signatures);
        }
        if self.alias_named_signature_types.remove(&resolved) {
            self.alias_named_signature_types.insert(reserved);
        }
        if let Some(indexes) = self.object_literal_index_infos.remove(&resolved) {
            self.object_literal_index_infos.insert(reserved, indexes);
        }
        if let Some(alias) = self.alias_of.remove(&resolved) {
            self.alias_of.insert(reserved, alias);
        }
        if let Some(source) = self.alias_body_sources.remove(&resolved) {
            self.alias_body_sources.insert(reserved, source);
        }
        reserved
    }

    /// instantiateAnonymousType / resolveAnonymousTypeMembers (checker.go).
    /// Substitute captured semantic members; source syntax supplies only method
    /// versus property rendering and index parameter names, never member types.
    pub(crate) fn instantiate_type_literal(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> Option<TypeId> {
        let origin = *self.type_literal_origins.get(&id)?;
        let mut properties = self.anonymous_properties.get(&id).map(|(p, _)| p.clone());
        let original_signatures = self.signature_types.get(&id).cloned();
        if properties.is_none() && original_signatures.is_none() {
            return None;
        }
        let cache_key = (id, map.to_vec());
        if let Some(&cached) = self.instantiated_objects.get(&cache_key) {
            return Some(cached);
        }
        let Some(Node::TypeLiteralNode(node)) = self.node_map.get(origin) else {
            return Some(self.intrinsics.error);
        };
        let owner = self.binder.symbol_of(origin)?;
        let placeholder = self.type_to_string(id);
        let reserved = self.store.new_named(TypeFlags::OBJECT, placeholder, Some(owner));
        self.instantiated_objects.insert(cache_key.clone(), reserved);
        let mut failed = false;
        for property in properties.iter_mut().flatten() {
            let current = self.property_type(property);
            let instantiated = self.instantiate_type(current, map, parameters, names);
            property.slot = crate::objects::PropertySlot::resolved(instantiated);
            if let Some(write) = &mut property.accessor_write {
                let write_type = self.parameter_type(write);
                let write_type = self.instantiate_type(write_type, map, parameters, names);
                write.set_type(write_type);
                failed |= self.parameter_type(write) == self.intrinsics.error;
            }
            failed |= instantiated == self.intrinsics.error;
            property.printed_slot =
                crate::objects::PrintedSlot::printed(self.type_to_string(instantiated));
        }
        let signatures: Option<Vec<_>> = original_signatures
            .unwrap_or_default()
            .into_iter()
            .map(|signature| {
                self.instantiate_signature_with_fresh_parameters(signature, map, parameters, names)
            })
            .collect();
        let Some(signatures) = signatures else {
            self.instantiated_objects.insert(cache_key, self.intrinsics.error);
            return Some(self.intrinsics.error);
        };
        let mut indexes = self.object_literal_index_infos.get(&id).cloned().unwrap_or_default();
        for index in &mut indexes {
            index.key = self.instantiate_type(index.key, map, parameters, names);
            index.value = self.instantiate_type(index.value, map, parameters, names);
            failed |= index.key == self.intrinsics.error || index.value == self.intrinsics.error;
        }
        if failed {
            self.instantiated_objects.insert(cache_key, self.intrinsics.error);
            return Some(self.intrinsics.error);
        }
        let mut members: Vec<_> = signatures
            .iter()
            .map(|signature| crate::objects::Member::Signature {
                printed: crate::objects::signature_member_text(self, signature),
            })
            .collect();
        let index_names: Vec<_> = node
            .members
            .iter()
            .filter_map(|member| {
                let tsr_ast::TypeElement::IndexSignatureDeclaration(index) = member else {
                    return None;
                };
                let Some(tsr_ast::BindingName::Identifier(name)) = index.parameters.first()?.name
                else {
                    return None;
                };
                Some(name.text)
            })
            .collect();
        for (position, index) in indexes.iter().enumerate() {
            members.push(crate::objects::Member::Index {
                readonly: index.readonly,
                name: index_names
                    .get(position)
                    .or_else(|| index_names.first())
                    .copied()
                    .unwrap_or("x")
                    .to_string(),
                key: self.type_to_string(index.key),
                value: self.type_to_string(index.value),
            });
        }
        for property in properties.iter().flatten() {
            let is_method = node.members.iter().any(|member| {
                let tsr_ast::TypeElement::MethodSignatureDeclaration(method) = member else {
                    return false;
                };
                let Some(symbol) = method.node_id.and_then(|id| self.binder.symbol_of(id)) else {
                    return false;
                };
                let printed = if let tsr_ast::PropertyName::ComputedPropertyName(computed) =
                    method.name
                {
                    let Some((printed, _)) = self.late_bound_symbol_member_name(computed) else {
                        return false;
                    };
                    printed
                } else {
                    self.binder.symbols().get(symbol).name.to_string()
                };
                self.type_literal_member_key(method.name, symbol, &printed) == property.name
            });
            let property_type = self.property_type(property);
            if is_method && let Some(overloads) = self.signature_types.get(&property_type).cloned()
            {
                for signature in overloads {
                    members.push(crate::objects::Member::Signature {
                        printed: format!(
                            "{}{}{}",
                            property.printed_name,
                            if property.optional { "?" } else { "" },
                            crate::objects::signature_member_text(self, &signature)
                        ),
                    });
                }
            } else {
                members.push(crate::objects::Member::Property {
                    name: property.printed_name.clone(),
                    optional: property.optional,
                    readonly: property.readonly,
                    printed: self.property_printed_type(property).into_owned(),
                });
            }
        }
        let single = matches!(
            node.members,
            [tsr_ast::TypeElement::CallSignatureDeclaration(_)
                | tsr_ast::TypeElement::ConstructSignatureDeclaration(_)]
        );
        let resolved = if single && signatures.len() == 1 {
            let text = self.signature_to_string(&signatures[0]);
            self.store.new_anonymous(TypeFlags::OBJECT, text, owner, true)
        } else {
            let text = crate::objects::render_object_type(&members);
            self.store.new_named(TypeFlags::OBJECT, text, Some(owner))
        };
        self.store.complete_object(reserved, resolved);
        self.type_literal_origins.insert(reserved, origin);
        if let Some(properties) = properties {
            self.anonymous_properties.insert(reserved, (properties, true));
        }
        if !signatures.is_empty() {
            self.signature_types.insert(reserved, signatures);
            if !single {
                self.alias_named_signature_types.insert(reserved);
            }
        }
        if !indexes.is_empty() {
            self.object_literal_index_infos.insert(reserved, indexes);
        }
        self.instantiate_alias_metadata(id, reserved, map, parameters, names);
        Some(reserved)
    }

    /// getPropertyNameFromType (checker.go): semantic lookup keys omit the
    /// display quotes used for literal names; symbol chains retain their
    /// existing bracketed lookup representation in this port.
    pub(crate) fn type_literal_member_key(
        &mut self,
        name: tsr_ast::PropertyName<'_>,
        symbol: SymbolId,
        printed: &str,
    ) -> String {
        if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name {
            let key = computed.expression.map(|expression| self.check_expression(expression));
            match key.map(|id| &self.store.get(id).data) {
                Some(
                    crate::types::TypeData::StringLiteral(value)
                    | crate::types::TypeData::NumberLiteral(value)
                    | crate::types::TypeData::EnumLiteral {
                        value:
                            crate::types::EnumLiteralValue::String(value)
                            | crate::types::EnumLiteralValue::Number(value),
                        ..
                    },
                ) => value.clone(),
                _ => printed.to_string(),
            }
        } else {
            self.binder.symbols().get(symbol).name.to_string()
        }
    }

    fn build_type_literal(&mut self, node: &tsr_ast::TypeLiteralNode<'a>) -> TypeId {
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
            // `typeToTypeNodeHelper`'s alias arm (`nodebuilderimpl.go`) runs
            // before the anonymous-object collapse: a literal that is a type
            // alias's body prints the alias name, exactly as the multi-member
            // path below does through the same `getAliasForTypeNode` lookup.
            let alias = node.node_id.and_then(|id| self.alias_symbol_for_type_node(id));
            let text = match alias {
                None => self.signature_to_string(&signature),
                Some(alias) if self.local_type_parameters_of(alias).is_empty() => {
                    self.binder.symbols().get(alias).name.to_string()
                }
                Some(alias) => {
                    let parameters = self.local_type_parameter_types_of(alias)
                        .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect::<Vec<_>>()).unwrap_or_default();
                    self.type_reference_text(alias, &parameters)
                }
            };
            let Some(symbol) = node.node_id.and_then(|id| self.binder.symbol_of(id)) else {
                return error;
            };
            let built = self.store.new_anonymous(TypeFlags::OBJECT, text, symbol, alias.is_none());
            self.signature_types.insert(built, vec![signature]);
            if let Some(alias) = alias
                && !self.local_type_parameters_of(alias).is_empty() {
                let arguments = self.local_type_parameter_types_of(alias)
                    .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                self.alias_of.insert(built, (alias, arguments));
            }
            if alias.is_some() {
                // The alias name is the print; the site re-render that
                // collapses the signature applies to an unaliased literal.
                self.alias_named_signature_types.insert(built);
            }
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
        let mut construct_signatures = Vec::new();
        let mut indexes = Vec::new();
        let mut properties = Vec::with_capacity(node.members.len());
        let mut typed_properties: Vec<crate::objects::AnonymousProperty> =
            Vec::with_capacity(node.members.len());
        let mut typed_signatures = Vec::new();
        let mut typed_construct_signatures = Vec::new();
        let mut typed_indexes = Vec::new();
        let mut seen_index_keys: Vec<TypeId> = Vec::new();
        // A merged duplicate member (below) changes the type's structure, but
        // a signature that WROTE this literal still reuses the written node
        // (`x is { a: string; a: string; }`,
        // `checkTypePredicateForRedundantProperties`).
        let mut merged_duplicate = false;
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
                        // `classifyPropertyName` (`nodebuilderimpl.go:2384`):
                        // a METHOD named `new` prints as a string literal, or
                        // it would read back as a construct signature
                        // (`{ "new"?(): any; }`, `parser645484`).
                        tsr_ast::PropertyName::Identifier(name) if name.text == "new" => {
                            "\"new\"".to_string()
                        }
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
                let construct = signature.kind == crate::signatures::SignatureKind::Construct;
                let name = name.unwrap_or_default();
                if is_property {
                    let Some(symbol) = self.binder.symbol_of(id) else { return error };
                    let optional = name.ends_with('?');
                    let printed_name = name.trim_end_matches('?').to_string();
                    let key = match self.node_map.get(id) {
                        Some(Node::MethodSignatureDeclaration(method)) => {
                            self.type_literal_member_key(method.name, symbol, &printed_name)
                        }
                        _ => return error,
                    };
                    let existing =
                        typed_properties.iter().position(|property| property.name == key);
                    let mut overloads = existing
                        .map(|index| self.property_type(&typed_properties[index]))
                        .and_then(|property_type| self.signature_types.get(&property_type))
                        .cloned()
                        .unwrap_or_default();
                    overloads.push(signature);
                    let printed_type = if let [signature] = overloads.as_slice() {
                        self.signature_to_string(signature)
                    } else {
                        let members: Vec<_> = overloads
                            .iter()
                            .map(|signature| crate::objects::Member::Signature {
                                printed: crate::objects::signature_member_text(self, signature),
                            })
                            .collect();
                        crate::objects::render_object_type(&members)
                    };
                    let method_type = self.store.new_anonymous(
                        TypeFlags::OBJECT,
                        printed_type.clone(),
                        symbol,
                        overloads.len() == 1,
                    );
                    self.signature_types.insert(method_type, overloads);
                    let property = crate::objects::AnonymousProperty {
                        accessor_write: None,
                        method: true,
                        origin: Some(symbol),
                        checked_declaration: None,
                        name: key,
                        printed_name,
                        printed_slot: crate::objects::PrintedSlot::printed(printed_type),
                        optional,
                        readonly: false,
                        slot: crate::objects::PropertySlot::resolved(method_type),
                    };
                    if let Some(index) = existing {
                        typed_properties[index] = property;
                    } else {
                        typed_properties.push(property);
                    }
                } else if construct {
                    typed_construct_signatures.push(signature);
                } else {
                    typed_signatures.push(signature);
                }
                let bucket = if is_property { &mut properties }
                    else if construct { &mut construct_signatures } else { &mut signatures };
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
                // getIndexInfosOfIndexSymbol (checker.go:19634): an info is
                // added for a key type only when `findIndexInfo` finds none
                // yet (:19655), so `{ [x: number]: string; [x: number]: string }`
                // has ONE info and prints one row. `index_signature_member`
                // declined a union key above, so the key is one type here and
                // the first declaration for it wins.
                if let [parameter] = index.parameters
                    && let Some(key_node) = parameter.r#type
                {
                    let key = self.get_type_from_type_node(key_node);
                    if seen_index_keys.contains(&key) {
                        merged_duplicate = true;
                        continue;
                    }
                    seen_index_keys.push(key);
                }
                typed_indexes.extend(self.index_infos_of_declaration(index));
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
                typed_properties.push(crate::objects::AnonymousProperty {
                    accessor_write: None,
                    method: false,
                    origin: member.node_id().and_then(|id| self.binder.symbol_of(id)),
                    checked_declaration: None,
                    name: accessor_name.text.to_string(),
                    printed_name: accessor_name.text.to_string(),
                    printed_slot: crate::objects::PrintedSlot::printed(printed.clone()),
                    optional: false,
                    readonly: is_getter && !paired,
                    slot: crate::objects::PropertySlot::resolved(member_type),
                });
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
            // the written annotation node, exactly as it does for a parameter
            // (`pseudoTypeEquivalentToType`'s error charity).
            let member_type = match property.r#type {
                Some(annotation) => self.get_type_from_type_node(annotation),
                None => self.intrinsics.any,
            };
            // `?` on a property signature; `!` cannot appear on one, so the
            // token\'s presence is enough to distinguish it.
            let optional =
                property.postfix_token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken);
            let readonly = property.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(m) if m.kind == SyntaxKind::ReadonlyKeyword)
            });
            // `addPropertyToElementList` (`nodebuilderimpl.go:2486`) prints
            // the member through `serializeTypeForDeclaration`, whose reuse
            // arm keeps the WRITTEN annotation node when it is equivalent to
            // the member's type (`crate::node_reuse`).
            let reused = property
                .r#type
                .and_then(|annotation| self.reused_annotation_text(annotation, member_type));
            let (member_type, printed) = if member_type == error {
                // No reusable node: still a whole-literal decline, because
                // inventing a spelling would be worse. Otherwise this port's
                // stand-in for `errorType` at printing positions is `any`.
                let Some(reused) = reused else { return error };
                (self.intrinsics.any, reused)
            } else {
                let printed = reused.unwrap_or_else(|| self.type_to_string(member_type));
                (member_type, printed)
            };
            if let Some(symbol) = property.node_id.and_then(|id| self.binder.symbol_of(id)) {
                // declareSymbolEx (binder.go:152): `PropertyExcludes` is
                // empty, so same-named property signatures of one literal
                // MERGE into one member symbol, and resolveAnonymousTypeMembers
                // (via getMembersOfSymbol, checker.go:16124) yields one
                // property typed from its first (value) declaration:
                // `{ a: string; a: string; }` prints `{ a: string; }`.
                if typed_properties.iter().any(|existing| existing.origin == Some(symbol)) {
                    merged_duplicate = true;
                    continue;
                }
                typed_properties.push(crate::objects::AnonymousProperty {
                    accessor_write: None,
                    method: false,
                    origin: Some(symbol),
                    checked_declaration: None,
                    name: self.type_literal_member_key(property.name, symbol, &name),
                    printed_name: name.clone(),
                    printed_slot: crate::objects::PrintedSlot::printed(printed.clone()),
                    optional,
                    readonly,
                    slot: crate::objects::PropertySlot::resolved(member_type),
                });
            }
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
            let key_type = match key {
                "string" => self.intrinsics.string,
                "number" => self.intrinsics.number,
                "symbol" => self.intrinsics.es_symbol,
                _ => return error,
            };
            typed_indexes.push(crate::index_signatures::IndexInfo {
                components: None,
                declaration: None,
                key: key_type,
                value,
                readonly: false,
            });
            indexes.push(crate::objects::Member::Index {
                readonly: false,
                name: "x".to_string(),
                key: key.to_string(),
                value: self.type_to_string(value),
            });
        }
        if merged_duplicate
            && let Some(id) = node.node_id
            && let Some(text) =
                crate::signatures::written_type_literal_text(node, &mut false, &mut false)
        {
            self.qualified_written_text.entry(id).or_insert(text);
        }
        // Native object serialization groups call, construct, index, property;
        // source order remains unchanged inside each signature group.
        signatures.append(&mut construct_signatures);
        typed_signatures.append(&mut typed_construct_signatures);
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
            Some(_) => crate::objects::render_object_type(&members),
        };
        // The binder gives a type literal its own anonymous `__type` symbol,
        // whose members table is where a property access on this type looks.
        let owner = node.node_id.and_then(|id| self.binder.symbol_of(id));
        let structural = printed.starts_with('{');
        let minted = self.store.new_named(TypeFlags::OBJECT, printed, owner);
        if structural {
            self.anonymous_properties.insert(
                minted,
                (
                    typed_properties,
                    self.mapped_template_depth > 0 || !self.alias_evaluation_bindings.is_empty(),
                ),
            );
            if !typed_signatures.is_empty() {
                self.signature_types.insert(minted, typed_signatures);
                // The literal renderer preserves method and index declarations.
                // Callable-expando rendering only has property syntax.
                self.alias_named_signature_types.insert(minted);
            }
            if !typed_indexes.is_empty() {
                self.object_literal_index_infos.insert(minted, typed_indexes);
            }
        }
        if let Some(alias) = node.node_id.and_then(|node| self.alias_symbol_for_type_node(node))
            && !self.local_type_parameters_of(alias).is_empty() {
            let arguments = self.local_type_parameter_types_of(alias)
                .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
            return self.alias_object_image(minted, alias, arguments);
        }
        minted
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
        // Native 5b1047d checker.go:24218 compares emptyTypeLiteralType,
        // not arbitrary empty objects. Its producer (checker.go:22933) uses
        // that identity only for an empty literal without its own alias.
        // Read the completed TypeId's original literal/host; no member forcing
        // or new cache. Generated, named and active literal images do not qualify.
        let mut no_supertype_reduction = types.len() == 2
            && types.iter().enumerate().any(|(index, &id)| {
                if !self.is_unaliased_empty_type_literal(id) {
                    return false;
                }
                let other = types[1 - index];
                let flags = self.store.get(other).flags;
                flags.intersects(TypeFlags::STRING | TypeFlags::NUMBER | TypeFlags::BIG_INT)
                    || flags.contains(TypeFlags::TEMPLATE_LITERAL)
                        && self.is_pattern_template(other)
            });
        if no_supertype_reduction && !self.alias_evaluation_bindings.is_empty() {
            // instantiateTypeWorker reconstructs intersections when the owning
            // alias arguments change, even for Brand<T> = number & {}
            // (native checker.go:22104,22244). Do not reselect a source flag
            // from the substituted operands of NonNullable<T> = T & {}.
            let reconstructed_alias = node.node_id
                .and_then(|id| self.type_alias_host_for_type_node(id))
                .and_then(|id| self.node_map.get(id))
                .is_some_and(|host| matches!(host, Node::TypeAliasDeclaration(alias)
                    if alias.type_parameters.iter().any(|parameter| {
                        let Some(symbol) = parameter.node_id.and_then(|id| self.binder.symbol_of(id)) else { return false };
                        self.alias_evaluation_bindings.iter().rev()
                            .find_map(|frame| frame.get(&symbol))
                            .is_some_and(|value| self.declared_types.get(&symbol) != Some(value))
                    })));
            if reconstructed_alias {
                no_supertype_reduction = false;
            } else {
                // For nested source annotations under unrelated frames, prove
                // both operands retained their original flags. This bounded
                // AST/binder read neither forces original types nor publishes a
                // second template/cache. Unsupported grammar is not a false
                // eligibility proof; retain the alias evaluator's refusal.
                for &operand in node.types {
                    match self.source_operand_is_closed(operand, &[], &[], 32) {
                        Some(true) => {}
                        Some(false) => {
                            no_supertype_reduction = false;
                            break;
                        }
                        None => return error,
                    }
                }
            }
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
            return self.get_intersection_type_with_reduction(
                &types,
                None,
                true,
                !no_supertype_reduction,
            );
        }
        match node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)) {
            None => self.get_intersection_type_with_reduction(
                &types,
                None,
                true,
                !no_supertype_reduction,
            ),
            Some(alias) if self.local_type_parameters_of(alias).is_empty() => self
                .get_intersection_type_with_reduction(
                    &types,
                    Some(alias),
                    true,
                    !no_supertype_reduction,
                ),
            Some(_) => error,
        }
    }

    /// emptyTypeLiteralType's completed source identity (checker.go:22933).
    pub(crate) fn is_unaliased_empty_type_literal(&self, id: TypeId) -> bool {
        self.type_literal_origins.get(&id).is_some_and(|&origin| {
            matches!(self.node_map.get(origin), Some(Node::TypeLiteralNode(literal))
                if literal.members.is_empty())
                && self.type_alias_host_for_type_node(origin).is_none()
        })
    }

    /// Source flag certificate for getTypeFromIntersectionTypeNode
    /// (native 5b1047d, checker.go:24218), before alias instantiation.
    /// Some(false) proves a retained parameter or non-pattern generic template;
    /// None is unsupported/recursive normalization, not completed absence.
    /// Checker-local AST/binder symbols and ordered written arguments identify
    /// alias projections, independently of current concrete frames. Read-only:
    /// no getters, cache, publication or mapper image. Traversal is bounded by
    /// path depth and rejects alias cycles; its cost is not a measured speed win.
    fn source_operand_is_closed(
        &self,
        node: TypeNode<'a>,
        parameters: &[(SymbolId, Option<bool>)],
        aliases: &[SymbolId],
        remaining: u8,
    ) -> Option<bool> {
        let remaining = remaining.checked_sub(1)?;
        let recurse = |node| self.source_operand_is_closed(node, parameters, aliases, remaining);
        match node {
            TypeNode::KeywordTypeNode(_) | TypeNode::LiteralTypeNode(_) => Some(true),
            TypeNode::TypeLiteralNode(literal) if literal.members.is_empty() => Some(true),
            TypeNode::ParenthesizedTypeNode(node) => recurse(node.r#type?),
            TypeNode::TemplateLiteralTypeNode(template) => {
                let mut closed = true;
                for span in template.template_spans {
                    closed &= recurse(span.r#type?)?;
                }
                Some(closed)
            }
            TypeNode::UnionTypeNode(union) => {
                let mut closed = true;
                for &node in union.types {
                    closed &= recurse(node)?;
                }
                // A parameter dependency alone does not determine normalized
                // flags: string | `west-${T}` still reduces to string before
                // instantiation. Do not call that proved ineligible.
                closed.then_some(true)
            }
            TypeNode::IntersectionTypeNode(intersection) => {
                let mut closed = true;
                for &node in intersection.types {
                    closed &= recurse(node)?;
                }
                closed.then_some(true)
            }
            TypeNode::TypeReferenceNode(reference) => {
                let tsr_ast::EntityName::Identifier(name) = reference.type_name? else {
                    return None;
                };
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    name.node_id?,
                    name.text,
                    SymbolFlags::TYPE,
                )?;
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER) {
                    return parameters
                        .iter()
                        .rev()
                        .find(|(key, _)| *key == symbol)
                        .map_or(Some(false), |(_, closed)| *closed);
                }
                if aliases.contains(&symbol) {
                    return None;
                }
                let [declaration] = self.binder.symbols().get(symbol).declarations.as_slice()
                else {
                    return None;
                };
                let Node::TypeAliasDeclaration(alias) = self.node_map.get(*declaration)? else {
                    return None;
                };
                if reference.type_arguments.len() != alias.type_parameters.len() {
                    return None;
                }
                let mut projected = parameters.to_vec();
                for (parameter, &argument) in
                    alias.type_parameters.iter().zip(reference.type_arguments)
                {
                    let symbol = parameter.node_id.and_then(|id| self.binder.symbol_of(id))?;
                    projected.push((symbol, recurse(argument)));
                }
                let mut aliases = aliases.to_vec();
                aliases.push(symbol);
                self.source_operand_is_closed(alias.r#type?, &projected, &aliases, remaining)
            }
            _ => None,
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
        let readonly = node
            .node_id
            .and_then(|id| self.nodes.parent(id))
            .is_some_and(|parent| self.is_readonly_type_operator(parent));
        let target = if readonly { "ReadonlyArray" } else { "Array" };
        let Some(target) = self.global_type_symbol(target) else { return error };
        let reference = self.create_type_reference(target, vec![element]);
        // getTypeFromArrayOrTupleTypeNode (checker.go:24115): an array node
        // that IS an alias body takes the deferred arm and carries the alias
        // (`type T10 = string[]` records `>T10 : T10`).
        self.deferred_alias_reference(node.node_id, reference)
    }

    /// The reference an alias-carrying deferred copy was made from (the
    /// canonical `(target, arguments)` instantiation), or `id` itself when it
    /// carries no alias. For consumers that build a NEW type from the
    /// reference's structure, where upstream's new type has no alias.
    pub(crate) fn without_alias(&self, id: TypeId) -> TypeId {
        if let Some(&source) = self.alias_body_sources.get(&id) { return source; }
        if !self.alias_of.contains_key(&id) {
            return id;
        }
        self.type_reference_targets
            .get(&id)
            .and_then(|key| self.instantiations.get(key))
            .copied()
            .unwrap_or(id)
    }

    /// `createDeferredTypeReference` (`checker.go:25121`) reached through
    /// `isDeferredTypeReferenceNode`'s alias arm (`:23237`). Alias bodies,
    /// including generic bodies, retain the enclosing symbol and ordered own
    /// parameters without changing the canonical reference. Member, iteration
    /// and relation queries retain the reference target and receiver context.
    fn deferred_alias_reference(&mut self, node: Option<NodeId>, reference: TypeId) -> TypeId {
        let Some(node) = node else { return reference };
        let Some(alias) = self.alias_symbol_for_type_node(node) else { return reference };
        if self.is_error(reference) || !self.type_reference_targets.contains_key(&reference) {
            return reference;
        }
        let arguments = self.local_type_parameter_types_of(alias)
            .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
        self.alias_object_image(reference, alias, arguments)
    }
    /// instantiateAnonymousType with an explicit enclosing alias (5b1047d).
    /// Alias identity and ordered arguments are part of the existing image key;
    /// never retag a cached alias-free body. Copy only completed semantic views.
    pub(crate) fn alias_object_image(&mut self, source: TypeId, alias: SymbolId, arguments: Vec<TypeId>) -> TypeId {
        if !self.store.get(source).flags.intersects(TypeFlags::OBJECT | TypeFlags::INDEXED_ACCESS | TypeFlags::INTERSECTION | TypeFlags::CONDITIONAL) || self.is_error(source) {
            return source;
        }
        let key = (alias, arguments.clone(), source);
        if let Some(&image) = self.deferred_alias_references.get(&key) { return image; }
        let ty = self.store.get(source).clone();
        let text = if arguments.is_empty() { self.binder.symbols().get(alias).name.to_owned() }
            else { self.type_reference_text(alias, &arguments) };
        let image = match ty.data {
            crate::types::TypeData::Named { members, .. } => self.store.new_named(ty.flags, text, members),
            crate::types::TypeData::Anonymous { symbol, .. } => self.store.new_anonymous(ty.flags, text, symbol, false),
            _ => return source,
        };
        if let Some(operands) = self.deferred_indexed_access_types.get(&source).copied() {
            self.deferred_indexed_access_types.insert(image, operands);
            self.deferred_index_mints.insert(image);
        }
        if let Some(signatures) = self.signature_types.get(&source).cloned() {
            self.signature_types.insert(image, signatures);
            self.alias_named_signature_types.insert(image);
        }
        if let Some(mapper) = self.instantiated_signature_mappers.get(&source).cloned() {
            self.instantiated_signature_mappers.insert(image, mapper);
        }
        if let Some(tuple) = self.tuple_element_lists.get(&source).cloned() {
            self.tuple_element_lists.insert(image, tuple);
        }
        if let Some(mask) = self.tuple_optional_masks.get(&source).cloned() {
            self.tuple_optional_masks.insert(image, mask);
        }
        if let Some(labels) = self.tuple_labels.get(&source).cloned() {
            self.tuple_labels.insert(image, labels);
        }
        if let Some(elements) = self.variadic_tuple_elements.get(&source).cloned() {
            self.variadic_tuple_elements.insert(image, elements);
        }
        if let Some(properties) = self.anonymous_properties.get(&source).cloned() {
            self.anonymous_properties.insert(image, properties);
        }
        if let Some(indexes) = self.object_literal_index_infos.get(&source).cloned() {
            self.object_literal_index_infos.insert(image, indexes);
        }
        if let Some(members) = self.object_literal_members.get(&source).cloned() {
            self.object_literal_members.insert(image, members);
        }
        if let Some(&origin) = self.type_literal_origins.get(&source) {
            self.type_literal_origins.insert(image, origin);
        }
        if let Some(&supplier) = self.instantiation_expression_sources.get(&source) {
            self.instantiation_expression_sources.insert(image, supplier);
            if let Some(&node) = self.instantiation_expression_nodes.get(&source) {
                self.instantiation_expression_nodes.insert(image, node);
            }
        }
        if let Some(reference) = self.type_reference_targets.get(&source).cloned() {
            self.type_reference_targets.insert(image, reference);
        }
        if let Some(&receiver) = self.type_reference_this_arguments.get(&source) {
            self.type_reference_this_arguments.insert(image, receiver);
        }
        if let Some(info) = self.mapped_types.get(&source).cloned() {
            self.mapped_types.insert(image, info);
        }
        if let Some(&modifiers) = self.mapped_identity_optionality.get(&source) {
            self.mapped_identity_optionality.insert(image, modifiers);
        }
        if let Some(&supplier) = self.mapped_identity_sources.get(&source) {
            self.mapped_identity_sources.insert(image, supplier);
        }
        self.alias_of.insert(image, (alias, arguments));
        self.alias_body_sources.insert(image, source);
        self.deferred_alias_references.insert(key, image);
        image
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
        if let Some(alias) = node.node_id.and_then(|id| self.alias_symbol_for_type_node(id))
            && !self.local_type_parameters_of(alias).is_empty()
            && !node.elements.iter().any(|element| matches!(element, TypeNode::RestTypeNode(_))) {
            let structural = self.tuple_type_node_structural(node);
            let arguments = self.local_type_parameter_types_of(alias)
                .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
            return self.alias_object_image(structural, alias, arguments);
        }
        // §79.1: `getAliasForTypeNode`'s three arms, the §72 rule at the
        // tuple mint — `type T2 = [number, string, boolean?]` prints `T2`
        // (`optionalTupleElements1` priced this at 99 G→W without it: the
        // §79 structural prints replaced prior `error` gaps at every
        // alias-wanting position). The named copy carries the element list
        // and optional mask so contexts and index reads work through it.
        if let Some(alias) = node.node_id.and_then(|id| self.alias_symbol_for_type_node(id))
            && self.local_type_parameters_of(alias).is_empty()
            && !node.elements.is_empty()
        // §956: the REST exclusion is gone. §40 added it because a
        // rest-bearing body did not RESOLVE — the arm minted a name over an
        // `any` body and cost 13 `RIGHT->WRONG` on `excessivelyLargeTupleSpread`.
        // §956 makes those bodies resolve, so the premise is gone, and
        // upstream plainly names them: `>WithOptAndRest : WithOptAndRest`
        // for `[first: number, second?: number, ...rest: string[]]`
        // (`namedTupleMembers`). The `structural == error` check below is what
        // still protects the old case — a body that cannot resolve declines
        // here exactly as before.
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
            // §956: a rest-bearing body is named ONLY when it stayed a PRINT-ONLY
            // variadic. §40's 13 `RIGHT->WRONG` on `excessivelyLargeTupleSpread`
            // reproduce exactly when this is left out, which is what established
            // the distinction: a rest over a CONCRETE tuple SPLICES into a real
            // element list, and that list is what access, instantiation and the
            // relater consume — putting a bare name over it loses them. A body
            // that stayed a spelling has no such consumers, so the name is free.
            //
            // `variadic_tuple_nodes` is the print-only mint's own registration
            // (§791), so the test is "did the body remain a spelling" rather than
            // a syntactic re-derivation of the same question.
            // A rest whose operand is written as a NAMED REFERENCE also loses the
            // alias, and that was derived from the oracle rather than guessed —
            // 42 non-generic rest-bearing tuple aliases across the corpus
            // baselines split cleanly:
            //
            // ```
            // type T06 = [string, ...string[]]          >T06 : T06                    NAME
            // type NonEmptyStringArray =
            //            [string, ...Array<string>]     >… : [string, ...string[]]     STRUCT
            // type Unbounded = [...Numbers, boolean]    >… : [...number[], boolean]    STRUCT
            // type T04 = [...[...string[]]]             >T04 : T04                    NAME
            // ```
            //
            // The axis is whether NORMALISATION REWROTE anything. `...string[]` is
            // already normal, so the tuple upstream creates carries the alias;
            // `...Array<string>` and `...Numbers` normalise to `...string[]` and
            // `...number[]`, which creates a DIFFERENT tuple and the alias is not
            // on it. A rest over a tuple LITERAL that splices is the same story and
            // the `variadic_tuple_nodes` test above already catches it
            // (`MixedSpread`), while `[...[...string[]]]` splices to itself and
            // keeps the name.
            let rest_over_a_reference = node.elements.iter().any(|element| {
                let TypeNode::RestTypeNode(rest) = element else { return false };
                match rest.r#type {
                    Some(TypeNode::TypeReferenceNode(_)) => true,
                    Some(TypeNode::NamedTupleMember(member)) => {
                        matches!(member.r#type, Some(TypeNode::TypeReferenceNode(_)))
                    }
                    _ => false,
                }
            });
            // §959 corrects §956's proxy here. It read
            // `!variadic_tuple_nodes.contains_key(&structural)` — "the body did not
            // stay a print-only spelling" — which was right while a rest-bearing body
            // had only TWO outcomes, a spelling or a spliced tuple. The reduction
            // above adds a THIRD, an ARRAY, and the old test sent
            // `type T03 = [...string[]]` and `type V15 = [...string[], ...number[]]`
            // to the structure where upstream names both.
            //
            // The question was always *"did normalisation produce a positional TUPLE
            // this name would hide"*, so ask that directly: a spliced tuple registers
            // in `tuple_element_lists` and neither a spelling nor an array does.
            if node.elements.iter().any(|e| matches!(e, TypeNode::RestTypeNode(_)))
                && (self.tuple_element_lists.contains_key(&structural) || rest_over_a_reference)
            {
                return structural;
            }
            let name = self.binder.symbols().get(alias).name.to_string();
            let named = self.store.new_named(TypeFlags::OBJECT, name, None);
            if let Some(entry) = self.tuple_element_lists.get(&structural).cloned() {
                self.tuple_element_lists.insert(named, entry);
            }
            if let Some(mask) = self.tuple_optional_masks.get(&structural).cloned() {
                self.tuple_optional_masks.insert(named, mask);
            }
            if let Some(labels) = self.tuple_labels.get(&structural).cloned() {
                self.tuple_labels.insert(named, labels);
            }
            if let Some(elements) = self.variadic_tuple_elements.get(&structural).cloned() {
                self.variadic_tuple_elements.insert(named, elements);
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
        //
        // **§956 removes the second half of that gate.** §40 declined the whole
        // node when a rest element sat beside an OPTIONAL or NAMED one, so
        // `[...T, number?]` was `errorType` and with it every signature holding
        // one — `<T extends unknown[]>(t1: [...T], t2: [...T, number?]) => T`
        // printed `any` in `variadicTuples1`. Nothing about those two element
        // kinds needs the element LIST: this road composes TEXT, and `number?`
        // and `label: T` are spellings, not structures. The gate was excluding
        // them because the road BELOW it — the real element-list mint — cannot
        // represent a rest, not because this one cannot print them.
        //
        // A LABELLED rest is `RestTypeNode(NamedTupleMember(..))`, NOT a
        // `NamedTupleMember` carrying `...`: `parse_tuple_element`
        // (`tsr-parser/src/types.rs:713`) consumes the `...` first and RECURSES,
        // and it constructs every member with `NamedTupleMember::new(None, ..)`
        // — so **`dot_dot_dot_token` is never set by this parser at all**. A
        // first draft tested that field and was dead code; the nesting is
        // unwrapped in the loop instead.
        if node.elements.iter().any(|element| matches!(element, TypeNode::RestTypeNode(_))) {
            let mut pieces = Vec::with_capacity(node.elements.len());
            let mut resolved_elements = Vec::with_capacity(node.elements.len());
            let mut spliced: Option<Vec<(TypeId, bool, Option<String>)>> = Some(Vec::new());
            for element in node.elements {
                let (prefix, suffix, inner) = match element {
                    // §956: `[a: string, b?: number, ...c: T]` — the label, the
                    // `?`, and a labelled REST all print as written. Upstream
                    // reuses the node, so the spelling is the answer.
                    TypeNode::RestTypeNode(rest) => match rest.r#type {
                        // The labelled rest, unwrapped one level.
                        Some(TypeNode::NamedTupleMember(member)) => {
                            let (Some(inner), Some(name)) = (member.r#type, member.name) else {
                                return error;
                            };
                            let question = if member.question_token.is_some() { "?" } else { "" };
                            (format!("...{}{question}: ", name.text), String::new(), inner)
                        }
                        Some(inner) => ("...".to_string(), String::new(), inner),
                        None => return error,
                    },
                    TypeNode::NamedTupleMember(member) => {
                        let (Some(inner), Some(name)) = (member.r#type, member.name) else {
                            return error;
                        };
                        let question = if member.question_token.is_some() { "?" } else { "" };
                        (format!("{}{question}: ", name.text), String::new(), inner)
                    }
                    TypeNode::OptionalTypeNode(optional) => {
                        let Some(inner) = optional.r#type else { return error };
                        (String::new(), "?".to_string(), inner)
                    }
                    other => (String::new(), String::new(), *other),
                };
                let resolved = self.get_type_from_type_node(inner);
                if resolved == error {
                    return error;
                }
                let member = match element {
                    TypeNode::NamedTupleMember(member) => Some(*member),
                    TypeNode::RestTypeNode(rest) => match rest.r#type {
                        Some(TypeNode::NamedTupleMember(member)) => Some(member),
                        _ => None,
                    },
                    _ => None,
                };
                resolved_elements.push(crate::tuples::TupleElement {
                    r#type: resolved,
                    spread: matches!(element, TypeNode::RestTypeNode(_)),
                    optional: matches!(element, TypeNode::OptionalTypeNode(_))
                        || member.is_some_and(|member| member.question_token.is_some()),
                    label: member.and_then(|member| member.name).map(|name| name.text.to_string()),
                });
                // A rest over a CONCRETE tuple splices — upstream expands it
                // flat (`excessivelyLargeTupleSpread`, the §40 falsifier's
                // population); any other rest keeps the whole print-only.
                if let Some(flat) = spliced.as_mut() {
                    if !matches!(element, TypeNode::RestTypeNode(_)) {
                        let (optional, label) = match element {
                            TypeNode::NamedTupleMember(member) => (
                                member.question_token.is_some(),
                                member.name.map(|name| name.text.to_string()),
                            ),
                            TypeNode::OptionalTypeNode(_) => (true, None),
                            _ => (false, None),
                        };
                        flat.push((resolved, optional, label));
                    } else if let Some((inner_elements, _)) =
                        self.tuple_element_lists.get(&resolved).cloned()
                    {
                        // createNormalizedTupleType (checker.go) rejects a
                        // concrete spread before expanding to 10,000 elements.
                        // The syntax path must share the semantic normalizer's
                        // bound, including when a default enables recursion.
                        if flat.len() + inner_elements.len() >= 10_000 {
                            return error;
                        }
                        let mask = self.tuple_optional_masks.get(&resolved);
                        let labels = self.tuple_labels.get(&resolved);
                        flat.extend(inner_elements.into_iter().enumerate().map(|(index, t)| {
                            (
                                t,
                                mask.and_then(|m| m.get(index)).copied().unwrap_or(false),
                                labels.and_then(|l| l.get(index)).cloned().flatten(),
                            )
                        }));
                    } else {
                        spliced = None;
                    }
                }
                // createNormalizedTupleType stores a rest over an array as its
                // ELEMENT type and the node builder prints `...E[]` from it — a
                // fresh array, so an alias the operand carried does not survive
                // (`[...Numbers, boolean]` prints `[...number[], boolean]`).
                let printed = if matches!(element, TypeNode::RestTypeNode(_)) {
                    let unaliased = self.without_alias(resolved);
                    self.type_to_string(unaliased)
                } else {
                    self.type_to_string(resolved)
                };
                pieces.push(format!("{prefix}{printed}{suffix}"));
            }
            if let Some(flat) = spliced {
                let readonly = node
                    .node_id
                    .and_then(|id| self.nodes.parent(id))
                    .is_some_and(|parent| self.is_readonly_type_operator(parent));
                if flat.iter().any(|(_, optional, label)| *optional || label.is_some()) {
                    let elements: Vec<_> =
                        flat.iter().map(|(t, optional, _)| (*t, *optional)).collect();
                    let labels: Vec<_> = flat.into_iter().map(|(_, _, label)| label).collect();
                    return self.create_optional_tuple_type(&elements, &labels, readonly);
                }
                return self
                    .create_tuple_type(flat.into_iter().map(|(t, _, _)| t).collect(), readonly);
            }
            // §959 (§958's rule, landed once the spelling had somewhere to live): a
            // tuple made ONLY of rests over array-likes IS an array of the union of
            // their element types — `createNormalizedTupleType`, since two unbounded
            // rests cannot be expressed positionally.
            //
            // ```
            // [...boolean[]]                  boolean[]            11 rows, genericRestParameters2
            // [...string[], ...Array<number>] (string | number)[]  variadicTuples2 V16-V18
            // [...any]                        any[]                variadicTuples1
            // ```
            //
            // The ALIAS road is untouched, which is §956's rule doing its work:
            // `type T03 = [...string[]]` still prints `T03` because nothing was
            // rewritten, while `V16 = [...string[], ...Array<number>]` prints the
            // structure — the two differ only in §956's reference test.
            //
            // A rest over something that is NOT an array-like keeps the decline:
            // `[...string]` is upstream's `any[]` by way of an ERROR, and answering
            // it here would be inventing that error's recovery.
            if node.elements.iter().all(|element| matches!(element, TypeNode::RestTypeNode(_))) {
                let mut element_types = Vec::with_capacity(node.elements.len());
                let mut every_operand_is_an_array = true;
                for element in node.elements {
                    let TypeNode::RestTypeNode(rest) = element else { continue };
                    let Some(operand) = rest.r#type else { return error };
                    let resolved = self.get_type_from_type_node(operand);
                    if self.store.get(resolved).flags.contains(TypeFlags::ANY) {
                        element_types.push(resolved);
                        continue;
                    }
                    if let Some(single) = self.tuple_spread_array_element(resolved) {
                        element_types.push(single);
                    } else {
                        every_operand_is_an_array = false;
                        break;
                    }
                }
                if every_operand_is_an_array && !element_types.is_empty() {
                    let element = self.get_union_type(&element_types);
                    let readonly = node
                        .node_id
                        .and_then(|id| self.nodes.parent(id))
                        .is_some_and(|parent| self.is_readonly_type_operator(parent));
                    let target = if readonly { "ReadonlyArray" } else { "Array" };
                    if let Some(target) = self.global_type_symbol(target) {
                        return self.create_type_reference(target, vec![element]);
                    }
                }
            }
            let readonly = node
                .node_id
                .and_then(|id| self.nodes.parent(id))
                .is_some_and(|parent| self.is_readonly_type_operator(parent));
            let text =
                format!("{}[{}]", if readonly { "readonly " } else { "" }, pieces.join(", "));
            let minted = self.store.new_named(TypeFlags::OBJECT, text, None);
            self.variadic_tuple_elements.insert(minted, (resolved_elements, readonly));
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
                    None if optional => format!("{}?", self.optional_tuple_element_text(element)),
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
        self.tuple_labels.insert(id, labels.to_vec());
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

    /// getThisType and getThisContainer (checker.go:22908, ast/utilities.go:1790).
    /// Arrows are transparent; other function and member containers stop the
    /// walk. A class annotation shares the identity used by this expressions.
    fn get_type_from_this_type_node(&mut self, node: &tsr_ast::ThisTypeNode<'a>) -> TypeId {
        let error = self.intrinsics.error;
        let Some(original) = node.node_id else { return error };
        let mut current = self.nodes.parent(original);
        while let Some(container) = current {
            let kind = self.nodes.kind(container);
            if kind == SyntaxKind::ComputedPropertyName {
                current = self
                    .nodes
                    .parent(container)
                    .and_then(|member| self.nodes.parent(member))
                    .and_then(|owner| self.nodes.parent(owner));
                continue;
            }
            if kind == SyntaxKind::Decorator {
                let Some(parent) = self.nodes.parent(container) else { return error };
                let member = if self.nodes.kind(parent) == SyntaxKind::Parameter {
                    self.nodes.parent(parent).unwrap_or(parent)
                } else {
                    parent
                };
                if self.nodes.parent(member).is_some_and(|owner| {
                    matches!(
                        self.nodes.kind(owner),
                        SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                    )
                }) {
                    current = self.nodes.parent(member).and_then(|owner| self.nodes.parent(owner));
                    continue;
                }
            }
            if matches!(
                kind,
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ModuleDeclaration
                    | SyntaxKind::ClassStaticBlockDeclaration
                    | SyntaxKind::PropertyDeclaration
                    | SyntaxKind::PropertySignature
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::MethodSignature
                    | SyntaxKind::Constructor
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::CallSignature
                    | SyntaxKind::ConstructSignature
                    | SyntaxKind::IndexSignature
                    | SyntaxKind::EnumDeclaration
                    | SyntaxKind::SourceFile
            ) {
                if kind == SyntaxKind::ClassStaticBlockDeclaration {
                    return error;
                }
                let Some(owner) = self.nodes.parent(container) else { return error };
                if !matches!(
                    self.nodes.kind(owner),
                    SyntaxKind::ClassDeclaration
                        | SyntaxKind::ClassExpression
                        | SyntaxKind::InterfaceDeclaration
                ) {
                    return error;
                }
                let modifiers = match self.node_map.get(container) {
                    Some(Node::MethodDeclaration(member)) => member.modifiers,
                    Some(Node::PropertyDeclaration(member)) => member.modifiers,
                    Some(Node::GetAccessorDeclaration(member)) => member.modifiers,
                    Some(Node::SetAccessorDeclaration(member)) => member.modifiers,
                    _ => &[],
                };
                if tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword) {
                    return error;
                }
                if let Some(Node::ConstructorDeclaration(constructor)) =
                    self.node_map.get(container)
                    && !constructor.body.and_then(|body| body.node_id()).is_some_and(|body| {
                        self.nodes.ancestors(original).any(|ancestor| ancestor == body)
                    })
                {
                    return error;
                }
                if self.nodes.kind(owner) == SyntaxKind::InterfaceDeclaration {
                    if let Some(&existing) = self.this_type_nodes.get(&owner) {
                        return existing;
                    }
                    let minted = self.store.new_named(
                        TypeFlags::TYPE_PARAMETER,
                        "this".to_string(),
                        self.binder.symbol_of(owner),
                    );
                    self.this_type_nodes.insert(owner, minted);
                    return minted;
                }
                let Some(symbol) = self.binder.symbol_of(owner) else { return error };
                if let Some(&existing) = self.this_types.get(&symbol) {
                    return existing;
                }
                let minted = self.store.new_named(
                    TypeFlags::TYPE_PARAMETER,
                    "this".to_string(),
                    Some(symbol),
                );
                self.this_types.insert(symbol, minted);
                return minted;
            }
            current = self.nodes.parent(container);
        }
        error
    }

    /// Ported from `Checker.getAliasSymbolForTypeNode` (`checker.go:23719`).
    ///
    /// The host is the nearest ancestor that is not a parenthesised type or a
    /// `readonly` type operator, and it names the type only when it is a type
    /// alias declaration.
    pub(crate) fn alias_symbol_for_type_node(&self, node: tsr_ast::NodeId) -> Option<SymbolId> {
        let host = self.type_alias_host_for_type_node(node)?;
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
        loop {
            let Some(parent) = self.nodes.parent(current) else {
                // JSDoc arm: the comment keeps no parent edge, and native's
                // reparsed alias sits beside the comment's host
                // (`reparseUnhosted`), so the walk continues from that host.
                match self.jsdoc_hosts.get(&current) {
                    Some(&jsdoc_host) if self.nodes.kind(current) == SyntaxKind::JSDoc => {
                        current = jsdoc_host;
                        continue;
                    }
                    _ => break,
                }
            };
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

    /// getAliasSymbolForTypeNode's source host, before display accessibility.
    fn type_alias_host_for_type_node(&self, node: NodeId) -> Option<NodeId> {
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
        match self.nodes.kind(host) {
            // `isTypeAlias` admits native's reparsed `JSTypeAliasDeclaration`,
            // whose `Type` is the typedef's `typeExpression.Type()` (the `{…}`
            // wrapper dropped) or the callback's reparsed signature.
            SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::JSDocTypedefTag
            | SyntaxKind::JSDocCallbackTag => Some(host),
            SyntaxKind::JSDocTypeExpression => self
                .nodes
                .parent(host)
                .filter(|&tag| self.nodes.kind(tag) == SyntaxKind::JSDocTypedefTag),
            _ => None,
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
    /// §952: the identifier a type node names, when it is a bare reference and
    /// nothing else. The mapped-type shape test compares three positions
    /// (`keyof T`, `T[P]`'s object and index) against written names, and an
    /// identity template is exactly the case where all three are bare.
    fn type_node_names(node: Option<TypeNode<'a>>) -> Option<&'a str> {
        let TypeNode::TypeReferenceNode(reference) = node? else { return None };
        if !reference.type_arguments.is_empty() {
            return None;
        }
        match reference.type_name? {
            tsr_ast::EntityName::Identifier(name) => Some(name.text),
            tsr_ast::EntityName::QualifiedName(_) => None,
        }
    }

    /// §952: the symbol whose member table a type reads its properties from —
    /// the same two shapes [`Checker::get_property_of_type`] dispatches on.
    fn members_owner_of(&self, id: TypeId) -> Option<SymbolId> {
        match &self.store.get(id).data {
            crate::types::TypeData::Named { members: Some(owner), .. } => Some(*owner),
            crate::types::TypeData::Anonymous { symbol, .. } => Some(*symbol),
            _ => None,
        }
    }

    /// The syntactic identity-template subset of isHomomorphicMappedType.
    fn identity_mapped_alias_node(
        &self,
        symbol: SymbolId,
    ) -> Option<&'a tsr_ast::MappedTypeNode<'a>> {
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(TypeNode::MappedTypeNode(mapped)) = alias.r#type
            && let [parameter_declaration] = self.local_type_parameters_of(symbol)
            && let Some(parameter_name) = parameter_declaration.name.map(|name| name.text)
            && let Some(mapped_parameter) = mapped.type_parameter
            && let Some(key_name) = mapped_parameter.name.map(|name| name.text)
            // The constraint must be `keyof T` for the alias's own parameter —
            // upstream's `isHomomorphicMappedType` test, syntactically.
            && let Some(TypeNode::TypeOperatorNode(operator)) = mapped_parameter.constraint
            && operator.operator.kind == SyntaxKind::KeyOfKeyword
            && Self::type_node_names(operator.r#type) == Some(parameter_name)
            // No `as` clause: a key remapping changes the NAMES, which is
            // exactly what reusing the source's owner cannot express.
            && mapped.name_type.is_none()
            // The template must be `T[P]` — the identity.
            && let Some(TypeNode::IndexedAccessTypeNode(access)) = mapped.r#type
            && Self::type_node_names(access.object_type) == Some(parameter_name)
            && Self::type_node_names(access.index_type) == Some(key_name)
        {
            Some(mapped)
        } else {
            None
        }
    }

    /// A mapped alias reference whose normalization may remove the enclosing
    /// alias identity. Other alias bodies keep their existing resolution path.
    fn mapped_alias_reference_body(&mut self, symbol: SymbolId) -> Option<TypeNode<'a>> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let body @ TypeNode::TypeReferenceNode(reference) = alias.r#type? else {
            return None;
        };
        let mapped = self.resolve_entity_name(reference.type_name?, SymbolFlags::TYPE)?;
        let declaration = self.binder.symbols().get(mapped).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(target)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(TypeNode::MappedTypeNode(mapped)) = target.r#type else { return None };
        if mapped.name_type.is_some() {
            return None;
        }
        let Some(TypeNode::TypeOperatorNode(constraint)) =
            mapped.type_parameter.and_then(|parameter| parameter.constraint)
        else {
            return None;
        };
        (constraint.operator.kind == SyntaxKind::KeyOfKeyword).then_some(body)
    }

    fn is_normalized_mapped_sequence(&mut self, resolved: TypeId) -> bool {
        self.tuple_element_lists.contains_key(&resolved)
            || self.variadic_tuple_elements.contains_key(&resolved)
            || (resolved != self.intrinsics.error
                && !self.store.get(resolved).flags.contains(TypeFlags::ANY)
                && self.tuple_spread_array_element(resolved).is_some())
    }

    /// The array/tuple branch of getTypeAliasInstantiation. The mapped body
    /// retains its deferred variadic operands until the alias's own parameters
    /// are substituted; createNormalizedTupleType then flattens those operands.
    fn instantiate_normalized_mapped_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let body = self.mapped_alias_reference_body(symbol)?;
        if !self.variadic_alias_in_progress.insert(symbol) {
            return None;
        }
        let resolved = (|| {
            let parameters = self.local_type_parameter_types_of(symbol)?;
            if parameters.is_empty() || parameters.len() != arguments.len() {
                return None;
            }
            let body = self.get_type_from_type_node(body);
            if !self.is_normalized_mapped_sequence(body) {
                return None;
            }
            let types: Vec<_> = parameters.iter().map(|&(t, _)| t).collect();
            let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
            let map: Vec<_> = types.iter().copied().zip(arguments.iter().copied()).collect();
            let instantiated = self.instantiate_type(body, &map, &types, &names);
            self.is_normalized_mapped_sequence(instantiated).then_some(instantiated)
        })();
        self.variadic_alias_in_progress.remove(&symbol);
        resolved
    }

    /// The identity-template part of instantiateMappedType
    /// (internal/checker/checker.go), shared by written and computed references.
    fn instantiate_identity_mapped_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) -> Option<TypeId> {
        let error = self.intrinsics.error;
        if arguments.len() == 1
            && let Some(mapped) = self.identity_mapped_alias_node(symbol)
        {
            // instantiateMappedType leaves primitive constituents unchanged.
            if self.type_of(arguments[0]).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER)
            {
                return Some(arguments[0]);
            }
            // `?` and `+?` add optionality, `-?` removes it, absent leaves it.
            let optionality =
                mapped.question_token.map(|token| !matches!(token.kind, SyntaxKind::MinusToken));
            // `readonly` / `+readonly` add, `-readonly` removes. The parser puts
            // the `+`/`-` in this slot, so a bare `readonly` is the keyword
            // itself.
            let readonly =
                mapped.readonly_token.map(|token| !matches!(token.kind, SyntaxKind::MinusToken));
            if self.store.get(arguments[0]).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
                || self.mapped_identity_sources.contains_key(&arguments[0])
                || self.is_generic_homomorphic_mapped_type(arguments[0])
            {
                let key = (symbol, arguments.to_vec());
                if let Some(&existing) = self.instantiations.get(&key) {
                    return Some(existing);
                }
                let name = self.binder.symbols().get(symbol).name.to_string();
                let text = format!("{name}<{}>", self.type_to_string(arguments[0]));
                let deferred = self.store.new_named(TypeFlags::OBJECT, text, None);
                self.type_reference_targets.insert(deferred, key.clone());
                self.mapped_identity_sources.insert(deferred, arguments[0]);
                self.mapped_identity_optionality.insert(deferred, (optionality, readonly));
                self.instantiations.insert(key, deferred);
                return Some(deferred);
            }
            // instantiateMappedType branches on tuples and arrays before
            // resolving object members. Tuple masks and labels carry the
            // element information needed by instantiateMappedTupleType.
            let tuple = self.variadic_tuple_elements.get(&arguments[0]).cloned().or_else(|| {
                self.tuple_element_lists.get(&arguments[0]).map(|(types, readonly)| {
                    let mask = self.tuple_optional_masks.get(&arguments[0]);
                    let labels = self.tuple_labels.get(&arguments[0]);
                    (
                        types
                            .iter()
                            .enumerate()
                            .map(|(index, &t)| crate::tuples::TupleElement {
                                r#type: t,
                                spread: false,
                                optional: mask
                                    .and_then(|mask| mask.get(index))
                                    .copied()
                                    .unwrap_or(false),
                                label: labels
                                    .and_then(|labels| labels.get(index))
                                    .cloned()
                                    .flatten(),
                            })
                            .collect(),
                        *readonly,
                    )
                })
            });
            if let Some((mut elements, source_readonly)) = tuple {
                if elements
                    .iter()
                    .any(|element| element.spread && !self.tuple_array_like(element.r#type))
                {
                    return None;
                }
                let mut variable_part = false;
                for element in &mut elements {
                    if element.spread {
                        variable_part = true;
                        if self.tuple_spread_array_element(element.r#type).is_none() {
                            // instantiateMappedTupleType applies the map
                            // itself to a Variadic operand, retaining it until
                            // that operand is substituted and normalized.
                            element.r#type =
                                self.instantiate_identity_mapped_alias(symbol, &[element.r#type])?;
                            continue;
                        }
                        // A Rest element maps through an array, then extracts
                        // its element type. Readonly belongs to the tuple,
                        // rather than the array used to represent this rest.
                        let rest = self.tuple_spread_array_element(element.r#type)?;
                        let mapped =
                            self.instantiate_identity_mapped_element(rest, optionality, true);
                        let array = self.global_type_symbol("Array")?;
                        element.r#type = self.create_type_reference(array, vec![mapped]);
                    } else {
                        // Indexed access into an optional tuple slot includes
                        // undefined unless exact optionality uses missingType.
                        // Missing is removed when printing an optional slot.
                        let template = if element.optional
                            && self.strict_null_checks
                            && !self.exact_optional_property_types
                        {
                            self.get_union_type(&[element.r#type, self.intrinsics.undefined])
                        } else {
                            element.r#type
                        };
                        let template_optionality = if self.exact_optional_property_types
                            && !variable_part
                            && optionality == Some(true)
                        {
                            None
                        } else {
                            optionality
                        };
                        element.r#type = self.instantiate_identity_mapped_element(
                            template,
                            template_optionality,
                            element.optional || variable_part,
                        );
                        element.optional = optionality.unwrap_or(element.optional);
                    }
                }
                return Some(
                    self.normalize_variadic_tuple(elements, readonly.unwrap_or(source_readonly)),
                );
            }
            if let Some((source_target, source_arguments)) =
                self.type_reference_targets.get(&arguments[0]).cloned()
                && let [element] = source_arguments.as_slice()
                && let source_readonly = ["Array", "ReadonlyArray"].iter().position(|global| {
                    self.global_type_symbol(global).map(|s| self.binder.merged_symbol(s))
                        == Some(self.binder.merged_symbol(source_target))
                })
                && let Some(source_readonly) = source_readonly
            {
                let mapped_element =
                    self.instantiate_identity_mapped_element(*element, optionality, true);
                // `readonly`/`+readonly` picks `ReadonlyArray`, `-readonly` picks
                // `Array`, and an absent modifier keeps whichever the source was.
                let target_name = match readonly {
                    Some(true) => "ReadonlyArray",
                    Some(false) => "Array",
                    None => ["Array", "ReadonlyArray"][source_readonly],
                };
                let Some(target) = self.global_type_symbol(target_name) else { return Some(error) };
                return Some(self.create_type_reference(target, vec![mapped_element]));
            }
            let source_owner = self.members_owner_of(arguments[0])?;
            let printed_arguments: Vec<String> =
                arguments.iter().map(|&a| self.type_to_string(a)).collect();
            let name = self.binder.symbols().get(symbol).name.to_string();
            let text = format!("{name}<{}>", printed_arguments.join(", "));
            let key = (text.clone(), symbol, arguments.to_vec());
            if let Some(&existing) = self.qualified_generic_reference_types.get(&key) {
                return Some(existing);
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(source_owner));
            self.mapped_identity_optionality.insert(minted, (optionality, readonly));
            self.qualified_generic_reference_types.insert(key, minted);
            self.type_reference_targets.insert(minted, (symbol, arguments.to_vec()));
            return Some(minted);
        }
        None
    }

    /// The identity-template case of instantiateMappedTypeTemplate.
    fn instantiate_identity_mapped_element(
        &mut self,
        element: TypeId,
        optionality: Option<bool>,
        is_optional: bool,
    ) -> TypeId {
        if !self.strict_null_checks {
            return element;
        }
        match optionality {
            Some(true) => {
                let types = match &self.store.get(element).data {
                    crate::types::TypeData::Union { types, .. } => types.as_slice(),
                    _ => std::slice::from_ref(&element),
                };
                if types.iter().any(|&t| {
                    self.store.get(t).flags.intersects(TypeFlags::UNDEFINED | TypeFlags::VOID)
                }) {
                    element
                } else {
                    self.get_union_type(&[element, self.intrinsics.undefined])
                }
            }
            Some(false) if is_optional => {
                self.get_type_with_facts(element, crate::flow::TypeFacts::NE_UNDEFINED)
            }
            _ => element,
        }
    }

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
            && self.local_type_parameters_of(symbol).len() == parameters
            && self.local_type_parameters_of(symbol)[node.type_arguments.len()..]
                .iter()
                .all(|declaration| declaration.default_type.is_some());
        // The bare arm carries **no lib gate**. §136's gate exists because a
        // partially-written list has to choose which position the default fills
        // and that choice is visible in print; a bare list fills every position
        // and has no choice to get wrong.
        // `getTypeFromClassOrInterfaceReference` (`checker.go:23169`): in a
        // JavaScript file a class or interface reference short of its
        // arguments still instantiates — the arity error is reported and
        // `fillMissingTypeArguments(…, isJs)` fills the tail with `any`
        // (`@type {Array}` is `any[]`). An alias reference has no such arm.
        let js_fill = node.type_arguments.len() < parameters
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::CLASS | tsr_binder::SymbolFlags::INTERFACE)
            // `ast.IsInJSFile(node)`: the file flag, reached through the
            // JSDoc host edge for a reference written in a JSDoc comment.
            && node.node_id.and_then(|id| self.source_file_of(id)).is_some_and(|file| {
                self.nodes.flags(file).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE)
            });
        let fillable = partially_written || bare_and_fully_defaulted || js_fill;
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
            if any_written || partially_written {
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
            // fillMissingTypeArguments (checker.go:21954) first maps every
            // unfilled position to errorType, so a default naming a later (or
            // its own) parameter is an invalid forward reference that becomes
            // errorType rather than escaping as a free parameter (TS2744 is
            // the diagnostic). Upstream prints that errorType as `any`; this
            // port's `error` is its gap marker (ADR-0038), so the definite
            // outcome uses `any` instead.
            //
            // Only a default whose declared type mentions its own or a later
            // parameter takes that road. Every other default keeps the
            // historical substitution, which resolves the default inside any
            // enclosing alias frame and refuses an unmapped parameter: that
            // refusal is what currently stops the eager expansion of
            // recursive defaulted aliases (`Conv<T, U = T>` in
            // infiniteConstraints), which upstream defers instead.
            let written = arguments.len();
            for index in written..parameters {
                let frames = std::mem::take(&mut self.alias_evaluation_bindings);
                let mut declared = self.get_default_from_type_parameter(types[index]);
                // `fillMissingTypeArguments`' JavaScript arm: a default
                // identical to `unknown` or `{}` is `any`, and an absent one is
                // `getDefaultTypeArgumentType(true)` = `any`.
                if js_fill
                    && let Some(default) = declared
                    && default != error
                {
                    let (unknown, empty) = (self.intrinsics.unknown, self.intrinsics.empty_object);
                    if self.is_type_identical_to(default, unknown)
                        == crate::relater::Ternary::Related
                        || self.is_type_identical_to(default, empty)
                            == crate::relater::Ternary::Related
                    {
                        declared = Some(self.intrinsics.any);
                    }
                }
                let declared = declared.unwrap_or(if js_fill {
                    self.intrinsics.any
                } else {
                    self.intrinsics.unknown
                });
                self.alias_evaluation_bindings = frames;
                if declared == error {
                    return error;
                }
                let forward = self.mentions_type_parameter(declared, &types[index..], &[]);
                let (resolved, map) = if forward {
                    let mut filled = arguments.clone();
                    filled.resize(parameters, self.intrinsics.any);
                    (declared, types.iter().copied().zip(filled).collect::<Vec<_>>())
                } else {
                    let resolved = if js_fill {
                        declared
                    } else {
                        self.get_default_from_type_parameter(types[index])
                            .unwrap_or(self.intrinsics.unknown)
                    };
                    if resolved == error {
                        return error;
                    }
                    (resolved, types.iter().copied().zip(arguments.iter().copied()).collect())
                };
                let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
                let instantiated = self.instantiate_type(resolved, &map, &types, &name_refs);
                if instantiated == error {
                    return error;
                }
                arguments.push(instantiated);
            }
        }
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            let declared = self.get_declared_type_of_symbol(symbol);
            let has_body = self.anonymous_properties.contains_key(&declared)
                || self.signature_types.contains_key(&declared)
                || self.instantiation_expression_sources.contains_key(&declared);
            if has_body && self.store.get(declared).flags.contains(TypeFlags::OBJECT) {
                let Some(own) = self.local_type_parameter_types_of(symbol) else { return error };
                let ids: Vec<_> = own.iter().map(|&(id, _)| id).collect();
                let names: Vec<_> = own.iter().map(|(_, name)| name.as_str()).collect();
                let map: Vec<_> = ids.iter().copied().zip(arguments.iter().copied()).collect();
                let image = self.instantiate_type(declared, &map, &ids, &names);
                let image = match node.node_id.and_then(|node| self.alias_symbol_for_type_node(node)) {
                    Some(alias) if !(self.alias_declaration_is_locally_scoped(node.type_name)
                        || self.mapped_types.get(&declared).is_some_and(|info| {
                            self.deferred_keyof_operands.get(&info.constraint).is_some_and(|variable| {
                                self.store.get(*variable).flags.contains(TypeFlags::TYPE_PARAMETER)
                                    && map.iter().any(|(parameter, image)| parameter == variable && image != variable)
                            })
                        })) => {
                        let alias_arguments = self.local_type_parameter_types_of(alias)
                            .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                        self.alias_object_image(image, alias, alias_arguments)
                    }
                    _ => image,
                };
                return image;
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
            && matches!(alias.r#type, Some(TypeNode::ConditionalTypeNode(_)))
            && self.in_alias_declared_position(node.node_id)
        {
            // §92.1: §91's evaluator now answers the computable slice of
            // this decline (extends-never conditionals over concrete
            // arguments); everything it refuses keeps the honest gap.
            if let Some(evaluated) = self.evaluate_conditional_alias(
                symbol,
                &arguments,
                node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)),
            ) {
                return evaluated;
            }
            // Upstream evaluates conditional aliases in alias-declared
            // positions even through type-parameter arguments
            // (`PrefixData<P>` answers `\`${P}:baz\``).
            return error;
        }
        // The same alias-declared position through an alias whose body is a
        // reference to a conditional alias (`type N3 = Not<boolean>` over
        // `type Not<C> = If<C, false, true>`): the new alias symbol names a
        // distributed result. A refusal keeps the named reference below.
        if self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && matches!(alias.r#type, Some(TypeNode::TypeReferenceNode(_)))
            && self.in_alias_declared_position(node.node_id)
            && let Some(evaluated) = self.evaluate_conditional_alias(
                symbol,
                &arguments,
                node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)),
            )
        {
            return evaluated;
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
            if self.variadic_tuple_elements.contains_key(&body) {
                let parameter_symbols: Vec<_> = alias
                    .type_parameters
                    .iter()
                    .filter_map(|parameter| parameter.node_id)
                    .filter_map(|id| self.binder.symbol_of(id))
                    .collect();
                let parameters: Vec<_> = parameter_symbols
                    .into_iter()
                    .map(|symbol| self.get_declared_type_of_symbol(symbol))
                    .collect();
                let names = self.local_type_parameter_names_of(symbol);
                let names: Vec<_> = names.iter().map(String::as_str).collect();
                if parameters.len() == arguments.len() && !parameters.is_empty() {
                    let map: Vec<_> =
                        parameters.iter().copied().zip(arguments.iter().copied()).collect();
                    // `createNormalizedTupleType` handles arrays, union arguments,
                    // and never as well as concrete tuple splices. All consumers
                    // use the same resolved element metadata and normalization.
                    let resolved = self.instantiate_type(body, &map, &parameters, &names);
                    if resolved != error {
                        self.variadic_alias_in_progress.remove(&symbol);
                        return resolved;
                    }
                }
            }
            self.variadic_alias_in_progress.remove(&symbol);
        }
        // §952: a generic alias whose body is a HOMOMORPHIC IDENTITY mapped
        // type — `{ [P in keyof T]: T[P] }`, with any combination of the `?` and
        // `readonly` modifiers — answers the ARGUMENT's own members.
        //
        // `Partial`, `Readonly` and `Required` are all exactly this shape, and
        // before this arm **every member of every mapped type gapped**:
        // `Partial<O>`'s `p.x`, `Readonly<O>`'s `r.x`, `keyof Partial<O>` and
        // `Partial<O>["x"]` all answered `errorType` (probed at §950's close).
        // The printed form was already right — upstream prints `Partial<O>`
        // because the mapped type carries an alias symbol — so this is a
        // members-only change, which is why it can reuse the existing mint.
        //
        // **Why reuse the source's member owner instead of synthesising
        // symbols.** Upstream's `resolveMappedTypeMembers` (`checker.go`) creates
        // a fresh property symbol per key and sets optionality and readonly on
        // it. This port's properties are binder symbols and the binder has no
        // facility for synthetic ones, so a faithful transliteration is blocked
        // on infrastructure that does not exist. For an IDENTITY template the
        // names and the types are the source's already — the only thing upstream
        // adds is the modifier — so the owner is reused and the modifier goes in
        // `mapped_identity_optionality`, read at
        // [`Checker::get_type_of_property_of_type`].
        //
        // **Restricted to the identity template on purpose.** A template that
        // TRANSFORMS (`Boxify<T> = { [P in keyof T]: Box<T[P]> }`) needs the
        // template instantiated per key, which needs the per-key binding this
        // arm deliberately does not build: handing back the source's member type
        // for `Boxify` would answer `string` where upstream answers
        // `Box<string>` — a confident wrong answer in place of a missing one,
        // which is the same line `record_index_info` draws for a literal-union
        // `Record` key (§785).
        if let Some(mapped) = self.instantiate_identity_mapped_alias(symbol, &arguments) {
            if self.mapped_identity_sources.contains_key(&mapped) {
                self.capture_mapped_alias(mapped, symbol, &arguments);
            }
            return mapped;
        }
        // getTypeAliasInstantiation caches by target and type argument
        // identities. Printed arguments can coincide across distinct scopes
        // (two mapped aliases can both use `Tuple[Key]`), so use the shared
        // reference factory rather than a spelling-keyed literal-alias mint.
        if partially_written
            && self
                .binder
                .symbols()
                .get(symbol)
                .declarations
                .first()
                .is_some_and(|&declaration| self.in_default_library(declaration))
        {
            let written = node.type_arguments.len();
            return self.create_type_reference_with_display(symbol, arguments, Some(written));
        }
        // The bare arm prints **every** argument, which is what upstream does:
        // `interface i00<T = number>` referenced as `<i00>x` prints
        // `i00<number>` (`genericDefaults.types:2538`). §136's display
        // truncation belongs to the partially-written arm alone — it exists so
        // a written `Map<string>` does not grow an argument nobody typed, a
        // question a bare reference does not raise.
        // serializeTypeForDeclaration reuses the written annotation node in a
        // signature when its type is the annotation's own: a reference to a
        // type-parameter-bodied alias resolves to its argument (below), yet a
        // signature still prints `set x(value: Fail<string>)`
        // (`divergentAccessorsTypes6`). Registered in §926's written-text
        // channel, which only annotation-reuse sites read.
        if !node.type_arguments.is_empty()
            && self.type_parameter_body_index(symbol).is_some()
            && let Some(id) = node.node_id
            && !self.qualified_written_text.contains_key(&id)
            && let Some(base) = Self::entity_name_text(node.type_name)
        {
            let spelled: Vec<String> = arguments[..node.type_arguments.len().min(arguments.len())]
                .iter()
                .map(|&argument| self.type_to_string(argument))
                .collect();
            self.qualified_written_text.insert(id, format!("{base}<{}>", spelled.join(", ")));
        }
        // instantiateMappedType maps a changed homomorphic variable through
        // mapTypeWithAlias; only a distributed union takes the new alias.
        let homomorphic_changed = self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && self.local_type_parameter_types_of(symbol).is_some_and(|parameters| {
                let declared = self.get_declared_type_of_symbol(symbol);
                self.mapped_types.get(&declared).is_some_and(|info| {
                    self.deferred_keyof_operands.get(&info.constraint).is_some_and(|variable| {
                        self.store.get(*variable).flags.contains(TypeFlags::TYPE_PARAMETER)
                            && parameters.iter().zip(&arguments)
                                .any(|((parameter, _), argument)| parameter == variable && argument != variable)
                    })
                })
            });
        let result = self.create_type_reference(symbol, arguments);
        let result = if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && self.type_parameter_body_index(symbol).is_none()
            && !homomorphic_changed
            && !self.is_normalized_mapped_sequence(result) {
            match node.node_id.and_then(|node| self.alias_symbol_for_type_node(node)) {
                Some(alias) if !self.alias_declaration_is_locally_scoped(node.type_name) => {
                    let parameters = self.local_type_parameter_types_of(alias)
                        .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                    self.alias_object_image(result, alias, parameters)
                }
                _ => result,
            }
        } else { result };
        // getTypeFromClassOrInterfaceReference (checker.go:23200): a generic
        // class or interface reference that IS an alias body is deferred and
        // carries the alias (`type ImmutableTypes = IImmutableMap<any>`).
        if self
            .binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            && !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
        {
            return self.deferred_alias_reference(node.node_id, result);
        }
        // getTypeAliasInstantiation supplies the enclosing alias to
        // mapTypeWithAlias. A distributed mapped union keeps that alias,
        // unlike a normalized array/tuple which has its structural display.
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(result).data
            && let Some(alias) = node.node_id.and_then(|id| self.alias_symbol_for_type_node(id))
            && self.local_type_parameters_of(alias).is_empty()
            && self.binder.symbols().get(symbol).declarations.first().copied()
                .and_then(|id| self.node_map.get(id))
                .is_some_and(|node| matches!(node, Node::TypeAliasDeclaration(alias) if matches!(alias.r#type,Some(TypeNode::MappedTypeNode(_)))))
        {
            let types = types.clone();
            return self.get_named_union_type(&types,TypeFlags::empty(),alias);
        }
        result
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
        let symbol = self.unresolved_symbol_for_entity_name(node.type_name);
        if symbol == self.symbols.unknown() {
            return self.intrinsics.error;
        }
        let mut printed = self.symbols.symbol_path(&symbol).expect("own unresolved symbol");
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

    /// Native getUnresolvedSymbolForEntityName follows the entity-name AST.
    /// Splitting printed text would invent parents for a dotted identifier.
    fn unresolved_symbol_for_entity_name(
        &mut self,
        name: Option<tsr_ast::EntityName<'a>>,
    ) -> crate::symbol_access::SymbolRef {
        let (text, left) = match name {
            Some(tsr_ast::EntityName::Identifier(identifier)) => (identifier.text, None),
            Some(tsr_ast::EntityName::QualifiedName(qualified)) => {
                let Some(right) = qualified.right else {
                    return self.symbols.unknown();
                };
                (right.text, qualified.left)
            }
            None => return self.symbols.unknown(),
        };
        if text.is_empty() {
            return self.symbols.unknown();
        }
        let parent = left.map(|left| self.unresolved_symbol_for_entity_name(Some(left)));
        self.symbols.unresolved_symbol(text, parent.as_ref()).expect("own unresolved parent")
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
        let in_alias_cycle =
            self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
                && node.node_id.and_then(|id| self.alias_symbol_for_type_node(id)).is_some_and(
                    |alias| {
                        self.resolutions
                            .on_stack(alias, crate::resolution::PropertyName::DeclaredType)
                    },
                );
        let Some(resolved) = self.resolve_entity_name_ex(name, SymbolFlags::TYPE, false) else {
            if in_alias_cycle {
                return error;
            }
            return self.unresolved_type_reference(node);
        };
        if in_alias_cycle {
            return error;
        }
        if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
            && self.alias_rooted_reference_declines(node, namespace, resolved)
        {
            return self.unresolved_type_reference(node);
        }
        // QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE: an argument-less reference
        // to a non-generic type alias answers `getTypeReferenceType`'s declared
        // type (`getTypeFromTypeAliasReference`, `checker.go:23580`) whenever
        // that type does NOT carry the alias. Upstream's declared type of an
        // alias is the body as built, and only alias-accepting constructors
        // attach the alias to it; a pre-existing body (`number`, `undefined`,
        // an interface) prints as itself, never as `N.T`. A body that does
        // carry the alias keeps the written-text mint below, because the
        // printer cannot yet qualify an alias name from its symbol
        // (NB-SYMBOL-CHAIN): routing those too measured 36 R→W, every one an
        // unqualified `T7` for `N.T7`. See `docs/parity/notes/type-refs.md`.
        if node.type_arguments.is_empty()
            && self.binder.symbols().get(resolved).flags.intersects(SymbolFlags::TYPE_ALIAS)
            && self.local_type_parameters_of(resolved).is_empty()
            && !self.resolutions.on_stack(resolved, PropertyName::DeclaredType)
        {
            let declared = self.get_declared_type_of_symbol(resolved);
            if declared != error && !self.declared_type_carries_alias(declared, resolved) {
                return self.get_regular_type_of_literal_type(declared);
            }
        }
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
                if let Some(&spelled) = self.enum_access_spelling.get(&regular) {
                    return spelled;
                }
                return regular;
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
            // QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, enum arm: upstream's
            // answer is the enum's declared type (`getTypeReferenceType`). It
            // replaces the mint wherever it prints as the mint would, so no
            // rendered line can move and the relater sees the real enum (a
            // mint is an OBJECT: `<foo.E1>0` assigned to `number` reported
            // TS2322 once alias-rooted names resolved). Where the prints differ
            // (`A.B.C.E` printed through a local alias `I`, a union's
            // named-constituent guard) the mint stays — routing those measured
            // 4 R→W, the NB-SYMBOL-CHAIN wall again.
            //
            // Scoped to an ALIAS-rooted name — the population the alias walk
            // in `resolve_entity_name_ex` newly resolves, which answered the
            // ANY-flagged unresolved mint before. A namespace-rooted enum keeps
            // §41's measured mint: routing it measured 3 R→W where a union's
            // named-constituent guard (`boolean | X.Foo`) turned it into `any`.
            if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
                && self.binder.symbols().get(resolved).flags.intersects(SymbolFlags::ENUM)
                && !self.binder.symbols().get(resolved).flags.intersects(SymbolFlags::ENUM_MEMBER)
            {
                let declared = self.get_declared_type_of_symbol(resolved);
                let printed = match node.node_id {
                    Some(site) => self.type_to_string_at(declared, site),
                    None => None,
                };
                if declared != error && printed.as_deref() == Some(text.as_str()) {
                    return self.get_regular_type_of_literal_type(declared);
                }
            }
            let key = (text.clone(), resolved);
            if let Some(&existing) = self.qualified_reference_types.get(&key) {
                return existing;
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(resolved));
            if self.binder.symbols().get(resolved).flags.contains(SymbolFlags::ENUM_MEMBER) {
                let declared = self.get_declared_type_of_symbol(resolved);
                let regular = self.get_regular_type_of_literal_type(declared);
                self.enum_member_regular.insert(minted, regular);
            }
            self.qualified_reference_types.insert(key, minted);
            return minted;
        }
        // QUALIFIED-TYPEREF-GET-TYPE-REFERENCE-TYPE, generic arm, scoped to an
        // ALIAS-rooted name (`React.HTMLAttributes<HTMLElement>` through
        // `import * as React`), the population the alias walk newly resolves:
        // the shared `getTypeReferenceType` road instantiates the target with
        // its arity window and defaults instead of the print-only mint, which
        // the relater could not see through (`= {}` reported TS2322).
        // Namespace-rooted generics keep §42 v2's mint: routing them measured
        // 54 R→W, unqualified `Keyed<K, V>` for `Seq.Keyed<K, V>` inside the
        // namespace's own members (NB-SYMBOL-CHAIN).
        if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS)
            && !self.local_type_parameters_of(resolved).is_empty()
        {
            return self.get_type_reference_type(node, resolved);
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
            let key = (text.clone(), resolved, arguments.clone());
            if let Some(&existing) = self.qualified_generic_reference_types.get(&key) {
                return existing;
            }
            let minted = self.store.new_named(TypeFlags::OBJECT, text, Some(resolved));
            self.qualified_generic_reference_types.insert(key, minted);
            self.type_reference_targets.insert(minted, (resolved, arguments));
            return minted;
        }
        self.unresolved_type_reference(node)
    }

    /// Whether a type alias's declared type carries the alias — upstream's
    /// `t.alias.symbol == symbol` — read from this port's print-at-creation
    /// representation: a declared type carries its alias exactly when it
    /// prints as the alias's own name (an alias-attributed union, the
    /// name-bearing mints of `get_declared_type_of_type_alias`). Anything
    /// else (an intrinsic, a literal, an interface the body merely names)
    /// was not created for the alias and prints as itself.
    fn declared_type_carries_alias(&mut self, declared: TypeId, alias: SymbolId) -> bool {
        if matches!(&self.store.get(declared).data,
            crate::types::TypeData::Union { symbol: Some(symbol), .. } if *symbol == alias)
        {
            return true;
        }
        // `isDeferredTypeReferenceNode` (`checker.go:23236`): a body that is
        // directly the alias's type node (`getAliasSymbolForTypeNode`) and is
        // an array, a tuple or a reference written with type arguments is
        // built as a deferred reference carrying the alias, whatever this
        // port's eager type prints (`type S = Container<string>` is `S`).
        let mut body = self.type_alias_body(alias);
        while let Some(TypeNode::ParenthesizedTypeNode(parenthesized)) = body {
            body = parenthesized.r#type;
        }
        match body {
            Some(TypeNode::ArrayTypeNode(_) | TypeNode::TupleTypeNode(_)) => return true,
            Some(TypeNode::TypeReferenceNode(reference))
                if !reference.type_arguments.is_empty() =>
            {
                return true;
            }
            _ => {}
        }
        let name = self.binder.symbols().get(alias).name.to_string();
        self.type_to_string(declared) == name
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

    /// Ported from `Checker.resolveEntityName` (`checker.go:15772`) with
    /// `ignoreErrors = true` and a nil `location`, **including the alias
    /// steps** [`Checker::resolve_entity_name`] leaves out:
    ///
    /// - `resolveQualifiedName` (`checker.go:15828`) resolves the left with
    ///   meaning `Namespace` and `dontResolveAlias = false`, so an
    ///   import-equals, `import * as` or ES-import left is followed to the
    ///   namespace or module before its exports are read;
    /// - the right name is looked up in `getExportsOfSymbol(namespace)` with
    ///   `getSymbol`'s meaning filter (an alias counts when its chain's flags
    ///   meet `meaning`), and when that misses on an alias namespace, in the
    ///   exports of `resolveAlias(namespace)` (`checker.go:15853`);
    /// - the found symbol is walked along its alias chain until it carries
    ///   `meaning` (`checker.go:15821`), unless `dont_resolve_alias`.
    ///
    /// `&mut` because `resolveAlias` is (its JS arms check expressions); the
    /// `&self` [`Checker::resolve_entity_name`] stays for callers that cannot
    /// take a mutable borrow. Not ported: the `CommonJS` `require` redirect
    /// (`checker.go:15836`) and the `export=` typedef fallback for an
    /// identifier namespace (`checker.go:15796`); each is a `None`, never a
    /// different symbol.
    ///
    /// No cache: upstream memoises `resolveAlias` per alias symbol
    /// (`aliasSymbolLinks.aliasTarget`), which this port recomputes (see
    /// [`Checker::resolve_alias`]); every caller here memoises the *type* it
    /// builds from the answer.
    pub(crate) fn resolve_entity_name_ex(
        &mut self,
        name: tsr_ast::EntityName<'a>,
        meaning: SymbolFlags,
        dont_resolve_alias: bool,
    ) -> Option<SymbolId> {
        let mut symbol = match name {
            tsr_ast::EntityName::Identifier(identifier) => {
                let found = self.resolve_name_with_export_alias(
                    identifier.node_id?,
                    identifier.text,
                    meaning,
                )?;
                self.binder.merged_symbol(found)
            }
            tsr_ast::EntityName::QualifiedName(qualified) => {
                let namespace =
                    self.resolve_entity_name_ex(qualified.left?, SymbolFlags::NAMESPACE, false)?;
                let right = qualified.right?;
                match self.get_symbol_of_exports(namespace, right.text, meaning) {
                    Some(found) => found,
                    None if self
                        .binder
                        .symbols()
                        .get(namespace)
                        .flags
                        .intersects(SymbolFlags::ALIAS) =>
                    {
                        let target = self.resolve_alias(namespace)?;
                        let target = self.binder.merged_symbol(target);
                        self.get_symbol_of_exports(target, right.text, meaning)?
                    }
                    None => return None,
                }
            }
        };
        let mut seen = 0;
        while !dont_resolve_alias
            && !self.binder.symbols().get(symbol).flags.intersects(meaning)
            && self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS)
        {
            // `resolveAlias` closes cycles with its own resolution frame; the
            // port's is `get_symbol_flags`' visited set, which the meaning
            // filter above already ran. A bounded walk keeps a cycle the
            // filter admitted (an unknown target counts as every meaning) a
            // miss rather than a hang.
            seen += 1;
            if seen > 64 {
                return None;
            }
            symbol = self.binder.merged_symbol(self.resolve_alias(symbol)?);
        }
        Some(symbol)
    }

    /// `getMergedSymbol(getSymbol(getExportsOfSymbol(namespace), name,
    /// meaning))` (`checker.go:15851`, `getSymbol` at `:2176`).
    ///
    /// `getExportsOfSymbol` reads a module's table through
    /// `getExportsOfModule`, which first follows `export =`
    /// (`resolveExternalModuleSymbol` with `dontResolveAlias = false`) and then
    /// adds `export *` re-exports; any other symbol answers its own `exports`.
    fn get_symbol_of_exports(
        &mut self,
        namespace: SymbolId,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let namespace = self.binder.merged_symbol(namespace);
        let found = if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::MODULE) {
            let mut module = self.resolve_external_module_symbol(namespace);
            if module != namespace
                && self.binder.symbols().get(module).flags.intersects(SymbolFlags::ALIAS)
            {
                module = self.resolve_alias(module)?;
            }
            let module = self.binder.merged_symbol(module);
            match self.get_export_of_module(module, name) {
                Some(found) => Some(found),
                None => self.binder.symbols().get(module).exports.get(name).copied(),
            }
        } else {
            self.binder.symbols().get(namespace).exports.get(name).copied()
        }?;
        let found = self.binder.merged_symbol(found);
        let flags = self.binder.symbols().get(found).flags;
        if flags.intersects(meaning) {
            return Some(found);
        }
        if flags.intersects(SymbolFlags::ALIAS) {
            // `getSymbol`'s alias arm: the chain's meaning decides, and an
            // unknown target reports every meaning (`getSymbolFlagsEx`,
            // `checker.go:16378`).
            return self.get_symbol_flags(found).intersects(meaning).then_some(found);
        }
        None
    }

    /// Whether an alias-rooted qualified reference must stay the unresolved
    /// gap it was before [`Checker::resolve_entity_name_ex`] learned to walk
    /// aliases, because the symbol it now reaches would be answered by
    /// machinery this port does not have, and the answer would be a wrong
    /// diagnostic rather than a missing one. Both arms are the pre-existing
    /// behaviour of the same names written without an alias; neither is a
    /// rule upstream has.
    ///
    /// - **Generic heritage.** The relater rejects `{}` for any generic
    ///   interface or class whose own heritage clause writes type arguments
    ///   (`interface S<T> extends D<T> { a?: string }`; `const y: S<number> =
    ///   {}` reports a false TS2322 with or without a namespace). Resolving
    ///   `React.HTMLAttributes<HTMLElement>` through `import * as React`
    ///   surfaced it as a new diagnostic in
    ///   `reactTagNameComponentWithPropsNoOOM2`. Owned by the relater /
    ///   base-type lanes (CLASS-GET-BASE-TYPES, INTERFACE-BASE-FROM-TYPE-NODE).
    /// - **Module augmentation.** The binder does not run
    ///   `mergeModuleAugmentation` (`checker.go:1407`), so a module augmented
    ///   from the reference's file has an incomplete export table
    ///   (`moduleAugmentationDoes{Interface,Namespace}MergeOfReexport`: TS2339
    ///   on members the augmentation adds). Only augmentations in the
    ///   reference's own file are seen; elsewhere this road matches the
    ///   ES-import road, which ignores augmentations too.
    ///
    /// Delete each arm when its owner lands; the falsifier is the named case
    /// going `EMPTY_RIGHT` with the arm removed.
    fn alias_rooted_reference_declines(
        &mut self,
        node: &tsr_ast::TypeReferenceNode<'a>,
        namespace: SymbolId,
        resolved: SymbolId,
    ) -> bool {
        if !node.type_arguments.is_empty() && self.has_generic_heritage(resolved) {
            return true;
        }
        let Some(target) = self.resolve_alias(namespace) else { return false };
        let target = self.binder.merged_symbol(target);
        // Only a module can be augmented; skip the statement scan otherwise.
        if !self.binder.symbols().get(target).flags.intersects(SymbolFlags::VALUE_MODULE) {
            return false;
        }
        let Some(mut file) = node.node_id else { return false };
        while let Some(parent) = self.nodes.parent(file) {
            file = parent;
        }
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return false };
        if !tsr_binder::is_external_module(source) {
            return false;
        }
        for statement in source.statements {
            let Statement::ModuleDeclaration(module) = statement else { continue };
            let Some(tsr_ast::ModuleName::StringLiteral(name)) = module.name else { continue };
            let (Some(declaration), Some(specifier)) = (module.node_id, name.node_id) else {
                continue;
            };
            if self
                .resolve_external_module_name(declaration, specifier)
                .is_some_and(|augmented| self.binder.merged_symbol(augmented) == target)
            {
                return true;
            }
        }
        false
    }

    /// A class or interface declaration of `symbol` whose heritage clause
    /// writes type arguments (`extends D<T>`).
    fn has_generic_heritage(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(interface)) => interface.heritage_clauses,
                Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
                _ => return false,
            };
            clauses.iter().any(|clause| clause.types.iter().any(|t| !t.type_arguments.is_empty()))
        })
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
        let TypeNode::TypeLiteralNode(literal) = self.type_alias_body(symbol)? else { return None };
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
        let evaluated = self.evaluate_conditional_alias(symbol, arguments, None)?;
        match &self.store.get(evaluated).data {
            crate::types::TypeData::Named { members: Some(members), .. } => Some(*members),
            _ => None,
        }
    }

    /// A closed concrete slice of native getTypeAliasInstantiation (5b1047d).
    /// Prove dependencies before entering the existing alias worker: its
    /// Checker-local (symbol, ordered arguments) key omits captured frames and
    /// has no active-union publication. Own parameters, literal arguments and
    /// finite literal lookup tables admit neither captures nor recursive edges.
    /// The worker then completes the body before the source-owned union is
    /// published; no new cache, reservation or member image is introduced.
    fn is_closed_literal_union_alias(&self, symbol: SymbolId, arguments: &[TypeId]) -> bool {
        if arguments.is_empty()
            || arguments.iter().any(|&argument| {
                self.literal_key_texts(argument).is_none_or(|keys| keys.is_empty())
            })
        {
            return false;
        }
        let [declaration] = self.binder.symbols().get(symbol).declarations.as_slice() else {
            return false;
        };
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(*declaration) else {
            return false;
        };
        let Some(body @ TypeNode::UnionTypeNode(_)) = alias.r#type else { return false };
        let parameters: Option<Vec<_>> = alias
            .type_parameters
            .iter()
            .map(|parameter| parameter.node_id.and_then(|node| self.binder.symbol_of(node)))
            .collect();
        let Some(parameters) = parameters else { return false };
        parameters.len() == arguments.len()
            && self.is_closed_literal_union_operand(body, &parameters, 16)
    }

    /// Read-only syntax/binder proof; do not chase aliases or force type nodes.
    /// The depth bound limits this preflight, not semantic alias resolution.
    fn is_closed_literal_union_operand(
        &self,
        node: TypeNode<'a>,
        parameters: &[SymbolId],
        remaining: u8,
    ) -> bool {
        let Some(remaining) = remaining.checked_sub(1) else { return false };
        let recurse = |node| self.is_closed_literal_union_operand(node, parameters, remaining);
        match node {
            TypeNode::ParenthesizedTypeNode(node) => node.r#type.is_some_and(recurse),
            TypeNode::LiteralTypeNode(node) => matches!(node.literal, Some(Node::StringLiteral(_))),
            TypeNode::UnionTypeNode(node) => node.types.iter().all(|&node| recurse(node)),
            TypeNode::TypeReferenceNode(reference) if reference.type_arguments.is_empty() => {
                let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
                    return false;
                };
                name.node_id
                    .and_then(|node| {
                        self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            node,
                            name.text,
                            SymbolFlags::TYPE,
                        )
                    })
                    .is_some_and(|symbol| parameters.contains(&symbol))
            }
            TypeNode::IndexedAccessTypeNode(indexed) => {
                let Some(TypeNode::TypeLiteralNode(table)) = indexed.object_type else {
                    return false;
                };
                let mut names = Vec::new();
                table.members.iter().all(|member| {
                    let tsr_ast::TypeElement::PropertySignatureDeclaration(property) = member
                    else {
                        return false;
                    };
                    let name = match Node::from(property.name) {
                        Node::Identifier(name) => name.text,
                        Node::StringLiteral(name) => name.text,
                        _ => return false,
                    };
                    if names.contains(&name) {
                        return false;
                    }
                    names.push(name);
                    property.modifiers.is_empty()
                        && property.postfix_token.is_none()
                        && property.initializer.is_none()
                        && matches!(property.r#type, Some(TypeNode::LiteralTypeNode(literal))
                            if matches!(literal.literal, Some(Node::StringLiteral(_))))
                }) && indexed.index_type.is_some_and(recurse)
            }
            _ => false,
        }
    }

    /// Whether `symbol` is the `NoInfer` intrinsic alias — `intrinsicTypeKinds`
    /// (`checker.go:364`) read by `getTypeAliasInstantiation`: an alias named
    /// `NoInfer` whose declared body is the `intrinsic` keyword.
    pub(crate) fn is_no_infer_alias(&self, symbol: SymbolId) -> bool {
        let data = self.binder.symbols().get(symbol);
        if data.name != "NoInfer" || !data.flags.contains(SymbolFlags::TYPE_ALIAS) {
            return false;
        }
        data.declarations.first().is_some_and(|&declaration| {
            matches!(self.node_map.get(declaration),
                Some(Node::TypeAliasDeclaration(alias))
                    if matches!(alias.r#type, Some(TypeNode::KeywordTypeNode(keyword))
                        if keyword.kind == SyntaxKind::IntrinsicKeyword))
        })
    }

    /// `isNoInferType` (`checker.go:26822`), answering the base type.
    ///
    /// Upstream's `getNoInferType` (`checker.go:27394`) creates a substitution
    /// type over the base with an `unknown` constraint. This port has no
    /// substitution type: `NoInfer<T>` is the alias reference
    /// [`Checker::create_type_reference`] mints for `(NoInfer, [base])` —
    /// the same identity key upstream's cache uses — recorded in
    /// `type_reference_targets`, which this reads. It prints `NoInfer<T>`, as
    /// `nodebuilderimpl.go:3511` does. Not ported: `getNoInferType`'s
    /// `isNoInferTargetType` collapse (`NoInfer<string>` is `string`) and the
    /// substitution flags, whose base-constraint and narrowable-reference
    /// readers would expose an unfixed contextual parameter type
    /// (`narrowingNoInfer1`). Readers that see through it here: relation
    /// normalization and inference.
    pub(crate) fn no_infer_base_type(&self, t: TypeId) -> Option<TypeId> {
        match self.type_reference_targets.get(&t) {
            Some((symbol, arguments)) if self.is_no_infer_alias(*symbol) => match **arguments {
                [base] => Some(base),
                _ => None,
            },
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
        if arguments.len() == 1
            && self.global_type_symbol_with_arity("NonNullable", 1) == Some(symbol)
        {
            return self.get_global_non_nullable_type_instantiation(arguments[0]);
        }
        if let Some(mapped) = self.instantiate_string_mapping_alias(symbol, &arguments) {
            self.instantiations.insert((symbol, arguments), mapped);
            return mapped;
        }
        if let Some(mapped) = self.instantiate_identity_mapped_alias(symbol, &arguments) {
            if self.mapped_identity_sources.contains_key(&mapped) {
                self.capture_mapped_alias(mapped, symbol, &arguments);
            }
            self.instantiations.insert((symbol, arguments), mapped);
            return mapped;
        }
        // Native 5b1047d getTypeAliasInstantiation completes its DeclaredType
        // owner before an instantiation-cache hit. Reuse that existing
        // SymbolId-owned publication only for the certified original bare
        // keyword route and full default-filled arity; completed/failed entries
        // and mapped/active contexts keep their existing routes and keys.
        if !self.declared_types.contains_key(&symbol)
            && self.original_generic_keyword_alias_body(symbol).is_some()
            && matches!(self.type_alias_body(symbol), Some(TypeNode::KeywordTypeNode(_)))
            && self.local_type_parameters_of(symbol).len() == arguments.len()
        {
            let _ = self.get_declared_type_of_symbol(symbol);
        }
        // getTypeAliasInstantiation (5b1047d:19327/23837) maps the declared
        // instantiation-expression object, not an empty alias-symbol table.
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            let declared = self.get_declared_type_of_symbol(symbol);
            if self.instantiation_expression_sources.contains_key(&declared)
                || self.instantiation_expression_composites.contains(&declared)
            {
                if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
                    return cached;
                }
                let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
                    return self.intrinsics.error;
                };
                let ids: Vec<_> = parameters.iter().map(|&(id, _)| id).collect();
                let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
                if ids.len() != arguments.len() { return self.intrinsics.error; }
                let map: Vec<_> = ids.iter().copied().zip(arguments.iter().copied()).collect();
                let image = self.instantiate_type(declared, &map, &ids, &names);
                self.instantiations.insert((symbol, arguments), image);
                return image;
            }
        }
        if let Some(&cached) = self.instantiations.get(&(symbol, arguments.clone())) {
            return cached;
        }
        // instantiateTypeWithAlias (checker.go:22104) over a TYPE PARAMETER:
        // a generic alias whose body is one of its own parameters has that
        // parameter as its declared type (§282), and instantiating a type
        // parameter answers the mapper's image untouched — no alias is ever
        // attached, so `type Id<T> = T; type X = Id<string>` records
        // `>X : string` and `WithSpec<any>` is `any`
        // (`conditionalTypeAnyUnion`).
        if let Some(index) = self.type_parameter_body_index(symbol)
            && let Some(&image) = arguments.get(index)
            && self.local_type_parameters_of(symbol).len() == arguments.len()
        {
            self.instantiations.insert((symbol, arguments), image);
            return image;
        }
        // A type alias instantiation has the flags and identity of its body.
        // Keyword bodies do not depend on the mapper or carry an alias name.
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(body @ TypeNode::KeywordTypeNode(keyword)) = alias.r#type
            && keyword.kind != SyntaxKind::IntrinsicKeyword
        {
            let evaluated = self.get_type_from_type_node(body);
            self.instantiations.insert((symbol, arguments), evaluated);
            return evaluated;
        }
        if let Some(evaluated) = self.evaluate_conditional_alias(symbol, &arguments, None) {
            self.instantiations.insert((symbol, arguments), evaluated);
            return evaluated;
        }
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && (matches!(alias.r#type, Some(TypeNode::IndexedAccessTypeNode(_)))
                || self.is_closed_literal_union_alias(symbol, &arguments))
            && let Some(evaluated) = self.evaluate_alias_body(symbol, &arguments)
        {
            let text = self.type_reference_text(symbol, &arguments);
            let evaluated = if let Some(&operands) =
                self.deferred_indexed_access_types.get(&evaluated)
            {
                let named = self.store.new_named(TypeFlags::INDEXED_ACCESS, text, None);
                self.deferred_indexed_access_types.insert(named, operands);
                self.deferred_index_mints.insert(named);
                self.type_reference_targets.insert(named, (symbol, arguments.clone()));
                named
            } else if let crate::types::TypeData::Union { types, .. } =
                self.store.get(evaluated).data.clone()
            {
                let named = crate::unions::create_union(
                    &mut self.store,
                    TypeFlags::empty(),
                    types,
                    Some((symbol, text)),
                );
                self.type_reference_targets.insert(named, (symbol, arguments.clone()));
                named
            } else if let crate::types::TypeData::Anonymous {
                symbol: owner, signature: true, ..
            } = self.store.get(evaluated).data
            {
                let named = self.store.new_anonymous(TypeFlags::OBJECT, text, owner, true);
                if let Some(signatures) = self.signature_types.get(&evaluated).cloned() {
                    self.signature_types.insert(named, signatures);
                }
                self.alias_named_signature_types.insert(named);
                self.type_reference_targets.insert(named, (symbol, arguments.clone()));
                named
            } else {
                evaluated
            };
            self.instantiations.insert((symbol, arguments), evaluated);
            return evaluated;
        }
        if let Some(template) = self.instantiate_template_alias(symbol, &arguments) {
            self.instantiations.insert((symbol, arguments), template);
            return template;
        }
        if let Some(mapped) = self.instantiate_normalized_mapped_alias(symbol, &arguments) {
            self.instantiations.insert((symbol, arguments), mapped);
            return mapped;
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
        // A deferred conditional keeps the flags of its body. In particular,
        // indexed access must defer member selection until its arguments have
        // chosen a branch, rather than treating the alias as an empty object.
        let conditional = self
            .binder
            .symbols()
            .get(symbol)
            .declarations
            .first()
            .and_then(|&declaration| self.node_map.get(declaration))
            .is_some_and(|node| {
                matches!(node, Node::TypeAliasDeclaration(alias)
                    if matches!(alias.r#type, Some(TypeNode::ConditionalTypeNode(_))))
            });
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
        let flags = if conditional { TypeFlags::CONDITIONAL } else { TypeFlags::OBJECT };
        let id = self.store.new_named(flags, printed, Some(member_symbol));
        self.instantiations.insert((symbol, arguments.clone()), id);
        if shown < arguments.len() {
            self.reference_display_arity.insert(id, shown);
        }
        // The same pair, the other way round. Substitution starts from a
        // `TypeId` and needs the pair, which only exists here as a key — see
        // [`crate::checker::Checker::type_reference_targets`]. Written on the
        // miss path only, so it is one insert per distinct reference.
        self.type_reference_targets.insert(id, (symbol, arguments.clone()));
        self.capture_mapped_alias(id, symbol, &arguments);
        if let Some(mapped) = self.instantiate_mapped_alias_sequence(id, symbol, &arguments) {
            self.instantiations.insert((symbol, arguments), mapped);
            return mapped;
        }
        id
    }

    /// `getGlobalNonNullableTypeInstantiation` (`checker.go:31207`). Preserve
    /// the global alias when it names an intersection, retaining its operands
    /// so subsequent instantiation operates on types rather than printed text.
    pub(crate) fn get_global_non_nullable_type_instantiation(&mut self, t: TypeId) -> TypeId {
        // Unresolved names represent error-any with a reusable written node.
        // Their semantic nullability cannot improve; retain that print carrier.
        if self.unresolved_types.contains(&t) {
            return t;
        }
        let Some(symbol) = self.global_type_symbol_with_arity("NonNullable", 1) else {
            return self.get_intersection_type(&[t, self.intrinsics.empty_object], None);
        };
        if let Some(&cached) = self.instantiations.get(&(symbol, vec![t])) {
            return cached;
        }
        let Some(evaluated) = self.evaluate_alias_body(symbol, &[t]) else {
            return self.intrinsics.error;
        };
        let result = if let crate::types::TypeData::Intersection { types, .. } =
            self.store.get(evaluated).data.clone()
        {
            let text = self.type_reference_text(symbol, &[t]);
            let named = self.store.intern_intersection(
                TypeFlags::INTERSECTION,
                crate::types::TypeData::Intersection { types, text, symbol: Some(symbol) },
            );
            self.type_reference_targets.insert(named, (symbol, vec![t]));
            named
        } else {
            evaluated
        };
        self.instantiations.insert((symbol, vec![t]), result);
        result
    }

    /// How an instantiated reference prints: `C<number>`, or `T[]` when the
    /// target is the global `Array`.
    ///
    /// `typeReferenceToTypeNode` (`nodebuilderimpl.go:2977`) special-cases
    /// `globalArrayType` and `globalReadonlyArrayType` before anything else, so
    /// the shorthand is a property of the **target**, not of how the type was
    /// written. `Array<Base>` prints `Base[]`.
    pub(crate) fn type_reference_text(&mut self, symbol: SymbolId, arguments: &[TypeId]) -> String {
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
        let text = if element == self.intrinsics.error { "any".to_owned() }
            else { crate::printing::type_to_string(self.store.get(element)) };
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
        #[cfg(feature = "work-trace")]
        let _work = self.trace_symbol_work(crate::work_trace::Operation::DeclaredTypeQuery, symbol);
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
        } else if flags.contains(SymbolFlags::ENUM_MEMBER) {
            self.get_declared_type_of_enum_member(symbol)
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
    /// Constant members carry their string/numeric value and enum identity,
    /// matching getEnumLiteralType (checker.go:25362). Equal values within one
    /// enum reuse the first member's type. The sequential evaluator supports
    /// prior same-enum references; unsupported expressions retain computed-enum
    /// types. See docs/architecture/checker-99-enum-literals.md.
    fn get_declared_type_of_enum(&mut self, symbol: SymbolId) -> TypeId {
        // §55 (`checker-notes-narrow.md`): the sequential constant folder.
        // `None` = computed; auto-increment dies after a string or computed
        // predecessor, per the language.
        #[derive(Clone, PartialEq)]
        enum MemberValue {
            Num(f64),
            Str(String),
        }
        // jsnum.Number.toInt32 (jsnum.go:52): truncate, then wrap modulo
        // 2^32. Rust's float-to-int cast saturates instead and is not the
        // language's conversion. Non-finite values map to zero.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap,
            reason = "ECMAScript ToInt32 truncates, wraps modulo 2^32, and reinterprets the sign bit"
        )]
        fn to_int32(value: f64) -> i32 {
            if !value.is_finite() {
                return 0;
            }
            value.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
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
                tsr_ast::Expression::NoSubstitutionTemplateLiteral(s) => {
                    Some(MemberValue::Str(s.text.to_string()))
                }
                // evaluator.NewEvaluator always skips outer parentheses,
                // but deliberately does not skip assertions/satisfies.
                tsr_ast::Expression::ParenthesizedExpression(p) => {
                    eval(p.expression.as_ref()?, enum_name, folded)
                }
                tsr_ast::Expression::PrefixUnaryExpression(u) => {
                    let inner =
                        u.operand.as_ref().and_then(|operand| eval(operand, enum_name, folded))?;
                    match (&inner, u.operator.kind) {
                        (MemberValue::Num(n), SyntaxKind::MinusToken) => Some(MemberValue::Num(-n)),
                        (MemberValue::Num(n), SyntaxKind::PlusToken) => Some(MemberValue::Num(*n)),
                        (MemberValue::Num(n), SyntaxKind::TildeToken) => {
                            Some(MemberValue::Num(f64::from(!to_int32(*n))))
                        }
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
                    let (ia, ib) = (to_int32(*a), to_int32(*b));
                    // The shift count is masked to 0..=31 first, so the `u32`
                    // conversion cannot lose a sign. `>>>` reinterprets its
                    // left operand as unsigned and retains the unsigned result.
                    #[expect(
                        clippy::cast_sign_loss,
                        reason = "ECMAScript shift semantics: masked count and unsigned reinterpretation"
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
                            f64::from((ia as u32) >> ((ib & 31) as u32))
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
                // evaluateEntity (checker.go:24060) accepts only a written
                // string-literal-like element name, not a computed key value.
                tsr_ast::Expression::ElementAccessExpression(access) => {
                    let Some(tsr_ast::Expression::Identifier(receiver)) = access.expression else {
                        return None;
                    };
                    if receiver.text != enum_name {
                        return None;
                    }
                    let member_name = match access.argument_expression {
                        Some(tsr_ast::Expression::StringLiteral(s)) => s.text,
                        Some(tsr_ast::Expression::NoSubstitutionTemplateLiteral(s)) => s.text,
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
                let literal_data = |text: String| match &value {
                    Some(MemberValue::Num(number)) => (
                        TypeFlags::ENUM_LITERAL | TypeFlags::NUMBER_LITERAL,
                        crate::types::TypeData::EnumLiteral {
                            value: crate::types::EnumLiteralValue::Number(if number.is_finite() {
                                number.to_string()
                            } else {
                                tsr_core::jsnum::format_number(*number)
                            }),
                            owner: symbol,
                            member: member_symbol,
                            text,
                        },
                    ),
                    Some(MemberValue::Str(string)) => (
                        TypeFlags::ENUM_LITERAL | TypeFlags::STRING_LITERAL,
                        crate::types::TypeData::EnumLiteral {
                            value: crate::types::EnumLiteralValue::String(string.clone()),
                            owner: symbol,
                            member: member_symbol,
                            text,
                        },
                    ),
                    None => {
                        (TypeFlags::ENUM, crate::types::TypeData::Named { text, members: None })
                    }
                };
                // §55.1: the single-member split mints DIVERGENT twins —
                // regular spelled as the enum, fresh spelled per-name — and
                // registers the access-road swap.
                if total_members == 1 {
                    let (flags, data) = literal_data(name.clone());
                    let regular = if value.is_some() {
                        self.store.intern_literal(flags, data, false)
                    } else {
                        self.store.new_named(flags, name.clone(), None)
                    };
                    if let Some(value) = &value {
                        self.enum_value_types.insert((symbol, canonical(value)), regular);
                    }
                    let per_name = if identifier_like {
                        format!("{name}.{member_name}")
                    } else {
                        format!("(typeof {name})[{}]", crate::printing::quote(&member_name))
                    };
                    let (flags, data) = literal_data(per_name);
                    let fresh = self.store.intern_literal(flags, data, true);
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
                let (flags, data) = literal_data(member_text.clone());
                let member_type = if value.is_some() {
                    self.store.intern_literal(flags, data, false)
                } else {
                    self.store.new_named(flags, member_text, None)
                };
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
        // getDeclaredTypeOfEnum uses the single regular literal directly.
        // Display metadata can differ between its member declaration and the
        // enum reference, but a one-value enum is not a semantic union wrapper.
        if members.len() == 1 {
            let single = members[0];
            if total_members == 1 {
                return single;
            }
            let ty = self.store.get(single).clone();
            let mut data = ty.data;
            match &mut data {
                crate::types::TypeData::EnumLiteral { text, .. }
                | crate::types::TypeData::Named { text, .. } => *text = name,
                _ => unreachable!("enum members have literal or computed enum payloads"),
            }
            let enum_type = self.store.intern_literal(ty.flags, data, false);
            let fresh = self.get_fresh_type_of_literal_type(single);
            self.enum_member_owners.insert(enum_type, symbol);
            self.enum_member_regular.insert(single, enum_type);
            self.enum_member_regular.insert(fresh, enum_type);
            self.enum_access_spelling.insert(single, enum_type);
            self.enum_access_spelling.insert(fresh, enum_type);
            return enum_type;
        }
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
            // Native 5b1047d1 getDeclaredTypeOfTypeAlias (checker.go:23837)
            // publishes the body before recording its own ordered parameters.
            // Keywords reuse Checker-owned intrinsics, never an alias-tagged
            // type. The existing declared_types owner and resolution frame
            // supply completion; local parameter/default syntax and the
            // existing (SymbolId, ordered arguments) instantiation keys remain
            // authoritative. No body evaluator or TypeId side-table tag runs.
            if let Some(body) = self.original_generic_keyword_alias_body(symbol) {
                if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
                    return error;
                }
                let resolved = self.get_type_from_type_node(body);
                return if self.resolutions.pop() { resolved } else { error };
            }
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
            // §957: a GENERIC alias whose body is a REST-BEARING tuple prints the
            // STRUCTURE, not `Name<Params>`. Same normalisation axis §956 derived
            // for the non-generic case, and the oracle splits generic
            // tuple-bodied aliases the same way:
            //
            // ```
            // type Foo<T, U> = [T, U]                >Foo : Foo<T, U>              NAME
            // type V0<…> = [A, B?, ...T, ...C[]]     >V0 : [A, (B | undefined)?, ...T, ...C[]]   STRUCT
            // type V1<…> = [A, ...T, B, ...C[], D]   >V1 : [A, ...T, B, ...C[], D]  STRUCT
            // type TV0<T extends unknown[]> = [string, ...T]   >TV0 : [string, ...T]  STRUCT
            // ```
            //
            // A plain tuple is interned WITH the alias; a rest-bearing one is built
            // by `createNormalizedTupleType`, whose normalisation creates a
            // different type that the alias is not on. **Newly reachable because
            // §956 made these bodies resolve at all** — before it, `[string, ...T]`
            // beside an optional element was `errorType`, so this arm had nothing
            // to return.
            if let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
                && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
                && let Some(TypeNode::TupleTypeNode(body)) = alias.r#type
                && body.elements.iter().any(|e| matches!(e, TypeNode::RestTypeNode(_)))
            {
                let structural = self.get_type_from_type_node(TypeNode::TupleTypeNode(body));
                if structural != error {
                    return structural;
                }
            }
            // getDeclaredTypeOfTypeAlias (checker.go:23837) declares the body
            // type AS BUILT, and the alias lands only through a constructor
            // that receives `getAliasForTypeNode` (ADR-0045). A template
            // literal (getTypeFromTemplateTypeNode), a `keyof` operator
            // (getTypeFromTypeOperatorNode) and a type query never receive
            // one, so `type Stringify<T extends string> = \`${T}\`` records
            // `>Stringify : \`${T}\`` and `type KeyOf<T> = keyof T` records
            // `>KeyOf : keyof T` (`templateLiteralTypes8`). A body this port
            // cannot resolve keeps the name mint below.
            if let Some(body) = self.alias_free_generic_alias_body(symbol) {
                if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
                    return error;
                }
                let resolved = self.get_type_from_type_node(body);
                if !self.resolutions.pop() {
                    return self.report_type_alias_circularity(symbol);
                }
                if resolved != error {
                    return resolved;
                }
            }
            // A homomorphic mapping that normalizes to an array or tuple
            // creates the normalized type without the enclosing alias identity
            // (instantiateMappedType -> createNormalizedTupleType, checker.go).
            if let Some(body) = self.mapped_alias_reference_body(symbol)
                && self.variadic_alias_in_progress.insert(symbol)
            {
                let resolved = self.get_type_from_type_node(body);
                self.variadic_alias_in_progress.remove(&symbol);
                if self.is_normalized_mapped_sequence(resolved) {
                    return resolved;
                }
            }
            // getDeclaredTypeOfTypeAlias (checker.go:23837) resolves a GENERIC
            // alias's body under the same push/pop frame as any other: a body
            // that reaches the alias again eagerly fails the pop, reports
            // TS2456 and declares `errorType` (`type T1<in in> = T1` records
            // `>T1 : any`, `varianceAnnotationsWithCircularlyReferencesError`).
            // A mention inside a construct native resolves lazily answers the
            // name mint, upstream's laziness at the §29 seam. The resolved body
            // is otherwise discarded: the declared type stays the mint below
            // until ADR-0045 rule 4 is built for these bodies.
            let name = self.binder.symbols().get(symbol).name.to_string();
            let mint = |checker: &mut Self| {
                checker.store.new_named(
                    TypeFlags::OBJECT,
                    format!("{name}<{}>", parameters.join(", ")),
                    None,
                )
            };
            if self.resolutions.deferred_since(symbol, PropertyName::DeclaredType) {
                return mint(self);
            }
            if let Some(body) = self.type_alias_body(symbol) {
                if !self.resolutions.push(symbol, PropertyName::DeclaredType) {
                    return error;
                }
                let resolved = self.get_type_from_type_node(body);
                if !self.resolutions.pop() {
                    return self.report_type_alias_circularity(symbol);
                }
                if resolved != error {
                    if matches!(body, TypeNode::IntersectionTypeNode(_))
                        && let crate::types::TypeData::Intersection { types, .. } = self.store.get(resolved).data.clone()
                    {
                        let text = format!("{name}<{}>", parameters.join(", "));
                        let image = self.store.intern_intersection(TypeFlags::INTERSECTION,
                            crate::types::TypeData::Intersection { types, text, symbol: Some(symbol) });
                        let arguments = self.local_type_parameter_types_of(symbol)
                            .map(|parameters| parameters.into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
                        self.alias_of.insert(image, (symbol, arguments));
                        return image;
                    }
                    return resolved;
                }
            }
            return mint(self);
        }
        let Some(type_node) = self.type_alias_body(symbol) else { return error };
        // getDeclaredTypeOfTypeAlias / getBuiltinIteratorReturnType
        // (checker.go:23857,6400): this intrinsic follows the strict-family
        // option, independently of strictNullChecks.
        if matches!(type_node, TypeNode::KeywordTypeNode(keyword)
            if keyword.kind == SyntaxKind::IntrinsicKeyword)
            && self.binder.symbols().get(symbol).name == "BuiltinIteratorReturn"
        {
            return if self.strict_builtin_iterator_return {
                self.intrinsics.undefined
            } else {
                self.intrinsics.any
            };
        }
        // §29: a mention of the alias inside a construct native resolves
        // lazily (`Resolutions::deferred_since`) answers the memoized NAME
        // placeholder — upstream's laziness, at the one seam print-at-creation
        // permits. No failure marking, so the outer resolution completes. Any
        // other re-entry is native's cycle: the push below fails every frame
        // from the alias onward.
        if self.resolutions.deferred_since(symbol, PropertyName::DeclaredType) {
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
                return self.report_type_alias_circularity(symbol);
            }
            let name = self.binder.symbols().get(symbol).name.to_string();
            let members = type_node.node_id().and_then(|id| self.binder.symbol_of(id));
            return self.store.new_named(TypeFlags::OBJECT, name, members);
        }
        let resolved = self.get_type_from_type_node(type_node);
        if !self.resolutions.pop() {
            // A cycle closed below this frame, so the answer above was built on a
            // partial one.
            return self.report_type_alias_circularity(symbol);
        }
        resolved
    }

    /// getDeclaredTypeOfTypeAlias's failed-pop arm (`checker.go:23837`):
    /// "Type alias '{0}' circularly references itself" at the declaration's
    /// name, answering `errorType`. Each failed frame reports once; the
    /// memoized `declared_types` entry keeps the alias from failing again.
    fn report_type_alias_circularity(&mut self, symbol: SymbolId) -> TypeId {
        use tsr_diagnostics::{Diagnostic, messages};
        let error = self.intrinsics.error;
        let Some(declaration) =
            self.binder.symbols().get(symbol).declarations.iter().copied().find(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::TypeAliasDeclaration
                        | SyntaxKind::JSDocTypedefTag
                        | SyntaxKind::JSDocCallbackTag
                )
            })
        else {
            return error;
        };
        let at = self.declaration_name_of(declaration).unwrap_or(declaration);
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return error };
        if !self.circularity_reported.insert(at) {
            return error;
        }
        let name = self.binder.symbols().get(symbol).name.to_string();
        self.report(
            file,
            Diagnostic::with_args(
                &messages::TYPE_ALIAS_0_CIRCULARLY_REFERENCES_ITSELF,
                self.error_span(at),
                [name],
            ),
        );
        error
    }

    /// A generic alias body whose type constructor never takes an alias
    /// symbol: a template literal, `keyof X`, or `typeof x` (parentheses
    /// transparent). The declared type of such an alias is the body itself.
    fn alias_free_generic_alias_body(&self, symbol: SymbolId) -> Option<TypeNode<'a>> {
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let mut body = alias.r#type?;
        while let TypeNode::ParenthesizedTypeNode(inner) = body {
            body = inner.r#type?;
        }
        match body {
            TypeNode::TemplateLiteralTypeNode(_) | TypeNode::TypeQueryNode(_) => Some(body),
            TypeNode::TypeOperatorNode(operator)
                if operator.operator.kind == SyntaxKind::KeyOfKeyword =>
            {
                Some(body)
            }
            _ => None,
        }
    }

    /// The position of the alias's own type parameter that IS its body
    /// (`type Id<T> = T` → `Some(0)`), the shape §282 answers with that
    /// parameter as the declared type. Parentheses are transparent, as in
    /// `getTypeFromTypeNode`. `None` for every other body.
    fn type_parameter_body_index(&self, symbol: SymbolId) -> Option<usize> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let mut body = alias.r#type?;
        while let TypeNode::ParenthesizedTypeNode(inner) = body {
            body = inner.r#type?;
        }
        let TypeNode::TypeReferenceNode(reference) = body else { return None };
        if !reference.type_arguments.is_empty() {
            return None;
        }
        let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
            return None;
        };
        alias.type_parameters.iter().position(|parameter| {
            parameter.name.is_some_and(|parameter_name| parameter_name.text == name.text)
        })
    }

    /// The original, unmapped keyword body of one uniquely bound TS alias.
    /// This read-only preflight never resolves annotations, captures, references
    /// or cycles. Parentheses are the only transparent syntax admitted here;
    /// explicit unions keep the alias identity of their own construction.
    fn original_generic_keyword_alias_body(&self, symbol: SymbolId) -> Option<TypeNode<'a>> {
        if !self.alias_evaluation_bindings.is_empty()
            || self.mapped_template_depth != 0
            || self.resolutions.on_stack(symbol, PropertyName::DeclaredType)
        {
            return None;
        }
        let entry = self.binder.symbols().get(symbol);
        let [declaration] = entry.declarations.as_slice() else { return None };
        if !entry.flags.contains(SymbolFlags::TYPE_ALIAS)
            || self.binder.symbol_of(*declaration) != Some(symbol)
        {
            return None;
        }
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(*declaration) else {
            return None;
        };
        if alias.type_parameters.is_empty()
            || !alias.type_parameters.iter().all(|parameter| {
                let Some(node) = parameter.node_id else { return false };
                let Some(owner) = self.binder.symbol_of(node) else { return false };
                let parameter = self.binder.symbols().get(owner);
                parameter.flags.contains(SymbolFlags::TYPE_PARAMETER)
                    && parameter.declarations.as_slice() == [node]
                    && self.nodes.parent(node) == Some(*declaration)
            })
        {
            return None;
        }
        let mut body = alias.r#type?;
        while let TypeNode::ParenthesizedTypeNode(parenthesized) = body {
            body = parenthesized.r#type?;
        }
        let TypeNode::KeywordTypeNode(keyword) = body else { return None };
        matches!(
            keyword.kind,
            SyntaxKind::AnyKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::ObjectKeyword
        )
        .then_some(body)
    }

    /// The body of a written or JSDoc type alias.
    ///
    /// Native reparses `@typedef {T} Name` and `@callback Name` as a
    /// `JSTypeAliasDeclaration` before binding (`reparseUnhosted`,
    /// `parser/reparser.go:70`), whose `Type` is the typedef's written type,
    /// its reparsed `@property` literal, or the callback's reparsed function
    /// type; `getDeclaredTypeOfTypeAlias` reads it through the same `Type()`
    /// field as an ordinary alias. This port's JSDoc parser already stores
    /// that reparsed body on the tag, so this is the equivalent projection at
    /// the checker boundary.
    pub(crate) fn type_alias_body(&self, symbol: SymbolId) -> Option<TypeNode<'a>> {
        let declarations = &self.binder.symbols().get(symbol).declarations;
        // A written alias wins in a merged symbol. Native's reparser creates a
        // JSTypeAliasDeclaration, but its checker still finds the ordinary
        // TypeAliasDeclaration body first for this merged shape.
        for &declaration in declarations {
            if let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) {
                return alias.r#type;
            }
        }
        // Same-named JSDoc aliases in distinct function scopes are currently
        // merged in file locals by the direct binder. Do not pick one body for
        // all of them; their existing initializer fallback is scope-correct.
        let [declaration] = declarations.as_slice() else { return None };
        match self.node_map.get(*declaration)? {
            Node::JSDocTypedefTag(typedef) => match typedef.type_expression? {
                Node::JSDocTypeExpression(expression) => expression.r#type,
                node => TypeNode::try_from(node).ok(),
            },
            Node::JSDocCallbackTag(callback) => callback.type_expression,
            _ => None,
        }
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
        //
        // The FIRST NAMED declaration, not the first declaration:
        // `getNameOfSymbolAsWritten` (`nodebuilderimpl.go:973`) takes
        // `core.FirstNonNil(symbol.Declarations, ast.GetNameOfDeclaration)`.
        // `export default function() {}` beside `export default interface A
        // {}` merges into one `default` whose first declaration is nameless.
        let symbols = self.binder.symbols();
        let declared_name = symbols
            .get(symbol)
            .declarations
            .iter()
            .find_map(|&declaration| self.node_map.get(declaration)?.name_id())
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

    /// Retain deferred conditional branches under the reference's mapper.
    /// `getDefaultConstraintOfConditionalType` consumes these semantic branches,
    /// rather than the alias declaration's uninstantiated parameter identities.
    pub(crate) fn capture_conditional_alias_branches(
        &mut self,
        id: TypeId,
        symbol: SymbolId,
        arguments: &[TypeId],
    ) {
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return;
        };
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return;
        };
        let Some(TypeNode::ConditionalTypeNode(conditional)) = alias.r#type else { return };
        let (Some(yes), Some(no)) = (conditional.true_type, conditional.false_type) else { return };
        let parameters = self.local_type_parameters_of(symbol);
        if parameters.len() != arguments.len() || self.instantiation_depth == 100 {
            return;
        }
        let mut frame = rustc_hash::FxHashMap::default();
        for (parameter, &argument) in parameters.iter().zip(arguments) {
            let Some(symbol) = parameter.node_id.and_then(|node| self.binder.symbol_of(node))
            else {
                return;
            };
            frame.insert(symbol, argument);
        }
        // getConstraintOfDistributiveConditionalType (checker.go:17286).
        // A constrained naked check is re-instantiated at its constraint before
        // considering the default branch union. Never falls back to that union.
        if let Some(check) =
            conditional.check_type.and_then(|node| self.distributive_conditional_parameter(node))
            && let Some(&argument) = frame.get(&check)
            && let Some(constraint) = self.base_constraint_of_type(argument)
            && constraint != argument
            && let Some(position) = parameters.iter().position(|parameter| {
                parameter.node_id.and_then(|node| self.binder.symbol_of(node)) == Some(check)
            })
        {
            let mut constrained_arguments = arguments.to_vec();
            constrained_arguments[position] = constraint;
            if let Some(constrained) =
                self.evaluate_conditional_alias(symbol, &constrained_arguments, None)
                && constrained != self.intrinsics.never
            {
                self.conditional_constraint_branches.insert(id, (constrained, constrained));
                return;
            }
        }
        self.instantiation_depth += 1;
        self.alias_evaluation_bindings.push(frame);
        let yes = self.get_type_from_type_node(yes);
        let no = self.get_type_from_type_node(no);
        self.alias_evaluation_bindings.pop();
        self.instantiation_depth -= 1;
        if yes != self.intrinsics.error && no != self.intrinsics.error {
            self.conditional_constraint_branches.insert(id, (yes, no));
        }
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
    /// Evaluate decidable checks, inferred parameters and distributed unions
    /// under the alias mapper. Deferred checks retain the caller's fallback.
    /// `result_alias` supplies mapTypeWithAlias's enclosing declaration name.
    pub(crate) fn evaluate_conditional_alias(
        &mut self,
        symbol: SymbolId,
        arguments: &[TypeId],
        result_alias: Option<SymbolId>,
    ) -> Option<TypeId> {
        if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
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
        let conditional = match alias.r#type {
            Some(TypeNode::ConditionalTypeNode(conditional)) => conditional,
            Some(TypeNode::TypeReferenceNode(reference)) if !parameters.is_empty() => {
                return self.evaluate_conditional_alias_reference(reference, frame, result_alias);
            }
            _ => return None,
        };
        self.instantiation_depth += 1;
        self.alias_evaluation_bindings.push(frame);
        let result = self.evaluate_conditional_node(conditional, result_alias);
        self.alias_evaluation_bindings.pop();
        self.instantiation_depth -= 1;
        if let Some(evaluated) = result {
            self.alias_evaluated_types.insert(evaluated);
        }
        result
    }

    /// Whether a type alias's declared type is a conditional type: its body is
    /// a conditional, or a reference to another generic alias whose declared
    /// type is (the chain [`Checker::evaluate_conditional_alias_reference`]
    /// evaluates).
    pub(crate) fn alias_declares_conditional(&self, mut symbol: SymbolId) -> bool {
        for _ in 0..100 {
            if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS) {
                return false;
            }
            let Some(Node::TypeAliasDeclaration(alias)) = self
                .binder
                .symbols()
                .get(symbol)
                .declarations
                .first()
                .and_then(|&declaration| self.node_map.get(declaration))
            else {
                return false;
            };
            match alias.r#type {
                Some(TypeNode::ConditionalTypeNode(_)) => return true,
                Some(TypeNode::TypeReferenceNode(reference))
                    if !alias.type_parameters.is_empty() =>
                {
                    let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
                        return false;
                    };
                    let Some(target) = name.node_id.and_then(|id| {
                        self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            id,
                            name.text,
                            SymbolFlags::TYPE,
                        )
                    }) else {
                        return false;
                    };
                    if self.local_type_parameters_of(target).is_empty() {
                        return false;
                    }
                    symbol = target;
                }
                _ => return false,
            }
        }
        false
    }

    /// A generic alias whose body is a reference to another alias:
    /// `type IsString<T> = Extends<T, string>`.
    ///
    /// getTypeFromTypeAliasReference (checker.go:23580) gives the outer alias
    /// the inner alias's instantiation as its declared type, so when that inner
    /// body is conditional the declared type is a deferred conditional whose
    /// root is the inner declaration. getTypeAliasInstantiation (checker.go:23641)
    /// then instantiates it under the outer mapper, and
    /// getConditionalTypeInstantiation (checker.go:22485) receives the caller's
    /// alias unchanged. This walks the same chain over syntax: the reference's
    /// arguments resolve under `frame`, missing tail arguments take their
    /// defaults under the preceding ones (fillMissingTypeArguments,
    /// checker.go:21954), and the inner alias evaluates with `result_alias`.
    fn evaluate_conditional_alias_reference(
        &mut self,
        reference: &tsr_ast::TypeReferenceNode<'a>,
        frame: rustc_hash::FxHashMap<SymbolId, TypeId>,
        result_alias: Option<SymbolId>,
    ) -> Option<TypeId> {
        let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else { return None };
        let target = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name.node_id?,
            name.text,
            SymbolFlags::TYPE,
        )?;
        if !self.binder.symbols().get(target).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let declarations = self.local_type_parameters_of(target);
        if declarations.is_empty() || reference.type_arguments.len() > declarations.len() {
            return None;
        }
        let error = self.intrinsics.error;
        self.instantiation_depth += 1;
        self.alias_evaluation_bindings.push(frame);
        let mut arguments: Vec<TypeId> = reference
            .type_arguments
            .iter()
            .map(|&argument| self.get_type_from_type_node(argument))
            .collect();
        self.alias_evaluation_bindings.pop();
        let mut complete = !arguments.contains(&error);
        if complete && arguments.len() < declarations.len() {
            let mut defaults = rustc_hash::FxHashMap::default();
            for (declaration, &argument) in declarations.iter().zip(&arguments) {
                match declaration.node_id.and_then(|id| self.binder.symbol_of(id)) {
                    Some(parameter) => {
                        defaults.insert(parameter, argument);
                    }
                    None => complete = false,
                }
            }
            for declaration in &declarations[arguments.len()..] {
                let (Some(default), Some(parameter), true) = (
                    declaration.default_type,
                    declaration.node_id.and_then(|id| self.binder.symbol_of(id)),
                    complete,
                ) else {
                    complete = false;
                    break;
                };
                self.alias_evaluation_bindings.push(defaults.clone());
                let filled = self.get_type_from_type_node(default);
                self.alias_evaluation_bindings.pop();
                if filled == error {
                    complete = false;
                    break;
                }
                defaults.insert(parameter, filled);
                arguments.push(filled);
            }
        }
        let result = if complete {
            self.evaluate_conditional_alias(target, &arguments, result_alias)
        } else {
            None
        };
        self.instantiation_depth -= 1;
        result
    }

    /// getTrueTypeFromConditionalType/getFalseTypeFromConditionalType for
    /// inference. Unlike a conditional's default constraint, these branches
    /// retain the original check parameter rather than its base constraint.
    /// Resolve only the branches: reading extends again can eagerly expand a
    /// recursive infer target before its instantiation guard observes a cycle.
    pub(crate) fn conditional_inference_branches(
        &mut self,
        id: TypeId,
    ) -> Option<(TypeId, TypeId)> {
        if let Some(&branches) = self.mapped_conditional_branches.get(&id) {
            return Some(branches);
        }
        self.with_conditional_inference_node(id, |checker, node| {
            Some((
                checker.get_type_from_type_node(node.true_type?),
                checker.get_type_from_type_node(node.false_type?),
            ))
        })
    }

    /// Read a conditional root under its original mapper without evaluating it.
    /// Branch inference and conditional-source matching share this alias walk.
    fn with_conditional_inference_node<R>(
        &mut self,
        id: TypeId,
        read: impl FnOnce(&mut Self, &'a tsr_ast::ConditionalTypeNode<'a>) -> Option<R>,
    ) -> Option<R> {
        if self.instantiation_depth == 100 {
            return None;
        }
        let (body, bindings) = if let Some(info) = self.conditional_inference_nodes.get(&id) {
            let Some(Node::ConditionalTypeNode(node)) = self.node_map.get(info.declaration) else {
                return None;
            };
            (TypeNode::ConditionalTypeNode(node), info.bindings.clone())
        } else {
            let (symbol, arguments) = self.type_reference_targets.get(&id)?.clone();
            if !self.alias_declares_conditional(symbol) {
                return None;
            }
            let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
            let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
                return None;
            };
            let parameters = self.local_type_parameters_of(symbol);
            if parameters.len() != arguments.len() {
                return None;
            }
            let mut bindings = rustc_hash::FxHashMap::default();
            for (parameter, argument) in parameters.iter().zip(arguments) {
                bindings.insert(
                    parameter.node_id.and_then(|node| self.binder.symbol_of(node))?,
                    argument,
                );
            }
            (alias.r#type?, bindings)
        };
        self.instantiation_depth += 1;
        self.alias_evaluation_bindings.push(bindings);
        let result = match body {
            TypeNode::ConditionalTypeNode(node) => read(self, node),
            // This reference is an alias body, where the ordinary resolver
            // deliberately gaps deferred conditionals. Preserve its reference
            // mapper instead of attempting to evaluate that conditional again.
            TypeNode::TypeReferenceNode(reference) => (|| {
                let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
                    return None;
                };
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    name.node_id?,
                    name.text,
                    SymbolFlags::TYPE,
                )?;
                let arguments = reference
                    .type_arguments
                    .iter()
                    .map(|&argument| self.get_type_from_type_node(argument))
                    .collect();
                let target = self.create_type_reference(symbol, arguments);
                self.with_conditional_inference_node(target, read)
            })(),
            _ => None,
        };
        self.alias_evaluation_bindings.pop();
        self.instantiation_depth -= 1;
        result
    }

    /// inferToConditionalType pairs the four operands of two deferred roots.
    /// Only read extends when the ordinary evaluator deferred its check before
    /// reaching that operand; concrete-check roots can already have resolved a
    /// recursive infer target and must not resolve it a second time here.
    pub(crate) fn conditional_inference_operands(&mut self, id: TypeId) -> Option<[TypeId; 4]> {
        if let Some(info) = self.mapped_conditionals.get(&id) {
            return Some(info.operands);
        }
        self.with_conditional_inference_node(id, |checker, node| {
            let check = checker.get_type_from_type_node(node.check_type?);
            if !checker.conditional_check_is_deferred(node, check) {
                return None;
            }
            Some([
                check,
                checker.get_type_from_type_node(node.extends_type?),
                checker.get_type_from_type_node(node.true_type?),
                checker.get_type_from_type_node(node.false_type?),
            ])
        })
    }

    /// getConditionalTypeInstantiation maps a retained ordinary conditional's
    /// outer bindings before evaluating its root. Keep the existing evaluator's
    /// distribution and infer-target ordering rather than reading extends here.
    pub(crate) fn instantiate_conditional_node(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let Some(info) = self.conditional_inference_nodes.get(&id).cloned() else {
            return self.intrinsics.error;
        };
        let Some(Node::ConditionalTypeNode(node)) = self.node_map.get(info.declaration) else {
            return self.intrinsics.error;
        };
        let mut bindings = info.bindings;
        for value in bindings.values_mut() {
            *value = self.instantiate_type(*value, map, parameters, names);
        }
        for &(parameter, value) in map {
            if let Some(&symbol) = self.type_parameter_symbols.get(&parameter) {
                bindings.insert(symbol, value);
            }
        }
        self.alias_evaluation_bindings.push(bindings);
        // Native roots instantiate in every consumer, not only constraints.
        // If evaluation still defers, mint a semantic CONDITIONAL carrying the
        // composed captured bindings; never publish error for mere deferral.
        let result = self.get_type_from_type_node(TypeNode::ConditionalTypeNode(node));
        self.alias_evaluation_bindings.pop();
        result
    }

    /// getConditionalTypeInstantiation composes the outer mapper before
    /// testing the check type or distributing its substituted constituents.
    pub(crate) fn instantiate_mapped_conditional(
        &mut self,
        id: TypeId,
        map: &[(TypeId, TypeId)],
        parameters: &[TypeId],
        names: &[&str],
    ) -> TypeId {
        let Some(info) = self.mapped_conditionals.get(&id).cloned() else {
            return self.intrinsics.error;
        };
        let Some(Node::ConditionalTypeNode(node)) = self.node_map.get(info.declaration) else {
            return self.intrinsics.error;
        };
        let mut bindings = info.bindings;
        for value in bindings.values_mut() {
            *value = self.instantiate_type(*value, map, parameters, names);
        }
        for &(parameter, value) in map {
            if let Some(&symbol) = self.type_parameter_symbols.get(&parameter) {
                bindings.insert(symbol, value);
            }
        }
        self.alias_evaluation_bindings.push(bindings);
        self.mapped_template_depth += 1;
        let result = self.get_type_from_type_node(TypeNode::ConditionalTypeNode(node));
        self.mapped_template_depth -= 1;
        self.alias_evaluation_bindings.pop();
        result
    }

    /// getConditionalType's branch walk under an active alias/infer mapper.
    fn evaluate_conditional_node(
        &mut self,
        conditional: &tsr_ast::ConditionalTypeNode<'a>,
        result_alias: Option<SymbolId>,
    ) -> Option<TypeId> {
        // §182 slice 1 (`checker-notes-narrow.md`): the `extends never`
        // shape was the only one evaluated; `getConditionalType`'s
        // non-deferred fast path evaluates ANY conditional whose CHECK type
        // is decidable, choosing a branch by assignability. The general
        // road is taken below; this shape keeps its own `literal_key_texts`
        // reading because keyof-emptiness is not an assignability question.
        let extends_is_never = matches!(conditional.extends_type,
            Some(TypeNode::KeywordTypeNode(keyword)) if keyword.kind == SyntaxKind::NeverKeyword);
        let error = self.intrinsics.error;
        let mut result = None;
        let has_infer_parameters =
            conditional.node_id.and_then(|id| self.binder.locals(id)).is_some_and(|locals| {
                locals.values().any(|&symbol| {
                    self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER)
                })
            });
        if let Some(check_node) = conditional.check_type {
            let check = self.get_type_from_type_node(check_node);
            // getConditionalType (checker.go:24319): errorType propagates
            // before any's two-branch expansion. Unresolved references retain
            // a written name here, but still represent that same errorType.
            // Decline evaluation so the port's reference fallback preserves
            // its written form, as it already does for intrinsic errorType.
            if self.is_error(check) {
                return None;
            }
            // isDeferredType (5b1047d:24475) uses generic object/index flags,
            // plus generic elements only for matching simple tuple syntax.
            if self.conditional_check_is_deferred(conditional, check) {
                return None;
            }
            // getConditionalTypeInstantiation distributes a naked parameter's
            // substituted union, retaining that constituent in the outer mapper
            // for both the extends test and the chosen branch.
            if let Some(parameter) = self.distributive_conditional_parameter(check_node) {
                // getConditionalTypeInstantiation reduces the substituted
                // check before distribution (checker.go:22499). A certified
                // whole-never intersection contributes no branch result.
                if self.store.get(check).flags.contains(TypeFlags::INTERSECTION) {
                    let apparent = self.apparent_type(check);
                    if self.intersection_has_never_discriminant(apparent) {
                        return Some(self.intrinsics.never);
                    }
                }
                if check == self.intrinsics.never {
                    return Some(check);
                }
                if let crate::types::TypeData::Union { types, .. } = &self.store.get(check).data {
                    let types = types.clone();
                    if self.instantiation_depth == 100 {
                        return None;
                    }
                    let mut results = Vec::with_capacity(types.len());
                    for constituent in types {
                        let mut frame = rustc_hash::FxHashMap::default();
                        frame.insert(parameter, constituent);
                        self.alias_evaluation_bindings.push(frame);
                        self.instantiation_depth += 1;
                        let result = self.evaluate_conditional_node(conditional, None);
                        self.instantiation_depth -= 1;
                        self.alias_evaluation_bindings.pop();
                        results.push(result?);
                    }
                    let result = self.get_union_type(&results);
                    // mapTypeWithAlias/getUnionTypeEx attaches the enclosing
                    // alias after reduction, and collapses a single constituent
                    // before considering the alias.
                    if let Some(alias) = result_alias
                        && let crate::types::TypeData::Union { types, .. } =
                            &self.store.get(result).data
                    {
                        let types = types.clone();
                        return Some(self.get_named_union_type(&types, TypeFlags::empty(), alias));
                    }
                    return Some(result);
                }
            }
            if check != error {
                result = self.evaluate_conditional_inference(conditional, check);
            }
            // Inference resolves and instantiates its own extends target. Only
            // resolve the ordinary relation target when there are no infer
            // parameters and inference did not already select a branch;
            // resolving a recursive infer target twice can expand it before
            // the instantiation guard observes the cycle.
            let extends_node = conditional.extends_type?;
            let mut extends = error;
            if result.is_none() && !has_infer_parameters {
                extends = self.get_type_from_type_node(extends_node);
                // getUnionTypeWorker (checker.go:25692) represents a
                // nullable-only union under non-strict null checks with a
                // widening null/undefined intrinsic. This port deliberately
                // has no widening identities, so the ordinary printing road
                // returns errorType. getConditionalType consumes the union only
                // semantically: use the corresponding non-widening intrinsic
                // here, preferring undefined exactly as the native branch does,
                // and leave general union construction alone.
                if extends == error && !self.strict_null_checks {
                    let mut nullable_node = extends_node;
                    while let TypeNode::ParenthesizedTypeNode(parenthesized) = nullable_node {
                        nullable_node = parenthesized.r#type?;
                    }
                    if let TypeNode::UnionTypeNode(union) = nullable_node {
                        let parts: Vec<_> = union
                            .types
                            .iter()
                            .map(|&node| self.get_type_from_type_node(node))
                            .collect();
                        if !parts.is_empty()
                            && parts.iter().all(|&part| {
                                self.store.get(part).flags.intersects(TypeFlags::NULLABLE)
                            })
                        {
                            extends = if parts.contains(&self.intrinsics.undefined) {
                                self.intrinsics.undefined
                            } else {
                                self.intrinsics.null
                            };
                        }
                    }
                }
            }
            // getConditionalType's extraTypes includes the true branch for
            // any and then continues into the false branch. Any/unknown extends
            // types instead select the true branch alone. Infer targets need
            // their context mapper first and retain their existing deferral.
            if !has_infer_parameters
                && check != error
                && self.store.get(check).flags.contains(TypeFlags::ANY)
            {
                if extends == error
                    || self.unresolved_types.contains(&extends)
                    || self.store.get(extends).flags.intersects(
                        TypeFlags::CONDITIONAL
                            | TypeFlags::INDEXED_ACCESS
                            | TypeFlags::INDEX
                            | TypeFlags::SUBSTITUTION,
                    )
                    || self.mentions_any_type_parameter(extends, 2)
                {
                    return None;
                }
                let true_type = self.get_type_from_type_node(conditional.true_type?);
                if true_type == error {
                    return None;
                }
                if self.store.get(extends).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
                    return Some(true_type);
                }
                let false_type = self.get_type_from_type_node(conditional.false_type?);
                if false_type == error {
                    return None;
                }
                return Some(self.get_union_type(&[true_type, false_type]));
            }
            let keys = if extends_is_never { self.literal_key_texts(check) } else { None };
            let keys_decided = keys.is_some();
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
                // An infer target must be related through its inference mapper.
                // Falling back to an unmapped target can select a false branch
                // merely because the structural inference path is incomplete.
                && !has_infer_parameters
                // A literal-key check against `never` was decided above; any
                // other check relates to `never` like any extends type
                // (`number extends never` is false).
                && !(extends_is_never && keys_decided)
                && check != error
            {
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
                // getConditionalType first excludes deferred extends operands;
                // a definite ordinary relation to a generic target cannot pick
                // a branch that must remain open for later instantiations.
                if extends != error
                    && !self.conditional_type_is_deferred(extends,
                        Self::conditional_checks_simple_tuples(conditional))
                    && !self.conditional_check_is_deferred(conditional, check)
                {
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
                        self.definite_conditional_outcome(check, extends)
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
        result
    }

    /// getPermissiveInstantiation/getRestrictiveInstantiation (5b1047d:24479)
    /// over supported semantic images. Both operands use the same ordered map;
    /// names never select parameters. Existing object/signature mapper caches
    /// own expensive substitution. Restrictive clones publish no constraint
    /// only after creation and preserve the original declaration symbol.
    fn definite_conditional_outcome(&mut self, check: TypeId, extends: TypeId) -> Option<bool> {
        let mut parameters: Vec<_> = self.type_parameter_symbols.keys().copied()
            .chain(self.this_types.values().copied())
            .chain(self.literal_this_types.values().copied()).filter(|&parameter| {
            self.mentions_type_parameter(check, &[parameter], &[])
                || self.mentions_type_parameter(extends, &[parameter], &[])
        }).collect();
        parameters.sort_unstable();
        parameters.dedup();
        if parameters.is_empty() {
            return match self.relate_ternary(check, extends, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => Some(true),
                crate::relater::Ternary::NotRelated => Some(false),
                crate::relater::Ternary::Unknown => None,
            };
        }
        let permissive: Vec<_> = parameters.iter().map(|&parameter| (parameter, self.intrinsics.wildcard)).collect();
        let permissive_check = self.instantiate_type(check, &permissive, &parameters, &[]);
        let permissive_extends = self.instantiate_type(extends, &permissive, &parameters, &[]);
        if self.is_error(permissive_check) || self.is_error(permissive_extends) { return None; }
        match self.relate_ternary(permissive_check, permissive_extends, crate::relater::Relation::Assignable) {
            crate::relater::Ternary::NotRelated => return Some(false),
            crate::relater::Ternary::Unknown => return None,
            crate::relater::Ternary::Related => {}
        }
        let restrictive: Vec<_> = parameters.iter().map(|&parameter| {
            let constrained = self.instantiated_type_parameters.contains_key(&parameter)
                || self.type_parameter_symbols.get(&parameter).is_some_and(|symbol| {
                    self.binder.symbols().get(*symbol).declarations.iter().any(|&declaration| {
                        matches!(self.node_map.get(declaration), Some(Node::TypeParameterDeclaration(node))
                            if node.constraint.is_some())
                    })
                });
            let image = if self.restrictive_parameter_instances.contains(&parameter) || !constrained { parameter }
                else if let Some(&image) = self.restrictive_type_parameters.get(&parameter) { image }
                else {
                    let text = self.type_to_string(parameter);
                    let image = self.store.new_named(TypeFlags::TYPE_PARAMETER, text, None);
                    if let Some(&symbol) = self.type_parameter_symbols.get(&parameter) { self.type_parameter_symbols.insert(image, symbol); }
                    self.restrictive_parameter_instances.insert(image);
                    self.restrictive_type_parameters.insert(parameter, image);
                    image
                };
            (parameter, image)
        }).collect();
        let restrictive_check = self.instantiate_type(check, &restrictive, &parameters, &[]);
        let restrictive_extends = self.instantiate_type(extends, &restrictive, &parameters, &[]);
        if self.is_error(restrictive_check) || self.is_error(restrictive_extends) { return None; }
        match self.relate_ternary(restrictive_check, restrictive_extends, crate::relater::Relation::Assignable) {
            crate::relater::Ternary::Related => Some(true),
            _ => None,
        }
    }

    /// isSimpleTupleType / isDeferredType (5b1047d:24469-24476).
    /// Existing semantic classifiers retain their own metadata/publication;
    /// this read adds no cache or graph identity. Tuple syntax only selects the
    /// native element-checking branch, never a printed-name heuristic.
    fn conditional_checks_simple_tuples(node: &tsr_ast::ConditionalTypeNode<'a>) -> bool {
        let tuple = |mut node: TypeNode<'a>| {
            while let TypeNode::ParenthesizedTypeNode(parent) = node {
                let inner = parent.r#type?;
                node = inner;
            }
            let TypeNode::TupleTypeNode(tuple) = node else { return None };
            (!tuple.elements.is_empty() && !tuple.elements.iter().any(|element| match element {
                TypeNode::OptionalTypeNode(_) | TypeNode::RestTypeNode(_) => true,
                TypeNode::NamedTupleMember(member) => member.question_token.is_some()
                    || member.dot_dot_dot_token.is_some(),
                _ => false,
            })).then_some(tuple.elements.len())
        };
        node.check_type.and_then(tuple).zip(node.extends_type.and_then(tuple))
            .is_some_and(|(check, extends)| check == extends)
    }

    fn conditional_type_is_deferred(&mut self, id: TypeId, check_tuples: bool) -> bool {
        if self.indexed_access_index_is_generic(id) || self.indexed_access_object_is_generic(id) {
            return true;
        }
        if check_tuples && let Some((elements, _)) = self.tuple_element_lists.get(&id) {
            let count = elements.len();
            for index in 0..count {
                let element = self.tuple_element_lists[&id].0[index];
                if self.conditional_type_is_deferred(element, false) { return true; }
            }
        }
        false
    }

    fn conditional_check_is_deferred(&mut self, node: &tsr_ast::ConditionalTypeNode<'a>, check: TypeId) -> bool {
        self.conditional_type_is_deferred(check, Self::conditional_checks_simple_tuples(node))
    }

    pub(crate) fn distributive_conditional_parameter(
        &self,
        mut node: TypeNode<'a>,
    ) -> Option<SymbolId> {
        while let TypeNode::ParenthesizedTypeNode(parenthesized) = node {
            node = parenthesized.r#type?;
        }
        let TypeNode::TypeReferenceNode(reference) = node else { return None };
        if !reference.type_arguments.is_empty() {
            return None;
        }
        let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else { return None };
        self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name.node_id?,
            name.text,
            SymbolFlags::TYPE_PARAMETER,
        )
    }

    /// getConditionalType's inference context for a concrete or any check.
    /// Generic checks defer; the outer worker handles union distribution.
    fn evaluate_conditional_inference(
        &mut self,
        conditional: &tsr_ast::ConditionalTypeNode<'a>,
        check: TypeId,
    ) -> Option<TypeId> {
        if self.store.get(check).flags.intersects(TypeFlags::UNION | TypeFlags::NEVER)
            || self.conditional_check_is_deferred(conditional, check)
        {
            return None;
        }
        let symbols: Vec<_> = self
            .binder
            .locals(conditional.node_id?)?
            .values()
            .copied()
            .filter(|&symbol| {
                self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_PARAMETER)
            })
            .collect();
        if symbols.is_empty() {
            return None;
        }
        let parameters: Vec<_> =
            symbols.iter().map(|&symbol| self.get_declared_type_of_symbol(symbol)).collect();
        let constraints: Vec<_> = parameters
            .iter()
            .map(|&parameter| {
                self.type_parameter_constraint(parameter)
                    .or_else(|| self.inferred_type_parameter_constraint(parameter))
                    .map(|constraint| {
                        // getConstraintFromTypeParameter treats an explicit any
                        // constraint as unknown for an ordinary infer parameter.
                        if self.store.get(constraint).flags.contains(TypeFlags::ANY) {
                            self.intrinsics.unknown
                        } else {
                            constraint
                        }
                    })
            })
            .collect();
        // Direct inferred-parameter dependencies are resolved lazily below.
        // Composite dependent constraints still need a general non-fixing
        // mapper; outer alias bindings are already active.
        if constraints.iter().flatten().any(|&constraint| {
            !parameters.contains(&constraint) && self.mentions_any_type_parameter(constraint, 2)
        }) {
            return None;
        }
        let target = self.get_type_from_type_node(conditional.extends_type?);
        if target == self.intrinsics.error || self.unresolved_types.contains(&target) {
            return None;
        }
        // A retained alias reference may denote a recursive array without
        // carrying normalized array arguments. The structural relation cannot
        // prove a failed array match from that incomplete source metadata.
        if self.tuple_spread_array_element(target).is_some()
            && !self.tuple_element_lists.contains_key(&check)
            && !self.variadic_tuple_elements.contains_key(&check)
            && self.tuple_spread_array_element(check).is_none()
            && self.type_reference_targets.get(&check).is_some_and(|(symbol, _)| {
                self.binder.symbols().get(*symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            })
        {
            return None;
        }
        let inferences = self.infer_conditional_parameters(check, target, &parameters);
        let map = self.resolve_conditional_inferences(&inferences, &constraints);
        let names: Vec<_> =
            symbols.iter().map(|&symbol| self.binder.symbols().get(symbol).name).collect();
        let target = self.instantiate_type(target, &map, &parameters, &names);
        if target == self.intrinsics.error {
            return None;
        }
        if self.store.get(check).flags.contains(TypeFlags::ANY) {
            if self.unresolved_types.contains(&target)
                || self.store.get(target).flags.intersects(
                    TypeFlags::CONDITIONAL
                        | TypeFlags::INDEXED_ACCESS
                        | TypeFlags::INDEX
                        | TypeFlags::SUBSTITUTION,
                )
                || self.mentions_any_type_parameter(target, 2)
            {
                return None;
            }
            let frame =
                symbols.into_iter().zip(map.into_iter().map(|(_, inferred)| inferred)).collect();
            self.alias_evaluation_bindings.push(frame);
            let true_type =
                conditional.true_type.map(|branch| self.get_type_from_type_node(branch));
            self.alias_evaluation_bindings.pop();
            let true_type = true_type?;
            if true_type == self.intrinsics.error {
                return None;
            }
            if self.store.get(target).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
                return Some(true_type);
            }
            let false_type = self.get_type_from_type_node(conditional.false_type?);
            if false_type == self.intrinsics.error {
                return None;
            }
            return Some(self.get_union_type(&[true_type, false_type]));
        }
        // Native getConditionalType defers the inferred extends operand,
        // then tests BOTH permissive and BOTH restrictive instantiations.
        if self.conditional_check_is_deferred(conditional, target) { return None; }
        let branch = match self.definite_conditional_outcome(check, target) {
            Some(true) => conditional.true_type?,
            Some(false) => conditional.false_type?,
            None => return None,
        };
        let frame =
            symbols.into_iter().zip(map.into_iter().map(|(_, inferred)| inferred)).collect();
        self.alias_evaluation_bindings.push(frame);
        let result = self.get_type_from_type_node(branch);
        self.alias_evaluation_bindings.pop();
        (result != self.intrinsics.error).then_some(result)
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
        if let Some(evaluated) = self.evaluate_conditional_alias(symbol, arguments, None) {
            self.alias_body_evaluations.insert(key, evaluated);
            self.alias_evaluated_types.insert(evaluated);
            return Some(evaluated);
        }
        let error = self.intrinsics.error;
        let mut result = None;
        if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            && let Some(declaration) =
                self.binder.symbols().get(symbol).declarations.first().copied()
            && let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration)
            && let Some(body) = alias.r#type
            && !matches!(body, TypeNode::ConditionalTypeNode(_))
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
                    // Recursive aliases (`ramdaToolsNoInfinite2`) nest here up
                    // to the 100-level guard, ~27 KiB of debug stack per level:
                    // more than a default 2 MiB thread has. Grow on demand
                    // (ADR-0030) rather than depend on the caller's stack.
                    let evaluated = tsr_core::stack::ensure_sufficient(|| {
                        if let TypeNode::UnionTypeNode(union) = body
                            && self.is_closed_literal_union_alias(symbol, arguments)
                        {
                            // Compute the closed primitive body, not its origin
                            // spelling. The reference factory supplies the exact
                            // source alias and ordered arguments before queries
                            // can print it; the general origin writer is unchanged.
                            let parts: Vec<_> = union
                                .types
                                .iter()
                                .map(|&node| self.get_type_from_type_node(node))
                                .collect();
                            self.get_union_type_unprinted(&parts)
                        } else {
                            self.get_type_from_type_node(body)
                        }
                    });
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

    /// `getIndexTypeEx` and `getLiteralTypeFromProperties` (checker.go), for
    /// substituted keyof operands. Property syntax distinguishes numeric names
    /// from quoted numeric names; index signatures contribute their key types.
    pub(crate) fn resolved_keyof_type(&mut self, target: TypeId) -> Option<TypeId> {
        if !self.index_types_in_progress.insert(target) {
            return None;
        }
        let result = self.resolved_keyof_type_worker(target);
        self.index_types_in_progress.remove(&target);
        result
    }

    fn resolved_keyof_type_worker(&mut self, target: TypeId) -> Option<TypeId> {
        if target == self.intrinsics.error {
            return None;
        }
        if let Some(keys) = self.mapped_index_type(target) {
            return Some(keys);
        }
        // Native alias instantiations expose the body's semantic kind. Keep
        // the reference separately for index origins and deferred alias names;
        // later substitution must run this same query on the rebuilt operand.
        let original = target;
        let target = self.binding_type_alias_body(target);
        if target != original
            && let Some(keys) = self.mapped_index_type(target)
        {
            return Some(keys);
        }
        // shouldDeferIndexType: an instantiable intersection containing an
        // empty anonymous object is one INDEX type, not a union of its keys.
        let deferred_intersection = matches!(&self.store.get(target).data, crate::types::TypeData::Intersection { types, .. }
                if self.maybe_type_of_kind(target, TypeFlags::INSTANTIABLE)
                    && types.iter().any(|&part| self.is_empty_anonymous_object_type(part)));
        if self.store.get(target).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE)
            || deferred_intersection
            || self.is_generic_homomorphic_mapped_type(target)
            || self.mapped_types.get(&target).cloned().is_some_and(|info| {
                info.name_type.is_some()
                    && self.signature_parameter_type_is_generic(info.constraint)
            })
        {
            let operand_type = if deferred_intersection || self.mapped_types.contains_key(&target) {
                original
            } else {
                target
            };
            let operand = self.type_to_string(operand_type);
            let operand = if deferred_intersection
                && self.store.get(operand_type).flags.contains(TypeFlags::INTERSECTION)
                && !crate::printing::prints_as_a_single_token(self.store.get(operand_type))
            {
                format!("({operand})")
            } else {
                operand
            };
            let text = format!("keyof {operand}");
            let id = self.store.new_named(TypeFlags::INDEX, text, None);
            self.deferred_keyof_types.insert(id);
            self.deferred_keyof_operands.insert(id, operand_type);
            self.deferred_index_mints.insert(id);
            return Some(id);
        }
        // getKnownKeysOfTupleType combines fixed positional string keys with
        // the array target's property/index keys (checker.go).
        let tuple = self
            .tuple_element_lists
            .get(&target)
            .map(|(elements, readonly)| (elements.len(), *readonly))
            .or_else(|| {
                self.variadic_tuple_elements.get(&target).map(|(elements, readonly)| {
                    (elements.iter().take_while(|element| !element.spread).count(), *readonly)
                })
            });
        if let Some((fixed, readonly)) = tuple {
            let array =
                self.global_type_symbol(if readonly { "ReadonlyArray" } else { "Array" })?;
            let array = self.create_type_reference(array, vec![self.intrinsics.never]);
            let mut keys = vec![self.resolved_keyof_type(array)?];
            keys.extend((0..fixed).map(|index| {
                self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(index.to_string()),
                    false,
                )
            }));
            return Some(self.get_union_type(&keys));
        }
        if self.store.get(target).flags.contains(TypeFlags::UNKNOWN) {
            return Some(self.intrinsics.never);
        }
        if self.store.get(target).flags.intersects(TypeFlags::ANY | TypeFlags::NEVER) {
            return Some(self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ]));
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(target).data {
            let types = types.clone();
            let mut keys = Vec::with_capacity(types.len());
            for t in types {
                keys.push(self.resolved_keyof_type(t)?);
            }
            return Some(self.get_intersection_type(&keys, None));
        }
        if let crate::types::TypeData::Intersection { types, .. } = &self.store.get(target).data {
            let types = types.clone();
            let mut keys = Vec::with_capacity(types.len());
            for t in types {
                keys.push(self.resolved_keyof_type(t)?);
            }
            return Some(self.get_union_type(&keys));
        }
        let apparent = self.apparent_type(target);
        // resolveReverseMappedTypeMembers (inference.go:1099) copies a source
        // property's declarations and nameType but not its valueDeclaration,
        // so getLiteralTypeFromProperty spells a written numeric name as its
        // string name. Computed names keep their name types.
        let reverse_mapped = self.reverse_placeholder_texts.contains_key(&apparent);
        let names = self.resolved_keyof_property_names(apparent)?;
        let mut keys = Vec::with_capacity(names.len());
        for name in names {
            let property = self.get_property_of_type(apparent, &name)?;
            if !self.binder.symbols().get(property).flags.intersects(SymbolFlags::VALUE) {
                continue;
            }
            let declaration = self
                .binder
                .symbols()
                .get(property)
                .value_declaration
                .or_else(|| self.binder.symbols().get(property).declarations.first().copied());
            let (property_name, modifiers) = match declaration.and_then(|id| self.node_map.get(id))
            {
                Some(Node::PropertySignatureDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::PropertyDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::MethodSignatureDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::MethodDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::GetAccessorDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::SetAccessorDeclaration(p)) => (Some(p.name), p.modifiers),
                Some(Node::PropertyAssignment(p)) => (Some(p.name), &[][..]),
                Some(Node::ShorthandPropertyAssignment(p)) => (Some(p.name), &[][..]),
                _ => (None, &[][..]),
            };
            if modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if matches!(token.kind, SyntaxKind::PrivateKeyword | SyntaxKind::ProtectedKeyword))
            }) {
                continue;
            }
            let key = match property_name {
                Some(tsr_ast::PropertyName::PrivateIdentifier(_)) => continue,
                Some(tsr_ast::PropertyName::NumericLiteral(literal)) if !reverse_mapped => {
                    self.store.intern_literal(
                        TypeFlags::NUMBER_LITERAL,
                        crate::types::TypeData::NumberLiteral(crate::printing::normalise_number(
                            literal.text,
                        )),
                        false,
                    )
                }
                Some(tsr_ast::PropertyName::ComputedPropertyName(computed)) => {
                    let expression = computed.expression?;
                    let t = self.check_expression(expression);
                    if t == self.intrinsics.error {
                        return None;
                    }
                    self.get_regular_type_of_literal_type(t)
                }
                _ => self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(name),
                    false,
                ),
            };
            keys.push(key);
        }
        let enum_object = matches!(self.store.get(apparent).data,
            crate::types::TypeData::Anonymous { symbol, .. }
            if self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::REGULAR_ENUM));
        for info in self.get_index_infos_of_type(apparent)? {
            // The synthetic reverse-mapping index is enumNumberIndexInfo,
            // which getLiteralTypeFromProperties excludes from keyof.
            if enum_object && info.key == self.intrinsics.number {
                continue;
            }
            keys.push(info.key);
            if info.key == self.intrinsics.string {
                keys.push(self.intrinsics.number);
            }
        }
        let union = self.get_union_type(&keys);
        let aliased_object = self.store.get(target).flags.contains(TypeFlags::OBJECT)
            && !self.mapped_types.contains_key(&target)
            && !self.mapped_identity_sources.contains_key(&target)
            && self.type_reference_targets.get(&target).is_some_and(|(symbol, _)| {
                self.binder.symbols().get(*symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
            });
        Some(if aliased_object || self.keyof_origin_applies(target) {
            let text = format!("keyof {}", self.type_to_string(target));
            self.union_with_origin_text(union, text)
        } else {
            union
        })
    }

    fn resolved_keyof_property_names(&mut self, target: TypeId) -> Option<Vec<String>> {
        match self.store.get(target).data {
            crate::types::TypeData::Anonymous { symbol, .. } => {
                let mut named: Vec<_> = self
                    .binder
                    .symbols()
                    .get(symbol)
                    .exports
                    .iter()
                    .filter(|(_, member)| {
                        self.binder.symbols().get(**member).flags.intersects(SymbolFlags::VALUE)
                    })
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
            crate::types::TypeData::Named { members: Some(owner), .. }
                if !self.binder.symbols().get(owner).flags.contains(SymbolFlags::TYPE_ALIAS) =>
            {
                let mut names = Vec::new();
                self.collect_keyof_property_names(owner, &mut names, &mut Vec::new())?;
                Some(names)
            }
            _ => self.keys_of(target),
        }
    }

    fn collect_keyof_property_names(
        &mut self,
        owner: SymbolId,
        names: &mut Vec<String>,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<()> {
        if visiting.len() > 32 || visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let mut own: Vec<_> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|(_, member)| {
                self.binder.symbols().get(**member).flags.intersects(SymbolFlags::VALUE)
            })
            .map(|(name, &member)| {
                (
                    self.binder.symbols().get(member).declarations.first().copied(),
                    (*name).to_string(),
                )
            })
            .collect();
        own.sort();
        for (_, name) in own {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for base in self.base_symbols_of(owner)? {
            self.collect_keyof_property_names(base, names, visiting)?;
        }
        visiting.pop();
        Some(())
    }

    /// §91: the property-name set of a type, in declaration order, or `None`
    /// where enumeration is not computable. Covers member-table types,
    /// intersections (the union of both sides' keys), and `Omit<T, K>` by its
    /// global symbol (keys of `T` minus `K`'s literals — the §45 Record
    /// precedent for special-casing one lib alias).
    fn keys_of(&mut self, id: TypeId) -> Option<Vec<String>> {
        if !self.key_names_in_progress.insert(id) {
            return None;
        }
        let result = self.keys_of_worker(id);
        self.key_names_in_progress.remove(&id);
        result
    }

    fn keys_of_worker(&mut self, id: TypeId) -> Option<Vec<String>> {
        if id == self.intrinsics.error {
            return None;
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned() {
            // §952.1: a homomorphic IDENTITY mapped type's keys are the
            // SOURCE's, because the mapping preserves names — that is what
            // "identity" means here, and the `as`-clause shape that would not
            // preserve them is refused at §952's mint.
            //
            // Without this the alias branch below runs `evaluate_alias_body` on
            // a MAPPED body, which cannot be evaluated, so `keyof Partial<O>`
            // answered `errorType` even after §952 gave the mint a member owner.
            // The owner is not what this road reads; the reference target is.
            if self.mapped_identity_optionality.contains_key(&id)
                && let [source] = arguments.as_slice()
            {
                return self.keys_of(*source);
            }
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
        let mut keys: Vec<_> = named.into_iter().map(|(_, name)| name).collect();
        for base in self.base_symbols_of_ex(owner, false)? {
            let base = self.get_declared_type_of_symbol(base);
            for key in self.keys_of(base)? {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        Some(keys)
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
            // `gatherTypeParameters(jsDoc, typedefOrCallback=true)`
            // (`parser/reparser.go:293`): the comment's `@template` tags.
            Some(Node::JSDocTypedefTag(_) | Node::JSDocCallbackTag(_)) => self
                .jsdoc_alias_doc(symbol)
                .and_then(|doc| {
                    doc.tags.iter().find_map(|tag| match tag {
                        tsr_ast::JSDocTag::JSDocTemplateTag(template) => {
                            Some(template.type_parameters)
                        }
                        _ => None,
                    })
                })
                .unwrap_or(&[]),
            _ => &[],
        }
    }

    /// The JSDoc comment whose `@typedef`/`@callback` tag declares `symbol` —
    /// the tag's parent, as in native's tree, where the reparsed alias also
    /// gathers that comment's `@template` tags.
    fn jsdoc_alias_doc(&self, symbol: SymbolId) -> Option<&'a tsr_ast::JSDoc<'a>> {
        let declaration = self.binder.symbols().get(symbol).declarations.first().copied()?;
        // Native narrows this visibility branch by declaration kind before
        // following its structural JSDoc parents (emitresolver.go:128-135).
        if !matches!(
            self.node_map.get(declaration),
            Some(Node::JSDocTypedefTag(_) | Node::JSDocCallbackTag(_))
        ) {
            return None;
        }
        match self.node_map.get(self.nodes.parent(declaration)?) {
            Some(Node::JSDoc(doc)) => Some(doc),
            _ => None,
        }
    }

    /// `EmitResolver.determineIfDeclarationIsVisible`'s JSDoc typedef arm
    /// (`internal/checker/emitresolver.go:128-135`).
    fn is_visible_jsdoc_type_alias(&self, symbol: SymbolId) -> bool {
        let Some(doc) = self.jsdoc_alias_doc(symbol) else { return false };
        let Some(doc_id) = doc.node_id else { return false };
        let Some(&host) = self.jsdoc_hosts.get(&doc_id) else { return false };
        self.nodes
            .parent(host)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::SourceFile)
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

#[cfg(test)]
mod conditional_error_tests {
    use super::*;

    #[test]
    fn typeof_parameter_projection_certifies_original_symbols_without_return_work() {
        for (alias, argument, certified) in [
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? P : never;",
                "typeof callable",
                true,
            ),
            (
                "type Renamed<F extends (...args: any[]) => any> = F extends (...args: infer P) => void ? P : never;",
                "typeof callable",
                true,
            ),
            (
                "type Renamed<F> = F extends (...args: any[]) => infer R ? R : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? (P) : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? P[] : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? P : (never);",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? P : string;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends ((...args: infer P) => any) ? P : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F extends ((...args: any[]) => any)> = F extends (...args: infer P) => any ? P : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F extends (...args: any[]) => number> = F extends (...args: infer P) => any ? P : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F extends unknown> = F extends (...args: infer P) => any ? P : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P extends string[]) => any ? P : never;",
                "typeof callable",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? P : never;",
                "(typeof callable)",
                false,
            ),
            (
                "type Renamed<F> = F extends (...args: infer P) => any ? F : never;",
                "typeof callable",
                false,
            ),
        ] {
            let source = format!(
                "interface Array<T> {{ [index: number]: T; }} {alias} function callable(value: string) {{ return value; }} type Result = Renamed<{argument}>;"
            );
            let arena = tsr_core::Arena::new();
            let parsed = tsr_parser::parse(&arena, &source);
            assert!(parsed.diagnostics.is_empty());
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "entry.ts", text: &source },
            );
            let root = parsed.source_file.node_id.unwrap();
            let callable = bound.lookup_local(root, "callable").unwrap();
            let declaration = bound.symbols().get(callable).value_declaration.unwrap();
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let initial_types = checker.symbol_types.clone();
            let mut walk = vec![Node::SourceFile(parsed.source_file)];
            let query = loop {
                let node = walk.pop().expect("typeof argument");
                if let Node::TypeQueryNode(query) = node {
                    break query;
                }
                tsr_ast::push_children(node, &mut walk);
            };
            for _ in 0..3 {
                assert_eq!(checker.typeof_is_parameter_projection(query), certified, "{alias}");
                assert!(checker.pending_signature_returns.is_empty());
                assert!(checker.signature_returns.is_empty());
                assert_eq!(checker.symbol_types, initial_types);
            }
            let ty = checker.get_type_from_type_query_node(query);
            let key = checker.type_literal_key(declaration);
            assert_eq!(checker.symbol_types[&callable], ty);
            if certified {
                assert!(
                    checker.pending_signature_returns[&key]
                        == crate::signatures::LazyReturnState::Pending
                );
                assert!(!checker.signature_returns.contains_key(&key));
            } else {
                assert!(checker.pending_signature_returns.is_empty());
                assert_eq!(checker.signature_returns[&key], Some(checker.intrinsics.string));
            }
        }
    }

    #[test]
    fn an_unresolved_check_does_not_evaluate_any_branches() {
        let arena = tsr_core::Arena::new();
        let source = "type Choose<T> = T extends number ? 1 : 2; type Broken = Missing.Type;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let tsr_ast::Statement::TypeAliasDeclaration(choose) = parsed.source_file.statements[0]
        else {
            panic!("Choose alias")
        };
        let tsr_ast::Statement::TypeAliasDeclaration(broken) = parsed.source_file.statements[1]
        else {
            panic!("Broken alias")
        };
        let symbol = bound.symbol_of(choose.node_id.unwrap()).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let unresolved = checker.get_type_from_type_node(broken.r#type.unwrap());
        assert!(checker.is_error(unresolved));
        assert_ne!(unresolved, checker.intrinsics.error);
        let result = checker.evaluate_conditional_alias(symbol, &[unresolved], None);
        assert_eq!(result, None);
        let genuine_any = checker.intrinsics.any;
        let result = checker.evaluate_conditional_alias(symbol, &[genuine_any], None).unwrap();
        assert_eq!(checker.type_to_string(result), "1 | 2");
    }
}

#[cfg(test)]
mod source_intersection_tests {
    use super::*;

    #[test]
    fn original_operand_certificate_distinguishes_parameters_from_closed_and_unsupported_work() {
        let source = "type Scalar = string; type Id<U> = U; type Closed<U> = Scalar;
type Opaque<U> = U extends number ? string : string;
type Default<U = string> = U; type Cycle = Cycle;
function read<X extends string>(pure: string, scalar: Scalar, projection: Id<X>,
    constant: Closed<X>, parameter: X, pattern: `west-${X}`, closed: `west-${string}`,
    opaque: Opaque<X>, defaulted: Default, cycle: Cycle,
    absorbing: string | `west-${X}`, intersecting: X & string,
    closedUnion: string | number, closedIntersection: Scalar & {}) {}";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "source-flags.ts", text: source },
        );
        let tsr_ast::Statement::FunctionDeclaration(function) = parsed.source_file.statements[6]
        else {
            panic!("function")
        };
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let parameter = bound.symbol_of(function.type_parameters[0].node_id.unwrap()).unwrap();
        // A concrete frame must not erase the original parameter dependency.
        checker
            .alias_evaluation_bindings
            .push([(parameter, checker.intrinsics.string)].into_iter().collect());
        let expected = [
            Some(true),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
            Some(false),
            Some(true),
            None,
            None,
            None,
            None,
            None,
            Some(true),
            Some(true),
        ];
        let before = format!(
            "{:?}|{:?}|{:?}|{:?}",
            checker.store,
            checker.declared_types,
            checker.type_literal_types,
            checker.alias_evaluation_bindings
        );
        for reverse in [false, true] {
            let mut indexes: Vec<_> = (0..expected.len()).collect();
            if reverse {
                indexes.reverse();
            }
            for index in indexes {
                for _ in 0..3 {
                    assert_eq!(
                        checker.source_operand_is_closed(
                            function.parameters[index].r#type.unwrap(),
                            &[],
                            &[],
                            32
                        ),
                        expected[index]
                    );
                }
            }
        }
        assert_eq!(
            format!(
                "{:?}|{:?}|{:?}|{:?}",
                checker.store,
                checker.declared_types,
                checker.type_literal_types,
                checker.alias_evaluation_bindings
            ),
            before
        );
        assert!(checker.diagnostics().is_empty());
    }

    #[test]
    fn source_exception_is_not_reselected_when_the_owning_alias_arguments_change() {
        let source = "type Constant<T> = string & {}; type NonNullable<T> = T & {};";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "source-flags.ts", text: source },
        );
        for reverse in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.strict_null_checks = true;
            let mut indexes = [0, 1];
            if reverse {
                indexes.reverse();
            }
            for index in indexes {
                let tsr_ast::Statement::TypeAliasDeclaration(alias) =
                    parsed.source_file.statements[index]
                else {
                    panic!("alias")
                };
                let symbol = bound.symbol_of(alias.type_parameters[0].node_id.unwrap()).unwrap();
                let original = checker.get_declared_type_of_symbol(symbol);
                checker.alias_evaluation_bindings.push([(symbol, original)].into_iter().collect());
                let source = checker.get_type_from_type_node(alias.r#type.unwrap());
                assert!(
                    matches!(&checker.type_of(source).data, crate::types::TypeData::Intersection { types, .. } if types.len() == 2)
                );
                checker.alias_evaluation_bindings.pop();
                checker
                    .alias_evaluation_bindings
                    .push([(symbol, checker.intrinsics.string)].into_iter().collect());
                for _ in 0..3 {
                    assert_eq!(
                        checker.get_type_from_type_node(alias.r#type.unwrap()),
                        checker.intrinsics.string
                    );
                }
                checker.alias_evaluation_bindings.pop();
            }
            let value =
                checker.get_global_non_nullable_type_instantiation(checker.intrinsics.string);
            assert_eq!(value, checker.intrinsics.string);
        }
    }
}

#[cfg(test)]
mod literal_union_alias_tests {
    use super::*;

    #[test]
    fn concrete_literal_union_keeps_its_structural_flags_and_source_owner() {
        let source = r#"
            type Unit = "north" | "south";
            type Plural<T extends Unit> = T | { north: "N"; south: "S" }[T];
        "#;
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "literal-union.ts", text: source },
        );
        let root = parsed.source_file.node_id.unwrap();
        let unit_owner = bound.lookup_local(root, "Unit").unwrap();
        let owner = bound.lookup_local(root, "Plural").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let unit = checker.get_declared_type_of_symbol(unit_owner);
        assert!(checker.is_closed_literal_union_alias(owner, &[unit]));
        let result = checker.create_type_reference(owner, vec![unit]);
        assert!(
            checker.type_of(result).flags.contains(TypeFlags::UNION),
            "result={:?}; body={:?}",
            checker.type_of(result),
            checker.alias_body_evaluations.get(&(owner, vec![unit]))
        );
        let mut keys = checker.literal_key_texts(result).unwrap();
        keys.sort();
        assert_eq!(keys, ["N", "S", "north", "south"]);
        assert_eq!(checker.type_reference_targets[&result], (owner, vec![unit]));
        assert_eq!(checker.type_to_string(result), "Plural<Unit>");
        for _ in 0..3 {
            assert_eq!(checker.create_type_reference(owner, vec![unit]), result);
        }
    }

    #[test]
    fn cold_reverse_and_warm_literal_aliases_keep_ordered_arguments_and_distinct_owners() {
        let source = r#"
            type Unit = "north" | "south";
            type Plural<T extends Unit> = T | { north: "N"; south: "S" }[T];
            type Other<T extends Unit> = T | { north: "N"; south: "S" }[T];
            type Pair<A extends Unit, B extends Unit> = A | B | { north: "N"; south: "S" }[B];
        "#;
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "literal-order.ts", text: source },
        );
        let root = parsed.source_file.node_id.unwrap();
        let plural = bound.lookup_local(root, "Plural").unwrap();
        let other = bound.lookup_local(root, "Other").unwrap();
        let pair = bound.lookup_local(root, "Pair").unwrap();
        for reverse in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let unit =
                checker.get_declared_type_of_symbol(bound.lookup_local(root, "Unit").unwrap());
            let north = checker.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral("north".into()),
                false,
            );
            let south = checker.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral("south".into()),
                false,
            );
            let cases = [
                (plural, vec![unit], vec!["N", "S", "north", "south"]),
                (plural, vec![north], vec!["N", "north"]),
                (other, vec![unit], vec!["N", "S", "north", "south"]),
                (pair, vec![north, south], vec!["S", "north", "south"]),
                (pair, vec![south, north], vec!["N", "north", "south"]),
            ];
            let mut order = [0, 1, 2, 3, 4];
            if reverse {
                order.reverse();
            }
            let mut results = vec![checker.intrinsics.error; cases.len()];
            for index in order {
                let (owner, arguments, expected) = &cases[index];
                let result = checker.create_type_reference(*owner, arguments.clone());
                let mut actual = checker.literal_key_texts(result).unwrap();
                actual.sort();
                assert_eq!(&actual, expected);
                assert_eq!(checker.type_reference_targets[&result], (*owner, arguments.clone()));
                results[index] = result;
            }
            assert_ne!(results[0], results[2], "equal bodies do not merge alias owners");
            assert_ne!(results[3], results[4], "the second argument selects the lookup value");
            let before = format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                checker.store,
                checker.instantiations,
                checker.alias_body_evaluations,
                checker.type_literal_types,
                checker.type_reference_targets,
                checker.alias_evaluation_bindings
            );
            for _ in 0..3 {
                for (index, (owner, arguments, _)) in cases.iter().enumerate() {
                    assert_eq!(
                        checker.create_type_reference(*owner, arguments.clone()),
                        results[index]
                    );
                    assert!(
                        checker
                            .type_to_string(results[index])
                            .starts_with(checker.binder.symbols().get(*owner).name)
                    );
                }
            }
            assert_eq!(
                format!(
                    "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                    checker.store,
                    checker.instantiations,
                    checker.alias_body_evaluations,
                    checker.type_literal_types,
                    checker.type_reference_targets,
                    checker.alias_evaluation_bindings
                ),
                before
            );
        }
    }

    #[test]
    fn literal_union_preflight_refuses_captures_recursion_and_unsupported_tables_without_writes() {
        let source = r#"
            type External = "outside";
            type Good<T> = T | { north: "N" }[T];
            type Indirect<T> = T | External;
            type Recursive<T> = T | Recursive<T>;
            type ObjectPart<T> = T | { value: T };
            type Optional<T> = T | { north?: "N" }[T];
            type Numeric<T> = T | { north: 1 }[T];
            type Duplicate<T> = T | { north: "N"; north: "X" }[T];
            type Computed<T> = T | { ["north"]: "N" }[T];
            function enclosing<Outer>() { type Captured<T> = T | Outer; }
        "#;
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "literal-refusal.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let north = checker.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            crate::types::TypeData::StringLiteral("north".into()),
            false,
        );
        let symbol =
            |name| bound.symbols().iter().find(|(_, symbol)| symbol.name == name).unwrap().0;
        let good = symbol("Good");
        let before = format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}",
            checker.store,
            checker.declared_types,
            checker.instantiations,
            checker.alias_body_evaluations,
            checker.alias_evaluation_bindings
        );
        assert!(checker.is_closed_literal_union_alias(good, &[north]));
        for arguments in [
            vec![],
            vec![north, north],
            vec![checker.intrinsics.string],
            vec![checker.intrinsics.any],
            vec![checker.intrinsics.unknown],
            vec![checker.intrinsics.never],
        ] {
            assert!(!checker.is_closed_literal_union_alias(good, &arguments));
        }
        for name in [
            "Indirect",
            "Recursive",
            "ObjectPart",
            "Optional",
            "Numeric",
            "Duplicate",
            "Computed",
            "Captured",
        ] {
            assert!(!checker.is_closed_literal_union_alias(symbol(name), &[north]), "{name}");
        }
        assert_eq!(
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}",
                checker.store,
                checker.declared_types,
                checker.instantiations,
                checker.alias_body_evaluations,
                checker.alias_evaluation_bindings
            ),
            before
        );
    }
}

#[cfg(test)]
mod generic_keyword_alias_tests {
    use super::*;

    #[test]
    fn original_keyword_aliases_publish_body_identity_in_cold_reverse_and_warm_order() {
        let source = "type A<First, Second = First> = boolean; type B<X> = ((number)); type C<T> = string; type D<T> = any; type E<T> = unknown; type F<T> = never; type G<T> = object; type H<T> = bigint; type I<T> = symbol; type J<T> = void; type K<T> = undefined;";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "keyword-alias.ts", text: source },
        );
        for reverse in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            let mut controls = vec![
                ("A", checker.intrinsics.boolean, "boolean"),
                ("B", checker.intrinsics.number, "number"),
                ("C", checker.intrinsics.string, "string"),
                ("D", checker.intrinsics.any, "any"),
                ("E", checker.intrinsics.unknown, "unknown"),
                ("F", checker.intrinsics.never, "never"),
                ("G", checker.intrinsics.non_primitive, "object"),
                ("H", checker.intrinsics.bigint, "bigint"),
                ("I", checker.intrinsics.es_symbol, "symbol"),
                ("J", checker.intrinsics.void, "void"),
                ("K", checker.intrinsics.undefined, "undefined"),
            ];
            if reverse {
                controls.reverse();
            }
            let count = checker.store.len();
            for phase in 0..3 {
                if phase != 0 {
                    controls.reverse();
                }
                for &(name, expected, text) in &controls {
                    let owner =
                        bound.symbols().iter().find(|(_, symbol)| symbol.name == name).unwrap().0;
                    let actual = checker.get_declared_type_of_symbol(owner);
                    assert_eq!(actual, expected, "{name}, reverse={reverse}, phase={phase}");
                    let declaration = bound.symbols().get(owner).declarations[0];
                    assert_eq!(
                        checker.type_to_string_at(actual, declaration).as_deref(),
                        Some(text)
                    );
                    assert_eq!(checker.declared_types.get(&owner), Some(&expected));
                    assert_eq!(checker.resolutions.depth(), 0);
                    assert!(!checker.alias_evaluated_types.contains(&actual));
                    assert!(!checker.type_reference_targets.contains_key(&actual));
                }
                assert_eq!(checker.store.len(), count, "keywords reuse existing intrinsics");
            }
            let owner = bound.symbols().iter().find(|(_, symbol)| symbol.name == "A").unwrap().0;
            let parameters = checker.local_type_parameters_of(owner);
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name.unwrap().text, "First");
            assert_eq!(parameters[1].name.unwrap().text, "Second");
            assert!(parameters[1].default_type.is_some());
            let left = vec![checker.intrinsics.string, checker.intrinsics.number];
            let right = vec![checker.intrinsics.number, checker.intrinsics.string];
            assert_eq!(
                checker.create_type_reference(owner, left.clone()),
                checker.intrinsics.boolean
            );
            assert_eq!(
                checker.create_type_reference(owner, right.clone()),
                checker.intrinsics.boolean
            );
            assert_eq!(
                checker.instantiations.get(&(owner, left)),
                Some(&checker.intrinsics.boolean)
            );
            assert_eq!(
                checker.instantiations.get(&(owner, right)),
                Some(&checker.intrinsics.boolean)
            );
            assert!(!checker.alias_evaluated_types.contains(&checker.intrinsics.boolean));
        }
    }

    #[test]
    fn keyword_preflight_declines_foreign_active_captured_and_nonkeyword_owners_without_work() {
        let source = r"
            type Good<A, B = A> = ((boolean));
            type BooleanUnion<F> = true | false;
            type Intersection<F> = number & {};
            type Ref<F> = Good<F>;
            type Literal<F> = true;
            type ObjectBody<F> = { value: F };
            type Conditional<F> = F extends string ? true : false;
            type Recursive<F> = Recursive<F>;
            type Marker<F> = intrinsic;
            type Nongeneric = boolean;
            function enclosing<Outer>() {
                type Captured<Inner> = Outer;
                type Uncaptured<Inner> = number;
            }
        ";
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "keyword-refusals.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let owner = |name| bound.symbols().iter().find(|(_, entry)| entry.name == name).unwrap().0;
        let good = owner("Good");
        let snapshot = |checker: &Checker<'_, '_>| {
            format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                checker.store,
                checker.declared_types,
                checker.instantiations,
                checker.alias_body_evaluations,
                checker.alias_evaluated_types,
                checker.type_reference_targets,
                checker.alias_evaluation_bindings,
                checker.type_parameter_symbols,
                checker.resolutions,
                checker.diagnostics,
                checker.mapped_template_depth
            )
        };
        let before = snapshot(&checker);
        assert!(checker.original_generic_keyword_alias_body(good).is_some());
        assert!(checker.original_generic_keyword_alias_body(owner("Uncaptured")).is_some());
        for name in [
            "BooleanUnion",
            "Intersection",
            "Ref",
            "Literal",
            "ObjectBody",
            "Conditional",
            "Recursive",
            "Marker",
            "Nongeneric",
            "Captured",
            "Outer",
            "enclosing",
        ] {
            assert!(checker.original_generic_keyword_alias_body(owner(name)).is_none(), "{name}");
        }
        assert_eq!(snapshot(&checker), before);

        assert!(checker.resolutions.push(good, PropertyName::DeclaredType));
        let active = snapshot(&checker);
        assert!(checker.original_generic_keyword_alias_body(good).is_none());
        assert_eq!(snapshot(&checker), active);
        assert!(checker.resolutions.pop(), "read-only refusal must not poison the frame");
        assert_eq!(snapshot(&checker), before);

        checker
            .alias_evaluation_bindings
            .push([(owner("Outer"), checker.intrinsics.string)].into_iter().collect());
        let mapped = snapshot(&checker);
        assert!(checker.original_generic_keyword_alias_body(good).is_none());
        assert_eq!(snapshot(&checker), mapped);
        checker.alias_evaluation_bindings.pop();
        checker.mapped_template_depth = 1;
        let template = snapshot(&checker);
        assert!(checker.original_generic_keyword_alias_body(good).is_none());
        assert_eq!(snapshot(&checker), template);
        checker.mapped_template_depth = 0;

        let resolved = checker.get_declared_type_of_symbol(good);
        assert_eq!(resolved, checker.intrinsics.boolean);
        let completed = snapshot(&checker);
        assert_eq!(checker.get_declared_type_of_symbol(good), resolved);
        assert_eq!(
            snapshot(&checker),
            completed,
            "completed publication is an unchanged cache hit"
        );
        let captured = checker.get_declared_type_of_symbol(owner("Captured"));
        assert_ne!(captured, checker.intrinsics.number);
        assert_ne!(captured, checker.intrinsics.boolean);
    }
}

#[cfg(test)]
#[path = "keyword_owner_tests.rs"]
mod keyword_owner_tests;

#[cfg(test)]
mod parameter_default_state_tests {
    use super::*;

    fn inspect_defaults(test: impl FnOnce(&mut Checker<'_, '_>, TypeId, TypeId, SymbolId)) {
        inspect_defaults_source("interface Box<T, U=T> { value:U }", test);
    }

    fn inspect_defaults_source(
        source: &str,
        test: impl FnOnce(&mut Checker<'_, '_>, TypeId, TypeId, SymbolId),
    ) {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
        let owner = bound.lookup_local(root, "Box").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let parameters = checker.local_type_parameter_types_of(owner).unwrap();
        let (t, u) = (parameters[0].0, parameters[1].0);
        let symbol = checker.type_parameter_symbols[&t];
        test(&mut checker, t, u, symbol);
    }

    #[test]
    fn completed_defaults_preserve_distinct_outer_bindings() {
        inspect_defaults(|checker, t, u, symbol| {
            assert_eq!(checker.get_default_from_type_parameter(u), Some(t));
            for argument in [checker.intrinsics.number, checker.intrinsics.string] {
                checker.alias_evaluation_bindings.push([(symbol, argument)].into_iter().collect());
                assert_eq!(checker.get_default_from_type_parameter(u), Some(argument));
                assert_eq!(checker.get_default_from_type_parameter(u), Some(argument));
                checker.alias_evaluation_bindings.pop();
            }
            assert_eq!(checker.get_default_from_type_parameter(u), Some(t));
        });
    }

    #[test]
    fn an_instantiated_parameters_default_uses_its_captured_target_mapper() {
        inspect_defaults(|checker, t, u, _| {
            for argument in [checker.intrinsics.number, checker.intrinsics.string] {
                let fresh = checker.store.new_named(TypeFlags::TYPE_PARAMETER, "U".into(), None);
                checker.instantiated_type_parameters.insert(
                    fresh,
                    crate::inference::InstantiatedTypeParameter {
                        target: u,
                        map: vec![(t, argument)],
                        parameters: vec![t],
                        names: vec!["T".into()],
                    },
                );
                assert_eq!(checker.get_default_from_type_parameter(fresh), Some(argument));
                assert_eq!(checker.get_default_from_type_parameter(u), Some(t));
            }
        });
    }

    #[test]
    fn an_instantiated_default_ignores_unrelated_ambient_alias_frames() {
        inspect_defaults(|checker, t, u, symbol| {
            let captured = checker.intrinsics.number;
            let ambient = checker.intrinsics.string;
            let fresh = checker.store.new_named(TypeFlags::TYPE_PARAMETER, "U".into(), None);
            checker.instantiated_type_parameters.insert(
                fresh,
                crate::inference::InstantiatedTypeParameter {
                    target: u,
                    map: vec![(t, captured)],
                    parameters: vec![t],
                    names: vec!["T".into()],
                },
            );
            checker.alias_evaluation_bindings.push([(symbol, ambient)].into_iter().collect());
            assert_eq!(checker.get_default_from_type_parameter(fresh), Some(captured));
            assert_eq!(checker.get_default_from_type_parameter(fresh), Some(captured));
            assert_eq!(checker.get_default_from_type_parameter(u), Some(ambient));
            checker.alias_evaluation_bindings.pop();
            assert_eq!(checker.get_default_from_type_parameter(u), Some(t));
        });
    }

    #[test]
    fn an_unresolved_default_preserves_its_named_identity_without_completed_reuse() {
        inspect_defaults_source(
            "interface Box<T, U=Missing<T>> { value:U }",
            |checker, _, u, _| {
                let symbol = checker.type_parameter_symbols[&u];
                let declaration = checker.binder.symbols().get(symbol).declarations[0];
                let Some(Node::TypeParameterDeclaration(node)) = checker.node_map.get(declaration)
                else {
                    panic!("expected U declaration");
                };
                let written = checker.get_type_from_type_node(node.default_type.unwrap());
                assert!(checker.is_error(written));
                assert_ne!(written, checker.intrinsics.error);
                for _ in 0..2 {
                    let returned = checker.get_default_from_type_parameter(u).unwrap();
                    assert!(checker.is_error(returned));
                    assert_ne!(returned, checker.intrinsics.error);
                    // Unresolved references currently mint a fresh identity on an
                    // uncached AST evaluation. Preserve its named payload, without
                    // converting it to the intrinsic gap or claiming completion.
                    assert_eq!(checker.store.get(returned).data, checker.store.get(written).data);
                    assert!(checker.type_parameter_default_cache.is_empty());
                }
            },
        );
    }
}
