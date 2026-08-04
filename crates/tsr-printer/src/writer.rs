//! The output buffer.
//!
//! Ported from typescript-go's `textWriter` (`internal/printer/textwriter.go`).
//!
//! The load-bearing behaviour is **deferred indentation**: `write_line` records
//! that a line ended but emits no indent, and the next `write_text` emits the
//! indent before its content. That is why a blank line carries no trailing
//! whitespace and why `increase_indent` between a `write_line` and the next write
//! still takes effect.
//!
//! Not ported: the source-map bookkeeping (`lineCount`, `linePos`), the symbol and
//! comment write kinds, `WriteLineForce`, and `IsAtStartOfLine`. Each belongs to
//! emit rather than to the round trip. Upstream's `WriteKind` enum is also absent:
//! it exists so a language service can colour each token differently, and with one
//! output target every kind writes the same bytes. `Printer::write_keyword` and
//! friends keep the vocabulary at the call sites, which is where it reads.

/// Upstream's `defaultIndentSize` (`internal/printer/textwriter.go`).
const DEFAULT_INDENT_SIZE: usize = 4;

/// Ported from typescript-go's `textWriter` (`internal/printer/textwriter.go`).
#[derive(Debug, Default)]
pub(crate) struct TextWriter {
    builder: String,
    indent: usize,
    /// Whether the buffer is positioned at the start of a line, so the next write
    /// must emit the indent first.
    line_start: bool,
}

impl TextWriter {
    /// A fresh writer, positioned **at the start of a line**.
    ///
    /// That is not a detail. Upstream's `textWriter.Clear` sets `lineStart: true`
    /// (`textwriter.go:27`), so `emitSourceFile`'s opening `p.writeLine()`
    /// (`printer.go:4637`) is a no-op on an empty buffer. Started at `false`, the
    /// same call emits a newline and **every printed file gains a leading blank
    /// line**.
    ///
    /// The round trip cannot see this — leading whitespace is not in the tree, so
    /// 11,726 cases passed with it — and it made `dts_emit` fail its first 289
    /// cases on line 1 before a single declaration was compared. A gate that only
    /// checks structure cannot check position.
    pub(crate) fn new() -> Self {
        Self { builder: String::new(), indent: 0, line_start: true }
    }

    /// Ported from `textWriter.writeText`.
    fn write_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.line_start {
            for _ in 0..self.indent * DEFAULT_INDENT_SIZE {
                self.builder.push(' ');
            }
            self.line_start = false;
        }
        self.builder.push_str(text);
    }

    /// Ported from `textWriter.Write`.
    pub(crate) fn write(&mut self, text: &str) {
        self.write_text(text);
    }

    /// Ported from `textWriter.RawWrite`, which bypasses indentation.
    pub(crate) fn raw_write(&mut self, text: &str) {
        self.builder.push_str(text);
        self.line_start = false;
    }

    /// Ported from `textWriter.WriteLine`.
    ///
    /// Idempotent, exactly as upstream: writing two line breaks in a row produces
    /// one. Callers that want a blank line ask for it explicitly.
    pub(crate) fn write_line(&mut self) {
        if !self.line_start {
            self.builder.push('\n');
            self.line_start = true;
        }
    }

    /// Ported from `textWriter.IncreaseIndent`.
    pub(crate) fn increase_indent(&mut self) {
        self.indent += 1;
    }

    /// Ported from `textWriter.DecreaseIndent`.
    pub(crate) fn decrease_indent(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    /// The last character written, for the separator check.
    ///
    /// **A deviation, and a deliberate one.** Upstream has no such accessor: it
    /// places every space by hand through `writeSpace()` and is correct because
    /// TypeScript's emitter has been exercised for a decade. Ported here, that
    /// design let `1 .toString()` print as `1.toString()` — the guard existed and
    /// 123 call sites bypassed it. Keeping the check as a backstop costs nothing,
    /// because whitespace is not in the tree.
    pub(crate) fn last_char(&self) -> Option<char> {
        self.builder.chars().last()
    }

    /// Ported from `textWriter.String`.
    pub(crate) fn into_string(self) -> String {
        self.builder
    }

    /// Ported from `textWriter.GetTextPos`.
    pub(crate) fn text_pos(&self) -> usize {
        self.builder.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_is_deferred_to_the_next_write() {
        // The indent belongs to the content, not to the line break, so a blank
        // line carries no trailing whitespace.
        let mut writer = TextWriter::new();
        writer.write("a");
        writer.write_line();
        writer.increase_indent();
        writer.write_line();
        writer.write("b");
        assert_eq!(writer.into_string(), "a\n    b");
    }

    #[test]
    fn write_line_is_idempotent() {
        // Upstream's `WriteLine` is a no-op at the start of a line; emit relies on
        // being able to ask for a break without tracking whether one just happened.
        let mut writer = TextWriter::new();
        writer.write("a");
        writer.write_line();
        writer.write_line();
        writer.write("b");
        assert_eq!(writer.into_string(), "a\nb");
    }

    #[test]
    fn decreasing_indent_below_zero_does_not_wrap() {
        let mut writer = TextWriter::new();
        writer.decrease_indent();
        writer.write_line();
        writer.write("a");
        assert_eq!(writer.into_string(), "a");
    }

    #[test]
    fn a_fresh_writer_is_already_at_the_start_of_a_line() {
        // This assertion used to read `"\na"`, encoding the state upstream's
        // `textWriter.Clear` does *not* leave the writer in. The consequence was a
        // leading blank line on every printed file — invisible to the round trip,
        // and the first thing a byte comparison found. See [`TextWriter::new`].
        let mut writer = TextWriter::new();
        writer.write_line();
        writer.write("a");
        assert_eq!(writer.into_string(), "a");
    }
}
