# Parity round 4 — cloud lanes

Ten cloud boxes, epic `tsr-2zk`, dispatched from the integration branch
`claude/beautiful-shannon-ar5gh0` after it merged `main` at `9f6db8a8`.
Every box follows [box-protocol.md](box-protocol.md) and the round-4 rules
below; this file adds only what is specific to this round.

## Why these ten lanes

`main` is ported concurrently by local boxes (118 non-merge commits between
the round-2 merge `b3cd078d` and `9f6db8a8`). Their lanes take whole
subsystems — relation arms and elaboration, property access, call and
overload resolution, inference candidates, declarations and heritage,
the symbol-chain printer (`tsr-2zk.39`), type aliases (`tsr-2zk.16.2`),
written-node reuse, laziness and parse/bind performance — and they commit
against `tsr-2zk.16.x` roots without changing the roots' Beads status. A root
being `open` and unassigned is therefore not evidence that nobody is working on
it. Each lane below was chosen so that:

1. its issues are `open`, unassigned, and cited by no commit on `main`
   (checked against `git log origin/main` at dispatch); and
2. its owned files received **no** commit on `main` since `b3cd078d`
   (`binary.rs`: one), measured with
   `git log --no-merges b3cd078d..origin/main -- <file>`.

The largest remaining clusters — TS2322/2345/2741/2353 relation reports,
TS2339 property misses and TS2769 overloads — stay with `main`'s lanes.

## Round-4 rules (in addition to the protocol)

- **Claims.** The integrator records each lane's issues as `in_progress`,
  assignee `claude-cloud-r4-<lane>`, in `.beads/issues.jsonl` on the
  integration branch. `bd` cannot be installed in cloud containers, so the
  claim is not in `main`'s Dolt database until the branch is merged; the
  lane list here is the authoritative record meanwhile.
- **Before starting each cluster and before each push**: `git fetch origin
  main` and run `git log origin/main --oneline -- <owned files>` and
  `git log origin/main --format=%s | grep <issue id>`. If `main` has started
  porting the same native function or the same issue, stop that cluster,
  record it in your notes file, and move to the next one. Duplicate work is
  rejected at integration.
- **Merge, don't rebase.** Merge `origin/claude/beautiful-shannon-ar5gh0`
  into your branch when the integrator announces a new head; resolve by
  measurement, never by guessing.
- **Performance is a hard gate.** A median child-CPU ratio above 1.03 at 41
  samples on either bench project is a rejection, not a note. A change on a
  hot path (relation, member resolution, instantiation, narrowing) also
  reports a callgrind instruction count on
  `scripts/generate_perf_project.py --modules 100` before and after.
- **Cross-lane hunks** go to `docs/parity/notes/<lane>-*.diff`, measured,
  never committed as code outside owned files.

## The lanes

Each lane: issues, root-cause hypothesis (to be confirmed by direct native
reproduction before code), native source to mirror (`vendor/typescript-go`
@ `5b1047d`, `internal/checker/` unless noted), owned files, files it must not
touch, and acceptance. **Acceptance is the same for every lane:** the target
cases convert; both §5 loss checks are empty against the box's frozen baseline;
the full coverage run completes; `cargo test --workspace --release` shows no
new failure; clippy is clean on touched code; the perf gate above passes; each
commit names its Beads issue. Everything else (hub files `check.rs`,
`expressions.rs`, `checker.rs`, `lib.rs`; `relater.rs`, `calls.rs`,
`inference.rs`, `members.rs`, `symbols.rs`, `declared.rs`, `signatures.rs`,
`contextual.rs`, `flow.rs`, `assignreport.rs`, `nonexistent_property.rs`,
`printing.rs`, `node_reuse.rs`) is **not owned by any round-4 box** except where
a lane names a specific function.

### r4-relcache — persistent relation results (`tsr-2zk.902`), perf owner

