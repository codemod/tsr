//! `checkTypeParameters`' default rules and `checkGrammarModifiers`'
//! type-parameter arms, pinned to 5b1047d1, for written lists and for the
//! `@template` lists the JS reparser makes (`gatherTypeParameters`,
//! `parser/reparser.go:293`). `docs/parity/notes/r6-jsdoc.md` §6.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every diagnostic of a checked file, as `(code, line, text at its span)`.
fn diagnostics(source: &str, javascript: bool) -> Vec<(u32, usize, String)> {
    let name = if javascript { "t.js" } else { "t.ts" };
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    if javascript {
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        for (_, docs) in parsed.jsdoc.iter() {
            for doc in docs {
                if let Some(id) = doc.node_id {
                    parsed.nodes.add_flags(id, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
                }
            }
        }
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut out: Vec<_> = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            let start = d.span.start as usize;
            let line = source[..start].matches('\n').count() + 1;
            (d.message.code(), line, source[start..d.span.end as usize].to_string())
        })
        .collect();
    out.sort();
    out
}

#[test]
fn written_lists_report_defaults_and_modifiers() {
    let source = "type A<T = U, U = string> = [T, U];\ntype B<T = string, U> = [T, U];\nclass C<private T> {}\n";
    assert_eq!(
        diagnostics(source, false),
        [(1273, 3, "private".to_string()), (2706, 2, "U".to_string()), (2744, 1, "U".to_string()),]
    );
}

#[test]
fn template_lists_report_defaults_and_modifiers() {
    let source = "/**
 * @template const T
 * @typedef {[T]} X
 */
/**
 * @template private T
 * @param {T} x
 * @returns {T}
 */
function f(x) { return x; }
/**
 * @template in T
 * @param {T} x
 */
function g(x) {}
/**
 * @template [T=string]
 * @template U
 * @typedef {[T, U]} E
 */
var e;
/**
 * @template [T=U]
 * @template [U=T]
 * @typedef {[T, U]} G
 */
var g2;
";
    assert_eq!(
        diagnostics(source, true),
        [
            (1273, 6, "private".to_string()),
            (1274, 12, "in".to_string()),
            (1277, 2, "const".to_string()),
            (2706, 18, "U".to_string()),
            (2744, 23, "U".to_string()),
        ]
    );
}
