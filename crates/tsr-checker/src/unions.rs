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
//! - **Subtype reduction** (`removeSubtypes`, `checker.go:25934`). Only
//!   `UnionReductionLiteral` is built, which is what `getUnionType` and
//!   `getTypeFromUnionTypeNode` both ask for anyway; `UnionReductionSubtype`
//!   arrives with `||` and `??`.
//!
//!   **The reason given here was "needs assignability, which does not exist".
//!   That was true when written and is not now** — §803 (2026-09-12) wrote
//!   `removeSubtypes` in twenty lines over
//!   `relate_ternary(.., Relation::StrictSubtype)`, the same predicate
//!   `single_common_supertype` (`inference.rs`) had already been running. The
//!   reduction is *buildable*; it is simply not built, and no measurement yet
//!   shows a caller that needs it — §803's own attempt measured **zero change**
//!   against the fallback it was written for, because those candidates are not
//!   subtype-reducible at all (STATUS §5, §802/§803).
//!
//!   Corrected rather than deleted, because "this is impossible" and "this is
//!   unbuilt and unmeasured" invite completely different next sessions, and the
//!   first one stood here for months after it stopped being true.
//! - **`origin` and `addNamedUnions`** (`checker.go:25705`), which keep
//!   `E | string` printing as `E | string` instead of expanding `E`'s members.
//!   A union with a *named* union among its constituents is a gap here
//!   (`bd tsr-ha6`).
//! - **`formatUnionTypes`' nullable reordering and enum collapsing**
//!   (`printer.go:383`). Both are unreachable given the two decisions above:
//!   nullable constituents are dropped before printing, and ~~an enum-like
//!   constituent only ever reaches a union through its own enum type, which is
//!   gapped (`bd tsr-8pz`)~~ — **that half is FALSE and §202 measured it so**:
//!   flow narrowing builds a union out of enum MEMBERS directly
//!   (`controlFlowBreakContinueWithLabel` joins `User.A | User.B` at a
//!   labelled break, where upstream prints `User`), so the enum collapsing is
//!   reachable and is now ported below. Porting them would read as coverage and provide
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
pub(crate) fn create_union(
    store: &mut TypeStore,
    extra_flags: TypeFlags,
    types: Vec<TypeId>,
    symbol: Option<(SymbolId, String)>,
) -> TypeId {
    create_union_with_text(store, extra_flags, types, symbol, None)
}

