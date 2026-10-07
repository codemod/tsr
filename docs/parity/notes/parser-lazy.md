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
