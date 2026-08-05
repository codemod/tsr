//! Union types: building one, ordering its constituents, and printing it.
//!
//! Ported from `Checker.getUnionType` / `getUnionTypeEx` / `getUnionTypeWorker`
//! (`checker.go:25617`, `:25628`, `:25653`), `addTypesToUnion`
//! (`checker.go:25761`), `removeRedundantLiteralTypes` (`checker.go:25838`),
//! `getUnionTypeFromSortedList` (`checker.go:25736`), `CompareTypes`
//! (`utilities.go:415`) and `formatUnionTypes` (`printer.go:383`).
//!
//! # The constituent order is the answer, not a rendering detail
//!
//! `A | B` is printed verbatim in every `.types` baseline, so a union whose
//! constituents are right and whose order is wrong fails a line as surely as a
//! wrong type — and it fails looking like a formatting bug. The order comes from
//! the **numeric values of `TypeFlags`**, which upstream's comment at
//! `types.go:420` says in terms, and which this port already carried across
//! one-for-one ([`crate::flags`]). `let x: number | string` prints
//! `string | number`, because `STRING` is `1 << 5` and `NUMBER` is `1 << 6`
//! (`baselines/reference/submodule/compiler/implicitConstParameters.types:15`).
//!
//! # `strictNullChecks` is assumed off, and that is visible here
//!
//! `addTypesToUnion` (`checker.go:25793`) **drops `null` and `undefined`
//! constituents entirely** when `strictNullChecks` is off, so `string | undefined`
//! is `string` under upstream's default options and `string | undefined` under
//! `--strict`. This port has no compiler options at all — nothing constructs a
//! `tsr_core::CompilerOptions` and [`Checker::new`](crate::Checker::new) takes a
//! bound file and nothing else — so one of the two behaviours has to be picked.
//!
//! **Off is picked**, on two grounds: it is upstream's default, and 1,351 of the
//! corpus's 12,444 cases (10.9%) set `@strict` or `@strictNullChecks`, so it is
//! also the majority behaviour by an order of magnitude. It is additionally the
//! assumption the rest of this crate already makes implicitly — nothing adds
//! `undefined` to an optional parameter's type either.
//!
//! **The consequence accepted:** every union mentioning `null` or `undefined` in
//! a `@strict` case is a *wrong* line rather than a gap, and those lines cannot
//! be distinguished from real defects by the histogram. **How this would be shown
//! wrong:** when the checker can read the case's options (`bd tsr-5s2`, behind
//! the program object `bd tsr-9or.1`) this assumption becomes a one-line lookup,
//! and the ported code below has the branch in the right place already.
//!
//! # What is deliberately not here
//!
//! - **Subtype reduction** (`removeSubtypes`, `checker.go:25934`) needs
//!   assignability, which does not exist. Only `UnionReductionLiteral` is
//!   reachable, which is what `getUnionType` and `getTypeFromUnionTypeNode` both
//!   ask for anyway; `UnionReductionSubtype` arrives with `||` and `??`.
//! - **`origin` and `addNamedUnions`** (`checker.go:25705`), which keep
//!   `E | string` printing as `E | string` instead of expanding `E`'s members.
//!   A union with a *named* union among its constituents is a gap here
//!   (`bd tsr-ha6`).
//! - **`formatUnionTypes`' nullable reordering and enum collapsing**
//!   (`printer.go:383`). Both are unreachable given the two decisions above:
//!   nullable constituents are dropped before printing, and an enum-like
//!   constituent only ever reaches a union through its own enum type, which is
//!   gapped (`bd tsr-8pz`). Porting them would read as coverage and provide
//!   none — the same judgement `crate::declared` made about instantiation depth.
//! - **A construction limit.** Upstream guards union construction with no count
//!   of its own; what it guards is `instantiateType` (`checker.go:22111`), which
//!   is what makes unions of unions grow. The recursion that exists here is over
//!   the *source* nesting of type nodes, which is finite in a parsed file, and
//!   the one recursion that could not be — a self-referential alias — terminates
//!   because a generic alias's body is never expanded. `bd tsr-el3.2` owns the
//!   limits, and they are not optional once instantiation is real.

use std::cmp::Ordering;

use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    flags::TypeFlags,
    printing,
    types::{TypeData, TypeId, TypeStore},
};

