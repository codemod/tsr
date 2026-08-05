//! The module object as a **lookup surface** and never as an answer.
//!
//! `ns.foo` resolves through `import * as ns from "./m"` and
//! `import ns = require("./m")`; `ns` itself keeps gapping. Upstream would give
//! `ns` a module type and run `getPropertyOfType` (`checker.go:18887`) over it.
//! `Checker::module_member_type` does the lookup and not the type, so the
//! receiver's own line is `errorType` **by construction** rather than by a
//! guard — see that function for the 577-against-1,192 measurement that decided
//! it.
//!
//! # The harness is two files, and the lib is absent
//!
//! Same shape as `tests/cross_file_aliases.rs`: `parse_into` + `bind_into` into
//! one identity space, with a `ModuleHost` spelled out in the fixture. No
//! fixture here names `Array`, `Promise` or `number[]`; [`no_lib_control`] pins
//! that by construction, so an unexpected gap elsewhere is distinguishable from
//! the missing standard library without re-deriving it.
//!
//! # The mutations
//!
//! Each mutation was applied one at a time, confirmed present with `grep -c`
//! returning exactly 1 **before** the test ran, and reverted after. The
//! "reddens" column is measured, not expected.
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | `Node::NamespaceImport(_) => return None` in `module_symbol_of_namespace_alias` | 4: [`a_namespace_import_resolves_a_member`], [`the_namespace_alias_itself_still_gaps_with_the_host_in_place`], [`a_member_of_a_namespace_import_that_is_itself_exported_resolves`], [`mutually_initialised_modules_gap_rather_than_hang`] |
//! | 2 | `Node::ImportEqualsDeclaration(_) => return None` in the same | 2: [`an_import_equals_require_resolves_a_member`], [`the_import_equals_alias_itself_still_gaps_with_the_host_in_place`] |
//! | 3 | drop the `export =` gate (`.then_some` → `Some`) | **nothing** — see below |
//! | 4 | `get_export_of_module(…)?` → `.unwrap_or(module_symbol)` | [`a_name_the_module_does_not_export_is_a_gap`] **only** |
//! | 5 | remove the `ALIAS` test in `module_member_type` | **nothing** — see below |
//!
//! Mutations 1 and 2 partition the two positive forms and their sets are
//! **disjoint**: the namespace-import tests cannot be reddened by the
//! import-equals mutation or the reverse. That is what makes each one evidence
//! about its own form rather than about the arm being present at all. Mutation 4
//! is disjoint from both.
//!
//! Note that the two receiver tests go red under 1 and 2 on their *first*
//! assertion — the member no longer resolves — and not on the gap assertion,
//! which is `error` either way. The gap half of those tests is pinned by
//! construction, not by a mutation, which is the point of choosing design (b):
//! there is no arm to delete that would make the receiver print.
//!
//! # Two mutations were written, applied, and did **not** bite
//!
//! Recorded rather than quietly dropped, because a guard no mutation can make
//! observable is decoration (`docs/conventions.md`).
//!
//! - **Mutation 5, the `ALIAS` test.** Removing it turns no test in the
//!   workspace red, because `module_symbol_of_namespace_alias` immediately asks
//!   `declaration_of_alias_symbol` for a declaration of one of six alias kinds,
//!   and a non-alias symbol has none. It stays because the two are equal only by
//!   accident: `declaration_of_alias_symbol` deliberately lists kinds
//!   `resolve_alias` declines, so a symbol *merged* with a namespace import
//!   would reach the module lookup without it.
//! - **Mutation 3, the `export =` gate.** No fixture can discriminate it, and
//!   the reason is structural rather than a missing fixture: TypeScript rejects
//!   *"An export assignment cannot be used in a module with other exported
//!   elements"*, so a module carrying `export=` has **nothing else in its
//!   `exports` table**. Gated and ungated therefore both miss.
//!   [`a_module_with_export_equals_is_a_gap`] pins the answer without pinning
//!   the gate. The named edit that makes the gate bite is an `export =` arm that
//!   looks members up on `getTypeOfSymbol(X)` (`checker.go:15556`'s consumer) —
//!   at that point the gate stops being a filter and becomes the dispatch
//!   between two arms.
//!
//! # The cycle guard is not this arm's, and it fires
//!
//! [`mutually_initialised_modules_gap_rather_than_hang`] is the cycle this arm
//! makes reachable — `a.ts`'s `x` initialised from `B.y` while `b.ts`'s `y` is
//! initialised from `A.x`. It terminates on
//! `get_type_of_variable_or_parameter_or_property_worker`'s existing
//! `resolutions.push` frame (`crates/tsr-checker/src/symbols.rs`), which
//! predates this work.
//!
//! **Mutation 6**, measured: `if !self.resolutions.push(symbol,
//! PropertyName::Type) && false {` — keep the frame, ignore its answer. The test
//! then **overflows the stack and aborts** (`signal: 6, SIGABRT`), not merely
//! fails. Recorded as "overflows" rather than "hangs" because that is what it
//! did.
//!
//! So no guard was added here, and the existing one is observable through this
//! arm rather than assumed to be. `get_symbol_flags`'s `seenSymbols`
//! (`checker.go:16368`) — the guard `tests/cross_file_aliases.rs` pins — is
//! **not** what closes this cycle: the loop here runs through variable
//! initialisers, not through an alias chain. Two guards, two cycles, and reading
//! alone would have named the wrong one, which is exactly the failure
//! `docs/conventions.md` records under *"faithfulness to upstream is not evidence
//! that a guard is load-bearing here"*.