- **Hypothesis.** `Checker::relate_ternary` (`relater.rs`) builds a fresh
  `Relater` and an empty `results` map per call, so every repeated
  `(source, target, relation)` pair repeats its whole structural walk. Native
  keeps `Relation.results` for the program (`relater.go` `recursiveTypeRelatedTo`,
  `checker.go` `getRelationKey`), so a repeat is a lookup. Measured symptom:
  TypeScript's own `src/jsTyping` and `src/typingsInstallerCore`
  (`vendor/typescript-go/_submodules/TypeScript/src`) never finish; stack
  samples sit in narrowing → `relate_ternary` → a 75-frame property/signature
  walk re-resolving inherited members.
- **Mirror.** `recursiveTypeRelatedTo`, `resetMaybeStack`, `getRelationKey`,
  the per-relation `results` maps and their `RelationComparisonResult` flags
  (`Succeeded`, `Failed`, `Reported`, `ComplexityOverflow`, `StackDepthOverflow`).
- **Obligations first.** `docs/architecture/checker-relation-publication.md`
  lists what must be settled before reuse (key equivalence incl. intersection
  state and generic references; lifetime; metadata forcing; what may be
  published — never `Unknown`, never a `Maybe` assumption, never a result
  computed while an alias/mapper/print frame or a resolution is open). Record
  each decision there per the checker port convention before the cache ships.
- **Owns.** In `relater.rs` only: `Relater` construction, its results store,
  `relate_ternary`, `compare_signature_ternary`, `recursive_type_related_to`'s
  publication path; a new `relation_cache.rs`; the new `Checker` field(s);
  `docs/architecture/checker-relation-publication.md`.
- **Must not touch.** Any relation *arm* in `relater.rs` (main's relate lane
  edits them), `assignreport.rs`, flow and narrowing code.
- **Extra acceptance.** `jsTyping` and `typingsInstallerCore` finish, with
  diagnostics recorded and compared with native tsgo's; wall and CPU on all
  three bench projects not slower; the corpus dumps byte-identical or
  improved.

### r4-unions — union reduction (`tsr-2zk.13.2`, `.16.86`, `.16.89`)

- **Hypothesis.** `UnionReductionSubtype` (`removeSubtypes`) is missing or
  declines on undecidable pairs, and `compareTypes`' arms differ, so `??`,
  `||`, `?:` and union-signature returns keep members native removes.
- **Mirror.** `getUnionType`/`getUnionTypeWorker` with `UnionReductionSubtype`,
  `removeSubtypes`, `isTypeRelatedTo(strictSubtypeRelation)` use there,
  `compareTypes`, `getReducedType` where it applies to unions.
- **Owns.** `unions.rs`, `intersections.rs`.

### r4-arrays — array literals, tuples, iteration (`.16.80`, `.16.93`, `.16.87`, `tsr-2zk.10`)

- **Hypothesis.** `checkArrayLiteral`'s tuple-context decision
  (`inTupleContext`, `isTupleLikeType`) and error-element handling diverge;
  TS2488/2548/2549 and spread/tuple diagnostics of `tsr-2zk.10` are partly
  unported.
- **Mirror.** `checkArrayLiteral`, `createArrayLiteralType`,
  `isTupleLikeType`, `getIteratedTypeOrElementType`,
  `checkIteratedTypeOrElementType`, `createNormalizedTupleType` consumers.
- **Owns.** `array_literals.rs`, `tuples.rs`, `iteration.rs`,
  `spread_overrides.rs`. `destructure.rs` is **not** owned (main touched it).

### r4-operators — operators and nullish operands (`tsr-2zk.12`, `.16.85`, `.16.123`, `.16.25`)

- **Hypothesis.** The `+`/`+=` arm, TS18046/18048/18050 nullable operands,
  `!x` type facts and the `||`/`??` generic gate are partially ported.
- **Mirror.** `checkBinaryLikeExpressionWorker` (all operator arms),
  `checkNonNullType`/`reportObjectPossiblyNullOrUndefinedError`,
  `checkPrefixUnaryExpression` (`getTypeFacts` truthiness for `!`),
  `isTypeAssignableToKind`.
- **Owns.** `operator_operands.rs`, `binary.rs`, `nullable_operand.rs`,
  `nullish.rs`, `comparison_overlap.rs`, `delete_operand.rs`.

### r4-jsx — JSX checking (`tsr-2zk.8`)

