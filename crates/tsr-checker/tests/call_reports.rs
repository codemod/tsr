//! The reporting pass of call resolution (`call_reports.rs`): `isSignatureApplicable`
//! with `reportErrors` (`checker.go:9256`). Expected diagnostics are tsgo's at
//! the pinned vendor commit.
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn reported(source: &str) -> Vec<(u32, String)> {
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
        .map(|(_, d)| (d.message.code(), source[d.span.start as usize..d.span.end as usize].into()))
        .collect()
}

/// A context-sensitive callback is assigned its parameter types once, so after
/// inference its type is the one `checkExpressionWithContextualType` gives it
/// under the instantiated parameter: the failure elaborates into the arrow's
/// return expression.
#[test]
fn a_generic_candidate_reports_its_context_sensitive_callback() {
    assert_eq!(
        reported(
            "declare function f<S extends number>(cb: (x: string) => S): S;\nf(x => x);\n\
             declare function g<T>(a: T, cb: (x: T) => number): void;\ng(\"a\", x => x);\n"
        ),
        [(2322, "x".into()), (2322, "x".into())]
    );
}

/// Under a generic candidate's instantiation, an object literal's published
/// type is the one checked under the call's resolved signature, which for a
/// single candidate is that instantiation; the failure elaborates into the
/// member even when the target mentions a literal type.
#[test]
fn an_instantiated_candidate_reports_its_object_literal() {
    assert_eq!(
        reported(
            "interface D { x: \"a\" | \"b\" }\n\
             declare function d3<T extends { y: D }>(o: T): void;\nd3({ y: { x: 1 } });\n"
        ),
        [(2322, "x".into())]
    );
}
