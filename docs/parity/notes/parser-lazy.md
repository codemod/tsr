# Parser language boundaries and lazy TypeScript JSDoc

## Recovery at 0e7824dd: tsr-2zk.16.405

Native remains `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The recovery source `box/parity-parser-final` was fetched read-only. Its
`bfaf6185` owned parser changes were rederived against native; the final
`7840bfca` automatic snapshot was not merged, cherry-picked, or accepted as
verified evidence. Its JavaScript call-typeargument gate was separately
rederived from native `Parser.tryParseTypeArgumentsInExpression`.
The cheap JSDoc classifier already present at b01886cf was not duplicated.

### Implemented operations and boundaries

`ScriptKind::{Js, Jsx}` preserve JavaScript identity. Filename inference maps
`.js/.mjs/.cjs` to Js, `.jsx` to Jsx. Native `getLanguageVariant` in
`internal/parser/utilities.go` selects JSX scanner mode for JS, JSX, TSX and
JSON; JSON still enters its separate value parser. Native
`parseJsxOpeningOrSelfClosingElementOrOpeningFragment` and
`tryParseTypeArgumentsInExpression` reject type arguments in JS/JSX while
retaining TypeScript parsing. The JSX caller invokes the ordinary type parser
for TSX, including native `<` rescanning. Missing identifier recovery in
`createIdentifierWithDiagnostic` finishes at `nodePos()` before leading
trivia; `report_missing_identifier` now records that empty extent without an
extra replacement allocation. Missing `super += 3` name is consequently
[5,5), as the direct native control confirms. Punctuation printing includes
native `...` and `</` messages.

No semantic cache, mapper, traversal or reuse extension was added. Script kind
belongs to one immutable parser invocation, not a checker cache. Node IDs are
private file identities until ordered NodeTable/NodeMap publication relocates
them to Program identity. Missing identifiers publish completed empty syntax,
not absent or provisional semantic results. Type arguments remain ordered
arena-owned slices: existing generated Publish traverses every typed argument,
including nested TypeReference arguments, with registered-node memoization.
The new publication control drops the private parser owner before reading
nested JSX/call type arguments, uses a preceding file to force relocation,
and checks concrete child names, range ownership and parent identity. Existing
publication code already handles this boundary; no second type-list table is
needed. Receiver/alias semantics are unchanged. The expensive worker remains
ordinary expression/JSX/type parsing, with no extra pass or cache.

### Fresh receipts, not historical passes

Ignored receipt directory: `target/recovery-parser/`. It contains frozen base
executables, before/after unfiltered dumps, native Go controls, transition JSON,
binary hashes, test logs, a read-only all-suite run and parser timing samples.
These ignored files remain Box-local; automatic commit fetching does not carry
them. The measured conclusions below are recorded here for integration.

Base is 0e7824dd with no unrelated edits. Base/candidate full unfiltered dumps:

| Population | Base | Candidate |
|---|---:|---:|
| Type rows | 477970 | 477996 |
| RIGHT type rows | 469785 | 469814 |
| WRONG type rows | 7190 | 7187 |
| GAP type rows | 995 | 995 |
| Diagnostic rows | 10570 | 10570 |
| RIGHT diagnostics | 4222 | 4226 |
| WRONG diagnostics | 1280 | 1276 |
| EMPTY_RIGHT diagnostics | 4968 | 4968 |
| EMPTY_WRONG diagnostics | 100 | 100 |

Every previous RIGHT/EMPTY_RIGHT key was explicitly looked up; zero adverse
transitions and zero vanished keys. All three
`parseJsxElementInUnaryExpressionNoCrash1/2/3` diagnostic cases change
WRONG -> RIGHT. Targets 1/2 have all type lines RIGHT; target 3 retains its
outer unary `boolean` versus `any` difference (13/14 target type rows RIGHT).

Direct Go ParseSourceFile controls exercised JS/JSX/TS/TSX malformed unary
JSX, relational-versus-call brackets, parenthesized JSX, nested JSX typeargs,
and missing super names. Parser/scanner release tests passed; all four new
boundary/publication tests passed; conformance parser smoke passed. Parser fmt
check and release parser-library no-deps clippy passed after correcting the
candidate's formatting and if-not-else lint. No workspace-lint pass is claimed.

A read-only external runner completed all 16 existing suites on 12444 cases,
without touching protected snapshots. Parser clean 5031/5031, scanner
termination 12444/12444; checker_types 8057/9538 (2906 skipped), matched
469814/478855 lines; diagnostics 4226/5502 (6942 skipped). These suites and
verdict dumps retain existing varied-configuration/known-divergence exclusions;
they do **not** establish >=99.9% exact full-configuration parity.

Fresh-process interleaved 21-pair parser-only timing, 15 complete parses per
sample, build/setup excluded: candidate/base median wall ratio 0.991832 for
native parser.ts and 1.001716 for dom.generated.d.ts. Equal output node counts
confirm equal parser work on these unchanged TS inputs. The latter is a small
observed increase; no statistically established no-regression or speed-win
claim is made. Neither is whole-project TSR/tsgo timing, and equivalent
complete-checker-work <=0.50 remains unverified.

### Native type-list metadata writer cutover (integration dependency)

The parent-reviewed canonical API is
`NodeTable::type_argument_list_span(host: NodeId) -> Option<Span>` and
`set_type_argument_list_span(host: NodeId, span: Option<Span>)`.
The parent owns its sparse sorted storage, rollback and host-ID relocation.
None is native nil; Some preserves an allocated list even with zero children.
The parser writer returns existing children plus raw metadata, captures
`node_end()` after `<` and before `>`, and registers it on the completed owner.
Ordinary and speculative type arguments use the native delimited-list worker;
parse errors do not by themselves reject a confirmed expression type list.
No first/last-child or bracket-source reconstruction is used. Comment-only
empty expression lists such as f</*c*/> preserve Some([2,2)); closing trivia
must not be included in the raw range or used to infer list absence.

Writers cover TypeQuery, TypeReference, ImportType, JSX, all four expression
instantiation producers, declaration heritage and JSDoc heritage. Absorption
into Call, TaggedTemplate, New and typeof-import transfers the exact metadata
and clears the obsolete wrapper host. None is propagated explicitly; empty
argument slices are not used to infer absence. Private parser invocation owns
this completed syntax metadata until the parent-owned publication rebases its
host IDs; file-relative list ranges remain unchanged. No semantic cache,
receiver/alias reuse, per-node allocation or second metadata convention added.
Rollback relies on the host-write boundary: publication attaches to new hosts,
and absorption clears wrappers created by the same expression/type worker.
Current speculative workers construct those hosts after their saved node count;
NodeTable truncate removes their entries. A future writer targeting an older
host must add snapshot/undo restoration rather than relying on truncation.
No such preexisting-host writer is introduced in this cutover.

Fresh direct Go controls confirm native typeof-f nil versus typeof-f<> [18,18),
f<> [2,2), f< /*a*/ T, /*b*/ >() [2,11), new-f list [6,15), and typeof-import
list [30,39). Raw ranges exclude delimiters and closing-token trivia. Diagnostic
consumers apply native full-source SkipTrivia to start separately; the parser
never clamps diagnostic spans. New consumer-visible parser tests cover these
states, all absorption paths, JSDoc/heritage and private-owner destruction with
a nonzero publication base.

The parent supplied the exact canonical sparse AST API for a compilation-only
local copy; that copy is not included in parser commits. Integrated parser and
scanner release tests, new metadata/publication controls, parser fmt and
parser-library clippy pass. A missing HasNodeId trait use in JSDoc was corrected
by accessing the existing node_id field. A full comparison initially exposed
15 previous RIGHT losses (13 vanished keys) in instantiationExpressionErrors:
our eager scanner's >= was split into a closing > after native delimited-list
recovery. The expression-list close now requires an already-single >, matching
native ReScanGreaterThanToken's packing/rejection operation; nested type
workers still split their own closes. A relational control pins that boundary.

Current unfiltered integrated results: 478039 type rows (RIGHT 469839, WRONG
7205, GAP 995), diagnostic rows 10570 (RIGHT 4227, WRONG 1275, EMPTY_RIGHT
4968, EMPTY_WRONG 100). Zero prior RIGHT/EMPTY_RIGHT losses and zero vanished
keys; all three unary JSX diagnostic targets remain RIGHT. Typequery/call empty
lists and raw ranges pass after private-owner destruction and host relocation.
These are existing harness populations, not full-configuration completion.

Performance **does not pass**: final fresh-process interleaved 21-pair parser
ratios versus frozen 0e7824dd are 1.041992 on parser.ts and 1.161778 on
dom.generated.d.ts. Avoiding absent-list setter work on new TypeReference hosts
retains the same canonical metadata contract but does not remove this measured
slowdown. Parent must resolve sparse storage/writer work attribution before a
no-hotpath-regression landing; no <=0.50 whole-checker claim follows. Integrate
AST/writer/consumer atomically and rerun complete current campaign gates. The
earlier JS-only fb043744 verification remains independent.

### Parse-produced trailing-comma state

The private TypeArgumentListMetadata carrier contains raw Span and the actual
parse_delimited_list trailing_comma result. All eight native accessor domains
publish HAS_TRAILING_COMMA on their concrete host with that metadata. Call/New
argument-list parsing does not stamp this flag, so these hosts have no collision.
Absorption reads the wrapper's metadata and flag, copies them to the resulting
host, and clears its obsolete span entry. Empty/nil lists never acquire the flag.
The parent grammar consumer reads host flag plus raw list end, reporting TS1009
before TS1099; no source-comma scan or child-derived trailing predicate required.

Parser release tests, metadata publication/trailing-state controls, library
clippy and fmt passed with the canonical compilation-only AST copy. Performance
remains rejected: baseline/candidate medians 1.043330/1.150199 for parser.ts/dom.
A temporary monotonic sparse append fast path measured 1.045144/1.153359; inlining
metadata wrappers measured 1.047094/1.149359. Neither fixes the slowdown. The
parent-owned temporary AST copies were removed, not committed. This writer is
correctness-complete but not an accepted no-hotpath-regression landing.

### Default full-source SkipTrivia consumer seam

`tsr_scanner::skip_trivia(source: &str, pos: u32) -> u32` ports pinned
`scanner.SkipTrivia` with default options only. It skips whitespace, comments,
shebang only at source offset zero, and native conflict markers with full-source
line context. Beyond-EOF offsets are unchanged. The API accepts ordinary u32
syntax offsets, not native negative synthesized positions. It does not scan a
substring or allocate a scanner/token/diagnostic buffer. It reuses existing
whitespace/line predicates, shebang helpers and seven-byte marker checks; native
conflict-marker advancement is shared with ordinary scanning. No JSDoc-star or
stop-at-comment options are introduced.

Direct Go controls and scanner regression tests agree on all 15 full-source
controls, including comment-only empty-list offset 2 -> 7, shebang/non-shebang,
non-line-start markers, exactly-seven-byte EOF markers, LS termination, BOM/NEL/
zero-width whitespace, unterminated comments and beyond-EOF offsets. Scanner
release tests, library no-deps clippy and fmt pass. A test's initially incorrect
LS byte-offset expectation was corrected to the observed native 15. The helper
preserves raw list metadata: parent grammar/TS2635 consumers compute their own
native endpoints without clamping. Parser parity and performance gates require
the atomic parent AST/consumer integration; no new passing claim is made here.

### Remaining createMissingIdentifier cursor boundary (tsr-2zk.2)

Pinned native createMissingIdentifier uses nodePos() for both empty endpoints.
The shared parser missing_identifier helper still used token start after trivia;
only report_missing_identifier had already been recovered to nodePos. The
remaining helper now uses scanner.full_start through node_end, without changing
allocation, identity, parent publication or diagnostic filtering. Direct native
controls confirm empty constructor-index children at [6,6) despite spaces,
comments, Unicode and CRLF, [9,9) for new ns.C, comma missing operand [5,5),
and delete missing operand [21,21). Existing constructor-index behavioral
assertions were corrected from post-trivia spans to observed native spans.

Fresh parser tests, parser library clippy and fmt pass with temporary canonical
AST copy. Full current dump converts commaOperatorWithoutOperand and
 deleteOperatorInvalidOperations diagnostic cases WRONG to RIGHT: RIGHT 4227
->4229, keys unchanged; every prior RIGHT/EMPTY_RIGHT and type RIGHT key
preserved, including vanished-key checks. Type counts unchanged. No full varied
configuration or raw native diagnostic-order claim follows.

Performance rejects this candidate: fresh same-flags control rebuilt by changing
only the missing-node cursor back to token start versus candidate node_end gives
41 interleaved affinity-pinned pairs x60 parses wall/CPU 1.025644/1.025780 on
parser.ts, 1.024943/1.025074 on dom, with identical complete node outputs and no
skipped JSDoc/diagnostic work. Helper inlining did not resolve it and was reverted.
The correctness patch is a separately recorded unaccepted candidate, not a
verified no-hotpath-regression landing. Temporary AST copy removed. Parent must
resolve the parse code-layout/worker attribution before acceptance; no threshold
waiver or heuristic missing-node exception supplied.

### Integration prerequisites still open

- Parent owns target 3's `negated_truthiness_type` native default boolean
  behavior. Parser has no special case for the malformed input.
- Integration owner should simplify compiler `loader.rs::parse_options` to
  `ParseOptions::for_file(name)`: its old JSX-option-dependent JS->TSX
  promotion becomes unreachable. Compiler/parser-smoke callsites with manual
  script-kind mapping and the stale checker test comment require owner review.
  No forbidden file was edited here.
- Raw Go diagnostic emission order differs from TSR's existing public parser
  position-sorted order: on `!< {:>` Go emits TS1003, TS1005, TS17008, TS1005;
  TSR returns TS17008, TS1003, TS1005, TS1005. Codes/messages/spans agree for
  these targets after source-order publication, but exact raw native order
  is **not** certified. Changing the shared scanner/parser diagnostic protocol
  needs coordinated ownership; this recovery does not suppress or repin it.
- `bd prime` succeeded, but `bd show`, `bd history`, and `bd update` could not
  find supplied issue tsr-2zk.16.405 in the Box database. No duplicate issue
  was created. Parent must attach these receipts to the existing issue.
- Full configuration expansion, exact diagnostic order/message/type printing,
  integrated no-RIGHT-loss gates and verified equivalent-work wall <=0.50
  remain campaign acceptance prerequisites, not this parser lane's passes.

# Lazy TypeScript JSDoc: tsr-2zk.17.1

## Pinned evidence and implemented boundary

Native: `vendor/typescript-go` commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

Direct Go controls invoke `parser.ParseSourceFile`, `Node.EagerJSDoc`, and
`Node.JSDoc` on the same statement, with TS, TSX, JS, and JSX script kinds.
For `@type {string}` and `@deprecated`, TS/TSX return zero eager comments,
one on demand, and one thereafter. `@see x` and `{@link x}` return one
already-eager comment. JS/JSX return one eagerly for all four controls.
Current TSR returns one eagerly for all four tags with its default options.
This confirms the hypothesis; it does not establish that the assigned JS
failures are caused by TS JSDoc eagerness.

One independently verified scanner discrepancy is fixed:
`scanner.hasJSDocTag` accepts exactly space, tab, LF, CR, `}`, `*`, or end of
text after a cheap tag name. TSR's `mentions_tag` used
`is_ascii_whitespace`, also accepting form feed. Native rejects form feed.
The fix uses the native byte set, for deprecated/see/link/linkcode/linkplain.
`crates/tsr-scanner/tests/parser_lazy_tags.rs` covers accepted terminators,
form feed, vertical tab, NBSP, and identifier suffixes. Ad hoc native and
TSR scanner controls reproduce the form-feed difference before and agreement
after. This fixes the classifier only; it does not implement lazy parsing.

## Ownership, publication, context, and actual work

Native workers: `parser.withJSDoc`, `parser.parseJSDocForNode`,
`SourceFile.resolveJSDoc`, `scanner.scanJSDocCommentForTags`.
Consumer: `Node.JSDoc(file)`; `Node.EagerJSDoc(file)` never forces work.
The native cache belongs to SourceFile (Program lifetime), keyed by concrete
node pointer, and is not a privateChecker cache. Parse options, exact source
text, and script kind come from that SourceFile. SourceFile locks serialize
first-demand parsing. Missing map entry is uncomputed; cached nil is completed
empty, distinct from absence. A write-locked worker is active but unpublished;
completed comment slices or nil publish only after worker return. No separate
unsupported/failure result is manufactured. TS comments do not produce JS
JSDoc diagnostics or JS hosted/unhosted reparses. Each parsed JSDoc root has
the concrete host as parent. Neither printed names nor equal spans identify a
host or alias. `@deprecated`'s cheap flag is provisional and requires parsed
confirmation. `@see`/`@link` force eager work for unused-identifier checking.

The classifier has no semantic cache or changed ownership: immutable source
comment bytes in the scanner's trivia window produce the current token's
flags, reset with each token. Its expensive downstream worker is JSDoc comment
parsing; this patch only corrects the input flag. No reuse extension or work
reduction is claimed. Integrator Beads follow-up request: measure first-demand
queries, completed hits, actual `parseJSDocComment` executions, publication
node counts and copy bytes before extending the lazy reuse boundary.

## Required integration prerequisites (not implemented)

A parser-only cutover cannot preserve current consumers. Do not turn off
`ParseOptions.jsdoc`, return empty docs, eagerly force all entries from `iter`,
or introduce a second speculative cache as a substitute.

1. **Separate JS/JSX from TS/TSX in parser dialect contracts.**
   `parser.rs::ScriptKind` maps `.js/.cjs/.mjs` to TypeScript and `.jsx` to Tsx.
   Native `Parser.isJavaScript` controls eager parse, JSDoc diagnostics and
   `reparseTags`; native JS also uses JSX grammar. Adding enum arms changes
   exported contracts; integration must serialize exhaustive consumers and
   `crates/tsr-compiler/src/loader.rs::parse_options` (currently promotes JS to
   Tsx depending on the compiler JSX option). Parser owner can then implement
   its dialect side without changing others' files.
2. **Replace eager entry enumeration with Program-owned demand access.**
   `checker.rs::Checker::set_jsdoc` copies materialized slices into
   `jsdoc_entries` and derives `jsdoc_hosts`; native reads SourceFile's cache.
   `crates/tsr-conformance/src/types_producer.rs:1328` enumerates
   `file.jsdoc().iter()`. Compiler `lib.rs` calls binder
   `bind_into_with_jsdoc` at lines 485 and 505; conformance
   `diagnostics_suite.rs:409` does likewise. Binder JS reparser consumers need
   eager JS docs, whereas TS callers must distinguish eager-only lookup from
   demand lookup. Existing checker direct lookups span
   `symbols.rs::jsdoc_parameter_annotation`, `jsdoc_type_annotation`,
   `jsdoc_cast_annotation`; `signatures.rs::jsdoc_return_annotation`,
   `jsdoc_this_parameter_type`, `jsdoc_has_no_function_return_annotation`;
   `jsdoc_params.rs::all_jsdoc_tags`, `jsdoc_reparsed_function`;
   `jsdoc_annotations.rs::last_jsdoc_satisfies_tag`;
   `jsdoc_modifiers.rs::jsdoc_reparsed_modifiers`;
   `jsdoc_links.rs::check_jsdoc_link_references`;
   `type_argument_arity.rs::jsdoc_augments_type_arguments`;
   `heritage_conformance.rs::check_class_implemented_types`.
   Integrator must migrate every direct `jsdoc_entries` lookup, not only the
   setter. Parent/alias bridge semantics must remain consistent with
   `Checker::jsdoc_hosts` until the native concrete-host parent contract is
   serialized. No parser shim is supplied.
3. **Define late-node publication into Program tables.**
   `ParsedFile` owns immutable `Arc<NodeTable>`; `publish` relocates a fixed
   private node range. Lazy JSDoc uses the ordinary type parser, registering
   nodes that checker/binder readers require in NodeTable and NodeMap.
   `JSDocTable::publish` currently publishes only already-existing entries.
   Late demand therefore requires integration-owned table/map allocation and
   identity contracts, not an isolated parser whose IDs collide with existing
   nodes. `crates/tsr-compiler/src/file.rs::File::jsdoc` currently exposes a
   borrowed table; compiler-owned tables and all consumers must adopt the
   agreed late-publication provider. Arena is Send but not Sync; preserve the
   established file-worker ownership boundary rather than copying native locks
   onto an unsynchronized bump allocator.
4. **Keep the JS reparser cutover coordinated.** Native `withJSDoc` calls
   `reparseTags` eagerly only in JS. Current hosted/unhosted JSDoc semantics are
   spread across binder/checker consumers, including `jsdoc_params.rs`.
   Parser reparser contract remains this lane's ownership, but caller cutover
   must be serialized by integration. None of the named JS cases is proven
   to be fixed by delaying TS documentation work.

## Verification at this checkout

Base checkout supplied: `5dd3bad84d12991e1ba169d2d5687321e1989740`.
Stable toolchain is `rustc 1.99.0`; offline registry worked without bootstrap.
Native tsgo built
with Go 1.26.8 and the pinned checkout without tracked changes.

Frozen release TSR SHA256:
`9a7745e938d62c13e3bcbf33039763cce31742f4cd9e5d56e8c74b099185967a`.
Candidate release TSR:
`f979253810ac402c9427aa3d173b3d7c6cbb97fa1949e8cd195bbdabcb225063`.
Native tsgo:
`7b85aa10584504012af7b3ec075675649675f2d76ca4bde51019f425c5a62302`.

- Workspace release tests passed, including the new classifier regression.
- Workspace fmt check passed after installing rustfmt in the disposable Box.
- Workspace clippy failed on pre-existing `tsr-vfs` test `assert_is_empty`
  warnings (`glob.rs:708`, `os.rs:532,543,544`). Scanner dependency-inclusive
  clippy also exposes existing `tsr-core/index.rs:161` `double_must_use`.
  `cargo clippy -p tsr-scanner --lib --no-deps -- -D warnings` passed.
- Baseline/candidate unfiltered dumps are byte-identical. Types: 477970 verdict
  rows, RIGHT 469765, WRONG 7212, GAP 993; diagnostic rows: 10570, RIGHT 4221,
  WRONG 1281, EMPTY_RIGHT 4968, EMPTY_WRONG 100. Zero formerly RIGHT line or
  RIGHT/EMPTY_RIGHT case losses; zero conversions.
- Full read-only coverage smoke invoked all 16 existing suite implementations
  via an external throwaway Rust executable, preserving prohibited snapshots.
  All completed on 12444 discovered cases. `checker_types` 8042/9538,
  skipped 2906, line rate 98.1017218%; `diagnostics` 4221/5502, skipped 6942.
  This is the existing suite population, not strict full message/span/order or
  varied-option completion evidence. The snapshot-writing `coverage` CLI was
  intentionally not run because snapshots are outside lane ownership.

Named corpus cases were validated against discovery and baseline dumps.
Diagnostic RIGHT: `conformance/jsdocPrivateName1`.
EMPTY_RIGHT: `compiler/lateBoundAssignmentCandidateJS1`,
`conformance/jsdocTypeTagOnParameter1`,
`conformance/typeFromPrivatePropertyAssignmentJs`,
`conformance/typeTagOnPropertyAssignment`.
WRONG: `compiler/jsDeclarationsInheritedTypes`,
`compiler/parseJsxElementInUnaryExpressionNoCrash1`, `...NoCrash2`, `...NoCrash3`,
`conformance/jsdocCatchClauseWithTypeAnnotation`, `conformance/jsdocImportType`.
All remain unchanged. No target case conversion is claimed.

Fresh-process interleaved public-project comparisons, filesystem warmed,
21 pairs per comparison, normal harness noEmit/incremental=false/composite=false:

| Project | Candidate/base observed wall | Candidate/base CPU | Candidate/native observed wall |
|---|---:|---:|---:|
| domain-model | 0.998854 | 1.077355 | 1.017127 |
| generic-imports | 0.995384 | 0.996520 | 0.932497 |

Noisy domain CPU retried with 41 pairs: wall 1.000986, CPU 0.999392.
Loaded-file scope, effective options and diagnostic fingerprints match in all
comparisons. Each project reports the same complete TS2322 message as its
comparator. Harness explicitly leaves `work_comparable`,
`complete_input_equivalence_verified`, `actual_checked_work_verified`, and
`target_verified` false: complete cross-tool query-input coverage and actual
performed checker work/budgets are unverified. These are observed measurements,
not a verified <=0.50 ratio or evidence that the root lazy port is complete.
