//! Cross-file aliases: `import { x } from "./m"` and `export { q } from "./m"`.
//!
//! Both are `getExternalModuleMember` (`checker.go:14667`) — one upstream
//! function reached from `getTargetOfImportSpecifier` (`checker.go:14647`) and
//! from `getTargetOfExportSpecifier`'s module-specifier case
//! (`checker.go:14966`). They are one arm, not two, which is why one test file
//! covers both.
//!
//! # The harness is two files, and no harness change was needed
//!
//! `tests/export_specifiers.rs` builds one file with `tsr_parser::parse` and
//! `tsr_binder::bind`. A cross-file fixture cannot: a symbol from another file
//! is only safe to hand the checker when its declarations index the *same* node
//! table ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)).
//!
//! **The multi-file spelling already existed and needs no new dependency.**
//! `tsr_parser::parse_into` takes the node table and map by `&mut` so ids
//! continue rather than restart, and `tsr_binder::bind_into` accumulates into one
//! `SymbolStore`. Both are already dev-dependencies of this crate. So `program`
//! below is `Program::parse` and `Program::bind_source_files`
//! (`crates/tsr-compiler/src/lib.rs`) reduced to what a fixture needs, and
//! `tsr-compiler` is *not* a dependency — it depends on `tsr-checker`, so it
//! could not be one.
//!
//! # No fixture here names `Array`, `Promise` or `number[]`
//!
//! The harness loads no lib files, so any of those would measure the missing
//! library rather than this arm. `no_lib_control` pins that by construction: it
//! asserts `var x: number[]` is a gap *here*, so a reader who sees an
//! unexpected gap elsewhere can tell the two apart without re-deriving it.
//!
//! # The mutations
//!
//! Each test names a mutation it is red under. Every mutation was applied one at
//! a time, confirmed present with `grep -c` returning exactly 1 **before** the
//! test ran, and reverted after; the "also reddens" column is what was measured,
//! not what was expected.
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | invert `get_export_of_module`'s `MODULE` guard | [`a_named_import_takes_the_exported_symbols_type`] + 3 others |
//! | 2 | let `get_external_module_member` fall back to the module symbol | [`a_named_import_of_a_name_the_module_does_not_export_is_a_gap`] **only** |
//! | 3 | `node.name` instead of `property_name.or(name)` for an import specifier | [`a_named_import_of_a_renamed_export_looks_up_the_property_name`] **only** |
//! | 4 | restore `return None` when `module_specifier.is_some()` | [`a_re_export_with_a_module_specifier_resolves_through_the_module`] + the chain |
//! | 5 | route the no-specifier case through `get_external_module_member` too | [`a_same_file_export_specifier_still_resolves_locally`] **only** |
//! | 6 | delete `|| seen.contains(&target)` from `get_symbol_flags` | [`a_re_export_cycle_between_two_files_terminates`] — by **hanging** |
//! | 7 | take `get_type_of_alias`'s `VALUE` test over raw flags | [`a_two_link_re_export_chain_across_three_files_resolves`] **only** |
//! | 8 | `SyntaxKind::ImportSpecifier => return None` in `resolve_alias` | [`a_same_file_export_specifier_naming_an_import_converts_too`] + 2 others |
//!
//! Tests 8 and 9 —
//! [`a_same_file_export_specifier_naming_an_import_converts_too`] and
//! [`a_same_file_export_specifier_naming_a_re_export_converts_too`] — were added
//! **after** the corpus run, and they are not new behaviour. They reproduce a
//! **registered must-not-move condition that moved**: the same-file
//! `export { q }` row fell 198 -> 165, which §16 of
//! `docs/architecture/checker-notes-symbols.md` had predicted would not change.
//! Each is red under the arm that explains it (8 above; 9 under mutation 4), and
//! test 8 carries its own discriminator in the body — the identical fixture
//! asserted to gap with **no** host, which is what makes it evidence that this
//! arm caused the movement rather than a story that it could have.
//!
//! The tests pair as (1, 2), (4, 5) and (6, 7), one behaviour change each, and
//! **within every pair the two mutations are disjoint** — neither test can be
//! reddened by its partner's mutation. That is the property being bought: it is
//! what makes each test evidence about one thing rather than about the arm being
//! present at all. Across pairs they overlap, and that is structural — mutation
//! 1 disables the export lookup, which every positive arm performs.
//!
//! # Two mutations were written, measured, and did **not** bite
//!
//! Recorded because a fixture that runs the right code and cannot discriminate
//! is this project's most common test defect (`checker-notes-arrays.md`, six in
//! one workstream), and because both were written on the belief that they were
//! the cycle guard:
//!
//! - **Removing `resolve_alias`'s `AliasTarget` resolution frame** — all 12
//!   green. The frame was deleted rather than kept; see `Checker::resolve_alias`.
//! - **Disabling `get_type_of_alias`'s `PropertyName::Type` frame** — all 12
//!   green. It stays, because it is upstream's and guards `getTypeOfSymbol`
//!   recursion that these fixtures do not reach.
//!
//! Only mutation 6 hangs the cycle fixture, which is how the real guard was
//! found. **Three candidates, one of them right, and reading alone had picked
//! the wrong one twice.**

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, SymbolFlags, SymbolId};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

