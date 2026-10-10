//! The base types of a class or interface.
//!
//! Ported from typescript-go `internal/checker/checker.go`, pinned at
//! 5b1047d10d32e7d5b446be4de56b126ff42f82bb: `getBaseConstructorTypeOfClass`
//! (`:16957`), `isConstructorType` (`:17013`), `getBaseTypes` (`:19167`),
//! `resolveBaseTypesOfClass` (`:19220`), `getBaseTypeNodeOfClass`,
//! `getInstantiatedConstructorsForTypeArguments` (`:19275`),
//! `resolveBaseTypesOfInterface` (`:19498`), `areAllOuterTypeParametersApplied`
//! (`:19520`), `isValidBaseType` (`:19537`) and `hasBaseType` (`:19551`).
//!
//! # Ownership and work boundaries
//!
//! - **Native state.** `InterfaceType.resolvedBaseConstructorType` and
//!   `InterfaceType.resolvedBaseTypes`/`baseTypesResolved`, both on the
//!   declared (target) type of a class or interface.
//! - **Key identity.** The merged class/interface [`SymbolId`]. This port has
//!   one declared type per such symbol
//!   ([`Checker::get_declared_type_of_class_or_interface`]), so the symbol is
//!   that type's identity. Instantiated references read their target's bases
//!   (native `resolveTypeReferenceMembers` instantiates them per reference);
//!   nothing per-reference is stored here.
//! - **Owner.** [`BaseTypeLinks`], one per `Checker` (Program lifetime).
//! - **Publication states.** A constructor-type entry is absent until
//!   resolved and then final: `undefined` without an `extends` clause,
//!   `errorType` after a resolution cycle or a non-constructor base, otherwise
//!   the checked expression type. A base-types entry is `resolved: false`
//!   while its `ResolvedBaseTypes` frame is on the resolution stack — a
//!   re-entry reads the partial list exactly as native returns
//!   `data.resolvedBaseTypes` when `pushTypeResolution` fails — and
//!   `resolved: true` afterwards.
//! - **Receiver/alias context.** None: the base type node is resolved in its
//!   own declaration's scope; no alias-evaluation frame is consulted.
//! - **Expensive work.** `check_expression` of the `extends` expression and the
//!   construct-signature instantiation, each once per class symbol.
//!
//! Diagnostics are not reported here. TS2506/TS2507/TS2508/TS2509/TS2310 keep
//! their existing check-pass owners in `check.rs`; this module only computes.
use rustc_hash::FxHashMap;
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    resolution::{PropertyName, ResolutionTarget},
    signatures::SignatureKind,
    types::{TypeData, TypeId},
};

/// Native `resolvedBaseConstructorType` and `resolvedBaseTypes` per class or
/// interface symbol. See the module documentation for the publication rules.
#[derive(Debug, Default)]
pub(crate) struct BaseTypeLinks {
    constructor_types: FxHashMap<SymbolId, TypeId>,
    base_types: FxHashMap<SymbolId, BaseTypes>,
}

#[derive(Debug, Clone, Default)]
struct BaseTypes {
    types: Vec<TypeId>,
    resolved: bool,
}

