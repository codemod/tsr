//! Source positions.
//!
//! Corresponds to typescript-go's `core.TextRange` (`internal/core/text.go`).
//! Like oxc's `Span`, positions are **byte** offsets into UTF-8 source text, held
//! as `u32`: line/column is a presentation concern computed on demand for
//! diagnostics, never carried on every node.

use std::fmt;

/// A half-open byte range `[start, end)` into a source file.
///
/// TypeScript represents "no position" as `-1`; we use [`Span::UNDEFINED`], which
/// keeps the type unsigned and the common case branch-free.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Byte offset of the first byte in the range.
    pub start: u32,
    /// Byte offset one past the last byte in the range.
    pub end: u32,
}

impl Span {
    /// The span used for synthesized nodes that have no source text.
    ///
    /// Mirrors typescript-go's `core.UndefinedTextRange()`, which uses `-1`.
    pub const UNDEFINED: Self = Self { start: u32::MAX, end: u32::MAX };

    /// Create a span covering `[start, end)`.
    #[inline]
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Create an empty span at `pos`.
    #[inline]
    #[must_use]
    pub const fn at(pos: u32) -> Self {
        Self { start: pos, end: pos }
    }

    /// Whether this span refers to real source text.
    #[inline]
    #[must_use]
    pub const fn is_defined(self) -> bool {
        self.start != u32::MAX
    }

    /// Number of bytes covered.
    #[inline]
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span covers no bytes.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// Whether `offset` falls within `[start, end)`.
    #[inline]
    #[must_use]
    pub const fn contains(self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Whether `other` is entirely within `self`.
    #[inline]
    #[must_use]
    pub const fn contains_span(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The smallest span covering both inputs.
    #[inline]
    #[must_use]
    pub fn cover(self, other: Self) -> Self {
        match (self.is_defined(), other.is_defined()) {
            (true, true) => Self::new(self.start.min(other.start), self.end.max(other.end)),
            (true, false) => self,
            _ => other,
        }
    }

    /// Index the source text this span refers to.
    ///
    /// Returns `None` when the span is undefined or out of bounds, so a bad span
    /// from a synthesized node cannot panic a diagnostic renderer.
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        if !self.is_defined() {
            return None;
        }
        source.get(self.start as usize..self.end as usize)
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_defined() {
            write!(f, "{}..{}", self.start, self.end)
        } else {
            f.write_str("<undefined>")
        }
    }
}

/// Anything with a source position.
pub trait GetSpan {
    /// The node's full span, including leading trivia handling done by the parser.
    fn span(&self) -> Span;
}

impl GetSpan for Span {
    #[inline]
    fn span(&self) -> Span {
        *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undefined_spans_do_not_index_source() {
        assert_eq!(Span::UNDEFINED.text("abc"), None);
        assert!(!Span::UNDEFINED.is_defined());
    }

    #[test]
    fn cover_ignores_undefined_operands() {
        let a = Span::new(2, 5);
        assert_eq!(a.cover(Span::UNDEFINED), a);
        assert_eq!(Span::UNDEFINED.cover(a), a);
        assert_eq!(a.cover(Span::new(7, 9)), Span::new(2, 9));
    }

    #[test]
    fn out_of_bounds_span_returns_none_rather_than_panicking() {
        assert_eq!(Span::new(0, 99).text("abc"), None);
        assert_eq!(Span::new(1, 3).text("abc"), Some("bc"));
    }
}