/// A [`ModuleHost`] over a fixed set of fixture files.
///
/// Stands in for `Program::GetResolvedModule` (`internal/compiler/program.go:521`)
/// composed with the loader's module resolution. Resolution proper is
/// `tsr-module`'s filesystem walk and is not what these tests are about: the
/// question here is what the checker does *with* a resolved file, so the
/// mapping is spelled out in the fixture.
///
/// `"./m"` names the fixture called `m`. A specifier with no matching fixture
/// answers `None`, which is upstream's unresolved module — and
/// [`a_named_import_from_an_unresolved_module_is_a_gap`] is the test that pins
/// it.
struct Fixtures {
    /// Fixture name to that file's `SourceFile` node id, in load order.
    files: Vec<(&'static str, NodeId)>,
}

impl ModuleHost for Fixtures {
    /// The fixtures resolve by exact name, so "found a file" and "found a file
    /// the program holds" are the same question here — unlike a real
    /// `Program`, where the second is a membership hop the first does not make.
    fn module_resolution_found(&self, importing_file: tsr_ast::NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }

    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, _)| *fixture == name).map(|&(_, id)| id)
    }
}

/// Everything one fixture program produced.
///
/// Held together because the checker borrows all three and the borrows must
/// outlive it.
struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

/// Parse and bind several files into **one** identity space.
///
/// This is `Program::parse` followed by `Program::bind_source_files` reduced to
/// a fixture: parse each file into the shared tables, then bind each in the
/// order it was parsed. Both orders are load-bearing and neither is checked by
/// the types — see `tsr_binder::bind_into`'s docs.
fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let file = tsr_parser::parse_into(
            arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(
            file.diagnostics.is_empty(),
            "fixture {name} must parse: {:?}",
            file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        parsed.push((*name, source, file.source_file));
    }

    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        // The name the host will be asked for is the fixture's, and the id is
        // this file's `SourceFile` node — the same id `source_file_of` reaches
        // by walking parents from any node inside it.
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound, host }
}