impl<'a> Checker<'a, '_> {
    /// `getBaseTypeNodeOfClass` (`checker.go:19267`):
    /// `ast.GetExtendsHeritageClauseElement(ast.GetClassLikeDeclarationOfSymbol(symbol))`
    /// — the first element of the first `extends` clause of the symbol's first
    /// class-like declaration.
    fn base_type_node_of_class(
        &self,
        class: SymbolId,
    ) -> Option<&'a tsr_ast::ExpressionWithTypeArguments<'a>> {
        let clauses = self.binder.symbols().get(class).declarations.iter().find_map(
            |&declaration| match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => Some(class.heritage_clauses),
                Some(Node::ClassExpression(class)) => Some(class.heritage_clauses),
                _ => None,
            },
        )?;
        clauses
            .iter()
            .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)?
            .types
            .first()
            .copied()
    }

    /// The base type node's `TypeArguments()` as native's reparsed tree holds
    /// them: the written list, or for a JS class writing none, the
    /// `@augments`/`@extends` tag's (`reparseHosted`, `parser/reparser.go`).
    fn base_type_node_type_arguments(
        &self,
        node: &tsr_ast::ExpressionWithTypeArguments<'a>,
    ) -> &'a [tsr_ast::TypeNode<'a>] {
        if !node.type_arguments.is_empty() {
            return node.type_arguments;
        }
        node.node_id.and_then(|id| self.jsdoc_augments_type_arguments(id)).unwrap_or(&[])
    }

    /// `getBaseConstructorTypeOfClass` (`checker.go:16957`), keyed by the class
    /// symbol. The circularity guard is the shared resolution stack's
    /// `ResolvedBaseConstructorType` frame; the forced member resolution
    /// native performs to surface circularities is
    /// [`Self::resolve_base_structure`].
    pub(crate) fn get_base_constructor_type_of_class(&mut self, class: SymbolId) -> TypeId {
        let class = self.binder.merged_symbol(class);
        if let Some(&resolved) = self.base_type_links.constructor_types.get(&class) {
            return resolved;
        }
        let error = self.intrinsics.error;
        let Some(node) = self.base_type_node_of_class(class) else {
            let undefined = self.intrinsics.undefined;
            self.base_type_links.constructor_types.insert(class, undefined);
            return undefined;
        };
        if !self
            .resolutions
            .push(ResolutionTarget::Symbol(class), PropertyName::ResolvedBaseConstructorType)
        {
            return error;
        }
        let constructor = match node.expression {
            Some(expression) => self.check_expression(expression),
            None => error,
        };
        if self.store.get(constructor).flags.intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION)
        {
            // Resolving the members of a class requires resolving the base
            // class of that class; native forces it here to catch cycles now.
            self.resolve_base_structure(constructor);
        }
        if !self.resolutions.pop() {
            return *self.base_type_links.constructor_types.entry(class).or_insert(error);
        }
        if !self.store.get(constructor).flags.contains(TypeFlags::ANY)
            && constructor != self.intrinsics.null
            && !self.is_constructor_type(constructor)
        {
            return *self.base_type_links.constructor_types.entry(class).or_insert(error);
        }
        *self.base_type_links.constructor_types.entry(class).or_insert(constructor)
    }

    /// The part of `resolveStructuredTypeMembers` (`checker.go`) that reaches
    /// base resolution, which is why `getBaseConstructorTypeOfClass` calls it:
    /// a class constructor type resolves its own base constructor type
    /// (`resolveAnonymousTypeMembers`' class arm), a class or interface
    /// instance resolves its base types (`resolveClassOrInterfaceMembers`), and
    /// an intersection resolves every constituent (`resolveIntersectionTypeMembers`).
    /// This port builds members lazily elsewhere, so only those reads are
    /// replayed; their results are cached by their own owners.
    fn resolve_base_structure(&mut self, t: TypeId) {
        match &self.store.get(t).data {
            TypeData::Intersection { types, .. } => {
                for constituent in types.clone() {
                    self.resolve_base_structure(constituent);
                }
            }
            &TypeData::Anonymous { symbol, .. } => {
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS) {
                    let _ = self.get_base_constructor_type_of_class(symbol);
                }
            }
            _ => {
                if let Some(target) = self.class_or_interface_target(t) {
                    let _ = self.get_base_types(target);
                }
            }
        }
    }

    /// `isConstructorType` (`checker.go:17013`). A construct-signature list
    /// this port cannot resolve (`None`) is a gap in the signature road, not
    /// native's empty list, so it does not turn the base into errorType; the
    /// class-symbol branch of `resolveBaseTypesOfClass` needs no signatures and
    /// the constructor branch declines on the same gap.
    fn is_constructor_type(&mut self, t: TypeId) -> bool {
        if self
            .signatures_of_type_kind(t, SignatureKind::Construct)
            .is_none_or(|signatures| !signatures.is_empty())
        {
            return true;
        }
        if self.store.get(t).flags.intersects(TypeFlags::TYPE_PARAMETER | TypeFlags::INDEXED_ACCESS)
        {
            return self
                .base_constraint_of_type(t)
                .is_some_and(|constraint| self.is_mixin_constructor_type(constraint));
        }
        false
    }

    /// `isMixinConstructorType` (`checker.go:17026`): a single construct
    /// signature with no type parameters and one rest parameter of type `any`
    /// or `any[]`.
    fn is_mixin_constructor_type(&mut self, t: TypeId) -> bool {
        let Some(signatures) = self.signatures_of_type_kind(t, SignatureKind::Construct) else {
            return false;
        };
        let [signature] = signatures.as_slice() else { return false };
        let [parameter] = signature.parameters.as_slice() else { return false };
        signature.type_parameters.is_empty() && parameter.rest && {
            let parameter_type = self.parameter_type(parameter);
            self.store.get(parameter_type).flags.contains(TypeFlags::ANY)
                || self.signature_array_element(parameter_type) == Some(self.intrinsics.any)
        }
    }

    /// `getBaseTypes` (`checker.go:19167`) of the declared type of a class or
    /// interface symbol. Empty for any other symbol. Circularity diagnostics
    /// (`reportCircularBaseType`) belong to the check pass.
    pub fn get_base_types(&mut self, symbol: SymbolId) -> Vec<TypeId> {
        let symbol = self.binder.merged_symbol(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        if !flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            return Vec::new();
        }
        if let Some(entry) = self.base_type_links.base_types.get(&symbol)
            && entry.resolved
        {
            return entry.types.clone();
        }
        if !self.resolutions.push(ResolutionTarget::Symbol(symbol), PropertyName::ResolvedBaseTypes)
        {
            return self
                .base_type_links
                .base_types
                .get(&symbol)
                .map(|entry| entry.types.clone())
                .unwrap_or_default();
        }
        self.base_type_links.base_types.entry(symbol).or_default();
        if flags.contains(SymbolFlags::CLASS)
            && let Some(base) = self.resolve_base_types_of_class(symbol)
            && let Some(entry) = self.base_type_links.base_types.get_mut(&symbol)
        {
            entry.types = vec![base];
        }
        if flags.contains(SymbolFlags::INTERFACE) {
            self.resolve_base_types_of_interface(symbol);
        }
        let _ = self.resolutions.pop();
        let entry = self.base_type_links.base_types.entry(symbol).or_default();
        entry.resolved = true;
        entry.types.clone()
    }

    /// `resolveBaseTypesOfClass` (`checker.go:19220`): the class's single base
    /// type, or `None` where native leaves `resolvedBaseTypes` empty.
    fn resolve_base_types_of_class(&mut self, class: SymbolId) -> Option<TypeId> {
        let constructor = self.get_base_constructor_type_of_class(class);
        let constructor = self.apparent_type(constructor);
        let flags = self.store.get(constructor).flags;
        if !flags.intersects(TypeFlags::OBJECT | TypeFlags::INTERSECTION | TypeFlags::ANY) {
            return None;
        }
        let node = self.base_type_node_of_class(class)?;
        let constructor_symbol = match self.store.get(constructor).data {
            TypeData::Anonymous { symbol, .. } | TypeData::Named { members: Some(symbol), .. } => {
                Some(self.binder.merged_symbol(symbol))
            }
            _ => None,
        };
        let base = match constructor_symbol {
            Some(symbol)
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS)
                    && !self.class_has_outer_type_parameters(symbol) =>
            {
                // getTypeFromClassOrInterfaceReference: the arity window and
                // fillMissingTypeArguments, errorType outside the window.
                let arguments = self.base_type_node_type_arguments(node);
                self.instantiated_heritage_base(symbol, arguments, node.node_id)
                    .unwrap_or(self.intrinsics.error)
            }
            _ if flags.contains(TypeFlags::ANY) => constructor,
            _ => self.first_instantiated_constructor_return(constructor, node)?,
        };
        if self.is_error(base) {
            return None;
        }
        let reduced = self.reduced_base_type(base);
        if !self.is_valid_base_type(reduced) {
            return None;
        }
        if self.class_or_interface_target(reduced) == Some(class)
            || self.has_base_type(reduced, class)
        {
            return None;
        }
        Some(reduced)
    }

    /// `!areAllOuterTypeParametersApplied(getDeclaredTypeOfSymbol(symbol))`
    /// (`checker.go:19520`) for the declared type itself: its type arguments
    /// are its own type parameters, so the test is whether the class has any
    /// outer type parameters at all (`getOuterTypeParameters`, `checker.go`),
    /// i.e. an enclosing declaration that declares type parameters.
    fn class_has_outer_type_parameters(&self, class: SymbolId) -> bool {
        let Some(declaration) =
            self.binder.symbols().get(class).declarations.iter().copied().find(|&declaration| {
                matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                )
            })
        else {
            return false;
        };
        let mut current = self.nodes.parent(declaration);
        while let Some(node) = current {
            if self.declares_type_parameters(node) {
                return true;
            }
            current = self.nodes.parent(node);
        }
        false
    }

    /// Whether `node` is one of `getOuterTypeParameters`' container kinds and
    /// declares type parameters of its own.
    fn declares_type_parameters(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::ClassDeclaration(n)) => !n.type_parameters.is_empty(),
            Some(Node::ClassExpression(n)) => !n.type_parameters.is_empty(),
            Some(Node::InterfaceDeclaration(n)) => !n.type_parameters.is_empty(),
            Some(Node::FunctionDeclaration(n)) => !n.type_parameters.is_empty(),
            Some(Node::FunctionExpression(n)) => !n.type_parameters.is_empty(),
            Some(Node::ArrowFunction(n)) => !n.type_parameters.is_empty(),
            Some(Node::MethodDeclaration(n)) => !n.type_parameters.is_empty(),
            Some(Node::TypeAliasDeclaration(n)) => !n.type_parameters.is_empty(),
            _ => false,
        }
    }

    /// `getInstantiatedConstructorsForTypeArguments` (`checker.go:19275`) then
    /// `getReturnTypeOfSignature(constructors[0])`: the first construct
    /// signature whose type-parameter window admits the written argument count,
    /// instantiated with those arguments (`getSignatureInstantiation`, which
    /// fills missing arguments from defaults). `None` when no constructor
    /// matches (native TS2508) or the signatures are not resolvable here.
    ///
    /// Native reads the reparsed tree, where a JS class's `@augments` type
    /// arguments sit on the heritage reference; see
    /// [`Self::base_type_node_type_arguments`].
    fn first_instantiated_constructor_return(
        &mut self,
        constructor: TypeId,
        node: &tsr_ast::ExpressionWithTypeArguments<'a>,
    ) -> Option<TypeId> {
        let signatures = self.signatures_of_type_kind(constructor, SignatureKind::Construct)?;
        let written = self.base_type_node_type_arguments(node);
        let count = written.len();
        let signature = signatures.into_iter().find(|signature| {
            let minimum = signature
                .type_parameters
                .iter()
                .rposition(|parameter| parameter.default.is_none())
                .map_or(0, |index| index + 1);
            count >= minimum && count <= signature.type_parameters.len()
        })?;
        if signature.type_parameters.is_empty() {
            return Some(signature.r#type);
        }
        let parameters = self.type_parameter_types(&signature)?;
        let names: Vec<_> = signature.type_parameters.iter().map(|p| p.name.as_str()).collect();
        let is_js = node.node_id.is_some_and(|id| self.in_js_file(id));
        let mut arguments: Vec<_> =
            written.iter().map(|&argument| self.get_type_from_type_node(argument)).collect();
        arguments.resize(parameters.len(), self.intrinsics.error);
        for index in count..parameters.len() {
            // fillMissingTypeArguments (`checker.go:21954`).
            let mut default = signature.type_parameters[index].default;
            if is_js
                && default.is_some_and(|default| {
                    default == self.intrinsics.unknown
                        || self.is_empty_anonymous_object_type(default)
                })
            {
                default = Some(self.intrinsics.any);
            }
            arguments[index] = match default {
                Some(default) => {
                    let map: Vec<_> =
                        parameters.iter().copied().zip(arguments.iter().copied()).collect();
                    self.instantiate_type(default, &map, &parameters, &names)
                }
                None if is_js => self.intrinsics.any,
                None => self.intrinsics.unknown,
            };
        }
        let map: Vec<_> = parameters.iter().copied().zip(arguments).collect();
        Some(self.instantiate_type(signature.r#type, &map, &parameters, &names))
    }

    /// `getReducedType` (`checker.go`) as far as a base type needs it: an
    /// intersection with a `never`-discriminant reduces to `never`.
    fn reduced_base_type(&mut self, t: TypeId) -> TypeId {
        if self.store.get(t).flags.contains(TypeFlags::INTERSECTION)
            && self.intersection_has_never_discriminant(t)
        {
            return self.intrinsics.never;
        }
        t
    }

    /// `isValidBaseType` (`checker.go:19537`).
    fn is_valid_base_type(&mut self, t: TypeId) -> bool {
        let flags = self.store.get(t).flags;
        if flags.contains(TypeFlags::TYPE_PARAMETER)
            && let Some(constraint) = self.base_constraint_of_type(t)
            && constraint != t
        {
            return self.is_valid_base_type(constraint);
        }
        if flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE | TypeFlags::ANY)
            && !self.is_generic_mapped_base(t)
        {
            return true;
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(t).data {
            return types
                .clone()
                .into_iter()
                .all(|constituent| self.is_valid_base_type(constituent));
        }
        false
    }

    /// `isGenericMappedType` (`checker.go`): a mapped type whose constraint is
    /// a generic index type. The key-remapping half is not distinguished.
    fn is_generic_mapped_base(&self, t: TypeId) -> bool {
        self.mapped_types
            .get(&t)
            .is_some_and(|info| self.maybe_type_of_kind(info.constraint, TypeFlags::INSTANTIABLE))
    }

    /// `getTargetType(t)` for a class or interface type (native
    /// `ObjectFlagsClassOrInterface|ObjectFlagsReference`), as its symbol.
    fn class_or_interface_target(&self, t: TypeId) -> Option<SymbolId> {
        let symbol = match self.type_reference_targets.get(&t) {
            Some(&(symbol, _)) => symbol,
            None => match self.store.get(t).data {
                TypeData::Named { members: Some(symbol), .. } => symbol,
                _ => return None,
            },
        };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            .then_some(symbol)
    }

    /// `hasBaseType` (`checker.go:19551`), with `checkBase` named by its symbol.
    fn has_base_type(&mut self, t: TypeId, check_base: SymbolId) -> bool {
        if let Some(target) = self.class_or_interface_target(t) {
            return target == check_base
                || self
                    .get_base_types(target)
                    .into_iter()
                    .any(|base| self.has_base_type(base, check_base));
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(t).data {
            return types
                .clone()
                .into_iter()
                .any(|constituent| self.has_base_type(constituent, check_base));
        }
        false
    }

    /// `resolveBaseTypesOfInterface` (`checker.go:19498`): every `extends`
    /// element of every interface declaration, appended in order. An element
    /// that is not a valid base type is TS2312 at the element, reported here
    /// as upstream does: the resolution runs once per symbol per checker
    /// (`base_type_links`). The circular arm's `reportCircularBaseType` stays
    /// `check.rs`'s `check_recursive_base_type`.
    fn resolve_base_types_of_interface(&mut self, symbol: SymbolId) {
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        for declaration in declarations {
            let Some(Node::InterfaceDeclaration(interface)) = self.node_map.get(declaration) else {
                continue;
            };
            let Some(clause) = interface
                .heritage_clauses
                .iter()
                .find(|clause| clause.token.kind == SyntaxKind::ExtendsKeyword)
            else {
                continue;
            };
            for &entry in clause.types {
                let base = self.interface_heritage_type(entry);
                let base = self.reduced_base_type(base);
                if self.is_error(base) {
                    continue;
                }
                if !self.is_valid_base_type(base) {
                    // `c.error(node, An_interface_can_only_extend_…)`.
                    if let Some(at) = entry.node_id
                        && let Some(file) = self.source_file_of_for_diagnostics(at)
                    {
                        let span = self.error_span(at);
                        self.report(
                            file,
                            tsr_diagnostics::Diagnostic::new(
                                &tsr_diagnostics::messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_STATICALLY_KNOWN_MEMBERS,
                                span,
                            ),
                        );
                    }
                    continue;
                }
                if self.class_or_interface_target(base) != Some(symbol)
                    && !self.has_base_type(base, symbol)
                    && let Some(entry) = self.base_type_links.base_types.get_mut(&symbol)
                {
                    entry.types.push(base);
                }
            }
        }
    }

    /// `getTypeFromTypeNode` of an interface `extends` element
    /// (`getTypeFromTypeReference` over its expression): a class or interface
    /// through `getTypeFromClassOrInterfaceReference`, a type alias through its
    /// declared type or an exact-arity instantiation. Anything else is
    /// `errorType` here.
    fn interface_heritage_type(
        &mut self,
        entry: &tsr_ast::ExpressionWithTypeArguments<'a>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(expression) = entry.expression else { return error };
        let Some(symbol) = self.heritage_entity_symbol(expression, SymbolFlags::TYPE) else {
            return error;
        };
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE) {
            return self
                .instantiated_heritage_base(symbol, entry.type_arguments, entry.node_id)
                .unwrap_or(error);
        }
        // `getTypeReferenceType` of a type parameter: its declared type; with
        // type arguments it is TS2315 and `errorType`.
        if flags.contains(SymbolFlags::TYPE_PARAMETER) {
            if !entry.type_arguments.is_empty() {
                return error;
            }
            return self.get_declared_type_of_symbol(symbol);
        }
        if flags.contains(SymbolFlags::TYPE_ALIAS) {
            if entry.type_arguments.is_empty() {
                return self.get_declared_type_of_symbol(symbol);
            }
            if entry.type_arguments.len() == self.local_type_parameters_of(symbol).len() {
                let arguments: Vec<_> = entry
                    .type_arguments
                    .iter()
                    .map(|&argument| self.get_type_from_type_node(argument))
                    .collect();
                return self.create_type_reference(symbol, arguments);
            }
        }
        error
    }
}
