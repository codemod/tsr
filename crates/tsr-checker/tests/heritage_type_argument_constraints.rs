//! `checkTypeReferenceNode` (`checker.go:2982`) for heritage clause entries:
//! interface `extends`, class `implements` and a class `extends` naming a
//! class check their type arguments against the type parameters'
//! constraints. Expected texts are native tsgo's (vendor `5b1047d`).

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, d)| (d.message.code(), d.text())).collect()
}

#[test]
fn heritage_type_arguments_are_checked_against_their_constraints() {
    let actual = diagnostics(
        "interface Base { foo: string; }\n\
         interface Derived extends Base { bar: string; }\n\
         interface A<T extends Derived> { x: T; }\n\
         interface B extends A<Base> { }\n\
         class Foo { fooMethod() {} }\n\
         class FooExtended { }\n\
         class Bar<T extends Foo> { }\n\
         class BarExtended extends Bar<FooExtended> { }\n\
         class Impl implements A<Base> { x!: Derived; }\n",
    );
    let missing_bar = "Property 'bar' is missing in type 'Base' but required in type 'Derived'.";
    assert_eq!(
        actual,
        [
            (2741, missing_bar.to_string()),
            (
                2741,
                "Property 'fooMethod' is missing in type 'FooExtended' but required in type 'Foo'."
                    .to_string()
            ),
            (2741, missing_bar.to_string()),
        ]
    );
}

#[test]
fn satisfied_heritage_arguments_are_silent() {
    let actual = diagnostics(
        "interface Base { foo: string; }\n\
         interface Derived extends Base { bar: string; }\n\
         interface A<T extends Base> { x: T; }\n\
         interface B extends A<Derived> { }\n",
    );
    assert_eq!(actual, []);
}