/// What `addTypesToUnion` learned about the types it walked.
///
/// Upstream returns a single `TypeFlags` and packs the extra questions into
/// high bits it reserves for the purpose — `TypeFlagsIncludesError`,
/// `TypeFlagsIncludesWildcard` and the rest of `TypeFlagsIncludesMask`
/// (`types.go`). Those bit values are *not* type flags, so reproducing them in
/// [`TypeFlags`] would put non-types in a set whose numeric values are already
/// load-bearing for ordering. A struct instead.
#[derive(Debug, Default, Clone, Copy)]
struct Includes {
    /// The union of every walked type's flags, recorded *before* nullable types
    /// are dropped — which is what makes the `undefined`-with-`void` and
    /// literal-with-base reductions see them.
    flags: TypeFlags,
    /// `TypeFlagsIncludesError`: one of the constituents was `errorType`.
    error: bool,
    /// A constituent was a union that prints as a name — an enum's declared type
    /// or the body of a named type alias. Upstream keeps those unexpanded
    /// through `origin`; this port has no origin and gaps instead.
    named_union: bool,
}

/// Create a union type, interned on its contents.
///
/// Ported from `Checker.newUnionType` + `getUnionTypeFromSortedList`'s cache
/// (`checker.go:25188`, `:25736`). A free function over the store rather than a
/// [`Checker`] method because [`Intrinsics::create`](crate::Intrinsics::create)
/// needs it before a `Checker` exists — see [`create_boolean_type`].
///
/// `types` must already be sorted, deduplicated and non-empty.
fn create_union(
    store: &mut TypeStore,
    extra_flags: TypeFlags,
    types: Vec<TypeId>,
    symbol: Option<(SymbolId, String)>,
) -> TypeId {
    let mut flags = TypeFlags::UNION | extra_flags;
    // `getUnionTypeFromSortedList` (`checker.go:25749`): a union of exactly the
    // two boolean literal types *is* `boolean`, and carries the flag to say so.
    // That is the whole mechanism by which `boolean` is a union upstream and
    // still prints as a keyword.
    if types.len() == 2
        && types.iter().all(|&id| store.get(id).flags.contains(TypeFlags::BOOLEAN_LITERAL))
    {
        flags |= TypeFlags::BOOLEAN;
    }
    let text = match &symbol {
        // The node builder reaches a named union through its enum-like branch
        // (`nodebuilderimpl.go:3260`) or its alias branch (`:3362`), and both
        // print the symbol's name rather than the constituents. That is why an
        // enum's declared type — a union of its members — prints `E`.
        Some((_, name)) => name.clone(),
        None => format_union_types(store, &types).join(" | "),
    };
    let symbol = symbol.map(|(id, _)| id);
    store.intern_union(flags, TypeData::Union { text, types, symbol })
}

/// The constituents as they are *printed*: `Checker.formatUnionTypes`
/// (`printer.go:383`), restricted to its boolean-literal clause.
///
/// A `boolean` inside a union is flattened into `false` and `true` when the
/// union is built, so something has to put it back together again for printing —
/// `string | boolean` is what upstream records
/// (`baselines/reference/submodule/conformance/…`), not `string | false | true`.
/// Upstream does it here rather than by keeping `boolean` whole, because the
/// *reduction* has to see the two literals: that is what makes `boolean | true`
/// be `boolean`.
///
/// The condition is upstream's: a boolean literal followed by one whose regular
/// form is `regularTrueType`. Sorting guarantees `false` precedes `true`
/// (`utilities.go:523`) and deduplication guarantees at most one of each, so
/// testing the *value* of the second is the same question as upstream's identity
/// comparison against `booleanType`'s last constituent.
///
/// **Two of upstream's clauses are not here.** Nullable constituents are moved to
/// the end of the printed list, and enum-like constituents are collapsed to their
/// base enum type. Neither can occur: nullable types are dropped while the union
/// is built (see the module docs), and an enum member only reaches a union
/// through its own enum type, which is gapped.
///
/// **This function was originally not ported at all**, on the argument that the
/// `TypeFlagsBoolean` keyword check in the node builder (`nodebuilderimpl.go:3255`)
/// already handles `boolean`. That argument is wrong and a test caught it: the
/// keyword check only fires for a union of *exactly* two boolean literals, so
/// `string | boolean` printed `string | false | true`. The two mechanisms
/// coincide only on the bare `boolean`, which is why the keyword check is no
/// longer consulted for printing at all — it would be a branch this one already
/// covers.
fn format_union_types(store: &TypeStore, types: &[TypeId]) -> Vec<String> {
    let is_boolean_literal = |id: TypeId| matches!(store.get(id).data, TypeData::BooleanLiteral(_));
    let mut printed = Vec::with_capacity(types.len());
    let mut index = 0;
    while index < types.len() {
        let id = types[index];
        if is_boolean_literal(id)
            && matches!(
                types.get(index + 1).map(|&next| &store.get(next).data),
                Some(TypeData::BooleanLiteral(true))
            )
        {
            printed.push("boolean".to_string());
            index += 2;
            continue;
        }
        printed.push(printing::type_to_string(store.get(id)));
        index += 1;
    }
    printed
}

