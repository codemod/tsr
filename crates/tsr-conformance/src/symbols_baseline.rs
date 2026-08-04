//! Reading upstream's `.symbols` baselines.
//!
//! These are the oracle for the binder, and they are a better one than a symbol
//! table dump would be. For **every identifier occurrence** in a file, upstream
//! records the symbol it resolved to and where every declaration of that symbol
//! is:
//!
//! ```text
//! === typeTagForMultipleVariableDeclarations.js ===
//! /** @type {number} */
//! var x,y,z;
//! >x : Symbol(x, Decl(typeTagForMultipleVariableDeclarations.js, 1, 3))
//! ```
//!
//! So the file tests name *resolution* end to end — scope walking, declaration
//! merging, and which declarations a symbol ended up with — rather than the shape
//! of any internal structure. That matters because a symbol table dump would
//! compare our data model against theirs, and the two are deliberately different
//! ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md),
//! [ADR-0013](../../../docs/adr/0013-checker-memoisation.md)). Resolution results
//! are model-independent.
//!
//! 12,482 of these are committed in the submodule, which is the same
//! already-generated-Go oracle the parser is judged against
//! ([ADR-0006](../../../docs/adr/0006-conformance-oracle.md)).

/// One declaration site: `Decl(file.ts, line, character)`, both zero-based.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Decl {
    /// The file the declaration is in, as the baseline names it.
    pub file: String,
    /// Zero-based line.
    pub line: u32,
    /// Zero-based character, in UTF-16 code units.
    pub character: u32,
}

/// One `>name : Symbol(...)` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRef {
    /// The identifier as written at the use site.
    pub occurrence: String,
    /// The symbol's name, which differs from `occurrence` for aliases.
    pub symbol: String,
    /// Every declaration of the symbol, in the order upstream listed them.
    pub declarations: Vec<Decl>,
}

/// The expectations for one file within a case.
#[derive(Debug, Clone, Default)]
pub struct FileSymbols {
    /// The file, as the `=== name ===` header gives it.
    pub file: String,
    /// Every `>name : Symbol(...)` annotation, in source order.
    pub refs: Vec<SymbolRef>,
}

/// Parse a `.symbols` baseline.
///
/// Unrecognised lines are source text and are skipped: the baseline interleaves
/// the original source with the `>` annotations, and only the annotations are
/// assertions.
#[must_use]
pub fn parse(text: &str) -> Vec<FileSymbols> {
    let mut files: Vec<FileSymbols> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("=== ") {
            if let Some(name) = rest.strip_suffix(" ===") {
                files.push(FileSymbols { file: name.to_string(), refs: Vec::new() });
            }
            continue;
        }
        let Some(rest) = line.strip_prefix('>') else { continue };
        let Some(parsed) = parse_ref(rest) else { continue };
        if let Some(current) = files.last_mut() {
            current.refs.push(parsed);
        }
    }
    files
}

/// `x : Symbol(x, Decl(a.ts, 1, 3), Decl(a.ts, 4, 0))`
fn parse_ref(line: &str) -> Option<SymbolRef> {
    let (occurrence, rest) = line.split_once(" : ")?;
    // Lines that are not symbol annotations — upstream also emits `>foo : any`
    // style lines in `.types` baselines, and a stray one here should be skipped
    // rather than mis-parsed.
    let inner = rest.strip_prefix("Symbol(")?.strip_suffix(')')?;

    // The symbol name runs to the first `, Decl(`; a name may itself contain a
    // comma (a computed or quoted name), so splitting on the marker rather than
    // on the first comma matters.
    let (symbol, decls) = match inner.find(", Decl(") {
        Some(at) => (&inner[..at], &inner[at + 2..]),
        // `Symbol(x)` with no declarations happens for some synthesised symbols.
        None => (inner, ""),
    };

    let mut declarations = Vec::new();
    for part in decls.split("Decl(").skip(1) {
        let body = part.trim_end().trim_end_matches(',').trim_end_matches(')');
        let mut fields = body.rsplitn(3, ", ");
        let character = fields.next()?.trim_end_matches(')').parse().ok()?;
        let line = fields.next()?.parse().ok()?;
        let file = fields.next()?.to_string();
        declarations.push(Decl { file, line, character });
    }

    Some(SymbolRef {
        occurrence: occurrence.trim().to_string(),
        symbol: symbol.trim().to_string(),
        declarations,
    })
}

