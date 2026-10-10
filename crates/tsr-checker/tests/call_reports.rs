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

/// `hasExcessProperties` runs before the structural comparison, and a type
/// variable of the enclosing declaration on either side does not decline: the
/// object literal's published type is native's (indexedAccessRelation).
#[test]
fn a_generic_candidate_reports_an_object_literal_over_enclosing_type_variables() {
    assert_eq!(
        reported(
            "type Pick<T, K extends keyof T> = { [P in K]: T[P] };\n\
             class Component<S> {\n    setState<K extends keyof S>(state: Pick<S, K>) {}\n}\n\
             export interface State<T> {\n    a?: T;\n}\nclass Foo {}\n\
             class Comp<T extends Foo, S> extends Component<S & State<T>> {\n\
             \x20   foo(a: T) {\n        this.setState({ a: a });\n    }\n}\n"
        ),
        [(2322, "a".into())]
    );
}

/// A `const` type parameter's object and array literal arguments decline:
/// native unions their const-context literal candidates
/// (`unionObjectAndArrayLiteralCandidates`), this port does not yet, so its
/// instantiation is not native's (typeParameterConstModifiers).
#[test]
fn a_const_type_parameter_literal_argument_declines() {
    assert_eq!(
        reported(
            "declare function f5<const T>(obj: { x: T, y: T }): T;\n\
             f5({ x: [1, 'x'], y: [2, 'y'] });\n"
        ),
        []
    );
}
