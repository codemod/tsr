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
