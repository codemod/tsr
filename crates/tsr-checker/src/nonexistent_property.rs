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

impl Checker<'_, '_> {
    /// The nonexistent-property check for one property access.
    pub(crate) fn check_nonexistent_property(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
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
        if !self.receiver_type_is_the_declared_one(receiver_id, receiver_type)
            || !self.declared_members_are_complete(receiver_type)
        {
            return;
        }
        if self.get_property_of_type(receiver_type, name.text).is_some() {
            return;
        }
        if self.is_a_universal_object_member(name.text)
            || self.receiver_is_declared_in_a_lib(receiver_type)
            || self.nonexistent_property_has_another_code(receiver_type, name.text)
        {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.nodes.span(name_id);
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
    /// - **A dotted name.** `narrowingOfDottedNames` narrows `a.b` by a type
    ///   guard on `a.b` itself (`checker.go`'s `isMatchingReference` over
    ///   access chains); this port's flow graph keys on a narrower set of
    ///   references, so the two disagree about what `a.b` is at the access.
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
            Some(Node::CallExpression(_) | Node::PropertyAccessExpression(_)) => false,
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

    /// The arms of `reportNonexistentProperty` that reach a **different code**
    /// at the same position.
    ///
    /// §9 recorded that these "were not the problem" — they cost a handful of
    /// lines against 254 — and that is a statement about the previous bound, not
    /// a licence to skip them. Under the suite's exact-multiset rule a wrong
    /// code at a right position fails its case exactly as a missing diagnostic
    /// does.
    ///
    /// | arm | upstream's code |
    /// |---|---|
    /// | the name exists on the *other* side of the class — an instance member reached through `typeof C`, or a static through an instance | TS2576 / TS2339-with-suggestion |
    /// | a near-miss name exists on the type | TS2551 `Property_0_does_not_exist_on_type_1_Did_you_mean_2` |
    ///
    /// The `lib`-version arm (TS2550) and the DOM arm (TS2812) need a library
    /// version and a DOM table this port does not model; both are silences.
    fn nonexistent_property_has_another_code(&mut self, receiver: TypeId, name: &str) -> bool {
        if self.property_names_of(receiver).iter().any(|candidate| {
            crate::check::spelling_suggestion(name, &[candidate.as_str()]).is_some()
        }) {
            return true;
        }
        self.other_side_of_class_has(receiver, name)
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

    /// Was the receiver's type declared in a bundled `lib.*.d.ts`?
    ///
    /// `reportNonexistentProperty` substitutes **TS2550**
    /// (`Property_0_does_not_exist_on_type_1_Do_you_need_to_change_your_target_library`)
    /// when the name exists in a *newer* library than the one configured, and
    /// **TS2812** when it is a DOM name and `lib.dom` is absent. Modelling either
    /// needs a per-lib version table this port does not have.
    ///
    /// What it does have is the set of files it is *checking*
    /// ([`Checker::set_checked_files`]): the libs are in the program and are
    /// never walked, so a declaration outside that set is a library
    /// declaration. Every one of `doYouNeedToChangeYourTargetLibraryES2016Plus` (4 lines),
    /// `missingDomElements` (3) and
    /// `modularizeLibrary_ErrorFromUsingES6FeaturesWithOnlyES5Lib` (1) is a lib
    /// receiver, and all eight are one of those two codes upstream. **A silence
    /// here, and it comes back with a lib-version table.**
    fn receiver_is_declared_in_a_lib(&mut self, receiver: TypeId) -> bool {
        let Some(symbol) = self.owning_symbol_of(receiver) else { return false };
        let declarations = self.binder.symbols().get(symbol).declarations.to_vec();
        declarations.iter().any(|declaration| {
            self.source_file_of_for_diagnostics(*declaration)
                .is_none_or(|file| !self.checked_files.contains(&file))
        })
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