- **Hypothesis.** TS2604/TS2786 element-type checks and attribute elaboration
  (`elaborateJsxComponents`) are missing; generic/overloaded components go
  through call resolution that is the calls lane's.
- **Mirror.** `checkJsxOpeningLikeElementOrOpeningFragment`,
  `checkJsxReturnAssignableToAppropriateBound`, `getJsxElementTypeTypeAt`,
  `elaborateJsxComponents` (the `assignreport.rs` arm is delivered as a
  measured patch).
- **Owns.** `jsx_component.rs`, `jsx_factory.rs`, `jsx_attributes.rs`; in
  `jsx_intrinsic.rs` only functions added by this lane.

### r4-jsdoc — JSDoc type hosting (`.16.98`, `.16.105`, `.16.106`, `.16.107`, `.16.38`)

- **Hypothesis.** `@type` hosting, `@template` gathering for class hosts,
  reparsed-node scope and JS default-export JSDoc types diverge from the
  reparser model.
- **Mirror.** `getJSDocType`-family use in `getTypeOfNode`/
  `getTypeForVariableLikeDeclaration`, `getEffectiveTypeParameterDeclarations`,
  `parser/reparser.go` hosting.
- **Owns.** `jsdoc_params.rs`, `jsdoc_annotations.rs`, `jsdoc_modifiers.rs`,
  `jsdoc_full_signature.rs`, `js_case_data.rs`.

### r4-index — index signatures and index access reports (TS2411, TS7053, TS7017, `.16.119`, `.16.51`)

- **Mirror.** `checkIndexConstraints`, `checkIndexConstraintForProperty`,
  `getIndexInfosOfType` for late-bound members, the TS7053/TS7017 arms of
  `getIndexedAccessTypeOrUndefined`'s error reporting.
- **Owns.** `index_signatures.rs`, `index_access_reports.rs`,
  `index_constraint.rs`.

### r4-templates — template literal types, string mappings, enums (`.16.118`, `tsr-6.11`, `tsr-6.18`)

- **Mirror.** `getTemplateLiteralType`, `isTypeMatchedByTemplateLiteralType`,
  `getStringMappingType`, `evaluateEntityNameExpression`/
  `computeEnumMemberValues`, constant evaluation for template expressions.
- **Owns.** `templates.rs`, `template_match.rs`, `string_mapping.rs`,
  `enum_initializer.rs`, `enum_member_name.rs`, `literals.rs`.

### r4-unused-grammar — unused-declaration and checker grammar diagnostics

- **Scope.** TS6133/6138/6192/6196/6198/6199/6205 (`checkUnusedIdentifiers`),
  checker-side grammar and strict-mode codes not owned by the parser
  (TS1238, TS1254, TS1320, TS1355, TS1100-1102, TS1210-1215).
- **Mirror.** `checkUnusedIdentifiers` and its helpers, the
  `checkGrammar*` functions in `checker.go`/`grammarchecks.go`.
