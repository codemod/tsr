//! Production ownership and native private-record controls.
use std::borrow::Cow;
use std::collections::HashSet;
use tsr_ast::NodeId;
use tsr_binder::{SymbolFlags, SymbolStore};
use tsr_checker::symbol_access::{CheckFlags, CheckerSymbols, SymbolAccessError};
use tsr_checker::{Intrinsics, TypeStore};

fn intrinsic() -> (TypeStore, tsr_checker::TypeId) {
    let mut types = TypeStore::new();
    let id = Intrinsics::create(&mut types).unresolved;
    (types, id)
}

#[test]
fn bound_symbols_share_program_identity_and_reject_another_program() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Shared", SymbolFlags::INTERFACE);
    let (_types1, unresolved1) = intrinsic();
    let first = CheckerSymbols::new(&symbols, unresolved1);
    let (_types2, unresolved2) = intrinsic();
    let second = CheckerSymbols::new(&symbols, unresolved2);
    let handle = first.bound(id).unwrap();
    assert_eq!(second.view(&handle).unwrap().name(), "Shared");
    assert_eq!(handle, second.bound(id).unwrap());
    let mut other_symbols = SymbolStore::new();
    let other_id = other_symbols.create("Shared", SymbolFlags::INTERFACE);
    assert_eq!(id.index(), other_id.index());
    let (_types3, unresolved3) = intrinsic();
    let other = CheckerSymbols::new(&other_symbols, unresolved3);
    let foreign = other.bound(other_id).unwrap();
    assert_ne!(handle, foreign);
    assert!(matches!(first.view(&foreign), Err(SymbolAccessError::ForeignProgram)));
}

#[test]
fn private_unknown_is_distinct_from_uncomputed_and_foreign_unknown() {
    let symbols = SymbolStore::new();
    let (_types4, unresolved4) = intrinsic();
    let first = CheckerSymbols::new(&symbols, unresolved4);
    let (_types5, unresolved5) = intrinsic();
    let second = CheckerSymbols::new(&symbols, unresolved5);
    let unknown = first.unknown();
    assert_ne!(Some(unknown.clone()), None);
    let view = first.view(&unknown).unwrap();
    assert_eq!(view.flags(), SymbolFlags::PROPERTY | SymbolFlags::TRANSIENT);
    assert_eq!(view.name(), "unknown");
    assert!(view.check_flags().is_empty());
    assert!(!view.has_member_table() && !view.has_export_table());
    assert_ne!(unknown, second.unknown());
    assert!(matches!(second.view(&unknown), Err(SymbolAccessError::ForeignChecker)));
}

#[test]
fn stamp_hashing_distinguishes_program_and_checker_domains() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Shared", SymbolFlags::INTERFACE);
    let (_types6, unresolved6) = intrinsic();
    let first = CheckerSymbols::new(&symbols, unresolved6);
    let (_types7, unresolved7) = intrinsic();
    let second = CheckerSymbols::new(&symbols, unresolved7);
    let keys: HashSet<_> =
        [first.bound(id).unwrap(), first.unknown(), second.unknown()].into_iter().collect();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&second.bound(id).unwrap()));
    assert!(keys.contains(&first.unknown()));
}

#[test]
fn retained_private_handle_rejects_replacement_checkers_after_owner_drop() {
    let symbols = SymbolStore::new();
    let (_types8, unresolved8) = intrinsic();
    let first = CheckerSymbols::new(&symbols, unresolved8);
    let retained = first.unknown();
    drop(first);
    for _ in 0..256 {
        let (_types9, unresolved9) = intrinsic();
        let replacement = CheckerSymbols::new(&symbols, unresolved9);
        assert!(matches!(replacement.view(&retained), Err(SymbolAccessError::ForeignChecker)));
    }
}

#[test]
fn moving_a_checker_and_program_store_preserves_domains() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Shared", SymbolFlags::INTERFACE);
    let (_types10, unresolved10) = intrinsic();
    let original = CheckerSymbols::new(&symbols, unresolved10);
    let bound = original.bound(id).unwrap();
    let unknown = original.unknown();
    let moved = Box::new(original);
    assert_eq!(moved.view(&unknown).unwrap().name(), "unknown");
    drop(moved);
    let moved_program = Box::new(symbols);
    let (_types11, unresolved11) = intrinsic();
    let checker = CheckerSymbols::new(&moved_program, unresolved11);
    assert_eq!(checker.view(&bound).unwrap().name(), "Shared");
}

