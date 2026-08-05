//! `export { q }` — the half of the alias row that never needed a module graph.
//!
//! An export specifier is bound as an alias symbol, and an alias symbol has no
//! value declaration, so every one of these lines lands in `rank_board`'s row
//! `SymbolFlags(ALIAS) / no value declaration` — a row whose name reads as
//! module work. `getTargetOfExportSpecifier` (`checker.go:14951`) branches on
//! **the export declaration's module specifier**, not on the specifier, so
//! `export { q }` is a plain `resolveEntityName` in the ordinary scope while
//! `export { q } from "./m"` is `getExternalModuleMember`.
//!
//! Measured on the `conformance/` half of the corpus before this landed: 154 of
//! the row's 794 lines there, and 86% of its same-file half
//! (`docs/architecture/checker-notes-symbols.md`).
//!
//! # These fixtures discriminate, and here is the evidence
//!
//! `docs/architecture/checker-notes-arrays.md` lists six fixtures in one
//! workstream that ran the right code and could not tell a right implementation
//! from a wrong one. Each test below names the mutation it is red under, and no
//! mutation reddens more than the test that names it:
//!
//! | test | mutation |
//! |---|---|
//! | [`a_same_file_export_specifier_takes_the_locals_type`] | delete the `ExportSpecifier` dispatch from `resolve_alias` |
//! | [`an_export_specifier_that_names_a_module_stays_a_gap`] | delete the `module_specifier.is_some()` guard |
//! | [`export_q_as_r_looks_up_the_property_name`] | use `specifier.name` instead of `property_name.or(name)` |
//! | [`a_type_only_export_specifier_stays_a_gap`] | **none — see the test; the mutation was tried and does not bite** |
//! | [`an_export_specifier_naming_an_alias_follows_the_chain`] | take the `VALUE` test over raw flags instead of `get_symbol_flags` |
//!
//! **No fixture here names `Array`, `Promise` or `number[]`.** The unit harness
//! builds one file with no lib, so any such name would measure the missing
//! library rather than this arm.