- **Owns.** `unused.rs`, `grammar.rs`, `strict_mode.rs`. Parser crates are not
  owned (main's parser lane).

### r4-declemit — declaration-emit accessibility diagnostics (TS4xxx, TS2883)

- **Hypothesis.** `GetDeclarationDiagnostics` only produces the
  `isolatedDeclarations` family; the checker-backed accessibility errors
  (TS4023/4025/4094/2883/2742 …) have no producer
  (`crates/tsr-conformance/src/diagnostics_suite.rs`, the
  `GetDeclarationDiagnostics` block).
- **Mirror.** `transformers/declarations` diagnostics
  (`getSymbolAccessibilityDiagnostic`, `createGetSymbolAccessibilityDiagnosticForNode`),
  checker `isSymbolAccessible`, `isEntityNameVisible`.
- **Owns.** `crates/tsr-dts/`, `symbol_access.rs`, and the
  `GetDeclarationDiagnostics` block of `diagnostics_suite.rs`.

### r4-helpers — external emit helpers and type-only names in value position (`tsr-2zk.21`, `.41`, `.6.3`)

Dispatched when `r4-declemit` finished (both chosen the same way as above;
`emit_helpers.rs` and `meaning_mismatch.rs` had no commit on `main` since
`b3cd078d`).

- **Hypothesis.** `checkExternalEmitHelpers` is called only from import/export
  declarations; the construct-level call sites (async, generators, `using`,
  object rest, decorators, for-await, `yield*`, private fields, `__setFunctionName`)
  are missing, so TS2354/TS2343/TS2807 are missing. r3-names measured a patch
  for the first set: `docs/parity/notes/names-emit-helpers.diff` on
  `claude/beautiful-shannon-ar5gh0-r3-names` (+8 cases, 0 losses at its base).
  TS2693/TS2749 type-used-as-value arms of the value-position cascade are
  partly unported.
- **Mirror.** `checkExternalEmitHelpers`, `resolveHelpersModule` and every
  call site in `checker.go`; `checkIdentifier`/`resolveEntityName`'s
  `isTypeOnly`/`checkAndReportErrorForUsingTypeAsValue` arms.
- **Owns.** `emit_helpers.rs`, `meaning_mismatch.rs`; new
  `check_construct_emit_helpers`-style functions it adds to `check.rs`, called
  from one line at the top of the walk.

### r4-classsyntax — super/this expressions, computed names, type-argument arity (`tsr-2zk.912`)

Dispatched when `r4-operators` finished. Owns `super_expression.rs`,
`this_expression.rs`, `computed_name.rs`, `type_argument_arity.rs` (none
touched by `main` since `b3cd078d`). Mirror `checkSuperExpression`,
`checkThisExpression`, `checkComputedPropertyName`, `checkTypeReferenceNode`'s
argument-count/constraint arms. Not TS2684/TS2558 (calls), not private names.

- `r4-operators` (1e6d66b, 60d399b): `!` type facts (+24 type lines, 5 cases)
  and the `+`/`+=` diagnostics arm (+10 cases). Its `+` type-cascade patch
  (+24 lines, +8 cases) waits on `tsr-2zk.913` (TS2313 circular constraints).

### r4-perf — checker work native caches in links (`tsr-2zk.910`), perf owner

Dispatched when `r4-jsdoc` finished. Main's own measurement (perf.md §10)
puts domain-model-large at 1.44x tsgo wall; r3-perf's attribution
(docs/parity/notes/perf-r3.md) found the remaining gap is work native does
once and caches in links. C1, C2 and C5 are integrated.

- **Candidates.** C7: memoise `signature_candidates_of_interface_symbol`
  (`signatures.rs`, 14.3% inclusive Ir) per `(symbol, kind)` for decided
  top-level answers, as native's resolved signature lists on the symbol's
  structured type. C8: a `couldContainTypeVariables` bit cached per `TypeId`
  in front of `mentions_type_parameter` (native `ObjectFlagsCouldContainTypeVariablesComputed`).
  C9: cache `heritage_entity_symbol` / `instantiated_heritage_base` per heritage
  entry, as native `resolveBaseTypesOfClass`/`getBaseTypes` links do.
- **Owns.** Exactly those functions (`signatures.rs`
  `signature_candidates_of_interface_symbol`, `inference.rs`
  `mentions_type_parameter*`, `members.rs` `heritage_entity_symbol`,
  `declared.rs` `instantiated_heritage_base`), new cache fields, a new
  `perf_links.rs`, and `docs/parity/notes/r4-perf.md`.
- **Must not touch.** Flow effects / `get_effects_signature` (main's
  `tsr-1yb.11.3`), relation code (`r4-relcache`), parse/bind.
- **Acceptance.** As every lane, plus: callgrind Ir on `--modules 100` before
  and after each commit, wall and CPU vs native tsgo on all three bench
  projects (`/tmp`-built tsgo per scripts/offline-cargo/build-tsgo.sh), and the
  convention record for each cache.

### Outcome notes

- `r4-declemit` (97c3a56): TS4025/TS4081 producer, 2 cases; the other 16
  declaration-emit cases need SymbolTracker hooks in the serializer (hub
  files), recorded under `tsr-2zk.904`.
- `r4-jsdoc` (7dd2df2): parameter-own `@type`, +7 type lines. Its three
  measured patches (+44 lines, +5 diagnostics cases) cost 4 losses and wait on
  `tsr-2zk.911`.
