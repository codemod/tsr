# r6-printer4: print-time mapped forms, routed printer lines (`tsr-2zk.1145`)

Lane r6-printer4 (epic `tsr-2zk`), round 6. Takes the printer lane over from
r6-lazytext (`r6-lazytext.md`): `printing.rs`, `signatures.rs`, `objects.rs`,
`union_signatures.rs`, `literals.rs`, `types.rs` and `crates/tsr-scanner`.
Vendor pinned at `5b1047d`.

## 0. Base and method

- **Frozen base: `8e2f8dd`**, the merge of main `eee504b` (batch BP; batch BO,
  r6-lazytext, landed at `8e88b8e`) into this branch. Its only difference
  from main is this lane's committed code, which the dumps do not reach
  without the diffs. Base dumps: types 550,355 RIGHT / 5,196 WRONG / 752 GAP;
  diagnostics 5,630 RIGHT, 5,606 EMPTY_RIGHT, 963 WRONG, 39 EMPTY_WRONG.
- **First freeze, superseded.** The box started before batch BO landed. Per
  the brief it froze on the tip of the time (`e6eadf4`, batch BL) with
  r6-lazytext's branch merged in (`329d8ed`; types 550,221 RIGHT), and
  measured item 1 on the stack batch BO was to land (that, r6-declared's
  `f9339d0`, both r6-lazytext diffs: 550,244 RIGHT, r6-lazytext's +23, 0
  lost). Item 1 measured the same there as on `8e2f8dd` (+12 / −0). Every
  number below is on `8e2f8dd` unless it says otherwise.
- **Oracle.** `scripts/offline-cargo/build-tsgo.sh` and the pinned compiler
  test runner (`go test -c ./internal/testrunner`), a probe copied into
  `testdata/tests/cases/compiler` (r5-printer3 §1). Every native line quoted
  below was read that way.
- Both dumps unfiltered, compared on `cut -f1,2`; `slowcases` on both.
- **Ir** is callgrind's total for the `profiling` build,
  `tsr -p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false --noEmit`.
  On this container the same binary repeats to within about 100 Ir on
  domain-model (`8e2f8dd`: 1,091,954,038 and 1,091,953,954).
- Setup: PyPI is blocked, so the offline bootstrap ran with a stdlib-only
  stand-in for `assemble.py`'s three `tomlkit` calls, kept outside the
  repository (`r5-operators3.md` §4).

## 1. A generic mapped type, a deferred `keyof` and a deferred indexed access are printed at print time

### Forcing constraint

`mappedTypeAsClauses` 0:106:

```ts
type TN2<T> = keyof { [P in keyof T as 'a' extends P ? 'x' : 'y']: string };
>TN2 : keyof { [P in keyof T as "a" extends P ? "x" : "y"]: string; }   // native
>TN2 : keyof { [P in keyof T as 'a' extends P ? 'x' : 'y']: string; }   // port
```

r6-lazytext made a deferred conditional's typed print a print-time plan
(ADR-0052), but this one is nested in two texts baked at their own mint:
the generic mapped type's (`mapped.rs`, `mapped_type_text`) and the deferred
`keyof`'s (`declared.rs`, `get_index_type`'s deferred arm), each built from
the inner baked text. The site renderer had no arm that prints either from
its parts, so it read the outer baked text and the conditional's plan was
never asked (ADR-0052, consequences).

### What native does

The node builder prints all three from their parts, at the print site:

- `createAnonymousTypeNode`'s mapped arm calls
  `createMappedTypeNodeFromType` (`nodebuilderimpl.go:1458`) for a mapped type
  that is still `isGenericMappedType`. It prints the constraint first (`keyof`
  of the modifiers type for a `keyof`-constrained declaration, `:1480`), then
  enters a new scope with the iteration type parameter (`enterNewScope`,
  `:1511`), names it (`typeParameterToDeclarationWithConstraint` →
  `typeParameterToName`), and prints the name type and
  `removeMissingType(template, isOptional)` (`:1517`). Two wrappers apply
  under `GenerateNamesForShadowedTypeParams`, which the `.types` writer
  sets (`testutil/tsbaseline/type_symbol_baseline.go:394`): a homomorphic
  declaration instantiated over a non-type-variable prints
  `M extends infer T_1 ? { [K in keyof T_1]: … } : never` (`:1483`,
  `:1533`), and a non-`keyof` declaration whose modifiers type is known but
  whose constraint is no longer a `keyof`-constrained type parameter prints
  `C extends infer T_1 extends keyof M ? { [K in T_1]: … } : never`
  (`:1475`, `:1497`, `:1557`). Neither is ported here (below).
