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
//! # `strictNullChecks` is assumed **on**, and that is visible here
//!
//! `addTypesToUnion` (`checker.go:25783`) **drops `null` and `undefined`
//! constituents entirely** when `strictNullChecks` is *off*, so
//! `let opt: number | undefined` prints `number` under `@strict: false` and
//! `number | undefined` otherwise. This port has no compiler options at all —
//! nothing constructs a `tsr_core::CompilerOptions` and
//! [`Checker::new`](crate::Checker::new) takes a bound file and nothing else —
//! so one of the two behaviours has to be picked.
//!
//! **On is picked, and this reverses the first answer.** The first version of
//! this module assumed *off*, reasoning that it was upstream's default. It is
//! not. `CompilerOptions.GetStrictOptionValue` (`internal/core/compileroptions.go:294`)
//! returns `options.Strict != TSFalse`, so an **unset** `strict` yields
//! `strictNullChecks: true`. Counted over the corpus: 2,170 of 12,444 cases
//! (17.4%) turn it off explicitly, 1,351 turn it on explicitly, and the
//! remaining 8,923 leave it unset — **so it is on for 82.6% of cases**, not off
//! for 89% as first recorded.
//!
//! Two baselines pin both directions, and they were what settled it:
//! `predicateSemantics.ts` sets `@strict: false` and records `opt: number | undefined`
//! as `>opt : number`, while `useRegexpGroups.ts` sets no strictness at all and
//! records `>result : RegExpExecArray | null`.
//!
//! **The consequence accepted:** a union mentioning `null` or `undefined` in one
//! of the 2,170 explicitly-non-strict cases is a *wrong* line rather than a gap.
//! That is the same shape of cost as before and one fifth of the size. **How
//! this would be shown wrong:** when the checker can read the case's options
//! (`bd tsr-5s2`, behind the program object `bd tsr-9or.1`) the assumption
//! becomes a one-line lookup, and the branch is already in the right place.
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
/// # Nullable constituents print last, whatever the sort says
///
/// `UNDEFINED` is `1 << 2` and `NULL` is `1 << 3`, so `CompareTypes` puts both
/// at the *front* of the constituent list — and upstream prints them at the
/// **end**, appending `c.nullType` and then `c.undefinedType` after everything
/// else (`printer.go:407`). So `string | undefined` is stored `[undefined,
/// string]` and printed `string | undefined`. This is not cosmetic: 6,811
/// baseline lines end in `| undefined`, only 28 begin with a nullable
/// constituent, and `null` never follows `undefined`.
///
/// The names are hardcoded because upstream appends the *canonical* intrinsics
/// rather than the constituents it found — which is also how `nullWideningType`
/// comes to print `null`. This port does not model the widening variants, so the
/// two coincide.
///
/// **One of upstream's clauses is still not here.** Enum-like constituents are
/// collapsed to their base enum type; an enum member only reaches a union
/// through its own enum type, which is gapped (`bd tsr-8pz`).
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
    let mut seen = TypeFlags::empty();
    let mut index = 0;
    while index < types.len() {
        let id = types[index];
        seen |= store.get(id).flags;
        if store.get(id).flags.intersects(TypeFlags::NULLABLE) {
            index += 1;
            continue;
        }
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
        printed.push(parenthesised(store, id));
        index += 1;
    }
    // `null` first, then `undefined` — upstream's order (`printer.go:407`), and
    // `>d : object | null | undefined` is what the baselines record.
    if seen.contains(TypeFlags::NULL) {
        printed.push("null".to_string());
    }
    if seen.contains(TypeFlags::UNDEFINED) {
        printed.push("undefined".to_string());
    }
    printed
}

