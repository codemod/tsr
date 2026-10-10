# r6-printer5: scanner diagnostics, the regular-expression validator, type-parameter names (`tsr-2zk.1174`, `.1192`, `.1170`, `.1260`)

Lane r6-printer5 (epic `tsr-2zk`), round 6. Takes the printer lane over from
r6-printer4 (`r6-printer4.md`): `printing.rs`, `signatures.rs`, `objects.rs`,
`union_signatures.rs`, `literals.rs`, `types.rs`, `crates/tsr-scanner`, and
this round `symbol_accessibility.rs`. Vendor pinned at `5b1047d`.

## 0. Base and method

- **Frozen base: `7dba1e1`** (main after batch BS). Batch CB (r6-printer4)
  had not landed on main when this box started; per the brief the box works
  on the tip of the time and merges CB when it lands.
- Base dumps, both unfiltered: types 550,412 RIGHT / 5,139 WRONG / 752 GAP;
  diagnostics 5,643 RIGHT, 5,606 EMPTY_RIGHT, 950 WRONG, 39 EMPTY_WRONG.
- **Oracle**: `scripts/offline-cargo/build-tsgo.sh`; every native line below
  was read from that `tsgo` (`--pretty false`).
- Compared on `cut -f1,2`; `slowcases` on both dumps.
- Setup: PyPI is blocked; the offline bootstrap ran with a stdlib-only
  stand-in for `assemble.py`'s three `tomlkit` calls, kept outside the
  repository (`r5-operators3.md` §4).

## 1. Scanner arms: numeric radix, `.digit`, `#` (`tsr-2zk.1174`, part)

r6-triage's row 23 is a family (§3 of `r6-triage.md`). Split by native
operation, its 15 cases are:

| part | native | cases | where |
|---|---|---|---|
| radix digit messages | `Scan` `0b`/`0o` arms, `scanner.go:724-746` | `invalidBinaryIntegerLiteralAndOctalIntegerLiteral`, `parseBigInt` (part) | this section |
| `.digit` | `Scan` `case '.'`, `scanner.go:604` → `scanNumber` | `parseBigInt` (part) | this section |
| `#` | `Scan` `case '#'`, `scanner.go:897-925` | `privateNameHashCharName` (and `shebangError`, outside the row) | this section |
| regex escapes and flags | `ReScanSlashToken(true)` + `regexp.go`, from `checkGrammarRegularExpressionLiteral` | `unicodeExtendedEscapesInRegularExpressions07/12/14/17/19`, `parser.numericSeparators.unicodeEscape`, `parserRegularExpressionDivideAmbiguity3` | §2 |
| keyword escapes | `Parser.nextToken`, `parser.go:381` | `scannerUnicodeEscapeInKeyword1/2` | §3 |
| regex-vs-divide | the parser's `reScanSlashToken` sites | `parser645086_1/2`, `parserRegularExpressionDivideAmbiguity4` | §3 |

### What native does, and the port

- `0b`/`0o` with no digit report `Binary digit expected` (TS1177) and `Octal
  digit expected` (TS1178), not the hex arm's TS1125. The port's shared radix
  arm used TS1125 for all three; it now picks the message by radix.
- `.` followed by a digit is `scanNumber` itself, which owns the bigint and
  identifier-suffix checks. The port's punctuation arm scanned the digits on
  its own, so `.1n` was a number followed by an identifier `n` (TS1005) where
  native reports TS1353 `A bigint literal must be an integer`. The `.digit`
  dispatch moved to `scan_token`, ahead of punctuation, with the token's
  flags.
- `#` has three exits in native: a private name, `#!` past the file start
  (TS18026 over two characters, an `Unknown` token of the `#` alone), and
  anything else, which is `Invalid character` at the `#` while the token is
  still a `PrivateIdentifier` (value `#`). The port made the last two a
  `HashToken`, which the parser then reported as TS1128/TS1068/TS1003. The
  `#\` arm now asks native's question too: the escape after `#` decodes to an
  identifier start (`peekUnicodeEscape`), else the `Invalid character` exit.