use tsr_ast::{Expression, Node, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, SymbolFlags};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

/// A [`ModuleHost`] over a fixed set of fixture files; `"./m"` names fixture
/// `m`, and an unmatched specifier answers `None`.
struct Fixtures {
    files: Vec<(&'static str, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, _)| *fixture == name).map(|&(_, id)| id)
    }
}

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

/// Parse and bind several files into **one** identity space, in load order.
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
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((*name, source, file.source_file));
    }

    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound, host }
}

/// The printed type of the **last** property access in the program.
///
/// This is the position the `.types` baseline records twice — `a.b` renders both
/// the access and the member name — and the population `bd tsr-6ph` item 4
/// measures at 1,618 lines.
///
/// `with_host` is the negative control: the *same* fixture with no host must
/// gap, which is what makes a green assertion evidence that this arm produced
/// the answer rather than something the port already did.
fn type_of_last_access(fixture: &Fixture<'_>, with_host: bool) -> String {
    let host: Option<&dyn ModuleHost> = if with_host { Some(&fixture.host) } else { None };
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let mut found = None;
    for index in 0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32") {
        let id = NodeId::new(index);
        if fixture.nodes.kind(id) != SyntaxKind::PropertyAccessExpression {
            continue;
        }
        let Some(Node::PropertyAccessExpression(node)) = fixture.node_map.get(id) else { continue };
        found = Some(node);
    }
    let node = found.expect("the fixture holds a property access");
    let id = checker.check_expression(Expression::PropertyAccessExpression(node));
    checker.type_to_string(id)
}

/// The printed type of the **receiver** — the alias's own reference position.
///
/// The sharpest condition on this slice: it must stay `error` with the host in
/// place, because a module type would print the file path where upstream prints
/// `typeof ns`.
fn type_of_receiver(fixture: &Fixture<'_>, name: &str) -> String {
    let host: Option<&dyn ModuleHost> = Some(&fixture.host);
    let mut checker =
        Checker::with_module_host(&fixture.bound, &fixture.nodes, &fixture.node_map, host);
    let mut found = None;
    for index in 0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32") {
        let id = NodeId::new(index);
        if !matches!(
            fixture.nodes.kind(id),
            SyntaxKind::NamespaceImport | SyntaxKind::ImportEqualsDeclaration
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
            "a namespace import or import-equals binds an alias symbol"
        );
        found = Some(symbol);
    }
    let symbol = found.unwrap_or_else(|| panic!("no namespace-shaped alias named `{name}`"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

// ---------------------------------------------------------------------------
// The two positive arms, disjoint under mutations 1 and 2.
// ---------------------------------------------------------------------------

/// `import * as ns from "./m"` — `getTargetOfNamespaceImport` (`checker.go:14628`).
#[test]
fn a_namespace_import_resolves_a_member() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export var foo: string = \"a\";"),
            ("main", "import * as ns from \"./m\";\nns.foo;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "string");
    // The negative control: the arm is what answered, not something already here.
    assert_eq!(type_of_last_access(&fixture, false), "error");
}

/// `import ns = require("./m")` — `getTargetOfImportEqualsDeclaration`'s
/// external-module-reference case (`checker.go:14439`).
#[test]
fn an_import_equals_require_resolves_a_member() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "export var foo: number = 1;"), ("main", "import ns = require(\"./m\");\nns.foo;")],
    );
    assert_eq!(type_of_last_access(&fixture, true), "number");
    assert_eq!(type_of_last_access(&fixture, false), "error");
}

// ---------------------------------------------------------------------------
// The condition the whole slice rests on, and it is pinned by construction.
// ---------------------------------------------------------------------------