/// A union constituent as it is printed, parenthesised where upstream would.
///
/// `emitUnionTypeConstituent` (`printer.go:2038`) is
/// `emitTypeNode(node, TypePrecedenceTypeOperator)`, and `emitTypeNode`
/// (`printer.go:2274`) writes a `(` when
/// `GetTypeNodePrecedence(node) < precedence`. Reading the ladder
/// (`ast/precedence.go:425`–`:480`, ascending: `Conditional` = `Lowest`,
/// `JSDoc`, `Function`, `Union`, `Intersection`, `TypeOperator`, `Postfix`,
/// `NonArray` = `Highest`), the constituents that sort below `TypeOperator` are
/// **conditional, JSDoc optional/variadic, function, constructor, nested union
/// and intersection** types.
///
/// # What this port can produce from that list, and what it deliberately cannot
///
/// - **Intersection** — `A & B | C & D` must print `(A & B) | (C & D)`.
/// - **A function type** — `(() => boolean) | undefined`. Recorded as
///   [`TypeData::Anonymous::signature`] at creation, because the text is built
///   there and that is the last point at which the node kind is known.
/// - **Nested unions cannot occur**: [`Checker::add_types_to_union`] flattens
///   them, so the arm is omitted rather than written and left unreachable.
/// - **Conditional and JSDoc types are unported.**
///
/// The two near misses are the reason this is a node-kind test and not a
/// text test: `typeof C` is a `TypeQueryNode`, which upstream gives
/// `TypePrecedenceTypeOperator` **so that it parenthesises in postfix position**
/// (`(typeof C)[]`) and not here; and `{ f: () => void; }` is a
/// `TypeLiteralNode` at `NonArray`, the highest precedence, despite containing
/// `=>`.
fn parenthesised(store: &TypeStore, id: TypeId) -> String {
    let ty = store.get(id);
    let needs = match &ty.data {
        // An intersection **that prints as `A & B`**. One that a type alias
        // names prints as that name, which the node builder emits as a
        // `TypeReferenceNode` at the highest precedence — the distinction this
        // rule got wrong on its first run, at 19 lines.
        TypeData::Intersection { .. } => !printing::prints_as_a_single_token(ty),
        TypeData::Anonymous { signature, .. } => *signature,
        _ => false,
    };
    let text = printing::type_to_string(ty);
    if needs { format!("({text})") } else { text }
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
        // `T | T` is `T` by identity for ANY `T` — including a named union,
        // which the worker below would otherwise expand and then decline for
        // want of upstream's `origin` denormalisation
        // (`checker-notes-arrays.md` §5, `enumLiteralsSubtypeReduction`).
        if types.iter().all(|&t| t == types[0]) {
            return types[0];
        }
        self.union_type_worker(types, TypeFlags::empty(), None, false)
    }

    /// [`Checker::get_union_type`] for a consumer that will never **print** the
    /// result.
    ///
    /// The worker answers `errorType` for any union with a *named* constituent
    /// — an enum, or a union type alias — because this port computes a type's
    /// printed text when the type is created and upstream's `origin`
    /// denormalisation (`checker.go:25705`) is unported, so `E | undefined`
    /// would print `E.a | E.b | undefined`. That guard is about the **printed
    /// line** and nothing else: upstream has no such case.
    ///
    /// A diagnostic that compares `(file, line, column, code)` never prints a
    /// type, so for those consumers the guard converts a right answer into an
    /// `errorType` that silences the rule. `check_used_before_assigned` is the
    /// first: `var c: E;` reached `getOptionalType` and got `errorType` back on
    /// every enum-typed case in the corpus — `checker-notes-diag2.md` §42.1.
    ///
    /// **How this would be shown wrong:** a caller printing a type obtained
    /// through here would emit expanded constituents. Nothing may call it from
    /// the query road, which is what keeps the `checker_types` gradient
    /// unmoved.
    pub(crate) fn get_union_type_unprinted(&mut self, types: &[TypeId]) -> TypeId {
        if types.is_empty() {
            return self.intrinsics.never;
        }
        if types.len() == 1 || types.iter().all(|&t| t == types[0]) {
            return types[0];
        }
        self.union_type_worker(types, TypeFlags::empty(), None, true)
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
        self.union_type_worker(types, extra_flags, Some(symbol), false)
    }

    /// `Checker.getUnionTypeWorker` (`checker.go:25653`) at
    /// `UnionReductionLiteral`.
    fn union_type_worker(
        &mut self,
        types: &[TypeId],
        extra_flags: TypeFlags,
        symbol: Option<SymbolId>,
        unprinted: bool,
    ) -> TypeId {
        let (mut set, includes) = self.add_types_to_union(types);

        // Upstream would build a denormalised `origin` here so the named union
        // prints unexpanded (`checker.go:25705`). Without it the constituents
        // would be printed instead — `E.A | E.B | string` where upstream writes
        // `E | string` — which is a wrong line rather than a missing one.
        if includes.named_union && !unprinted {
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
            // With `strictNullChecks` off a union of nothing but `null` and
            // `undefined` empties the set, and upstream answers
            // `undefinedWideningType` or `nullWideningType`
            // (`checker.go:25692`). Neither widening intrinsic exists here —
            // the same recorded divergence as `Checker::with_module_host`'s
            // `undefined` seeding — and answering the non-widening twins would
            // merge identities upstream keeps apart, so this is a gap.
            if !self.strict_null_checks && includes.flags.intersects(TypeFlags::NULLABLE) {
                return self.intrinsics.error;
            }
            // Every constituent was `never`, the only other way to empty the
            // set.
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
    /// Flattens nested unions, drops `never`, then sorts by
    /// [`Self::compare_types`] and removes adjacent duplicates. Nullable types
    /// are **kept** — see the module docs on `strictNullChecks`.
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
        // `checker.go:25783`: with `strictNullChecks` off, `null` and
        // `undefined` never enter a union's constituent set — `T | undefined |
        // null` *is* `T` in that mode, and the baselines print it that way
        // (`lib.es5.d.ts`'s `then` renders with both stripped in every
        // non-strict case). Their flags still reach `includes` above, which is
        // what the all-nullable fallback in `union_type_worker` reads. When
        // the flag is on, the constituents survive into the type and
        // `format_union_types` moves them to the end of the printed form.
        if !self.strict_null_checks && flags.intersects(TypeFlags::NULLABLE) {
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

    /// `compareTypeNames` (`utilities.go:589`) and the type-reference half of
    /// `CompareTypes`'s object arm (`:446`–`:460`).
    ///
    /// # Why a type reference cannot be compared by its printed text
    ///
    /// `getTypeNameSymbol` (`utilities.go:607`) returns the **target** symbol
    /// for a reference, so `number[]` and `string[]` both answer `Array`,
    /// `s1 == s2`, and `compareTypeNames` returns 0. Upstream then falls into
    /// the object arm and reaches `compareTypeLists(resolvedTypeArguments)`
    /// (`:660`), which is `CompareTypes` elementwise — and `CompareTypes` on
    /// `number` against `string` is `1 << 6` against `1 << 5`, so **`string`
    /// sorts first**. The baselines record `string[] | number[]`, 25 lines, and
    /// `Set<string> | Set<number>`, 20.
    ///
    /// Comparing the printed text instead answers `"number[]" < "string[]"` and
    /// reverses them. The two agree for references to *different* targets — the
    /// text begins with the target's name — so the disagreement is confined to
    /// **references sharing a target**, which is exactly where the argument
    /// lists are what upstream is comparing. `bd tsr-bgz`.
    ///
    /// # The pair upstream reads was already stored here
    ///
    /// The issue recorded this as needing `TypeData::Named` to carry a symbol
    /// and an argument list — a reshape of a type two workstreams share.
    /// [`Checker::type_reference_targets`] already holds exactly that pair for
    /// every reference, written by
    /// [`Checker::create_type_reference`](crate::Checker::create_type_reference)
    /// and kept for substitution, so this reads it rather than duplicating it.
    ///
    /// # Scope, stated rather than quietly widened
    ///
    /// **Only the reference arm.** A non-reference [`TypeData::Named`] — a class
    /// or interface instance type, an enum member, an object literal type —
    /// keeps the text comparison. Widening it would need upstream's other
    /// distinction too: `getTypeNameSymbol` returns *nil* for an anonymous
    /// object type, where this port's `Named` carries a symbol, so an object
    /// literal would start sorting before unnamed types instead of after. That
    /// is a separate change with its own population and none of the 137 measured
    /// lines need it.
    fn compare_type_names(
        &self,
        a: TypeId,
        b: TypeId,
        left: &crate::types::Type,
        right: &crate::types::Type,
    ) -> Ordering {
        if let (Some((target_a, args_a)), Some((target_b, args_b))) =
            (self.type_reference_targets.get(&a), self.type_reference_targets.get(&b))
        {
            if target_a == target_b {
                return self.compare_type_lists(args_a, args_b);
            }
            let symbols = self.binder.symbols();
            return symbols.get(*target_a).name.cmp(symbols.get(*target_b).name);
        }
        // A tuple has NO name upstream — it is a reference to a synthesised
        // target with no symbol — so its printed text must not enter the name
        // comparison. Left named, it sorted by ASCII (`[` before letters),
        // which put `[number, string]` ahead of `null[]` where the baseline
        // records the named type first (`bd tsr-5ll`, 34 lines, 8 cases).
        let left_name =
            if self.tuple_element_lists.contains_key(&a) { None } else { type_name(&left.data) };
        let right_name =
            if self.tuple_element_lists.contains_key(&b) { None } else { type_name(&right.data) };
        match (left_name, right_name) {
            (Some(x), Some(y)) => x.cmp(y),
            // A type with no name sorts after one with a name
            // (`utilities.go:614`).
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        }
    }

    /// `compareTypeLists` (`utilities.go:660`): shorter lists first, then
    /// elementwise by `CompareTypes`.
    fn compare_type_lists(&self, a: &[TypeId], b: &[TypeId]) -> Ordering {
        a.len().cmp(&b.len()).then_with(|| {
            a.iter()
                .zip(b)
                .map(|(&x, &y)| self.compare_types(x, y))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        })
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
            .then_with(|| self.compare_type_names(a, b, left, right))
            // `compareTupleTypes` (`utilities.go`), reached in upstream's
            // object-kind switch when both references target tuples: readonly
            // tuples after plain ones, then **ascending arity**
            // (`[string] | [number, boolean]` — the `bd tsr-5ll` issue text
            // had this backwards and is corrected there), then elementwise
            // `CompareTypes` over the element lists, which upstream reaches as
            // `compareTypeLists(resolvedTypeArguments)` two lines below the
            // tuple test. Element flags and labels are equal by construction
            // here while the modifier tuple forms refuse
            // (`get_type_from_tuple_type_node`).
            .then_with(|| {
                match (self.tuple_element_lists.get(&a), self.tuple_element_lists.get(&b)) {
                    (Some((elements_a, readonly_a)), Some((elements_b, readonly_b))) => readonly_a
                        .cmp(readonly_b)
                        .then(elements_a.len().cmp(&elements_b.len()))
                        .then_with(|| self.compare_type_lists(elements_a, elements_b)),
                    _ => Ordering::Equal,
                }
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

impl crate::checker::Checker<'_, '_> {
    /// A union reduced with `UnionReductionSubtype`, or `None` where the
    /// reduction is not decidable — `checker-notes-assign.md` §9.
    ///
    /// Upstream's `removeSubtypes` removes a constituent when
    /// `isTypeRelatedTo(source, target, strictSubtypeRelation)` holds. This
    /// port runs the same test through [`Ternary`] and **declines whole** on:
    ///
    /// - any pair reading [`Ternary::Unknown`] — the population whose wrong
    ///   removals priced `tsr-eak`'s 1.03 refusal;
    /// - a `Related` pair of two class instances — upstream additionally
    ///   requires `isTypeDerivedFrom` there (the `ObjectFlagsClass` caveat in
    ///   the `removeSubtypes` loop), which is unported;
    /// - a type-parameter constituent — upstream tests it against the union
    ///   of the *others* (the union-constraint branch), not pairwise.
    ///
    /// The key-property and `hasEmptyObject` branches upstream carries are
    /// performance, not semantics: primitives are strict subtypes only of
    /// empty object shapes, and the pairwise walk reaches those through the
    /// ordinary relation.
    pub(crate) fn union_with_subtype_reduction(
        &mut self,
        types: &[crate::types::TypeId],
    ) -> Option<crate::types::TypeId> {
        use crate::relater::Ternary;
        let literal = self.get_union_type(types);
        let constituents = match &self.store.get(literal).data {
            crate::types::TypeData::Union { types, .. } => types.clone(),
            // Zero or one constituent after literal reduction: nothing a
            // subtype pass could remove.
            _ => return Some(literal),
        };
        if constituents
            .iter()
            .any(|&c| self.store.get(c).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER))
        {
            return None;
        }
        // The first measurement fired the §9 bar's leg 2 at 46 and its named
        // falsifier was exact: `properties_related_to`'s own doc says
        // `readonly`, optionality and the other modifiers are "not compared
        // at all … the one place this function can be too permissive", and
        // this reduction is precisely the consumer that acts on the
        // too-permissive `Related` — `{ a } | { readonly a }` removed a
        // constituent upstream's directed relation keeps
        // (`readonlyPropertySubtypeRelationDirected`, 36 of the 46; the
        // relation learned READONLY in §14 and that clause is gone). Until
        // the relation reads the remaining modifiers, a constituent carrying one — or
        // carrying a generic instantiation (`NonNullable<T>` reduced to `T`)
        // — declines the whole reduction, syntactically.
        for &constituent in &constituents {
            // §16 deleted the modifier decline's last clause: readonly (§14),
            // optionality (§15) and privacy (§16) all live in the relation
            // now, and a protected-target pair reads `Unknown` there — the
            // gate below declines it as it declines every undecidable pair.
            if let Some((_, arguments)) = self.type_reference_targets.get(&constituent)
                && arguments.iter().any(|&a| {
                    self.store.get(a).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
                })
            {
                return None;
            }
        }
        // Iterate exactly as upstream does — from the end, re-testing against
        // the surviving list — so removal order cannot differ.
        let mut kept = constituents;
        let mut i = kept.len();
        while i > 0 {
            i -= 1;
            let source = kept[i];
            let mut remove = false;
            for &target in &kept {
                if target == source {
                    continue;
                }
                match self.relate_ternary(source, target, crate::relater::Relation::StrictSubtype) {
                    Ternary::Unknown => return None,
                    Ternary::Related => {
                        if self.is_class_instance(source) && self.is_class_instance(target) {
                            return None;
                        }
                        remove = true;
                        break;
                    }
                    Ternary::NotRelated => {}
                }
            }
            if remove {
                kept.remove(i);
            }
        }
        Some(self.get_union_type(&kept))
    }

    /// The §17 nominal verdict (`checker-notes-assign.md`): `Some(false)`
    /// when two class instances with DIFFERENT symbols each declare an OWN
    /// privacy marker (or one does and neither has heritage); `None` — no
    /// claim — everywhere else.
    pub(crate) fn nominal_class_pair_verdict(
        &self,
        source: crate::types::TypeId,
        target: crate::types::TypeId,
    ) -> Option<bool> {
        let source_symbol = self.class_instance_symbol(source)?;
        let target_symbol = self.class_instance_symbol(target)?;
        if self.binder.merged_symbol(source_symbol) == self.binder.merged_symbol(target_symbol) {
            return None;
        }
        let (source_private, source_heritage) = self.class_privacy_and_heritage(source_symbol)?;
        let (target_private, target_heritage) = self.class_privacy_and_heritage(target_symbol)?;
        if source_private && target_private {
            return Some(false);
        }
        if (source_heritage || target_heritage) && (source_private || target_private) {
            return None;
        }
        if source_private || target_private {
            return Some(false);
        }
        None
    }

    /// `(declares_own_privacy, has_heritage)` for a class symbol, from its
    /// declaration's syntax.
    fn class_privacy_and_heritage(&self, symbol: tsr_binder::SymbolId) -> Option<(bool, bool)> {
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        let Some(tsr_ast::Node::ClassDeclaration(class)) = self.node_map.get(declaration) else {
            return None;
        };
        let has_heritage = !class.heritage_clauses.is_empty();
        let has_privacy = class.members.iter().any(|member| {
            let node = tsr_ast::Node::from(*member);
            let Some(id) = node.node_id() else { return false };
            if let Some(tsr_ast::Node::PropertyDeclaration(property)) = self.node_map.get(id) {
                if matches!(property.name, tsr_ast::PropertyName::PrivateIdentifier(_)) {
                    return true;
                }
                return property.modifiers.iter().any(|modifier| {
                    tsr_ast::Node::from(*modifier).node_id().is_some_and(|m| {
                        matches!(
                            self.nodes.kind(m),
                            tsr_ast::SyntaxKind::PrivateKeyword
                                | tsr_ast::SyntaxKind::ProtectedKeyword
                        )
                    })
                });
            }
            false
        });
        Some((has_privacy, has_heritage))
    }

    /// `is_class_instance` with the symbol kept.
    fn class_instance_symbol(&self, id: crate::types::TypeId) -> Option<tsr_binder::SymbolId> {
        let symbol = match &self.store.get(id).data {
            crate::types::TypeData::Named { members: Some(symbol), .. }
            | crate::types::TypeData::Anonymous { symbol, .. } => Some(*symbol),
            _ => self.type_reference_targets.get(&id).map(|(symbol, _)| *symbol),
        }?;
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(tsr_binder::SymbolFlags::CLASS)
            .then_some(symbol)
    }

    /// Whether a type is a class **instance** type — the shape upstream's
    /// `removeSubtypes` guards with `ObjectFlagsClass`.
    fn is_class_instance(&self, id: crate::types::TypeId) -> bool {
        let symbol = match &self.store.get(id).data {
            crate::types::TypeData::Named { members: Some(symbol), .. }
            | crate::types::TypeData::Anonymous { symbol, .. } => Some(*symbol),
            _ => self.type_reference_targets.get(&id).map(|(symbol, _)| *symbol),
        };
        symbol.is_some_and(|s| {
            self.binder.symbols().get(s).flags.intersects(tsr_binder::SymbolFlags::CLASS)
        })
    }
}
