//! Native `mergeModuleAugmentation` resolves external import-equals chains.
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, SymbolId};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

fn augmented<'a>(arena: &'a Arena, bridge: &'a str) -> (BindResult<'a>, SymbolId, SymbolId) {
    let sources = [
        (
            "core.d.ts",
            "declare function core(): core.Item; declare namespace core { interface Item { original: number; } } export = core;",
        ),
        ("bridge.d.ts", bridge),
        ("other.d.ts", "export interface Item { distinct: boolean; }"),
        (
            "main.ts",
            "import bridge = require('bridge'); declare module 'bridge' { interface Item { added: string; } }",
        ),
    ];
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut bound = BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in sources {
        let parsed =
            parse_into(arena, text, ParseOptions::for_file(name), &mut nodes, &mut node_map);
        assert!(parsed.diagnostics.is_empty(), "{name}: {:?}", parsed.diagnostics);
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            arena,
            parsed.source_file,
            &nodes,
            FileInfo { name, text },
        );
    }
    let core = bound.lookup_local(roots[0], "core").unwrap();
    let other = bound.symbol_of(roots[2]).unwrap();
    let bound =
        bound.merge_module_augmentations(arena, &nodes, &node_map, |bound, _, name, _, _| {
            let index = match name {
                "core" => 0,
                "bridge" => 1,
                "other" => 2,
                _ => return None,
            };
            bound.symbol_of(roots[index])
        });
    (bound, core, other)
}

#[test]
fn external_import_equals_merges_the_actual_export_equals_namespace() {
    let arena = Arena::new();
    let (bound, core, other) = augmented(&arena, "import core = require('core'); export = core;");
    let item = bound.symbols().get(bound.merged_symbol(core)).exports["Item"];
    let item = bound.symbols().get(bound.merged_symbol(item));
    assert!(item.members.contains_key("original"));
    assert!(item.members.contains_key("added"));
    let distinct = bound.symbols().get(other).exports["Item"];
    let distinct = bound.symbols().get(bound.merged_symbol(distinct));
    assert!(distinct.members.contains_key("distinct"));
    assert!(!distinct.members.contains_key("added"));
}

#[test]
fn circular_export_equals_does_not_publish_an_augmentation_target() {
    let arena = Arena::new();
    let (bound, core, _) = augmented(&arena, "import bridge = require('bridge'); export = bridge;");
    let item = bound.symbols().get(bound.merged_symbol(core)).exports["Item"];
    assert!(!bound.symbols().get(bound.merged_symbol(item)).members.contains_key("added"));
}
