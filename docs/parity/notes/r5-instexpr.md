# Parity lane: r5-instexpr (tsr-2zk.1006, tsr-2zk.1005)

Round-5 box on epic `tsr-2zk`, pinned vendor `5b1047d`. Two items:
instantiation expressions (`tsr-2zk.1006`) and unique-symbol identity
(`tsr-2zk.1005`).

## 1. Baseline

Frozen at `ccb48e7` before any edit: `diagverdictdump` 12,238 rows,
`verdictdump` 552,533 lines (544,166 RIGHT, 7,379 WRONG, 988 GAP).

Setup note: as in `r5-operators3.md` §4, PyPI is blocked here, so the
offline bootstrap's three `tomlkit` calls were served by a stdlib-only
stand-in kept outside the repository (`/tmp/claude-0/shim`), not committed.

## 2. Instantiation expressions (tsr-2zk.1006)

### 2.1 What was missing

`f<number>` reached `check_expression_worker`'s `_ => error` arm and printed
`any`; `typeof f<T>` returned the gap from `get_type_from_type_query_node`
before resolving anything. `conformance/instantiationExpressionErrors` alone
had 66 WRONG type lines from this.

### 2.2 The port

`crates/tsr-checker/src/instantiation_expressions.rs` ports
`checkExpressionWithTypeArguments` (`checker.go:10637`) and
`getInstantiationExpressionType` (`checker.go:10660`) with its closures
`getInstantiatedSignatures` and `getInstantiatedTypePart`, plus the pieces of
`checkTypeArguments` (`checker.go:9222`), `hasCorrectTypeArgumentArity`
(`:9214`), `fillMissingTypeArguments` and `getSignatureInstantiation`
(`:19293`) that the closures call. Hooks: one arm in
`check_expression_worker` (expression form), one line in
`get_type_from_type_query_node` (type-query form, which now runs the
expression type through the same function), and the check walk's
`ExpressionWithTypeArguments`/`TypeQueryNode` visits (reports).

### 2.3 Representation of the minted type (the judgment call)

Upstream mints `newObjectType(Anonymous|InstantiationExpressionType,
t.symbol)` and copies `resolved.members` and `resolved.indexInfos` into it.
This port has no single resolved-members record; an object's structure is
spread over `TypeData` plus side tables. The minted type therefore:

- keeps `t`'s `TypeData` variant and symbol (`Anonymous { symbol }` or
  `Named { members }`), so property *lookup* reaches the same declarations as
  on `t` (`get_property_of_type_ex` reads the owner);
- records the property table as an authoritative `anonymous_properties`
  entry (`true`), read through `t` (`get_type_of_property_of_type`), so a
  generic reference's members arrive substituted even though the new type
  has no `type_reference_targets` entry. An empty table is not recorded: it
  adds nothing the symbol's own lookup lacks, and an entry routes a lone
  signature through the callable-expando re-render, which prints braces;
- records the instantiated call then construct signatures in
  `signature_types`, which `signatures_of_type_kind` reads before anything
  else;
- bakes its text the way `createTypeNodeFromObjectType` prints it: call,
  construct, index rows, properties; a lone signature with nothing else is
  the arrow form. Braces text is added to `alias_named_signature_types` so
  the site re-render (which knows property syntax only) does not turn
  `g<U>(): U` into `g: <U>() => U`.

Property order follows `getNamedMembers` (`checker.go:22049`): for a class or
interface owner, members declared inside it first, then the rest; that is why
`C<string>` prints `{ new (x: string): C<string>; f<U>(x: U): U[];
prototype: C<any>; }` with `prototype` last.

Rejected alternative: re-deriving properties per read from `t`'s owner
without a table. It fails for generic references (no substitution without a
`type_reference_targets` entry), and copying that entry would claim the new
type *is* a reference to the generic, which the relater would compare by
type arguments.

### 2.4 Cache contract (`docs/conventions.md` checker-port boundaries)

- Native operation: `c.instantiationExpressionTypes`.
- Key identity and owner: `(node id, expression TypeId)`, private to the
  Checker (`InstantiationExpressionLinks`).
