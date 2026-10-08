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

#[test]
fn imported_value_read_and_assignment_keep_distinct_native_results() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export const value = 17;"),
            ("main", "import { value } from './m'; value; value++; value = 2;"),
        ],
    );
    let root = fixture.host.files[1].1;
    let tsr_ast::Node::SourceFile(file) = fixture.node_map.get(root).unwrap() else { panic!() };
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let tsr_ast::Statement::ExpressionStatement(read) = file.statements[1] else { panic!() };
    let read_type = checker.check_expression(read.expression.unwrap());
    assert_eq!(checker.type_to_string(read_type), "17");
    let tsr_ast::Statement::ExpressionStatement(increment) = file.statements[2] else { panic!() };
    let tsr_ast::Expression::PostfixUnaryExpression(increment) = increment.expression.unwrap()
    else {
        panic!()
    };
    // `checker.go:11094`: upstream's own `errorType` (ADR-0048), not the gap.
    let assignment_type = checker.check_expression(increment.operand.unwrap());
    assert_eq!(assignment_type, checker.intrinsics().native_error);
    let tsr_ast::Statement::ExpressionStatement(assignment) = file.statements[3] else { panic!() };
    let tsr_ast::Expression::BinaryExpression(assignment) = assignment.expression.unwrap() else {
        panic!()
    };
    let target_type = checker.check_expression(assignment.left.unwrap());
    assert_eq!(target_type, checker.intrinsics().native_error);
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