- `typeToTypeNode`'s `TypeFlagsIndex` arm prints `keyof` and the operand's
  own node (emitted at `TypePrecedenceTypeOperator`, `printer.go:2274`).
- Its `TypeFlagsIndexedAccess` arm prints the object and index types' own
  nodes.

Each part is a full `typeToTypeNode` at the site: a deferred conditional in
the `as` clause prints its typed parts, an interface is qualified where the
site needs it (`N.X`), and a type parameter the site shadows is renamed in
every part (probe, native):

```ts
function h<P>(p: P) {
    type Q<T> = keyof { [P in keyof T as P]: T[P] };
    let q!: Q<P>; return q;
}
>q : keyof { [P_1 in keyof P as P_1]: P[P_1]; }
```

### The port

Three print-time plans in `printing.rs`, under ADR-0052's contract (the baked
text stays the site-free print; the site renderer makes the decision when it
prints):

- `generic_mapped_text_at`: a `mapped_types` entry that is still
  `is_generic_mapped_type` prints `{ [readonly][name in constraint as
  nameType][?]: template; }` from its `MappedTypeInfo`, in native's order:
  the constraint (`keyof` of `modifiers_source` when `keyof_constraint`,
  parenthesised as `mapped_type_text` does), then the iteration parameter's
  name through the renderer's allocation (`type_parameter_name_at`), pushed
  on `render_type_parameter_scope` for the name type and the template, then
  the template as `mapped_template_type` (the `removeMissingType` read the
  mint used). Every part goes through `type_to_string_at`.
- `deferred_keyof_text_at`: a `deferred_keyof_types` mint with a recorded
  operand (`deferred_keyof_operands`) prints `keyof` and the operand at the
  site.
- `deferred_indexed_access_text_at`: a `deferred_indexed_access_types` mint
  prints `object[index]` at the site, the object parenthesised by the mint's
  own `wrap_array_element_text`. A mint named by an alias reference
  (`declared.rs`' indexed-alias arm) carries a `type_reference_targets`
  entry; the renderer's reference arm prints it before this plan is asked,
  and the plan declines it too.

`print_time_text_at` asks the three in turn. Its caller is the site
renderer's baked-text arm in `checker.rs` (main's), beside r6-lazytext's
conditional dispatch, so the dispatch ships as
[`r6-printer4-print-time-plans.diff`](r6-printer4-print-time-plans.diff),
which applies on top of `r6-lazytext-conditional-text.diff`. An alias name,
an origin and a reference still win, because the dispatch sits where the
renderer read the baked text, after those arms (native's alias arm,
`nodebuilderimpl.go:3362`, also comes first).

**Why the indexed-access plan is part of this item.** The mapped plan alone
measured +9 / −0 (below), but a probe showed it renaming the iteration
parameter in its declaration and not in a baked `T[P]` template:
`keyof { [P_1 in keyof P as P_1]: P[P]; }`. That is worse than the base's
consistent (and equally wrong) `P`, even though no corpus line has the
shape. The indexed-access plan makes the template take the same allocation,
and the probe then matches native.

