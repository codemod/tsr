# r6-lazytext: printed text decided when it is printed

Lane r6-lazytext (epic `tsr-2zk`), round 6. Takes over the printer lane from
r6-printer. Vendor pinned at `5b1047d`.

The port bakes a type's text when the type is minted (`TypeData::Named`,
`types.rs`; ADR-0050's forcing constraint). Native prints only when it emits:
the node builder runs `typeToTypeNode` at print, and every reuse decision it
makes (`serializeTypeForDeclaration`, `tryReuseExistingTypeNode`) runs then.
Two measured, lossless parity diffs were held only because baking their text
moved print work onto the checking path. This lane moves those decisions to
print time.

## 0. Base and method

- **Frozen base:** `8c52b48` (the tip of `claude/beautiful-shannon-ar5gh0`
  when the box started; batch BF, r6-printer, had not landed). Base dumps:
  types 549,903 RIGHT / 5,557 WRONG / 843 GAP; diagnostics 5,552 RIGHT,
  5,600 EMPTY_RIGHT, 1,041 WRONG, 45 EMPTY_WRONG.
- **Reference:** the two held diffs applied to the base as written
  (`r6-nodereuse-property-slot-spreads.diff`,
  `r5-mapped6-conditional-typed-print.diff`): types +24 RIGHT, diagnostics
  unchanged; but 4 losses (`controlFlowGenericTypes` 0:298, 0:300, 0:301,
  0:303), all from the conditional diff, which waits on r6-declared's
  `f9339d0` (not on the base; r6-mapped.md §7).
- **Ir:** callgrind on the `profiling` build,
  `tsr -p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false --noEmit`.
  Base: domain-model 1,091,331,952; generic-imports 343,686,809.
- Both dumps unfiltered, compared on `cut -f1,2`; `slowcases` on both.
- Setup: PyPI is blocked, so the offline bootstrap ran with a stdlib-only
  stand-in for `assemble.py`'s three `tomlkit` calls, kept outside the
  repository (`r5-operators3.md` §4).

## 1. `tsr-2zk.1120`: a spread member's reuse is decided at print

### What native does

`addPropertyToElementList` (`nodebuilderimpl.go:2486`) serializes each
property of an object type it prints through `serializeTypeForDeclaration`
(`:2181`), whose reuse arm asks the property's declaration for its pseudo
type and reuses the written node when it is equivalent to the property's
type. That runs inside `typeToTypeNode`, so only for a type that is printed,
and once per print.

### What the held diff did, and what it cost

`r6-nodereuse-property-slot-spreads.diff` made `spreads.rs` call
`reused_property_type_text(origin, displayed, None)` at each of the three
places a spread mints a property's printed slot (the merged optional
property, a copied source property, an intersection group). The object's own
print reads its slots at once, so the reuse ran for every annotated spread
member whether or not anything ever printed it: 1,248 visitor runs on
domain-model and about one `resolve_name` per entity name, +0.12% Ir
(`r6-nodereuse.md` §2).

### The port

- `PrintedSlot` (`objects.rs`) gains a reuse plan:
  `PrintedSlot::of_declaration(text, displayed)` holds the displayed type's
  own print (what the slot baked before) and the displayed type. Nothing is
  decided at the mint.
- The site renderer's deferred-slot arm (`printing.rs`,
  `deferred_member_text_at`, formerly `deferred_accessor_text_at`) re-prints
  a planned slot when it prints the object: it asks
  `reused_property_type_text(origin, displayed, None)` and writes the reused
  annotation, else keeps the baked print. This is the arm that already
  re-prints an on-demand accessor slot, so both kinds of print-time slot go
  through one member plan, one visited guard and one resolving-variable
  guard. A plan with declaration slots only declines on re-entry (it keeps
  its baked print, which is what the image printed before), where an
  accessor slot still elides to `any`.
- The plan travels with the property: widening and regularization clone an
  unchanged `AnonymousProperty` and keep its slot, and every producer that
  changes a property's type writes a new slot, which drops the plan. So the
  widened image of `{ ...a }` (the type a variable declaration prints) asks
  the same reuse at print as the fresh one.
- The spreads.rs half is r6-errorsplit's file, so it ships as
  `r6-lazytext-spread-members.diff` (with its test,
  `tests/print_time_spread_members.rs`): the three mint sites write
  `PrintedSlot::of_declaration` instead of asking the reuse.

**The reuse is asked site-free (`None`), as the mint asked it.** Asked with
the print site, the visitor declines a name the site cannot reach:
`declarationEmitComputedPropertyNameSymbol1/2` print `{ x?: { [Foo.sym]: 0; }; }`
from `index.ts`, which imports `Type` but not `Foo`. Native's typeToString
still writes the computed name as the declaration spelled it (the
accessibility tracker reports, it does not refuse). Measured: with the site,
4 of the 19 lines stayed WRONG; site-free, all 19 convert. The structural
arm, which needs a site, therefore stays unasked for spread members, as it
was in the diff.