#[test]
fn unresolved_symbols_share_full_paths_and_private_declared_type_only() {
    let symbols = SymbolStore::new();
    let (_types, unresolved_type) = intrinsic();
    let mut first = CheckerSymbols::new(&symbols, unresolved_type);
    let (_types12, unresolved12) = intrinsic();
    let mut second = CheckerSymbols::new(&symbols, unresolved12);
    let a = first.unresolved_path("Missing.Child");
    assert_eq!(a, first.unresolved_path("Missing.Child"));
    let other = first.unresolved_path("Other.Child");
    let foreign = second.unresolved_path("Missing.Child");
    assert_ne!(a, other);
    assert_ne!(a, foreign);
    assert!(matches!(first.view(&foreign), Err(SymbolAccessError::ForeignChecker)));
    let view = first.view(&a).unwrap();
    assert_eq!(view.name(), "Child");
    assert_eq!(view.flags(), SymbolFlags::TYPE_ALIAS | SymbolFlags::TRANSIENT);
    assert_eq!(view.check_flags(), CheckFlags::UNRESOLVED);
    assert_eq!(view.declared_type(), Some(unresolved_type));
    let parent = view.parent().unwrap();
    assert_eq!(parent, first.unresolved_path("Missing"));
    assert_eq!(first.symbol_path(&a).unwrap(), "Missing.Child");
    assert_eq!(first.unresolved_path(""), first.unknown());
    assert_eq!(first.unresolved_path("Missing."), first.unknown());
    // checker.go:23139 keeps a separator for an existing empty-named parent.
    // Treating that parent as absent would collide with an unqualified child.
    let empty_parent =
        first.new_symbol(SymbolFlags::VALUE_MODULE, Cow::Borrowed(""), CheckFlags::empty());
    let qualified = first.unresolved_symbol("Child", Some(&empty_parent)).unwrap();
    assert_eq!(first.symbol_path(&qualified).unwrap(), ".Child");
    assert_ne!(qualified, first.unresolved_path("Child"));
}

#[test]
fn private_clones_keep_bound_and_private_origin() {
    let mut symbols = SymbolStore::new();
    let id = symbols.create("Original", SymbolFlags::INTERFACE);
    let (_types13, unresolved13) = intrinsic();
    let mut first = CheckerSymbols::new(&symbols, unresolved13);
    let (_types14, unresolved14) = intrinsic();
    let mut second = CheckerSymbols::new(&symbols, unresolved14);
    let origin = first.bound(id).unwrap();
    let clone = first.clone_symbol(&origin).unwrap();
    let other_clone = second.clone_symbol(&origin).unwrap();
    assert_ne!(clone, origin);
    assert_ne!(clone, other_clone);
    assert!(matches!(second.view(&clone), Err(SymbolAccessError::ForeignChecker)));
    assert_eq!(first.view(&clone).unwrap().origin(), Some(origin.clone()));
    assert_eq!(first.merged_symbol(&origin).unwrap(), clone);
    assert_eq!(second.merged_symbol(&origin).unwrap(), other_clone);
    let again = first.clone_symbol(&clone).unwrap();
    assert_eq!(first.view(&again).unwrap().origin(), Some(clone.clone()));
    assert_eq!(first.merged_symbol(&clone).unwrap(), again);
}

#[test]
fn bound_clone_preserves_native_fields_and_program_immutability() {
    let mut symbols = SymbolStore::new();
    let parent = symbols.create("Parent", SymbolFlags::VALUE_MODULE);
    let id = symbols.create("Target", SymbolFlags::INTERFACE);
    let member = symbols.create("p", SymbolFlags::PROPERTY);
    let export = symbols.create("e", SymbolFlags::FUNCTION);
    let marker = symbols.create("marker", SymbolFlags::EXPORT_VALUE);
    let declaration = NodeId::new(1);
    let s = symbols.get_mut(id);
    s.declarations.push(declaration);
    s.value_declaration = Some(declaration);
    s.parent = Some(parent);
    s.export_symbol = Some(marker);
    s.members.insert("p", member);
    s.exports.insert("e", export);
    let (_types15, unresolved15) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved15);
    let bound = checker.bound(id).unwrap();
    let clone = checker.clone_symbol(&bound).unwrap();
    let view = checker.view(&clone).unwrap();
    assert_eq!(view.flags(), SymbolFlags::INTERFACE | SymbolFlags::TRANSIENT);
    assert_eq!(view.parent(), Some(checker.bound(parent).unwrap()));
    assert_eq!(view.value_declaration(), Some(declaration));
    assert_eq!(view.member("p"), Some(checker.bound(member).unwrap()));
    assert_eq!(view.export("e"), Some(checker.bound(export).unwrap()));
    assert_eq!(view.export_symbol(), None);
    assert!(view.check_flags().is_empty());
    checker.append_declaration(&clone, NodeId::new(2)).unwrap();
    checker.set_member(&clone, Cow::Borrowed("p"), &checker.bound(export).unwrap()).unwrap();
    checker.set_export(&clone, Cow::Borrowed("e"), &checker.bound(member).unwrap()).unwrap();
    assert_eq!(symbols.get(id).declarations.as_slice(), &[declaration]);
    assert_eq!(symbols.get(id).members["p"], member);
    assert_eq!(symbols.get(id).exports["e"], export);
    assert_eq!(
        checker.append_declaration(&bound, NodeId::new(3)),
        Err(SymbolAccessError::ImmutableProgram)
    );
}

