# Lane notes: r5-typeparams2 (`tsr-2zk.966`; `.901`, `.911`, `.913`)

Round-5 cloud lane, continuing r4-typeparams (`r4-typeparams.md` §1). Native
source is `vendor/typescript-go` @ `5b1047d` (`internal/checker/checker.go`
unless noted). Frozen baseline: this box's dumps at `ac56208` (main) — types
543119 RIGHT / 8301 WRONG / 1113 GAP; diagnostics 5070 RIGHT / 1521 WRONG /
5551 EMPTY_RIGHT / 96 EMPTY_WRONG. Ir is `valgrind --tool=callgrind` total
instructions of the release `tsr`, `--singleThreaded --pretty false`, on
`generate_perf_project.py --modules 100` ("p100") and
`benches/projects/domain-model-large` ("dml"); every variant's CLI output is
`cmp`-identical to the base's on both.

Base Ir at `ac56208`: p100 **2,841,811,003**; dml **5,732,440,233**.

## §1 Re-measuring r4-typeparams' merged-parameter diff on `ac56208` — held

`r4-typeparams-merged-parameters.diff` re-applied (one conflict: main's
`identity_mapped_alias_node` now skips parentheses and compares parameter
owners; it only needed the `*` deref) plus two `for … in local` loops that
became `.iter()`.

| Set (unfiltered, vs frozen base) | type lines → RIGHT | type losses | diag cases | diag losses |
|---|---|---|---|---|
| merged list alone | +131 | 6 | +3 | 0 |
| merged list + memo (§2) + alias sweep (§4) | +145 | 6 | +3 | 0 |
| … + identity substitution (§3 diff) | +148 | 3 | +3 | 0 |
| … + other files' alias call sites (§4 diff) | +148 | 3 | +3 | 0 |

The held set is `r5-typeparams2-merged-parameters.diff` (applies on this
branch's head) plus `r5-typeparams2-identity-substitution.diff`; the final row
was re-measured on the final code of both. Tests pass and clippy reports
nothing in `declared.rs`, `perf_links.rs` or `inference.rs` on that set.

**Why it is held.** The three remaining losses (§5) are lines the merged list
unmasks in code outside this lane. Without the `inference.rs` diff the set has
six. Nothing in the owned functions can avoid them without special-casing.

The diagnostics loss r4 recorded (interfaceExtendsObjectIntersection's 11×
TS2507) no longer appears: main's `8460f71` port of
`getDeclaredTypeOfTypeAlias` changed that path. The three diagnostics gains
are r4's (privacyCheckExportAssignmentOnExportedGenericInterface1,
nonPrimitiveInGeneric, nonPrimitiveStrictNull).

## §2 The per-symbol memo (native `localTypeParameters` / alias `typeParameters`) — measured, held with §1

**Forcing measurement.** Without a memo the merged walk costs p100
2,841,811,003 → 2,922,636,447 Ir (**+2.84%**;
`local_type_parameters_of` inclusive 87.1M Ir). Instrumented counts on the
memo build: p100 202,479 queries, 146,949 of them on a symbol with two or
more declarations or parameters, **36** worker executions; dml 402,579 /
292,549 / 36. Every `Array`/`Promise`-like lib symbol is interface + var, and
each query re-walked every declaration and re-resolved each parameter's
merged symbol for the de-duplication.

**Native operation.** `getLocalTypeParametersOfClassOrInterfaceOrTypeAlias`
(`checker.go:23806`) runs once per symbol, inside
`getDeclaredTypeOfClassOrInterface` (stored as the declared type's
`localTypeParameters`/`typeParameters`) and `getDeclaredTypeOfTypeAlias`
(`links.typeParameters`, `:23837`). Consumers read the stored list.

**Identity and owner.** `PerfLinks::local_type_parameters`
(`perf_links.rs`, in its own block after r4-perf's tables):
`SymbolId → LocalTypeParametersLink`, private to one `Checker`, lifetime of
the checker. The key is the symbol whose `declarations` are read (callers
pass merged symbols; an unmerged symbol is its own key and reads its own
declarations, exactly as before). No options, mapper or relation context
enter: the answer is a function of the bound tree only.

**Publication.** The list depends on nothing the checker computes —
declarations, node kinds, JSDoc comments, the binder's parameter symbols — so
there is no provisional state: the first query computes and publishes a
completed answer, and an absent entry means "never asked". `Empty` is a
completed answer (no parameters), distinct from absent.

**Encoding.** The table is a dense `Vec` indexed by `SymbolId::index()`, not
a hash map: the hot reader is `global_type_symbol_with_arity` (306,889 calls
on dml, every one a multi-declaration lib global), and the hash-probe build
measured dml +0.30% / p100 +0.15% Ir against the dense build's ±0.006%.
`PerfLinks` has no arena lifetime, so the value names nodes:
`Declared(declaration)` when one declaration's own list is the whole answer
(re-read through the node map, which is what the pre-memo code did on every
call), `Merged(Rc<[NodeId]>)` for the parameter nodes of a list several
declarations contributed to. On both bench projects every computed entry is
`Declared` (0 merged reads); `Merged` occurs in the corpus's class+interface
merges (`react16.d.ts`, `genericDefaults`).

**Receiver/alias context.** None: the list is the symbol's, not a reference's.
Instantiation and defaults stay with their existing owners.

**Work boundary.** The worker is `compute_local_type_parameters` (walk every
declaration, `declared_type_parameters_of`, de-duplicate by merged parameter
symbol). It runs once per symbol. The common case — one declaration with at
most one parameter — returns that declaration's list without probing the memo,
because the probe costs more than the read. A single declaration with two or
more parameters still goes through the memo: native de-duplicates within one
list too (`class C<T, T>` prints `C<T>`, typesWithDuplicateTypeParameters
0:0/0:1, genericsWithDuplicateTypeParameters1 0:4 — a build that skipped it
lost exactly those three lines).

**Measured** (final held set, merged + memo + §3 diff, on this branch's §4
head): p100 2,841,811,003 → **2,841,994,554** (+0.006%); dml 5,732,440,233 →
**5,732,709,666** (+0.005%); generic-imports 399,676,039 → **399,680,186**
(+0.001%). The Ir of these variants moves by up to ±0.3% with inlining
decisions under `codegen-units = 16`; the hash-probe and two-borrow drafts are
the measured examples.

**Falsifier.** If a symbol's declarations could change after binding (a late
merge the checker performs), a stale `Declared` would answer the old list.
TSR merges in the binder/program before checking; a checker-time merge would
have to invalidate this table.

