# r6-typesroots — fourteen stale `checker_types` root clusters (`tsr-2zk.16.*`)

Round-6 parity box on epic `tsr-2zk`. Its items are fourteen root clusters
from the first types triage (`docs/parity/types-triage.md`, tree
`cfa02cf4`) that were still marked `in_progress` with no assignee. The lane
owns only the files it creates and this note. Every hook in another lane's
file ships as a measured diff under `docs/parity/notes/r6-typesroots-*.diff`,
and the query it calls lands in the box's own file under
`#[expect(dead_code)]` until the diff removes the allowance (the
r5-jsdoc3 pattern). Native source is `vendor/typescript-go` @ `5b1047d`.

## 0. Baseline and setup

Frozen base: `b18aec06` (batch BB had not landed on
`claude/beautiful-shannon-ar5gh0` at dispatch).

- types 549,853 RIGHT / 5,607 WRONG / 843 GAP of 556,303;
- diagnostics: 12,238 cases;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `--singleThreaded true
  --pretty false`): domain-model 1,090,695,983; generic-imports 343,099,428.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept outside the repo.
The native probe oracle is `scripts/offline-cargo/build-tsgo.sh`'s `tsgo`
(Go 1.26.8, `Version 7.1.0-dev`). Native types were read from its
diagnostics (assign the probed expression to `never`).

## 1. Re-measurement on the frozen base

Each cluster's "finished alone" cases, then its "also blocked" cases, as
non-RIGHT type lines / total on `b18aec06`:

| Cluster | Finished-alone cases | Blocked cases | State |
|---|---|---|---|
| `.16.14` FORIN-VARIABLE-INDEX-TYPE | 6 of 7 RIGHT; keyofAndForIn 2, typeGuardsTypeParameters 4 | isomorphicMappedTypeInference 17, mappedTypes4 8 | §8 |
| `.16.16` APPEND-LOCAL-TYPE-PARAMETERS | 4 of 6 RIGHT; recursiveGenericMethodCall 2, typesWithDuplicateTypeParameters 2 | 3 cases, 11 lines | §8 |
| `.16.21` TYPE-ALIAS-ACCESSIBILITY-GATE | 0 of 5; 29 lines | 3 cases, 31 lines | §8 |
| `.16.28` LATE-BIND-INDEX-SIGNATURE-TYPE-LITERAL | 0 of 5; 6 lines | — | §3 |
| `.16.32` IMPORT-TYPE-NODE-TYPE-MEANING | 0 of 4; 23 lines | importUsedInGenericImportResolves 1 | §8 |
| `.16.34` TYPE-LITERAL-PROPERTY-ANNOTATION-REUSE | **4 of 4 RIGHT** | intersectionTypeInference2 2 | already fixed |
| `.16.35` BINDING-ELEMENT-ALLOW-MISSING-DEFAULT | **4 of 4 RIGHT** | — | already fixed |
| `.16.36` INSTANTIATION-EXPRESSION-TYPE | 1 of 4; 11 lines | — | §8 |
| `.16.40` TYPELIT-INDEX-INFO-KEY-DEDUPE | **4 of 4 RIGHT** | — | already fixed |
| `.16.41` TYPEREF-ARITY-ERRORTYPE-COMPOSES | 0 of 4; 5 lines | — | §2 |
| `.16.43` BINDING-ELEMENT-COMPUTED-NAME-INDEXED-ACCESS | 2 of 3 RIGHT; genericObjectRest 2 | renamingDestructuredPropertyInFunctionType 1 | §8 |
| `.16.46` ORIGIN-SLICE-GATE | 1 of 3 RIGHT; 22 lines | literalTypes2 10 | §7 |
| `.16.47` BINDING-PATTERN-IMPLIED-TYPE | 2 of 3 RIGHT; restParameterWithBindingPattern1 3 | renamingDestructuredPropertyInFunctionType 1 | §8 |
| `.16.54` TYPELIT-DUPLICATE-PROPERTY-SYMBOL-MERGE | **3 of 3 RIGHT** | — | already fixed |

