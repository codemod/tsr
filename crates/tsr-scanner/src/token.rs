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
        /// Trivia before this token contained a `/** … */` comment.
        const PRECEDING_JSDOC_COMMENT = 1 << 1;
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
        /// A string or template contained an invalid escape — a legacy octal
        /// (`\55`) or decimal (`\8`) escape (§147, `checker-notes-narrow.md`).
        const CONTAINS_INVALID_ESCAPE = 1 << 11;
        /// Leading `*` on a JSDoc continuation line was skipped before this token.
        const PRECEDING_JSDOC_LEADING_ASTERISKS = 1 << 15;
        /// A string literal was delimited by single quotes.
        const SINGLE_QUOTE = 1 << 16;
        /// The preceding JSDoc comment mentions `@deprecated`.
        ///
        /// Found by a cheap substring scan during trivia, not by parsing: it lets
        /// the parser flag a node as *possibly* deprecated without paying for the
        /// JSDoc parse. Callers must confirm by looking at the parsed tags.
        const PRECEDING_JSDOC_WITH_DEPRECATED = 1 << 17;
        /// The preceding JSDoc comment mentions `@see`, `@link`, `@linkcode`, or
        /// `@linkplain`. Same cheap scan as
        /// [`TokenFlags::PRECEDING_JSDOC_WITH_DEPRECATED`].
        const PRECEDING_JSDOC_WITH_SEE_OR_LINK = 1 << 18;
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

impl Token {
    /// The AST's view of these flags.
    ///
    /// `tsr_ast::TokenFlags` is the wider set the parser and checker share; the
    /// scanner defines only the subset it can determine. Bit positions are
    /// deliberately identical — asserted below — so this is a reinterpretation
    /// rather than a mapping that could drift.
    #[must_use]
    pub const fn ast_flags(self) -> tsr_ast::TokenFlags {
        tsr_ast::TokenFlags::from_bits_truncate(self.flags.bits())
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
    fn scanner_and_ast_flag_bits_agree() {
        // `ast_flags` reinterprets the bits, so divergence would silently relabel
        // flags rather than fail to compile.
        use tsr_ast::TokenFlags as Ast;
        assert_eq!(TokenFlags::PRECEDING_LINE_BREAK.bits(), Ast::PRECEDING_LINE_BREAK.bits());
        assert_eq!(TokenFlags::UNTERMINATED.bits(), Ast::UNTERMINATED.bits());
        assert_eq!(TokenFlags::SCIENTIFIC.bits(), Ast::SCIENTIFIC.bits());
        assert_eq!(TokenFlags::HEX_SPECIFIER.bits(), Ast::HEX_SPECIFIER.bits());
        assert_eq!(TokenFlags::BINARY_SPECIFIER.bits(), Ast::BINARY_SPECIFIER.bits());
        assert_eq!(TokenFlags::OCTAL_SPECIFIER.bits(), Ast::OCTAL_SPECIFIER.bits());
        assert_eq!(TokenFlags::CONTAINS_SEPARATOR.bits(), Ast::CONTAINS_SEPARATOR.bits());
        assert_eq!(TokenFlags::UNICODE_ESCAPE.bits(), Ast::UNICODE_ESCAPE.bits());
        assert_eq!(TokenFlags::CONTAINS_INVALID_ESCAPE.bits(), Ast::CONTAINS_INVALID_ESCAPE.bits());
        assert_eq!(TokenFlags::SINGLE_QUOTE.bits(), Ast::SINGLE_QUOTE.bits());
        assert_eq!(TokenFlags::PRECEDING_JSDOC_COMMENT.bits(), Ast::PRECEDING_JSDOC_COMMENT.bits());
        assert_eq!(
            TokenFlags::PRECEDING_JSDOC_LEADING_ASTERISKS.bits(),
            Ast::PRECEDING_JSDOC_LEADING_ASTERISKS.bits()
        );
        assert_eq!(
            TokenFlags::PRECEDING_JSDOC_WITH_DEPRECATED.bits(),
            Ast::PRECEDING_JSDOC_WITH_DEPRECATED.bits()
        );
        assert_eq!(
            TokenFlags::PRECEDING_JSDOC_WITH_SEE_OR_LINK.bits(),
            Ast::PRECEDING_JSDOC_WITH_SEE_OR_LINK.bits()
        );
    }

    #[test]
    fn token_is_small_enough_to_pass_by_value() {
        // A token is copied on every parser step; keep it register-sized.
        assert!(size_of::<Token>() <= 16, "Token grew to {} bytes", size_of::<Token>());
    }
}
