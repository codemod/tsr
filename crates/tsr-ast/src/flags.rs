//! Node, token, and modifier flag sets.
//!
//! Ported from typescript-go's `internal/ast/nodeflags.go`,
//! `internal/ast/tokenflags.go`, and `internal/ast/modifierflags.go`. Bit values
//! are load-bearing — TypeScript serializes some of them and the checker does
//! range and mask tests — so they are written out explicitly rather than derived
//! from declaration order.

use bitflags::bitflags;

bitflags! {
    /// Flags attached to every node.
    ///
    /// Corresponds to typescript-go's `ast.NodeFlags`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct NodeFlags: u32 {
        /// `let` variable declaration.
        const LET = 1 << 0;
        /// `const` variable declaration.
        const CONST = 1 << 1;
        /// `using` variable declaration.
        const USING = 1 << 2;
        /// Node was synthesized during parsing.
        const REPARSED = 1 << 3;
        /// Node was synthesized during transformation.
        const SYNTHESIZED = 1 << 4;
        /// Chained member expression rooted at a pseudo-optional expression.
        const OPTIONAL_CHAIN = 1 << 5;
        /// Export context, initialized by binding.
        const EXPORT_CONTEXT = 1 << 6;
        /// Interface contains references to `this`.
        const CONTAINS_THIS = 1 << 7;
        /// Function implicitly returns on one of its code paths.
        const HAS_IMPLICIT_RETURN = 1 << 8;
        /// Function has an explicit reachable return on one of its code paths.
        const HAS_EXPLICIT_RETURN = 1 << 9;
        /// Parsed where `in` expressions are not allowed.
        const DISALLOW_IN_CONTEXT = 1 << 10;
        /// Parsed in the `yield` context of a generator.
        const YIELD_CONTEXT = 1 << 11;
        /// Parsed as part of a decorator.
        const DECORATOR_CONTEXT = 1 << 12;
        /// Parsed in the `await` context of an async function.
        const AWAIT_CONTEXT = 1 << 13;
        /// Parsed where conditional types are not allowed.
        const DISALLOW_CONDITIONAL_TYPES_CONTEXT = 1 << 14;
        /// The parser errored while parsing the code that created this node.
        const THIS_NODE_HAS_ERROR = 1 << 15;
        /// Parsed in a JavaScript file.
        const JAVASCRIPT_FILE = 1 << 16;
        /// This node or one of its descendants had an error.
        const THIS_NODE_OR_ANY_SUB_NODES_HAS_ERROR = 1 << 17;
        /// File has async functions, initialized by binding.
        const HAS_ASYNC_FUNCTIONS = 1 << 18;
        /// File possibly contains a dynamic `import()`.
        ///
        /// Approximate and never cleared: incremental parsing does not reset it
        /// when the import is removed. Upstream documents this as a deliberate
        /// simplicity-for-precision trade.
        const POSSIBLY_CONTAINS_DYNAMIC_IMPORT = 1 << 19;
        /// File possibly contains `import.meta`. Approximate, as above.
        const POSSIBLY_CONTAINS_IMPORT_META = 1 << 20;
        /// Node has preceding JSDoc comments.
        const HAS_JSDOC = 1 << 21;
        /// Node was parsed inside JSDoc.
        const JSDOC = 1 << 22;
        /// Node was inside an ambient context.
        const AMBIENT = 1 << 23;
        /// An ancestor was the statement of a `with` statement.
        const IN_WITH_STATEMENT = 1 << 24;
        /// Parsed in a JSON file.
        const JSON_FILE = 1 << 25;
        /// Comment text contained `@deprecated`; must be confirmed via JSDoc lookup.
        const POSSIBLY_CONTAINS_DEPRECATED_TAG = 1 << 26;
        /// Node is unreachable according to the binder.
        const UNREACHABLE = 1 << 27;
        /// Node was transformed during parsing, so its source text no longer
        /// matches the AST naively.
        const REPARSER_TRANSFORMED_LITERAL = 1 << 28;
        /// A list-like node was written with a comma before its closing token.
        ///
        /// Upstream stores this on `NodeArray.HasTrailingComma`; this AST keeps
        /// child slices plain, so the enclosing node carries the same fact.
        const HAS_TRAILING_COMMA = 1 << 29;
        /// The file is a DEFAULT LIBRARY file (`lib.*.d.ts`), stamped by the
        /// loader from `default_library_path` exactly as
        /// [`NodeFlags::JAVASCRIPT_FILE`] is stamped from the extension —
        /// upstream's `SourceFile.LibReferenceDirectives`-adjacent
        /// `isDefaultLib` bit, which the node builder consults when choosing
        /// reference spellings (printseam §7's missing key).
        const DEFAULT_LIBRARY = 1 << 30;
        /// The node's type parameter or type argument list was written as
        /// empty brackets, `<>`.
        ///
        /// Upstream tells `class C<>` from `class C` by the list being a
        /// non-nil `NodeList` with no nodes (`checkGrammarTypeParameterList`,
        /// `checkGrammarForAtLeastOneTypeArgument`); this AST keeps the list as
        /// a plain slice, so the owner carries the fact. No owner kind has both
        /// a type parameter and a type argument list, so one bit serves both.
        /// The brackets' positions are recovered from the source text.
        const EMPTY_TYPE_LIST = 1 << 31;

        /// Any block-scoped declaration form.
        const BLOCK_SCOPED = Self::LET.bits() | Self::CONST.bits() | Self::USING.bits();
        /// Any constant declaration form.
        const CONSTANT = Self::CONST.bits() | Self::USING.bits();
    }
}