`.16.35`, `.16.40` and `.16.54` are fully RIGHT, and `.16.34`'s own four
cases are too; its one remaining blocked case waits on
INFERENCE-REVERSE-MAPPED-INTERSECTION, another cluster. These four can be
closed.

## 2. `.16.41`: an arity error is `errorType`, and `errorType` composes (diff)

**Forcing constraint.** `getTypeFromClassOrInterfaceReference`
(`checker.go:23169`) and `getTypeFromTypeAliasReference`
(`checker.go:23596`) test the written argument count against
`[getMinTypeArgumentCount, len(typeParameters)]` (`checker.go:21938`) and,
outside it, report TS2314/TS2707 and return `c.errorType`. That is a
computed answer, an ordinary `any`-flagged type, and the types that
contain it are built normally: `IFoo[]` is `any[]`, `C<I>` is `C<any>`.
`get_instantiated_type_reference` (`declared.rs`) answered the port's gap
(`intrinsics.error`, ADR-0038) for every count it could not fill, inside the
window or outside it. Its per-argument guard (`if resolved == error {
return error }`) then declined the enclosing reference, so
`>c2 : C<any>` recorded `any` (the top-level gap is spelled `any` by the
variable's line, and the composed line is lost).

**Port.** `crate::reference_arity::reference_arity_answers_error_type` is
the window test: one past the last parameter without a default is the
minimum, the parameter count is the maximum. The JS leg of the class arm
never answers `errorType` (`if !isJs { return c.errorType }`), so a class
or interface referenced from a `.js` file stays on the existing fill road.
The alias arm has no JS exception. The hook
(`r6-typesroots-arity-error-type.diff`, `declared.rs`) answers
`intrinsics.native_error` (ADR-0048) before the fill decision. A count
inside the window that this port cannot fill keeps the gap, because there
upstream computes a real reference.

**Alternative rejected.** Dropping the per-argument guard instead would
make every gap argument compose into a printed reference (`C<error>`). That
is the wrong line the guard exists to prevent. The fault is the producer
answering the gap for a decided outcome, not the guard.

**Measured** (diff applied on `b18aec06`): types +31 WRONG→RIGHT, zero
losses, diagnostics unchanged. The four cluster cases go fully RIGHT
(genericArrayWithoutTypeAnnotation, genericInterfacesWithoutTypeArguments,
genericTypeReferencesRequireTypeArgs, missingTypeArguments2), plus
returnTypeTypeArguments 12, missingTypeArguments1 6,
genericsWithoutTypeParameters1 4, and the four
genericTypeReferenceWithoutTypeArgument cases 1 each. slowcases is clean.
Ir: domain-model ×1.00051, generic-imports ×1.00000.

**Falsifier.** A reference outside the window whose native line is not
`any`-composed (for example a JS class reference with too many arguments
under `noImplicitAny`) would show as a new R→W here. The unit tests in
`crates/tsr-checker/tests/reference_arity_error_type.rs` (shipped in the
diff) pin both directions, and the JS leg.

**Ships as** `r6-typesroots-arity-error-type.diff`: the `declared.rs` hook,
the removal of the query's `#[expect(dead_code)]`, the new unit tests, and
six existing assertions that recorded the gap where native answers
`errorType` (`tests/types.rs` ×5, `tests/yield_expression.rs` ×1; each
asserted `"error"` with a comment saying upstream answers `errorType`, and
`errorType` prints `any`, ADR-0048). The yield assertion's comment called
`G<number>` for `G<T, R, N>` "a slot this port cannot fill"; it is outside
the window, so native's annotation is `errorType` and the yield is `any`.

## Diffs, in apply order

Every diff applies to `b18aec06` plus this branch's commits and the diffs
above it.

1. `r6-typesroots-arity-error-type.diff` (§2): `declared.rs`,
   `reference_arity.rs`, tests. +31 types, zero losses.
