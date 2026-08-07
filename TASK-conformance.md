THIS FILE IS THE NON-CHECKER, NON-DIAGNOSTICS CONFORMANCE WORKSTREAM'S HANDOFF.

CURRENT PROGRESS (2026-08-07)

  parser_typescript      5,031/5,031 = 100.00% (up from 5,001)
  dts_reachable_target     496/1,162 = 42.69%  (up from 495; corrected visibility
                                                exposed one inference case)
  dts_emit                  215/341  = 63.05%  (up from 161/339)
  dts_shape                 680/918  = 74.07%  (up from 618/912)
  printer_round_trip    11,755/11,778 = 99.80%

The parser clean-file milestone is complete. Its harness now prepares virtual
tsconfig/jsconfig files and walks actual program roots and dependencies, so the
100% result does not come from skipping JavaScript roots whose `allowJs` setting
lives in config. The first printer pass preserved typed tagged templates, import
type attributes, trailing array elisions, JSX attribute raw text, and escaped
private identifiers. A second pass added stable recovery for generic-arrow object
constraints, malformed export assignments, digit-starting decoded identifiers,
and `new <T>C()`; 23 printer failures remain, almost entirely JSDoc/comment
retention and malformed constructor/accessor recovery trees. Declaration shape
now synthesizes collision-free `_default` bindings for default and export-equals
expressions and retains classes named by default `new` expressions.

The all-failure classifier is now
`cargo run -p tsr-conformance --example dts_failures`. Add `-- --cases` for one
tab-separated row per non-pass. It correlates the two declaration suites, so a
`dts_emit` mismatch is called exact-text-only only when `dts_shape` passes. The
first full classification found 243 wrong-kind/name/order shape failures, 24
extra declarations/output files, and 16 missing declarations. The first
checker-free structural pass retained written types inside arrow/function
initializers, kept module/global augmentations and their imports, and retained
every declaration participating in a merged symbol. After that pass the live
shape residue is 227 wrong-kind/name/order, 26 extra, and 15 missing; parser skips
remain 6 source units and 8 upstream declaration baselines.

The scanner/AST/printer pipeline now also carries upstream's `SingleQuote` token
flag. Reused module names, property names, and literal types therefore preserve
their source delimiter instead of being unconditionally rewritten with double
quotes. This raised byte-exact `dts_emit` by another 12 cases without changing
declaration shape or printer round-trip.

The next exactness pass removed the separator guard's spaces between nested type
argument closers, preserved negative numeric literal type signs in the parser,
formatted reconstructed numeric constants at JavaScript's exponent thresholds,
kept uninitialized ambient enum members uninitialized, stripped `override` from
interface method signatures, and lowered `import defer` to an ordinary import in
declaration output. Together these raised `dts_emit` by another 10 cases, again
without changing declaration shape or printer round-trip.

Preserved triple-slash `path`, `types`, and `lib` references now precede emitted
declarations in their original mixed-kind order. Path references are rewritten
to declaration suffixes, resolution modes are retained, and attribute values are
escaped. This closed another two byte-exact cases without changing declaration
shape or printer round-trip.

The next structural pass propagated ambient context through nested namespaces,
preventing synthesized `export {}` markers from leaking into five declaration
files. Class overload implementations are now omitted, repeated private overloads
collapse to one nominal marker, initialized parameters before required parameters
emit `| undefined`, and untyped type-member parameters emit `any`. Together these
raised `dts_emit` by ten cases and `dts_shape` by five from the previous checkpoint.

Syntactically apparent arrow returns and empty function bodies now produce
function types instead of falling back to `any`, closing ten more byte-exact
cases. Non-entity class bases are hoisted to collision-free `Class_base`
declarations and referenced from the rewritten `extends` clause; their type stays
`any` when checker inference is required and the existing diagnostic remains.
That restores the declaration structure in another eighteen `dts_shape` cases
without changing the checker-free `dts_emit` denominator.

`TASK.md` belongs to the checker-types gradient and `TASK-diagnostics.md` belongs
to diagnostics. Do not put work from either stream here. This file owns the
remaining parser, conformance-harness, declaration-transform, and printer work
identified by these coverage rows:

  parser_reachable_target  5,031/10,570 = 47.60%
  parser_typescript        5,001/5,031  = 99.40%
  dts_reachable_target       495/1,162  = 42.60%
  dts_emit                   161/339    = 47.49%
  dts_shape                  618/912    = 67.76%

THE BOUNDARY

