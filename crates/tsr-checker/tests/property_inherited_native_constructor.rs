//! isAssignmentToReadonlyEntity permits both access-expression kinds only
//! within the constructor owning the field (pinned tsgo 5b1047d).
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

#[test]
fn element_access_constructor_permission_uses_the_declaring_class() {
    let source = "class C { readonly x: number; constructor() { this['x'] = 1; } }\nclass D extends C { constructor() { super(); this['x'] = 2; } }\ndeclare let c: C;\nc['x'] = 3;\n";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let errors: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.message.code() == 2540)
        .map(|(_, d)| (d.span.start, d.args.clone()))
        .collect();
    let derived = u32::try_from(source.find("this['x'] = 2").unwrap() + 5).unwrap();
    let outside = u32::try_from(source.find("c['x'] = 3").unwrap() + 2).unwrap();
    assert_eq!(errors, vec![(derived, vec!["x".to_owned()]), (outside, vec!["x".to_owned()])]);
}
