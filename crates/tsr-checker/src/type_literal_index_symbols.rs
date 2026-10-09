//! Computed member names of a TYPE LITERAL that do not late-bind:
//! `getResolvedMembersOrExportsOfSymbol`'s dynamic-name switch
//! (`checker.go`, late-bound members) and `getIndexInfosOfIndexSymbol`'s
//! late-bindable index arm (`checker.go:19662`).
//!
//! The binder gives a member with a dynamic name an ANONYMOUS symbol
//! (`bindPropertyOrMethodOrAccessor`, `binder.go:978`), so it is in no
//! members table. The checker then gives it one of three fates:
//!
//! 1. `isLateBindableName` (`checker.go:19961`): an entity-name expression
//!    whose type is usable as a property name. It becomes a real member.
//!    That is `objects.rs`'s `late_bound_symbol_member_name`, unchanged here.
//! 2. `hasLateBindableIndexSignature` (`checker.go:19971`): an entity-name
//!    expression (`isLateBindableAST`, `checker.go:19990`) whose type is
//!    assignable to `string | number | symbol`. It joins the `__index`
//!    symbol and contributes an index info keyed by `number` if the name is
//!    assignable to `number`, else `symbol`, else `string`
//!    (`checker.go:19676-19690`). `var v: { [e](): number }` with an
//!    unresolved `e` (type `any`) is `{ [x: number]: () => number; }`.
//! 3. Anything else, including every name that is not an entity-name
//!    expression (`["" + ""]`), is dropped: the member is in no table and
//!    the literal does not show it (`computedPropertyNamesDeclarationEmit4`
//!    records `{}`).
//!
//! `docs/parity/notes/r6-typesroots.md` §3.

use crate::checker::Checker;
use crate::flags::TypeFlags;
use crate::types::TypeId;

/// What a non-late-bound computed member of a type literal contributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(dead_code, reason = "consumer: r6-typesroots-late-bound-index.diff (declared.rs)")]
pub(crate) enum TypeLiteralComputedName {
    /// Not a late-bindable index signature: the member is dropped.
    Dropped,
    /// A late-bindable index signature of this key kind.
    Index(&'static str),
}

impl<'a> Checker<'a, '_> {
    /// Fate 2 or 3 of the module doc for a computed name that did not
    /// late-bind (`hasLateBindableIndexSignature`, then the key dispatch of
    /// `getIndexInfosOfIndexSymbol`).
    #[expect(dead_code, reason = "consumer: r6-typesroots-late-bound-index.diff (declared.rs)")]
    pub(crate) fn type_literal_computed_name(
        &mut self,
        computed: &tsr_ast::ComputedPropertyName<'a>,
    ) -> TypeLiteralComputedName {
        let Some(expression) = computed.expression else {
            return TypeLiteralComputedName::Dropped;
        };
        // isLateBindableAST: the expression is an entity name.
        let Some(id) = tsr_ast::Node::from(expression).node_id() else {
            return TypeLiteralComputedName::Dropped;
        };
        if !self.is_entity_name_expression(id) {
            return TypeLiteralComputedName::Dropped;
        }
        // checkComputedPropertyName is the expression's checked type.
        let key = self.check_expression(expression);
        let string_number_symbol = self.get_union_type(&[
            self.intrinsics.string,
            self.intrinsics.number,
            self.intrinsics.es_symbol,
        ]);
        if !self.is_type_assignable_to(key, string_number_symbol) {
            return TypeLiteralComputedName::Dropped;
        }
        if self.is_type_assignable_to(key, self.intrinsics.number) {
            TypeLiteralComputedName::Index("number")
        } else if self.is_type_assignable_to(key, self.intrinsics.es_symbol) {
            TypeLiteralComputedName::Index("symbol")
        } else {
            TypeLiteralComputedName::Index("string")
        }
    }

    /// `getTypeOfSymbol` of a method signature that joined the index
    /// symbol: its function type, plus `undefined` when the method is
    /// optional under `strictNullChecks` (`addOptionality`). `None` when this
    /// port cannot build the signature.
    #[expect(dead_code, reason = "consumer: r6-typesroots-late-bound-index.diff (declared.rs)")]
    pub(crate) fn type_literal_computed_method_type(
        &mut self,
        method: tsr_ast::NodeId,
        optional: bool,
    ) -> Option<TypeId> {
        let signature = self.get_signature_from_declaration(method)?;
        let symbol = self.binder.symbol_of(method)?;
        let printed = self.signature_to_string(&signature);
        let method_type = self.store.new_anonymous(TypeFlags::OBJECT, printed, symbol, true);
        self.signature_types.insert(method_type, vec![signature]);
        if optional && self.strict_null_checks {
            let undefined = self.intrinsics.undefined;
            return Some(self.get_union_type(&[method_type, undefined]));
        }
        Some(method_type)
    }
}