/// Whether a named import/export alias has a semantic target, independent of
/// whether [`Checker::get_type_of_symbol`] can use that target as a value.
fn alias_resolves(fixture: &Fixture<'_>, name: &str) -> bool {
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let symbol = (0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32"))
        .map(NodeId::new)
        .filter(|&id| {
            matches!(
                fixture.nodes.kind(id),
                SyntaxKind::ImportSpecifier | SyntaxKind::ExportSpecifier
            )
        })
        .filter_map(|id| fixture.bound.symbol_of(id))
        .find(|&symbol| fixture.bound.symbols().get(symbol).name == name)
        .unwrap_or_else(|| panic!("no import or export specifier named `{name}`"));
    checker.resolve_alias(symbol).is_some()
}

/// Render the alias at its declaration, rather than baking the remote name.
fn alias_rendered_at(fixture: &Fixture<'_>, name: &str) -> String {
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let declaration = (0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32"))
        .map(NodeId::new)
        .filter(|&id| {
            matches!(
                fixture.nodes.kind(id),
                SyntaxKind::ImportSpecifier | SyntaxKind::ExportSpecifier
            )
        })
        .find(|&id| {
            fixture.bound.symbol_of(id).is_some_and(|s| fixture.bound.symbols().get(s).name == name)
        })
        .unwrap_or_else(|| panic!("no specifier named `{name}`"));
    let symbol = fixture.bound.symbol_of(declaration).expect("the specifier binds an alias");
    let id = checker.get_type_of_symbol(symbol);
    let reference =
        fixture.node_map.get(declaration).and_then(|node| node.name_id()).unwrap_or(declaration);
    checker.type_to_string_at(id, reference).unwrap_or_else(|| "error".to_string())
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
    type_of_variable_with_host(fixture, name, false)
}

/// [`type_of_variable`] with the fixture host wired in.
///
/// `import * as p from "./m"` reaches the module through *module resolution*,
/// so a no-host checker answers `error` for every member of it — including the
/// control. The member-access tests at the end of this file all need it, and
/// the first run without it failed the control too, which is how a harness gap
/// announces itself rather than masquerading as a finding.
fn type_of_variable_with_host(fixture: &Fixture<'_>, name: &str, with_host: bool) -> String {
    let host: Option<&dyn ModuleHost> = if with_host { Some(&fixture.host) } else { None };
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let symbol: SymbolId = (0..u32::try_from(fixture.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == SyntaxKind::VariableDeclaration)
        // `filter_map` then `find`, not `find_map` then `filter`: the latter
        // takes the *first* variable in the program and then checks its name,
        // so it only ever worked for a fixture with exactly one. Every fixture
        // here had one until the member-access tests below added a second.
        .filter_map(|id| fixture.bound.symbol_of(id))
        .find(|&s| fixture.bound.symbols().get(s).name == name)
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
fn a_named_import_of_a_name_the_module_does_not_export_is_any() {
    // **Flipped by §130** (`checker-notes-narrow.md`; the thirty-fourth
    // stand-in): the module resolves, its non-empty star-free exports table
    // establishes the name's absence, and the alias answers upstream's
    // TS2305 error-any at every use. The pin's load-bearing claim is
    // UNCHANGED and still discriminates: falling back to the module symbol
    // would print `typeof /m.ts` here, and `any` is not `typeof /m.ts` —
    // the establishment gates (non-empty, no `export *`, no `default`, no
    // non-identifier keys) are what keep an under-filled table an honest
    // gap rather than a confident any.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { q } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "q", true), "any");
}

#[test]
fn a_named_import_from_an_unresolved_module_is_any() {
    // **Renamed with its new truth by §119** (`checker-notes-narrow.md`) — the
    // twenty-ninth stand-in to come due. This used to pin `error`, the
    // §31.1-era reading that an unresolved module leaves its imports as gaps.
    // The corpus says otherwise: upstream answers TS2307's deliberate
    // error-answer at every use of the alias, and its `.types` baselines print
    // `any` (`importNotElidedWhenNotFound`, `unusedInvalidTypeArguments` —
    // §119's +235). The fixture set contains no file named `nope`, so the
    // specifier is unfindable by construction and the §119 arm answers `any`.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./nope\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "any");
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
fn a_named_import_reads_a_value_member_from_an_export_equals_target() {
    // `resolveExternalModuleSymbol` (`checker.go:15556`) makes a module that
    // writes `export = X` *be* `X`; `getExternalModuleMember`
    // (`checker.go:14667`) then reads the named member from
    // `getTypeOfSymbol(X)`, not from the original module's exports table.
    //
    // The annotation is asymmetric (`number`, not the alias fallback's `any`)
    // and the target has no module exports of its own, so neither a direct
    // module-table lookup nor a missing-member error can satisfy this control.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { x: number };\nexport = o;\n"),
            ("a", "import { x } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "x", true), "number");
}

#[test]
fn a_missing_member_of_an_export_equals_target_remains_a_gap() {
    // The negative half: a resolved target is not itself the answer. Upstream
    // asks for the named property, and a miss remains a miss. Falling back to
    // the target would print its whole `{ x: number }` type here.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { x: number };\nexport = o;\n"),
            ("a", "import { absent } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "absent", true), "error");
}

#[test]
fn a_default_named_import_does_not_select_an_export_equals_property() {
    // `getTargetOfImportSpecifier` sends the name `default` through native's
    // dedicated synthetic-default road before `getExternalModuleMember`. The
    // ordinary target property is `number` here, so this is red if the
    // export-equals member arm incorrectly handles `default` itself. The
    // synthetic default is the whole `export =` value
    // (`getTargetOfModuleDefault`, `checker.go:14578`).
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { default: number };\nexport = o;\n"),
            ("a", "import { default as picked } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "picked", true), "{ default: number; }");
}

#[test]
fn a_default_re_export_does_not_select_an_export_equals_property() {
    // The matching `getTargetOfExportSpecifier` control. Returning the target's
    // numeric `default` property would type `forwarded` as `number`; the
    // native dedicated-default road answers the whole `export =` value.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { default: number };\nexport = o;\n"),
            ("a", "export { default as forwarded } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "forwarded", true), "{ default: number; }");
}

#[test]
fn a_plain_es_module_named_import_keeps_its_existing_path() {
    // The non-`export =` control. The target and module symbol are identical,
    // so this must continue to use the original exports-table road rather than
    // trying to treat the module object as an export-equals value.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import { x } from \"./m\";\n")]);
    assert_eq!(type_of_alias(&fixture, "x", true), "number");
}

#[test]
fn a_member_needing_site_aware_type_spelling_remains_a_gap() {
    // Native can spell this return type as `import("./m").Result` from the
    // importing file. This checker currently bakes `Owner.Result` into the
    // function type, so resolving the member would replace a semantic gap with
    // a wrong spelling. Primitive members do not cross that naming boundary.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare namespace Owner { export interface Result {}\nexport function make(): Result }\nexport = Owner;\n",
            ),
            ("a", "import { make } from \"./m\";\n"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "make", true), "error");
}

#[test]
fn function_interface_fallback_members_are_not_export_equals_members() {
    // `length` and `name` are supplied by the global Function interface, not
    // declared on Owner. Native passes skipObjectFunctionPropertyAugment=true
    // for this lookup, so their primitive number/string types must not let
    // these fallback members masquerade as named exports of Owner.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("lib", "interface Function { readonly length: number; readonly name: string }\n"),
            ("m", "declare function Owner(): void;\nexport = Owner;\n"),
            ("a", "import { length, name } from \"./m\";\n"),
        ],
    );
    assert!(!alias_resolves(&fixture, "length"));
    assert!(!alias_resolves(&fixture, "name"));
}

#[test]
fn an_own_numeric_function_member_is_still_an_export_equals_member() {
    // The skip flag applies only to Object/Function augmentation. An explicitly
    // declared own member with the same primitive shape remains discoverable.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("lib", "interface Function { readonly length: number }\n"),
            (
                "m",
                "declare function Owner(): void;\ndeclare namespace Owner { export const ownCount: 7 }\nexport = Owner;\n",
            ),
            ("a", "import { ownCount } from \"./m\";\n"),
        ],
    );
    assert!(alias_resolves(&fixture, "ownCount"));
    assert_eq!(type_of_alias(&fixture, "ownCount", true), "7");
}

#[test]
fn an_enum_literal_member_keeps_its_owner_at_the_import_site() {
    // The value member is E.A, not 0 and not the namespace object Owner.
    // Two consumers of the same member must name its enum through their own
    // namespace import. Looking up the value must not bake either site's name.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare namespace Owner { export enum E { A = 3, B = 8 }\nexport const state: E.A }\nexport = Owner;\n",
            ),
            (
                "a",
                "import * as First from \"./m\";\nimport { state as firstState } from \"./m\";\n",
            ),
            (
                "b",
                "import * as Second from \"./m\";\nimport { state as secondState } from \"./m\";\n",
            ),
        ],
    );
    assert!(alias_resolves(&fixture, "firstState"));
    assert_eq!(alias_rendered_at(&fixture, "firstState"), "First.E.A");
    assert_eq!(alias_rendered_at(&fixture, "secondState"), "Second.E.A");
}

