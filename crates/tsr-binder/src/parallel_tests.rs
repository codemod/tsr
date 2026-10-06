use super::*;
use tsr_core::Arena;

fn assert_equivalent(actual: &BindResult<'_>, expected: &BindResult<'_>) {
    assert_eq!(actual.max_depth, expected.max_depth);
    assert_eq!(actual.symbols.len(), expected.symbols.len());
    for ((id, a), (expected_id, e)) in actual.symbols.iter().zip(expected.symbols.iter()) {
        assert_eq!(id, expected_id);
        assert_eq!(
            (a.flags, a.name, &a.declarations, a.value_declaration, a.parent, a.export_symbol),
            (e.flags, e.name, &e.declarations, e.value_declaration, e.parent, e.export_symbol),
            "symbol {id:?}"
        );
        for (a, e) in [(&a.members, &e.members), (&a.exports, &e.exports)] {
            assert_eq!(a.is_present(), e.is_present());
            assert_eq!(
                a.iter().collect::<FxHashMap<_, _>>(),
                e.iter().collect::<FxHashMap<_, _>>()
            );
        }
    }
    assert_eq!(actual.node_symbols, expected.node_symbols);
    assert_eq!(actual.locals, expected.locals);
    assert_eq!(actual.global_exports, expected.global_exports);
    assert_eq!(actual.globals, expected.globals);
    assert_eq!(actual.merged, expected.merged);
    assert_eq!(actual.merge_conflicts, expected.merge_conflicts);
    assert_eq!(
        format!("{:?}", actual.module_augmentations),
        format!("{:?}", expected.module_augmentations)
    );
    assert_eq!(
        actual.pattern_ambient_module_augmentations,
        expected.pattern_ambient_module_augmentations
    );
    assert_eq!(actual.undefined_symbol, expected.undefined_symbol);
    assert_eq!(actual.computed_names, expected.computed_names);
    assert_eq!(format!("{:?}", actual.diagnostics), format!("{:?}", expected.diagnostics));
    assert_eq!(actual.flow.graph_image(), expected.flow.graph_image());
    assert_eq!(actual.node_flow, expected.node_flow);
    assert_eq!(actual.facts, expected.facts);
    assert_eq!(actual.end_flow, expected.end_flow);
    assert_eq!(actual.return_flow, expected.return_flow);
    assert_eq!(actual.fallthrough_flow, expected.fallthrough_flow);
}

#[test]
fn private_binds_publish_identically_in_file_order() {
    let inputs = [
        (
            "a.d.ts",
            "interface Shared { a: string; } namespace N { export const first: 1; } declare module 'mod' { export const x: number; } function seed(x: string | number) { for (;;) { try { switch (x) { case 'a': continue; default: break; } } finally { x = 'a'; } break; } return x; }",
        ),
        (
            "b.ts",
            "interface Shared { b: number; } namespace N { export const second = 2; } const duplicate = 1; function branch(x: string | number) { label: for (let i = 0; i < 3; i++) { try { switch (x) { case 'a': continue label; default: if (i) break; } } finally { x = 'b'; } } return x; }",
        ),
        (
            "c.ts",
            "const duplicate = 2; import { x } from 'mod'; export { x }; declare global { interface Shared { c: boolean; } } declare module 'mod' { export const y: string; }",
        ),
        (
            "d.js",
            "/** @typedef {{v: string}} Value */ function make() {} make.x = 1; module.exports = make; exports.p = 2; const obj = { [-(0x10)]: 1, [+0b10]: 2, [0x10]: 3 };",
        ),
        ("e.d.ts", "export as namespace U; export const q: number;"),
        ("f.d.ts", "export as namespace U; export const r: string;"),
        ("empty.ts", ""),
    ];
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let files: Vec<_> = inputs
        .iter()
        .map(|(name, text)| {
            let text = &*arena.alloc_str(text);
            let parsed = tsr_parser::parse_into(
                &arena,
                text,
                tsr_parser::ParseOptions::for_file(name),
                &mut nodes,
                &mut map,
            );
            (FileInfo { name, text }, parsed)
        })
        .collect();
    let names = PreparedNames::new(&arena, &map);
    let mut serial = BindResult::empty();
    for (info, parsed) in &files {
        let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
        serial = bind_into_with_jsdoc(serial, &arena, parsed.source_file, &nodes, *info, &jsdoc);
    }
    let completed = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .iter()
            .rev()
            .map(|(info, parsed)| {
                let names = &names;
                let nodes = &nodes;
                scope.spawn(move || {
                    if parsed.source_file.statements.iter().any(|statement| {
                        matches!(statement, tsr_ast::Statement::NamespaceExportDeclaration(_))
                    }) {
                        return None;
                    }
                    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
                    Some(bind_file(
                        names,
                        nodes,
                        parsed.source_file,
                        *info,
                        &jsdoc,
                        parsed.node_range.clone(),
                    ))
                })
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
    });
    let mut parallel = BindResult::empty();
    let identity = parallel.symbols.identity().clone();
    for (local, (info, parsed)) in completed.into_iter().rev().zip(&files) {
        parallel = if let Some(local) = local {
            parallel.publish_file(&arena, &nodes, local)
        } else {
            let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
            bind_into_with_jsdoc(parallel, &arena, parsed.source_file, &nodes, *info, &jsdoc)
        };
        assert_eq!(parallel.symbols.identity(), &identity);
    }
    assert_equivalent(&parallel, &serial);
}

#[test]
#[should_panic(expected = "file bind belongs to a different node table")]
fn publication_rejects_a_foreign_ast_owner() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let text = arena.alloc_str("export const x = 1;");
    let parsed = tsr_parser::parse_into(
        &arena,
        text,
        tsr_parser::ParseOptions::default(),
        &mut nodes,
        &mut map,
    );
    let names = PreparedNames::new(&arena, &map);
    let local = bind_file(
        &names,
        &nodes,
        parsed.source_file,
        FileInfo { name: "a.ts", text },
        &[],
        parsed.node_range,
    );
    let foreign = NodeTable::new();
    let _ = BindResult::empty().publish_file(&arena, &foreign, local);
}
