//! Tokens and the flags the scanner records about them.

use tsr_ast::SyntaxKind;
use tsr_core::{GetSpan, Span};

bitflags::bitflags! {
    /// Facts about a token that its kind and span do not capture.
    ///
    /// Corresponds to typescript-go's `ast.TokenFlags`
    /// (`internal/ast/tokenflags.go`). Only the subset the scanner can determine
    /// is defined here; the parser adds more.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TokenFlags: u32 {
        /// Trivia before this token contained a line break.
        ///
        /// Load-bearing for automatic semicolon insertion, which is why it rides
        /// on the token rather than requiring a separate trivia scan.
        const PRECEDING_LINE_BREAK = 1 << 0;
        /// The literal was not closed before end of file or line.
        const UNTERMINATED = 1 << 2;
        /// An identifier or string contained a `\u` escape.
        const UNICODE_ESCAPE = 1 << 10;
        /// A numeric literal used exponent notation.
        const SCIENTIFIC = 1 << 4;
        /// A numeric literal was written `0x…`.
        const HEX_SPECIFIER = 1 << 6;
        /// A numeric literal was written `0b…`.
        const BINARY_SPECIFIER = 1 << 7;
        /// A numeric literal was written `0o…`.
        const OCTAL_SPECIFIER = 1 << 8;
        /// A numeric literal contained `_` separators.
        const CONTAINS_SEPARATOR = 1 << 9;
    }
}

/// A scanned token: what it is, where it is, and what the scanner noticed.
///
/// `Copy` and 12 bytes, so passing it by value is free — the parser holds the
/// current token in a register rather than behind a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// Which token this is.
    pub kind: SyntaxKind,
    /// Its extent in the source, excluding leading trivia.
    pub span: Span,
    /// What the scanner noticed while producing it.
    pub flags: TokenFlags,
}

impl Token {
    /// Create a token.
    #[must_use]
    pub const fn new(kind: SyntaxKind, span: Span, flags: TokenFlags) -> Self {
        Self { kind, span, flags }
    }

    /// Whether trivia before this token contained a line break.
    #[must_use]
    pub const fn has_preceding_line_break(self) -> bool {
        self.flags.contains(TokenFlags::PRECEDING_LINE_BREAK)
    }

    /// Whether the token is a literal that ran off the end of its line or file.
    #[must_use]
    pub const fn is_unterminated(self) -> bool {
        self.flags.contains(TokenFlags::UNTERMINATED)
    }
}

impl GetSpan for Token {
    fn span(&self) -> Span {
        self.span
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_small_enough_to_pass_by_value() {
        // A token is copied on every parser step; keep it register-sized.
        assert!(size_of::<Token>() <= 16, "Token grew to {} bytes", size_of::<Token>());
    }
}
