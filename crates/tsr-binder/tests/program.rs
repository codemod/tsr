//! Binding several files of one program into one identity space.
//!
//! The binder half of
//! [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md).
//! `tests/bind.rs` covers binding a file; this covers what changes when a second
//! file is bound into the same result — which is identity, and the global scope
//! that identity makes reachable.

use tsr_ast::{NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, FileInfo, SymbolFlags, SymbolId};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

/// One file of a bound program: what it is called, and the ids it claimed.
struct File {
    name: &'static str,
    root: tsr_ast::NodeId,
    nodes: std::ops::Range<u32>,
}

/// Parse and bind several files into one node table and one symbol store, the
/// way a program does.
fn bind_program<'a>(
    arena: &'a Arena,
    files: &[(&'static str, &'a str)],
    nodes: &mut NodeTable,
    node_map: &mut NodeMap<'a>,
) -> (BindResult<'a>, Vec<File>) {
    let mut result = BindResult::empty();
    let mut parsed = Vec::new();
    // Parsed one at a time and bound as we go, which is the order a program
    // uses and the order `bind_into` documents as the caller's responsibility.
    for (name, text) in files {
        let file = parse_into(arena, text, ParseOptions::for_file(name), nodes, node_map);
        let root = file.source_file.node_id.expect("the source file is registered");
        result = tsr_binder::bind_into(result, file.source_file, nodes, FileInfo { name, text });
        parsed.push(File { name, root, nodes: file.node_range });
    }
    (result, parsed)
}

/// Resolve `name` as a value, starting from the root of `file`.
fn resolve(
    result: &BindResult<'_>,
    nodes: &NodeTable,
    node_map: &NodeMap<'_>,
    file: &File,
    name: &str,
) -> Option<SymbolId> {
    result.resolve_name(nodes, node_map, file.root, name, SymbolFlags::VALUE)
}

#[test]
fn a_name_declared_in_one_script_file_resolves_from_another() {
    // The point of the whole issue. Before this, `a.ts` and `b.ts` were bound
    // separately and `fromA` was simply not there.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[("a.ts", "declare var fromA: number;"), ("b.ts", "declare var fromB: string;")],
        &mut nodes,
        &mut node_map,
    );

    assert!(resolve(&result, &nodes, &node_map, &files[1], "fromA").is_some(), "b.ts sees a.ts");
    assert!(
        resolve(&result, &nodes, &node_map, &files[0], "fromB").is_some(),
        "and a.ts sees b.ts"
    );
    assert!(resolve(&result, &nodes, &node_map, &files[0], "nowhere").is_none());
}

#[test]
fn a_foreign_symbols_declaration_indexes_the_right_file() {
    // **The soundness property ADR-0034 exists for.** A symbol resolved from
    // another file is only usable if its `value_declaration` names a node in the
    // shared table that really is that declaration. Under per-file numbering
    // both files' first symbols are `SymbolId(0)` and both declarations are low
    // node ids, so a wrong answer here looks entirely plausible — which is why
    // this asserts the declaration lands inside the *declaring* file's id range
    // rather than merely that it exists.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("a.ts", "declare var shared: number;"),
            ("b.ts", "declare var other: string; declare var another: string;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    let symbol = resolve(&result, &nodes, &node_map, &files[1], "shared").expect("resolves");
    let declaration = result.symbols().get(symbol).value_declaration.expect("has a declaration");
    let id = declaration.as_u32();

    assert!(
        files[0].nodes.contains(&id),
        "`shared` is declared in {} (ids {:?}), but its declaration is node {id}",
        files[0].name,
        files[0].nodes,
    );
    assert!(!files[1].nodes.contains(&id), "and not in the file that asked");
    assert_eq!(nodes.kind(declaration), SyntaxKind::VariableDeclaration);
    assert_eq!(result.symbols().get(symbol).name, "shared");
}

#[test]
fn two_files_declaring_different_names_get_different_symbol_ids() {
    // One store, so the ids are drawn from one sequence. Under per-file stores
    // both of these would be `SymbolId(0)`.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[("a.ts", "declare var one: number;"), ("b.ts", "declare var two: number;")],
        &mut nodes,
        &mut node_map,
    );

    let one = resolve(&result, &nodes, &node_map, &files[0], "one").expect("resolves");
    let two = resolve(&result, &nodes, &node_map, &files[1], "two").expect("resolves");
    assert_ne!(one, two, "two declarations, two symbols");
    assert_eq!(result.symbols().get(one).name, "one");
    assert_eq!(result.symbols().get(two).name, "two");
}