**Not ported: the two wrappers.** The corpus lines that need them
(`inlineMappedTypeModifierDeclarationEmit` ×14,
`mappedTypeGenericInstantiationPreservesHomomorphism` ×5, r6-accessible §3
(b)'s `PrivateMapped<T[any]>`) are minted today as an alias reference
(`OmitReal<T, K>`), printed by the renderer's reference arm before any
print-time plan is asked: an inaccessible alias with no structure behind it
(ADR-0045 rule 4, `tsr-2zk.16.2`). A wrapper built now would print on no
line, so it would be unmeasured code. Its inputs are also not all recorded:
the homomorphic wrapper prints the target's template re-instantiated over a
fresh `T_1`, and an alias-frame image of a mapped node (`mapped_type_info`)
keeps no target, only the template already instantiated over the modifiers
type. It is due when `.16.2` gives those mints their mapped structure.

**Rejected: keep the iteration parameter's written name** (what
`mapped_type_text` bakes). It is consistent without the indexed-access plan,
but it is not native's rename, and the indexed-access plan converts three
more lines by itself (`declarationEmitNestedAnonymousMappedType` 0:0, 0:2,
0:3: `Part1[Property]` where native expands `Part1`, an alias the site cannot
name).

### Measured

Against the frozen base `8e2f8dd`, both dumps unfiltered (the first freeze's
stack measured the same two rows):

| | types | diagnostics | slowcases |
|---|---|---|---|
| mapped + `keyof` plans | +9 RIGHT, 0 lost | unchanged, 0 lost | clean (both dumps) |
| + indexed-access plan (this item) | **+12 RIGHT, 0 lost** (550,367) | unchanged, 0 lost | clean (both dumps) |

The +12:

- `mappedTypeAsClauses` 0:106 (the target);
- `keyRemappingKeyofResult` 0:36, 0:37, 0:38, 0:40, 0:43: `Remapped`, a
  function-local generic mapped alias, is printed where it is not
  accessible, so native expands it, and the expansion's `as` clause is a
  deferred conditional;
- `declarationEmitNestedAnonymousMappedType` 0:0–0:3: two function-local
  mapped aliases, `Part2` over `Part1`, expanded at the site;
- `declarationEmitInlinedDistributiveConditional` 0:0, 0:4: the mapped
  constraint `import("./internal").PublicKeys1<keyof Obj>` is now printed by
  the reference arm at the site, which already writes the import type for
  that alias (`r6-accessible.md` §3 (a)).

No other line changed text. Without the dispatch diff the committed code is
unreachable, so the commit alone moves no line.

Ir (profiling build; two runs each):

| | domain-model | generic-imports |
|---|---|---|
| base `8e2f8dd` | 1,091,954,038 / 1,091,953,954 | 343,695,029 / 343,703,366 |
| + the diff | 1,092,053,423 / 1,092,081,729 (+0.010%) | 343,704,845 / 343,701,699 (+0.002%) |

The domain-model delta is the dispatch's closure in
`type_to_string_at_worker` (262 K Ir inclusive; the worker goes from
8.60 M to 8.70 M): the site renderer runs during checking on domain-model,
and the plans now print there what they print at any site. That is native's
work at those prints, not new work at the mint. CLI output (`--pretty false`)
is byte-identical on domain-model, domain-model-large and generic-imports.

### Checker port convention (`docs/conventions.md`)

- *Native operation:* `createMappedTypeNodeFromType`, `typeToTypeNode`'s index
  and indexed-access arms, asked inside `typeToTypeNode`.
- *Key identity and owner:* no new table. The plans read the mint's existing
  records: `mapped_types` (keyed by the minted `TypeId`; the parts
  `getTypeFromMappedTypeNode`/`instantiateMappedType` produced),
  `deferred_keyof_operands`/`deferred_keyof_types` and
  `deferred_indexed_access_types`, each written once when its type is
  minted.
- *Publication states:* absent (the type prints its baked text) or complete.
  No print result is cached; each print asks again, as each native
  `typeToTypeNode` does.
- *Receiver/alias context:* the print site only. The mapped type's own
  mapper is already applied to its recorded parts (an instance's parts are
  the instantiated ones); the print site's alias-evaluation frames are not
  read. The iteration parameter's name is allocated in the print's own
  `render_type_parameter_names` and scoped by `render_type_parameter_scope`,
  truncated when the mapped form is printed.
