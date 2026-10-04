//! Dependent on semantic `MODULE_EXPORTS` publication. Native control:
//! structuralCommonjsWrapper.ts, tsgo 5b1047d. The wrapper's `TypeId` and binder
//! symbol stay stable; only the referenced export value is lazily serialized.

use tsr_ast::{NodeFlags, push_children};
use tsr_binder::SymbolFlags;
use tsr_checker::{Checker, types::TypeData};
use tsr_core::Arena;

#[test]
fn completed_commonjs_wrapper_preserves_identity_in_cold_reversed_and_warm_views() {
    let source = "exports.item = 1; const before = module;
                  function inside() { const nested = module; return nested; }
                  const after = module;";
    for reverse in [false, true] {
        let arena = Arena::new();
        let mut parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/named.js", text: source },
        );
        let symbol = *bound.locals(root).unwrap().get("module").unwrap();
        assert!(bound.symbols().get(symbol).flags.contains(SymbolFlags::MODULE_EXPORTS));
        let export = *bound.symbols().get(symbol).members.get("exports").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ty = checker.get_type_of_symbol(symbol);
        assert!(
            matches!(checker.type_of(ty).data, TypeData::Anonymous { symbol: owner, signature: false, .. } if owner == symbol)
        );
        let data = checker.type_of(ty).data.clone();
        let mut nodes = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
        let mut sites = Vec::new();
        while let Some(node) = nodes.pop() {
            if let tsr_ast::Node::Identifier(node) = node
                && matches!(node.text, "before" | "nested" | "after")
            {
                sites.push(node.node_id.unwrap());
            }
            push_children(node, &mut nodes);
        }
        // Three declarations plus the return's nested reference are distinct
        // native enclosing declarations, not one spelling-keyed print site.
        assert_eq!(sites.len(), 4);
        if reverse {
            sites.reverse();
        }
        let mut export_identity = None;
        for _ in 0..4 {
            for &site in &sites {
                assert_eq!(
                    checker.type_to_string_at(ty, site).as_deref(),
                    Some("{ exports: typeof import(\"./named\"); }")
                );
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
                let value = checker.get_type_of_symbol(export);
                assert_eq!(*export_identity.get_or_insert(value), value);
                assert_eq!(bound.symbols().get(symbol).members.get("exports"), Some(&export));
            }
            sites.reverse();
        }
    }
}

#[test]
fn export_equals_class_uses_module_container_except_at_its_own_name_scope() {
    // Native assignmentCommonJsAliases: class/export access/assignment uses
    // typeof import("./klass"); the class's own written name stays typeof Bound.
    let source = "module.exports = class Bound { field = 9; method() { return Bound; } };
                  const before = module.exports; module.exports;
                  class Plain {} const after = module.exports;";
    for reverse in [false, true] {
        let arena = Arena::new();
        let mut parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/klass.js", text: source },
        );
        let mut nodes = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
        let mut sites = Vec::new();
        let mut symbol = None;
        while let Some(node) = nodes.pop() {
            match node {
                tsr_ast::Node::ClassExpression(node) => {
                    let declaration = node.node_id.unwrap();
                    symbol = bound.symbol_of(declaration);
                    sites.push((declaration, "typeof import(\"./klass\")"));
                }
                tsr_ast::Node::Identifier(node) if node.text == "Bound" => {
                    sites.push((node.node_id.unwrap(), "typeof Bound"));
                }
                tsr_ast::Node::Identifier(node) if matches!(node.text, "before" | "after") => {
                    sites.push((node.node_id.unwrap(), "typeof import(\"./klass\")"));
                }
                _ => {}
            }
            push_children(node, &mut nodes);
        }
        assert_eq!(sites.len(), 5);
        let symbol = symbol.unwrap();
        let plain = bound.lookup_local(root, "Plain").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ty = checker.get_type_of_symbol(symbol);
        let data = checker.type_of(ty).data.clone();
        assert!(matches!(data, TypeData::Anonymous { symbol: owner, .. } if owner == symbol));
        let unrelated = checker.get_type_of_symbol(plain);
        if reverse {
            sites.reverse();
        }
        for _ in 0..4 {
            for &(site, expected) in &sites {
                assert_eq!(checker.type_to_string_at(ty, site).as_deref(), Some(expected));
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
                // Same file and CLASS flags do not certify an export= container.
                assert_eq!(
                    checker.type_to_string_at(unrelated, site).as_deref(),
                    Some("typeof Plain")
                );
            }
            sites.reverse();
        }
    }
}