use tsr_binder::SymbolFlags;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the export-specifier alias whose exported name is `name`.
///
/// This is the position the `.types` baseline records: the specifier's own name
/// is a **declaration name**, and `types_producer` answers a declaration name
/// through its parent's symbol and `getTypeOfSymbol`
/// (`crates/tsr-conformance/src/types_producer.rs:1097`). Reaching the symbol
/// the same way is what makes the assertions here the assertions the corpus
/// scores.
fn type_of_export(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut found = None;
    for index in 0..u32::try_from(parsed.nodes.len()).expect("node count fits in u32") {
        let id = tsr_ast::NodeId::new(index);
        if parsed.nodes.kind(id) != tsr_ast::SyntaxKind::ExportSpecifier {
            continue;
        }
        let Some(symbol) = bound.symbol_of(id) else { continue };
        let entry = bound.symbols().get(symbol);
        if entry.name != name {
            continue;
        }
        assert!(
            entry.flags.intersects(SymbolFlags::ALIAS),
            "an export specifier binds an alias symbol"
        );
        assert!(
            entry.value_declaration.is_none(),
            "an alias has no value declaration — that is why the row is named as it is"
        );
        found = Some(symbol);
    }
    let symbol = found.unwrap_or_else(|| panic!("no export specifier named {name}"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// Every value form in `conformance/exportsAndImports1.types`, which is the
/// case that contributes the most lines to this row.
///
/// The baseline is the specification and it is quoted rather than paraphrased:
///
/// ```text
/// export { v, f, C, I, E, D, M, N, T, a };
/// >v : number
/// >f : () => void
/// >C : typeof C
/// >E : typeof E
/// >M : typeof M
/// ```
///
/// **Red under:** deleting the `SyntaxKind::ExportSpecifier` dispatch from
/// `Checker::resolve_alias`, which is the arm itself. Every assertion here
/// becomes `error`.
#[test]
fn a_same_file_export_specifier_takes_the_locals_type() {
    let source = "var v = 1;\n\
                  function f() { }\n\
                  class C { }\n\
                  enum E { A }\n\
                  namespace M { export var x = 1; }\n\
                  export { v, f, C, E, M };";
    assert_eq!(type_of_export(source, "v"), "number");
    assert_eq!(type_of_export(source, "f"), "() => void");
    assert_eq!(type_of_export(source, "C"), "typeof C");
    assert_eq!(type_of_export(source, "E"), "typeof E");
    assert_eq!(type_of_export(source, "M"), "typeof M");
}

/// `export { q } from "./m"` is a different function upstream —
/// `getExternalModuleMember` — and it needs a module graph this checker does
/// not have (`Checker::new` takes `(binder, nodes, node_map)` and `BindResult`
/// exposes no specifier-to-file map).
///
/// The danger is precise: a local named `q` may also exist, and resolving the
/// specifier locally would answer with **that** local's type. That is a
/// confident wrong answer where a gap belongs, which `docs/adr/0038` and
/// `docs/adr/0039` rank as worse than the gap — it is invisible in the
/// aggregate and it falsely credits a line.
///
/// **Red under:** deleting the `export.module_specifier.is_some()` guard from
/// `Checker::export_specifier_target`. The assertion then reads `number`.
#[test]
fn an_export_specifier_that_names_a_module_stays_a_gap() {
    assert_eq!(type_of_export("var q = 1;\nexport { q } from \"./m\";", "q"), "error");
}

/// `export { q as r }` exports `r` and looks up `q` — upstream's
/// `node.PropertyNameOrName()` (`checker.go:14952`).
///
/// The two names are deliberately given **different** types here, so the test
/// distinguishes "looked up the right name" from "looked up a name that
/// happened to resolve": a fixture where `q` and `r` were both `number` would
/// run this code and prove nothing.
///
/// **Red under:** replacing `specifier.property_name.or(specifier.name)` with
/// `specifier.name` in `Checker::export_specifier_target`. The first assertion
/// then reads `string` — the *other* local — rather than failing to resolve,
/// which is why the fixture declares one.
#[test]
fn export_q_as_r_looks_up_the_property_name() {
    let source = "var q = 1;\nvar r = \"s\";\nexport { q as r };";
    assert_eq!(type_of_export(source, "r"), "number");
}

/// An export specifier naming a type-only symbol has no type.
///
/// `getTypeOfAlias` (`checker.go:18612`) returns `errorType` unless the target
/// carries `SymbolFlagsValue`, and the baseline records exactly that:
/// `export { I, N, T }` prints `>I : any`, `>N : any`, `>T : any`, which is
/// upstream printing `errorType` as `any`.
///
/// **This port answers `errorType` and reports a gap, so these lines stay
/// unconverted.** That is the accepted cost of keeping `errorType` and `anyType`
/// distinguishable (`docs/adr/0038`): answering `any` here would convert the
/// line and would make every genuinely-uncomputed `any` indistinguishable from
/// this one. It is recorded rather than hidden because it caps what this arm can
/// convert.
///
/// **This test is NOT red under the obvious mutation, and that was measured
/// rather than assumed.** Deleting the `SymbolFlags::VALUE` test from
/// `Checker::get_type_of_alias` leaves all six tests in this file green:
/// `get_type_of_symbol` has no arm for `INTERFACE` or `TYPE_ALIAS` either, so
/// both routes reach `errorType` and no fixture built from a type-only target
/// can tell them apart. The guard stays because it is upstream's — the same
/// standing as `Binder::resolve_name`'s locals-before-members order, which
/// `crates/tsr-binder/src/lib.rs:340` records as *"stated rather than pinned by
/// a test that could not bite"*.
///
/// It becomes observable the moment `get_type_of_symbol` grows an arm for a
/// type-only symbol shape. **That is the falsifier**: whoever adds one must
/// re-run this test with the guard removed and expect red.
#[test]
fn a_type_only_export_specifier_stays_a_gap() {
    let source = "interface I { }\ntype T = number;\nexport { I, T };";
    assert_eq!(type_of_export(source, "I"), "error");
    assert_eq!(type_of_export(source, "T"), "error");
}

/// `export { a }` where `a` is itself an alias.
///
/// This is the case that forced `Checker::get_symbol_flags`
/// (`getSymbolFlagsEx`, `checker.go:16367`) to be ported. `getTypeOfAlias`
/// takes its `Value` test over the target's flags **followed through the alias
/// chain**, and `SymbolFlags::ALIAS` is disjoint from `SymbolFlags::VALUE`, so a
/// raw test says "no value" for a namespace that plainly has the type
/// `typeof N`.
///
/// **Red under:** replacing `self.get_symbol_flags(target)` with
/// `self.binder.symbols().get(target).flags` in `Checker::get_type_of_alias` —
/// the assertion reads `error`. That mutation leaves every other test in this
/// file green, because no other fixture here has an alias whose target is an
/// alias.
#[test]
fn an_export_specifier_naming_an_alias_follows_the_chain() {
    let source = "namespace N { export var v = 1; }\nimport a = N;\nexport { a };";
    assert_eq!(type_of_export(source, "a"), "typeof N");
}

/// A name that does not resolve is a gap, not a guess.
#[test]
fn an_export_specifier_naming_nothing_is_a_gap() {
    assert_eq!(type_of_export("export { nothingHere };", "nothingHere"), "error");
}

/// A **merged** symbol takes its alias target from the last alias-shaped
/// declaration, not from `declarations[0]`.
///
/// `getDeclarationOfAliasSymbol` (`checker.go:16397`) is
/// `core.FindLast(symbol.Declarations, ast.IsAliasSymbolDeclaration)`. Both
/// halves matter here: `export interface I {}` beside `export { N as I }` binds
/// **one** symbol carrying `INTERFACE | ALIAS` whose first declaration is the
/// `InterfaceDeclaration` — a node with no alias target at all.
///
/// This is the shape `docs/architecture/checker-notes-arrays.md` named as the
/// falsifier for the export-marker arm (*"merged declarations are where to
/// look"*), and `examples/symbol_dispatch_split.rs`'s `UNCLASSIFIED KIND`
/// control found exactly one line of it on the full corpus. One line is not why
/// it is fixed; the fact that the predicted failure is the one that showed up
/// is.
///
/// **Both fixtures are illegal TypeScript** — merging an alias with a local is
/// a duplicate-identifier error — and that is not a defect in the test. The
/// corpus is full of error cases, they carry `.types` baselines, and the
/// checker still has to pick the right declaration in them. Neither fixture
/// produces a parse diagnostic, which `type_of_export` asserts.
///
/// **Red under:** replacing `Checker::declaration_of_alias_symbol` with
/// `declarations.first()`. Both assertions then read `error`. It reddens no
/// other test in this file, because no other fixture here merges.
#[test]
fn a_merged_alias_uses_the_last_alias_shaped_declaration() {
    let source = "export interface I { }\n\
                  namespace N { export var v = 1; }\n\
                  export { N as I };";
    assert_eq!(type_of_export(source, "I"), "typeof N");
}
