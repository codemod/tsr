THIS FILE IS THE NON-CHECKER, NON-DIAGNOSTICS CONFORMANCE WORKSTREAM'S HANDOFF.

CURRENT PROGRESS (2026-08-08, thirteenth session on main — the binder-symbols push)

  parser_typescript      5,031/5,031 = 100.00%
  printer_round_trip   11,776/11,776 = 100.00% (was 11,755/11,778 — COMPLETE;
                                                two error-recovery cases became
                                                faithful parse-diagnostic skips)
  binder_symbols          8,422/8,461 = 99.54% (was 8,310/8,473; the thirteenth
                                                session added late-bound bracket
                                                spellings, const-propagated
                                                computed names, export-default
                                                naming, module-specifier and
                                                namespace-import transparency,
                                                exact unit pairing, and the
                                                reserved-word parser fidelity
                                                that moved 12 error-recovery
                                                cases out of the population
                                                faithfully)
  dts_emit                  333/374  = 89.04%  (was 327/374; +6 this session)
  dts_shape                860/1,008 = 85.32%  (was 859/1,007; the `!!!!`
                                                marker fix moved one case in)
  dts_reachable_target     492/1,162 = 42.34%  (population, unchanged)

Twelfth session, in commit order:
  36c5918  printer_round_trip -> 100%: the parser's constructor arm ported
           faithfully (commits on the `constructor` keyword alone, parses type
           parameters and a return type, string-literal "constructor" + `(`;
           parser.go:1917), an asterisk commits to a method (parser.go:1944),
           setters print their return type annotation, and JSDoc-owned
           QuestionToken/DotDotDotToken/AsteriskToken left the round-trip
           histogram (they are trivia the comment-free print can never
           reproduce — the module doc's own exclusion, made positional).
           Also: dts_emit path references rebase against the emitted
           declaration file's directory (transform.go:464) — commonSourceDirectory.
  6cbaa35  annotated object-literal accessors keep their shape: get/set pair
           keeps both signatures, lone getter -> readonly property, lone
           setter -> mutable property. examples/dtsdump.rs added (produced vs
           expected text per unit).
  feb1885  an arrow returning an annotated name copies the annotation
           (typeReferenceDirectives4/6), gated on certain resolution: own
           annotated parameter first, else file scope only when no scope can
           intervene; `typeof` annotations never copy (they resolve through
           upstream; typeReferenceDirectives7 keeps its honest any).
  <marker> `!!!!` baseline annotations are runner metadata, not section text
           (noEmitOnError; the noCheck emit is the right oracle for a
           checker-free emitter).
  790e1e8  JSDoc @returns types returns and setter @param types the value
           parameter in JavaScript files (declarationEmitClassAccessorsJs1).
  8dcdc71  binder suite indexes alias-transparent spellings: the .symbols
           baseline is checker-written and prints an aliased symbol under the
           alias's name with the target's declarations; entity
           `import x = a.b.c` resolves through resolve_name + exports tables
           and indexes its target (and members, three deep) under the alias
           spelling. No binder behavior changed. +91 cases.

What remains, classified (from dts_failures + the binder snapshot at 8dcdc71):
  - dts_emit 41 failures: ~25 need the checker (inferred arrow/function types,
    import synthesis driven by inferred types, TS7056 suppression, node-builder
    alias resolution); ~10 are parse skips on cases whose sources deliberately
    mix invalid syntax (they stay skips faithfully); the residue is comment
    preservation inside types (unionTypeWithLeadingOperator,
    declarationEmitWorkWithInlineComments) and the CommonJS `exports.x = …`
    declaration family (assignmentToVoidZero1, jsDeclarationEmitExportAssigned*).
  - dts_shape 148 failures: dominated by checker-driven import
    synthesis/retention ("want import") — the emitted type names the import,
    and the type is inferred. Not reachable per-unit without the checker.
  Then four binder-suite follow-ups (d7d765a, c800d98, 814fd4d and the
  compounding fix) took binder_symbols 8,401 -> 8,408: transparency compounds
  through alias members (circularImportAlias), chains resolve through
  intermediate aliases (importStatementsInterfaces), `import x = require("m")`
  resolves ambient-module targets (privacyGloImport), and line counting honors
  CR/LS/PS breaks (allowUnescapedParagraphAndLineSeparators…).

  - binder_symbols 39 failures, all filed: the JSDoc declaration family
    (~10, `bd tsr-2` — typedef/callback/template/overload lines need the
    JSDocTable plumbed into bind, the arena precedent shows the shape), the
    augmentation-of-reexport family (~8, `bd tsr-3` — checker-owned merge,
    the namespace_import_target seam is where a transparency layer would go),
    late-bound expando/contextual members (~6, `bd tsr-4` — one case's
    generated-Go baseline disagrees with a plain reading of
    GetPropertyNameForPropertyNameNode's signed arm; resolve against the
    oracle first), and singles (unicode JSX attributes, ramdaTools type-alias
    scoping, moduleElementsInWrongContext recovery lines).

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

Empty binding patterns are now elided, and patterns containing defaulted binding
elements flatten to their bound names when declaration syntax cannot retain the
initializer. Function and method overload implementations are omitted whenever a
same-named signature sibling exists, including inside namespaces and classes.
Those sibling-list fixes raised `dts_emit` by eight and `dts_shape` by twenty-one
from the previous checkpoint.

Syntactically visible function expando assignments now promote function-valued
variables to function declarations and emit their properties through a merged
namespace, including constant computed property names. Untyped property
signatures emit `any`, while computed members whose name cannot be represented in
a declaration are omitted from classes, interfaces, and written type literals.
This pass added five byte-exact cases and two declaration-shape cases.

Function declarations now receive the same syntax-visible expando namespace,
including empty namespaces for non-nameable late-bound keys. Named default-export
functions are lowered to a declaration/namespace merge followed by `export
default`, keeping the result parseable. This closed five more declaration-shape
cases. Public optional parameter properties now retain their implicit `undefined`
in both the synthesized property and constructor parameter, adding one byte-exact
case without changing private isolated-declaration output.

Declaration parameters now recursively remove binding-element defaults while
preserving the source pattern's trailing comma as an explicit AST flag carried
through transformed nodes and the shared list printer. Destructured constructor
parameter properties flatten to their bound names when their element types are
written directly or reachable through a non-generic top-level tuple/object alias.
These syntax-only passes added four byte-exact declaration cases without changing
the declaration denominator, parser clean-file result, or printer round trip.

`stripInternal` now consumes the corpus option and original source trivia without
requiring checker state. The declaration transform removes statements, class and
type members, and synthesized parameter-property fields whose closest leading
comment contains `@internal`, while retaining the corresponding constructor
parameters. This closed `compiler/stripInternal1`; the inline-comments case now
reaches its later comment/layout mismatch instead of failing on extra properties.

Declaration emit now opts into source-aware printing for the nearest leading
JSDoc on statements and class/type members, deduplicating comments when several
synthesized nodes share an original range and honoring `removeComments`. Structural
`@overload`/`@constructor` blocks remain excluded until they can be mapped to the
right transformed overload nodes. This closed `commentsVariableStatement1`,
`seeTag1`, and `seeTag2` without changing declaration shape.

Declaration emit now carries `strictNullChecks` into the syntax-only type builder.
Strict cases preserve `null` as a literal type recursively, while explicit
non-strict cases continue widening it to `any`. This closed
`conformance/exportDefaultExpressionComments` without changing declaration shape
or any checker-owned behavior.

Syntax-built object type members now retain the original property/method spans,
allowing source-aware declaration printing to recover their JSDoc. Nested
const-object string and template literal types reuse source spelling, standalone
literal-const values canonicalize to double quotes, and prefixed bigint syntax
widens according to its operand. Together these close
`compiler/isolatedDeclarationsLiterals` without changing declaration shape.

Non-ambient namespace bodies now run export reachability after declaration
transformation. Private types erased from class members therefore stop retaining
private aliases and declarations, while types still present in exported members
continue pulling their dependencies into the namespace. Preserved declaration
references also normalize an explicit same-directory `./` prefix. This closes
fifteen declaration-shape cases and seven byte-exact cases without changing any
suite population.

External-module files now run the same reachability fixpoint after declaration
transformation. Top-level declarations retained only by types erased from private
class members are removed, while dependencies named by emitted public types,
export declarations/assignments, and side-effect imports remain. This closes
another thirteen declaration-shape cases and seven byte-exact cases without
changing any suite population.

Visibility collection now treats declarations inside module/namespace bodies as
bindings in that nested scope. An augmentation-local interface name or self type
therefore no longer retains a same-named file-level import, while member types
that genuinely name an external declaration still retain their imports. This
closes seven declaration-shape cases and eight byte-exact cases without changing
any suite population.

Mapped types now follow upstream's multiline printer default instead of silently
assuming a SingleLine emit flag. Types copied into synthesized arrow-function
signatures retain the compact form through an explicit synthesized-signature
print context. Together these layout paths close six byte-exact cases without
changing declaration shape, suite populations, or printer round-trip semantics.

Mapped `+`/`-` modifier tokens now survive parsing and reconstruct their implied
`readonly`/`?` syntax, including adjacent `+?` and `-?` pairs. Declaration
transformation also recursively normalizes a missing mapped value annotation to
`any` without changing the parser's recovery tree. These close two more
byte-exact cases without changing declaration shape or suite populations.

Empty namespace bodies now print single-line `{ }` exactly when the original
block's source braces shared a line, even if the body's statements were filtered
away; source-multiline empty bodies stay multiline, and empty interface or
type-literal bodies are unaffected. This closes two more byte-exact cases
(`declareDottedModuleName`, `moduleSymbolMerging`).

A source-written `declare` modifier is now masked off declarations whose parent
is not the source file, matching `ensureModifierFlags`; four transform arms had
hardcoded file-level modifier context (`privacyVarDeclFile`). `declarationMap`
now appends the trailing `//# sourceMappingURL=` comment — the map itself is not
needed for byte-exact output (`declarationMaps`). File-level visibility now
carries the names reachability passed through, so a retained multi-declarator
`var` statement prunes declarators nothing reaches
(`declareFileExportAssignmentWithVarFromVariableStatement`, +1 shape).

The over-emission check no longer judges units whose analysis reports
diagnostics: upstream suppresses declaration output per file when that file has
declaration-emit errors (`isolatedDeclarationErrorsDefault` emits `f.d.ts` and
nothing for its five erroring units), and units needing inference are already
outside the judged population. Three emit and eight shape cases.

Module detection now honors the non-syntactic inputs: a `.mts`/`.cts` unit (or
`moduleDetection: force`) is a module with no import/export syntax, so its
private declarations drop and the `export {}` marker remains
(`moduleDetectionIsolatedModulesCjsFileScope`, +1 emit and +1 shape). A binding pattern destructuring a namespace import
keeps its shape typed `typeof` the entity, with pattern bound names joining
reachability and the source phase retaining the entity's import
(`declarationEmitExpressionInExtends6`, +2 shape); patterns destructuring any
other entity honestly moved to the needs-inference bucket
(`declarationEmitNonExportedBindingPattern`, `destructuredDeclarationEmit` —
their wants restate checker member types), shrinking the emit denominator
378 -> 374. The reachable-target population corrected 496 -> 492: pattern
statements upstream analyzes were invisible to the reachability-filtered
analysis until pattern bound names joined `declared_names`, and an initially
committed analysis exemption for namespace-import destructuring was reverted
as diverging from upstream's isolatedDeclarations verdicts
(`declarationEmitExpressionInExtends6` emits correctly but sits in the
inference bucket, exactly as upstream classifies it).

Baseline declaration sections name the *output* location while units name the
*source* one, so exact-name matching silently excluded every
outDir/declarationDir-remapped case as "no emitted .d.ts section" — upstream had
emitted. A unique-basename fallback (declining any ambiguous pairing) grew the
emit denominator 341 -> 395 and shape 918 -> 1,016; 25 emit and 67 shape of the
new population pass at once, and the newly visible failures are genuine
JS-declaration JSDoc and program-level import gaps, not mispairings. Scores
before and after this change are not directly comparable.

The complementary correction: input echoes are also flattened to basenames, so
exact-name exclusion failed for pathed inputs and paired emitted units against
node_modules stubs and lib-override inputs
(`moduleLocalImportNotIncorrectlyRedirected`, `libTypeScriptOverride*`,
`duplicatePackage_subModule` were mispairings, not import-retention bugs). The
echo region is now the longest prefix of sections whose basename and content
match an input unit; only sections after it are outputs. Nineteen emit and
eleven shape bogus comparisons left the population.

A default-exported primitive literal now keeps its canonicalized value as the
synthesized const's initializer (`declare const _default = 0;`) instead of
widening (`modulePreserve4` progressed, `nodeNextCjsNamespaceImportDefault2`
closed), and node_modules units are excluded from output pairing — they are
program inputs upstream never emits (`compositeWithNodeModulesSourceFile`).
JSDoc `@protected`/`@private` tags now act as accessibility modifiers on
JavaScript class members, keyed off the root `JAVASCRIPT_FILE` node flag the
harness stamps because the parser never sees file names
(`lateBoundAssignmentCandidateJS3`).

Output pairing is now positional per basename group: exact-name claiming was
actively wrong when two flattened outputs share a name
(`moduleDeclarationExportStarShadowingGlobalIsNameable` closed). A preserved
reference already naming a declaration file keeps its name — rewriting produced
`bar.d.d.ts` (`commonSourceDirectory` now fails only on outDir-relative path
depth, which needs output-path awareness).

The first slice of JSDoc-typed JavaScript declaration emit is in: constructor
`this.X = …` assignments synthesize typed class properties before the
constructor (typed by `@type`, else initializer widening) carrying the
assignment's span so comment replay emits their JSDoc; `@param {T} name` types
untyped parameters; `@implements A` appends a heritage clause. The type builder
covers keywords and bare names only — unions, generics, and function types
still fall to `any` (the whole `argumentsReferenceInConstructor*_Js` family
plus three `jsdocImplements*` cases, +8 exact).

The second JSDoc slice parses braced type texts for real: a type is
whitespace-padded to its original file offset and parsed by
`tsr_parser::parse_standalone_type`, whose nodes continue the file's own node
table so the grafted subtree behaves exactly like parsed syntax. On top of
that, `@typedef {T} Name` and `@callback Name` (+`@param`/`@returns`)
synthesize type aliases with `@template` type parameters, placed before the
top-level statement containing their comment and exported in modules; `@type`
annotates written class properties and single-declarator variable statements
(+2 emit, +13 shape). Comment replay now walks every JSDoc block in a node's
leading trivia, and an alias owns its consecutive typedef-comment run exactly
when a blank line or EOF follows the run (`recursiveTypeReferences2` pins all
arms); rest params keep their written element type behind moved dots, and
`@typedef {Object}` + `@property` builds the object literal
(`jsdocTemplateTagNameResolution`, `callbackTagVariadicType`,
`importDeferJsdoc`, +3 emit, +2 shape). Comment-run ownership is kind-aware —
a following non-typedef comment claims a `@typedef` run's trivia but not a
`@callback`'s — grafted nodes carry `NodeFlags::REPARSED` so reparsed mapped
types print single-line, and nested property assignments on an empty-object
const spell its type literal (`typeFromPropertyAssignment39`, +1 emit). A
dotted `@typedef` name wraps in `export declare namespace` with the alias
exported inside, its comment staying on the host statement
(`jsDeclarationsImportNamespacedType`, +1 emit and +1 shape). A binding pattern destructuring a namespace import
keeps its shape typed `typeof` the entity, with pattern bound names joining
reachability and the source phase retaining the entity's import
(`declarationEmitExpressionInExtends6`, +2 shape); patterns destructuring any
other entity honestly moved to the needs-inference bucket
(`declarationEmitNonExportedBindingPattern`, `destructuredDeclarationEmit` —
their wants restate checker member types), shrinking the emit denominator
378 -> 374. The reachable-target population corrected 496 -> 492: pattern
statements upstream analyzes were invisible to the reachability-filtered
analysis until pattern bound names joined `declared_names`, and an initially
committed analysis exemption for namespace-import destructuring was reverted
as diverging from upstream's isolatedDeclarations verdicts
(`declarationEmitExpressionInExtends6` emits correctly but sits in the
inference bucket, exactly as upstream classifies it). JSDoc-only type
spellings rewrite to TypeScript before grafting: bare `?` is `any | null`,
postfix `?`/`=`/`!` are null/undefined unions or plain, `function(...)`
collapses to `Function`, and `X.<...>` generics lose the dot with `Object.<`
becoming `Record<` (`jsDeclarationsReusesExistingNodesMappingJSDocTypes`, +1
emit). A qualified name's root resolves in namespace space, where type
parameters never shadow, so a signature's `<E>` cannot drop the `import * as
E` its annotation names (`declarationEmitRetainedAnnotationRetainsImportInOutput`,
+1 emit and +1 shape). A binding pattern destructuring a namespace import
keeps its shape typed `typeof` the entity, with pattern bound names joining
reachability and the source phase retaining the entity's import
(`declarationEmitExpressionInExtends6`, +2 shape); patterns destructuring any
other entity honestly moved to the needs-inference bucket
(`declarationEmitNonExportedBindingPattern`, `destructuredDeclarationEmit` —
their wants restate checker member types), shrinking the emit denominator
378 -> 374. The reachable-target population corrected 496 -> 492: pattern
statements upstream analyzes were invisible to the reachability-filtered
analysis until pattern bound names joined `declared_names`, and an initially
committed analysis exemption for namespace-import destructuring was reverted
as diverging from upstream's isolatedDeclarations verdicts
(`declarationEmitExpressionInExtends6` emits correctly but sits in the
inference bucket, exactly as upstream classifies it).

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

1. ~~CLOSE THE 30 CLEAN-FILE PARSER FAILURES~~ DONE — parser_typescript reads
   5,031/5,031 and printer_round_trip 11,776/11,776 (twelfth session header).

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
