//! Native 5b1047d module-consumer-wave41: an augmentation supplies requiredToken
//! to both original and namespace-copy views, making them Function-related.
use tsr_checker::relater::{Relation, Ternary};
use tsr_conformance::{TestCase, types_producer};
use tsr_core::Arena;

#[test]
fn merged_augmentation_does_not_yet_prove_function_relation() {
    for interop in [false, true] {
        for synthetic in [false, true] {
            for reversed in [false, true] {
                let text = format!(
                    "// @noLib: true\n// @module: commonjs\n\
                     // @esModuleInterop: {interop}\n\
                     // @allowSyntheticDefaultImports: {synthetic}\n\
                     // @filename: globals.d.ts\n\
                     interface Array<T> {{}}\ninterface Boolean {{}}\n\
                     interface CallableFunction {{}}\ninterface Function {{ requiredToken: any; }}\n\
                     interface IArguments {{}}\ninterface NewableFunction {{}}\n\
                     interface Number {{}}\ninterface Object {{}}\n\
                     interface RegExp {{}}\ninterface String {{}}\n\
                     // @filename: mod.ts\nexport const token = 17;\n\
                     // @filename: bridge.ts\nexport {{}};\n\
                     declare module './mod' {{ export const requiredToken: 17; }}\n\
                     // @filename: use.ts\n/// <reference path='globals.d.ts' />\n\
                     import './bridge';\n\
                     import original = require('./mod');\nimport * as copy from './mod';\n\
                     original(); copy();\n"
                );
                let mut case =
                    TestCase::parse("probe/module-augmentation-absence", "fixture.ts", &text);
                if reversed {
                    case.files.reverse();
                }
                let arena = Arena::new();
                let program = types_producer::program_for_case(&arena, &case);
                let mut checker = types_producer::configured_checker(&program);
                let root = program.source_file("mod.ts").unwrap().source_file().node_id.unwrap();
                let owner = program.binder().symbol_of(root).unwrap();
                let entry = program.binder().symbols().get(owner);
                // Native merges the augmentation into the module symbol
                // (`mergeModuleAugmentation`), and so does this port since
                // names-modules §4: the export and the declaration are there.
                // The relation below still answers `Unknown` — the relater
                // does not yet close a module object's export bound.
                assert_eq!(entry.declarations.len(), 2);
                assert_eq!(entry.declarations[0], root);
                assert!(entry.exports.contains_key("requiredToken"));
                let source = checker.get_type_of_symbol(owner);
                let target = checker
                    .get_declared_type_of_symbol(program.binder().global("Function").unwrap());
                for _ in 0..3 {
                    for relation in
                        [Relation::Assignable, Relation::Subtype, Relation::StrictSubtype]
                    {
                        assert_eq!(
                            checker.relate_ternary(source, target, relation),
                            Ternary::Unknown,
                            "merged augmentation, interop={interop}, synthetic={synthetic}, \
                             reversed={reversed}, relation={relation:?}"
                        );
                    }
                }
            }
        }
    }
}
