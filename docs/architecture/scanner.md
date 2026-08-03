# The scanner

**Crates:** `tsr-scanner`, `tsr-diagnostics`
**Ported from:** `vendor/typescript-go/internal/scanner/scanner.go`
**Conformance:** `scanner_termination` 12,444/12,444 · `scanner_clean_files` 5,648/5,648 (100%)

## Pull-based, not a token stream

[`Scanner`] hands out one token at a time rather than producing a `Vec<Token>`,
because two constructs cannot be lexed without grammatical context:

- **`/` is division or a regular expression.** `a / b` and `a(/b/)` are
  lexically identical up to the slash. The parser calls
  `rescan_as_regular_expression` when its position permits a literal.
- **`>>` may be one shift operator or two closing angle brackets.**
  `List<List<T>>` needs the split; `a >> b` does not. Hence
  `rescan_greater_than`.

Template continuations are a third case, though a decidable one: after a
`TemplateHead`, the `}` ending a substitution has to be re-scanned as template
text via `rescan_template_continuation`.

A materialised token vector cannot express any of this, which is why the scanner
is a cursor.

## Backtracking

`save()` / `restore()` capture and rewind position. TypeScript's grammar is not
LL(k) — distinguishing an arrow function from a parenthesised expression means
trying one and backing out — so the parser will lean on this heavily.

`restore` **discards diagnostics emitted since the save**. Without that, a path
the parser abandoned would still report its errors, and speculative parsing would
produce phantom diagnostics. The conformance harness already depends on it for the
regex probe.

## Decisions worth knowing

**Keywords are derived from `SyntaxKind`, not duplicated.** Every keyword kind is
named `<Word>Keyword` and its source text is the lowercase form, so the table is
computed rather than maintained. `tests/keyword_conformance.rs` checks the
derivation against upstream's hand-written map in both directions — that it
resolves every upstream keyword, *and* that it invents none.

**Spans are byte offsets.** `é` is two bytes; a character-based offset would put
the next token in the wrong place. Tested directly.

**Errors never stop the scan.** A malformed literal still produces a token, and an
unknown character produces `Unknown` and advances. A scanner that stalls hangs the
parser forever, which is why `scanner_termination` checks progress explicitly
rather than trusting it.

## Three bugs the corpus found

None of these were visible by inspection; all three came from running 12,444 real
files. They are recorded because each represents a category of mistake that will
recur.

1. **U+2028 / U+2029 inside string literals.** They are line terminators in the
   grammar, but ES2019's JSON-superset proposal permits them *unescaped* inside
   string literals. Treating them uniformly as line breaks truncated strings.
   Corpus: `allowUnescapedParagraphAndLineSeparatorsInStringLiteral`.

2. **U+0085 (NEL) and U+200B are whitespace.** NEL is in Zs, so it is whitespace,
   but it is deliberately *outside* ECMAScript's line-terminator set — upstream
   comments on exactly this. Corpus: `fileWithNextLine2`, whose own comment says
   "it should be treated like a space".

3. **Lone surrogates are legal.** `"\u{D800}"` is a valid string literal, because
   JavaScript strings are UTF-16. Rust's `char` cannot hold a surrogate, so
   `char::from_u32` returns `None` and the naive code reported an error on source
   TypeScript accepts. Surrogate *pairs* are now combined (`💩` is one
   character) and a lone one degrades to U+FFFD **without a diagnostic**. Corpus:
   `unicodeExtendedEscapesInStrings10/11`.

The shared lesson: the failure mode is a *false positive* on valid code, and
inspection does not find those. Only the corpus does.

## JSX mode

JSX children obey different rules from expressions: everything up to the next `<`
or `{` is literal text, so `&nbsp;`, backslashes, and quotes carry no special
meaning. Attribute values are raw strings — `title="a\b"` contains a backslash,
not an escape. And JSX names may contain `-`, which is a subtraction everywhere
else.

None of that is decidable by the scanner alone: all three depend on *where in an
element* the cursor is, which only the parser knows. So JSX is a set of explicit
entry points rather than a mode flag — `scan_jsx_token`, `scan_jsx_identifier`,
`scan_jsx_attribute_value`, and re-scan variants for each.

`scan_jsx_identifier` **mutates the current token in place** rather than producing
a new one, matching upstream: `data` has already been scanned by the time the
parser knows it wants `data-foo`.

`rescan_jsx_attribute_value` exists because the parser holds one token of
lookahead — by the time it has consumed `=`, the value was already scanned under
expression rules, where `"a\b"` reports a bad escape. The rescan rewinds and
discards those misattributed diagnostics.

Whitespace-only children are distinguished by whether they contain a line break:
`<div>\n  </div>` has no text child (that is indentation), while `<div>  </div>`
does. Getting this wrong changes rendered output.

## Not yet built
- **JSDoc scanning.** Upstream has a separate mode for doc comments.
- **Shebang handling** for `#!` on the first line.
- **Speed.** A full corpus run takes ~58s in a debug build. The scanner is not the
  bottleneck yet, but `rayon` across cases is the obvious first move.
