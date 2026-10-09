# r6-printer4: print-time mapped forms, routed printer lines (`tsr-2zk.1145`)

Lane r6-printer4 (epic `tsr-2zk`), round 6. Takes the printer lane over from
r6-lazytext (`r6-lazytext.md`): `printing.rs`, `signatures.rs`, `objects.rs`,
`union_signatures.rs`, `literals.rs`, `types.rs` and `crates/tsr-scanner`.
Vendor pinned at `5b1047d`.

## 0. Base and method

- **Frozen base:** batch BO (r6-lazytext) had not landed on
  `claude/beautiful-shannon-ar5gh0` when the box started (tip `e6eadf4`,
  batch BL). The brief says to use the current tip until then; the lane's
  predecessor's branch (`origin/…-r6-lazytext`, `e2dbd90`) was merged onto
  it, so the lane's own code is under every measurement (`329d8ed`, the
  merge; only `docs/parity/round5.md`, the snapshots and `.beads` conflicted,
  and main's side was kept for each).
  - Base dumps at `329d8ed`: types 550,221 RIGHT / 5,324 WRONG / 758 GAP;
    diagnostics 5,612 RIGHT, 5,606 EMPTY_RIGHT, 981 WRONG, 39 EMPTY_WRONG.
- **Stack base.** Item 1 prints through r6-lazytext's held conditional
  dispatch, so it is measured on the stack batch BO will land: the base, plus
  r6-declared's `f9339d0` (cherry-picked, not committed here), plus
  `r6-lazytext-spread-members.diff` and `r6-lazytext-conditional-text.diff`.
  Stack-base dumps: types 550,244 RIGHT (**+23**, r6-lazytext's number, 0
  lost against the base), diagnostics unchanged, slowcases clean.
- **Oracle.** `scripts/offline-cargo/build-tsgo.sh` and the pinned compiler
  test runner (`go test -c ./internal/testrunner`), a probe copied into
  `testdata/tests/cases/compiler` (r5-printer3 §1). Every native line quoted
  below was read that way.
- Both dumps unfiltered, compared on `cut -f1,2`; `slowcases` on both.
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
  `removeMissingType(template, isOptional)` (`:1517`). The baseline's
  `typeToString` flags carry no `GenerateNamesForShadowedTypeParams`, so the
  homomorphic wrapper (`:1483`, `:1533`) and the modifier-preserving
  wrapper (`:1497`) do not apply.
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

**Rejected: keep the iteration parameter's written name** (what
`mapped_type_text` bakes). It is consistent without the indexed-access plan,
but it is not native's rename, and the indexed-access plan converts three
more lines by itself (`declarationEmitNestedAnonymousMappedType` 0:0, 0:2,
0:3: `Part1[Property]` where native expands `Part1`, an alias the site cannot
name).

### Measured

Against the stack base, both dumps unfiltered:

| | types | diagnostics | slowcases |
|---|---|---|---|
| mapped + `keyof` plans | +9 RIGHT, 0 lost | unchanged, 0 lost | clean (both dumps) |
| + indexed-access plan (this item) | **+12 RIGHT, 0 lost** (550,256) | unchanged, 0 lost | clean (both dumps) |

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

Ir: see §5.

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
