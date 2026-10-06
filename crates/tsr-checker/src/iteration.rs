//! The iteration-types engine: `getIterationTypesOfIterable` and its
//! resolvers (pinned tsgo 5b1047d, `internal/checker/checker.go`).
//!
//! An [`IterationTypes`] holds the *yield*, *return* and *next* types of an
//! iterable, each absent exactly where upstream's field is nil. Every query
//! answers `Err(Unsupported)` when this port cannot decide the protocol (an
//! unresolved member, an unfinished relation, a recursive resolution), which
//! is never evidence that the type is not iterable: only a completed
//! `Ok(IterationTypes::NONE)` reports `reportTypeNotIterableError`.
//!
//! Work boundary: upstream caches by `(type id, use & IterationUseCacheFlags)`
//! in `iterationTypesCache`. This port does not cache yet; the engine runs at
//! check sites (one query per for-of head, binding pattern, destructuring
//! target and spread) whose types are already cached by `check_expression`.
//! The only mutable state is the private Checker's existing
//! `resolving_iteration_types` active set, keyed by the queried `TypeId`, which
//! breaks recursive `[Symbol.iterator]` protocols (an iterator method whose
//! return inference iterates its own receiver). Re-entry answers
//! `Err(Unsupported)` and removing the key publishes nothing.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::types::{TypeData, TypeId};

/// A query this port cannot decide; never a protocol failure.
pub(crate) type Unsupported = ();

bitflags::bitflags! {
    /// `IterationUse` (`checker.go:482`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct IterationUse: u32 {
        const ALLOWS_SYNC_ITERABLES = 1 << 0;
        const ALLOWS_ASYNC_ITERABLES = 1 << 1;
        const ALLOWS_STRING_INPUT = 1 << 2;
        const FOR_OF_FLAG = 1 << 3;
        const YIELD_STAR_FLAG = 1 << 4;
        const SPREAD_FLAG = 1 << 5;
        const DESTRUCTURING_FLAG = 1 << 6;
        const POSSIBLY_OUT_OF_BOUNDS = 1 << 7;
        const SPREAD = Self::ALLOWS_SYNC_ITERABLES.bits() | Self::SPREAD_FLAG.bits();
        const DESTRUCTURING =
            Self::ALLOWS_SYNC_ITERABLES.bits() | Self::DESTRUCTURING_FLAG.bits();
        const FOR_OF = Self::ALLOWS_SYNC_ITERABLES.bits()
            | Self::ALLOWS_STRING_INPUT.bits()
            | Self::FOR_OF_FLAG.bits();
        const FOR_AWAIT_OF = Self::ALLOWS_SYNC_ITERABLES.bits()
            | Self::ALLOWS_ASYNC_ITERABLES.bits()
            | Self::ALLOWS_STRING_INPUT.bits()
            | Self::FOR_OF_FLAG.bits();
        const YIELD_STAR = Self::ALLOWS_SYNC_ITERABLES.bits() | Self::YIELD_STAR_FLAG.bits();
        const ASYNC_YIELD_STAR = Self::ALLOWS_SYNC_ITERABLES.bits()
            | Self::ALLOWS_ASYNC_ITERABLES.bits()
            | Self::YIELD_STAR_FLAG.bits();
        const GENERATOR_RETURN_TYPE = Self::ALLOWS_SYNC_ITERABLES.bits();
        const ASYNC_GENERATOR_RETURN_TYPE = Self::ALLOWS_ASYNC_ITERABLES.bits();
    }
}

/// `IterationTypes` (`checker.go:225`): nil fields are `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // upstream's field names
pub(crate) struct IterationTypes {
    pub(crate) yield_type: Option<TypeId>,
    pub(crate) return_type: Option<TypeId>,
    pub(crate) next_type: Option<TypeId>,
}

impl IterationTypes {
    /// `noIterationTypes`.
    pub(crate) const NONE: Self = Self { yield_type: None, return_type: None, next_type: None };

    /// `IterationTypes.hasTypes` (`checker.go:6408`).
    pub(crate) fn has_types(self) -> bool {
        self.yield_type.is_some() || self.return_type.is_some() || self.next_type.is_some()
    }

    fn all(ty: TypeId) -> Self {
        Self { yield_type: Some(ty), return_type: Some(ty), next_type: Some(ty) }
    }
}

/// `IterationTypeKind` (`checker.go:219`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IterationTypeKind {
    Yield,
    Return,
    Next,
}

/// `IterationTypesResolver` (`checker.go:520`): the sync or async half.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Resolver {
    Sync,
    Async,
}

impl Resolver {
    fn iterator_symbol_name(self) -> &'static str {
        match self {
            Self::Sync => "[Symbol.iterator]",
            Self::Async => "[Symbol.asyncIterator]",
        }
    }

    /// `getGlobalIterableType`, `getGlobalIteratorObjectType`,
    /// `getGlobalIterableIteratorType`, `getGlobalGeneratorType` — the
    /// iterable half of `getIterationTypesOfIterableFast`.
    fn iterable_fast_globals(self) -> [&'static str; 4] {
        match self {
            Self::Sync => ["Iterable", "IteratorObject", "IterableIterator", "Generator"],
            Self::Async => {
                ["AsyncIterable", "AsyncIteratorObject", "AsyncIterableIterator", "AsyncGenerator"]
            }
        }
    }

    /// The iterator half of `getIterationTypesOfIteratorFast`.
    fn iterator_fast_globals(self) -> [&'static str; 4] {
        match self {
            Self::Sync => ["Iterator", "IteratorObject", "IterableIterator", "Generator"],
            Self::Async => {
                ["AsyncIterator", "AsyncIteratorObject", "AsyncIterableIterator", "AsyncGenerator"]
            }
        }
    }

    /// `getGlobalBuiltinIteratorTypes`.
    fn builtin_iterator_globals(self) -> &'static [&'static str] {
        match self {
            Self::Sync => &["ArrayIterator", "MapIterator", "SetIterator", "StringIterator"],
            Self::Async => &["ReadableStreamAsyncIterator"],
        }
    }
}