- *Expensive work boundary:* the renderer only. A re-entrant print of the
  same identity is caught by `rendering_composites` and keeps the baked
  text. The mint's work is unchanged.

**Falsifiers.** `printing::tests::a_generic_mapped_form_and_an_indexed_access_print_their_parts_at_the_site`
(committed; the two plans asked directly: `{ [P in keyof T]: N.X; }` and
`N.X[K]` at a site outside the namespace, where the mint baked `X`), and
`print_time_mapped_form` (in the diff: `TN2` and the `P_1` rename through the
whole renderer).

## 2. Printer lines routed this round (item 2)

Read on the frozen base `8e2f8dd`:

- **`conditionalTypes2` `T_1` renaming** (0:39, 0:47, 0:139): already RIGHT
  on the base. r6-mapped §3 (e) listed them before the shadow-rename work
  that landed since. The case's other non-RIGHT lines (0:172–184, 0:227) are
  r6-mapped §3 (a)'s alias-declared `error` arm (`declared.rs`, r6-declared2).
- **`recursiveGenericMethodCall`** (0:1, 0:4): RIGHT on the base. A local
  `interface Generator<T>` merges with the global
  `Generator<T, TReturn = any, TNext = any>`, and native prints every filled
  argument; r6-declared's `ebac88b` (`reference_print_arity`, landed before
  batch BO) converted both. Nothing is left for the printer lane.