#[test]
fn a_named_import_from_an_export_equals_enum_is_a_member_not_the_enum_object() {
    // Here the export-equals target IS an enum, rather than a namespace
    // containing an enum-typed variable. The selected alias denotes A, while
    // the namespace import denotes typeof E. Neither is the primitive value 3.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "enum E { A = 3, B = 8 }\nexport = E;\n"),
            ("a", "import * as EnumView from \"./m\";\nimport { A as selected } from \"./m\";\n"),
        ],
    );
    assert!(alias_resolves(&fixture, "selected"));
    assert_eq!(alias_rendered_at(&fixture, "selected"), "EnumView.A");
}

#[test]
fn a_unique_symbol_member_retains_uniqueness_but_its_copy_widens() {
    // Native imports the property's unique identity. It does not select the
    // namespace object, nor flatten the imported member itself to symbol.
    // A new const initialized from that import is a copy and does widen.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare namespace Owner { export const key: unique symbol }\nexport = Owner;\n"),
            ("a", "import { key as importedKey } from \"./m\";\nconst copied = importedKey;\n"),
        ],
    );
    assert!(alias_resolves(&fixture, "importedKey"));
    assert_eq!(alias_rendered_at(&fixture, "importedKey"), "unique symbol");
    assert_eq!(type_of_variable_with_host(&fixture, "copied", true), "symbol");
}

