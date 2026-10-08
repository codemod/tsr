# r5-jsdoc4 — JSDoc walk coverage, heritage constraints, catch clauses (round 5)

Lane `tsr-2zk.1075`, epic `tsr-2zk`. Native source is `vendor/typescript-go`
@ `5b1047d`. Continues [r5-jsdoc3](r5-jsdoc3.md) (its §5 is this lane's item
list), [r5-jsdoc2](r5-jsdoc2.md) and
[ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

Frozen baseline (`f5d0291`, batch AB landed with r5-jsdoc3's seven diffs):
types **545,044 RIGHT** of 552,533 aligned lines (906 GAP, 6,583 WRONG);
diagnostics **5,385 RIGHT / 5,584 EMPTY_RIGHT** of 12,238 rows.

Perf is judged by Callgrind Ir (`tsr -p <project> --singleThreaded --pretty
false --noEmit`). Three runs of the base binary on domain-model read
1,196,991,723 / 1,196,992,447 / 1,197,691,013: a 0.06% spread, which is the
noise floor every Ir figure below is read against.

## 1. Walk coverage: every reparsed type node native checks (item 1)

**Forcing fact.** r5-jsdoc3 §3 landed `check_node`'s JSDoc hook
(`jsdoc_reparsed_type_nodes`) for a variable's, class property's and
parameter's own type, a function's `@returns`, and typedef bodies. Its module
doc listed what native's `checkSourceElement` also reaches and the hook did
not: casts, `@satisfies`, `@this`, a full-signature `@type`, `@callback`,
`@overload`, and export/accessor `@type`.

**Port** (`jsdoc_checks.rs`), each node as `reparser.go` exposes it:

| Reparsed node | Native reader | Rule visited |
|---|---|---|
| `@type` on a parenthesized expression or `return` → `AsExpression` (`makeNewCast`, `reparser.go:378`) | `checkAssertion` → `checkSourceElement(type)` (`checker.go:12302`) | each typed `@type` of the host's **last** comment (every one wraps again) |
| `@satisfies` → `SatisfiesExpression` (`reparser.go:396`) | `checkSatisfiesExpression` (`checker.go:10743`) | each typed `@satisfies` of the last comment, on the hosts `jsdoc_satisfies_target` already answers for the relation check |
| `@this` → a prefixed `this` parameter (`reparser.go:487`) | `checkSignatureDeclaration` → `checkParameter` | its type; **and** `checkParameter`'s `this` arm: TS2681/TS2730/TS2784 at the tag name (the parameter's location) |
| full-signature `@type` (`FullSignature`) | `checkFunctionOrMethodDeclaration` (`checker.go:10143`), `checkFunctionExpressionOrObjectLiteralMethod` (`:3441`) | the type node |
| `@callback` → `JSTypeAliasDeclaration` of a function type (`reparser.go:100`) | `checkTypeAliasDeclaration` | the function type the parser already builds (`parse_callback_tag`) |
| `@overload` → a `FunctionDeclaration`/`MethodDeclaration`/`ConstructorDeclaration` per tag (`reparser.go:134`) | `checkFunctionDeclaration` → `checkSignatureDeclaration` | each signature's `@this`, `@param` and `@return` types, for a function/method/constructor host outside every object literal |
| export assignment `@type` (`reparser.go:356`) | `checkExportAssignment` (`checker.go:5662`) | the type node |

An accessor's own `@type` was already covered: the replay's getter arm makes
it the function's `return_type`.

`JSDocReparsedFunction` gained `this_tag` (the first `@this` that prefixes a
parameter), set where the replay already tracked `ThisParameter::Reparsed`.
`jsdoc_satisfies_target` (`jsdoc_annotations.rs`) became `pub(crate)` so the
walk asks the same host rule the relation check uses; no behaviour change.

**Judgment calls.**

- *Full signature and export `@type` are visited with the whole walk.*
  Native only `getTypeFromTypeNode`s them, which reports name resolution and
  arity but not `checkTypeReferenceNode`'s TS2344. This port reports
  TS2304 only from the walk (`check_type_reference_name`), so visiting is the
  only road to the resolution errors; the over-report is a constraint error
  inside one of these two positions. No corpus case moved either way.
  Falsifier: a case with a TS2344 inside a function's full-signature `@type`.
- *Casts follow native's hosting, not the typing road.* The walk takes each
  typed `@type` of the last comment; `jsdoc_cast_annotation` (the type the
  paren answers) takes the first typed `@type` of any comment. They differ
  only for two typed `@type` tags, or a non-last comment carrying one.
- *Overload signatures' `@template` tags and every function's `@template`
  constraints are not visited* (nor `@implements`, `@import`, typedef
  `@property` types): no corpus row needs them, and the function-level
  `@template` walk would also need TS2706/TS2744 (`jsdocTemplateTagDefault`),
  which the TypeScript path computes in a different file.
- *The object-literal rule* is `parsingContexts`' `PCObjectLiteralMembers`
  bit, which stays set through every list parsed inside an object literal;
  "any `ObjectLiteralExpression` ancestor" is the same set of hosts.

**Measured** (unfiltered, against `f5d0291`):

- diagnostics **5,385 → 5,386 RIGHT** (thisTag3); EMPTY_RIGHT unchanged;
  checkJsTypeDefNoUnusedLocalMarked stays WRONG but loses its wrong TS6133
  (the cast now references `FooFun`; the remaining TS2352 is the deferred
  cast check, `expressions.rs`);
- types unchanged (545,044); zero RIGHT→non-RIGHT on both dumps;
- Ir: domain-model 1,197,819,796 / 1,197,767,845 (base 1,196.99–1,197.69 M,
  +0.01–0.07%, inside the base's own spread), generic-imports 342,898,976
  (base 342,914,464). Nothing new runs on the TypeScript path: the hook is
  gated on `file_is_js`.
- `crates/tsr-checker/tests/jsdoc_reparsed_checks.rs`: nine tests; six fail
  without the change.

**Not covered, with the piece they wait on.**

- `jsdocResolveNameFailureInTypedef`, `reuseTypeAnnotationImportTypeInGlobalThisTypeArgument`:
  a typedef in the comment before end of file. The parser parses that
  comment but does not attach it (`parse_source_file`, "not yet attached");
  native's `withJSDoc(eof, …)` hosts it on the end-of-file token. Parser.
- `jsFileMethodOverloads3`, `overloadTag1/2`: TS7012/TS2394 come from the
  overload *signatures*, which `signatures.rs` does not build (`.16.163`).
- `typedefMultipleTypeParameters` TS2314: the arity rule
  (`type_argument_arity.rs`) does not count a typedef's `@template`
  parameters.