bitflags! {
    /// Flags recorded by the scanner about how a token was written.
    ///
    /// Corresponds to typescript-go's `ast.TokenFlags`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TokenFlags: u32 {
        /// Token had a preceding line break.
        const PRECEDING_LINE_BREAK = 1 << 0;
        /// Token had a preceding JSDoc comment.
        const PRECEDING_JSDOC_COMMENT = 1 << 1;
        /// Token contains no source text (synthesized).
        const UNTERMINATED = 1 << 2;
        /// Template literal contains an extended escape.
        const EXTENDED_UNICODE_ESCAPE = 1 << 3;
        /// Numeric literal was written in scientific notation.
        const SCIENTIFIC = 1 << 4;
        /// Numeric literal was written in octal.
        const OCTAL = 1 << 5;
        /// Numeric literal was written in hexadecimal.
        const HEX_SPECIFIER = 1 << 6;
        /// Numeric literal was written in binary.
        const BINARY_SPECIFIER = 1 << 7;
        /// Numeric literal was written in octal with the `0o` prefix.
        const OCTAL_SPECIFIER = 1 << 8;
        /// Numeric literal contains `_` separators.
        const CONTAINS_SEPARATOR = 1 << 9;
        /// Identifier contains a unicode escape.
        const UNICODE_ESCAPE = 1 << 10;
        /// Identifier contains an invalid escape sequence.
        const CONTAINS_INVALID_ESCAPE = 1 << 11;
        /// Leading `*` on a JSDoc continuation line was skipped before this token.
        const PRECEDING_JSDOC_LEADING_ASTERISKS = 1 << 15;
        /// String literal was delimited by single quotes.
        const SINGLE_QUOTE = 1 << 16;
        /// The preceding JSDoc comment mentions `@deprecated`.
        const PRECEDING_JSDOC_WITH_DEPRECATED = 1 << 17;
        /// The preceding JSDoc comment mentions `@see`, `@link`, `@linkcode`, or
        /// `@linkplain`.
        const PRECEDING_JSDOC_WITH_SEE_OR_LINK = 1 << 18;
        /// The literal is a hex, binary or octal specifier.
        const BINARY_OR_OCTAL_SPECIFIER =
            Self::BINARY_SPECIFIER.bits() | Self::OCTAL_SPECIFIER.bits();
        /// Any numeric-literal representation flag.
        const NUMERIC_LITERAL_FLAGS = Self::SCIENTIFIC.bits()
            | Self::OCTAL.bits()
            | Self::HEX_SPECIFIER.bits()
            | Self::BINARY_OR_OCTAL_SPECIFIER.bits()
            | Self::CONTAINS_SEPARATOR.bits();
    }
}

bitflags! {
    /// Modifiers applied to a declaration.
    ///
    /// Corresponds to typescript-go's `ast.ModifierFlags`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct ModifierFlags: u32 {
        /// `public`.
        const PUBLIC = 1 << 0;
        /// `private`.
        const PRIVATE = 1 << 1;
        /// `protected`.
        const PROTECTED = 1 << 2;
        /// `readonly`.
        const READONLY = 1 << 3;
        /// `override`.
        const OVERRIDE = 1 << 4;
        /// `export`.
        const EXPORT = 1 << 5;
        /// `abstract`.
        const ABSTRACT = 1 << 6;
        /// `ambient`, i.e. `declare`.
        const AMBIENT = 1 << 7;
        /// `static`.
        const STATIC = 1 << 8;
        /// `accessor`.
        const ACCESSOR = 1 << 9;
        /// `async`.
        const ASYNC = 1 << 10;
        /// `default`.
        const DEFAULT = 1 << 11;
        /// `const` on an enum.
        const CONST = 1 << 12;
        /// `in` variance annotation.
        const IN = 1 << 13;
        /// `out` variance annotation.
        const OUT = 1 << 14;
        /// A decorator is present.
        const DECORATOR = 1 << 15;
        /// `immediate` phase modifier.
        const IMMEDIATE = 1 << 16;
        /// `defer` phase modifier.
        const DEFERRED = 1 << 17;

        /// The three accessibility modifiers.
        const ACCESSIBILITY_MODIFIER =
            Self::PUBLIC.bits() | Self::PRIVATE.bits() | Self::PROTECTED.bits();
        /// Variance annotations.
        const MODIFIER = Self::IN.bits() | Self::OUT.bits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_flags_match_their_parts() {
        assert_eq!(NodeFlags::BLOCK_SCOPED, NodeFlags::LET | NodeFlags::CONST | NodeFlags::USING);
        assert_eq!(NodeFlags::CONSTANT, NodeFlags::CONST | NodeFlags::USING);
        assert_eq!(
            ModifierFlags::ACCESSIBILITY_MODIFIER,
            ModifierFlags::PUBLIC | ModifierFlags::PRIVATE | ModifierFlags::PROTECTED
        );
    }

    #[test]
    fn bit_positions_match_upstream() {
        // Spot-check values the checker masks against; a renumbering here would
        // silently change semantics.
        assert_eq!(NodeFlags::LET.bits(), 1);
        assert_eq!(NodeFlags::JAVASCRIPT_FILE.bits(), 1 << 16);
        assert_eq!(NodeFlags::REPARSER_TRANSFORMED_LITERAL.bits(), 1 << 28);
        assert_eq!(ModifierFlags::EXPORT.bits(), 1 << 5);
    }
}