/// `booleanType` — the union `false | true` (`checker.go:1002`).
///
/// Upstream builds it with `getUnionType` like any other union. Here it is built
/// directly, because it is created inside
/// [`Intrinsics::create`](crate::Intrinsics::create) — the general path is a
/// [`Checker`] method and needs the intrinsics that call is producing.
///
/// **The shortcut is exact, not an approximation.** Every reduction the general
/// path applies is a no-op on two boolean literal types: neither is `never`,
/// `any`, `unknown` or nullable; `removeRedundantLiteralTypes` has no clause for
/// boolean literals (`checker.go:25843`) and neither is fresh; and the pair is
/// already in `CompareTypes` order, since `false` sorts before `true`
/// (`utilities.go:523`). That claim is not left to the reader —
/// `the_boolean_intrinsic_is_the_union_the_general_path_builds` asserts the two
/// routes produce the same [`TypeId`].
pub(crate) fn create_boolean_type(
    store: &mut TypeStore,
    regular_false: TypeId,
    regular_true: TypeId,
) -> TypeId {
    create_union(store, TypeFlags::empty(), vec![regular_false, regular_true], None)
}

/// `getSortOrderFlags` (`utilities.go:581`).
///
/// Every enum-like *unit* type sorts as though it were `TypeFlagsEnum`, so that
/// enum members of one enum stay together whichever literal kind they are.
fn sort_order_flags(flags: TypeFlags) -> u32 {
    if flags.intersects(TypeFlags::ENUM_LITERAL | TypeFlags::ENUM)
        && !flags.contains(TypeFlags::UNION)
    {
        return TypeFlags::ENUM.bits();
    }
    flags.bits()
}

/// The name a type sorts under: `getTypeNameSymbol` (`utilities.go:608`).
///
/// Upstream returns the type's alias symbol or its own symbol and compares
/// `Symbol.Name`. This port compares the **printed form** instead, because a
/// [`TypeData::Named`] carries its name as text and not as a symbol.
///
/// The two agree for every plain named type, where the printed form *is* the
/// symbol name. They part company for two references to the same generic:
/// upstream compares the symbol names (equal), then the type-argument lists by
/// `CompareTypes`, so `C<string> | C<number>` keeps `string` before `number`;
/// this compares `"C<number>"` against `"C<string>"` and orders them the other
/// way. Recorded rather than fixed, because fixing it means giving
/// `TypeData::Named` a symbol and an argument list, which is a reshape of a type
/// two workstreams share. `bd tsr-bgz`.
fn type_name(data: &TypeData) -> Option<&str> {
    match data {
        TypeData::Named { text, .. } | TypeData::Union { text, symbol: Some(_), .. } => Some(text),
        _ => None,
    }
}

impl Checker<'_, '_> {
    /// `Checker.getUnionType` (`checker.go:25617`) — `UnionReductionLiteral`,
    /// no alias.
    ///
    /// The entry point for every union that is not a named one: a union type
    /// node without an enclosing type alias, and (when they land) the logical
    /// operators.
    pub(crate) fn get_union_type(&mut self, types: &[TypeId]) -> TypeId {
        // `getUnionTypeEx` (`checker.go:25630`): the empty union is `never` and
        // the one-element union is that element. Both are upstream's, and the
        // second is why `type T = string | string` prints `string`.
        if types.is_empty() {
            return self.intrinsics.never;
        }
        if types.len() == 1 {
            return types[0];
        }
        self.union_type_worker(types, TypeFlags::empty(), None)
    }