Excluded, because other agents own them:

  - checker implementation, checker type inference, and a CheckerResolver for
    declaration emit;
  - diagnostic rules, diagnostic comparison, parser-diagnostic extraction, and
    changes whose product is a new diagnostic oracle;
  - making the 667 inference-needing declaration cases byte-exact by asking the
    checker for variable, parameter, function, or method types.

Do not try to raise either `*_reachable_target` percentage by weakening its
predicate. They are population measurements, not compiler pass rates.
`parser_reachable_target` is capped because error baselines mix syntactic and
semantic diagnostics. `dts_reachable_target` is deliberately the checker-free
population. Preserve both meanings.

FIRST: RE-MEASURE AND CLASSIFY THE LIVE TREE

The checked-in snapshots are capped at the first 100 failures and the declaration
document's residue counts predate the corrected `dts_emit` denominator. Before a
large change, add or use a read-only classifier that walks all failures and emits
case-level buckets for `dts_emit` and `dts_shape`. Do not price work by counting
the first 100 snapshot entries. The classifier must keep these distinct:

  - wrong declaration kind/name/order;
  - missing declaration;
  - extra declaration or output file;
  - exact-text-only difference;
  - source parse skip;
  - emitted-baseline reparse skip;
  - unsupported printer kind.

Do not change suite inclusion or comparison rules while classifying. A classifier
is instrumentation, not a new gate.

RANKED WORK

1. CLOSE THE 30 CLEAN-FILE PARSER FAILURES

This moves `parser_typescript`, not `parser_reachable_target`. The current snapshot
breaks down into these bounded families:

  - 7 private-name member/call/class-expression cases;
  - 6 JavaScript-mode cases, including JSX-in-JS, package fixtures, React source,
    and untyped modules;
  - 6 conditional, mapped, generic-arrow, `infer`, and `satisfies` ambiguities;
  - 4 import-defer/import-attribute/resolution-mode cases;
  - 3 dynamic TSX tag-name cases;
  - 2 auto-accessor/decorator-expression cases;
  - 2 malformed/default-named-import recovery cases.

For each family, compare the parser branch and recovery position with upstream,
add a focused parser regression test, then re-run `parser_typescript`. Do not
change diagnostics ownership or add checker errors here. Definition of done:
5,031/5,031 on the existing clean-file denominator with no scanner regression.

2. RECOVER THE PARSER-RELATED `.d.ts` SKIPS

Current bounded prerequisite residue:

  - `dts_emit`: 16 cases where a source unit does not parse cleanly;
  - `dts_shape`: 10 cases where a source unit does not parse cleanly;
  - `dts_shape`: 10 cases where the upstream `.d.ts` baseline does not reparse.

Map these cases onto the parser families above. Fix parser syntax or recovery when
the source is valid. For upstream declaration text that does not reparse, first
prove whether the gap is unsupported valid `.d.ts` syntax or an intentionally
invalid upstream baseline; only the former is parser work. Never turn a parse skip
into a declaration failure by emitting from a recovery tree.

3. REMOVE THE PRINTER'S EXPLICIT UNSUPPORTED CASE

Implement `NoSubstitutionTemplateLiteral` in the printer and cover both ordinary
source printing and declaration emit. It currently accounts for one unsupported
`dts_emit` case and two unsupported `dts_shape` cases. Verify that
`printer_round_trip` does not regress.

4. FIX DECLARATION STRUCTURE THAT IS DECIDABLE WITHOUT THE CHECKER

This is the primary `dts_shape` workstream. Work from a fresh all-failure
histogram, but begin with the repeated families already visible in the snapshot:

  - preserve side-effect imports and imports required by module augmentations;
  - retain `declare module`, `declare global`, and namespace/module augmentation
    structure;
  - generate declarations for binding patterns instead of emitting `<pattern>`;
  - handle syntax-visible default-export declarations and anonymous declaration
    names such as `_default`;
  - preserve source-order declaration merging where binder/program facts already
    make the merge visible;
  - emit syntax-visible helper declarations for expression-based class heritage
    only where their shape does not require inferred types;
  - implement syntax-visible CommonJS/export-assignment structure;
  - retain imports and scope-fix markers based on file/module facts already
    available from the binder or Program;
  - stop over-emitting declaration files for units upstream does not emit.

Boundary test for every item: if choosing the declaration, its name, or its type
requires a checker query, leave it to the checker workstream. Program-level facts,
module relationships, binder symbols, source syntax, and JSDoc parsing are in
scope. Do not smuggle inferred `any` into this stream to make a shape pass.