**Falsifier:** `crates/tsr-scanner/tests/numeric_and_hash_arms.rs` (each arm
checked against tsgo's output for the same text).

### Measured (against `7dba1e1`, both dumps unfiltered)

- diagnostics **+3 / −0**: `invalidBinaryIntegerLiteralAndOctalIntegerLiteral`,
  `privateNameHashCharName`, `compiler/shebangError` (TS18026).
- types **+0 / −0** on aligned lines; 48 lines newly align (`parseBigInt`,
  `privateNameHashCharName`, `bigintPropertyName`), 47 RIGHT and one WRONG
  (`parseBigInt:0:113`, not aligned on the base). RIGHT 550,412 → 550,459.
- slowcases clean on both dumps. The change is three dispatch arms; the only
  per-token cost is a one-byte look-ahead on a `.` token.

## 2. The regular-expression validator (`tsr-2zk.1192`, and §1's regex part)

### Forcing constraint

Codes TS1499–TS1538 had no emitter: r6-triage row 41 (10 cases) and the
regex rows of row 23 (8 cases). `regularExpressionWithNonBMPFlags` alone
wants six TS1499; `unicodeExtendedEscapesInRegularExpressions07` wants
TS1198 for `/\u{110000}/u`.

### What native does

The parser's `reScanSlashToken` (`parser.go:2998`) calls
`ReScanSlashToken()` with no argument: it finds the body's end and the
flags, reports only `Unterminated regular expression literal`, and validates
nothing. Validation is a **checker** grammar check:
`checkRegularExpressionLiteral` (`checker.go:8012`) runs, once per literal
(`NodeCheckFlagsTypeChecked`), `checkGrammarRegularExpressionLiteral`
(`grammarchecks.go:68`): in a file with no parse diagnostics, a scanner of
the checker's own re-scans from the literal and calls
`ReScanSlashToken(true)`. With `true` the flag loop reports unknown,
duplicate, `u`+`v` and target-gated flags (`scanner.go:1180-1193`), and
`regExpParser.run` (`regexp.go:1043`) walks the body with `s.end` narrowed
to it. The checker's `onError` callback keeps one diagnostic per start
position, and a `Message`-category report (`Did you mean …?`, TS1369) at the
same span as the last kept error becomes its related information.

### The port

- `crates/tsr-scanner/src/regexp.rs`: `scan_regular_expression_errors(text,
  token_start, language_version)` is `ReScanSlashToken(true)` (flag loop,
  unterminated recovery span) and `regExpParser` function for function
  (`scanDisjunction` … `run`), returning the callback's arguments in report
  order. `unicode_properties.rs` is `unicodeproperties.go`, transcribed by a
  script, source order kept.
- **A scanner of its own, rejected alternative: reuse `Scanner`.**
  `regExpParser` moves native's byte cursor, narrows `s.end`, reads
  `s.char()` (one byte, `-1` past `end`), and calls `scanEscapeSequence`,
  `scanUnicodeEscape`, `scanHexDigits` and `scanIdentifier` on it; every
  reported column follows those byte moves. `Scanner` has a char-level
  cursor, decoded values and a rewindable diagnostic list shaped for the
  parser. Bending it to the validator would put regex-only branches in the
  token scanner's hot path. The module carries the few routines it needs,
  transliterated over bytes; `scanEscapeSequence` here is the regex use only
  (`scan_escape_into` stays the string/template one). What would make reuse
  win: a byte-cursor `Scanner`, which this port does not have.
- Native's routines return Go strings that may hold lone surrogates or raw
  bytes. The validator reads them only as empty / one code point / longer
  (for `a-b` ranges), so `CharValue` keeps exactly that, with native's
  surrogate splitting in non-unicode mode (`pendingLowSurrogate`).
- Map iteration: native suggests from Go maps (`maps.Keys`), in no order.
  `getSpellingSuggestion` keeps the minimum distance and breaks ties with
  `strings.Compare`, so the answer does not depend on order; the port walks
  the tables in source order.
- `literals.rs` `check_grammar_regular_expression_literal`: the
  `hasParseDiagnostics` gate, the re-scan from the literal's `/` at the
  checker's `language_version`, and the `onError` fold. Its caller is the
  node walk's dispatch in `check.rs` (main's), shipped as
  [`r6-printer5-regexp-check-hook.diff`](r6-printer5-regexp-check-hook.diff);
  the walk visits each literal once, which is native's once-per-literal
  guard. Without the diff the function is unreached (`allow(dead_code)`, as
  r6-printer4's plans were).

### Checker port convention

- *Native operation:* `checkGrammarRegularExpressionLiteral` →
  `ReScanSlashToken(true)` → `regExpParser.run`.
- *Key identity and owner:* none. No cache, no side table; per literal, the
  validator's state (group names, references, decimal escapes) lives on the
  stack for one call, as native's `regExpParser` does.
- *Publication states:* none; diagnostics are reported, nothing is stored.
- *Receiver/alias context:* the literal's source text and the checker's
  target only.
- *Expensive work boundary:* one re-scan of one literal's text, only for
  regular expression literals, only in files without parse diagnostics. The
  parse is unchanged (the validator is never called from the parser).

**Falsifiers:** `crates/tsr-scanner/tests/regexp_validator.rs` (tsgo's
output for each literal). Beyond the dumps, every single-file regex case in
the corpus (55 files: `*regularExpression*`, `*regExp*`,
`unicodeExtendedEscapesInRegularExpressions*`) was run through tsgo and the
port at `--target es2015` and `esnext`: the regex codes match on all but
`parserRegularExpressionDivideAmbiguity4`, whose extra TS1005 is the
parser's (§3).

### Measured (stacked on §1, against `7dba1e1`)

- diagnostics **+18 / −0** over §1 (+21 / −0 with it): the ten row-41
  cases, `unicodeExtendedEscapesInRegularExpressions07/12/14/17/19`,
  `parser.numericSeparators.unicodeEscape`,
  `parserRegularExpressionDivideAmbiguity3`, and `parser579071` (outside
  both rows: TS1005 `']' expected` inside a pattern).
- types +0 / −0. slowcases clean on both dumps.
- Ir (`profiling` build, `--singleThreaded --noEmit`): domain-model
  1,092,517,840 → 1,093,526,503 (+0.09%), generic-imports 343,703,462 →
  342,864,605 (−0.24%). Neither project has a regular expression literal:
  callgrind shows the validator never called. The domain-model delta sits
  in `check_node_worker` (the new kind test runs once per node, about 1 M Ir
  over the program) and code layout; CLI output is byte-identical on both
  projects.

## 3. The parser half of row 23 (diff and routed)

### Keyword escapes, TS1260 (diff)

`Parser.nextToken` (`parser.go:381`) reports `Keywords cannot contain escape
characters` on the token it is about to leave when that token is a keyword
whose scan saw an escape (`HasUnicodeEscape || HasExtendedUnicodeEscape`).
`createIdentifier` (`parser.go:5850`) and one async-arrow look-ahead consume
with `nextTokenWithoutCheck` instead, so a keyword written with an escape is
silent wherever it is parsed as a name (`var await`, `{ default: 1
}`, `type type = 1`). The scanner already marks such a keyword
(`TokenFlags::UNICODE_ESCAPE`, §302 of the scanner notes); nothing read it.

[`r6-printer5-keyword-escapes.diff`](r6-printer5-keyword-escapes.diff)
(parser.rs, expression.rs, module.rs: main's; new test
`crates/tsr-parser/tests/keyword_escapes.rs`): `next_token` reports, the new
`next_token_without_check` is the old body, and `parse_identifier`,
`parse_identifier_name` and `declare global`'s name consume with it. The flag
is tested first and the report is out of line (`#[cold]`), so an ordinary
token pays one bit test.

Measured on top of §2's diff (`/tmp/box/c2`): diagnostics **+3 / −0**
(`scannerUnicodeEscapeInKeyword1`, `scannerUnicodeEscapeInKeyword2`,
`switchStatementsWithMultipleDefaults`), types +0 / −0, slowcases clean, CLI
identical. Ir against §2's stack: domain-model 1,093,526,503 →
1,093,911,730 (+0.035%), generic-imports 342,864,605 → 343,667,876
(+0.23%). Against the frozen base the three together are +0.13% and −0.01%:
generic-imports moves by about ±0.25% from code layout alone (§2's binary,
whose new code never runs there, measured −0.24%). The first build, with the
keyword test before the flag test and no `#[cold]`, measured +0.13% / +0.33%
over §2; the shape above is the cheaper one.

### Regex-vs-divide recovery, TS1134 vs TS1161 (routed, main's parser)

`parser645086_1` / `_2` (`var v = /[]/]/`): native reports TS1005 at the
`]`, then TS1134 `Variable declaration expected` at the last `/`; the next
statement's `reScanSlashToken` then finds an unterminated literal at the same
`/`, and `parseErrorAtRange`'s same-start guard drops that TS1161 because it
came **second**. The port keeps the scanner's diagnostics in their own list
and merges them in `Parser::finish` scanner-first at an equal start
(`parser.rs`, §195 of the parser notes: "the scanner reports while scanning
the token, before the parser can say anything about it"). That holds for a
token's first scan and fails for a **rescan**, which the parser asks for
after it has reported at that position. The faithful fix is the sink, not
the scanner: scanner reports must reach `would_repeat_last_error` in emission
order (drain `Scanner::take_diagnostics` into the parser's list after each
scan and rescan, with `restore` truncating both). That is main's parser;
not built here.

`parserRegularExpressionDivideAmbiguity4`'s extra TS1005 is the same
family's recovery after the unterminated literal (parser).
