//! Private set-only reads are checked in checked JS as well as TS, pinned
//! checkPropertyAccessExpressionOrQualifiedName, tsgo 5b1047d.
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

#[test]
fn checked_javascript_private_setter_reads_but_not_definite_writes_error() {
    let source = "class C { set #x(value) {} read() { return this.#x; } write() { this.#x = 1; } update() { this.#x += 1; } }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.js", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.unwrap(),
        FileContext { ambient: false, has_parse_errors: false },
    );
    let positions: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.message.code() == 2806)
        .map(|(_, d)| d.span.start)
        .collect();
    let read = u32::try_from(source.find("this.#x;").unwrap()).unwrap();
    let update = u32::try_from(source.find("this.#x +=").unwrap()).unwrap();
    assert_eq!(positions, vec![read, update]);
}
