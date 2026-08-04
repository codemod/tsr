# ADR-0027 — A byte-order mark selects the encoding, and a binary file is reported once

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10`.

## Context

`examples/over_reports.rs` (added the same day) measured every diagnostic we emit
that upstream does not, because the `diagnostics` suite structurally cannot see
them: it fails a case for emitting too much *or* too little, and with no checker
almost everything fails for too little. That measurement put the **parser and
scanner at 7,234 over-reported diagnostics over 509 cases**, against the binder's
126 over 47 — a 57:1 split.

The largest single code was `TS1127 Invalid character`: **1,812 diagnostics over
just 8 cases.** A count that lopsided is a cascade, not eight bugs, and it was two:

### Five corpus files are UTF-16, and we read them as UTF-8

`compiler/bom-utf16be`, `bom-utf16le`, `unicodeIdentifierNames`, `promiseTest`,
and `collisionCodeGenModuleWithUnicodeNames` begin `FF FE` or `FE FF`. Read as
UTF-8 they become a run of replacement characters with a NUL between every letter,
and the scanner reports `TS1127` on nearly every position.

The harness's `read_lossy` was `String::from_utf8_lossy`, justified by a comment
that is half right: "a handful of corpus files are deliberately malformed to
exercise encoding handling; refusing to read them would remove them from the
denominator, which is the opposite of what a conformance harness should do." The
second clause is correct and is preserved. The first conflated two different files
— `compiler/corrupted` really is malformed, but these five are *correctly encoded
files in the other encoding TypeScript accepts*.

Upstream decodes them in `decodeBytes` (`internal/vfs/internal/internal.go:170`):
`FF FE` → UTF-16LE, `FE FF` → UTF-16BE, `EF BB BF` → strip, else UTF-8.

### A genuinely binary file was reported per character

`compiler/TransportStream` (557 over-reports) and `compiler/corrupted` (8) are
binary. Upstream reports **`TS1490 File appears to be binary` exactly once** and
abandons the file: `scanner.Scan`'s default arm
(`internal/scanner/scanner.go:936-941`) reports at offset 0 with length 0, sets
`pos` to the end of the text, and returns `KindNonTextFileMarkerTrivia`.
`corrupted.errors.txt` contains that one error and nothing else.

## Decision

Port both. `decode_bytes` goes in `tsr-vfs`, matching upstream's placement in
`internal/vfs`, and the harness's `read_lossy` delegates to it. The scanner gains
the binary-file arm.

### Why `char::REPLACEMENT_CHARACTER` is the right test

Upstream's condition is `ch == utf8.RuneError`, and it does **not** check the
decoded size. In Go, `utf8.DecodeRuneInString` returns `RuneError` both for an
invalid byte (size 1) and for a validly encoded U+FFFD (size 3), so upstream
declares a file binary in either case.

That is not a hypothetical: `corrupted.ts`'s bytes are `EF BF BD 1F EF BF BD …` —
`EF BF BD` is a *valid* UTF-8 encoding of U+FFFD. The file upstream calls binary
contains no invalid UTF-8 at all.

So matching on the replacement character reproduces upstream exactly, including
the quirk, and it is the only test available: by the time the scanner sees the
text, `decode_bytes` has already substituted U+FFFD for anything undecodable, so
an invalid byte and a real U+FFFD are indistinguishable — as they are for
upstream.

## Consequences

- Lossy replacement is kept for genuinely invalid input, so `compiler/corrupted`
  is still read and still judged. Dropping it from the corpus to avoid the error
  would be the failure mode the original comment warned about.
- An odd trailing byte in a UTF-16 file is dropped rather than an error, matching
  `binary.Read` into a `[]uint16` of length `len(s)/2`.
- Unpaired surrogates become U+FFFD, as `utf16.Decode` does. Note the interaction:
  a UTF-16 file with an unpaired surrogate will now be declared *binary* by the
  scanner, because the surrogate decodes to the replacement character. Upstream
  behaves the same way for the same reason.
- **`tsr-vfs` still does not read the disk.** `decode_bytes` is placed there so
  the decision lives with the file system rather than in the harness, and so an
  OS-backed `FileSystem` gets it for free — but today the harness is its only
  caller. This is not a claim that the VFS decodes files; it is where the function
  belongs when it does.

## Measured effect

| | over-reported diagnostics | cases |
|---|---:|---:|
| parser/scanner before | 7,234 | 509 |
| after `decode_bytes` | 5,300 (TS1127: 1,812 → 565) | — |
| after the binary arm | **2,675** | **503** |

`TS1127` is gone entirely. The cascades it fed shrank with it: `TS1005` 1,907 →
1,084, `TS1012` 2,456 → 578, `TS1109` 315 → 280.

**The ceiling barely moved, and that is the honest headline.** The `diagnostics`
suite's cap went from 90.9% to 90.9% — 502 capped cases to 500. A 63% cut in
over-reported *diagnostics* bought two *cases*, because six files were producing a
third of all the noise between them.

The lesson is worth recording because it was mine to learn: I picked this work by
diagnostic count, calling TS1127 "~25% of all parser noise", which was true and
nearly irrelevant to the number that matters. **Rank remaining parser work by
cases, not by diagnostics** — `TS1005` (318 cases) and `TS1012` (186) are the real
targets, and they are error-recovery divergences rather than one cascade.

What it did buy, beyond two cases: the corpus is no longer being *judged on
garbage*. Five files were parsed, bound, and printed from mojibake, and two suites
grew denominators when that stopped:

| suite | before | after |
|---|---|---|
| `binder_symbols` | 8,284/8,451 | 8,290/8,457 |
| `printer_round_trip` | 11,672/11,728 | 11,679/11,735 |

Both numerators moved by the same amount as their denominators (+6, +7): files that
could not be judged now can be, and they pass. No suite regressed.

## How we would know this was wrong

- `a_binary_file_is_reported_once_and_abandoned` (`tsr-scanner`), verified to fail
  with the scanner arm reverted. It asserts one diagnostic rather than one per
  byte, the zero-length span at offset 0, the token sequence, and that a stray
  *decodable* non-ASCII character is still `TS1127` rather than `TS1490`.
- `a_byte_order_mark_selects_the_encoding` and
  `decoding_does_not_reject_a_malformed_file` (`tsr-vfs`). **These two were not
  verified by reverting**, because they live in the same file as `decode_bytes` and
  stashing it removes both. Their evidence is the corpus measurement instead:
  TS1127 fell 1,812 → 565 from the decoding change alone, before the scanner arm
  existed.
- The interaction to watch is a real file that legitimately contains U+FFFD. It
  will be declared binary. That is upstream's behaviour, but if a corpus case ever
  disagrees, this is why.
