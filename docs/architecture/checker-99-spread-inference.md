# Generic rest spreads and contextual literal constraints

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 6c7fe447: 452,420/478,855 correct assertions (94.48%).
Goal: 99%, requiring 474,067 correct assertions.

## Native behavior and representation

inferTypeArguments (checker.go:9390) builds a spread argument type for a
non-array rest. getEffectiveCallArguments (:30042) expands tuple spreads into
synthetic arguments. getSpreadArgumentType (:29500) preserves a lone mutable
array or generic array identity, converts readonly inputs into mutable images,
retains variadic positions and labels, and derives arrays from other iterables.
Ordinary tail arguments use the corresponding tuple or indexed contextual type.
Const/primitive contexts retain literals; other fresh literals widen.

The port now expands effective elements in the non-array-rest inference tail
and constructs their type with the existing tuple normalizer. Optional tuple
elements become required synthetic arguments with their possible undefined
value; variadic positions remain spreads. getMutableArrayOrTupleType preserves
mutable generic U and maps readonly U to [...U]. Union inputs distribute and
iterator inputs use the existing element-type resolver. Spreads before ordinary
parameters and the older array-rest overload path remain separate work.

checkExpressionWithContextualType regularizes a literal matching its context
before ordinary literal widening. isLiteralOfContextualType (:25522) consults
resolved base constraints for indexed, conditional and substitution types,
not just direct type-parameter constraints. It also recognizes symbol contexts
and recursively checks literal constraints. This reuses base_constraint_of_type,
including its recursion guards.

The stronger constraint rule exposed a contextual lookup bug after union
discrimination. A mapped union constituent already has instantiated properties,
but the contextual path re-read the shared declaration symbol and recovered
its template's P. It now uses get_type_of_property_of_type on the discriminated
constituent. This preserves the existing discrimination decision and observes
the chosen member's semantic type.

## Experiments and controls

The initial spread builder rejected placeholder error types for context-sensitive
callbacks, losing ten variadicTuples2 assertions. Keeping those placeholders
through the existing deferred inference pass restored all ten. The first full
spread run gained 75 with zero RIGHT losses. Resolving base constraints raised
the gross gain to 144 but lost four correlatedUnions property assertions.
Tracing showed contexts RecordMap[P]/TypeMap[P] instead of the instantiated
number/string members. Resolving alias bodies again did not help and was
reverted. Reading the instantiated property after discrimination recovered the
four losses and improved additional contextual union cases. A complete rerun
then gained 205 with zero RIGHT losses. Review added an exact optional property
control: native tuple spreads still supply undefined for an optional element
with that flag enabled, so tuple expansion must not suppress it.

spread_inference.rs covers tuple expansion, fixed prefixes/suffixes, optional
and labeled elements, variadic tuples, mutable and readonly arrays, generic
array identities, union spreads, any spreads, iterables, primitive/literal/const
constraints and ordinary widening. Native declaration controls are retained at
/tmp/tsr-99-spread.ts, /tmp/tsr-99-spread-extra.ts and
/tmp/tsr-99-spread-literals.ts with matching output folders and -native.log files.
All use explicit strict mode and ES2015. An additional exactOptionalPropertyTypes:true control keeps undefined in an
optional tuple spread. In total, 27 native declaration outcomes pass. A reduced correlatedUnions control
asserts three property types from the pinned upstream .types baseline.

A unique-symbol tuple preserves its unique type but still prints [unique symbol]
where native declaration emit writes [typeof key]. That declaration rendering
is not counted as a matching control. No oracle or corpus inputs were changed.

Simplification and review run sequentially in the main thread under the user's
tool mapping. Review checks array identity, readonly conversion, optional and
variadic flags, argument order, literal widening, callback placeholders, shared
constraint recursion and contextual member instantiation. No independent or
cross-model coverage is claimed.

## Verified checkpoint

CODE_CHECKPOINT: 452,625/478,855 assertions (94.52%). Aligned verdicts:
474,243 total, 452,625 RIGHT, 3,378 GAP, 18,240 WRONG. Against 6c7fe447:
197 WRONG->RIGHT and 8 GAP->RIGHT, zero RIGHT losses, one GAP->WRONG and
four changed wrong answers. The new wrong answer preserves an unresolved
PrivateMapped alias instead of expanding its conditional body. The largest
gains are genericRestParameters1 (42), correlatedUnions (41),
reverseMappedTypeIntersectionConstraint (27) and arraySpreadInCall (24).
The exact-optional correction preserves this corpus count.

Evidence:
- /tmp/tsr-99-spread-final-verdict.tsv
- /tmp/tsr-99-spread-final-verdict.log
- /tmp/tsr-99-spread-final-transitions.txt
- Checker sources plus trace_case SHA-256: af7acadafab46283e4f11bd3c73055eaf81868ea56d5f648f16e8058554180dd

6,704/9,538 cases pass completely (70.29%), up five. Release workspace tests
finish with exit 0 and 188 passing result blocks. Clippy with warnings denied,
all 3,361 upstream anchors and the refreshed checker_types snapshot pass.
Clippy's test-only raw-string delimiter correction was followed by all five
focused tests and clippy again; the checker/trace source hash remained unchanged.
Format and whitespace are checked with the final checkpoint.

The depend instrument retains stale C1/C4 controls (tsr-6.29). Its final
control output is recorded below; aligned verdicts and the checker_types
snapshot provide the authoritative coverage counts.

```text
## Controls
  C1 construction: roots that do not gap  572  (expect 0)
  C2 construction: cycles 285, depth-cap hits 0
  C3 arithmetic:   root buckets sum to 4369, gap lines walked 4369
  C4 frozen:       STATUS.md publishes ~127,736 gap lines at 63.66%
```

The 99% target still requires 21,442 additional correct assertions.

Derived array interfaces still decline (native /tmp/tsr-99-spread-derived.ts
returns string[] for interfaces extending Array/ReadonlyArray). Their iterable
and array-like representation remains separate from the admitted tuple/array
shapes here. Broader effective arguments before ordinary parameters, complete spread
applicability, CheckMode propagation and generic mapped/conditional inference
remain tracked in tsr-6.28/tsr-6.30. The 99% goal remains active.
