# r6-jsdoc — the JSDoc triage's held clusters (round 6)

Lane `r6-jsdoc`, epic `tsr-2zk` (with `tsr-2zk.1110`, `tsr-2zk.1100`).
Native source is `vendor/typescript-go` @ `5b1047d`. Continues
[r5-jsdoc5](r5-jsdoc5.md) §2/§3/§6 (the T1–T12 triage), [r5-js](r5-js.md)
and [ADR-0046](../../adr/0046-jsdoc-reparse-is-a-checker-query.md).

## 1. Base and method

Frozen base `b18aec06` (main `17265fac` plus bookkeeping), measured in this
container: types **549,853 RIGHT** of 556,303 aligned lines (843 GAP, 5,607
WRONG); diagnostics **5,530 RIGHT / 5,596 EMPTY_RIGHT** of 12,238 rows
(1,063 WRONG, 49 EMPTY_WRONG). Perf is Callgrind Ir of `tsr -p <project>
--singleThreaded --pretty false --noEmit`; the base binary reads
domain-model **1,091,518,529** and generic-imports **343,082,952**, and two
runs of that same binary differ by about 56k Ir (0.005%).

Every expectation was checked against a native `tsgo` built from the pinned
submodule (`scripts/offline-cargo/build-tsgo.sh`). Every measurement is
unfiltered against the base; `slowcases` runs on both dumps.

Lane ownership leaves most producers in other files. Each such change ships
as a measured diff here, against this lane's own commit on the branch; the
lane's commits are neutral alone (both dumps identical to the base), so a
diff's numbers are the whole effect.

## 2. T4: `@overload` tags are declarations (`tsr-2zk.16.163`)

**Forcing fact.** `overloadTag1`'s `export function overloaded(a,b)` under
two `@overload` blocks printed `(a: number, b: number) => number` — the
first overload's parameters read as the implementation's — and its `a`
printed `any`. Native's reparser turns each `@overload` of every comment on
a function, method or constructor declaration (outside object literals)
into a body-less declaration of the host's kind, name and modifiers
(`reparseUnhosted`, `parser/reparser.go:134`; `reparseJSDocSignature`,
`:142`), and `parseListIndex` (`parser/parser.go:619`) puts it **before** the
host. The binder makes it another declaration of the host's symbol;
`getSignaturesOfSymbol` (`checker.go:19806`) reads it as an overload and
skips the host as the implementation (`previous.Flags&NodeFlagsReparsed !=
0` stands in for adjacency); the host's own parameters read the comment's
top-level `@param`s.

**Port.** ADR-0046 keeps declaration-emitting arms as parser-built nodes.
Three halves:

- **Parser** (`parse_tag`'s `overload` arm, diff): `parseOverloadTag`
  (`parser/jsdoc.go:1157`) — the run of `@param`/`@this` tags and one
  `@return` folds into the tag through `parse_jsdoc_signature`, as for
  `@callback`, stored as the reparsed function type (real
  `ParameterDeclaration`s with the reparsed `?`, `...` and names). Unlike a
  callback's, a missing `@return` leaves the return type unset: the reparse
  is a body-less declaration, whose return is `any`.
- **Binder** (`bind_jsdoc_declarations`, diff): binds the signature, and
  inserts the tag into the host symbol's declarations ahead of the host,
  only for the hosts `reparseUnhosted` accepts.
- **Checker** (`jsdoc_overloads.rs`, owned, committed): the parts a function
  type cannot carry — the host (`jsdoc_overload_host`), the tags before a
  host (`jsdoc_overload_tags`), the implementation rule against a reparsed
  previous (`jsdoc_overload_precedes_implementation`), the error location
  (the tag name, `finishReparsedNode(signature, tag.TagName())`) and the
  TS7012 arm of `checkFunctionOrMethodDeclaration` (`checker.go:3446`,
  `reportImplicitAny`'s reparsed arm at `:18332`). The diff adds
  `jsdoc_overload_signature`: `getSignatureFromDeclaration` over the tag's
  function type, with `gatherTypeParameters(jsDoc, false)`
  (`reparser.go:293`) for a function or method; a constructor's are the
  class's, set by the constructor list.

The hooks (diff [`r6-jsdoc-overload.diff`](r6-jsdoc-overload.diff)):

- `signatures.rs`: `get_signatures_of_symbol_for_type` takes an overload
  tag's signature; `resolve_class_construct_signatures` lists each
  constructor's tags before it (native's `__constructor` symbol holds
  them); `is_overload_implementation` asks the owned rule first;
  `type_parameter_of` becomes `pub(crate)`.
- `check.rs`: `checkFunctionOrConstructorSymbol` sees the tags as body-less
  function declarations beside their host (shared parent, host's modifiers,
  no "implementation expected" for a reparsed previous); TS2394 relates the
  tag's signature and reports at the tag name; `constructor_siblings_of`
  includes them; and the JS gate of `checkFunctionOrMethodDeclaration`
  (`checker.go:3430`): a JS file never checks the local symbol and checks the
  symbol only when it has a parent (an export or a member).
- `symbols.rs` `jsdoc_parameter_annotation`: the decline on any comment
  carrying `@overload` goes. It was written when the implementation's
  parameters read the first overload's `@param`; the replay reads the last
  comment's top-level tags, which no longer include an overload's run.

**Judgment calls.** The declaration in the symbol is the **tag** node and the
signature's declaration is the tag's **function type**: no node can be the
reparsed `FunctionDeclaration` itself (ADR-0046 fact 2), and
`reorder_candidates` must see a function-like declaration (a tag would read
as parentless and reverse the list). The committed owned functions carry
`#[expect(dead_code)]` naming the diff, the handshake `signatures.rs:2832`
describes: applying the diff deletes them, and forgetting to fails
`-D warnings`.

**Measured** (the diff on this lane's commit, unfiltered): types **549,853
→ 549,890 RIGHT (+37 lines)**, cases **+4** (`overloadTag1`,
`overloadTag2`, `jsFileMethodOverloads`, `jsFileMethodOverloads3`);
diagnostics **5,530 → 5,531 RIGHT** (`jsFileMethodOverloads3`: TS7012 ×2);
`overloadTag1`/`2` gain their TS2394 rows' codes but stay WRONG (below).
Zero losses on both dumps; no other row changed text; `slowcases` clean.
Ir domain-model 1,092,357,885 (+0.08%), generic-imports 343,074,123
(−0.003%). `crates/tsr-checker/tests/jsdoc_overloads.rs` (in the diff):
three tests, each failing without it; each expectation checked against
native `tsgo`.

**Left in T4.**

- `overloadTag1` TS2769 ×2 and `overloadTag2` TS2554: `calls.rs`'
  `check_resolve_call_arity` answers `Undecided` for every call in a JS
  file, so no argument or arity error is reported there. Narrowing it is
  `chooseOverload`'s JS arms (`isUntypedSignatureInJSFile`, `arguments`
  use), main's file.
- `overloadTag2` TS7006 on the constructor's undocumented `b`:
  `implicit_any.rs` `check_implicit_any_parameters` declines every JS file
  (its own comment: written before the `@param` replay existed).
- `overloadTag3`'s two `new Foo()` lines (`Foo<any>` for `Foo<number>`):
  not the overload — a JS `new` of a `@template` class with no arguments
  does not infer from the assignment's contextual type, overload or not.

**Falsifier.** A JS method with `@overload` whose host's own `@template`
differs from the overload comment's: native gives each overload its own
comment's type parameters; a case wanting the host's would show
`gatherTypeParameters` read from the wrong comment.
