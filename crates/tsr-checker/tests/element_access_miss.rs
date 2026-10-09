//! `getPropertyTypeForIndexType`'s failure arms by identity (ADR-0048),
//! switched by r5-errorsplit6 after a line-by-line check against the native
//! identity probe (`docs/parity/notes/r5-errorsplit6.md` §2).
//!
//! A failed element access is upstream's `errorType` (`checker.go:8176`)
//! where the port's lookup over the receiver is complete, and stays the
//! port's gap where its member image is known to be short. Identity is
//! asserted, not spelling.

use tsr_checker::Checker;
use tsr_core::Arena;

#[derive(Debug, PartialEq, Eq)]
enum Identity {
    /// Upstream's `errorType` (`Intrinsics::native_error`).
    NativeError,
    /// The port's gap (`Intrinsics::error`).
    Gap,
    /// Upstream's `anyType`.
    Any,
    /// Anything else, printed.
    Other(String),
}

/// What the file's last element access expression answered.
fn last_element_access(source: &str) -> Identity {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    #[allow(clippy::cast_possible_truncation)]
    let id = (0..parsed.nodes.len() as u32)
        .map(tsr_ast::NodeId::new)
        .filter(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ElementAccessExpression)
        .last()
        .expect("an element access");
    let node = parsed.node_map.get(id).expect("a mapped node");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let type_id = checker.check_expression(expression);
    let intrinsics = checker.intrinsics();
    if type_id == intrinsics.native_error {
        Identity::NativeError
    } else if type_id == intrinsics.error {
        Identity::Gap
    } else if type_id == intrinsics.any {
        Identity::Any
    } else {
        Identity::Other(checker.type_to_string(type_id))
    }
}

/// A named key the receiver does not have: `nil` after TS2339/TS7053
/// (`checker.go:27196`), so `errorType`.
#[test]
fn a_missing_named_member_is_native_error() {
    let source = "interface I { a: number }\ndeclare let i: I;\ni[\"b\"];\n";
    assert_eq!(last_element_access(source), Identity::NativeError);
}

/// A union key with one miss fails the whole access with that miss
/// (`getIndexedAccessTypeOrUndefined`, `checker.go:26975`).
#[test]
fn a_union_key_with_a_missing_member_is_native_error() {
    let source =
        "interface I { a: number }\ndeclare let i: I;\ndeclare let k: \"a\" | \"b\";\ni[k];\n";
    assert_eq!(last_element_access(source), Identity::NativeError);
}

/// The control: a key the receiver has is its member's type.
#[test]
fn a_present_member_is_its_type() {
    let source = "interface I { a: number }\ndeclare let i: I;\ni[\"a\"];\n";
    assert_eq!(last_element_access(source), Identity::Other("number".to_string()));
}

/// TS2476, then `errorType` (`checker.go:8157`). It answered the `any`
/// stand-in.
#[test]
fn a_const_enum_read_through_a_non_literal_is_native_error() {
    let source = "const enum E { A }\ndeclare let k: string;\nE[k];\n";
    assert_eq!(last_element_access(source), Identity::NativeError);
}

/// `isAssignmentToReadonlyEntity`: TS2540 and `nil` (`checker.go:27036`),
/// for a readonly field and for a namespace's exported `const`.
#[test]
fn a_write_to_a_readonly_member_is_native_error() {
    let field = "class C { readonly a = 1; m() { this[\"a\"] = 2; } }\n";
    assert_eq!(last_element_access(field), Identity::NativeError);
    let namespace = "namespace M { export const x = 0; }\nM[\"x\"] = 1;\n";
    assert_eq!(last_element_access(namespace), Identity::NativeError);
}

/// A readonly tuple's element named by a literal is a readonly entity; a
/// `number` index takes the index-signature road, which still answers the
/// element type natively, so the port keeps its stand-in there.
#[test]
fn a_readonly_tuple_write_splits_by_index_road() {
    let named = "declare const t: readonly [number, number];\nt[0] = 1;\n";
    assert_eq!(last_element_access(named), Identity::NativeError);
    let indexed =
        "declare const t: readonly [number, number];\ndeclare let i: number;\nt[i] = 1;\n";
    assert_eq!(last_element_access(indexed), Identity::Any);
}

/// A function's late-bound assignment members are unbound in this port
/// (`binder.go:1002`), so a miss on a function's type is the port's gap,
/// not a claim about the receiver.
#[test]
fn a_miss_on_a_function_receiver_stays_the_gap() {
    let source = "function foo() {}\ndeclare const s: \"x\";\nfoo[s];\n";
    assert_eq!(last_element_access(source), Identity::Gap);
}

/// `createUnionOrIntersectionProperty`'s object-literal arm is ported
/// (`docs/parity/notes/r6-errorsplit.md` §5): the literal constituent
/// contributes `undefined`, so the access is no miss at all.
#[test]
fn a_union_with_an_object_literal_reads_undefined_for_the_literal() {
    let source = "declare let x: { a: string } | undefined;\n(x || {})[\"a\"];\n";
    assert_eq!(last_element_access(source), Identity::Other("string | undefined".to_owned()));
}
