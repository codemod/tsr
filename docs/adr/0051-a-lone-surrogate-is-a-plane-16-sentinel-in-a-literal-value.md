# ADR-0051: A lone surrogate in a literal value is a two-character plane-16 sentinel

- **Status:** Accepted. Built in `crates/tsr-scanner/src/lib.rs`
  (`push_js_string_code_point`, `decode_lone_surrogate_sentinel`,
  `Scanner::push_code_point`) and `crates/tsr-checker/src/printing.rs`
  (`quote`).
- **Date:** 2026-10-09
- **Issue:** `bd tsr-2zk.1128` (lane r6-printer). Measurements:
  [`docs/parity/notes/r6-printer.md`](../parity/notes/r6-printer.md) §5.
  The design options were first listed in
  [`r5-printer3.md`](../parity/notes/r5-printer3.md) §6.2.
- **Pinned upstream:** `vendor/typescript-go` @ `5b1047d`.

## The forcing constraint

A JavaScript string is a sequence of UTF-16 code units, and it may hold an
unpaired surrogate: `"\u{D800}"`, `"\uDC00"`, `` `\u{D800}` ``. Native's
string values are Go strings. `stringutil.EncodeJSStringRune`
(`internal/stringutil/util.go:327`) stores a lone surrogate as the three
WTF-8 bytes `ED A0 80`, which are invalid UTF-8.
`stringutil.DecodeJSStringRune` (`:339`) reads them back, and the printer's
`escapeStringWorker` (`internal/printer/utilities.go:84`) always escapes
such a code point as `\uD800` (uppercase, four digits). So
`unicodeExtendedEscapesInStrings10(target=es6)` prints
`>"\u{D800}" : "\uD800"`.

A Rust `String` must be valid UTF-8, so it cannot hold those bytes. Until
now the scanner (`Scanner::push_code_point`) substituted U+FFFD. That loses
the value: the printer cannot recover the code unit, so the line prints
`"�"`. Every lone surrogate also collapses onto one literal, so `"\uD800"`
and `"\uDC00"` intern to the same `StringLiteral` type.

Measured payoff: 4 type lines (`unicodeExtendedEscapesIn{Strings,Templates}1{0,1}(target=es6)`).

## The options

1. **WTF-8 values** (`Vec<u8>` or a `JsString` newtype) from the scanner's
   token value, through the AST's literal text (`&'a str`, generated code),
   to the checker's `TypeData::StringLiteral(String)`. This is native's
   representation and is exactly faithful. But every consumer of a
   literal's text changes type: the AST codegen, the binder's property
   names, every `&str` comparison in the checker, and the printers. It would
   be a multi-crate change for a 4-line payoff, and it adds a decode on
   every comparison with source text.
2. **UTF-16 values** (`Vec<u16>`). This is the JavaScript model exactly, but
   it costs the same as option 1 plus a conversion at every boundary with
   UTF-8 source text.
3. **A side table** keyed by node, holding the original code units, with
   U+FFFD kept in the `String`. This fixes printing at one site only. Type
   identity stays wrong (`"\uD800"` and `"\uDC00"` are one type), and the
   table has to be consulted wherever a literal's value is printed.
4. **A one-character sentinel inside valid UTF-8.** Map lone surrogate
   U+D800 + n to U+10F800 + n. *Built first and refused.* A corpus string
   literal already holds the top of that range:
   `unicodeExtendedEscapesIn{Strings,Templates}06` write `"\u{10FFFF}"`, the
   maximum code point. The sentinel printed it as `"\uDFFF"`, so those 2
   RIGHT lines went WRONG on the first unfiltered run (r6-printer §5). The
   pre-build check had only grepped for the raw bytes of plane-15/16
   characters, and that cannot see an escaped one. Any one-character
   sentinel borrows real code points, so it conflates whichever of them a
   test happens to write.
5. **A two-character sentinel inside valid UTF-8** (chosen). Lone surrogate
   U+D800 + n is stored as U+10FFFF (a noncharacter) followed by
   U+10F000 + n. A real U+10FFFF, or a real U+10F000–U+10F7FF, is stored as
   itself. Only the exact sequence "U+10FFFF then a code point in
   U+10F000–U+10F7FF" reads as a sentinel. That is a noncharacter followed
   by a private-use character, which no corpus file writes, raw or escaped
   (`\u{10FFFF}` is never followed by a plane-16 escape).

## The decision

Option 5. It keeps literal identity: distinct surrogates are distinct
strings, and a real U+10FFFF is not a surrogate. It is the only option sized
to its payoff. And the representation stays a `String`, so the AST, the
binder and the checker are untouched.

What is built:

- `tsr_scanner::push_js_string_code_point(cp, out)` appends the sentinel
  for a surrogate and the character otherwise. This is native's
  `EncodeJSStringRune`. `Scanner::push_code_point` uses it for a lead
  surrogate that does not combine with a following `\uXXXX` trail, and for
  any trail surrogate.
- `tsr_scanner::decode_lone_surrogate_sentinel(s)` returns the surrogate a
  sentinel at the start of `s` stands for. This is the surrogate arm of
  `DecodeJSStringRune`.
- `printing::quote` escapes a sentinel as `\uXXXX`, uppercase. That is
  `escapeStringWorker`'s unconditional surrogate arm. `quote_ascii`
  inherits it.

## Consequences accepted

- **One two-character sequence is conflated.** A string that really holds
  U+10FFFF immediately followed by U+10F000–U+10F7FF is read as a lone
  surrogate. Native keeps them distinct.
- **A lone surrogate is two Rust `char`s.** A consumer that walks
  `chars()` sees two characters where JavaScript has one code unit. No
  current consumer measures a literal's length or indexes it.
- **`CombineSurrogatePairs` is not ported.** Native merges a high and a low
  sentinel when separately scanned values are joined (template literal
  types, `+` folding: `"\uD83D" + "\uDE00"` is `"😀"`). The port leaves the
  two sentinels side by side, and that prints `"😀"`. No corpus
  line reaches it.
- **Other printers do not decode the sentinel.** That covers the emitter's
  `escape_template` and the declaration emitter. They print source text for
  a written literal and see the sentinel only in a synthesized one. No
  corpus output covers that path.

## How we would know this is wrong

- A corpus string writing U+10FFFF followed by a U+10F000–U+10F7FF
  character (raw or escaped). Grep both forms when the submodule moves.
  §5's first run is the evidence that the escaped form matters.
- A consumer that needs a literal's UTF-16 length or code units (for
  example a future `.length` evaluation, or the string-mapping intrinsics
  on surrogates). It would have to decode the sentinel. If more than one
  such consumer appears, option 1 is cheaper than teaching each one.
- If the corpus starts reaching `CombineSurrogatePairs` (a joined
  surrogate pair printing two escapes), port it as a join-time
  canonicalization over the sentinel, exactly as native does over WTF-8.
