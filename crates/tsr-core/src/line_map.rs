//! Byte offset to line and character, the way ECMAScript counts.
//!
//! Ported from `ComputeECMALineStartsSeq` (`internal/core/core.go:435`),
//! `ComputeLineOfPosition`, and `GetECMALineAndUTF16CharacterOfPosition`
//! (`internal/scanner/scanner.go:2685`) at the pinned commit.
//!
//! # Why this is in `tsr-core` and not in whoever needed it first
//!
//! It was written twice before it was written here. The conformance harness has
//! carried a `line_and_character` since the symbol baselines needed one, and the
//! diagnostic formatter needs the same answer to print `a.ts(3,7)`. Two
//! implementations of "which line is this offset on" is exactly the shape of
//! duplication that stays correct until one of them meets a `\r` — and only one
//! of them had ever met a `\r`.
//!
//! # What counts as a line break
//!
//! Four things, and the last two are the ones a hand-rolled version misses:
//! `\n`, `\r`, `\r\n` (**one** break, not two), U+2028 LINE SEPARATOR, and
//! U+2029 PARAGRAPH SEPARATOR. TypeScript inherits the set from the ECMAScript
//! grammar's `LineTerminator`, which is why a `.js` file using U+2028 as its only
//! separator is a single line to a naive scanner and many lines to `tsc`.