/// Byte offset to zero-based line and character, matching upstream's counting.
///
/// Characters are **UTF-16 code units**, because that is what TypeScript's
/// positions are and therefore what the baselines contain. Counting `char`s would
/// agree on ASCII and drift on anything astral, which is the kind of difference
/// that shows up as a handful of mysterious failures rather than an obvious one.
#[must_use]
pub fn line_and_character(source: &str, offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (index, byte) in source.as_bytes().iter().enumerate().take(offset) {
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    let character = source
        .get(line_start..offset)
        .map_or(0, |text| text.chars().map(char::len_utf16).sum::<usize>());
    #[allow(clippy::cast_possible_truncation)]
    (line, character as u32)
}

/// Every token's full start in a file, recovered by scanning forwards.
///
/// TypeScript's `node.pos` is the *full start* — where the node's leading trivia
/// begins — not where its first token does. For
///
/// ```text
/// class C {
///    foo();
/// ```
///
/// `foo`'s declaration is reported at line 0, character 9: just after the `{`,
/// on the line above the one `foo` is written on. Comparing token starts, or even
/// token-start *lines*, disagrees with the baseline everywhere.
///
/// # Why forwards
///
/// This used to walk *backwards* over trivia from the token start, which cannot
/// work: a `//` comment is unrecognisable scanning backwards without re-lexing
/// the line, so a declaration preceded by one landed on the comment's end rather
/// than before it. Measured 2026-08-04 over the corpus, that mis-attributed
/// **2,015 declaration positions** — 817 cases failed on nothing else — and the
/// error had a signature: the expected line was 2 above ours in 860 of them, the
/// shape of a `//` comment plus a blank line.
///
/// The scanner already computes exactly this quantity ([`Scanner::full_start`]),
/// because the parser needs it. One forward pass over the file therefore gives
/// the answer directly rather than approximating it.
///
/// # Where it is still approximate
///
/// The scanner is context-free and the parser is not: a `/` is a regex or a
/// division depending on grammatical context, `>>` splits inside type arguments,
/// and JSX text is not lexed as ordinary tokens. In such a region this scan's
/// token boundaries can diverge from the parser's, and a declaration start may
/// not land on one. That case falls back to [`full_start`]'s backwards walk, so
/// the result is never worse than before — it is exact wherever the forward scan
/// agrees, which is everywhere outside those regions.
///
/// [`Scanner::full_start`]: tsr_scanner::Scanner::full_start
pub struct FullStarts {
    /// `(token start, token full start)`, ascending by token start.
    entries: Vec<(u32, u32)>,
}

impl FullStarts {
    /// Scan `source` once and record where every token's trivia begins.
    #[must_use]
    pub fn scan(source: &str) -> Self {
        let mut scanner = tsr_scanner::Scanner::new(source);
        let mut entries = Vec::new();
        loop {
            let token = scanner.scan();
            entries.push((token.span.start, scanner.full_start()));
            if token.kind == tsr_ast::SyntaxKind::EndOfFile {
                break;
            }
        }
        Self { entries }
    }

    /// The full start of the token beginning at `start`.
    ///
    /// Falls back to [`full_start`] when `start` is not a token boundary this
    /// scan found; see the note above on why that can happen.
    #[must_use]
    pub fn of(&self, source: &str, start: u32) -> u32 {
        match self.entries.binary_search_by_key(&start, |(token_start, _)| *token_start) {
            Ok(index) => self.entries[index].1,
            Err(_) => full_start(source, start),
        }
    }
}

/// The position upstream reports for a declaration, recovered by walking back
/// over trivia.
///
/// Superseded by [`FullStarts`], which gets this right; kept as its fallback for
/// the positions a forward scan cannot place, and because it is what the
/// difference between the two is measured against.
///
/// Both comment forms are handled. A block comment is recognised by its closing
/// `*/`. A `//` comment cannot be recognised scanning backwards at all — it ends
/// at a line break like the whitespace around it — so it is found by scanning the
/// *line* forwards from its start, which is the smallest amount of re-lexing that
/// answers the question. Quote counting decides whether a `//` is a comment or
/// text inside a string; that is an approximation, and it is confined to the
/// fallback, where the alternative was ignoring line comments entirely. Measured
/// 2026-08-04: handling them is worth 20 conformance cases, with no regressions.
#[must_use]
pub fn full_start(source: &str, start: u32) -> u32 {
    let bytes = source.as_bytes();
    let mut i = start as usize;
    loop {
        // Whitespace, including line breaks.
        while i > 0 && bytes[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        // A block comment: skip back to its opening.
        if i >= 2 && bytes[i - 1] == b'/' && bytes[i - 2] == b'*' {
            let mut j = i - 2;
            while j >= 2 && !(bytes[j - 1] == b'*' && bytes[j - 2] == b'/') {
                j -= 1;
            }
            if j >= 2 {
                i = j - 2;
                continue;
            }
        }
        // A line comment, which the whitespace skip above has just walked to the
        // end of. It has no closing token to recognise, so the line it sits on is
        // scanned forwards for the `//` that opens it.
        if let Some(comment) = line_comment_start(bytes, i) {
            i = comment;
            continue;
        }
        break;
    }
    #[allow(clippy::cast_possible_truncation)]
    {
        i as u32
    }
}

/// Where the `//` comment ending at `end` begins, if that is what ends there.
///
/// Scans the line forwards from its start, because a line comment has no closing
/// token: `x` in `// x` looks exactly like `x` in code read backwards. Quotes are
/// counted so that the `//` in `"http://…"` is not mistaken for one — an
/// approximation that a real lexer would not need, and one confined to this
/// fallback.
fn line_comment_start(bytes: &[u8], end: usize) -> Option<usize> {
    if end == 0 {
        return None;
    }
    let line_start = bytes[..end].iter().rposition(|byte| *byte == b'\n').map_or(0, |n| n + 1);
    let mut quotes = 0usize;
    let mut index = line_start;
    while index + 1 < end {
        match bytes[index] {
            b'"' | b'\'' | b'`' => quotes += 1,
            b'/' if bytes[index + 1] == b'/' && quotes % 2 == 0 => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn a_line_comment_is_leading_trivia_of_what_follows_it() {
        // Upstream's `pos` is where the trivia begins, and a comment above a
        // declaration is trivia of that declaration.
        let source = "}\n\n// why\n\ntype T = 1;\n";
        let token = source.find("type").expect("the declaration") as u32;
        // Trivia begins immediately after the `}`.
        let expected = source.find('\n').expect("the first line break") as u32;
        assert_eq!(full_start(source, token), expected);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn a_slash_inside_a_string_does_not_open_a_comment() {
        let source = "const u = \"http://x\";\ntype T = 1;\n";
        let token = source.find("type").expect("the declaration") as u32;
        let expected = source.find('\n').expect("the line break") as u32;
        assert_eq!(full_start(source, token), expected);
    }

    #[test]
    fn a_reference_with_one_declaration_parses() {
        let parsed = parse_ref("x : Symbol(x, Decl(a.ts, 1, 3))").expect("parses");
        assert_eq!(parsed.occurrence, "x");
        assert_eq!(parsed.symbol, "x");
        assert_eq!(parsed.declarations, [Decl { file: "a.ts".into(), line: 1, character: 3 }]);
    }

    #[test]
    fn merged_declarations_are_all_listed() {
        let parsed =
            parse_ref("I : Symbol(I, Decl(a.ts, 0, 0), Decl(a.ts, 2, 1))").expect("parses");
        assert_eq!(parsed.declarations.len(), 2);
        assert_eq!(parsed.declarations[1], Decl { file: "a.ts".into(), line: 2, character: 1 });
    }

    #[test]
    fn the_file_header_starts_a_new_group() {
        let text = "=== a.ts ===\nvar x;\n>x : Symbol(x, Decl(a.ts, 0, 4))\n\
                    === b.ts ===\nvar y;\n>y : Symbol(y, Decl(b.ts, 0, 4))\n";
        let files = parse(text);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file, "a.ts");
        assert_eq!(files[0].refs.len(), 1);
        assert_eq!(files[1].refs[0].occurrence, "y");
    }

    #[test]
    fn source_lines_are_not_mistaken_for_annotations() {
        // The baseline interleaves source; only `>` lines are assertions, and a
        // `>` in the source (a comparison, a JSX close) must not parse as one.
        let files = parse("=== a.ts ===\nif (a > b) {}\n>a : Symbol(a, Decl(a.ts, 0, 0))\n");
        assert_eq!(files[0].refs.len(), 1, "only the annotation counts");
    }

    #[test]
    fn positions_are_utf16_code_units_not_chars() {
        // An astral character is one `char` and two UTF-16 units. TypeScript
        // counts the latter, so a `chars().count()` here would be off by one for
        // every emoji on the line.
        let source = "const a = '😀'; const b = 1;";
        let offset = source.find("const b").expect("present");
        #[allow(clippy::cast_possible_truncation)]
        let (line, character) = line_and_character(source, offset as u32);
        assert_eq!(line, 0);
        assert_eq!(character, 16, "expected UTF-16 units");
    }

    #[test]
    fn a_forward_scan_places_a_declaration_before_its_line_comments() {
        // The case the backwards walk could not do, and the reason the corpus
        // mis-attributed 2,015 declaration positions before this existed:
        // upstream reports `interface A` at 0,0 — back through both comments.
        let source = "// one\n// two\n\ninterface A {}\n";
        #[allow(clippy::cast_possible_truncation)]
        let interface = source.find("interface").expect("present") as u32;
        let full_starts = FullStarts::scan(source);
        assert_eq!(full_starts.of(source, interface), 0);
        assert_eq!(line_and_character(source, full_starts.of(source, interface)), (0, 0));

        // The backwards walk used to answer (1, 6) here — the end of the second
        // comment — because it could not recognise a `//` scanning backwards. It
        // now scans each line forwards to find one, so the two agree. The forward
        // scan is still the primary: it is exact, and this is an approximation
        // that quote-counts its way past `"http://…"`.
        assert_eq!(line_and_character(source, full_start(source, interface)), (0, 0));
    }

    #[test]
    fn a_forward_scan_agrees_with_the_backwards_walk_where_that_one_worked() {
        let source = "class C {\n   foo();\n}";
        #[allow(clippy::cast_possible_truncation)]
        let foo = source.find("foo").expect("present") as u32;
        let full_starts = FullStarts::scan(source);
        assert_eq!(full_starts.of(source, foo), full_start(source, foo));
        assert_eq!(line_and_character(source, full_starts.of(source, foo)), (0, 9));
    }

    #[test]
    fn a_position_that_is_not_a_token_boundary_falls_back() {
        // Inside a string literal, so no token starts there. The fallback keeps
        // the old behaviour rather than returning some earlier token's trivia.
        let source = "const s = \"abcdef\";";
        let full_starts = FullStarts::scan(source);
        assert_eq!(full_starts.of(source, 13), full_start(source, 13));
    }

    #[test]
    fn full_start_is_the_end_of_the_previous_token() {
        // The case that motivated it: `foo` is on line 1, but upstream reports
        // its declaration at the `{` on line 0.
        let source = "class C {\n   foo();\n}";
        #[allow(clippy::cast_possible_truncation)]
        let foo = source.find("foo").expect("present") as u32;
        let pos = full_start(source, foo);
        assert_eq!(line_and_character(source, pos), (0, 9));
    }

    #[test]
    fn full_start_walks_back_over_a_block_comment() {
        let source = "var a; /* note */ var b;";
        #[allow(clippy::cast_possible_truncation)]
        let b = source.rfind("var b").expect("present") as u32;
        // Just after `var a;`, i.e. before the comment and its surrounding space.
        assert_eq!(full_start(source, b), 6);
    }

    #[test]
    fn full_start_of_the_first_token_is_the_file_start() {
        assert_eq!(full_start("class C {}", 0), 0);
        assert_eq!(full_start("\n\n  class C {}", 4), 0);
    }

    #[test]
    fn lines_count_from_zero() {
        let source = "a\nb\nc";
        assert_eq!(line_and_character(source, 0), (0, 0));
        assert_eq!(line_and_character(source, 2), (1, 0));
        assert_eq!(line_and_character(source, 4), (2, 0));
    }
}
