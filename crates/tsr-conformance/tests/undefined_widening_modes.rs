//! Active undefined-widening identity, pinned against native 5b1047d1.
use tsr_conformance::{TestCase, types_producer};

#[test]
fn real_program_options_select_the_slot_without_changing_global_or_shadowed_bindings() {
    for (directives, strict) in [
        ("", true),
        ("// @strict: true\n", true),
        ("// @strict: false\n", false),
        ("// @strictNullChecks: true\n", true),
        ("// @strictNullChecks: false\n", false),
        ("// @strict: true\n// @strictNullChecks: false\n", false),
        ("// @strict: false\n// @strictNullChecks: true\n", true),
    ] {
        let source = format!(
            "{directives}const global = undefined;\n\
             declare const ordinary: undefined;\n\
             function shadow(undefined: number) {{ return undefined; }}\n\
             const shadowResult = shadow(101);\n"
        );
        let case = TestCase::parse("probe/undefined-widening-modes", "mode.ts", &source);
        let arena = tsr_core::Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let i = *checker.intrinsics();
        assert_eq!(i.undefined_widening == i.undefined, strict, "{directives}");
        assert_eq!(i.undefined_widening.index(), if strict { 4 } else { 5 });
        // 29: `nullWideningType`'s loose twin occupies slot 8; the regular
        // `""`, `0` and `0n` (`checker.go:1049`) follow the other intrinsics.
        assert_eq!(checker.type_count(), 29);
        assert_eq!(checker.type_of(i.undefined_widening).flags, tsr_checker::TypeFlags::UNDEFINED);
        for other in [i.any, i.error, i.missing, i.null, i.never] {
            assert_ne!(i.undefined_widening, other);
        }

        let global = program.binder().undefined_symbol().unwrap();
        // The synthetic global carries the active widening identity
        // (`checker.go:1345`), which in strict mode is the ordinary type.
        assert_eq!(checker.get_type_of_symbol(global), i.undefined_widening);
        let file = program.source_file("mode.ts").unwrap();
        let root = tsr_ast::Node::SourceFile(file.source_file()).node_id().unwrap();
        let ordinary = program.binder().lookup_local(root, "ordinary").unwrap();
        assert_eq!(checker.get_type_of_symbol(ordinary), i.undefined);
        let shadow = program.binder().lookup_local(root, "shadowResult").unwrap();
        assert_eq!(checker.get_type_of_symbol(shadow), i.number);
    }
}
