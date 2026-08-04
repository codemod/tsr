//! How a list of children is laid out.
//!
//! Ported from typescript-go's `ListFormat` (`internal/printer/printer.go`, the
//! `LF*` constants and `getOpeningBracket`/`getClosingBracket`).
//!
//! This is the piece that makes the printer a printer rather than a pile of loops.
//! Upstream emits *every* child list — parameters, members, arguments, union
//! constituents, JSX children — through one `emitList` driven by these flags, and
//! the named combinations at the bottom are the actual specification of how each
//! construct is laid out. Reproducing them is most of what "structurally faithful"
//! means here, because it is where upstream's formatting decisions live.
//!
//! Two flags are accepted and ignored, and it matters that this is stated rather
//! than discovered: [`ListFormat::PRESERVE_LINES`] and
//! [`ListFormat::PREFER_NEW_LINE`] depend on the original node positions, which
//! upstream consults to keep the author's line breaks. This printer does not
//! preserve source layout — the round trip compares trees, not text — so both
//! degrade to `SINGLE_LINE`. Phase 5 needs them for real.

use bitflags::bitflags;

bitflags! {
    /// Ported from typescript-go's `ListFormat` (`internal/printer/printer.go`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct ListFormat: u32 {
        // Line terminators.
        /// `LFSingleLine` — the default; the list prints on one line.
        const SINGLE_LINE = 0;
        /// `LFMultiLine`.
        const MULTI_LINE = 1 << 0;
        /// `LFPreserveLines` — accepted and ignored; see the module docs.
        const PRESERVE_LINES = 1 << 1;
        /// `LFLinesMask`.
        const LINES_MASK =
            Self::SINGLE_LINE.bits() | Self::MULTI_LINE.bits() | Self::PRESERVE_LINES.bits();

        // Delimiters.
        /// `LFNotDelimited`.
        const NOT_DELIMITED = 0;
        /// `LFBarDelimited`.
        const BAR_DELIMITED = 1 << 2;
        /// `LFAmpersandDelimited`.
        const AMPERSAND_DELIMITED = 1 << 3;
        /// `LFCommaDelimited`.
        const COMMA_DELIMITED = 1 << 4;
        /// `LFAsteriskDelimited` — JSDoc only, and JSDoc is not emitted here.
        const ASTERISK_DELIMITED = 1 << 5;
        /// `LFDelimitersMask`.
        const DELIMITERS_MASK = Self::BAR_DELIMITED.bits()
            | Self::AMPERSAND_DELIMITED.bits()
            | Self::COMMA_DELIMITED.bits()
            | Self::ASTERISK_DELIMITED.bits();

        /// `LFAllowTrailingComma`.
        const ALLOW_TRAILING_COMMA = 1 << 6;

        /// `LFIndented`.
        const INDENTED = 1 << 7;
        /// `LFSpaceBetweenBraces`.
        const SPACE_BETWEEN_BRACES = 1 << 8;
        /// `LFSpaceBetweenSiblings`.
        const SPACE_BETWEEN_SIBLINGS = 1 << 9;

        // Brackets and braces.
        /// `LFBraces`.
        const BRACES = 1 << 10;
        /// `LFParenthesis`.
        const PARENTHESIS = 1 << 11;
        /// `LFAngleBrackets`.
        const ANGLE_BRACKETS = 1 << 12;
        /// `LFSquareBrackets`.
        const SQUARE_BRACKETS = 1 << 13;
        /// `LFBracketsMask`.
        const BRACKETS_MASK = Self::BRACES.bits()
            | Self::PARENTHESIS.bits()
            | Self::ANGLE_BRACKETS.bits()
            | Self::SQUARE_BRACKETS.bits();

        /// `LFOptionalIfNil`.
        const OPTIONAL_IF_NIL = 1 << 14;
        /// `LFOptionalIfEmpty`.
        const OPTIONAL_IF_EMPTY = 1 << 15;
        /// `LFOptional`.
        const OPTIONAL = Self::OPTIONAL_IF_NIL.bits() | Self::OPTIONAL_IF_EMPTY.bits();

        /// `LFPreferNewLine` — accepted and ignored; see the module docs.
        const PREFER_NEW_LINE = 1 << 16;
        /// `LFNoTrailingNewLine`.
        const NO_TRAILING_NEW_LINE = 1 << 17;
        /// `LFNoInterveningComments` — comments are not emitted, so this is inert.
        const NO_INTERVENING_COMMENTS = 1 << 18;
        /// `LFNoSpaceIfEmpty`.
        const NO_SPACE_IF_EMPTY = 1 << 19;
        /// `LFSingleElement`.
        const SINGLE_ELEMENT = 1 << 20;
        /// `LFSpaceAfterList`.
        const SPACE_AFTER_LIST = 1 << 21;
    }
}