    /// `getUnionTypeEx` with an alias (`checker.go:25628`), **minus the
    /// single-constituent collapse**.
    ///
    /// # A deviation, confined to one-member enums
    ///
    /// Upstream returns the constituent itself when there is one of them, alias
    /// or not, so `enum E { A }` declares the type `E.A` rather than a union.
    /// That still *prints* `E`, because the node builder asks
    /// `getDeclaredTypeOfSymbol(parent) == t` and substitutes the enum's name
    /// (`nodebuilderimpl.go:3262`).
    ///
    /// This port prints from text computed when a type is created, so it cannot
    /// ask a question whose answer arrives later. Keeping the one-element union
    /// is the smaller of the two available errors: the printed line stays right
    /// in both positions, and the divergence is one extra layer of union around
    /// a single member rather than a type with no constituents at all.
    ///
    /// **How this would be shown wrong:** when printing reads the store instead
    /// of a cached string, this special case must be deleted and the collapse
    /// restored — a one-member enum whose declared type is a union will show up
    /// the moment anything compares an enum type for identity against its single
    /// member's type.
    pub(crate) fn get_named_union_type(
        &mut self,
        types: &[TypeId],
        extra_flags: TypeFlags,
        symbol: SymbolId,
    ) -> TypeId {
        if types.is_empty() {
            return self.intrinsics.never;
        }
        self.union_type_worker(types, extra_flags, Some(symbol))
    }

    /// `Checker.getUnionTypeWorker` (`checker.go:25653`) at
    /// `UnionReductionLiteral`.
    fn union_type_worker(
        &mut self,
        types: &[TypeId],
        extra_flags: TypeFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        let (mut set, includes) = self.add_types_to_union(types);

        // Upstream would build a denormalised `origin` here so the named union
        // prints unexpanded (`checker.go:25705`). Without it the constituents
        // would be printed instead — `E.A | E.B | string` where upstream writes
        // `E | string` — which is a wrong line rather than a missing one.
        if includes.named_union {
            return self.intrinsics.error;
        }

        if includes.flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            if includes.flags.contains(TypeFlags::ANY) {
                // `checker.go:25659`: `IncludesError` wins over `IncludesAny`,
                // which is upstream's own "a gap in a constituent is a gap in
                // the union" and needs no deviation from this port.
                return if includes.error { self.intrinsics.error } else { self.intrinsics.any };
            }
            return self.intrinsics.unknown;
        }

        if includes
            .flags
            .intersects(TypeFlags::ENUM | TypeFlags::LITERAL | TypeFlags::UNIQUE_ES_SYMBOL)
        {
            set = self.remove_redundant_literal_types(set, includes.flags);
        }

        if set.is_empty() {
            // Everything was dropped. With `strictNullChecks` off that means the
            // union was nothing but `null` and `undefined`, where upstream
            // answers `nullWideningType` or `undefinedWideningType`
            // (`checker.go:25692`) — types this port does not model and which
            // are *not* `nullType` and `undefinedType`. Answering with those
            // would merge identities upstream keeps apart, so this is a gap.
            if includes.flags.intersects(TypeFlags::NULLABLE) {
                return self.intrinsics.error;
            }
            return self.intrinsics.never;
        }