#[test]
fn private_clone_clears_check_flags_export_link_and_declared_type() {
    let symbols = SymbolStore::new();
    let (_types16, unresolved16) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved16);
    let original = checker.unresolved_path("Missing.Child");
    let unknown = checker.unknown();
    checker.set_check_flags(&original, CheckFlags::UNRESOLVED | CheckFlags::READONLY).unwrap();
    checker.set_export_symbol(&original, Some(&unknown)).unwrap();
    let clone = checker.clone_symbol(&original).unwrap();
    let view = checker.view(&clone).unwrap();
    assert!(view.flags().contains(SymbolFlags::TRANSIENT));
    assert!(view.check_flags().is_empty());
    assert_eq!(view.export_symbol(), None);
    assert_eq!(view.declared_type(), None);
    assert!(!view.has_member_table() && !view.has_export_table());
    assert_eq!(view.parent(), checker.view(&original).unwrap().parent());
}

#[test]
fn private_clone_tables_are_independent_and_edges_remain_shared() {
    let symbols = SymbolStore::new();
    let (_types17, unresolved17) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved17);
    let source =
        checker.new_symbol(SymbolFlags::INTERFACE, Cow::Borrowed("Source"), CheckFlags::empty());
    let first =
        checker.new_symbol(SymbolFlags::PROPERTY, Cow::Borrowed("first"), CheckFlags::empty());
    let second =
        checker.new_symbol(SymbolFlags::PROPERTY, Cow::Borrowed("second"), CheckFlags::empty());
    checker.set_member(&source, Cow::Borrowed("p"), &first).unwrap();
    checker.set_export(&source, Cow::Borrowed("e"), &first).unwrap();
    let clone = checker.clone_symbol(&source).unwrap();
    assert_eq!(checker.view(&clone).unwrap().member("p"), Some(first.clone()));
    checker.set_member(&clone, Cow::Borrowed("p"), &second).unwrap();
    checker.set_export(&clone, Cow::Borrowed("e"), &second).unwrap();
    assert_eq!(checker.view(&source).unwrap().member("p"), Some(first.clone()));
    assert_eq!(checker.view(&source).unwrap().export("e"), Some(first));
}

#[test]
fn raw_parent_and_merged_parent_channels_stay_distinct() {
    let symbols = SymbolStore::new();
    let (_types18, unresolved18) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved18);
    let source =
        checker.new_symbol(SymbolFlags::VALUE_MODULE, Cow::Borrowed("source"), CheckFlags::empty());
    let target =
        checker.new_symbol(SymbolFlags::VALUE_MODULE, Cow::Borrowed("target"), CheckFlags::empty());
    let export =
        checker.new_symbol(SymbolFlags::PROPERTY, Cow::Borrowed("export"), CheckFlags::empty());
    checker.set_parent(&export, Some(&source)).unwrap();
    checker.record_merged(&target, &source).unwrap();
    let raw = checker.view(&export).unwrap().parent().unwrap();
    assert_eq!(raw, source);
    assert_eq!(checker.merged_symbol(&raw).unwrap(), target);
    assert_eq!(checker.view(&export).unwrap().parent(), Some(source));
}

