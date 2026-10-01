# Concrete indexed access and recursive object identities

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline afdc0a5b: 451,569/478,855 correct assertions (94.30%).

## Native rules and representation

getTypeFromIndexedAccessTypeNode and getIndexedAccessTypeOrUndefined
(checker.go:22950,26935) use the same semantic object/index resolver for concrete
and generic annotations. Concrete union indexes project each property and keep
the enclosing alias on the resulting union. The port now takes that shared path
before its remaining legacy fallbacks.

getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode (checker.go:22933) creates
and caches an object identity before resolving its members. The port now reserves
a non-interned identity, builds the members, and completes that identity. Recursive
symbol reads of an annotated literal use the reserved identity instead of marking
the inference stack circular. The cache includes the node, effective alias
bindings, and mapped-template mode. Signature and anonymous-property metadata
move to the completed identity. The eager printer still needs a provisional
written spelling; this is not a complete port of native lazy rendering.

getTypeAliasInstantiation instantiates the alias body. Indexed bodies now evaluate
under their argument mapper. Deferred indexed results retain object/index metadata
and an alias reference; unions carry their alias symbol and arguments, and function
results preserve alias naming alongside signatures. Instantiation rebuilds a named
indexed alias through its arguments before resolving its operands. This preserves
both semantic substitution and alias display, including array parenthesization.

Key enumeration now includes known inherited keys. An unresolved base cannot prove
an empty key set and therefore declines enumeration; a recursion set stops cyclic
key queries. This repairs the false never result introduced by interpreting an
unresolved circular base as an empty object.

## Experiments and review

The initial combined build gained161 assertions but lost89. Most losses exposed
missing alias metadata: returning a raw indexed type expanded Entry<T,K>-style
aliases, and origin text alone did not mark a union as a named type. Preserving
alias identities and rebuilding them through the existing reference mapper reduced
the losses to six. Four came from incomplete base-key enumeration and two from
function-alias rendering. Both mechanisms are repaired in the final comparison.

Focused native controls cover recursive indexed member reads, tuple-length aliases,
terminating recursive indexed conditions, union-key aliases, never keys, generic
indexed alias identity, independent string/number mapper instances, and inherited
keys. Rust assertions check the resulting expression/signature types. One older
checker test intentionally asserted the previous foreign-keyof gap;its expected
result now matches native string|number after an exact pinned control. Native
declaration emission succeeds and verifies the inferred return types.

Manual sequential review checked reserved identity completion, mapper-specific
cache keys, preserved optionality on recursive symbol reads, alias metadata and
instantiation ordering, and inherited-key cycle handling. No independent agent
review was performed. A RIGHT loss rejects the checkpoint; all initial losses
were resolved before delivery. The denominator, oracle, and expectations remain
unchanged.

## Measurement and limits

Checkpoint CODE_CHECKPOINT: 451,723/478,855 correct assertions (94.33%).
6,668/9,538 complete cases (69.91%).
Aligned verdicts: 474,243 total; 451,723 RIGHT; 3,513 GAP; 19,007 WRONG.
Relative to afdc0a5b: 118 WRONG→RIGHT, 36 GAP→RIGHT, zero RIGHT losses,
40 GAP→WRONG, 59 changed wrong answers, and 2 WRONG→GAP.
The 95% threshold is454,913; 3,190 additional matches remain.

The newly exposed wrong answers are19 assignmentCompatWithObjectMembers rows,
17 correlatedUnions callback/context rows,3 declarationEmitNoNonRequiredParens
rows, and1 typeInferenceWithExcessPropertiesJsx row. They remain visible in the
comparison. The largest gains are44 correlated-union assertions,19 never-index
assertions,18 recursive object assertions, and9 recursive indexed alias assertions.

Remaining work stays in tsr-6.30: generic mapped-value extraction, full generic
object/index deferral, completely lazy recursive alias instantiation/rendering,
and callback inference through mapped indexes. The Tree control's concrete return
terminates correctly, while its unconstrained generic declaration still expands
too eagerly up to the existing instantiation limit. Recursive accessor spelling
can still use a provisional empty form when no written spelling is available.
These are known limitations, not completed native parity.

Evidence: /tmp/tsr-95-recursive-indexed-final-verdict.{tsv,log},
/tmp/tsr-95-recursive-indexed-final-transitions.txt,
/tmp/tsr-95-recursive-indexed-{controls,workspace-tests,clippy,anchors,coverage,depend}.log,
and /tmp/tsr-95-oracle-recursive-indexed.ts with native emitted declarations.
Checker sources plus trace_case.rs SHA256:
eb61f36de4460ef6402608349c575600cbb8b1e6656ae45f7934292b170b0dd5.

Release workspace tests,clippy with warnings denied,all3,364 upstream anchors,
format and whitespace checks pass. Checker snapshot refreshed. Binder retains
its preceding verified100% result. Fresh depend walks4,519 lines;C1 fails with
575 roots that no longer gap,C2 reports287 cycles and zero depth-cap hits,C3
balances,and C4 still cites historical counts. These do not establish reachable
coverage;instrument repair remains tsr-6.29.
