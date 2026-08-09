//! TS2339 — `Property '{0}' does not exist on type '{1}'.`
//!
//! `reportNonexistentProperty` (`checker.go:11530`), error node the property
//! name.
//!
//! # This is the second attempt, and the first one is why it exists
//!
//! `docs/architecture/checker-notes-diag2.md` §9 built this rule against the
//! bound *"the receiver carries `members: Some(_)`"*, measured **2 conversions
//! against 254 wrong lines**, and refused it. The diagnosis it left behind is
//! the design of this file:
//!
//! > An absent property and an unbuilt members table are the same `None`, and no
//! > predicate over the *type* separates them.
//!
//! [`crate::member_completeness`] is the separation, and it works by asking the
//! **walk** rather than the type. Everything else here is the four declines
//! §9 correctly said were *not* what refused it — they were never the problem,
//! but each is a wrong code at a right position and the position is what the
//! suite compares.

use tsr_ast::{Node, NodeId};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `ast.SymbolFlagsBlockScoped` (`internal/ast/symbolflags.go:77`) — the names
/// that live in the global *scope* without being properties of the global
/// *object*.
const BLOCK_SCOPED: SymbolFlags =
    SymbolFlags::BLOCK_SCOPED_VARIABLE.union(SymbolFlags::CLASS).union(SymbolFlags::ENUM);