#[test]
fn foreign_edges_are_rejected_before_mutating_a_private_record() {
    let symbols = SymbolStore::new();
    let (_types19, unresolved19) = intrinsic();
    let mut first = CheckerSymbols::new(&symbols, unresolved19);
    let (_types20, unresolved20) = intrinsic();
    let second = CheckerSymbols::new(&symbols, unresolved20);
    let target = first.unknown();
    let foreign = second.unknown();
    assert_eq!(first.set_parent(&target, Some(&foreign)), Err(SymbolAccessError::ForeignChecker));
    assert_eq!(
        first.set_member(&target, Cow::Borrowed("bad"), &foreign),
        Err(SymbolAccessError::ForeignChecker)
    );
    assert_eq!(first.record_merged(&target, &foreign), Err(SymbolAccessError::ForeignChecker));
    let view = first.view(&target).unwrap();
    assert_eq!(view.parent(), None);
    assert!(!view.has_member_table());
}

#[test]
fn same_symbol_redirect_is_valid_and_not_a_recursive_chain_walk() {
    let symbols = SymbolStore::new();
    let (_types21, unresolved21) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved21);
    let unknown = checker.unknown();
    checker.record_merged(&unknown, &unknown).unwrap();
    assert_eq!(checker.merged_symbol(&unknown).unwrap(), unknown);
}

#[test]
fn storage_accounting_distinguishes_inline_payload_and_live_handle_references() {
    let symbols = SymbolStore::new();
    let (_types, unresolved) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved);
    let initial = checker.storage_usage();
    assert_eq!(initial.records, 1);
    assert_eq!(initial.unresolved_entries, 0);
    assert_eq!(initial.edge_capacity, 0);
    let child = checker.unresolved_path("Missing.Child");
    let usage = checker.storage_usage();
    assert_eq!(usage.records, 3);
    assert_eq!(usage.unresolved_entries, 2);
    assert!(usage.inline_record_bytes > initial.inline_record_bytes);
    assert!(usage.owned_name_bytes >= "Missing".len() + "Child".len());
    assert!(usage.path_key_bytes >= "Missing".len() + "Missing.Child".len());
    let retained = child.clone();
    assert_eq!(checker.storage_usage().stamp_references, usage.stamp_references + 1);
    drop(retained);
    assert_eq!(checker.storage_usage(), usage);
}

#[test]
fn leaf_text_and_ast_parent_are_preserved_even_when_full_path_keys_collide() {
    let symbols = SymbolStore::new();
    let (_types, unresolved) = intrinsic();
    let mut checker = CheckerSymbols::new(&symbols, unresolved);
    let dotted_leaf = checker.unresolved_symbol("Root.Child", None).unwrap();
    assert_eq!(checker.view(&dotted_leaf).unwrap().name(), "Root.Child");
    assert_eq!(checker.view(&dotted_leaf).unwrap().parent(), None);
    assert_eq!(checker.unresolved_path("Root.Child"), dotted_leaf);
    // Qualified lookup still resolves its parent before the full-path hit.
    let root = checker.unresolved_symbol("Root", None).unwrap();
    assert_eq!(checker.storage_usage().unresolved_entries, 2);
    let initial = checker.storage_usage();
    assert_eq!(checker.unresolved_path("NeverCreated."), checker.unknown());
    assert_eq!(checker.storage_usage(), initial);
    assert_eq!(checker.symbol_path(&root).unwrap(), "Root");
}

#[test]
fn bound_clones_distinguish_absent_from_initialized_empty_tables() {
    let mut program = SymbolStore::new();
    let absent = program.create("Absent", SymbolFlags::INTERFACE);
    let initialized = program.create("Initialized", SymbolFlags::INTERFACE);
    program.get_mut(initialized).members.initialize();
    program.get_mut(initialized).exports.initialize();
    let (_types, unresolved) = intrinsic();
    let mut symbols = CheckerSymbols::new(&program, unresolved);
    let absent = symbols.bound(absent).unwrap();
    let initialized = symbols.bound(initialized).unwrap();
    let absent_clone = symbols.clone_symbol(&absent).unwrap();
    let initialized_clone = symbols.clone_symbol(&initialized).unwrap();
    for id in [&absent, &absent_clone] {
        let view = symbols.view(id).unwrap();
        assert!(!view.has_member_table() && !view.has_export_table());
        assert!(view.member("missing").is_none() && view.export("missing").is_none());
    }
    for id in [&initialized, &initialized_clone] {
        let view = symbols.view(id).unwrap();
        assert!(view.has_member_table() && view.has_export_table());
        assert!(view.member("missing").is_none() && view.export("missing").is_none());
    }
    symbols.set_member(&absent_clone, Cow::Borrowed("private"), &initialized).unwrap();
    assert!(symbols.view(&absent_clone).unwrap().has_member_table());
    assert!(!symbols.view(&absent).unwrap().has_member_table());
    assert!(symbols.view(&initialized).unwrap().member("private").is_none());
}