- **`declarationEmitPartialNodeReuseTypeReferences` `c.ts`** (2:0, 2:2,
  2:3): still WRONG, routed to main's `tsr-2zk.39`. Native's reuse of the
  annotation `N.SpecialString` in `c.ts` fails `trackExistingEntityName`
  (`N` is not in scope there), so `serializeTypeName` names the symbol
  through `symbolToTypeNode`, and getSymbolChain reaches `N` only through
  its module: `import("./a").N.SpecialString`. The port's
  `serialize_type_name` (node_reuse.rs) names it with `reference_text_at`,
  whose `symbol_chain` (checker.rs) refuses the module specifier because
  `c.ts` imports `"./a"` under some binding (the `imported_here` gate, "our
  resolver just cannot walk every re-export form yet"). r6-triage's row 6
  (`GET-SYMBOL-CHAIN-NEEDS-QUALIFICATION/EXPORT-SPECIFIER-NOT-IN-SCOPE`,
  23 cases) has the same gate as its port location. It is the symbol-chain
  printer's, not this lane's; §3's resolver `symbol_chain_at` is the
  faithful walk that gate stands in for.

## 3. Well-known-symbol keys: `[Symbol.iterator]` (item 3)

### Forcing constraint

A mapped type over an array's keys resolves the array's well-known-symbol
members (r6-accessible §4, `unique_symbol_keys.rs`). The port printed them
`[iterator]` and `[unscopables]`; native prints `[Symbol.iterator]` and
`[Symbol.unscopables]`. Seven corpus lines carry them, all in
`mappedTypeWithAsClauseAndLateBoundProperty{,2}`.

### What native does

`getPropertyNameNodeForSymbolFromNameType` prints
`[symbolToExpression(nameType.symbol, Value)]`; `lookupSymbolChain` →
`getSymbolChain` (`nodebuilderimpl.go:1087`). `iterator` has no accessible
chain at the site, so getSymbolChain walks `getContainersOfSymbol`
(`symbolaccessibility.go:280`). The parent is the interface
`SymbolConstructor`, which has no value meaning, so
`getWithAlternativeContainers` (`:117`) looks for a variable in scope whose
type is the interface's declared type (`:137`, "`Symbol` acts like a
namespace when looking up `Symbol.toStringTag`") and offers it beside the
interface. Parents are tried in `sortByBestName` order (module specifiers,
else `compareSymbols`); the first with a chain of its own wins. Probes:

```ts
interface Ctor { readonly it: unique symbol }
declare var Sym: Ctor;
declare function f<T>(t: T): { [K in typeof Sym.it]: T };
const m = f(1);          // tsgo: { [Ctor.it]: number; }  (Ctor sorts first)
// with `declare var Sym: Ctor;` written first: { [Sym.it]: number; }
```

### The port (diff)

The resolver (`symbol_accessibility.rs`) already ports
`getContainersOfSymbol` with the variable-match arm. What was missing is
getSymbolChain over it: the printer's own `symbol_chain` (checker.rs)
qualifies through modules and namespaces only. Both files are unowned this
round, so this ships as
[`r6-printer4-symbol-chain-containers.diff`](r6-printer4-symbol-chain-containers.diff):

- `DeclarationEmitResolver::symbol_chain_at`: getSymbolChain as native
  writes it (accessible chain, `needsQualification` of its root, the
  containers sorted by `sortByBestName` with `CountPathComponents`, the
  parent's chain under `getQualifiedLeftMeaning`, the `export =` shortcut,
  `getAliasForSymbolInContainer`, and the `endOfChain` /
  `yieldModuleSymbol` tail). No cache of its own: it reads the resolver's
  `accessibility.chains`, which `accessible_symbol_chain` already owns.
- `unique_symbol_property_name_at` asks it where the printer's
  `symbol_chain` has no prefix, and spells a chain of identifiers with dots
  (`createExpressionFromSymbolChain`'s property-access form). Every earlier
  arm (the bare name, `best_name`, the unreachable module parent, the
  printer's chain) is unchanged, so r6-accessible's measured spellings stand.
- Test: `tests/unique_symbol_container_chain.rs`, the two probes above; both
  fail without the diff (`{ [it]: number; }`).

### Measured

On top of item 1's diff, against it, both dumps unfiltered: types **+0 / −0**
(the seven lines change text and stay WRONG), diagnostics unchanged,
slowcases clean, CLI output identical. Ir: domain-model 1,092,152,756 /
1,092,127,586 against item 1's 1,092,053,423 / 1,092,081,729;
generic-imports 343,722,180 / 343,700,772. Neither new function is called
on either project (callgrind lists no call), so the difference is code
layout.

**What still blocks the seven lines: member order.** Native lists the
mapped object's members in `keyof number[]`'s order (the array's
declaration order, `toString`, `toLocaleString`, `pop`, …); the port lists
them sorted by name (`concat`, `copyWithin`, `entries`, …). That is the key
union's order in `mapped.rs` (r6-declared2), not the printer.

### The identity question (recorded, not built)

A written symbol-keyed member is named by its bracketed expression text
(`[s]`, objects.rs `late_bound_symbol_member_name`, members.rs
`late_bound_members_of`); a mapped member keyed by the same unique symbol
is named `__@s@<id>` (`unique_symbol_keys.rs`). Native has one identity,
the escaped name, so the two would unify in a relation or a property read.
No corpus line exercises the mismatch (r6-accessible §4 measured 0
diagnostics moved), and the written side's owner is members.rs (main):
the member table is keyed there, objects.rs only prints it. Converging on
`__@name@id` means re-keying `late_bound_members_of` and every reader that
looks a computed member up by its text; it is main's, with a falsifier
that a written `[s]` member and a mapped `[K in typeof s]` member relate.

## 4. r6-triage's printer-owned rows (item 4)

r6-triage (`b2fc79b`, on `e6eadf4`) owns these rows to r6-lazytext, this
lane's predecessor. None is ported this round; each is listed with what its
port needs.

| row | cluster | cases | what the port needs |
|---|---|---|---|
| 19 | `SHADOWED-TYPEPARAM-RENAME` | 15 (64 lines) | native's `typeParameterToName` under `GenerateNamesForShadowedTypeParams` with `enterNewScope`'s pre-naming (`nodebuilderscopes.go:223`): a signature's own type parameters are named first and shadow the site, and every other type parameter met in the print is renamed by text (`S_1`). The port renames by a token-wise text substitution (`signatures.rs` `type_parameter_renames` / `apply_renames`) plus `rename_type_parameters_for_site` (inference.rs, main). The faithful port routes every signature type parameter through `allocate_type_parameter_name` (printing.rs) at scope entry and retires the substitution; its inference.rs half is main's. |
| 23 | `SCANNER-NUMERIC-AND-ESCAPE-DIAGNOSTICS` (family) | 15 diag | split first: `scanBinaryOrOctalDigits`, `scanEscapeSequence`, `scanNumber` reports, and the keyword-escape checks, each in `crates/tsr-scanner`. |
| 41 | `REGEXP-SCANNER-VALIDATION` | 10 diag | `scanner/regexp.go` (the regular-expression body and flag validator) is unported; a new scanner module. |
| 92 | `YIELD-NEXT-TYPE-FROM-CONTEXTUAL-TYPE` | 5 | `checkAndAggregateYieldOperandTypes`' next type from the yield's contextual type (`signatures.rs` `return_type_from_body`). |
| 102 | `GENERIC-ARG-NIL-CONTEXTUAL-SIGNATURE` | 4 | `getContextualCallSignature`'s exactly-one-applicable rule (`signatures.rs` `get_type_of_function_expression`). |
| 106, 110 | `FUNCEXPR-NIL-CONTEXTUAL-SIGNATURE-POSITIONS` | 4 + 4 | `getContextualType`'s arrow/return and parameter-initializer arms answering nil (`signatures.rs` `has_no_contextual_type`). |
| 145 | `UNION-SIGNATURE-CALL-CONSTRUCT` | 4 diag | union-type call and construct resolution reports (`union_signatures.rs`). |
| 160 | `INFER-TYPE-PREDICATE-FROM-BODY` | 3 | `getTypePredicateFromBody` (`signatures.rs`). |
| 166 | `CONTEXTUAL-TYPE-FOR-ASSIGNMENT-EXPRESSION-JS` | 3 | `getContextualTypeForAssignmentExpression`'s JS arm. |
| 170 | `SHADOWED-TYPEPARAM-RENAME/FREE-PARAM-BYTEXT` | 3 | row 19's by-text cache for a free type parameter (`rename_type_parameters_for_site`'s refused byText arm, inference.rs). |
| 173 | `BINDING-PATTERN-IMPLIED-TYPE/EXPRESSION-PARAM-ANY-DECLINE` | 3 | `getTypeForVariableLikeDeclaration`'s initializer arm (`signatures.rs` `parameter_of`). |

## 5. Report

**Commits** (on `claude/beautiful-shannon-ar5gh0-r6-printer4`):

| commit | what | alone |
|---|---|---|
| `329d8ed` | merge of r6-lazytext's branch (first freeze) | — |
| `ca07c6c` | item 1: print-time mapped form, deferred `keyof`, deferred indexed access (printing.rs) + dispatch diff | +0 / −0 (unreached) |
| `8e2f8dd` | merge of main `eee504b` (re-freeze, batch BO landed) | — |
| `c14a112` | item 1 re-measured on `8e2f8dd`; the wrappers recorded | docs |
| this commit | items 2–4 and the item-3 diff | docs |

**Diffs, in apply order** (each applies on `8e2f8dd`):

1. [`r6-printer4-print-time-plans.diff`](r6-printer4-print-time-plans.diff)
   (checker.rs, main; new test): **+12 types / −0**, diagnostics unchanged,
   slowcases clean, Ir dm +0.010%, gi +0.002%, CLI identical.
2. [`r6-printer4-symbol-chain-containers.diff`](r6-printer4-symbol-chain-containers.diff)
   (symbol_accessibility.rs, unique_symbol_keys.rs, unowned; new test):
   +0 / −0, diagnostics unchanged, slowcases clean, Ir unaffected (never
   called on either project), CLI identical. Faithful spelling, blocked on
   member order (§3).

Stacked on `8e2f8dd`: types 550,355 → 550,367 RIGHT (**+12**), 0 lost;
diagnostics 5,630 RIGHT / 5,606 EMPTY_RIGHT unchanged, 0 lost.

**Remaining, with causes:** §1's two wrappers (wait on `.16.2`'s alias
structure); §2's `c.ts` (main's `.39`, the `imported_here` gate); §3's
member order (mapped.rs) and the symbol-key identity (members.rs, main);
§4's rows.
