# The print seam — why type texts must stop being minted at creation

Status: DESIGN STUDY (checker-1, 2026-08-10). No code. The §136
refusal is the forcing document; this is the reasoning §136's map
demands before anyone touches the seam again.

## 1. The forcing constraint, measured

Every type in this port carries ONE immutable text, minted when the
type is created. Upstream carries NO text at all: every print is
`typeToString(type, enclosingDeclaration)` — the node builder walks
the STRUCTURE at the site, chooses names by what is in scope THERE
(`lookupSymbolChain`, `getSpecifierForModuleSymbol`,
`typeParameterToName`), and omits what the site makes redundant
(trailing default-equal type arguments, `nodebuilder`'s
`typeToTypeNodeHelper`).

The measured cost of the divergence, by head:

| head | lines | § that priced it |
|---|---:|---|
| temporal qualified spellings | ~400 | §41/§81/§95-§96 |
| `import("...").Name` per-file spellings | 128 | the deferred head |
| qualified names (design P's residue: FILE-module containers) | 644 | qualnamep census |
| es6ExportEqualsInterop's alias renames | ~100 | §132's two trap firings |
| §136's default-fill (blocked SOLELY on prints) | +208 refused | §136's five iterations |
| underscoreTest1 `T_1` shadow renames | 149 | §20.1 double refusal |

Five §-numbered attempts have now hit this wall from five different
directions (§81 blunt qualification 114:6,769; §102 written-reuse
carriage 88:763; §132 iterations 2–4; §136 iterations 2–5; the
tsr-4jk one-alias rename cracking under §132). **The wall is one
wall.**

## 2. Where texts are born today (the census §136 paid for)

Thirty-five `new_named` call sites across nine files, in three
families:

1. **The reference mint** — `create_type_reference` →
   `type_reference_text` (declared.rs): one text per
   `(symbol, arguments)` pair, interned forever. §136's trim landed
   here and changed nothing downstream because of family 3.
2. **The qualified/alias mints** — the §41/§46 `qualified_reference_types`
   family: `Name<args>` texts with member-carrying symbols.
3. **The baked-signature rebuilds** — `instantiate_signature_type`
   and the written-reuse seams (`written_text`, §36.1/§77): signature
   TEXTS carried as strings, substituted textually at instantiation.
   §136's iteration 4 proved family-3 texts never pass through
   family-1's printer.

Plus the union render, tuple prints, and the §72/§89 keep-text set —
each its own small text authority.

## 3. The design space

### (a) Full per-site rendering (upstream's shape)
Delete stored texts; `type_to_string_at(id, site)` renders from
structure every time. Faithful, and the ONLY design that answers
all six heads. Cost: every one of the ~35 mint sites and 3 families
must first carry enough STRUCTURE to render from (family 3 today
carries only strings — the §40 print-only rest-tuples, the baked
outer texts §10.13 routes). This is a subsystem rebuild with a long
red valley: nothing prints until most of it prints.

### (b) Deferred first-print with site context
Keep one text per type but mint it LAZILY at first print, with the
site of first print supplying context. Cheaper, and wrong: the
first site wins and every other site inherits its spelling — the
temporal/underscoreTest1 wants are DIFFERENT spellings for the SAME
type at different sites. Rejected on the §20.1 evidence.

### (c) Site-override table (the incumbent, grown)
Keep creation texts; accumulate per-site overrides (the §55.1
enum-access spelling, the §72 alias renames, the §89 keep-text set
are all this). Each new head adds a table and a trap; §132 measured
the tables interfering (a SECOND alias breaks the one-alias
rename). This is the wall §-attempts keep hitting. Rejected as a
destination; it is what we have.

### The recommendation: (a), reached incrementally by STRUCTURE-FIRST

Not a big-bang rewrite. The falsifiable path:

1. **Make family 3 structural.** Baked signatures become real
   signature types (parameters as TypeIds, which `Signature` already
   holds — the STRING carriage is the §36-era reuse seam, kept only
   where a written annotation genuinely wins). This alone is
   measurable: §88's expansion family and §136's signature prints
   converge on family 1.
2. **Route every print through one renderer** with an OPTIONAL site:
   `type_to_string_at(id, Option<NodeId>)`. Types keep their minted
   text as the no-site fallback, so suite behaviour is unchanged
   until a site-aware arm lands — additive, measurable per arm.
3. **Land the site-aware arms one head at a time**, each with its
   own §-bar: trailing-default omission first (unblocks §136's
   +208), then namespace qualification at FILE-module containers
   (qualnamep's 644 with the modulespecifiers half), then the
   `import("...").Name` mint, then typeParameterToName's
   byText/shadow context (§20.1's 149).

**How we would know (a)-incremental is wrong:** if step 1's
structural signatures cannot reproduce the current 100%
printer_round_trip and 85.8% gradient BYTE-FOR-BYTE before any
site-aware arm lands, the reuse seams are load-bearing in a way
this study missed, and the ledger returns here.

## 4. What this does NOT block

The gradient still has non-print heads: the contextual/inference
arc (checker-2's pipeline), narrowing legs (predicate inference,
asserts-this), resolver parity (ImportEquals entity forms 205,
tsr-9or.1), generator residuals (§135's contextual next-types).
The print seam is the largest SINGLE owner in checker-1's lane, not
the only work.

## 5. Step-1 scoping addendum (first read, same day)

`instantiate_signature_type` (inference.rs:889) is ALREADY
structural — it holds `Signature` structs with TypeId parameters and
re-renders text via `signature_to_string` at mint time; the §90.1
print-rename and §89 keep-text machinery hang off it. So the study's
family-3 description ("string substitution") is PARTLY WRONG as
written: the signature REBUILD is structural; the string carriage is
confined to the §77-family written-reuse seams (`written_text`,
`written_return`) and the §10.13 baked outer texts.

CONSEQUENCE for §136's mystery: the `Iterable<number, any, any>`
texts should therefore have passed through `type_reference_text`
(where iteration 4's trim sat) — and measured byte-identical anyway.
Either the trim's default-resolution comparison failed silently
(the lib default resolving to a DIFFERENT `any`-flagged TypeId than
the argument — intrinsic identity vs a substituted clone), or the
texts are born in a written-reuse substitution after all. **The next
window's FIRST probe: re-apply §136's fill from its recorded map,
and put an env-gated eprintln at every point that constructs a
string containing `", any, any>"** — one filtered typedArrays run
names the birth site in minutes, and THAT site is where step 2's
renderer work begins. Do not start step 1's byte-identical refactor
before this probe; the refactor's shape depends on its answer.

## 6. The text-birth probe RAN (same day) — and §136 re-priced at a near-miss

Three env-gated probes (type_reference_text, signature_to_string,
store.new_named-with-backtrace) all read ZERO births for the
offending texts while the output still carried them — the birth site
is **`reference_text_at` (checker.rs:1315), the §95 composite
re-render**: it assembles `Name<ALL arguments>` TRANSIENTLY from
`type_reference_targets` at print time; nothing stored ever holds
the string. (En route, the §136 iteration-4/5 "byte-identical"
readings were explained: probes confirmed the trim inputs were
correct — those iterations almost certainly measured a STALE BINARY;
rtk masks cargo's Compiling lines, so the §88-trap's recompile check
must precede any byte-identical claim.)

With the trim shared into `reference_text_at` and the §136 fill
promoted, the pair reads **+525 G→R / 236 G→W / 49 R→W (~1.9:1)** —
typedArrays 108 WHOLE, complexRecursiveCollections 100,
genericDefaults 55. The adverse decomposes into many small
consumers of newly-resolving references (tsxLibraryManagedAttributes
29, arrayFrom 22, declarationEmit 17, the yield-position generic
calls 17 R→W), each needing its own gate. PARKED at the near-miss,
reverted byte-identical (verified against the accepted baseline);
the working diff is reproducible from this section in minutes:
(1) fill in get_instantiated_type_reference (non-empty written
lists, defaults cover the tail, per-position instantiate under the
map-so-far); (2) `visible_reference_arity` (closed-default trailing
trim, all-declarations arity scan); (3) the trim applied in
`reference_text_at` — stored mint texts UNTOUCHED. The next window
gates the adverse families one at a time; +240 net right is sitting
here behind ~5 small gates.

## 7. §136's second window — the written-arity model, and where it stops

The display-arity rework replaced the defaults-trim with the
faithful mechanism: a default-filled reference CARRIES ITS WRITTEN
ARITY (`reference_display_arity`), stamped at the annotation mint,
propagated through instantiation rebuilds, consulted by the stored
text and the composite re-render. Measured **+549 G→R / 212 G→W /
6 R→W (2.5:1, net +335)** — the yield-call R→W class vanished
(written-full preserved), R→W fell 49 → 6. Three position families
now have MEASURED wants:
  - WRITTEN-REUSE positions (the majority): written arity —
    typedArrays 108, complexRecursiveCollections 114, asyncGenerators;
  - BUILDER positions (inferred prints): FULL arity —
    tsxLibraryManagedAttributes 37, arrayFrom, declarationEmit
    (~87 lines G→W under the written model; flipping the composite
    to full INVERTS the totals, 518:253 — the reuse family is 6×
    the builder family);
  - genericDefaults: both in one case (+43/−18).
THE MISSING KEY, named: a **DEFAULT_LIBRARY NodeFlag** (or
file-name access in the checker) — the builder-position adverse is
user-file defaulted generics while every big win is lib-driven; a
lib-file gate on the FILL was unbuildable this window because the
checker cannot identify a lib file (no NodeFlag, no host hook).
PARKED ON BRANCH `checker1-136-wip` (pushed) at the +335 state —
next window: add the lib flag (parser stamps it from
default_library_path, one bit beside JAVASCRIPT_FILE), gate the
fill on it, re-measure; predicted ≥450 at ≥8:1.

## 8. §136 LANDS — the DEFAULT_LIBRARY flag was the key, exactly as §7 predicted

One bit (`NodeFlags::DEFAULT_LIBRARY`, bit 30), stamped by the
loader from `default_library_path` beside the JAVASCRIPT_FILE
stamp; `Checker::in_default_library` mirrors `in_js_file`; the
§136 fill gates on LIB-DECLARED targets. Measured:
**+474 G→R / +4 W→R against 105 G→W / ZERO R→W (4.55:1 raw)** —
typedArrays 108 and complexRecursiveCollections 114 whole,
asyncGenerators both files, right 411,117 → 411,733 = **85.97%**,
cases 4,150 → 4,172 (+22). The residual adverse is OWNED
ELSEWHERE: arrayFrom's 22 and the IteratorObject families' ~27 are
the twice-priced WRITTEN UNION ORDER head (§77.2 — the lib writes
`Iterable<T> | ArrayLike<T>`, our union sort spells it backwards;
these unions only became CONSTRUCTIBLE because the fill resolved
their constituents), and mapGroupBy/objectGroupBy ride the same
spellings. Excluding the priced-elsewhere class the arm reads
5.8:1. The user-file builder-position families (tsx,
genericDefaults) stay gapped behind the lib gate, exactly as
designed — their unlock remains per-site printing. The §136 arc
closes: five iterations refused, the probe, the written-arity
model, the lib gate — the printseam study's step-3 FIRST ARM is
landed, and the study's incremental path is now validated
end-to-end. Diagnostics rode along +19 (2,123 → 2,142).

## 9. §136's bare arm — the fill with no choice to make, +87/−0

§8 closed the §136 arc with the lib gate and said the user-file families
"stay gapped behind the lib gate, exactly as designed — their unlock remains
per-site printing". **For the fully-bare subcase that is not true, and the
measurement says so: `genericDefaults` — the case §8 named as the adverse —
gains 10 lines.**

### What was broken

`interface CompleteRuleConfig<M extends TypesMap = TypesMap>` referenced with no
type arguments at all. Upstream's window is
`[minTypeArgumentCount, len(typeParameters)]` (`checker.go:23189`) and
`minTypeArgumentCount` is **zero** when every parameter has a default, so a bare
reference is inside it and fills from the defaults. This port required
`!node.type_arguments.is_empty()` before it would fill, so a bare reference fell
out of the window and answered `errorType`.

### The cost was three subsystems away from the cause

Found while chasing what looked like a narrowing bug on a real repository:

```ts
const rule = doc.toJSON() as CompleteRuleConfig | null | string;
if (!rule || typeof rule !== "object") { continue; }
rule.id = "x";   // TS18047: 'rule' is possibly 'null'
```

An `errorType` constituent poisons the union it sits in. The truthiness step
answered `errorType` instead of `CompleteRuleConfig | string`, and the `typeof`
step then minted `object | null` out of it — so `null` came *back*, in a program
that had just tested for it.

Nothing in the diagnostic, the narrowing, or the union code was wrong. The
instrument that named it was `const t: never = rule;`, which prints the narrowed
type in the error message: `object | null` after the second guard where a
non-generic twin printed `P`. **When a narrowing result is wrong and the
narrowing code is right, print the type — the input is what changed.**

### Why the lib gate does not extend to this arm

§7's gate exists because a **partially**-written list must choose which position
its default fills, and that choice is visible in print — the builder-position
adverse (`tsxLibraryManagedAttributes`, `genericDefaults`) was measured at ~87
G→W under the written-arity model. A **bare** list fills every position and makes
no choice. There is nothing for the gate to protect, which is why the case that
priced the gate is now on the gaining side of it.

The same reasoning fixes the print: the bare arm passes **no** display arity, so
it renders every filled argument. Upstream prints `i00<number>` for a bare `i00`
(`genericDefaults.types:2538`). The first build reused §136's `Some(written)`
and printed `I<>` — an empty argument list, a spelling no TypeScript emits.

### Measured

Baseline is a `git worktree --detach` at HEAD, one corpus for both runs.

| | before | after |
|---|---:|---:|
| `checker_types` lines | 415,435 | **415,522** (+87) |
| `checker_types` per-case | — | **8 cases gain, 0 lose** |
| `diagnostics` cases | 2,378 | **2,379** (+1, `compiler/customEventDetail`) |
| `diagnostics` per-case | — | 1 gain, **0 lost** |

Gains: `signatureCombiningRestParameters3` (16), `…4` (12),
`reverseMappedTypeInferenceWidening1` (12), `customEventDetail` (11),
`mergedInstantiationAssignment` (10), `genericDefaults` (10),
`reverseMappedTypeInferenceSameSource1` (4), `awaitedTypeStrictNull` (2).

### The real-repository case it was found on is **not** fixed

Honest, because the temptation is to claim the origin story. `CompleteRuleConfig`
defaults to `TypesMap = Record<string, NodeType>`, and `Record<K, V>` — a lib
**mapped type** — still answers `errorType` here, so the default resolves to
error and the fill returns error at the same place. The four TS18047 on
`ast-grep-executor.ts` are unchanged, and their owner is the mapped-type
subsystem.

The second shape reported in the same session (`marketing-site.ts`, TS2322 on a
cached `string | undefined`) has the identical structure: `env.MARKETING_BASE_URL`
is `errorType` because the t3-env `createEnv` return type is unresolved, so the
assignment cannot narrow. **Both user-visible "narrowing bugs" are unresolved
types leaking into flow analysis.** That is a class, and it is worth naming: a
gap in the type side does not stay on the type side — it surfaces as a wrong
*diagnostic* somewhere else entirely, which is why they are so hard to attribute.

### How you would know this was wrong

Two arity true positives (`crates/tsr-checker/tests/types.rs`): a bare reference
to a generic with **no** default, and one whose defaults only **partly** cover.
Both must stay `error`. The mutation table there records something worth
repeating: **neither guard is individually observable** — the predicate and the
fill loop's `else { return error }` each refuse the partly-covered case, so only
mutating *both* reddens the test. The pair holds the rule; either alone reads as
dead code and is not.