#[test]
fn an_original_module_value_export_cannot_replace_an_export_equals_member() {
    // getExportsOfModuleWorker carries only TYPE/NAMESPACE-only exports over
    // from the original module. This invalid extra value export is ignored:
    // native still selects the target's 7, not the original module's 19.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare const o: { Item: 7 };\nexport declare const Item: 19;\nexport = o;\n"),
            ("a", "import { Item as selected } from \"./m\";\n"),
        ],
    );
    assert!(alias_resolves(&fixture, "selected"));
    assert_eq!(alias_rendered_at(&fixture, "selected"), "7");
}

#[test]
fn an_original_value_export_cannot_make_a_declined_object_member_representable() {
    // Native selects the target's object here too, not the numeric export.
    // The former is outside this port's representable slice, so keep the gap.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare const o: { Item: { wrong: string } };\nexport declare const Item: 19;\nexport = o;\n",
            ),
            ("a", "import { Item as selected } from \"./m\";\n"),
        ],
    );
    assert!(!alias_resolves(&fixture, "selected"));
}

#[test]
fn a_type_only_supplement_does_not_erase_the_value_member() {
    // Upstream combines a value property on the export-equals target with a
    // supplemental type export on the original module. This checker cannot
    // allocate that synthetic combined symbol without crossing the immutable
    // binder boundary, so it must decline rather than return either half and
    // erase the other meaning.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare const o: { Item: number };\nexport interface Item { tag: string }\nexport = o;\n",
            ),
            ("a", "import { Item } from \"./m\";\n"),
        ],
    );
    assert!(!alias_resolves(&fixture, "Item"));
}

#[test]
fn a_unique_symbol_value_does_not_fall_through_to_a_type_only_supplement() {
    // The target owns a unique-symbol VALUE named Token and the original module
    // owns a distinct type-only Token. Native combines both meanings. Selecting
    // either the now-representable value or the interface alone would erase
    // one meaning; this still needs a synthetic symbol and remains a gap.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare namespace Owner { export const Token: unique symbol }\nexport interface Token { tag: string }\nexport = Owner;\n",
            ),
            ("a", "import { Token } from \"./m\";\n"),
        ],
    );
    assert!(!alias_resolves(&fixture, "Token"));
}

#[test]
fn a_declined_object_value_does_not_fall_through_to_a_type_only_supplement() {
    // The supplemental VALUE shortcut must not admit a type-only supplement:
    // native combines the object's value with the interface's type meaning.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "m",
                "declare const o: { Item: { wrong: string } };\nexport interface Item { tag: number }\nexport = o;\n",
            ),
            ("a", "import { Item as selected } from \"./m\";\n"),
        ],
    );
    assert!(!alias_resolves(&fixture, "selected"));
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

// ---------------------------------------------------------------------------
// Member access on a module object, where the member is an ALIAS
//
// `import * as P from "./m"; P.y` reads `m`'s exports through
// `get_property_of_anonymous_symbol`, whose `symbolIsValue` gate
// (`checker.go:22095`) has two disjuncts. Only the first was ported, and an
// alias's own flags carry **no** `VALUE` bit — so every export written as a
// *specifier* rather than as a declaration answered "no property", which
// `nonexistent_property` reports as TS2339.
//
// The corpus never caught it, and it covers the negative direction only:
// `conformance/exportNamespace3` and `conformance/importEquals2` both pass
// *because* a type-only alias must NOT become a value, and both broke when the
// alias half landed without `excludeTypeOnlyMeanings`. Nothing in the corpus
// covers the positive, which is why the whole barrel-module shape was broken
// while the suite read 100% on it. `checker-notes-diag2.md` §400.
// ---------------------------------------------------------------------------