        self.get_union_type_from_sorted_list(set, extra_flags, symbol)
    }

    /// `Checker.getUnionTypeFromSortedList` (`checker.go:25736`).
    fn get_union_type_from_sorted_list(
        &mut self,
        types: Vec<TypeId>,
        extra_flags: TypeFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        if types.len() == 1 && symbol.is_none() {
            return types[0];
        }
        let named = symbol.map(|id| (id, self.binder.symbols().get(id).name.to_string()));
        create_union(&mut self.store, extra_flags, types, named)
    }

    /// `Checker.addTypesToUnion` (`checker.go:25761`).
    ///
    /// Flattens nested unions, drops `never`, drops nullable types (see the
    /// module docs on `strictNullChecks`), then sorts by [`Self::compare_types`]
    /// and removes adjacent duplicates.
    fn add_types_to_union(&self, source: &[TypeId]) -> (Vec<TypeId>, Includes) {
        let mut types: Vec<TypeId> = Vec::with_capacity(source.len());
        let mut includes = Includes::default();
        let mut last: Option<TypeId> = None;
        for &id in source {
            if last == Some(id) {
                continue;
            }
            last = Some(id);
            let flags = self.store.get(id).flags;
            if flags.contains(TypeFlags::UNION) {
                let (constituents, named) = match &self.store.get(id).data {
                    TypeData::Union { types, symbol, .. } => (types.clone(), symbol.is_some()),
                    // A `UNION`-flagged type whose payload is not a union cannot
                    // be built by this module, and inventing a constituent list
                    // for one would be worse than treating it as opaque.
                    _ => (vec![id], false),
                };
                includes.named_union |= named;
                for constituent in constituents {
                    self.add_type_to_union(&mut types, &mut includes, constituent);
                }
            } else {
                self.add_type_to_union(&mut types, &mut includes, id);
            }
        }
        if types.len() >= 2 {
            types.sort_by(|&a, &b| self.compare_types(a, b));
            types.dedup();
        }
        (types, includes)
    }

    /// The `addType` closure inside `addTypesToUnion` (`checker.go:25764`).
    fn add_type_to_union(&self, types: &mut Vec<TypeId>, includes: &mut Includes, id: TypeId) {
        let flags = self.store.get(id).flags;
        // "We ignore 'never' types in unions" (`checker.go:25767`).
        if flags.contains(TypeFlags::NEVER) {
            return;
        }
        includes.flags |= flags;
        if self.is_error(id) {
            includes.error = true;
        }
        // `checker.go:25793`, with `strictNullChecks` assumed off — see the
        // module docs. This is the single line the assumption lives on.
        if flags.intersects(TypeFlags::NULLABLE) {
            return;
        }
        types.push(id);
    }

    /// `Checker.removeRedundantLiteralTypes` (`checker.go:25838`) at
    /// `reduceVoidUndefined == false`, which is what `UnionReductionLiteral`
    /// passes.
    ///
    /// A literal is redundant beside its own base primitive — `string | "a"` is
    /// `string` — and a *fresh* literal is redundant beside its regular twin.
    ///
    /// Two of upstream's clauses are omitted rather than written and left
    /// unreachable: template-literal and string-mapping types do not exist in
    /// this port, and the `undefined`-beside-`void` clause is guarded by
    /// `reduceVoidUndefined`, which only `UnionReductionSubtype` sets.
    fn remove_redundant_literal_types(
        &mut self,
        mut types: Vec<TypeId>,
        includes: TypeFlags,
    ) -> Vec<TypeId> {
        let mut index = types.len();
        while index > 0 {
            index -= 1;
            let id = types[index];
            let ty = self.store.get(id);
            let (flags, fresh) = (ty.flags, ty.fresh);
            let remove = flags.contains(TypeFlags::STRING_LITERAL)
                && includes.contains(TypeFlags::STRING)
                || flags.contains(TypeFlags::NUMBER_LITERAL)
                    && includes.contains(TypeFlags::NUMBER)
                || flags.contains(TypeFlags::BIG_INT_LITERAL)
                    && includes.contains(TypeFlags::BIG_INT)
                || flags.contains(TypeFlags::UNIQUE_ES_SYMBOL)
                    && includes.contains(TypeFlags::ES_SYMBOL)
                || fresh && {
                    let regular = self.get_regular_type_of_literal_type(id);
                    types.contains(&regular)
                };
            if remove {
                types.remove(index);
            }
        }
        types
    }

    /// `CompareTypes` (`utilities.go:415`), reduced to the types this port has.
    ///
    /// The order of the tests is upstream's and is what produces the printed
    /// constituent order: sort-order flags, then name, then per-kind data, then
    /// creation order.
    fn compare_types(&self, a: TypeId, b: TypeId) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        let (left, right) = (self.store.get(a), self.store.get(b));
        sort_order_flags(left.flags)
            .cmp(&sort_order_flags(right.flags))
            .then_with(|| match (type_name(&left.data), type_name(&right.data)) {
                (Some(x), Some(y)) => x.cmp(y),
                // A type with no name sorts after one with a name
                // (`utilities.go:614`).
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| match (&left.data, &right.data) {
                // "String literal types are ordered by their values."
                (TypeData::StringLiteral(x), TypeData::StringLiteral(y)) => x.cmp(y),
                // "Numeric literal types are ordered by their values" — by
                // *value*, so `"10"` sorts after `"9"`. The value is recovered
                // from the normalised text, which round-trips for every literal
                // `crate::printing::normalise_number` can format.
                (TypeData::NumberLiteral(x), TypeData::NumberLiteral(y)) => x
                    .parse::<f64>()
                    .ok()
                    .zip(y.parse::<f64>().ok())
                    .and_then(|(x, y)| x.partial_cmp(&y))
                    .unwrap_or(Ordering::Equal),
                // `false` before `true` (`utilities.go:523`), which is what
                // makes `boolean` print as `false | true` when it is expanded.
                (TypeData::BooleanLiteral(x), TypeData::BooleanLiteral(y)) => x.cmp(y),
                // Enum members and type parameters are ordered by their symbols'
                // declaration positions upstream (`compareSymbols`). Here they
                // fall through to the type-id tiebreak below, which is creation
                // order — and enum member types are created in declaration
                // order, so the two agree for the case that reaches this.
                _ => Ordering::Equal,
            })
            // "Fall back to type IDs. This results in type creation order."
            .then(a.cmp(&b))
    }
}
