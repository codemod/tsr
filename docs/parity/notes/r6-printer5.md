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