### Measured (lane code + `r6-lazytext-spread-members.diff`, against the base)

| | types | diagnostics | slowcases |
|---|---|---|---|
| this item | **+19 RIGHT, 0 lost** (549,922) | unchanged, 0 lost | clean (both dumps) |

The +19 are exactly the held diff's: `spreadObjectPermutations` (both
configurations) 4, `spreadObjectNoCircular1` 2, `spreadUnionPropOverride` 2,
`thislessFunctionsNotContextSensitive3` 2, `unionExcessPropsWithPartialMember`
2, `declarationEmitComputedPropertyName{Enum2,Symbol1,Symbol2}` 6,
`intersectionIncludingPropFromGlobalAugmentation` 1.

Ir (profiling build, `--noEmit`):

| | domain-model | generic-imports |
|---|---|---|
| base `8c52b48` | 1,091,331,952 | 343,686,809 |
| this item | 1,091,320,289 (−0.001%) | 343,688,076 (+0.0004%) |
| held diff as written (r6-nodereuse §2, its own base) | +0.12% | +0.00% |

The checking path does no reuse work at all now; only a print pays for it.
CLI output (`--pretty false`) is byte-identical to the base on domain-model,
domain-model-large and generic-imports.

**Falsifier.** `print_time_spread_members::a_spread_member_reuses_its_declaration_when_printed`:
the baked text of `{ ...a }` is `{ unused?: string | undefined; x: string; }`
and the site print is `{ unused?: string; x: string; }`. A mint that decides
the reuse again changes the first assertion; a renderer that stops asking
changes the second. A type printed only through a baked (site-free) reader,
such as a diagnostic message, keeps the unreused print; that was the base's
print too, and no diagnostic in the corpus moved under the held diff.

**Checker port convention** (`docs/conventions.md`):
- *Native operation:* `addPropertyToElementList` → `serializeTypeForDeclaration`
  (`nodebuilderimpl.go:2486`, `:2181`), asked inside `typeToTypeNode`.
- *Key identity and owner:* no new table. The plan is a field of the
  `AnonymousProperty` held in its image's `anonymous_properties` entry, keyed
  by that image's `TypeId`. Its key is the property's declaration provenance
  (`origin`) and the displayed type (the slot's type after
  `remove_missing_type`, as the mint printed it).
- *Publication states:* written once, complete, when the property is minted;
  never mutated; a producer that changes the property's type writes a new
  slot without a plan. No print result is cached: each print asks again, as
  each native `typeToTypeNode` does.
- *Receiver/alias context:* none. The reuse is asked site-free; no mapper,
  alias or receiver is captured.
- *Expensive work boundary:* the visitor (`reused_property_type_text`, its
  `resolve_name` walks) runs only in the site renderer, per print of the
  member.

## 2. `tsr-2zk.1135`: a deferred conditional's branches are printed at print

### What native does

A conditional whose check type is generic is deferred (`getConditionalType`,
`checker.go`); its `root` keeps the node and its `mapper` the instantiation
context. The node builder prints it from its typed parts
(`conditionalTypeToTypeNode`, `nodebuilderimpl.go:2916`): the check and
extends types, then `getTrueTypeFromConditionalType` and
`getFalseTypeFromConditionalType`, which instantiate the written branches
under the mapper only when asked, and the printer emits the check type at
`TypePrecedenceUnion` and the extends type at `TypePrecedenceFunction`
(`printer.go:2058`, `:2275`).

### What the held diff did, and what it cost

