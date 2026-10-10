# ADR-0053: The compiler driver defers JSDoc in non-JavaScript files, as typescript-go does

- **Status:** accepted (round 7, `r7-perf`, `tsr-2zk.17.1`; drafted in round 6
  by `r6-checkperf`, `tsr-2zk.1131`, as ADR-0051 and held until 0051 was
  taken by the lone-surrogate record)
- **Date:** 2026-10-10 (drafted 2026-10-09)
- **Supersedes:** [ADR-0010](0010-jsdoc-is-a-parse-option.md) in part — the
  option stays and its default stays on; the compiler driver no longer takes
  the default for `.ts`/`.tsx`/`.d.ts` files.
- **Related:** [ADR-0008](0008-jsdoc-parsed-eagerly.md) (why parsing is eager
  when it happens), [ADR-0009](0009-performance-gate.md)

## The forcing constraint

A callgrind profile of the round-6 base (`docs/parity/notes/r6-checkperf.md`
§4) puts `Parser::parse_jsdoc_at` at **148.7 M Ir inclusive on
`domain-model` (13.6% of the whole process) and 148.9 M on
`generic-imports` (43.3%)** — the same number on both, because it is the
default lib `.d.ts` files' documentation, parsed on every compilation. The
top four non-allocator self-cost functions of `generic-imports` are scanner
functions doing that work (`Scanner::bump`, `peek`, `scan`,
`jsdoc_ranges_in`, `scan_jsdoc_comment_text_token`).

typescript-go does not do this work. `withJSDoc`
(`internal/parser/jsdoc.go:56`, pinned `5b1047d`) only flags a documented
node of a non-JavaScript file (`NodeFlagsHasJSDoc`, plus
`PossiblyContainsDeprecatedTag` from a text scan) and returns, **unless one
of the node's comments carries `@see`, `@link`, `@linkcode` or `@linkplain`**
(`jsdocScannerInfoHasSeeOrLink`, set by the scanner's
`scanJSDocCommentForTags`, `internal/scanner/scanner.go:350`). Those are
parsed eagerly, because unused-identifier checking reads them. The rest is
parsed on the first `Node.JSDoc()` read (`parseJSDocForNode`,
`jsdoc.go:19`).

What reads lazily? In the checker, `checkSourceElementWorker` reads
`EagerJSDoc` only (`checker.go:2255`), as does the `@augments` grammar check
(`grammarchecks.go:916`). The lazy `JSDoc(nil)` readers are
`GetJSDocDeprecatedTag` (`ast/utilities.go:1220`, from
`addDeprecatedSuggestionWorker`, `checker.go:14044`: a suggestion's related
information) and `getAllJSDocTags` (`checker/jsdoc.go:89`, from
`checkUnmatchedJSDocParameters`, which reports through `errorOrSuggestion`
— a suggestion in a TS file). Neither reaches an error or the CLI's output.

## The decision

`ParseOptions` gains `defer_ts_jsdoc`. With it, `Parser::parse_jsdoc_at`
parses a construct's comments only when one of them carries one of the four
tags (the same test as `hasJSDocTag`); otherwise it returns the empty list,
as `withJSDoc` does. `ParseOptions::deferring_ts_jsdoc(name)` sets it for
every file that is not JavaScript by extension (`ast.IsSourceFileJS`), and
the compiler driver (`tsr-compiler`'s loader and `Program::parse`, which the
CLI and the conformance harness both use) applies it to every program file.
The default stays off, so every other consumer — the parser's own tests,
`jsdoc_probe`, a future language service — still gets every comment.

## Alternatives

**Lazy parsing, as upstream does.** Still blocked by ADR-0008's lifetime
reason (no allocation into a `ParsedFile`'s arena after construction). It
would also serve the deprecated-suggestion and hover readers this decision
leaves without JSDoc in TS files. It wins as soon as that obstacle goes.

**Skip JSDoc in declaration files only.** Measured as a first probe (dm wall
ratio 0.761 → 0.672, gi 0.915 → 0.622 against tsgo, CLI identical). Rejected:
an extension rule with no upstream counterpart, and it would keep parsing
documentation upstream does not parse in `.ts` sources.

**Keep ADR-0010's eager default in the driver.** Rejected by the measurement
above: the work is upstream-absent, so the TSR/tsgo ratio compares different
work, the thing ADR-0009 and ADR-0010 exist to prevent.

## Consequences accepted

- In a `.ts`/`.tsx`/`.d.ts` program file, a comment without
  `@see`/`@link` is not in the `JSDocTable`. A checker reader that consulted
  such a comment in a TS file now sees nothing. On the whole conformance
  corpus both dumps (12,238 diagnostics rows, 556,303 type rows) were
  byte-identical with and without the change, and the CLI output was
  identical on `domain-model`, `domain-model-large`, `generic-imports` and
  `src/jsTyping` — so no current reader depends on one in a way the corpus
  sees.
- `@deprecated` suggestions and hover, should this port grow them, will need
  the lazy path (or a parse of the host's comments on demand) in TS files.
- Two parse configurations exist in practice (driver and default), as
  ADR-0010 already accepted for benchmarks.

## How we would know this was wrong

- A conformance case whose errors depend on a TS-file comment without
  `@see`/`@link`: it would change verdict when this lands (it did not on the
  round-6 corpus), or a later checker port that reads one (e.g. a JSDoc
  `@param` error that upstream reports as an error in a TS file).
- Upstream starting to read `JSDoc(nil)` on an error path in the checker.
  `grep -n 'JSDoc(nil)\|JSDoc(file' internal/checker/*.go` at the pinned
  commit finds the two readers named above; a third one is the falsifier.