## §3 The genericDefaults losses — identity substitution (held diff, `inference.rs`)

`interface i07 { a: A; } interface i07<A = number> { b: A; }`: the first
declaration's `A` is the global interface (native
`isTypeParameterSymbolDeclaredInContainer`, already in the binder). With the
merged list `i07` is generic, so reading `(<i07>x).a` instantiates `A`
(global) under `i07`'s parameters `[A]`. `instantiate_type` asks
`mentions_type_parameter`; the global interface's `Named` type has no
structural metadata, so the walk reaches the printed-name fallback, which
finds the identifier `A` and reports a mention. The worker then answers error
(`any`). Native substitutes by identity and never confuses the two.

**Fix (`r5-typeparams2-identity-substitution.diff`, `inference.rs` — main's
file, held).** Before the printed fallback, a non-generic class's or
interface's own declared type (`declared_types[symbol] == id`, `CLASS |
INTERFACE`, no local type parameters) is not a mention: native
`couldContainTypeVariables` (`:22184`) either says false (a thisless
interface is not a reference) or instantiates the reference's only argument,
its own `this`, back to itself — the result is the same type either way. The
printed name can only coincide with a parameter's name.

Measured on top of §1's full set: the three genericDefaults lines
(1052/1053/1057) return to RIGHT, no other verdict moves (148 gains, 3
losses).

## §4 `type_alias_declaration_of` (`core.Find(IsEitherTypeAliasDeclaration)`) — committed (`ab375ec`, gated in the next commit)

`getDeclaredTypeOfTypeAlias` (`:23845`) and its readers locate the alias
declaration with `core.Find(symbol.Declarations,
ast.IsEitherTypeAliasDeclaration)`. TSR's alias readers took
`declarations.first()`, which is a function when a function merges with the
alias (the binder binds function declarations first, as native
`bindEachFunctionsFirst`). New `type_alias_declaration_of` (including the
reparsed JSDoc typedef/callback, `IsEitherTypeAliasDeclaration`'s
`JSTypeAliasDeclaration`); every alias reader in `declared.rs` calls it (18
sites). `is_no_infer_alias`'s first-declaration read is left alone (the
intrinsic contract is not this lane's).

**Measured alone** (first-declaration `local_type_parameters_of`): zero
verdict changes either way — the alias readers only diverge for a symbol whose
local type parameters the first declaration hides, which is §1's job. With §1
it adds 14 type lines (131 → 145).

**Correction (Ir).** The first commit was pushed without an Ir run; measured
afterwards it cost p100 +0.47% / dml +0.45%: `alias_free_generic_alias_body`
and others call it for non-alias symbols, and `find` scanned all of a lib
global's declarations where `first()` read one. The follow-up commit returns
`None` unless the symbol has `SymbolFlags::TYPE_ALIAS` — exact, since every
declaration the search accepts binds that flag (`TypeAliasDeclaration`, and
`declare_jsdoc_symbol(…, TYPE_ALIAS, …)` for typedef and callback tags) —
giving p100 **2,841,931,079** (+0.004%) and dml **5,732,211,326** (−0.004%),
zero verdict changes, perf 1.003 / 0.966 (domain-model / generic-imports, 21
samples).

The remaining call sites in other files (`constraints.rs`, `mapped.rs`,
`members.rs`, `signatures.rs`, `string_mapping.rs`, `templates.rs`) are held in
`r5-typeparams2-alias-call-sites.diff`.

## §5 The complexRecursiveCollections losses — not this lane's

`Immutable.Collection` is functions + namespace + interface. First-declaration
reading answered `[]`, so `Collection<any, U>` failed and the print-only rename
clone (`rename_type_parameters_for_site`, §102/§107) declined; the overloads
printed from their written annotations (`Collection.Indexed<Z>`). With the
merged list the clone succeeds, instantiates `Collection.Indexed<Z>` (a
qualified generic mint) to the fresh `Z_1`, and `instantiate_type_worker`'s
reference arm rebuilds it with `create_type_reference_with_display`, which
prints the bare declared name: `Indexed<Z_1>`. Native's node builder reuses
the written `Collection.Indexed<Z>` node with the renamed parameter
(`tryReuseExistingTypeNode` under the renaming), or qualifies from the
symbol chain (NB-SYMBOL-CHAIN, `tsr-2zk.39`). Both homes are outside this lane
(`inference.rs`, `signatures.rs`, the printer); lines 910/916/924 were RIGHT
only because the clone declined.