#[test]
fn a_modules_top_level_names_are_not_globals() {
    // The script/module distinction, and the reason `lib.*.d.ts` are scripts. A
    // module's top-level names are its *exports*; leaking them into the global
    // scope would make every `import`ed name globally visible, which is the
    // single most visible way to get this wrong.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("m.ts", "export const exported = 1; const notExported = 2;"),
            ("s.ts", "const own = 3;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    assert!(resolve(&result, &nodes, &node_map, &files[1], "exported").is_none());
    assert!(resolve(&result, &nodes, &node_map, &files[1], "notExported").is_none());
    // …while the script file's own name is global, so the test cannot pass by
    // globals simply being empty.
    assert!(resolve(&result, &nodes, &node_map, &files[0], "own").is_some());
}

#[test]
fn a_local_shadows_a_global() {
    // Globals are consulted only after the lexical walk has run off the top of
    // the file, so the nearer declaration wins. Checking the *identity* of what
    // resolves, not merely that something did.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[("a.ts", "declare var name: number;"), ("b.ts", "declare var name: string;")],
        &mut nodes,
        &mut node_map,
    );

    let from_b = resolve(&result, &nodes, &node_map, &files[1], "name").expect("resolves");
    let declaration = result.symbols().get(from_b).value_declaration.expect("has one");
    assert!(
        files[1].nodes.contains(&declaration.as_u32()),
        "b.ts's own `name` wins over the global from a.ts",
    );
}

#[test]
fn a_repeated_global_merges_into_one_symbol_carrying_both_declarations() {
    // The divergence `merge_globals` documents, pinned so it cannot change
    // silently. Upstream's `mergeGlobalSymbol` would produce one symbol carrying
    // both declarations; this keeps the first and drops the second.
    //
    // Asserted from a *third* file, so neither declaration is lexically in
    // scope and the answer comes from the global table alone.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("first.ts", "declare var repeated: number;"),
            ("second.ts", "declare var repeated: string;"),
            ("third.ts", "const unrelated = 1;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    let symbol = resolve(&result, &nodes, &node_map, &files[2], "repeated").expect("resolves");
    let declaration = result.symbols().get(symbol).value_declaration.expect("has one");
    // `SetValueDeclaration` keeps the FIRST — upstream only replaces when the
    // target has none (`checker.go:14176`), so this half is unchanged by
    // merging and is what distinguishes "merged" from "second one won".
    assert!(
        files[0].nodes.contains(&declaration.as_u32()),
        "the first declaration is still the value declaration after merging",
    );
    // This assertion read `1` until merging landed, with a comment saying
    // "merging would make this 2". It does.
    assert_eq!(
        result.symbols().get(symbol).declarations.len(),
        2,
        "`mergeSymbol` unions the declarations (`checker.go:14178`)",
    );
    // And both declarations are the ones that were written, one per file —
    // not the first counted twice, which is what a merge that appended the
    // wrong side would produce.
    let declarations = &result.symbols().get(symbol).declarations;
    assert!(
        declarations.iter().any(|d| files[0].nodes.contains(&d.as_u32()))
            && declarations.iter().any(|d| files[1].nodes.contains(&d.as_u32())),
        "one declaration from each file",
    );
}

#[test]
fn a_file_bound_alone_resolves_exactly_as_it_always_did() {
    // The guard on everything above: `bind` is `bind_into` over an empty seed,
    // so a consumer that binds one file must see no behaviour change.
    //
    // **This test first asserted `globals().is_empty()` and the implementation
    // was right.** A single *script* file is a program of one, and upstream's
    // `initializeChecker` makes its top-level names globals exactly as it would
    // for one file among many. Nothing observable changes, and the reason is
    // worth stating: for one file the globals are a subset of the root's own
    // locals, which the lexical walk reaches through `lookup_local(root)`
    // *before* the fall-through is tried. So the fall-through can only find
    // names the walk already found.
    let arena = Arena::new();
    // The function body declares a name of its own, so a `merge_globals` that
    // walked every scope instead of the file root would put `nested` in globals
    // and the subset assertion below would catch it. Without a nested
    // declaration this fixture cannot see that mutation — checked.
    let source =
        "declare var only: number; function f() { const nested = 1; return only + nested; }";
    let parsed = tsr_parser::parse(&arena, source);
    let result = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "a.ts", text: source },
    );

    let root = parsed.source_file.node_id.expect("registered");
    let resolve = |name: &str| {
        result.resolve_name(&parsed.nodes, &parsed.node_map, root, name, SymbolFlags::VALUE)
    };
    assert!(resolve("only").is_some(), "its own names still resolve lexically");
    assert!(resolve("absent").is_none(), "and an absent one still does not");

    // Every global is a top-level name of this very file, so the fall-through
    // adds no reachable name. That is the property that makes the change
    // invisible to a single-file consumer, and it is what is actually asserted.
    let root_locals = result.locals(root).expect("a script file has locals");
    for name in result.globals().keys() {
        assert!(root_locals.contains_key(name), "global {name} is not a local of the only file");
    }
}

