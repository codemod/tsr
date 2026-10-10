//! `ParseOptions::defer_ts_jsdoc` (ADR-0053): a non-JavaScript program file
//! parses a construct's comments only when one carries `@see`/`@link`, as
//! typescript-go's `withJSDoc` does (`internal/parser/jsdoc.go:56`).

use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_with_options};

fn documented(name: &str, source: &str) -> usize {
    let arena = Arena::new();
    let source = arena.alloc_str(source);
    let options = ParseOptions::for_file(name).deferring_ts_jsdoc(name);
    parse_with_options(&arena, source, options).jsdoc.len()
}

#[test]
fn plain_comments_in_a_ts_file_are_deferred() {
    let source = "/** Adds. @param a first */\nfunction add(a: number) {}\n";
    assert_eq!(documented("a.ts", source), 0);
    assert_eq!(documented("a.d.ts", source), 0);
    assert_eq!(documented("a.tsx", source), 0);
}

#[test]
fn see_and_link_comments_in_a_ts_file_are_parsed() {
    for tag in ["@see other", "{@link other}", "{@linkcode other}", "{@linkplain other}", "@see"] {
        let source = format!("/** Adds. {tag} */\nfunction add(a: number) {{}}\n");
        assert_eq!(documented("a.ts", &source), 1, "{tag}");
    }
    // `hasJSDocTag` needs a delimiter after the name.
    assert_eq!(documented("a.ts", "/** @seeing @linked */\nfunction add() {}\n"), 0);
}

#[test]
fn any_comment_of_the_construct_with_a_tag_parses_all_of_them() {
    let source = "/** first */\n/** second @see x */\nfunction add() {}\n";
    let arena = Arena::new();
    let source = arena.alloc_str(source);
    let options = ParseOptions::for_file("a.ts").deferring_ts_jsdoc("a.ts");
    let parsed = parse_with_options(&arena, source, options);
    let docs: Vec<_> = parsed.jsdoc.iter().map(|(_, docs)| docs.len()).collect();
    assert_eq!(docs, [2]);
}

#[test]
fn javascript_files_and_the_default_parse_every_comment() {
    let source = "/** Adds. @param a first */\nfunction add(a) {}\n";
    for name in ["a.js", "a.jsx", "a.mjs", "a.cjs", "A.JS"] {
        assert_eq!(documented(name, source), 1, "{name}");
    }
    let arena = Arena::new();
    let source = arena.alloc_str(source);
    let parsed = parse_with_options(&arena, source, ParseOptions::for_file("a.ts"));
    assert_eq!(parsed.jsdoc.len(), 1);
}
