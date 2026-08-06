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
            // Every constituent was `never`, which is the only way to empty the
            // set now that nullable types are kept. Upstream's other two answers
            // here — `nullWideningType` and `undefinedWideningType`
            // (`checker.go:25692`) — are reachable only with `strictNullChecks`
            // off, so they are not ported rather than written and left dead.
            // They come back with `bd tsr-5s2`, and they are *not* `nullType`
            // and `undefinedType`: answering with those would merge identities
            // upstream keeps apart.
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
        // `checker.go:25783` drops `null` and `undefined` here when
        // `strictNullChecks` is off. It is assumed **on** — see the module docs
        // — so the drop does not happen and this is where it would go. The
        // constituents survive into the type; `format_union_types` is what moves
        // them to the end of the *printed* form.
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
        match (type_name(&left.data), type_name(&right.data)) {
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