#[test]
fn a_module_bound_alone_has_no_globals_at_all() {
    // The other half, and the one that shows `merge_globals` is testing
    // something rather than merging unconditionally.
    let arena = Arena::new();
    let source = "export const exported = 1;";
    let parsed = tsr_parser::parse(&arena, source);
    let result = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "m.ts", text: source },
    );
    assert!(result.globals().is_empty(), "a module contributes no globals");
}

#[test]
fn binding_a_second_file_does_not_disturb_the_first_files_symbols() {
    // `resuming` resizes the dense per-node columns rather than reallocating
    // them. If it reallocated, every symbol and flow node the first file
    // recorded would be dropped — and the first file's own resolution would
    // silently start failing.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("a.ts", "declare var first: number; function inA() { return first; }"),
            ("b.ts", "declare var second: string;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    // Resolved from a nested scope inside the *first* file, so the lookup walks
    // that file's own containers — the ones `node_symbols` and `locals` hold.
    let in_a = files[0]
        .nodes
        .clone()
        .find(|id| nodes.kind(tsr_ast::NodeId::new(*id)) == SyntaxKind::FunctionDeclaration)
        .map(tsr_ast::NodeId::new)
        .expect("the fixture has a function");
    assert!(
        result.resolve_name(&nodes, &node_map, in_a, "first", SymbolFlags::VALUE).is_some(),
        "the first file's scopes survived the second file being bound",
    );
    assert!(result.symbol_of(in_a).is_some(), "and so did its node-to-symbol column");
}

/// The bundled lib file, or `None` when the submodule is not checked out.
/// See docs/conventions.md — a submodule-dependent test skips.
fn bundled_lib(name: &str) -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)?
        .join("vendor/typescript-go/internal/bundled/libs")
        .join(name);
    std::fs::read_to_string(path).ok()
}

#[test]
fn a_user_file_resolves_the_real_libs_globals() {
    // The end of the chain, against the actual shipped `lib.es5.d.ts` rather
    // than a fixture that declares `interface Array<T> {}`. A fixture would
    // prove the machinery works on something shaped like a lib file; this
    // proves it works on the file whose absence `bd tsr-9or.1` measured at
    // 18,387 gap lines.
    //
    // `lib.es5.d.ts` is a script, not a module — it has no top-level `import`
    // or `export` — which is exactly why its declarations are globals.
    let Some(lib) = bundled_lib("lib.es5.d.ts") else {
        return; // the submodule is not checked out
    };
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[("lib.es5.d.ts", lib.as_str()), ("user.ts", "const x = 1;")],
        &mut nodes,
        &mut node_map,
    );
    let user = &files[1];

    // Type-position names, which is where the measurement put 3,210 lines.
    for name in ["Array", "Object", "String", "Number", "Boolean", "Function"] {
        let symbol = result
            .resolve_name(&nodes, &node_map, user.root, name, SymbolFlags::TYPE)
            .unwrap_or_else(|| panic!("`{name}` should resolve from a user file"));
        assert_eq!(result.symbols().get(symbol).name, name);
        let declaration = result.symbols().get(symbol).declarations.first().copied();
        let declaration = declaration.unwrap_or_else(|| panic!("`{name}` has a declaration"));
        assert!(
            files[0].nodes.contains(&declaration.as_u32()),
            "`{name}` must resolve to the lib file's declaration, not something in user.ts",
        );
    }

    // Value-position names, where the measurement put 3,126.
    for name in ["parseInt", "NaN", "Infinity", "JSON", "Math"] {
        assert!(
            result.resolve_name(&nodes, &node_map, user.root, name, SymbolFlags::VALUE).is_some(),
            "`{name}` should resolve from a user file",
        );
    }

    // And the negatives, so the test cannot pass by resolving everything.
    //
    // **This first named `Promise` and the implementation was right.**
    // `lib.es5.d.ts` declares `interface Promise<T>` — it is
    // `PromiseConstructor` and `declare var Promise` that live in
    // `lib.es2015.promise.d.ts`. So `Promise` in *type* position resolves from
    // ES5 alone, which is worth knowing: the measurement counted 640 `Promise`
    // lines and they are reachable without the ES2015 libs. `Map`, `Set` and
    // `Iterable` really are ES2015-only — checked against the shipped files.
    for name in ["Map", "Set", "WeakMap", "Iterable"] {
        assert!(
            result.resolve_name(&nodes, &node_map, user.root, name, SymbolFlags::TYPE).is_none(),
            "`{name}` is declared in an ES2015 lib this program did not load",
        );
    }
    assert!(
        result.resolve_name(&nodes, &node_map, user.root, "NotAThing", SymbolFlags::TYPE).is_none(),
    );
}

