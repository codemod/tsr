//! String helpers the compiler shares.
//!
//! Ported from `internal/stringutil/util.go` at the pinned commit. Upstream
//! keeps these in a leaf package that the binder, the checker and the emitters
//! all import; the same is true here, which is why they live in `tsr-core`
//! rather than in whichever crate needed one first.

/// A name with one layer of matching surrounding quotes removed.
///
/// `stringutil.StripQuotes` (`internal/stringutil/util.go:222`). The three quote
/// characters are upstream's: `'`, `"` and `` ` ``.
///
/// # Why this is shared rather than written where it is needed
///
/// An ambient module's symbol name **contains its quotes** — `declare module
/// "fs"` is the symbol `"fs"`, per `getDeclarationName`
/// (`internal/binder/binder.go:311`) — and that is what keeps it from colliding
/// with an ordinary global called `fs`. Every consumer that wants to *display*
/// the specifier has to take them off again, and upstream does so through this
/// one function at each site (`getSpecifierForModuleSymbol`,
/// `nodebuilderimpl.go:1261`).
///
/// Writing the test inline is how a codebase ends up with four spellings of one
/// predicate, three of which forget the backtick.
///
/// # Bytes, not runes, and it is still faithful
///
/// Upstream decodes the first and last *runes* before comparing them. All three
/// quote characters are ASCII, so a leading or trailing multi-byte rune can
/// never equal one, and comparing bytes reaches the same answer without the
/// decode. The slice below is therefore always on a character boundary.
#[must_use]
pub fn strip_quotes(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() < 2 {
        return name;
    }
    let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
    if first == last && matches!(first, b'\'' | b'"' | b'`') {
        return &name[1..name.len() - 1];
    }
    name
}

#[cfg(test)]
mod tests {
    use super::strip_quotes;

    #[test]
    fn matching_quotes_are_removed_and_nothing_else_is() {
        assert_eq!(strip_quotes("\"fs\""), "fs");
        assert_eq!(strip_quotes("'fs'"), "fs");
        assert_eq!(strip_quotes("`fs`"), "fs");
        // Unmatched, mismatched, or absent: unchanged.
        assert_eq!(strip_quotes("\"fs"), "\"fs");
        assert_eq!(strip_quotes("\"fs'"), "\"fs'");
        assert_eq!(strip_quotes("fs"), "fs");
        // One layer only, which is what upstream removes.
        assert_eq!(strip_quotes("\"\"fs\"\""), "\"fs\"");
    }

    #[test]
    fn short_and_degenerate_inputs_are_returned_whole() {
        // `len(name) < 2` upstream. `"` alone is one character and must not be
        // read as its own opening and closing quote.
        assert_eq!(strip_quotes(""), "");
        assert_eq!(strip_quotes("\""), "\"");
        // Two quotes ARE a matching pair around nothing, upstream included.
        assert_eq!(strip_quotes("\"\""), "");
    }

    #[test]
    fn a_multibyte_boundary_is_never_split() {
        // The byte comparison stands in for upstream's rune decode. A name
        // whose first byte is part of a multi-byte character can never match an
        // ASCII quote, so the slice below is never reached for one.
        assert_eq!(strip_quotes("日本"), "日本");
        assert_eq!(strip_quotes("\"日本\""), "日本");
    }
}