/// `m`'s value, re-exported under a new name by a specifier.
const RENAMED: (&str, &str) = ("renamed", "export { x as y } from \"./m\";\n");

#[test]
fn a_module_objects_member_resolves_through_a_re_export_specifier() {
    // The shape every `index.parts.d.ts` barrel has, and the one that reported
    // 1,384 TS2339 on a 22-package repository against `tsc`'s zero.
    //
    // Red under: `symbol_is_value` taking the raw flags, which is what it did.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[M, RENAMED, ("a", "import * as p from \"./renamed\";\nconst v = p.y;\n")],
    );
    assert_eq!(type_of_variable_with_host(&fixture, "v", true), "number");
}

#[test]
fn a_module_objects_member_resolves_through_a_same_file_export_specifier() {
    // No module specifier on the export — the alias is local. Same gate, and it
    // is worth its own test because `resolve_alias` takes a different arm for
    // it than for the re-export above.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("local", "const inner: number = 1;\nexport { inner as outer };\n"),
            ("a", "import * as p from \"./local\";\nconst v = p.outer;\n"),
        ],
    );
    assert_eq!(type_of_variable_with_host(&fixture, "v", true), "number");
}

#[test]
fn a_type_only_re_export_is_not_a_value_member() {
    // `excludeTypeOnlyMeanings` (`checker.go:16374`). `symbolIsValue` passes
    // `includeTypeOnlyMembers: false`, so a type-only alias must not contribute
    // its target's value-ness however much of a value the target is.
    //
    // This is the direction the corpus already tests — and it is the direction
    // that broke when the alias half first landed without the gate, silently,
    // reported by the suite only as `1,898 -> 1,896`.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("cls", "export class C {}\n"),
            ("typeonly", "export type { C } from \"./cls\";\n"),
            ("a", "import * as p from \"./typeonly\";\nconst v = p.C;\n"),
        ],
    );
    // Not `typeof C`: the member is not a value, so the lookup misses and the
    // variable has no type — which is what leaves upstream's TS2339 standing.
    assert_eq!(type_of_variable_with_host(&fixture, "v", true), "error");
}

#[test]
fn a_directly_declared_export_is_unaffected() {
    // The control for the pair above: a member that was never an alias. If this
    // ever moves, the change is not about aliases at all.
    let arena = Arena::new();
    let fixture = program(&arena, &[M, ("a", "import * as p from \"./m\";\nconst v = p.x;\n")]);
    assert_eq!(type_of_variable_with_host(&fixture, "v", true), "number");
}