#[test]
fn two_declarations_of_a_global_interface_merge_their_members() {
    // The case the whole change exists for: `interface Array<T>` is declared in
    // 8 bundled lib files and `String` in 11, and first-in-wins kept one. With
    // merging, `Math.trunc` (declared only in `lib.es2015.core.d.ts`) resolves
    // through the `Math` symbol whose base declaration is in `lib.es5.d.ts`.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("first.ts", "interface I { a: number; }"),
            ("second.ts", "interface I { b: string; }"),
            ("third.ts", "const unrelated = 1;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    let symbol = resolve(&result, &nodes, &node_map, &files[2], "I").expect("resolves");
    let data = result.symbols().get(symbol);
    assert_eq!(data.declarations.len(), 2, "both interface declarations");
    let mut members: Vec<&str> = data.members.keys().copied().collect();
    members.sort_unstable();
    assert_eq!(members, ["a", "b"], "`mergeSymbolTable` (`checker.go:14109`)");
}

#[test]
fn a_member_declared_in_both_halves_merges_rather_than_taking_the_first() {
    // **Why the member merge recurses.** Two declarations of a lib interface
    // routinely split a member's *overloads* between them — `Array.from` has one
    // in `lib.es2015.core.d.ts` and another in `lib.es2015.iterable.d.ts`. If
    // the member table took first-in-wins, `m` would carry one declaration and
    // print a single signature where upstream prints the overload set: a wrong
    // answer, not a missing one.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("first.ts", "interface I { m(): void; }"),
            ("second.ts", "interface I { m(x: string): void; }"),
            ("third.ts", "const unrelated = 1;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    let symbol = resolve(&result, &nodes, &node_map, &files[2], "I").expect("resolves");
    let member = *result.symbols().get(symbol).members.get("m").expect("`m` is a member");
    assert_eq!(
        result.symbols().get(member).declarations.len(),
        2,
        "the member symbol carries BOTH signatures, so the overload set is complete",
    );
}

#[test]
fn a_conflicting_redeclaration_does_not_merge() {
    // `getExcludedSymbolFlags` (`checker.go:14147`) gates the merge, and
    // `SymbolFlags::excludes` is this port's spelling of it: a `let` may not be
    // redeclared at all in value space, so these two stay unmerged. Upstream
    // reports a diagnostic here and this port has none (`bd tsr-5e7.6`), so the
    // observable part is only that the tables were not joined.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let (result, files) = bind_program(
        &arena,
        &[
            ("first.ts", "declare let conflicting: number;"),
            ("second.ts", "declare let conflicting: string;"),
            ("third.ts", "const unrelated = 1;"),
        ],
        &mut nodes,
        &mut node_map,
    );

    let symbol = resolve(&result, &nodes, &node_map, &files[2], "conflicting").expect("resolves");
    assert_eq!(
        result.symbols().get(symbol).declarations.len(),
        1,
        "a conflicting redeclaration is left alone, not merged",
    );
}