`r5-mapped6-conditional-typed-print.diff` replaced the deferred
conditional's written text, at its mint in `declared.rs`, with
`conditional_type_text`: `get_type_from_type_node` of all four parts plus a
print of each, for every deferred conditional minted, printed or not.
domain-model Ir +0.153% (r6-mapped.md §5, §7). On this lane's base it also
lost `controlFlowGenericTypes` 0:298, 0:300, 0:301 and 0:303, which r6-declared's
`f9339d0` (getObjectTypeInstantiation's referenced-parameter keying) fixes.

### The port

- The deferred conditional keeps its written text at the mint, as at the
  base. Its mint already captures the root and the mapper: an inline root's
  `conditional_inference_nodes` entry (node and flattened alias bindings) or,
  inside a mapped template, its `mapped_conditionals` entry. Nothing new is
  recorded.
- `printing.rs`, `deferred_conditional_text`: at print, the captured bindings
  are installed as the only alias-evaluation frame (the print site's own
  frames set aside), `mapped_template_depth` is set as the mint had it, and
  `conditional_type_text` (moved here from the diff) reads and prints the
  four parts. A part that does not evaluate, re-entry into the same type
  (`rendering_composites`) or the instantiation-depth limit keeps the
  written text.
- The parentheses follow the part's node kind, recovered from the type as
  the union printer does (`crate::unions::union_constituent_needs_parentheses`):
  a conditional or function check type and a conditional extends type are
  parenthesised. The diff read the precedence off the printed text
  (`node_reuse.rs`'s `text_precedence`, a file with no owner this round);
  the type test needs no new helper there and no line in the corpus
  differs.
- The parts are printed site-free (`type_to_string`), as the mint printed
  them in the diff.
- Two hunks outside this lane ship as `r6-lazytext-conditional-text.diff`:
  - `declared.rs` (r6-declared): `deferred_conditional_text_at`, which reads
    the captured root (`ConditionalInferenceNode`'s fields are private to
    that file) and calls the renderer;
  - `checker.rs` (main): the site renderer prints that text where it printed
    the baked written text (just before `qualified_name_at`), so an alias
    name or an origin still wins, as it did over the diff's baked text.
  The diff applies on the base and on top of `f9339d0`.

### Measured

Stack: the base, `f9339d0` (cherry-picked, not committed here), the lane
code and the diff. Both dumps unfiltered against the frozen base.

| | types | diagnostics | slowcases |
|---|---|---|---|
| this item | **+4 RIGHT, 0 lost** (549,907) | unchanged, 0 lost | clean (both dumps) |

The +4: `complicatedIndexesOfIntersectionsAreInferencable` 0:5,
`simplifyingConditionalWithInteriorConditionalIsRelated` 0:17 and 0:19,
`wideningWithTopLevelTypeParameter` 0:47. `controlFlowGenericTypes`' four
lines stay RIGHT.

Two lines that are WRONG at the base and in the held diff print a different
wrong text: `circularTypeArgumentsLocalAndOuterNoCrash1` 0:1 and
`recursiveConditionalTypes` 0:8. Native prints the circularity result for
both (`f.NumArray<any>`, `any`). At the mint the held diff's part reads
failed under the circularity in progress and it kept the written text; at
print the resolution has completed, the reads succeed, and the print nests
one level deeper. The port's divergence is the missing circularity result
(TS2615's `any`), not where the text is printed.

**Not converted: `mappedTypeAsClauses` 0:106** (the held diff's fifth line).
The conditional sits in a generic mapped type's `as` clause under `keyof`:
`keyof { [P in keyof T as "a" extends P ? "x" : "y"]: string; }`. Both
enclosing texts are baked at their own mint (`mapped_type_text` in
`mapped.rs`, the `keyof` origin text in `declared.rs`) from the
conditional's baked written text, and the site renderer has no arm that
re-prints a generic mapped form from its parts. It converts when the
mapped form is printed at print time (createMappedTypeNodeFromType,
`nodebuilderimpl.go:1471`); see §3.

Ir (profiling build, `--noEmit`; the reference is base + `f9339d0`, which
moves Ir by itself):

| | domain-model | generic-imports |
|---|---|---|
| base + `f9339d0` | 1,090,579,495 | 343,697,412 |
| + this item | 1,090,655,507 (+0.007%) | 343,693,633 (−0.001%) |
| held diff as written (r6-mapped §7) | +0.153% | |

No check-path code changed: the mint is the base's, and the renderer runs
only when something prints the type. CLI output is byte-identical to the
base on domain-model, domain-model-large and generic-imports.

**Falsifier.** `print_time_conditional_text::a_deferred_conditional_prints_its_typed_branches_when_printed`
(in the diff): `function f<T>(t: T extends undefined ? never : T | string)`
bakes the written `… : T | string` and prints `… : string | T` at the site.

**Checker port convention:**
- *Native operation:* `conditionalTypeToTypeNode` with
  `getTrueTypeFromConditionalType`/`getFalseTypeFromConditionalType`.
- *Identity and owner:* the existing mint captures
  (`conditional_inference_nodes`, `mapped_conditionals`, private Checker
  tables keyed by the minted `TypeId`); the key is the conditional node plus
  the flattened alias bindings and the mapped-template flag, the identity
  `type_literal_key` uses.
- *Publication:* the captures are written once, complete, at the mint. No
  printed text is cached; each print reads the parts again (they are
  memoized by the ordinary type-node caches).
- *Consumer context:* the print site's frames are set aside for the read;
  the parts print site-free.
- *Work boundary:* four `get_type_from_type_node` reads and four prints,
  per print of the type, none at the mint.
