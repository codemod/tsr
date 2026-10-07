//! Full-source default native SkipTrivia offsets and context boundaries.
use tsr_scanner::skip_trivia;

#[test]
fn full_source_offsets_preserve_shebang_and_conflict_context() {
    for (source, start, expected) in [
        ("#!/usr/bin/env node\nvalue", 0, 20),
        ("x #!not-a-shebang", 2, 2),
        ("x\n<<<<<<< HEAD\nvalue", 2, 15),
        ("x <<<<<<< HEAD\nvalue", 2, 2),
        ("<<<<<<<", 0, 0),
        ("=======", 0, 0),
        ("<<<<<<< HEAD\u{2028}value", 0, 15),
        ("=======\nignored\n>>>>>>> branch\nvalue", 0, 31),
        ("x\n /*a*/ T, /*b*/ >", 11, 18),
        ("f</*c*/>", 2, 7),
        (" /* unterminated", 0, 16),
        ("// comment\u{2028}value", 0, 13),
        ("\u{0085}\u{200b}\u{feff}value", 0, 8),
        ("* decoration", 0, 0),
        ("value", 10, 10),
    ] {
        assert_eq!(skip_trivia(source, start), expected, "{source:?} at {start}");
    }
}