/// [`create_union`] with §53's origin text: the spelling renders from the
/// unexpanded origin entries while the constituent list stays flattened.
fn create_union_with_text(
    store: &mut TypeStore,
    extra_flags: TypeFlags,
    types: Vec<TypeId>,
    symbol: Option<(SymbolId, String)>,
    origin_text: Option<String>,
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
    let text = match (&symbol, origin_text) {
        // The node builder reaches a named union through its enum-like branch
        // (`nodebuilderimpl.go:3260`) or its alias branch (`:3362`), and both
        // print the symbol's name rather than the constituents. That is why an
        // enum's declared type — a union of its members — prints `E`.
        (Some((_, name)), _) => name.clone(),
        (None, Some(origin)) => origin,
        (None, None) => format_union_types(store, &types).join(" | "),
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
/// `Symbol.Name`.
///
/// **This comment used to say the port compares the printed form instead, and
/// that `C<string> | C<number>` therefore came out backwards (`bd tsr-bgz`,
/// "recorded rather than fixed, because fixing it means giving
/// `TypeData::Named` a symbol and an argument list"). §591 and §596 fixed it
/// without that reshape**, and the reshape was never needed: a REFERENCE
/// already carries its target symbol and arguments in
/// `type_reference_targets`, so two references to the same generic compare
/// their symbols (equal) and then their argument lists by `compare_type_lists`,
/// exactly as upstream does.
///
/// Re-measured at the §597 baseline: **zero** remaining non-right lines whose
/// union constituents are the same generic with different arguments. The
/// numbers that justified deferring it were describing a road this port no
/// longer takes.
/// The name a NON-reference type sorts under — its **symbol's** name where it
/// has one, and only otherwise its printed text.
///
/// §596. `getTypeNameSymbol` (`utilities.go:608`) answers a SYMBOL and
/// `compareSymbolsWorker` (`:385`) compares `s.Name`, which for `namespace Foo {
/// interface Yep {} }` is **`"Yep"`** — never the qualified `"Foo.Yep"` this
/// port prints. Two namespaced types with the same member name therefore
/// compare EQUAL upstream and fall through to the declaration-position rule
/// (§592), which is what orders them; sorting `"Bar.Yep"` before `"Foo.Yep"` by
/// ASCII put them in the opposite order (`namespaceDisambiguationInUnion`).
///
/// Only the qualified case can differ: for an unqualified name the printed text
/// and the symbol name are the same string, so this narrows the comparison
/// rather than redirecting it.
fn named_symbol_name<'a>(
    data: &'a TypeData,
    symbols: &'a tsr_binder::SymbolStore<'a>,
) -> Option<&'a str> {
    if let TypeData::Named { text, .. } = data {
        // A `Named` type that prints STRUCTURALLY has no name upstream would
        // sort it by: it is an inline object type, `getTypeNameSymbol` returns
        // its `__type`/`__object` symbol, and `compareSymbolsWorker` orders it
        // by DECLARATION POSITION (§592) — which is why
        // `{ kind: 'foo'; … } | { kind: 'bar'; … }` keeps its written order.
        // Answering `None` here is what routes it to that rule.
        if text.starts_with('{') {
            return None;
        }
        // Otherwise the text IS the name — an alias's or an interface's — and
        // upstream compares only the LAST segment, because
        // `getTypeNameSymbol` answers a symbol and `compareSymbolsWorker`
        // (`:385`) compares `s.Name`. `namespace Foo { interface Yep {} }` is
        // `"Yep"`, never the qualified `"Foo.Yep"` this port prints, so two
        // namespaced types with the same member name compare EQUAL and fall
        // through to declaration position (`namespaceDisambiguationInUnion`).
        let name = text.rsplit('.').next().unwrap_or(text);
        let _ = symbols;
        // The `members` symbol is deliberately NOT consulted: it is *"the
        // symbol whose members table this type's properties live in"*, which
        // for `type R = { a: number }` is the literal's synthetic `__type`,
        // not the alias `R` that upstream names it by
        // (`getTypeNameSymbol`'s first branch, `t.alias.symbol`). Reading it
        // collapsed `R`, `W` and `RW` to one name and printed them in source
        // order — `readonly (R | W | RW)[]` where `callWithSpread4` wants
        // `readonly (R | RW | W)[]`. **That cost a PASSING case**, and is why
        // this reads the printed text instead.
        //
        return Some(name);
    }
    type_name(data)
}

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
        // SS156 (checker-notes-callres2.md): `never` never joins a union —
        // upstream's addTypeToUnion skips it unconditionally, and the one
        // road here that could carry it (`&&`'s falsy-extraction union)
        // printed `E | never` where the corpus wants `E`.
        if types.contains(&self.intrinsics.never) {
            let never = self.intrinsics.never;
            let kept: Vec<TypeId> = types.iter().copied().filter(|&t| t != never).collect();
            return self.get_union_type(&kept);
        }
        // A semantic non-null intersection beside its base is subsumed by it.
        // This explicit relation covers flow joins while the general subtype
        // reducer still has capability boundaries (see checker-99-adjusted-facts).
        if !self.non_null_refinement_bases.is_empty()
            && types.iter().any(|member| {
                self.non_null_refinement_bases.get(member).is_some_and(|base| types.contains(base))
            })
        {
            let reduced: Vec<TypeId> = types
                .iter()
                .copied()
                .filter(|member| {
                    !self
                        .non_null_refinement_bases
                        .get(member)
                        .is_some_and(|base| types.contains(base))
                })
                .collect();
            return self.get_union_type(&reduced);
        }
        // `T | T` is `T` by identity for ANY `T` — including a named union,
        // which the worker below would otherwise expand and then decline for
        // want of upstream's `origin` denormalisation
        // (`checker-notes-arrays.md` §5, `enumLiteralsSubtypeReduction`).
        if types.iter().all(|&t| t == types[0]) {
            return types[0];
        }
        // SS202: a union whose constituents are exactly the members of ONE
        // enum IS that enum's declared type — upstream's `formatUnionTypes`
        // enum collapsing (`printer.go:383`). This module's header called
        // that branch UNREACHABLE, on the ground that "an enum-like
        // constituent only ever reaches a union through its own enum type";
        // **flow narrowing refutes it** — `controlFlowBreakContinueWithLabel`
        // builds `User.A | User.B` from two MEMBERS at a labelled-break join
        // and upstream prints `User`. The member-to-enum road
        // (`enum_member_owners`) already existed.
        if let Some(&owner) = self.enum_member_owners.get(&types[0])
            && types.iter().all(|t| self.enum_member_owners.get(t) == Some(&owner))
            && let Some(&declared) = self.declared_types.get(&owner)
            && let TypeData::Union { types: all, .. } = &self.store.get(declared).data
            && all.len() == types.len()
            && all.iter().all(|member| types.contains(member))
        {
            return declared;
        }
        self.union_type_worker(types, TypeFlags::empty(), None, false, true)
    }

    /// `getUnionTypeEx(..., UnionReductionNone)`: contextual tuple slices
    /// preserve all possible element types, including literals beside bases.
    pub(crate) fn get_union_type_without_reduction(&mut self, types: &[TypeId]) -> TypeId {
        if types.is_empty() {
            return self.intrinsics.never;
        }
        if types.contains(&self.intrinsics.error) {
            return self.intrinsics.error;
        }
        if types.len() == 1 || types.iter().all(|&id| id == types[0]) {
            return types[0];
        }
        self.union_type_worker(types, TypeFlags::empty(), None, false, false)
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
        self.union_type_worker(types, TypeFlags::empty(), None, true, true)
    }

    /// `getUnionTypeEx` with an alias (`checker.go:25628`), **minus the
    /// single-constituent collapse**.
    ///
    /// Single-value enums now return their literal directly in
    /// `get_declared_type_of_enum`. Other named union clients retain this helper's
    /// display wrapper until their alias representation is ported.
    pub(crate) fn get_named_union_type(
        &mut self,
        types: &[TypeId],
        extra_flags: TypeFlags,
        symbol: SymbolId,
    ) -> TypeId {
        if types.is_empty() {
            return self.intrinsics.never;
        }
        self.union_type_worker(types, extra_flags, Some(symbol), false, true)
    }

    /// `Checker.getUnionTypeWorker` (`checker.go:25653`) at
    /// `UnionReductionLiteral`.
    fn union_type_worker(
        &mut self,
        types: &[TypeId],
        extra_flags: TypeFlags,
        symbol: Option<SymbolId>,
        unprinted: bool,
        reduce_literals: bool,
    ) -> TypeId {
        let (mut set, includes) = self.add_types_to_union(types);

        if reduce_literals && !self.non_null_refinement_bases.is_empty() {
            let retained = set.clone();
            set.retain(|part| {
                !self
                    .non_null_refinement_bases
                    .get(part)
                    .is_some_and(|base| retained.contains(base))
            });
        }

        // §53 (`checker-notes-narrow.md`): upstream's denormalised `origin`
        // (`checker.go:25705`) — a union with a NAMED constituent keeps the
        // unexpanded entries for its SPELLING while the set stays the
        // flattened members. Entries: each deduped input contributes itself,
        // or its own origin entries if it carries them; sorted by the
        // comparator (`numberAssignableToEnumInsideUnion` wants
        // `boolean | E` for the written `E | boolean`).
        let origin_entries: Option<Vec<TypeId>> = if includes.named_union
            && !unprinted
            && symbol.is_none()
        {
            let mut entries: Vec<TypeId> = Vec::new();
            for &id in types {
                let contributed: Vec<TypeId> =
                    if let Some(own) = self.union_origin.get(&id) { own.clone() } else { vec![id] };
                for entry in contributed {
                    if !entries.contains(&entry) {
                        entries.push(entry);
                    }
                }
            }
            // Entry order, from the baselines: nullable entries LAST
            // (`MyEnum | undefined`), everything else by its first
            // MEMBER's sort bits (`boolean | E` for the written
            // `E | boolean`).
            let key = |checker: &Self, id: TypeId| -> (bool, u32) {
                let flags = checker.store.get(id).flags;
                if flags.intersects(TypeFlags::NULLABLE) {
                    return (true, 0);
                }
                let first = match &checker.store.get(id).data {
                    TypeData::Union { types, .. } => types.first().copied().unwrap_or(id),
                    _ => id,
                };
                (false, sort_order_flags(checker.store.get(first).flags))
            };
            entries.sort_by(|&a, &b| {
                let (ka, kb) = (key(self, a), key(self, b));
                ka.cmp(&kb).then_with(|| self.compare_types(a, b))
            });
            Some(entries)
        } else {
            // §742: a NAMED constituent inside an ALIASED union
            // (`symbol.is_some()`) reaches here and proceeds — the union
            // prints its own alias name, so upstream's `origin`
            // denormalisation (the reason the unaliased road above exists)
            // never comes into play. This arm used to answer `errorType` for
            // that shape (`type T = S[] | S` DECLARED error and silenced
            // every rule gating on the declared type, TS2454 first), which
            // was the §42.1/§76 printing guard applied one road too wide.
            None
        };

        if reduce_literals && includes.flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            if includes.flags.contains(TypeFlags::ANY) {
                // `checker.go:25659`: `IncludesError` wins over `IncludesAny`,
                // which is upstream's own "a gap in a constituent is a gap in
                // the union" and needs no deviation from this port.
                return if includes.error { self.intrinsics.error } else { self.intrinsics.any };
            }
            return self.intrinsics.unknown;
        }

        // getUnionTypeWorker (checker.go:25661): genuine undefined dominates missing.
        if reduce_literals && set.contains(&self.intrinsics.undefined) {
            set.retain(|&member| member != self.intrinsics.missing);
        }

        if reduce_literals
            && includes.flags.intersects(
                TypeFlags::ENUM
                    | TypeFlags::LITERAL
                    | TypeFlags::UNIQUE_ES_SYMBOL
                    | TypeFlags::TEMPLATE_LITERAL
                    | TypeFlags::STRING_MAPPING,
            )
        {
            set = self.remove_redundant_literal_types(set, includes.flags);
        }
        // `checker.go:25678`.
        if reduce_literals
            && includes.flags.contains(TypeFlags::STRING_LITERAL)
            && includes.flags.intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
        {
            set = self.remove_string_literals_matched_by_template_literals(set);
        }

        if reduce_literals && set.iter().any(|id| self.constrained_type_variables.contains_key(id))
        {
            set = self.remove_constrained_type_variables(set);
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

        if let Some(entries) = origin_entries {
            return self.build_origin_union(set, extra_flags, entries);
        }
        self.get_union_type_from_sorted_list(set, extra_flags, symbol)
    }

    /// removeConstrainedTypeVariables (internal/checker/checker.go:25881).
    /// Recombine intersections that collectively cover a variable's constraint.
    fn remove_constrained_type_variables(&mut self, mut types: Vec<TypeId>) -> Vec<TypeId> {
        let mut variables = Vec::new();
        for id in &types {
            if let Some(&(variable, _)) = self.constrained_type_variables.get(id)
                && !variables.contains(&variable)
            {
                variables.push(variable);
            }
        }
        for variable in variables {
            let primitives: Vec<_> = types
                .iter()
                .filter_map(|id| {
                    let &(owner, primitive) = self.constrained_type_variables.get(id)?;
                    (owner == variable).then_some(primitive)
                })
                .collect();
            let Some(constraint) = self.base_constraint_of_type(variable) else { continue };
            let covered = match &self.store.get(constraint).data {
                TypeData::Union { types, .. } => types.iter().all(|part| primitives.contains(part)),
                _ => primitives.contains(&constraint),
            };
            if covered {
                types.retain(|id| {
                    self.constrained_type_variables
                        .get(id)
                        .is_none_or(|&(owner, _)| owner != variable)
                });
                if !types.contains(&variable) {
                    types.push(variable);
                }
            }
        }
        types.sort_by(|&a, &b| self.compare_types(a, b));
        types
    }

    /// `Checker.getUnionTypeFromSortedList` (`checker.go:25736`).
    fn get_union_type_from_sorted_list(
        &mut self,
        types: Vec<TypeId>,
        extra_flags: TypeFlags,
        symbol: Option<SymbolId>,
    ) -> TypeId {
        // Native union interning reuses the enum's declared member set after
        // flattening; a falsy member plus the remaining members is that enum.
        if symbol.is_none()
            && let Some(&owner) = types.first().and_then(|first| self.enum_member_owners.get(first))
            && let Some(&declared) = self.declared_types.get(&owner)
            && let TypeData::Union { types: all, .. } = &self.store.get(declared).data
            && all == &types
        {
            return declared;
        }
        if types.len() == 1 && symbol.is_none() {
            return types[0];
        }
        let named = symbol.map(|id| (id, self.binder.symbols().get(id).name.to_string()));
        // formatUnionTypes (printer.go): a complete run of enum literals prints
        // as its enum, including fresh literals and enums inside larger unions.
        // Keep the original constituents so printing does not change freshness.
        let mut displayed = Vec::new();
        let mut index = 0;
        while index < types.len() {
            let current = types[index];
            let complete_enum = self
                .enum_member_owners
                .get(&current)
                .and_then(|owner| self.declared_types.get(owner))
                .copied()
                .and_then(|declared| {
                    let TypeData::Union { types: members, .. } = &self.store.get(declared).data
                    else {
                        return None;
                    };
                    let members = members.clone();
                    let end = index + members.len();
                    if end <= types.len()
                        && types[index..end].iter().zip(&members).all(|(&actual, &expected)| {
                            self.get_regular_type_of_literal_type(actual)
                                == self.get_regular_type_of_literal_type(expected)
                        })
                    {
                        Some((declared, end))
                    } else {
                        None
                    }
                });
            if let Some((declared, end)) = complete_enum {
                displayed.push(declared);
                index = end;
            } else {
                displayed.push(current);
                index += 1;
            }
        }
        let text =
            (displayed != types).then(|| format_union_types(&self.store, &displayed).join(" | "));
        let built =
            create_union_with_text(&mut self.store, extra_flags, types.clone(), named, text);
        // §58: named unions register their member set for the flow-join
        // identity consult.
        if symbol.is_some() {
            self.named_union_by_members.entry(types).or_insert(built);
        }
        built
    }

    /// Keep a union's denormalized origin spelling while retaining its resolved
    /// constituents. Used by `keyof` and distributive intersections.
    ///
    /// Upstream's `getLiteralTypeFromProperties` attaches
    /// `origin = newIndexType(t)` and distributive intersections attach a shorter
    /// intersection origin. The node builder prints a union's origin
    /// in preference to the union (`nodebuilderimpl.go:3439`, then the `Index`
    /// arm at `:3472`). Here the origin is the union's own interned text, which
    /// makes the origin-carrying union a *distinct* type from the bare one —
    /// deliberately, and the same shape §53 already uses.
    ///
    /// Declines a union that already prints as a symbol's name, and anything
    /// that is not a union at all (a one-key `keyof` is its single literal
    /// upstream too, because a one-element union collapses and takes no origin).
    pub(crate) fn union_with_origin_text(&mut self, union: TypeId, origin_text: String) -> TypeId {
        let TypeData::Union { types, symbol, .. } = &self.store.get(union).data else {
            return union;
        };
        if symbol.is_some() {
            return union;
        }
        let types = types.clone();
        let extra = self.store.get(union).flags & !(TypeFlags::UNION | TypeFlags::BOOLEAN);
        create_union_with_text(&mut self.store, extra, types, None, Some(origin_text))
    }

    fn build_origin_union(
        &mut self,
        set: Vec<TypeId>,
        extra_flags: TypeFlags,
        entries: Vec<TypeId>,
    ) -> TypeId {
        // §53's entry reduction: an entry whose member set is CONTAINED in
        // another entry's collapses into it (`x || y` where `x`'s written
        // expansion equals alias `y`'s set answers `T`); on EQUAL sets the
        // NAMED entry wins over an anonymous spelling.
        let member_sets: Vec<Vec<TypeId>> = entries
            .iter()
            .map(|&entry| match &self.store.get(entry).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![entry],
            })
            .collect();
        let is_named = |checker: &Self, id: TypeId| {
            matches!(&checker.store.get(id).data, TypeData::Union { symbol: Some(_), .. })
        };
        let mut keep = vec![true; entries.len()];
        for i in 0..entries.len() {
            if !keep[i] {
                continue;
            }
            for j in 0..entries.len() {
                if i == j || !keep[j] || !keep[i] {
                    continue;
                }
                let i_in_j = member_sets[i].iter().all(|m| member_sets[j].contains(m));
                let j_in_i = member_sets[j].iter().all(|m| member_sets[i].contains(m));
                if i_in_j && j_in_i {
                    // Equal sets: the named spelling wins; ties keep the first.
                    if is_named(self, entries[j]) && !is_named(self, entries[i]) {
                        keep[i] = false;
                    } else {
                        keep[j] = false;
                    }
                } else if i_in_j {
                    keep[i] = false;
                } else if j_in_i {
                    keep[j] = false;
                }
            }
        }
        let entries: Vec<TypeId> =
            entries.into_iter().zip(keep).filter_map(|(e, k)| k.then_some(e)).collect();
        // §53's reduction against a base primitive: an ENUM entry beside
        // the primitive its members widen to is REMOVED by upstream's
        // subtype reduction (`number | e` answers `number`,
        // `unionSubtypeIfEveryConstituentTypeIsSubtype`); origin must not
        // resurrect it.
        {
            let has_number =
                entries.iter().any(|&e| self.store.get(e).flags.contains(TypeFlags::NUMBER));
            let has_string =
                entries.iter().any(|&e| self.store.get(e).flags.contains(TypeFlags::STRING));
            if has_number || has_string {
                let dropped: Vec<TypeId> = entries
                    .iter()
                    .copied()
                    .filter(|&e| {
                        let flags = self.store.get(e).flags;
                        let enum_union = flags.contains(TypeFlags::UNION)
                            && flags.intersects(TypeFlags::ENUM_LITERAL | TypeFlags::ENUM);
                        if !enum_union {
                            return true;
                        }
                        // This port's enum-member types carry only ENUM
                        // (`declared.rs`'s member mint), so numeric-ness is
                        // read off the DECLARATION: a member with a string
                        // initializer makes the enum a string enum.
                        let symbol = match &self.store.get(e).data {
                            TypeData::Union { symbol: Some(symbol), .. } => *symbol,
                            _ => return true,
                        };
                        let mut any_string = false;
                        let mut saw_members = false;
                        let declarations = self.binder.symbols().get(symbol).declarations.clone();
                        for declaration in declarations {
                            if let Some(tsr_ast::Node::EnumDeclaration(declaration)) =
                                self.node_map.get(declaration)
                            {
                                for member in declaration.members {
                                    saw_members = true;
                                    if matches!(
                                        member.initializer,
                                        Some(tsr_ast::Expression::StringLiteral(_))
                                    ) {
                                        any_string = true;
                                    }
                                }
                            }
                        }
                        let all_numeric = saw_members && !any_string;
                        let all_string = saw_members && any_string;
                        !(all_numeric && has_number || all_string && has_string)
                    })
                    .collect();
                if dropped.len() != entries.len() {
                    // Members of the dropped enums leave the SET as well.
                    let mut reduced_set: Vec<TypeId> = Vec::new();
                    for entry in &dropped {
                        match &self.store.get(*entry).data {
                            TypeData::Union { types, .. } => {
                                reduced_set.extend(types.iter().copied());
                            }
                            _ => reduced_set.push(*entry),
                        }
                    }
                    reduced_set.sort_by(|&a, &b| self.compare_types(a, b));
                    reduced_set.dedup();
                    return self.build_origin_union(reduced_set, extra_flags, dropped);
                }
            }
        }
        // §53's slice gate (falsifier (a) recurred at scale without it —
        // `temporal`'s 82 site-sensitive alias spellings, the object-bearing
        // `string[] | Color` order class): an origin spelling is claimed
        // ONLY when every entry is an ENUM-named union or a non-object
        // plain type. Everything else keeps the pre-§53 GAP.
        let intersection_origin = entries
            .iter()
            .any(|&entry| matches!(self.store.get(entry).data, TypeData::Intersection { .. }));
        for &entry in &entries {
            let flags = self.store.get(entry).flags;
            let enum_union = flags.contains(TypeFlags::UNION)
                && flags.intersects(TypeFlags::ENUM_LITERAL | TypeFlags::ENUM)
                && matches!(&self.store.get(entry).data, TypeData::Union { symbol: Some(_), .. });
            // §97: an ALIAS-named union entry is admitted now that
            // `type_to_string_at`'s origin arm can re-render its spelling
            // per site — the site-sensitivity that made §53 exclude it.
            let alias_union = flags.contains(TypeFlags::UNION)
                && matches!(&self.store.get(entry).data,
                    TypeData::Union { symbol: Some(symbol), .. }
                        if self.binder.symbols().get(*symbol).flags
                            .contains(tsr_binder::SymbolFlags::TYPE_ALIAS));
            let plain = !flags.intersects(TypeFlags::OBJECT | TypeFlags::UNION);
            if !(enum_union || alias_union || plain || intersection_origin) {
                // §58.1 (`checker-notes-narrow.md`): before declining, a set
                // that IS a named union's member set answers the named type —
                // the join of `State`'s constituent with `State` itself
                // (`get_union_type([State, c1])`) re-forms State's set and
                // must not error (the anonymous-object union-print seam,
                // diagnosed by the TSR_JOIN_DEBUG instrument).
                // OBJECT-membered sets only: the seam's signature is the
                // retained object literal joining its alias's constituents;
                // literal-membered sets are the §52.1 site-sensitive class
                // and stay declined (+5/13 measured without this gate).
                if set.iter().any(|&member| {
                    self.store.get(member).flags.contains(TypeFlags::OBJECT)
                        && !self.store.get(member).flags.contains(TypeFlags::UNION)
                }) {
                    let mut sorted = set.clone();
                    sorted.sort_by(|&a, &b| self.compare_types(a, b));
                    sorted.dedup();
                    if let Some(&named) = self.named_union_by_members.get(&sorted) {
                        return named;
                    }
                }
                return self.intrinsics.error;
            }
        }
        let mut parts = Vec::with_capacity(entries.len());
        for &entry in &entries {
            let printed = crate::printing::type_to_string(self.store.get(entry));
            if printed == "error" {
                return self.intrinsics.error;
            }
            parts.push(parenthesised(&self.store, entry));
        }
        if set.is_empty() {
            return self.intrinsics.never;
        }
        if entries.len() == 1 {
            return entries[0];
        }
        let text = parts.join(" | ");
        let built = create_union_with_text(&mut self.store, extra_flags, set, None, Some(text));
        self.union_origin.entry(built).or_insert(entries);
        built
    }

    /// §53's projection: rebuild `original`'s union keeping only `kept`
    /// members. An origin entry survives WHOLE when all its members
    /// survive; a partially-surviving entry decomposes to its surviving
    /// members; no origin means a plain rebuild.
    pub(crate) fn rebuild_union_subset(&mut self, original: TypeId, kept: &[TypeId]) -> TypeId {
        let Some(entries) = self.union_origin.get(&original).cloned() else {
            return self.get_union_type(kept);
        };
        // filterType retains union origins, but discards an intersection origin
        // after its normalized union is filtered (checker.go:26568).
        if entries.len() == 1 && self.store.get(entries[0]).flags.contains(TypeFlags::INTERSECTION)
        {
            return self.get_union_type(kept);
        }
        let mut projected: Vec<TypeId> = Vec::new();
        for entry in entries {
            let members: Vec<TypeId> = match &self.store.get(entry).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![entry],
            };
            if members.iter().all(|member| kept.contains(member)) {
                projected.push(entry);
            } else {
                for member in members {
                    if kept.contains(&member) {
                        projected.push(member);
                    }
                }
            }
        }
        if projected.is_empty() {
            return self.intrinsics.never;
        }
        if let [single] = projected.as_slice() {
            return *single;
        }
        self.build_origin_union(kept.to_vec(), TypeFlags::empty(), projected)
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
                includes.named_union |= named || self.union_origin.contains_key(&id);
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
    /// Template-literal and string-mapping types are redundant beside `string`
    /// as string literals are. The `undefined`-beside-`void` clause is omitted:
    /// it is guarded by `reduceVoidUndefined`, which only
    /// `UnionReductionSubtype` sets.
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
            let remove = flags.intersects(
                TypeFlags::STRING_LITERAL | TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING,
            ) && includes.contains(TypeFlags::STRING)
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

    /// `removeStringLiteralsMatchedByTemplateLiterals` (`checker.go:25857`):
    /// a string literal matched by a pattern template or string mapping is
    /// redundant beside it. `isTypeMatchedByTemplateLiteralType` under
    /// `compareTypesAssignable` is the relater's template-target arm under the
    /// assignable relation.
    fn remove_string_literals_matched_by_template_literals(
        &mut self,
        mut types: Vec<TypeId>,
    ) -> Vec<TypeId> {
        let templates: Vec<_> =
            types.iter().copied().filter(|&ty| self.is_pattern_template(ty)).collect();
        if templates.is_empty() {
            return types;
        }
        let mut index = types.len();
        while index > 0 {
            index -= 1;
            let ty = types[index];
            if !self.store.get(ty).flags.contains(TypeFlags::STRING_LITERAL) {
                continue;
            }
            let matched = templates.iter().any(|&template| {
                if self.store.get(template).flags.contains(TypeFlags::TEMPLATE_LITERAL) {
                    self.is_type_assignable_to(ty, template)
                } else {
                    self.is_member_of_string_mapping(ty, template)
                }
            });
            if matched {
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
    /// `compareSymbols` (`utilities.go:366`) restricted to the OBJECT arm
    /// `CompareTypes` reaches it from (`:440`–`:444`).
    ///
    /// Upstream orders two object types by their symbols' **first declaration
    /// position**, falling back to the symbol name and then to symbol id. Only
    /// the position half is ported: it is the half that decides the
    /// anonymous-vs-anonymous population (§590's 103-line plurality), and a
    /// name tiebreak between two symbols whose declarations already compare
    /// equal cannot arise for the type-literal nodes that population is made
    /// of.
    ///
    /// A missing symbol sorts AFTER a present one, which is
    /// `compareSymbolsWorker`'s `s1 == nil => 1` (`:370`).
    fn compare_type_symbols(
        &self,
        left: &crate::types::Type,
        right: &crate::types::Type,
    ) -> Ordering {
        // Upstream reaches this only inside `t1.flags&TypeFlagsObject != 0`.
        if !left.flags.intersects(TypeFlags::OBJECT) || !right.flags.intersects(TypeFlags::OBJECT) {
            return Ordering::Equal;
        }
        let position = |data: &TypeData| -> Option<u32> {
            let symbol = match data {
                TypeData::Anonymous { symbol, .. } => Some(*symbol),
                TypeData::Named { members, .. } => *members,
                _ => None,
            }?;
            let declaration = *self.binder.symbols().get(symbol).declarations.first()?;
            Some(self.nodes.span(declaration).start)
        };
        match (position(&left.data), position(&right.data)) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }

    fn compare_type_names(
        &self,
        a: TypeId,
        b: TypeId,
        left: &crate::types::Type,
        right: &crate::types::Type,
    ) -> Ordering {
        let reference_a = self.type_reference_targets.get(&a);
        let reference_b = self.type_reference_targets.get(&b);
        if let (Some((target_a, args_a)), Some((target_b, args_b))) = (reference_a, reference_b)
            && target_a == target_b
        {
            return self.compare_type_lists(args_a, args_b);
        }
        // A tuple has NO name upstream — it is a reference to a synthesised
        // target with no symbol — so its printed text must not enter the name
        // comparison. Left named, it sorted by ASCII (`[` before letters),
        // which put `[number, string]` ahead of `null[]` where the baseline
        // records the named type first (`bd tsr-5ll`, 34 lines, 8 cases).
        // §591: a type REFERENCE names itself by its TARGET's symbol, whichever
        // the other side is. `getTypeNameSymbol` (`utilities.go:608`) returns
        // `t.symbol` for anything carrying `ObjectFlagsReference`, so
        // `JSX.Element[]` is `Array<JSX.Element>` and sorts under **"Array"** —
        // before `JSX.Element`'s "Element". This port compared the reference's
        // PRINTED TEXT (`"JSX.Element[]"`) whenever the other side was not also
        // a reference, which put `JSX.Element` first and inverted the union.
        //
        // The both-references branch above was already doing this correctly;
        // the bug was that it was the ONLY road to a target name. This is the
        // same defect the tuple exclusion below was written for — a printed
        // text standing in for a name upstream takes from a symbol.
        // A tuple is a reference to a SYNTHESISED target with no symbol, so
        // upstream's `getTypeNameSymbol` answers nil for it. Excluded on both
        // roads, not just the text one.
        // §597: an ENUM member has no name to compare. `getTypeNameSymbol`
        // (`utilities.go:608`) answers a symbol only for a type parameter, a
        // string mapping, or an object carrying `ClassOrInterface`/`Reference`
        // — an enum literal is none of those, so upstream reaches the type-id
        // tiebreak, which is enum DECLARATION order. `compare_types`'s own
        // comment already said enum members "fall through to the type-id
        // tiebreak below"; they did not, because this port prints them as
        // `Choice.Yes` and the text comparison answered first, sorting `No`
        // before `Yes` by ASCII (`enumLiteralTypes1`/`2`,
        // `stringEnumLiteralTypes1`/`2` all want `Choice.Yes | Choice.No`).
        if left.flags.intersects(TypeFlags::ENUM_LIKE)
            || right.flags.intersects(TypeFlags::ENUM_LIKE)
        {
            return Ordering::Equal;
        }
        let symbols = self.binder.symbols();
        let left_name = if self.tuple_element_lists.contains_key(&a) {
            None
        } else if let Some((target, _)) = reference_a {
            Some(symbols.get(*target).name)
        } else {
            named_symbol_name(&left.data, symbols)
        };
        let right_name = if self.tuple_element_lists.contains_key(&b) {
            None
        } else if let Some((target, _)) = reference_b {
            Some(symbols.get(*target).name)
        } else {
            named_symbol_name(&right.data, symbols)
        };
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
            // §592: *"Order unnamed or identically named object types by
            // symbol"* (`utilities.go:441`), which
            // `compareSymbolsWorker` (`:376`) answers by the first
            // DECLARATION's position — i.e. SOURCE order. This is the arm that
            // decides two anonymous object literals, and without it they fell
            // to the type-id tiebreak below, which is this port's creation
            // order and not upstream's.
            .then_with(|| self.compare_type_symbols(left, right))
            // compareTypeMappers orders instantiations of the same anonymous
            // member by their mapped types. Equal source lists identify the
            // flat mapper shape retained by instantiate_signature_type.
            .then_with(|| {
                let (
                    TypeData::Anonymous { symbol: left_symbol, .. },
                    TypeData::Anonymous { symbol: right_symbol, .. },
                ) = (&left.data, &right.data)
                else {
                    return Ordering::Equal;
                };
                if left_symbol != right_symbol {
                    return Ordering::Equal;
                }
                match (
                    self.instantiated_signature_mappers.get(&a),
                    self.instantiated_signature_mappers.get(&b),
                ) {
                    (Some(left), Some(right))
                        if left.iter().map(|pair| pair.0).eq(right.iter().map(|pair| pair.0)) =>
                    {
                        let left: Vec<_> = left.iter().map(|pair| pair.1).collect();
                        let right: Vec<_> = right.iter().map(|pair| pair.1).collect();
                        self.compare_type_lists(&left, &right)
                    }
                    _ => Ordering::Equal,
                }
            })
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
    /// `removeSubtypes` (`checker.go:25937`), using strict-subtype relations.
    /// Union-constrained parameters are compared with the union of the other
    /// surviving constituents; other sources are compared pairwise. Unknown
    /// relations decline the whole reduction. Class derivation and fresh-object
    /// excess checks preserve the existing relation restrictions.
    ///
    /// See `docs/architecture/checker-99-parameter-reduction.md` for native
    /// controls and the remaining representation limits.
    pub(crate) fn union_with_subtype_reduction(
        &mut self,
        types: &[crate::types::TypeId],
    ) -> Option<crate::types::TypeId> {
        use crate::relater::Ternary;
        // getUnionTypeWorker reduces the flattened set BEFORE constructing its
        // named-union origin (checker.go:25684, :25705). A printing decline on
        // an unreduced origin must not hide a decidable subtype elimination.
        let literal = self.get_union_type_unprinted(types);
        let constituents = match &self.store.get(literal).data {
            crate::types::TypeData::Union { types, .. } => types.clone(),
            // Zero or one constituent after literal reduction: nothing a
            // subtype pass could remove.
            _ => return Some(literal),
        };
        // §513: constituents with IDENTICAL PRINTED TEXT are one type to
        // every consumer of this port — print-at-creation is the data model
        // (ADR-0003) — where upstream reaches the same collapse through
        // interning. Two per-expression mints of `() => number` reduced this
        // way is what `contextualTyping32`'s `(() => number)[]` needs; the
        // pairwise walk below never decided an anonymous pair.
        let constituents: Vec<crate::types::TypeId> = {
            let mut seen: Vec<String> = Vec::with_capacity(constituents.len());
            let mut distinct = Vec::with_capacity(constituents.len());
            for &constituent in &constituents {
                // Unique symbols and parameters from different declarations
                // remain distinct even when their displayed names coincide.
                if self.store.get(constituent).flags.intersects(
                    crate::flags::TypeFlags::UNIQUE_ES_SYMBOL
                        | crate::flags::TypeFlags::TYPE_PARAMETER,
                ) {
                    distinct.push(constituent);
                    continue;
                }
                let text = self.type_to_string(constituent);
                if !seen.contains(&text) {
                    seen.push(text);
                    distinct.push(constituent);
                }
            }
            distinct
        };
        if let [single] = constituents.as_slice() {
            return Some(*single);
        }
        // §357: a PRIMITIVE constituent is never a removal candidate unless an
        // empty object type is present — upstream's gate at the top of the
        // removal loop (`hasEmptyObject || source.flags&StructuredOrInstantiable`,
        // checker.go:25955). Without it a non-strict `undefined` beside a class
        // instance either got removed (the relater says Related) or declined
        // the whole reduction; upstream keeps it (`generatorTypeCheck22`'s
        // `Bar | Baz | undefined`). Resolve properties, indexes and both
        // signature kinds for represented objects (isEmptyResolvedType,
        // checker.go:26481); a zero-property callable is not empty. This is
        // removal eligibility only, never an empty-object identity rewrite.
        let structured_or_instantiable = crate::flags::TypeFlags::OBJECT
            | crate::flags::TypeFlags::UNION
            | crate::flags::TypeFlags::INTERSECTION
            | crate::flags::TypeFlags::TYPE_PARAMETER
            | crate::flags::TypeFlags::INDEX
            | crate::flags::TypeFlags::INDEXED_ACCESS
            | crate::flags::TypeFlags::CONDITIONAL
            | crate::flags::TypeFlags::SUBSTITUTION
            | crate::flags::TypeFlags::TEMPLATE_LITERAL
            | crate::flags::TypeFlags::STRING_MAPPING;
        let has_empty_object = constituents.iter().any(|&constituent| {
            matches!(&self.store.get(constituent).data,
                crate::types::TypeData::Named { text, .. } if text == "{}")
                || self.store.get(constituent).flags.contains(TypeFlags::OBJECT)
                    && self
                        .get_property_names_of_type(constituent)
                        .is_some_and(|names| names.is_empty())
                    && self
                        .get_index_infos_of_type(constituent)
                        .is_some_and(|infos| infos.is_empty())
                    && [
                        crate::signatures::SignatureKind::Call,
                        crate::signatures::SignatureKind::Construct,
                    ]
                    .into_iter()
                    .all(|kind| {
                        self.signatures_of_type_kind(constituent, kind)
                            .is_some_and(|signatures| signatures.is_empty())
                    })
        });
        // Iterate exactly as upstream does — from the end, re-testing against
        // the surviving list — so removal order cannot differ.
        let mut kept = constituents;
        let mut i = kept.len();
        while i > 0 {
            i -= 1;
            let source = kept[i];
            if !has_empty_object
                && !self.store.get(source).flags.intersects(structured_or_instantiable)
            {
                continue;
            }
            // removeSubtypes (checker.go:25959): a union-constrained
            // parameter can inhabit the remaining union without inhabiting
            // any one of its constituents individually.
            if self.store.get(source).flags.contains(crate::flags::TypeFlags::TYPE_PARAMETER)
                && self.base_constraint_of_type(source).is_some_and(|constraint| {
                    self.store.get(constraint).flags.contains(crate::flags::TypeFlags::UNION)
                })
            {
                let others: Vec<_> = kept.iter().copied().filter(|&ty| ty != source).collect();
                let target = self.get_union_type(&others);
                match self.relate_ternary(source, target, crate::relater::Relation::StrictSubtype) {
                    Ternary::Related => {
                        kept.remove(i);
                    }
                    Ternary::NotRelated => {}
                    Ternary::Unknown => return None,
                }
                continue;
            }
            // removeSubtypes (checker.go:25970): different unit-valued keys
            // disqualify a target before the general relation is consulted.
            // Besides avoiding expensive comparisons, this keeps an unported
            // member relation from blocking reduction of a disjoint union.
            let keyed =
                TypeFlags::OBJECT | TypeFlags::INTERSECTION | TypeFlags::INSTANTIABLE_NON_PRIMITIVE;
            let key = if self.store.get(source).flags.intersects(keyed) {
                self.get_property_names_of_type(source).and_then(|names| {
                    names.into_iter().find_map(|name| {
                        // A method's value is always an object, never a unit.
                        // Resolving its signature just to discover that can
                        // re-enter the return inference building this union.
                        if self.get_property_of_type(source, &name).is_some_and(|symbol| {
                            self.binder
                                .symbols()
                                .get(symbol)
                                .flags
                                .contains(tsr_binder::SymbolFlags::METHOD)
                        }) {
                            return None;
                        }
                        let ty = self.get_type_of_property_of_type(source, &name)?;
                        self.store
                            .get(ty)
                            .flags
                            .intersects(TypeFlags::UNIT)
                            .then(|| (name, self.get_regular_type_of_literal_type(ty)))
                    })
                })
            } else {
                None
            };
            let mut remove = false;
            for &target in &kept {
                if target == source {
                    continue;
                }
                if let Some((name, key_type)) = &key
                    && self.store.get(target).flags.intersects(keyed)
                    && let Some(target_key) = self.get_type_of_property_of_type(target, name)
                    && self.store.get(target_key).flags.intersects(TypeFlags::UNIT)
                    && self.get_regular_type_of_literal_type(target_key) != *key_type
                {
                    continue;
                }
                // §357: upstream removes a class-instance source only when it
                // DERIVES from the class-instance target — `removeSubtypes`
                // (checker.go:26011) requires `isTypeDerivedFrom` when both
                // carry `ObjectFlagsClass`, and a related-but-underived pair
                // is KEPT, not undecidable. `[new Bar, new Baz]` over two
                // unrelated classes answers `Bar | Baz`
                // (`generatorTypeCheck22`). Tested BEFORE the relation, which
                // an underived pair never needs — that is also what keeps the
                // relater's absent-property conservatism (row 2's `Unknown`)
                // out of the class-pair path. The derivation test is §146.1's
                // declared-heritage walk; its unfollowable links answer "not
                // derived", which keeps the constituent — measured, not
                // assumed.
                if self.is_class_instance(source) && self.is_class_instance(target) {
                    let (Some(source_symbol), Some(target_symbol)) =
                        (self.class_instance_symbol(source), self.class_instance_symbol(target))
                    else {
                        return None;
                    };
                    let mut visiting = Vec::new();
                    if !self.heritage_chain_contains(source_symbol, target_symbol, &mut visiting) {
                        continue;
                    }
                }
                // §453: `hasExcessProperties` (relater.go:2667) runs BEFORE
                // the property walk in every relation, and a FRESH
                // object-literal source carrying a property the target lacks
                // fails it — so `{id:1, name:"foo"}` is never removed
                // against `{id:1}` and the array literal's element union
                // keeps both (`contextualTyping9/12`'s
                // `({ id: number; } | { id: number; name: string; })[]`).
                // The target-side index-signature admission of
                // `isKnownProperty` is not read here; a target with a string
                // index would wrongly keep its literal sources, and no
                // corpus case in the pool carries that shape.
                if self.fresh_object_literal_types.contains(&source)
                    && self.fresh_literal_has_excess_property(source, target)
                {
                    continue;
                }
                match self.relate_ternary(source, target, crate::relater::Relation::StrictSubtype) {
                    Ternary::Unknown => return None,
                    Ternary::Related => {
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
        Some(self.subtype_union_from_sorted_list(kept, types))
    }

    /// getUnionTypeWorker's post-reduction origin (checker.go:25705). A single
    /// alias covering every survivor is retained, even if its original member
    /// list has redundancy. Other origins require exact, non-overlapping counts.
    fn subtype_union_from_sorted_list(&mut self, kept: Vec<TypeId>, source: &[TypeId]) -> TypeId {
        let mut named = Vec::new();
        self.add_named_unions(&mut named, source);
        let mut entries: Vec<_> = kept
            .iter()
            .copied()
            .filter(|part| {
                !named.iter().any(|&id| {
                    matches!(&self.store.get(id).data, TypeData::Union { types, .. }
                        if types.contains(part))
                })
            })
            .collect();
        if named.len() == 1 && entries.is_empty() {
            return named[0];
        }
        let named_count: usize = named
            .iter()
            .map(|&id| match &self.store.get(id).data {
                TypeData::Union { types, .. } => types.len(),
                _ => 0,
            })
            .sum();
        if !named.is_empty() && named_count + entries.len() == kept.len() {
            entries.extend(named);
            entries.sort_by(|&a, &b| self.compare_types(a, b));
            let text = format_union_types(&self.store, &entries).join(" | ");
            let built =
                create_union_with_text(&mut self.store, TypeFlags::empty(), kept, None, Some(text));
            self.union_origin.entry(built).or_insert(entries);
            return built;
        }
        self.get_union_type_from_sorted_list(kept, TypeFlags::empty(), None)
    }

    /// addNamedUnions (checker.go:25824): aliases are atomic entries; a union
    /// origin is traversed, while a non-union origin keeps its enclosing union.
    fn add_named_unions(&self, named: &mut Vec<TypeId>, source: &[TypeId]) {
        for &id in source {
            let TypeData::Union { symbol, .. } = &self.store.get(id).data else { continue };
            let origin = self.union_origin.get(&id);
            if symbol.is_some_and(|symbol| {
                self.binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .contains(tsr_binder::SymbolFlags::TYPE_ALIAS)
            }) || origin.is_some_and(|entries| {
                entries.len() == 1 && !self.store.get(entries[0]).flags.contains(TypeFlags::UNION)
            }) {
                if !named.contains(&id) {
                    named.push(id);
                }
            } else if let Some(entries) = origin {
                self.add_named_unions(named, entries);
            }
        }
    }

    /// §453's excess test: any member NAME of the fresh literal `source` that
    /// `target` does not carry. Names come from the literal's own `__object`
    /// members table; a source minted without one has nothing to test.
    pub(crate) fn fresh_literal_has_excess_property(
        &mut self,
        source: crate::types::TypeId,
        target: crate::types::TypeId,
    ) -> bool {
        let crate::types::TypeData::Named { members: Some(symbol), .. } =
            &self.store.get(source).data
        else {
            return false;
        };
        let symbol = self.binder.merged_symbol(*symbol);
        let names: Vec<String> = self
            .binder
            .symbols()
            .get(symbol)
            .members
            .keys()
            .map(|name| (*name).to_string())
            .collect();
        if !self.is_excess_property_check_target(target) {
            return false;
        }
        names.iter().any(|name| !self.is_known_excess_property(target, name))
    }

    /// isExcessPropertyCheckTarget (relater.go:749).
    fn is_excess_property_check_target(&self, target: crate::types::TypeId) -> bool {
        use crate::{flags::TypeFlags, types::TypeData};
        let ty = self.store.get(target);
        match &ty.data {
            TypeData::Union { types, .. } => {
                types.iter().any(|&ty| self.is_excess_property_check_target(ty))
            }
            TypeData::Intersection { types, .. } => {
                types.iter().all(|&ty| self.is_excess_property_check_target(ty))
            }
            _ => ty.flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE),
        }
    }

    /// isKnownProperty (relater.go:719), including union constituents and
    /// applicable index signatures. Spread-only names are absent from the
    /// source literal's own binder members and are not excess properties.
    fn is_known_excess_property(&mut self, target: crate::types::TypeId, name: &str) -> bool {
        use crate::{flags::TypeFlags, types::TypeData};
        match self.store.get(target).data.clone() {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => {
                types.into_iter().any(|ty| self.is_known_excess_property(ty, name))
            }
            _ => {
                if self.get_type_of_property_of_type(target, name).is_some() {
                    return true;
                }
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    TypeData::StringLiteral(name.to_owned()),
                    false,
                );
                self.get_applicable_index_info(target, key).is_some()
            }
        }
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
    pub(crate) fn class_instance_symbol(
        &self,
        id: crate::types::TypeId,
    ) -> Option<tsr_binder::SymbolId> {
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

#[cfg(test)]
mod tests;
