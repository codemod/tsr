//! `isRelatedToEx`'s nullable-union narrowing (`relater.go:2646`) as the
//! reporter sees it: a definitely non-nullable source against `null` and/or
//! `undefined` plus one other type is related to, and reported against, that
//! type; an aliased union keeps its name in the head. Expected texts are
//! native tsgo's (vendor `5b1047d`, `--strict`).

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::format::write_flattened_diagnostic_message;

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
    checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic)| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, diagnostic, "\n");
            (diagnostic.message.code(), text)
        })
        .collect()
}

#[test]
fn a_missing_property_is_reported_against_the_non_nullable_constituent() {
    let actual = diagnostics(
        "interface Foo { a: string }\n\
         interface Bar { b: string; c: string }\n\
         const x: Foo | undefined = {};\n\
         const y: Foo | null | undefined = {};\n\
         const w: Bar | null = {};\n\
         function f(p: Foo | undefined) {}\n\
         f({});\n",
    );
    let missing_a = "Property 'a' is missing in type '{}' but required in type 'Foo'.";
    assert_eq!(
        actual,
        [
            (2741, missing_a.to_string()),
            (2741, missing_a.to_string()),
            (
                2739,
                "Type '{}' is missing the following properties from type 'Bar': b, c".to_string()
            ),
            (2741, missing_a.to_string()),
        ]
    );
}

#[test]
fn an_aliased_union_keeps_its_name_in_the_head() {
    let actual = diagnostics(
        "interface Foo { a: string }\n\
         type FU = Foo | undefined;\n\
         const q: FU = {};\n",
    );
    assert_eq!(
        actual,
        [(
            2322,
            "Type '{}' is not assignable to type 'FU'.\n  \
         Property 'a' is missing in type '{}' but required in type 'Foo'."
                .to_string()
        )]
    );
}

#[test]
fn a_union_of_two_object_types_is_not_narrowed() {
    let actual = diagnostics(
        "interface Foo { a: string }\n\
         interface Bar { b: string; c: string }\n\
         const z: Foo | Bar | undefined = {};\n",
    );
    let heads: Vec<(u32, &str)> =
        actual.iter().map(|(code, text)| (*code, text.lines().next().unwrap_or(""))).collect();
    assert_eq!(heads, [(2322, "Type '{}' is not assignable to type 'Bar | Foo | undefined'.")]);
}