- Publication states: absent, active (`None`), completed. A part this port
  cannot compute completes the entry as the gap and publishes no report.
- Expensive work: one signature instantiation per key.

### 2.5 Two declines, each mirroring native laziness

Native resolves a signature's return type lazily
(`resolveStructuredTypeMembers` creates signatures without them). This port
completes returns when it reads signatures, so two self-references would
close cycles native never forms:

- `declare function h<T>(): typeof h<T>` (`selfReferentialFunctionType`,
  `circularInstantiationExpression`): reading `h`'s signatures from inside
  its own return re-entered the return and reported TS2577, and before the
  active state existed it overflowed the stack in the full run.
- `ReturnType<typeof createCacheReducer<QR>>` inside a parameter annotation
  of `createCacheReducer` (`instantiationExpressionErrorNoCrash`) reported an
  extra TS2502 when the walk forced the query.

So: a query whose leftmost name's type or signature return is on the
resolution stack answers the gap (`type_query_closes_eager_cycle`), an object
part whose symbol has an active return answers the gap
(`signature_return_is_active`), and re-entry of an active key answers the
gap. The check walk does not force a type query's type; it only publishes
what a computation parked. How we would know this is wrong: a corpus case
where native reports TS2635/TS2344 on a type query that nothing else in this
port resolves (the report would be missing).

### 2.6 Diagnostics

TS2635 is reported at `skipTrivia(typeArguments.Pos())..typeArguments.End()`,
approximated by the first argument's start and last argument's end (the
empty list `f<>` takes the empty width before `>`). Constraint failures go
through `report_relation_failure` with `Type_0_does_not_satisfy_the_constraint_1`
at the argument node, as `checkTypeArguments` does. TS2848 (instantiation
expression on the right of `instanceof`) is checked on the walk's visit.
The computation parks its reports; the walk drains them once (ADR-0040's
reporting road), which keeps a speculative type query from reporting twice.

### 2.7 Measured

Against the frozen baseline, unfiltered: types 87 WRONG→RIGHT and 4
GAP→RIGHT (+91 RIGHT lines; `instantiationExpressionErrors` all 66 lines,
`aliasInstantiationExpressionGenericIntersectionNoCrash1/2`,
`assignmentToInstantiationExpression`, `instanceofOnInstantiationExpression`,
`genericCallWithoutArgs`, `varianceAnnotations`, …), 3 GAP→WRONG (§2.8), no
RIGHT line lost. Diagnostics: `instanceofOnInstantiationExpression` WRONG→RIGHT
(TS2635/TS2848), no RIGHT or EMPTY_RIGHT case changed. Coverage after:
checker_types 8,240/9,538, diagnostics 4,532/5,502. Performance: callgrind Ir
domain-model 1,285,352,788 → 1,285,232,700, generic-imports 344,835,559 →
344,823,035; median CPU ratios at 41 samples swung 0.988–1.034 between
projects and runs, which the flat Ir attributes to noise.

### 2.8 Remaining clusters (not converted here)

- `typeof Array<number>` as a written annotation prints the reused node
  upstream (`createAnonymousTypeNodeEx`'s instantiation-expression arm
  reuses `typeof X<…>` when it still denotes the type). The minted text is
  structural, so `arrayTypeOfTypeOf` 0:6/0:8 move from GAP to WRONG (2
  lines). Needs node-reuse at the mint or the printer (`node_reuse.rs`,
  `symbols.rs`).
- `instantiationExpressionErrors` TS2558 (26,24: `f<number>?.<number>()`)
  and TS2554 (39,2): call resolution on an instantiation-expression callee,
  `calls.rs` (main's).
- `instantiationExpressionErrorNoCrash` TS2344 (15,38): `ReturnType`'s
  constraint against the signature-less minted object, reached through an
  eagerly resolved mapped template (`mapped.rs`, r5-mapped3).
- `selfReferentialFunctionType` 0:2 prints `any` for the `f` inside
  `typeof f<T>` (was the gap): the identifier is now checked, and `f`'s
  symbol type is mid-resolution there. GAP→WRONG, one line.
