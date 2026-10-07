//! Pinned tsgo 5b1047d: forEachProperty, synthetic modifier precedence,
//! isClassDerivedFromDeclaringClasses and canonical protected receiver context.
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

#[test]
fn static_composite_checks_every_protected_origin_and_public_precedence() {
    let source = "class A { protected static s = 1; }\nclass B { protected static s = 1; }\ndeclare function mix<X, Y>(x: X, y: Y): X & Y;\nclass Both extends mix(A, B) { static read() { Both.s; } }\nclass One extends A { static read() { Both.s; } }\nBoth.s;\nclass Public { static s = 1; }\nclass Mixed extends mix(A, Public) {}\nMixed.s;\nclass Base { protected p = 1; }\nconst Alias = Base;\nclass Derived extends Alias { read(b: Base) { return b.p; } }\n";
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
        .filter(|(_, d)| matches!(d.message.code(), 2445 | 2446))
        .map(|(_, d)| (d.message.code(), d.span.start, d.args.clone()))
        .collect();
    let mut expected: Vec<_> = source
        .match_indices("Both.s")
        .map(|(at, _)| {
            (2445, u32::try_from(at + 5).unwrap(), vec!["s".to_owned(), "typeof Both".to_owned()])
        })
        .collect();
    expected.push((
        2446,
        u32::try_from(source.find("b.p;").unwrap() + 2).unwrap(),
        vec!["p".to_owned(), "Derived".to_owned(), "Base".to_owned()],
    ));
    assert_eq!(errors, expected);
}
