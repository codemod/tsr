//! Cheap scanner flags control the parser's native eager/lazy JSDoc boundary.

use tsr_scanner::{TokenFlags, tokenize};

#[test]
fn parser_lazy_tag_terminators_match_native_byte_set() {
    for tag in ["deprecated", "see", "link", "linkcode", "linkplain"] {
        let flag = if tag == "deprecated" {
            TokenFlags::PRECEDING_JSDOC_WITH_DEPRECATED
        } else {
            TokenFlags::PRECEDING_JSDOC_WITH_SEE_OR_LINK
        };
        for (terminator, expected) in [
            ("", true),
            (" ", true),
            ("\t", true),
            ("\n", true),
            ("\r", true),
            ("}", true),
            ("*", true),
            ("\u{b}", false),
            ("\u{c}", false),
            ("\u{a0}", false),
            ("x", false),
        ] {
            let source = format!("/** @{tag}{terminator} */ let x;");
            let (tokens, diagnostics) = tokenize(&source);
            assert!(diagnostics.is_empty(), "{source:?}: {diagnostics:?}");
            assert_eq!(tokens[0].flags.contains(flag), expected, "{source:?}");
        }
    }
}