/// The byte offset each line starts at, line 0 first (`ComputeECMALineStarts`).
///
/// Always non-empty: a zero-length text is one line starting at 0, which is what
/// makes [`line_and_character`] total rather than fallible.
#[must_use]
pub fn ecma_line_starts(text: &str) -> Vec<u32> {
    let bytes = text.as_bytes();
    // Upstream sizes this `strings.Count(text, "\n") + 1`, which is exact for the
    // overwhelmingly common file and one short for a `\r`-only one.
    // Clippy suggests the `bytecount` crate. Declined: this is a capacity hint
    // on a vector that is about to be filled by the loop below, so a
    // SIMD-accelerated count would shave a pass that is already cheaper than the
    // scan it precedes, in exchange for a dependency.
    #[allow(clippy::naive_bytecount)]
    let mut starts = Vec::with_capacity(bytes.iter().filter(|byte| **byte == b'\n').count() + 1);

    let mut pos = 0usize;
    let mut line_start = 0usize;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte.is_ascii() {
            pos += 1;
            match byte {
                b'\r' => {
                    // A `\r\n` is one line break. Upstream expresses this as a
                    // `fallthrough` into the `\n` arm after consuming the `\n`.
                    if pos < bytes.len() && bytes[pos] == b'\n' {
                        pos += 1;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    starts.push(line_start as u32);
                    line_start = pos;
                }
                b'\n' => {
                    #[allow(clippy::cast_possible_truncation)]
                    starts.push(line_start as u32);
                    line_start = pos;
                }
                _ => {}
            }
        } else {
            // A multi-byte sequence. Only U+2028 and U+2029 break a line, and
            // both are three bytes, but decoding is what keeps `pos` on a
            // character boundary for everything else.
            let character = text[pos..].chars().next().unwrap_or('\u{FFFD}');
            pos += character.len_utf8();
            if matches!(character, '\u{2028}' | '\u{2029}') {
                #[allow(clippy::cast_possible_truncation)]
                starts.push(line_start as u32);
                line_start = pos;
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    starts.push(line_start as u32);
    starts
}

/// Which line `position` falls on (`ComputeLineOfPosition`), zero-based.
///
/// A binary search, as upstream's is. A position past the end answers the last
/// line rather than panicking, which matters because a diagnostic may be
/// positioned at end-of-file.
#[must_use]
pub fn compute_line_of_position(line_starts: &[u32], position: u32) -> u32 {
    match line_starts.binary_search(&position) {
        Ok(line) => {
            #[allow(clippy::cast_possible_truncation)]
            let line = line as u32;
            line
        }
        // `Err(i)` is where it would be inserted, so the line containing it is
        // the one before. `i` is never 0: every line map starts at offset 0, and
        // an offset below 0 does not exist.
        Err(insertion) => {
            #[allow(clippy::cast_possible_truncation)]
            let line = insertion.saturating_sub(1) as u32;
            line
        }
    }
}

/// How many UTF-16 code units `text` encodes to (`core.UTF16Len`).
///
/// TypeScript's character offsets are UTF-16 code units because the language's
/// own string type is, and every baseline and every editor protocol inherits
/// that. Counting `char`s agrees on ASCII and drifts on anything astral: an emoji
/// is one `char` and two code units.
#[must_use]
pub fn utf16_len(text: &str) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let length = text.chars().map(char::len_utf16).sum::<usize>() as u32;
    length
}

/// Zero-based line and UTF-16 character for a byte offset
/// (`GetECMALineAndUTF16CharacterOfPosition`).
///
/// **Zero-based, as upstream's is.** Every caller that prints a position adds one
/// to both, which is why `tsc` reports `a.ts(1,1)` for offset 0.
///
/// An offset that lands inside a multi-byte character, or past the end of the
/// text, is clamped to the enclosing line's start rather than panicking — a
/// diagnostic with a bad span should print in the wrong place, not take the
/// compiler down.
#[must_use]
pub fn line_and_character(text: &str, line_starts: &[u32], position: u32) -> (u32, u32) {
    let line = compute_line_of_position(line_starts, position);
    let start = line_starts.get(line as usize).copied().unwrap_or(0) as usize;
    let end = (position as usize).min(text.len()).max(start);
    let character = text.get(start..end).map_or(0, utf16_len);
    (line, character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_feed_starts_a_line() {
        assert_eq!(ecma_line_starts("a\nb\nc"), [0, 2, 4]);
    }

    #[test]
    fn an_empty_text_is_one_line() {
        // Total-ness depends on this: `line_starts[0]` must always exist.
        assert_eq!(ecma_line_starts(""), [0]);
    }

    #[test]
    fn a_trailing_break_starts_an_empty_last_line() {
        assert_eq!(ecma_line_starts("a\n"), [0, 2]);
    }

    #[test]
    fn a_carriage_return_pair_is_one_break() {
        // The bug a `\n`-only implementation has: `a\r\nb` is two lines, and
        // counting `\r` and `\n` separately makes it three.
        assert_eq!(ecma_line_starts("a\r\nb"), [0, 3]);
    }

    #[test]
    fn a_lone_carriage_return_is_a_break() {
        // Classic-Mac line endings, still present in the corpus.
        assert_eq!(ecma_line_starts("a\rb"), [0, 2]);
    }

    #[test]
    fn the_ecmascript_separators_break_lines() {
        // Three bytes each, and invisible in most editors.
        assert_eq!(ecma_line_starts("a\u{2028}b"), [0, 4]);
        assert_eq!(ecma_line_starts("a\u{2029}b"), [0, 4]);
        // A non-ASCII character that is *not* a separator must not.
        assert_eq!(ecma_line_starts("é"), [0]);
    }

    #[test]
    fn a_character_is_counted_in_utf16_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        // Two bytes, one code unit.
        assert_eq!(utf16_len("é"), 1);
        // Four bytes, one `char`, two code units — the case that makes this
        // different from `chars().count()`.
        assert_eq!(utf16_len("😀"), 2);
    }

    #[test]
    fn a_position_maps_to_its_line_and_character() {
        let text = "let a = 1;\nlet b = 2;\n";
        let starts = ecma_line_starts(text);

        assert_eq!(line_and_character(text, &starts, 0), (0, 0));
        assert_eq!(line_and_character(text, &starts, 4), (0, 4));
        // The offset of the `\n` itself is still on the first line.
        assert_eq!(line_and_character(text, &starts, 10), (0, 10));
        // The first byte after it is the second line's character 0.
        assert_eq!(line_and_character(text, &starts, 11), (1, 0));
        assert_eq!(line_and_character(text, &starts, 15), (1, 4));
    }

    #[test]
    fn an_astral_character_shifts_the_column_by_two() {
        let text = "const a = '😀';";
        let starts = ecma_line_starts(text);
        // The `'` closing the string is 4 bytes past the emoji's start but 2
        // UTF-16 units past it, which is what `tsc` would report.
        let emoji_start = u32::try_from(text.find('😀').unwrap()).unwrap();
        assert_eq!(line_and_character(text, &starts, emoji_start), (0, 11));
        assert_eq!(line_and_character(text, &starts, emoji_start + 4), (0, 13));
    }

    #[test]
    fn a_position_past_the_end_lands_on_the_last_line() {
        // Reachable: a diagnostic positioned at end-of-file.
        let text = "a\nb";
        let starts = ecma_line_starts(text);
        assert_eq!(line_and_character(text, &starts, 3), (1, 1));
        assert_eq!(line_and_character(text, &starts, 99), (1, 1));
    }
}