/// **The receiver must not move.** `ns` is `error` *with* the host, because no
/// module type is ever created.
///
/// Not red under mutations 1–4: it is pinned by the absence of a `MODULE` arm in
/// `get_type_of_symbol` and by `resolve_alias` answering `None` for a namespace
/// import. The mutation it *is* red under is the rejected design — giving
/// `get_type_of_alias` the module symbol's type — which is exactly the change
/// this test exists to forbid.
#[test]
fn the_namespace_alias_itself_still_gaps_with_the_host_in_place() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export var foo: string = \"a\";"),
            ("main", "import * as ns from \"./m\";\nns.foo;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "string", "the member resolves");
    assert_eq!(type_of_receiver(&fixture, "ns"), "error", "and the module object is never named");
}

/// The same, for the import-equals form, whose alias row is 433 lines.
#[test]
fn the_import_equals_alias_itself_still_gaps_with_the_host_in_place() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "export var foo: number = 1;"), ("main", "import ns = require(\"./m\");\nns.foo;")],
    );
    assert_eq!(type_of_last_access(&fixture, true), "number", "the member resolves");
    assert_eq!(type_of_receiver(&fixture, "ns"), "error", "and the module object is never named");
}

// ---------------------------------------------------------------------------
// The gaps, each red under its own mutation.
// ---------------------------------------------------------------------------

/// `export = X` gaps: the module *is* `X` downstream and its members are
/// `getTypeOfSymbol(X)`'s, which this port does not have. Mutation 3.
#[test]
fn a_module_with_export_equals_is_a_gap() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "declare var foo: string;\nexport = foo;"),
            ("main", "import * as ns from \"./m\";\nns.foo;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "error");
}

/// A name the module does not export is a **miss**, not the module object.
/// Mutation 4.
#[test]
fn a_name_the_module_does_not_export_is_a_gap() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export var foo: string = \"a\";"),
            ("main", "import * as ns from \"./m\";\nns.bar;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "error");
}

/// A specifier the host cannot resolve gaps, exactly as it did before this arm.
#[test]
fn an_unresolved_specifier_is_a_gap() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "export var foo: string = \"a\";"),
            ("main", "import * as ns from \"./gone\";\nns.foo;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "error");
}

/// An ordinary local receiver is untouched — the arm falls through for anything
/// that is not a namespace-shaped alias.
#[test]
fn a_local_object_receiver_takes_the_ordinary_path() {
    let arena = Arena::new();
    let fixture =
        program(&arena, &[("main", "class C { foo: string = \"a\"; }\ndeclare var c: C;\nc.foo;")]);
    assert_eq!(type_of_last_access(&fixture, true), "string");
}

// ---------------------------------------------------------------------------
// Cycles, and the control that is not about this arm.
// ---------------------------------------------------------------------------

/// Two modules whose exported variables are initialised from each other's
/// members. This arm is what makes the cycle reachable; the frame that closes it
/// is `get_type_of_variable_or_parameter_or_property_worker`'s and predates it.
///
/// **The answer is `any`, and it is upstream's rather than this arm's.**
/// `reportCircularityError` (`checker.go:18822`) returns `errorType` only when
/// the declaration carries a type annotation; a circular *initialiser* with no
/// annotation is `anyType` and a diagnostic. `Checker::report_circularity_error`
/// is that function, and it predates this work. Asserted here rather than
/// silently avoided, because "`errorType`, never `anyType`" is a rule about the
/// answers *this* arm invents and not about a computed upstream answer arrived
/// at through it.
#[test]
fn mutually_initialised_modules_gap_rather_than_hang() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("a", "import * as B from \"./b\";\nexport var x = B.y;"),
            ("b", "import * as A from \"./a\";\nexport var y = A.x;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "any");
}

/// A member whose own type comes from another module resolves — one hop of the
/// chain, so the answer is not an accident of same-file resolution.
#[test]
fn a_member_of_a_namespace_import_that_is_itself_exported_resolves() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("inner", "export var deep: string = \"a\";"),
            ("m", "import * as i from \"./inner\";\nexport var foo = i.deep;"),
            ("main", "import * as ns from \"./m\";\nns.foo;"),
        ],
    );
    assert_eq!(type_of_last_access(&fixture, true), "string");
}

/// **The lib control.** The harness loads no lib files, so `number[]` is a gap
/// *here*. Pinned by construction so that an unexpected gap in this file is
/// distinguishable from the missing standard library without re-deriving it.
#[test]
fn no_lib_control() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[(
            "main",
            "class C { foo: string = \"a\"; }\nvar x: number[];\ndeclare var c: C;\nc.foo;",
        )],
    );
    assert_eq!(type_of_last_access(&fixture, true), "string", "the class member resolves");
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let mut declared = None;
    for index in 0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32") {
        if let Some(symbol) = fixture.bound.lookup_local(NodeId::new(index), "x") {
            declared = Some(symbol);
            break;
        }
    }
    let symbol = declared.expect("`x` is declared at the top level of the only file");
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string(id), "error", "`number[]` has no lib to resolve against");
}