#[test]
fn exported_require_aliases_use_target_meaning_in_the_original_scope_walk() {
    // Pinned 5b1047d getSymbol / nameresolver.go:99: a class export has
    // TYPE and VALUE, but namespace/function exports must leave an outer TYPE
    // binding visible. Unconditional exported ALIAS admission fails the latter.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            (
                "globals",
                "interface NamespaceAlias { marker: string; }
interface CallableAlias { marker: number; }
interface ClassAlias { marker: boolean; }",
            ),
            ("namespace", "export const value = 'west';"),
            ("callable", "declare function callable(value: number): number; export = callable;"),
            ("class", "declare class ClassTarget { marker: number; } export = ClassTarget;"),
            (
                "entry",
                "export import NamespaceAlias = require('./namespace');
export import CallableAlias = require('./callable');
export import ClassAlias = require('./class');
declare const namespaceType: NamespaceAlias;
declare const callableType: CallableAlias;
declare const classType: ClassAlias;
export const namespaceMarker = namespaceType.marker;
export const callableMarker = callableType.marker;
export const classMarker = classType.marker;
export const value = NamespaceAlias.value;
export const result = CallableAlias(7);
export const instance = new ClassAlias();
export const instanceMarker = instance.marker;",
            ),
        ],
    );
    let entry = fixture.host.files.last().unwrap().1;
    let module = fixture.bound.symbol_of(entry).unwrap();
    for name in ["NamespaceAlias", "CallableAlias", "ClassAlias"] {
        assert!(fixture.bound.lookup_local(entry, name).is_none(), "exports-only owner");
        assert!(fixture.bound.symbols().get(module).exports.contains_key(name));
    }
    let expectations = [
        ("classMarker", "number"),
        ("namespaceMarker", "string"),
        ("callableMarker", "number"),
        ("value", "\"west\""),
        ("result", "number"),
        // Native prints the instance as ClassAlias; the unrelated declaration
        // naming gap remains. Its semantic member must still be number.
        ("instanceMarker", "number"),
    ];
    for reverse in [false, true] {
        let mut checker = Checker::with_module_host(
            &fixture.bound,
            &fixture.nodes,
            &fixture.node_map,
            Some(&fixture.host),
        );
        for warm in [false, true] {
            for index in 0..expectations.len() {
                let index = if reverse ^ warm { expectations.len() - index - 1 } else { index };
                let (name, expected) = expectations[index];
                let symbol = fixture.bound.symbols().get(module).exports[name];
                let ty = checker.get_type_of_symbol(symbol);
                assert_eq!(checker.type_to_string(ty), expected, "{name}, {reverse}, {warm}");
            }
        }
    }
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    checker.apply_compiler_options(&tsr_core::CompilerOptions {
        module: tsr_core::ModuleKind::CommonJS,
        ..Default::default()
    });
    for &(_, file) in &fixture.host.files {
        checker.check_source_file(
            file,
            tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
        );
    }
    assert!(checker.diagnostics().is_empty(), "native empty bag: {:?}", checker.diagnostics());
}

#[test]
fn unsupported_exported_require_target_does_not_claim_an_outer_binding() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("globals", "interface MissingAlias { marker: boolean; }"),
            (
                "entry",
                "export import MissingAlias = require('./missing');
declare const typed: MissingAlias;
export const marker = typed.marker;",
            ),
        ],
    );
    let entry = fixture.host.files.last().unwrap().1;
    let module = fixture.bound.symbol_of(entry).unwrap();
    let alias = fixture.bound.symbols().get(module).exports["MissingAlias"];
    for with_host in [false, true] {
        assert_eq!(type_of_variable_with_host(&fixture, "marker", with_host), "error");
    }
    // The binder does not decide target meaning. Its legacy caller continues
    // outward, while an unsupported checker target stops without substituting
    // the otherwise-compatible global TYPE symbol.
    let outer = fixture.bound.resolve_name(
        &fixture.nodes,
        &fixture.node_map,
        entry,
        "MissingAlias",
        SymbolFlags::TYPE,
    );
    assert!(outer.is_some());
    assert_ne!(outer, Some(alias));
    assert_eq!(
        fixture.bound.resolve_name_with_export_alias(
            &fixture.nodes,
            &fixture.node_map,
            entry,
            "MissingAlias",
            SymbolFlags::TYPE,
            |found, _| {
                assert_eq!(found, alias);
                None
            },
        ),
        None,
    );
}

#[test]
fn exported_require_meaning_declines_a_cyclic_indirect_target() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("globals", "interface Cycle { marker: boolean; }"),
            ("a", "import back = require('./b'); export = back;"),
            ("b", "import back = require('./a'); export = back;"),
            (
                "entry",
                "export import Cycle = require('./a');
declare const typed: Cycle;
export const marker = typed.marker;",
            ),
        ],
    );
    assert_eq!(type_of_variable_with_host(&fixture, "marker", true), "error");
}