impl Checker<'_, '_> {
    /// The nonexistent-property check for one property access.
    ///
    /// **No `.js` decline**, and §37's audit is why. Every other rule this
    /// session declines JS because JSDoc supplies types this port does not
    /// parse — but that argument is about *annotations*, and this rule reports
    /// on a member's **absence** from a table the binder built from real
    /// declarations. JSDoc adds no members. Removing the decline here and in
    /// `crate::type_argument_arity`, which declined for the same borrowed
    /// reason, is worth 3 cases.
    pub(crate) fn check_nonexistent_property(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors {
            return;
        }
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else { return };
        let (Some(receiver), Some(member)) = (access.expression, access.name) else { return };
        // A private name has its own diagnostics (TS2339 is reported for one,
        // but so are TS18013 and TS18016 from `crate::members`' own note), and
        // the corpus's private-name cases are a row of their own.
        let tsr_ast::MemberName::Identifier(name) = member else { return };
        let Some(name_id) = name.node_id else { return };
        // An optional chain strips `null`/`undefined` before the lookup, and
        // this rule reads the receiver's type directly — so the two disagree
        // exactly where `?.` is written.
        if access.question_dot_token.is_some() {
            return;
        }

        let Some(receiver_id) = receiver.node_id() else { return };
        let receiver_type = self.check_expression(receiver);
        if self.global_this_member_is_not_reported(receiver_type, name.text) {
            return;
        }
        if !self.receiver_type_is_the_declared_one(receiver_id, receiver_type)
            || !self.declared_members_are_complete(receiver_type)
        {
            return;
        }
        if self.get_property_of_type(receiver_type, name.text).is_some() {
            return;
        }
        if self.is_a_universal_object_member(name.text)
            || self.other_side_of_class_has(receiver_type, name.text)
        {
            return;
        }
        // `reportNonexistentProperty`'s suggestion arm: a near-miss member name
        // is **TS2551**, not TS2339. §33 made the same move for TS2304/TS2552 —
        // the spelling algorithm is already exact, so reporting the code it
        // selects costs one message.
        let candidates = self.property_names_of(receiver_type);
        if let Some(suggestion) = crate::check::spelling_suggestion(
            name.text,
            &candidates.iter().map(String::as_str).collect::<Vec<_>>(),
        ) {
            let suggestion = suggestion.to_string();
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
            let span = self.error_span(name_id);
            let printed = self.type_to_string(receiver_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1_DID_YOU_MEAN_2,
                    span,
                    [name.text.to_string(), printed, suggestion],
                ),
            );
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        let printed = self.type_to_string(receiver_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [name.text.to_string(), printed],
            ),
        );
    }

    /// Whether `globalThis.<name>` is a missing property upstream stays silent
    /// about — which is nearly all of them.
    ///
    /// `globalThis` does not reach `reportNonexistentProperty` at all. It has
    /// its own arm several branches earlier
    /// (`internal/checker/checker.go:11337-11344`), and the arm's answer is
    /// `anyType`:
    ///
    /// ```go
    /// if leftType.symbol == c.globalThisSymbol {
    ///     globalSymbol := c.globalThisSymbol.Exports[right.Text()]
    ///     if globalSymbol != nil && globalSymbol.Flags&ast.SymbolFlagsBlockScoped != 0 {
    ///         c.error(right, diagnostics.Property_0_does_not_exist_on_type_1, …)
    ///     } else if c.noImplicitAny {
    ///         c.error(right, diagnostics.Element_implicitly_has_an_any_type_because_type_0_has_no_index_signature, …)
    ///     }
    ///     return c.anyType
    /// }
    /// ```
    ///
    /// So TS2339 is reported for exactly one shape: a name that **is** a global
    /// and is `let`/`const`/`class`/`enum`, because those live in the global
    /// *scope* without being properties of the global *object*. An undeclared
    /// name is TS7017 under `noImplicitAny` and silence otherwise — never
    /// TS2339.
    ///
    /// `SymbolFlagsBlockScoped` is `BlockScopedVariable | Class | Enum`
    /// (`internal/ast/symbolflags.go:77`).
    ///
    /// **Why this landed with the `declare global` merge rather than before
    /// it.** Until globals carried what a `declare global` block declares, this
    /// port's `typeof globalThis` had nothing in it that
    /// [`Checker::declared_members_are_complete`] would call complete, so the
    /// rule declined on that gate and the missing arm was invisible.
    /// `compiler/extendGlobalThis` — which augments `namespace globalThis` and
    /// then writes `globalThis.tests` where `test` was declared — is the case
    /// that turned it up, as a `diagnostics` regression of exactly one. Its
    /// `.types` baseline records `>globalThis.tests : any`, with no error, and
    /// the case runs `@strict: false` so the `noImplicitAny` arm is off too.
    ///
    /// **TS7017 is not ported here.** It is a different code with its own row,
    /// and reporting it from this file would put an implicit-any diagnostic in
    /// the nonexistent-property rule. What this function owes is the silence.
    fn global_this_member_is_not_reported(
        &mut self,
        receiver_type: crate::types::TypeId,
        name: &str,
    ) -> bool {
        if Some(receiver_type) != self.global_this_type {
            return false;
        }
        !self
            .binder
            .global(name)
            .is_some_and(|symbol| self.binder.symbols().get(symbol).flags.intersects(BLOCK_SCOPED))
    }

    /// Is the receiver's type the one its declaration says, rather than one
    /// **narrowing** or **inference** produced?
    ///
    /// The completeness predicate answers for a type. This answers for the
    /// *expression that produced it*, and it is the second half of the same
    /// question: a table can be complete and still be the wrong table, because
    /// this port narrowed where upstream did not or inferred a different return.
    ///
    /// Three shapes are declined, each with its case:
    ///
    /// - **A call receiver.** `c.foo().bar()` — `fluentClasses` — returns the
    ///   polymorphic `this` type, which this port does not model. It was this
    ///   rule's last remaining *loss*, and a loss is the one outcome the bar
    ///   forbids outright.
    /// - ~~**A dotted name.**~~ **DELETED, §37's audit, +2.** It was declined
    ///   because `narrowingOfDottedNames` narrows `a.b` by a guard on `a.b`
    ///   itself and this port's flow graph keys on a narrower set of references.
    ///   That is still true, but §35 made the *namespace* receiver decidable —
    ///   `N.x.y` is not narrowed by anything — and the decline was costing more
    ///   than the narrowing family costs. The narrowing cases are still wrong
    ///   lines; they are outnumbered.
    /// - **A narrowed identifier**, detected by comparing the flow type against
    ///   the symbol's declared type. `controlFlowInstanceof`,
    ///   `narrowByClauseExpressionInSwitchTrue7` and `typePredicateInLoop` are
    ///   the family, and it is exactly the family `checker-notes-narrow.md`
    ///   owns.
    ///
    /// A `this` receiver is **not** declined: `thisBinding` and `statics` are
    /// conversions and `this` in a class body is not narrowed by anything this
    /// port models.
    fn receiver_type_is_the_declared_one(&mut self, receiver: NodeId, flowed: TypeId) -> bool {
        match self.node_map.get(receiver) {
            Some(Node::CallExpression(_)) => false,
            Some(Node::Identifier(identifier)) => {
                let text = identifier.text;
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    receiver,
                    text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                let symbol = self.binder.merged_symbol(symbol);
                self.get_type_of_symbol(symbol) == flowed
            }
            _ => true,
        }
    }

    /// Is `name` a member every object type has anyway?
    ///
    /// **Not a decline — a correction.** `getApparentType` (`checker.go:19161`)
    /// hands an object type through unchanged, but `resolveObjectTypeMembers`'s
    /// `addInheritedMembers` layers the **global `Object`** interface's members
    /// underneath every one of them, and the global `Function`'s underneath any
    /// type carrying a call or construct signature. So `i.toString()` on an
    /// interface with no `toString` is legal, and `f.apply(…)` on an object type
    /// with a call signature is too.
    ///
    /// This port's member walk reads the declared table and its `extends`
    /// chain, and neither reaches those. Seven of this rule's first measurement
    /// **losses** were exactly this — `objectMembersOnTypes`,
    /// `classAppearsToHaveMembersOfObject`, `objectTypePropertyAccess`,
    /// `fluentClasses`, and the three `objectTypeWith*Signature*` cases — and
    /// they are losses rather than merely wrong lines because those cases pass
    /// today.
    ///
    /// Consulting the two globals by *name* is narrower than layering their
    /// members into the walk, and narrower in the safe direction: a name that is
    /// on `Object` or `Function` is never reported, whether or not the receiver
    /// would really have inherited it.
    fn is_a_universal_object_member(&mut self, name: &str) -> bool {
        for global in ["Object", "Function"] {
            let Some(symbol) = self.binder.globals().get(global).copied() else { continue };
            if self.binder.symbols().get(symbol).members.contains_key(name) {
                return true;
            }
        }
        false
    }

    /// Is `name` declared on the class's *other* side?
    ///
    /// `class C { x = 1 }` accessed as `C.x` is TS2576
    /// (`Property_0_is_a_static_member_of_type_1`), not TS2339. The check is on
    /// the symbol's two tables, which is where the binder already keeps the two
    /// sides apart.
    fn other_side_of_class_has(&mut self, receiver: TypeId, name: &str) -> bool {
        let Some(symbol) = self.owning_symbol_of(receiver) else { return false };
        let entry = self.binder.symbols().get(symbol);
        entry.members.contains_key(name) || entry.exports.contains_key(name)
    }

    /// Every property name the completeness walk can see on a type.
    ///
    /// Only ever called after [`Checker::declared_members_are_complete`] has
    /// answered `true`, so the list is the whole list.
    fn property_names_of(&mut self, receiver: TypeId) -> Vec<String> {
        let Some(symbol) = self.owning_symbol_of(receiver) else { return Vec::new() };
        let mut names = Vec::new();
        let mut visiting = Vec::new();
        self.collect_property_names(symbol, &mut names, &mut visiting);
        names
    }

    fn collect_property_names(
        &mut self,
        owner: SymbolIdAlias,
        names: &mut Vec<String>,
        visiting: &mut Vec<SymbolIdAlias>,
    ) {
        if visiting.contains(&owner) {
            return;
        }
        visiting.push(owner);
        for (member, id) in &self.binder.symbols().get(owner).members {
            if self.binder.symbols().get(*id).flags.intersects(SymbolFlags::VALUE) {
                names.push((*member).to_string());
            }
        }
        let Some(bases) = self.base_symbols_of(owner) else { return };
        for base in bases {
            self.collect_property_names(base, names, visiting);
        }
    }

    /// The symbol a `Named` type's members belong to.
    fn owning_symbol_of(&self, receiver: TypeId) -> Option<SymbolIdAlias> {
        match &self.store.get(receiver).data {
            crate::types::TypeData::Named { members, .. } => *members,
            _ => None,
        }
    }
}

use crate::types::TypeId;
type SymbolIdAlias = tsr_binder::SymbolId;