/// The type of the alias declared by an import or export specifier named `name`.
///
/// This is the position the `.types` baseline records and the position
/// `rank_board`'s row `declaration name … SymbolFlags(ALIAS) / no value
/// declaration` is measured at: the specifier's own name is a **declaration
/// name**, which `types_producer` answers through its parent's symbol and
/// `getTypeOfSymbol` (`crates/tsr-conformance/src/types_producer.rs`).
///
/// `with_host` is what makes the negative control possible: the *same* fixture
/// checked without a host must gap, which is how
/// [`the_same_fixture_gaps_with_no_host`] shows the arm is additive rather than
/// asserting it.
fn type_of_alias(fixture: &Fixture<'_>, name: &str, with_host: bool) -> String {
    let host: Option<&dyn ModuleHost> = if with_host { Some(&fixture.host) } else { None };
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let mut found = None;
    for index in 0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32") {
        let id = NodeId::new(index);
        if !matches!(
            fixture.nodes.kind(id),
            SyntaxKind::ImportSpecifier | SyntaxKind::ExportSpecifier
        ) {
            continue;
        }
        let Some(symbol) = fixture.bound.symbol_of(id) else { continue };
        let entry = fixture.bound.symbols().get(symbol);
        if entry.name != name {
            continue;
        }
        assert!(
            entry.flags.intersects(SymbolFlags::ALIAS),
            "an import or export specifier binds an alias symbol"
        );
        assert!(
            entry.value_declaration.is_none(),
            "an alias has no value declaration — that is why the row is named as it is"
        );
        found = Some(symbol);
    }
    let symbol = found.unwrap_or_else(|| panic!("no import or export specifier named `{name}`"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// [`type_of_alias`] restricted to one specifier kind.
///
/// Needed only where a fixture holds an import specifier **and** an export
/// specifier of the same name — `import { x } from "./m"; export { x };` — which
/// is exactly the shape that explains the moved same-file row. Picking "the last
/// one" there would depend on declaration order rather than on the kind under
/// test.
fn type_of_alias_of_kind(
    fixture: &Fixture<'_>,
    name: &str,
    kind: SyntaxKind,
    with_host: bool,
) -> String {
    let host: Option<&dyn ModuleHost> = if with_host { Some(&fixture.host) } else { None };
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let symbol = (0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == kind)
        .filter_map(|id| fixture.bound.symbol_of(id))
        .find(|&s| fixture.bound.symbols().get(s).name == name)
        .unwrap_or_else(|| panic!("no {kind:?} named `{name}`"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// The type of a variable declared in the last fixture file, by symbol name.
///
/// Only [`no_lib_control`] needs this; it is here rather than inline so the
/// control reads as one assertion.
fn type_of_variable(fixture: &Fixture<'_>, name: &str) -> String {
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, None);
    let symbol: SymbolId = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        .find_map(|id| fixture.bound.symbol_of(id))
        .filter(|&s| fixture.bound.symbols().get(s).name == name)
        .expect("the fixture declares that variable");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// A module exporting one annotated value, used by most tests below.
///
/// Annotated rather than `export const x = 1` deliberately: a `const` with a
/// literal initialiser answers the literal type `1`, which would make the
/// assertions below depend on literal widening as well as on this arm.
const M: (&str, &str) = ("m", "export const x: number = 1;\n");

#[test]
fn a_named_import_takes_the_exported_symbols_type() {
    // The arm, in its plainest form. `x` is an alias whose target is `m`'s
    // exported `x`, and the exported symbol's type is what the alias has.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "number");
}

#[test]
fn the_same_fixture_gaps_with_no_host() {
    // The additivity claim, as a test rather than an argument: the identical
    // program checked by a `Checker::new` — which is `with_module_host(…, None)`
    // — answers `error`, which is what every call site that existed before this
    // arm answers. Pinned by construction: with no host,
    // `resolve_external_module_name` cannot reach step 2 at all.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", false), "error");
}

#[test]
fn a_named_import_of_a_renamed_export_looks_up_the_property_name() {
    // `node.PropertyNameOrName()` (`checker.go:14677`): `import { x as y }`
    // looks up `x` in the module and calls the local alias `y`. Reading the
    // specifier's `name` instead would look up `y`, which `m` does not export.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x as y } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "y", true), "number");
}

#[test]
fn a_named_import_of_a_name_the_module_does_not_export_is_a_gap() {
    // **The negative half of the pair, and it is what stops the arm answering a
    // plausible wrong type.** `getExportOfModule` (`checker.go:14789`) answers
    // `nil` for a name that is not in the table; falling back to the module
    // symbol would print `typeof /m.ts` here — a confident wrong answer where a
    // gap belongs.
    //
    // Red under: `get_external_module_member` falling back to the module
    // symbol. Green under the `MODULE`-guard mutation its pair names, because a
    // missing name gaps either way.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { q } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "q", true), "error");
}

#[test]
fn a_named_import_from_an_unresolved_module_is_a_gap() {
    // A control pinned by **construction**, not by arithmetic: the fixture set
    // contains no file named `nope`, so `Fixtures::resolved_module` cannot
    // answer `Some` for it whatever the checker does. This is upstream's
    // unresolved module, `Cannot_find_module_0_or_its_corresponding_type_declarations`.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./nope\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "error");
}

#[test]
fn a_named_import_from_a_file_that_is_not_a_module_is_a_gap() {
    // `sourceFile.Symbol != nil` (`checker.go:15321`) — upstream's
    // `File_0_is_not_a_module`. `s` has no top-level `import` or `export`, so
    // the binder never calls `bind_source_file_as_external_module` and the file
    // has no symbol.
    //
    // **This is why the host answers a file and not a symbol.** Resolution
    // succeeded; there is simply no module at the end of it, and only the
    // checker can tell those two apart.
    let arena = Arena::new();
    let fixture =
        program(&arena, &[("s", "var x: number = 1;\n"), ("a", "import { x } from \"./s\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "error");
}

#[test]
fn a_same_file_export_specifier_naming_an_import_converts_too() {
    // **The registered must-not-move condition that MOVED, reproduced.**
    //
    // `docs/architecture/checker-notes-symbols.md` §16 named the same-file
    // `export { q }` row as unchanged by this arm. It fell 198 -> 165 on the
    // corpus. This test is the mechanism, isolated: the specifier is same-file
    // and its lookup is untouched, but its **target** is an import alias that
    // could not resolve before.
    //
    // `getTypeOfAlias` (`checker.go:18598`) takes its `VALUE` test over
    // `getSymbolFlags` (`checker.go:16367`), which walks the alias *chain*. So
    // `export { x }` naming `import { x } from "./m"` was `errorType` for a
    // reason that had nothing to do with the export specifier: the chain ended
    // at an alias with no `VALUE` bit, because the import arm did not exist.
    //
    // The same-file row was therefore never a population of same-file *work* —
    // it is a population of same-file *syntax*, and some of it was blocked
    // cross-file all along.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./m\";\nexport { x };\n")]);
    assert_eq!(type_of_alias_of_kind(&fixture, "x", SyntaxKind::ExportSpecifier, true), "number");
    // And it is the arm that did it: without the host the same fixture gaps.
    assert_eq!(type_of_alias_of_kind(&fixture, "x", SyntaxKind::ExportSpecifier, false), "error");
}

#[test]
fn a_same_file_export_specifier_naming_a_re_export_converts_too() {
    // The second of exactly two routes by which a same-file specifier can
    // convert, enumerated from the diff rather than guessed: its target is an
    // export specifier that *does* carry a module specifier.
    //
    // Together with the test above this closes the question the corpus raised.
    // The same-file lookup itself is byte-identical across `fa29e66^..fa29e66`
    // — `export_specifier_target`'s `module_specifier.is_none()` branch was not
    // touched — so a same-file line can only have converted through its target,
    // and its target is reached by one of these two new arms.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            M,
            ("b", "export { x } from \"./m\";\n"),
            ("a", "import { x } from \"./b\";\nexport { x };\n"),
        ],
    );
    assert_eq!(type_of_alias_of_kind(&fixture, "x", SyntaxKind::ExportSpecifier, true), "number");
}

#[test]
fn a_re_export_with_a_module_specifier_resolves_through_the_module() {
    // The other half of the arm. Before this, `export { x } from "./m"` was the
    // documented `return None` on the module-specifier guard in
    // `export_specifier_target` — the guard that made `export { q }` correct is
    // now the branch, not the bail-out.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "export { x } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "number");
}

#[test]
fn a_same_file_export_specifier_still_resolves_locally() {
    // **The pair's other half, and a regression guard on the arm that landed in
    // `c60b086`.** `export { q }` is a plain local lookup, so it must *not*
    // reach `get_external_module_member`: that function reads the export
    // declaration's module specifier, which this form does not have.
    //
    // Red under: routing the no-specifier case through
    // `get_external_module_member`. Green under the mutation its pair names,
    // which only affects declarations that *have* a specifier.
    let arena = Arena::new();
    let fixture = program(&arena, &[("a", "const q: number = 1;\nexport { q };\n")]);
    assert_eq!(type_of_alias(&fixture, "q", true), "number");
}

#[test]
fn a_re_export_cycle_between_two_files_terminates() {
    // `a.ts` re-exports `q` from `b.ts`, which re-exports `q` from `a.ts`. A
    // real program can be shaped this way and `docs/conventions.md` is explicit
    // that a port which passes the corpus and hangs on a real program is the
    // worse failure — so this test exists to assert termination, and the value
    // it checks is secondary.
    //
    // **What makes it terminate is `get_symbol_flags`'s visited set** —
    // upstream's own `seenSymbols` (`checker.go:16368`) — and finding that out
    // cost three wrong guesses, each recorded because the wrong ones are the
    // useful part.
    //
    // The first draft of this arm ported upstream's `AliasTarget` resolution
    // frame (`checker.go:16272`) *as* the termination guard. This test is green
    // with that frame removed, and green with `get_type_of_alias`'s
    // `PropertyName::Type` frame disabled too. Both were measured, not reasoned
    // about; the `AliasTarget` frame was then deleted rather than shipped,
    // because a guard nobody can make fire reads as safety and supplies none.
    // See `Checker::resolve_alias` for why it cannot: that function is not
    // self-recursive in this port.
    //
    // Red — by **hanging** rather than by assertion, which is what a termination
    // guard's failure looks like — under: deleting `|| seen.contains(&target)`
    // from `Checker::get_symbol_flags`. Run it with a timeout.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("a", "export { q } from \"./b\";\n"), ("b", "export { q } from \"./a\";\n")],
    );
    assert_eq!(type_of_alias(&fixture, "q", true), "error");
}

#[test]
fn a_two_link_re_export_chain_across_three_files_resolves() {
    // The acyclic counterpart, and the reason the cycle test above is not
    // vacuous: a chain of the same length that does *not* close must still reach
    // the value. `c` declares it, `b` re-exports it, `a` re-exports `b`'s
    // re-export — so `a`'s alias target is itself an alias, and
    // `get_type_of_symbol` re-enters `get_type_of_alias` for it.
    //
    // Red under: taking `get_type_of_alias`'s `VALUE` test over the target's raw
    // flags instead of over `get_symbol_flags`. `SymbolFlags::ALIAS` is disjoint
    // from `VALUE`, so a raw test rejects `a`'s target — which is itself an
    // alias — before its type is ever asked for. Green under the cycle test's
    // mutation, because nothing here loops.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("c", "export const q: number = 1;\n"),
            ("b", "export { q } from \"./c\";\n"),
            ("a", "export { q } from \"./b\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "q", true), "number");
}

#[test]
fn a_named_import_from_a_module_with_export_equals_is_a_gap() {
    // `resolveExternalModuleSymbol` (`checker.go:15556`) makes a module that
    // writes `export = X` *be* `X`, and upstream then reads the member off
    // `getTypeOfSymbol(X)` through `getPropertyOfTypeEx`, possibly combining a
    // value symbol with a type symbol. None of that is ported, so the arm
    // declines the whole form rather than looking the name up in a table that
    // no longer means what it did.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { x: number };\nexport = o;\n"),
            ("a", "import { x } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "x", true), "error");
}

#[test]
fn no_lib_control() {
    // Pinned by construction and not by arithmetic: this harness parses and
    // binds only the fixture files, so no `lib.*.d.ts` is present and `Array`
    // does not exist. Any fixture above that named `number[]`, `Array` or
    // `Promise` would be measuring that absence.
    //
    // `docs/conventions.md`: three probes in this project re-implemented the
    // harness and measured a different compiler. This is the same hazard one
    // level down — a *test* that measures the missing library instead of the
    // arm — and the control is what makes the difference visible rather than
    // inferred.
    let arena = Arena::new();
    let fixture = program(&arena, &[("a", "var x: number[];\n")]);
    assert_eq!(type_of_variable(&fixture, "x"), "error");
}