impl Checker<'_, '_> {
    /// `getIterationTypesOfIterable` (`checker.go:6265`) without its cache
    /// and without diagnostics; [`Checker::report_iteration_diagnostics`] is
    /// the error-node half.
    pub(crate) fn get_iteration_types_of_iterable(
        &mut self,
        ty: TypeId,
        use_: IterationUse,
    ) -> Result<IterationTypes, Unsupported> {
        if self.is_error(ty) {
            return Err(());
        }
        if ty == self.intrinsics.any {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        self.get_iteration_types_of_iterable_worker(ty, use_)
    }

    /// `getIterationTypesOfIterableWorker` (`checker.go:6287`).
    fn get_iteration_types_of_iterable_worker(
        &mut self,
        ty: TypeId,
        use_: IterationUse,
    ) -> Result<IterationTypes, Unsupported> {
        if let TypeData::Union { types, .. } = self.store.get(ty).data.clone() {
            let mut all = Vec::with_capacity(types.len());
            for constituent in types {
                let types = self.get_iteration_types_of_iterable_worker(constituent, use_)?;
                if !types.has_types() {
                    return Ok(IterationTypes::NONE);
                }
                all.push(types);
            }
            return Ok(self.combine_iteration_types(&all));
        }
        if self.is_error(ty) {
            return Err(());
        }
        if use_.contains(IterationUse::ALLOWS_ASYNC_ITERABLES) {
            let types = self.get_iteration_types_of_iterable_fast(ty, Resolver::Async)?;
            if types.has_types() {
                return if use_.contains(IterationUse::FOR_OF_FLAG) {
                    self.get_async_from_sync_iteration_types(types)
                } else {
                    Ok(types)
                };
            }
            let types = self.get_iteration_types_of_iterable_slow(ty, Resolver::Async)?;
            if types.has_types() {
                return Ok(types);
            }
        }
        if use_.contains(IterationUse::ALLOWS_SYNC_ITERABLES) {
            let types = self.get_iteration_types_of_iterable_fast(ty, Resolver::Sync)?;
            if types.has_types() {
                return if use_.contains(IterationUse::ALLOWS_ASYNC_ITERABLES) {
                    self.get_async_from_sync_iteration_types(types)
                } else {
                    Ok(types)
                };
            }
            let types = self.get_iteration_types_of_iterable_slow(ty, Resolver::Sync)?;
            if types.has_types() {
                return if use_.contains(IterationUse::ALLOWS_ASYNC_ITERABLES) {
                    self.get_async_from_sync_iteration_types(types)
                } else {
                    Ok(types)
                };
            }
        }
        Ok(IterationTypes::NONE)
    }

    /// The global type named `name` at `arity`, or `None` where upstream's
    /// getter answers `emptyGenericType`.
    fn iteration_global(&self, name: &str, arity: usize) -> Option<tsr_binder::SymbolId> {
        self.global_type_symbol_with_arity(name, arity)
            .map(|symbol| self.binder.merged_symbol(symbol))
    }

    /// `isReferenceToType` against one of `names`: the reference's type
    /// arguments, or `None` when `ty` is not such a reference.
    fn reference_to_global(&self, ty: TypeId, names: &[&str], arity: usize) -> Option<Vec<TypeId>> {
        let (target, arguments) = self.type_reference_targets.get(&ty)?;
        let target = self.binder.merged_symbol(*target);
        names
            .iter()
            .any(|name| self.iteration_global(name, arity) == Some(target))
            .then(|| arguments.clone())
    }

    /// `getIterationTypesOfIterableFast` (`checker.go:6357`) and, with
    /// `iterator`, `getIterationTypesOfIteratorFast` (`checker.go:6506`).
    fn get_iteration_types_fast(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
        iterator: bool,
    ) -> Result<IterationTypes, Unsupported> {
        let globals = if iterator {
            resolver.iterator_fast_globals()
        } else {
            resolver.iterable_fast_globals()
        };
        if let Some(arguments) = self.reference_to_global(ty, &globals, 3) {
            let [yield_type, return_type, next_type] = arguments.as_slice() else { return Err(()) };
            return self.get_resolved_iteration_types(
                resolver,
                *yield_type,
                *return_type,
                *next_type,
            );
        }
        if let Some(arguments) =
            self.reference_to_global(ty, resolver.builtin_iterator_globals(), 1)
        {
            let [yield_type] = arguments.as_slice() else { return Err(()) };
            let return_type = self.get_builtin_iterator_return_type();
            return self.get_resolved_iteration_types(
                resolver,
                *yield_type,
                return_type,
                self.intrinsics.unknown,
            );
        }
        Ok(IterationTypes::NONE)
    }

    fn get_iteration_types_of_iterable_fast(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
    ) -> Result<IterationTypes, Unsupported> {
        self.get_iteration_types_fast(ty, resolver, false)
    }

    /// `IterationTypesResolver.getResolvedIterationTypes` (`checker.go:6386`).
    fn get_resolved_iteration_types(
        &mut self,
        resolver: Resolver,
        yield_type: TypeId,
        return_type: TypeId,
        next_type: TypeId,
    ) -> Result<IterationTypes, Unsupported> {
        Ok(IterationTypes {
            yield_type: Some(
                self.resolve_iteration_type(resolver, yield_type)?.unwrap_or(yield_type),
            ),
            return_type: Some(
                self.resolve_iteration_type(resolver, return_type)?.unwrap_or(return_type),
            ),
            next_type: Some(next_type),
        })
    }

    /// `resolveIterationType`: identity for sync, `getAwaitedType` for async.
    /// `Ok(None)` is upstream's nil awaited type.
    fn resolve_iteration_type(
        &mut self,
        resolver: Resolver,
        ty: TypeId,
    ) -> Result<Option<TypeId>, Unsupported> {
        match resolver {
            Resolver::Sync => Ok(Some(ty)),
            Resolver::Async => self.awaited_type(ty).map(Some).ok_or(()),
        }
    }

    /// `getBuiltinIteratorReturnType` (`checker.go:6402`).
    fn get_builtin_iterator_return_type(&self) -> TypeId {
        if self.strict_builtin_iterator_return {
            self.intrinsics.undefined
        } else {
            self.intrinsics.any
        }
    }

    /// `combineIterationTypes` (`checker.go:6420`).
    fn combine_iteration_types(&mut self, all: &[IterationTypes]) -> IterationTypes {
        let union = |checker: &mut Self, pick: fn(&IterationTypes) -> Option<TypeId>| {
            let types: Vec<TypeId> = all.iter().filter_map(pick).collect();
            (!types.is_empty()).then(|| checker.get_union_type(&types))
        };
        IterationTypes {
            yield_type: union(self, |t| t.yield_type),
            return_type: union(self, |t| t.return_type),
            next_type: union(self, |t| t.next_type),
        }
    }

    /// `getAsyncFromSyncIterationTypes` (`checker.go:6436`).
    fn get_async_from_sync_iteration_types(
        &mut self,
        types: IterationTypes,
    ) -> Result<IterationTypes, Unsupported> {
        let any = self.intrinsics.any;
        if !types.has_types()
            || (types.yield_type == Some(any)
                && types.return_type == Some(any)
                && types.next_type == Some(any))
        {
            return Ok(types);
        }
        // Pinned upstream awaits a nil yield or return through
        // getAwaitedTypeEx(nil), which this port cannot reproduce; decline.
        let (Some(yield_type), Some(return_type)) = (types.yield_type, types.return_type) else {
            return Err(());
        };
        Ok(IterationTypes {
            yield_type: Some(self.awaited_type(yield_type).ok_or(())?),
            return_type: Some(self.awaited_type(return_type).ok_or(())?),
            next_type: types.next_type,
        })
    }

    /// `getIterationTypeOfIterable(use, IterationTypeKindReturn, t)` orElse
    /// `anyType`, the `yield*` answer of `checkYieldExpression`
    /// (`checker.go:10998`). `None` when the protocol is undecided here.
    pub(crate) fn yield_star_return_type(
        &mut self,
        operand: TypeId,
        is_async: bool,
    ) -> Option<TypeId> {
        let any = self.intrinsics.any;
        if operand == any {
            return Some(any);
        }
        let use_ = if is_async { IterationUse::ASYNC_YIELD_STAR } else { IterationUse::YIELD_STAR };
        let types = self.get_iteration_types_of_iterable(operand, use_).ok()?;
        Some(types.return_type.unwrap_or(any))
    }

    /// The `yield*` arm of `checkAndAggregateYieldOperandTypes`
    /// (`checker.go:20322`): `getYieldedTypeOfYieldExpression`'s yielded type
    /// (`checkIteratedTypeOrElementType` with a sent type of `any`, `anyType`
    /// when no yield type exists, awaited in an async generator) and the
    /// delegate's iteration NEXT type. `None` when the protocol is undecided.
    pub(crate) fn yield_star_operand_types(
        &mut self,
        operand: TypeId,
        is_async: bool,
    ) -> Option<(TypeId, Option<TypeId>)> {
        let any = self.intrinsics.any;
        if operand == any {
            return Some((any, Some(any)));
        }
        let use_ = if is_async { IterationUse::ASYNC_YIELD_STAR } else { IterationUse::YIELD_STAR };
        let types = self.get_iteration_types_of_iterable(operand, use_).ok()?;
        let yielded =
            if operand == self.intrinsics.never { any } else { types.yield_type.unwrap_or(any) };
        let yielded = if is_async { self.awaited_type_no_alias(yielded)? } else { yielded };
        Some((yielded, types.next_type))
    }

    /// `getIterationTypesOfGeneratorFunctionReturnType` (`checker.go:6224`):
    /// the iterable protocol of a generator's (annotated or contextual)
    /// return type, else the type read as an iterator itself.
    pub(crate) fn get_iteration_types_of_generator_function_return_type(
        &mut self,
        ty: TypeId,
        is_async: bool,
    ) -> Result<IterationTypes, Unsupported> {
        if self.is_error(ty) {
            return Err(());
        }
        if ty == self.intrinsics.any {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        let (use_, resolver) = if is_async {
            (IterationUse::ASYNC_GENERATOR_RETURN_TYPE, Resolver::Async)
        } else {
            (IterationUse::GENERATOR_RETURN_TYPE, Resolver::Sync)
        };
        let result = self.get_iteration_types_of_iterable(ty, use_)?;
        if result.has_types() {
            return Ok(result);
        }
        self.get_iteration_types_of_iterator_worker(ty, resolver)
    }

    /// `getIterationTypeOfGeneratorFunctionReturnType` (`checker.go:6216`):
    /// `Ok(None)` is upstream's nil (an `any` return type, or a missing slot).
    pub(crate) fn get_iteration_type_of_generator_function_return_type(
        &mut self,
        kind: IterationTypeKind,
        return_type: TypeId,
        is_async: bool,
    ) -> Result<Option<TypeId>, Unsupported> {
        if self.is_error(return_type) {
            return Err(());
        }
        if return_type == self.intrinsics.any {
            return Ok(None);
        }
        let types =
            self.get_iteration_types_of_generator_function_return_type(return_type, is_async)?;
        Ok(match kind {
            IterationTypeKind::Yield => types.yield_type,
            IterationTypeKind::Return => types.return_type,
            IterationTypeKind::Next => types.next_type,
        })
    }

    /// `createGeneratorType` (`checker.go:20434`): `Generator` (or
    /// `AsyncGenerator`) of the three slots, falling back to
    /// `IterableIterator` and then to the empty object type when the lib
    /// declares neither. The fallback's TS2318 report belongs to the global
    /// lookup and is not made here.
    pub(crate) fn create_generator_type(
        &mut self,
        yield_type: TypeId,
        return_type: TypeId,
        next_type: TypeId,
        is_async: bool,
    ) -> Result<TypeId, Unsupported> {
        let resolver = if is_async { Resolver::Async } else { Resolver::Sync };
        let unknown = self.intrinsics.unknown;
        let yield_type = self.resolve_iteration_type(resolver, yield_type)?.unwrap_or(unknown);
        let return_type = self.resolve_iteration_type(resolver, return_type)?.unwrap_or(unknown);
        let [generator, iterable_iterator] = if is_async {
            ["AsyncGenerator", "AsyncIterableIterator"]
        } else {
            ["Generator", "IterableIterator"]
        };
        let Some(target) = self
            .global_type_symbol_with_arity(generator, 3)
            .or_else(|| self.global_type_symbol_with_arity(iterable_iterator, 3))
        else {
            return Ok(self.intrinsics.empty_object);
        };
        Ok(self.create_type_reference(target, vec![yield_type, return_type, next_type]))
    }

    /// `checkGeneratorInstantiationAssignabilityToReturnType`
    /// (`checker.go:29697`) without an error node: whether the generator
    /// instantiated from `return_type`'s own iteration types is assignable to
    /// it — the predicate upstream filters a union return type with.
    pub(crate) fn generator_instantiation_assignable_to_return_type(
        &mut self,
        return_type: TypeId,
        is_async: bool,
    ) -> Result<bool, Unsupported> {
        let any = self.intrinsics.any;
        let yield_type = self
            .get_iteration_type_of_generator_function_return_type(
                IterationTypeKind::Yield,
                return_type,
                is_async,
            )?
            .unwrap_or(any);
        let generator_return = self
            .get_iteration_type_of_generator_function_return_type(
                IterationTypeKind::Return,
                return_type,
                is_async,
            )?
            .unwrap_or(yield_type);
        let next_type = self
            .get_iteration_type_of_generator_function_return_type(
                IterationTypeKind::Next,
                return_type,
                is_async,
            )?
            .unwrap_or(self.intrinsics.unknown);
        let instantiation =
            self.create_generator_type(yield_type, generator_return, next_type, is_async)?;
        match self.relate_ternary(instantiation, return_type, crate::relater::Relation::Assignable)
        {
            crate::relater::Ternary::Related => Ok(true),
            crate::relater::Ternary::NotRelated => Ok(false),
            crate::relater::Ternary::Unknown => Err(()),
        }
    }

    /// The annotated half of `checkYieldExpression` (`checker.go:10982`):
    /// `getReturnTypeFromAnnotation(fn)`, a union filtered by
    /// [`Self::generator_instantiation_assignable_to_return_type`], answers
    /// a non-star `yield` with its NEXT iteration type, orElse `anyType`.
    pub(crate) fn annotated_yield_next_type(
        &mut self,
        annotated: TypeId,
        is_async: bool,
    ) -> Result<TypeId, Unsupported> {
        let mut return_type = annotated;
        if self.store.get(return_type).flags.contains(TypeFlags::UNION) {
            let mut undecided = false;
            return_type = self.filter_type(return_type, |checker, constituent| {
                checker
                    .generator_instantiation_assignable_to_return_type(constituent, is_async)
                    .unwrap_or_else(|()| {
                        undecided = true;
                        false
                    })
            });
            if undecided {
                return Err(());
            }
        }
        Ok(self
            .get_iteration_type_of_generator_function_return_type(
                IterationTypeKind::Next,
                return_type,
                is_async,
            )?
            .unwrap_or(self.intrinsics.any))
    }

    /// Whether `name` is decidably absent from `ty` — the existing
    /// complete-table contract shared with the for-of yield resolver.
    /// A complete table that lists `name` while the property lookup missed it
    /// is an unresolved member, not an absence.
    fn iteration_member_decidably_absent(&mut self, ty: TypeId, name: &str) -> bool {
        // `never` (and its implicit/silent variants) has no properties:
        // `getPropertyOfType` on it answers nil for every name.
        if self.store.get(ty).flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NEVER) {
            return true;
        }
        match self.declared_property_table(ty) {
            Some(table) => !table.iter().any(|(member, _)| member == name),
            None => self.miss_is_established(ty, name),
        }
    }

    /// `getIterationTypesOfIterableSlow` (`checker.go:6460`).
    fn get_iteration_types_of_iterable_slow(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
    ) -> Result<IterationTypes, Unsupported> {
        if !self.resolving_iteration_types.insert(ty) {
            return Err(());
        }
        let result = self.get_iteration_types_of_iterable_slow_worker(ty, resolver);
        self.resolving_iteration_types.remove(&ty);
        result
    }

    fn get_iteration_types_of_iterable_slow_worker(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
    ) -> Result<IterationTypes, Unsupported> {
        let name = resolver.iterator_symbol_name();
        // getPropertyOfType reads the apparent type: `string` iterates through
        // the global `String` interface's `[Symbol.iterator]`.
        let ty = if self.store.get(ty).flags.intersects(TypeFlags::PRIMITIVE) {
            self.apparent_type(ty)
        } else {
            ty
        };
        let Some(method) = self.get_property_of_type(ty, name) else {
            return if self.iteration_member_decidably_absent(ty, name) {
                Ok(IterationTypes::NONE)
            } else {
                Err(())
            };
        };
        if self.property_is_optional(method) {
            return Ok(IterationTypes::NONE);
        }
        let Some(method_type) = self.iteration_property_type(ty, name) else { return Err(()) };
        if self.is_error(method_type) {
            return Err(());
        }
        if method_type == self.intrinsics.any {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        let signatures = self.iteration_call_signatures(method_type)?;
        let mut returns = Vec::new();
        for signature in &signatures {
            if self.signature_min_argument_count(signature) == 0 {
                if self.is_error(signature.r#type) {
                    return Err(());
                }
                returns.push(signature.r#type);
            }
        }
        if returns.is_empty() {
            return Ok(IterationTypes::NONE);
        }
        let iterator = self.get_intersection_type(&returns, None);
        self.get_iteration_types_of_iterator_worker(iterator, resolver)
    }

    /// The call signatures of a member type; a primitive has none.
    fn iteration_call_signatures(
        &mut self,
        ty: TypeId,
    ) -> Result<Vec<crate::signatures::Signature>, Unsupported> {
        if self.store.get(ty).flags.intersects(TypeFlags::PRIMITIVE) {
            return Ok(Vec::new());
        }
        self.call_signatures_of_type(ty).ok_or(())
    }

    /// `getIterationTypesOfIteratorWorker` (`checker.go:6495`).
    fn get_iteration_types_of_iterator_worker(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
    ) -> Result<IterationTypes, Unsupported> {
        if self.is_error(ty) {
            return Err(());
        }
        if ty == self.intrinsics.any {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        let types = self.get_iteration_types_fast(ty, resolver, true)?;
        if types.has_types() {
            return Ok(types);
        }
        // getIterationTypesOfIteratorSlow (`checker.go:6533`).
        let next = self.get_iteration_types_of_method(ty, resolver, "next")?;
        let ret = self.get_iteration_types_of_method(ty, resolver, "return")?;
        let throw = self.get_iteration_types_of_method(ty, resolver, "throw")?;
        Ok(self.combine_iteration_types(&[next, ret, throw]))
    }

    /// `getIterationTypesOfMethod` (`checker.go:6541`).
    ///
    /// Not ported: the arm that reads a method declared only by the global
    /// `Generator`/`Iterator` through its instantiation mapper
    /// (`checker.go:6585`). This port's `get_property_of_type` answers no
    /// symbol for a member inherited through a type-argument heritage entry
    /// (`interface I extends Iterator<0, 1, 2>`), so the arm's identity test
    /// has nothing to compare; the lookup misses and the query declines.
    fn get_iteration_types_of_method(
        &mut self,
        ty: TypeId,
        resolver: Resolver,
        name: &str,
    ) -> Result<IterationTypes, Unsupported> {
        let method = self.get_property_of_type(ty, name);
        if method.is_none() && !self.iteration_member_decidably_absent(ty, name) {
            return Err(());
        }
        if method.is_none() && name != "next" {
            return Ok(IterationTypes::NONE);
        }
        let mut method_type = None;
        if let Some(method) = method
            && !(name == "next" && self.property_is_optional(method))
        {
            let Some(mut found) = self.iteration_property_type(ty, name) else { return Err(()) };
            if self.is_error(found) {
                return Err(());
            }
            if name != "next" {
                found =
                    self.get_type_with_facts(found, crate::flow::TypeFacts::NE_UNDEFINED_OR_NULL);
            }
            method_type = Some(found);
        }
        if method_type == Some(self.intrinsics.any) {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        let signatures = match method_type {
            Some(method_type) => self.iteration_call_signatures(method_type)?,
            None => Vec::new(),
        };
        if signatures.is_empty() {
            return Ok(IterationTypes::NONE);
        }
        let mut parameter_types = Vec::new();
        let mut return_types = Vec::new();
        for signature in &signatures {
            if name != "throw" && !signature.parameters.is_empty() {
                let parameter = self.signature_type_at_position(signature, 0).ok_or(())?;
                if self.is_error(parameter) {
                    return Err(());
                }
                parameter_types.push(parameter);
            }
            if self.is_error(signature.r#type) {
                return Err(());
            }
            return_types.push(signature.r#type);
        }
        let mut returns = Vec::new();
        let mut next_type = None;
        if name != "throw" {
            let parameter_type = if parameter_types.is_empty() {
                self.intrinsics.unknown
            } else {
                self.get_union_type(&parameter_types)
            };
            if name == "next" {
                next_type = Some(parameter_type);
            } else {
                let resolved_parameter = self
                    .resolve_iteration_type(resolver, parameter_type)?
                    .unwrap_or(self.intrinsics.any);
                returns.push(resolved_parameter);
            }
        }
        let method_return = self.get_intersection_type(&return_types, None);
        let resolved_return =
            self.resolve_iteration_type(resolver, method_return)?.unwrap_or(self.intrinsics.any);
        let result = self.get_iteration_types_of_iterator_result(resolved_return)?;
        let yield_type = if result.has_types() {
            returns.push(result.return_type.ok_or(())?);
            result.yield_type
        } else {
            returns.push(self.intrinsics.any);
            Some(self.intrinsics.any)
        };
        let return_type = self.get_union_type(&returns);
        Ok(IterationTypes { yield_type, return_type: Some(return_type), next_type })
    }

    /// `getIterationTypesOfIteratorResult` (`checker.go:6649`).
    fn get_iteration_types_of_iterator_result(
        &mut self,
        ty: TypeId,
    ) -> Result<IterationTypes, Unsupported> {
        if self.is_error(ty) {
            return Err(());
        }
        if ty == self.intrinsics.any {
            return Ok(IterationTypes::all(self.intrinsics.any));
        }
        if let Some(arguments) = self.reference_to_global(ty, &["IteratorYieldResult"], 1) {
            return Ok(IterationTypes {
                yield_type: arguments.first().copied(),
                ..IterationTypes::NONE
            });
        }
        if let Some(arguments) = self.reference_to_global(ty, &["IteratorReturnResult"], 1) {
            return Ok(IterationTypes {
                return_type: arguments.first().copied(),
                ..IterationTypes::NONE
            });
        }
        let ty = self.binding_type_alias_body(ty);
        let yield_type = self.iterator_result_value(ty, self.intrinsics.false_type)?;
        let return_type = self.iterator_result_value(ty, self.intrinsics.true_type)?;
        if yield_type.is_none() && return_type.is_none() {
            return Ok(IterationTypes::NONE);
        }
        Ok(IterationTypes {
            yield_type,
            return_type: Some(return_type.unwrap_or(self.intrinsics.void)),
            next_type: None,
        })
    }

    /// `filterType(t, isYieldIteratorResult|isReturnIteratorResult)` then
    /// `getTypeOfPropertyOfType(result, "value")` (`checker.go:6663`).
    fn iterator_result_value(
        &mut self,
        ty: TypeId,
        done: TypeId,
    ) -> Result<Option<TypeId>, Unsupported> {
        let parts = match self.store.get(ty).data.clone() {
            TypeData::Union { types, .. } => types,
            _ => vec![ty],
        };
        let mut matching = Vec::new();
        for part in parts {
            if self.store.get(part).flags.contains(TypeFlags::NEVER) {
                continue;
            }
            // isIteratorResult (`checker.go:6690`): a missing `done` is false.
            let done_type = match self.get_property_of_type(part, "done") {
                Some(_) => self.get_type_of_property_of_type(part, "done").ok_or(())?,
                None if self.iteration_member_decidably_absent(part, "done") => {
                    self.intrinsics.false_type
                }
                None => return Err(()),
            };
            if self.is_error(done_type) {
                return Err(());
            }
            match self.relate_ternary(done, done_type, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => matching.push(part),
                crate::relater::Ternary::NotRelated => {}
                crate::relater::Ternary::Unknown => return Err(()),
            }
        }
        if matching.is_empty() {
            return Ok(None);
        }
        let result = self.get_union_type(&matching);
        match self.get_type_of_property_of_type(result, "value") {
            Some(value) if self.is_error(value) => Err(()),
            Some(value) => Ok(Some(value)),
            None if matching
                .iter()
                .all(|&part| self.iteration_member_decidably_absent(part, "value")) =>
            {
                Ok(None)
            }
            None => Err(()),
        }
    }

    /// `getIteratedTypeOrElementType`'s error half (`checker.go:6106`) for
    /// `checkIteratedTypeOrElementType` (`checker.go:6095`): reports
    /// `reportTypeNotIterableError` (`checker.go:6699`) on `error_node` when
    /// the protocol decidably fails. Upstream reports the union's whole type
    /// when one constituent fails (`checker.go:6293`).
    ///
    /// Only the `iterableExists` road is ported: without a global `Iterable`
    /// upstream takes the array-like road (TS2461/TS2495), which stays
    /// unreported here.
    pub(crate) fn check_iterated_type_or_element_type(
        &mut self,
        use_: IterationUse,
        input: TypeId,
        error_node: NodeId,
    ) {
        if input == self.intrinsics.any || self.is_error(input) {
            return;
        }
        let allow_async = use_.contains(IterationUse::ALLOWS_ASYNC_ITERABLES);
        if input == self.intrinsics.never {
            self.report_type_not_iterable_error(error_node, input, allow_async);
            return;
        }
        let iterable_exists = self.iteration_global("Iterable", 3).is_some();
        if !iterable_exists {
            return;
        }
        if let Ok(types) = self.get_iteration_types_of_iterable(input, use_)
            && !types.has_types()
        {
            self.report_type_not_iterable_error(error_node, input, allow_async);
        }
    }

    /// `reportTypeNotIterableError` (`checker.go:6699`). The await suggestion
    /// is related information only and is not ported.
    fn report_type_not_iterable_error(
        &mut self,
        error_node: NodeId,
        ty: TypeId,
        allow_async: bool,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(error_node) else { return };
        let message = if allow_async {
            &messages::TYPE_0_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR
        } else {
            &messages::TYPE_0_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        };
        let text = self.type_to_string(ty);
        let span = self.error_span(error_node);
        self.report(file, Diagnostic::with_args(message, span, [text]));
    }

    /// The iteration check of an array binding pattern: the element arm of
    /// `getBindingElementTypeFromParentType` (`checker.go:17749`) reports on
    /// the pattern for every named element, and `checkVariableLikeDeclaration`
    /// (`checker.go:5876`) reports on the declaration when no element has a
    /// name (`needCheckWidenedType`).
    pub(crate) fn check_array_binding_pattern_iteration(&mut self, pattern_id: NodeId) {
        if self.file_has_parse_errors
            || self.in_js_file(pattern_id)
            || self.nodes.kind(pattern_id) != SyntaxKind::ArrayBindingPattern
        {
            return;
        }
        let Some(Node::BindingPattern(pattern)) = self.node_map.get(pattern_id) else { return };
        let Some(holder) = self.nodes.parent(pattern_id) else { return };
        let named = pattern.elements.iter().find(|element| element.name.is_some());
        if let Some(element) = named {
            let Some(element_id) = element.node_id else { return };
            let parent_type = self.get_type_for_binding_element_parent(holder);
            if parent_type == self.intrinsics.any || self.is_error(parent_type) {
                return;
            }
            let parent_type = self.destructuring_parent_adjusted(element_id, holder, parent_type);
            let use_ = if element.dot_dot_dot_token.is_some() {
                IterationUse::DESTRUCTURING
            } else {
                IterationUse::DESTRUCTURING | IterationUse::POSSIBLY_OUT_OF_BOUNDS
            };
            self.check_iterated_type_or_element_type(use_, parent_type, pattern_id);
            return;
        }
        if !matches!(
            self.nodes.kind(holder),
            SyntaxKind::VariableDeclaration | SyntaxKind::Parameter | SyntaxKind::BindingElement
        ) {
            return;
        }
        // getWidenedTypeForVariableLikeDeclaration's declared half; this
        // port's parent road already widens an initializer the same way.
        let declared = self.get_type_for_binding_element_parent(holder);
        if declared == self.intrinsics.any || self.is_error(declared) {
            return;
        }
        self.check_iterated_type_or_element_type(IterationUse::DESTRUCTURING, declared, holder);
    }

    /// The source type `checkDestructuringAssignment` (`checker.go:12569`)
    /// hands an array or object literal target: the right operand of a `=`
    /// assignment, or, for an element of an array target, the element type
    /// `checkArrayLiteralDestructuringElementAssignment` (`checker.go:12663`)
    /// computes (positional access on an array-like source, else the iterated
    /// type). `None` for other positions and for undecided sources.
    pub(crate) fn destructuring_assignment_source(&mut self, node: NodeId) -> Option<TypeId> {
        let parent = self.nodes.parent(node)?;
        match self.node_map.get(parent)? {
            Node::BinaryExpression(binary) => {
                if binary.operator_token.is_none_or(|token| token.kind != SyntaxKind::EqualsToken)
                    || binary.left.and_then(|left| left.node_id()) != Some(node)
                {
                    return None;
                }
                // A `[x = d]` default is an element, not an assignment.
                if self.nodes.parent(parent).is_some_and(|grand| {
                    self.nodes.kind(grand) == SyntaxKind::ArrayLiteralExpression
                }) {
                    return None;
                }
                let right = binary.right?;
                let source = self.check_expression(right);
                (!self.is_error(source)).then_some(source)
            }
            Node::ArrayLiteralExpression(literal) => {
                let index = literal.elements.iter().position(|e| e.node_id() == Some(node))?;
                let source = self.destructuring_assignment_source(parent)?;
                if source == self.intrinsics.any {
                    return Some(source);
                }
                let element = if self.binding_parent_is_array_like(source)? {
                    let index_type = self.store.intern_literal(
                        TypeFlags::NUMBER_LITERAL,
                        TypeData::NumberLiteral(index.to_string()),
                        false,
                    );
                    let apparent = self.apparent_type(source);
                    self.resolved_indexed_access_type(apparent, index_type, false)?
                } else {
                    self.iterated_element_type(source)?
                };
                (!self.is_error(element)).then_some(element)
            }
            _ => None,
        }
    }

    /// `checkArrayLiteralAssignment` (`checker.go:12648`) for an array
    /// literal destructuring target.
    pub(crate) fn check_array_destructuring_assignment_iteration(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(source) = self.destructuring_assignment_source(node) else { return };
        self.check_iterated_type_or_element_type(
            IterationUse::DESTRUCTURING | IterationUse::POSSIBLY_OUT_OF_BOUNDS,
            source,
            node,
        );
    }

    /// The spread arm of `checkArrayLiteral` (`checker.go:8064`) and of
    /// `getSpreadArgumentType` (`checker.go:29540`): a spread whose operand is
    /// not array-like is iterated, reporting on the operand. A spread inside
    /// a destructuring target is a rest element and reports nothing.
    pub(crate) fn check_spread_element_iteration(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::SpreadElement(spread)) = self.node_map.get(node) else { return };
        let Some(expression) = spread.expression else { return };
        let Some(expression_id) = expression.node_id() else { return };
        let Some(parent) = self.nodes.parent(node) else { return };
        match self.nodes.kind(parent) {
            SyntaxKind::ArrayLiteralExpression => {
                if self.assignment_target_kind(parent)
                    != crate::expressions::AssignmentTargetKind::None
                {
                    return;
                }
            }
            SyntaxKind::CallExpression | SyntaxKind::NewExpression => {}
            _ => return,
        }
        let spread_type = self.check_expression(expression);
        if spread_type == self.intrinsics.any || self.is_error(spread_type) {
            return;
        }
        if self.binding_parent_is_array_like(spread_type) != Some(false) {
            return;
        }
        self.check_iterated_type_or_element_type(IterationUse::SPREAD, spread_type, expression_id);
    }

    /// The `ForInStatement` arm of `getTypeForVariableLikeDeclaration`
    /// (`checker.go:16658`): the index type of the (non-nullable) iterated
    /// expression, as `getExtractStringType` (`checker.go:26709`) when it is a
    /// type parameter or deferred `keyof`, else `string`.
    pub(crate) fn for_in_variable_type(&mut self, statement: NodeId) -> TypeId {
        let string = self.intrinsics.string;
        let Some(Node::ForInOrOfStatement(for_in)) = self.node_map.get(statement) else {
            return string;
        };
        let Some(expression) = for_in.expression else { return string };
        let checked = self.check_expression(expression);
        if self.is_error(checked) {
            return string;
        }
        // getNonNullableTypeIfNeeded.
        let checked = if self
            .get_type_facts(checked)
            .intersects(crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL)
        {
            self.get_non_nullable_type(checked)
        } else {
            checked
        };
        let Some(index_type) = self.resolved_keyof_type(checked) else { return string };
        if !self
            .store
            .get(index_type)
            .flags
            .intersects(TypeFlags::TYPE_PARAMETER | TypeFlags::INDEX)
        {
            return string;
        }
        // getGlobalExtractSymbol, then getTypeAliasInstantiation.
        let Some(extract) = self.global_type_symbol_with_arity("Extract", 2) else { return string };
        self.create_type_reference(extract, vec![index_type, string])
    }

    /// `getYieldedTypeOfYieldExpression` (`checker.go:11019`)'s iteration
    /// check for a `yield*` inside a generator, reported on the operand.
    pub(crate) fn check_yield_star_iteration(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::YieldExpression(yield_expression)) = self.node_map.get(node) else {
            return;
        };
        if yield_expression.asterisk_token.is_none() {
            return;
        }
        let Some(operand) = yield_expression.expression else { return };
        let Some(operand_id) = operand.node_id() else { return };
        let Some(container) = self.containing_function(node) else { return };
        let (asterisk, modifiers) = match self.node_map.get(container) {
            Some(Node::FunctionDeclaration(f)) => (f.asterisk_token, f.modifiers),
            Some(Node::MethodDeclaration(f)) => (f.asterisk_token, f.modifiers),
            Some(Node::FunctionExpression(f)) => (f.asterisk_token, f.modifiers),
            _ => return,
        };
        if asterisk.is_none() {
            return;
        }
        let is_async = modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == SyntaxKind::AsyncKeyword)
        });
        let use_ = if is_async { IterationUse::ASYNC_YIELD_STAR } else { IterationUse::YIELD_STAR };
        let operand_type = self.check_expression(operand);
        self.check_iterated_type_or_element_type(use_, operand_type, operand_id);
    }

    /// The type half of `checkRightHandSideOfForOf` (`checker.go:17678`):
    /// `checkIteratedTypeOrElementType` (`checker.go:6095`) of the non-null
    /// operand on the iterable road (`getIteratedTypeOrElementType`,
    /// `checker.go:6116`) — the yield type, orElse `any` when the operand is
    /// `never` or not iterable (both reported by
    /// [`Self::check_for_of_iteration`]). `None` when the global `Iterable`
    /// is absent (the array-like road, not ported here) or the engine cannot
    /// decide.
    pub(crate) fn for_of_iterated_type(&mut self, statement: NodeId) -> Option<TypeId> {
        let Some(Node::ForInOrOfStatement(for_of)) = self.node_map.get(statement) else {
            return None;
        };
        let expression = for_of.expression?;
        let use_ = if for_of.await_modifier.is_some() {
            IterationUse::FOR_AWAIT_OF
        } else {
            IterationUse::FOR_OF
        };
        self.iteration_global("Iterable", 3)?;
        let checked = self.check_expression(expression);
        let input = self.check_non_null_type(checked);
        if input == self.intrinsics.any || input == self.intrinsics.never {
            return Some(self.intrinsics.any);
        }
        if self.is_error(input) {
            return None;
        }
        let types = self.get_iteration_types_of_iterable(input, use_).ok()?;
        Some(types.yield_type.unwrap_or(self.intrinsics.any))
    }

    /// `checkRightHandSideOfForOf` (`checker.go:17678`)'s iteration check.
    pub(crate) fn check_for_of_iteration(&mut self, node: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        let Some(Node::ForInOrOfStatement(statement)) = self.node_map.get(node) else { return };
        if self.nodes.kind(node) != SyntaxKind::ForOfStatement {
            return;
        }
        let Some(expression) = statement.expression else { return };
        let Some(expression_id) = expression.node_id() else { return };
        let use_ = if statement.await_modifier.is_some() {
            IterationUse::FOR_AWAIT_OF
        } else {
            IterationUse::FOR_OF
        };
        let checked = self.check_expression(expression);
        let input = self.check_non_null_type(checked);
        self.check_iterated_type_or_element_type(use_, input, expression_id);
    }
}