Every structural fix needs:

  - a focused `tsr-declarations` test;
  - the relevant `dts_shape` cases moving from fail to pass;
  - no newly passing case obtained by shrinking the denominator;
  - a `dts_emit` measurement, since structural fixes can move both gates.

5. ADD THE NON-CHECKER PROGRAM/JSDOC INPUTS THE TRANSFORM IS MISSING

Port only the portions that can be answered without type inference:

  - module-augmentation import preservation, which needs cross-file Program facts;
  - CommonJS export-assignment and syntax-visible expando declaration structure;
  - JSDoc parsing and comment/source-node retention needed by declaration emit;
  - JavaScript declaration forms whose declaration kind and name are explicit in
    source or JSDoc.

Keep type-bearing JSDoc conversion and expando property types out when they require
checker types. A useful intermediate result is improved `dts_shape` even when
checker-free `dts_emit` still skips the case for inference.

6. RAISE BYTE-EXACT DECLARATION PRINTING

After each structural slice, classify the remaining `dts_emit` failures by first
difference and address exact-text families independently:

  - preserve JSDoc and declaration comments with source-node attachment;
  - preserve original quote style for module specifiers and literal types;
  - reproduce blank lines, multiline empty lists, indentation, and statement
    separators exactly;
  - preserve literal spelling when upstream reuses source text;
  - preserve declaration ordering after generated/import-retention nodes are
    inserted;
  - remove residual `any` differences only when the required type is derivable by
    the existing syntactic type builder. Checker-derived types are out of scope.

For printer-only changes, `dts_shape` should normally remain unchanged. If it
moves, inspect why before accepting the change: either output became parseable,
or a supposedly textual edit changed declaration structure.

7. IMPLEMENT PER-CONFIGURATION CORPUS EXECUTION

There are 757 configuration-varied parser cases currently excluded. Add a harness
execution model that expands a case into its upstream option variants and pairs
each run with the matching `(target=...,module=...,...)` baseline. This work owns
case discovery, option parsing, run identity, and baseline pairing only. It does
not own interpreting mixed error baselines or adding diagnostic rules.

Required controls:

  - variants are separate judged runs, never merged into one expected output;
  - option order in a baseline name does not create ambiguous matches;
  - a missing exact variant baseline is not read as "expects no diagnostics";
  - existing plain-baseline denominators remain byte-identical before variants are
    deliberately added to a suite;
  - module-resolution and file-loader variants continue to match their existing
    baselines.

This enables broader future coverage, but by itself must not claim the 757 mixed
error cases as parser passes.

8. AUDIT THE CORPUS-ORACLE EXCLUSIONS WITHOUT CREDITING THEM AS COMPILER WORK

Two excluded populations need upstream/corpus resolution:

  - 500 known `.diff` divergences;
  - 617 cases with no recorded upstream output.

Classify them by whether a reproducible upstream run can generate an authoritative
baseline at the pinned commit. If regenerated baselines are introduced, record the
upstream command and pin and review denominator changes separately from compiler
changes. Never infer "clean" from absent output and never delete `.diff` handling
just to enlarge a denominator.

MEASUREMENT RULES

  - Always report numerator and denominator, not percentage alone.
  - Keep denominator changes in a separate commit from parser/emitter fixes.
  - Run all four declaration rows together:
      isolated_declarations
      dts_reachable_target
      dts_emit
      dts_shape
  - Run parser changes against:
      scanner_termination
      scanner_clean_files
      parser_typescript
      printer_round_trip when parser AST shape changes
  - A higher `dts_reachable_target` caused by silencing `TS9xxx` analysis is a
    regression unless the syntactic builder is proven to emit the exact type.
  - A higher pass rate caused by a smaller denominator is not implementation
    progress. State the population change separately.

SUGGESTED EXECUTION ORDER

  A. Full failure classifiers and the bounded template-literal printer support.
  B. Parser's 30 clean-file failures and the declaration parse skips they unblock.
  C. High-frequency checker-free structural declaration families.
  D. Program/JSDoc inputs needed by those structural families.
  E. Exact declaration text: comments, quotes, directives, and formatting.
  F. Per-configuration harness execution.
  G. Upstream `.diff` and no-output audit.

Do not stop this stream because checker-dependent cases remain. Its completion
condition is that every remaining failure or skip is classified as checker-owned,
diagnostics-owned, upstream-oracle-owned, or a measured low-frequency residual;
all parser, harness, transform, and printer families above have focused tests and
fresh corpus measurements.