/// The named combinations, ported one-for-one from upstream's block.
///
/// Each is upstream's own layout decision for that construct. Where this printer
/// deviates it does so by *not consulting* a flag (see the module docs), never by
/// changing which flags a construct carries.
///
/// Deliberately complete, so some are unused: the value of a ported table is that
/// it answers "how does upstream lay this out?" for every construct, including the
/// ones whose emit sites have not been converted from hand-rolled loops yet. A
/// half-table would have to be re-derived from `printer.go` each time one is, and
/// the drift tracker would have nothing to compare against for the rest.
#[allow(dead_code)]
impl ListFormat {
    /// `LFModifiers`.
    pub(crate) const MODIFIERS: Self = Self::SINGLE_LINE
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::NO_INTERVENING_COMMENTS)
        .union(Self::SPACE_AFTER_LIST);
    /// `LFHeritageClauses`.
    pub(crate) const HERITAGE_CLAUSES: Self = Self::SINGLE_LINE.union(Self::SPACE_BETWEEN_SIBLINGS);
    /// `LFMultiLineTypeLiteralMembers`.
    pub(crate) const MULTI_LINE_TYPE_LITERAL_MEMBERS: Self =
        Self::MULTI_LINE.union(Self::INDENTED).union(Self::OPTIONAL_IF_EMPTY);
    /// `LFSingleLineTupleTypeElements`.
    pub(crate) const SINGLE_LINE_TUPLE_TYPE_ELEMENTS: Self =
        Self::COMMA_DELIMITED.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::SINGLE_LINE);
    /// `LFUnionTypeConstituents`.
    pub(crate) const UNION_TYPE_CONSTITUENTS: Self =
        Self::BAR_DELIMITED.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::SINGLE_LINE);
    /// `LFIntersectionTypeConstituents`.
    pub(crate) const INTERSECTION_TYPE_CONSTITUENTS: Self =
        Self::AMPERSAND_DELIMITED.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::SINGLE_LINE);
    /// `LFObjectBindingPatternElements`.
    pub(crate) const OBJECT_BINDING_PATTERN_ELEMENTS: Self = Self::SINGLE_LINE
        .union(Self::ALLOW_TRAILING_COMMA)
        .union(Self::SPACE_BETWEEN_BRACES)
        .union(Self::COMMA_DELIMITED)
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::NO_SPACE_IF_EMPTY);
    /// `LFArrayBindingPatternElements`.
    pub(crate) const ARRAY_BINDING_PATTERN_ELEMENTS: Self = Self::SINGLE_LINE
        .union(Self::ALLOW_TRAILING_COMMA)
        .union(Self::COMMA_DELIMITED)
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::NO_SPACE_IF_EMPTY);
    /// `LFObjectLiteralExpressionProperties`.
    pub(crate) const OBJECT_LITERAL_EXPRESSION_PROPERTIES: Self = Self::PRESERVE_LINES
        .union(Self::COMMA_DELIMITED)
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::SPACE_BETWEEN_BRACES)
        .union(Self::INDENTED)
        .union(Self::BRACES)
        .union(Self::NO_SPACE_IF_EMPTY);
    /// `LFImportAttributes`.
    pub(crate) const IMPORT_ATTRIBUTES: Self = Self::OBJECT_LITERAL_EXPRESSION_PROPERTIES;
    /// `LFArrayLiteralExpressionElements`.
    pub(crate) const ARRAY_LITERAL_EXPRESSION_ELEMENTS: Self = Self::PRESERVE_LINES
        .union(Self::COMMA_DELIMITED)
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::ALLOW_TRAILING_COMMA)
        .union(Self::INDENTED)
        .union(Self::SQUARE_BRACKETS);
    /// `LFCallExpressionArguments`.
    pub(crate) const CALL_EXPRESSION_ARGUMENTS: Self = Self::COMMA_DELIMITED
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::SINGLE_LINE)
        .union(Self::PARENTHESIS);
    /// `LFNewExpressionArguments`.
    pub(crate) const NEW_EXPRESSION_ARGUMENTS: Self =
        Self::CALL_EXPRESSION_ARGUMENTS.union(Self::OPTIONAL_IF_NIL);
    /// `LFTemplateExpressionSpans`.
    pub(crate) const TEMPLATE_EXPRESSION_SPANS: Self =
        Self::SINGLE_LINE.union(Self::NO_INTERVENING_COMMENTS);
    /// `LFMultiLineBlockStatements`.
    pub(crate) const MULTI_LINE_BLOCK_STATEMENTS: Self = Self::INDENTED.union(Self::MULTI_LINE);
    /// `LFVariableDeclarationList`.
    pub(crate) const VARIABLE_DECLARATION_LIST: Self =
        Self::COMMA_DELIMITED.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::SINGLE_LINE);
    /// `LFMultiLineFunctionBodyStatements`.
    pub(crate) const MULTI_LINE_FUNCTION_BODY_STATEMENTS: Self = Self::MULTI_LINE;
    /// `LFClassMembers`.
    pub(crate) const CLASS_MEMBERS: Self = Self::INDENTED.union(Self::MULTI_LINE);
    /// `LFInterfaceMembers`.
    pub(crate) const INTERFACE_MEMBERS: Self = Self::INDENTED.union(Self::MULTI_LINE);
    /// `LFEnumMembers`.
    pub(crate) const ENUM_MEMBERS: Self =
        Self::COMMA_DELIMITED.union(Self::INDENTED).union(Self::MULTI_LINE);
    /// `LFCaseBlockClauses`.
    pub(crate) const CASE_BLOCK_CLAUSES: Self = Self::INDENTED.union(Self::MULTI_LINE);
    /// `LFNamedImportsOrExportsElements`.
    pub(crate) const NAMED_IMPORTS_OR_EXPORTS_ELEMENTS: Self = Self::COMMA_DELIMITED
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::ALLOW_TRAILING_COMMA)
        .union(Self::SINGLE_LINE)
        .union(Self::SPACE_BETWEEN_BRACES)
        .union(Self::NO_SPACE_IF_EMPTY);
    /// `LFJsxElementOrFragmentChildren`.
    pub(crate) const JSX_ELEMENT_OR_FRAGMENT_CHILDREN: Self =
        Self::SINGLE_LINE.union(Self::NO_INTERVENING_COMMENTS);
    /// `LFJsxElementAttributes`.
    pub(crate) const JSX_ELEMENT_ATTRIBUTES: Self =
        Self::SINGLE_LINE.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::NO_INTERVENING_COMMENTS);
    /// `LFCaseOrDefaultClauseStatements`.
    pub(crate) const CASE_OR_DEFAULT_CLAUSE_STATEMENTS: Self = Self::INDENTED
        .union(Self::MULTI_LINE)
        .union(Self::NO_TRAILING_NEW_LINE)
        .union(Self::OPTIONAL_IF_EMPTY);
    /// `LFHeritageClauseTypes`.
    pub(crate) const HERITAGE_CLAUSE_TYPES: Self =
        Self::COMMA_DELIMITED.union(Self::SPACE_BETWEEN_SIBLINGS).union(Self::SINGLE_LINE);
    /// `LFSourceFileStatements`.
    pub(crate) const SOURCE_FILE_STATEMENTS: Self =
        Self::MULTI_LINE.union(Self::NO_TRAILING_NEW_LINE);
    /// `LFDecorators`.
    pub(crate) const DECORATORS: Self =
        Self::MULTI_LINE.union(Self::OPTIONAL).union(Self::SPACE_AFTER_LIST);
    /// `LFTypeArguments`.
    pub(crate) const TYPE_ARGUMENTS: Self = Self::COMMA_DELIMITED
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::SINGLE_LINE)
        .union(Self::ANGLE_BRACKETS)
        .union(Self::OPTIONAL);
    /// `LFTypeParameters`.
    pub(crate) const TYPE_PARAMETERS: Self = Self::TYPE_ARGUMENTS;
    /// `LFParameters`.
    pub(crate) const PARAMETERS: Self = Self::COMMA_DELIMITED
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::SINGLE_LINE)
        .union(Self::PARENTHESIS);
    /// `LFIndexSignatureParameters`.
    pub(crate) const INDEX_SIGNATURE_PARAMETERS: Self = Self::COMMA_DELIMITED
        .union(Self::SPACE_BETWEEN_SIBLINGS)
        .union(Self::SINGLE_LINE)
        .union(Self::INDENTED)
        .union(Self::SQUARE_BRACKETS);

    /// Ported from `getOpeningBracket`.
    pub(crate) fn opening_bracket(self) -> Option<&'static str> {
        match self.intersection(Self::BRACKETS_MASK) {
            Self::BRACES => Some("{"),
            Self::PARENTHESIS => Some("("),
            Self::ANGLE_BRACKETS => Some("<"),
            Self::SQUARE_BRACKETS => Some("["),
            _ => None,
        }
    }

    /// Ported from `getClosingBracket`.
    pub(crate) fn closing_bracket(self) -> Option<&'static str> {
        match self.intersection(Self::BRACKETS_MASK) {
            Self::BRACES => Some("}"),
            Self::PARENTHESIS => Some(")"),
            Self::ANGLE_BRACKETS => Some(">"),
            Self::SQUARE_BRACKETS => Some("]"),
            _ => None,
        }
    }

    /// Whether the list prints across lines.
    ///
    /// `PRESERVE_LINES` degrades to single-line here; upstream would consult the
    /// original positions. See the module docs.
    pub(crate) fn is_multi_line(self) -> bool {
        self.contains(Self::MULTI_LINE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_come_from_the_mask() {
        assert_eq!(ListFormat::PARAMETERS.opening_bracket(), Some("("));
        assert_eq!(ListFormat::PARAMETERS.closing_bracket(), Some(")"));
        assert_eq!(ListFormat::TYPE_ARGUMENTS.opening_bracket(), Some("<"));
        assert_eq!(ListFormat::CLASS_MEMBERS.opening_bracket(), None);
    }

    #[test]
    fn the_named_combinations_match_upstreams_bits() {
        // Spot-checks against `internal/printer/printer.go`'s block. These are
        // upstream's layout decisions, so a divergence here is a divergence in
        // output shape, not a detail.
        assert!(ListFormat::CLASS_MEMBERS.contains(ListFormat::INDENTED));
        assert!(ListFormat::CLASS_MEMBERS.is_multi_line());
        assert!(ListFormat::ENUM_MEMBERS.contains(ListFormat::COMMA_DELIMITED));
        assert!(
            ListFormat::NAMED_IMPORTS_OR_EXPORTS_ELEMENTS
                .contains(ListFormat::SPACE_BETWEEN_BRACES)
        );
        assert!(!ListFormat::UNION_TYPE_CONSTITUENTS.is_multi_line());
        assert!(ListFormat::UNION_TYPE_CONSTITUENTS.contains(ListFormat::BAR_DELIMITED));
    }

    #[test]
    fn preserve_lines_degrades_to_single_line() {
        // Stated as a test so the deviation cannot be lost: upstream would keep the
        // author's line breaks here.
        assert!(!ListFormat::OBJECT_LITERAL_EXPRESSION_PROPERTIES.is_multi_line());
    }
}
