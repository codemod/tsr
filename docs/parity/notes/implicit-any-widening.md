# Lane `implicit-any-widening` notes (tsr-2zk.11)

`noImplicitAny` reporting (`reportImplicitAny`, `reportErrorsFromWidening`) and
literal widening. Baseline for every number below: the `06f25e0` lane list,
measured from branch head `7070635` with `diagverdictdump` (3,471 RIGHT
diagnostics cases, 2,017 WRONG).

## 1. TS7051 is an arm of `reportImplicitAny`, not a separate rule

`reportImplicitAny`'s parameter arm (`checker.go:18290`, pinned `5b1047d`)
reports `Parameter has a name but no type. Did you mean 'arg{N}: {name}'?`
instead of TS7006/TS7019 when the parameter belongs to a call signature, a
method signature or a function type **and** its name is either a type keyword
(`IsTypeNodeKind(IdentifierToKeywordKind(name))`) or resolves with the `Type`
meaning from the parameter. `implicit_any.rs` printed TS7006/TS7019 for these
because the arm did not exist.

`parameter_name_is_probably_a_type` asks exactly those two questions:
`tsr_scanner::keyword_kind` for the keyword half (only the keyword members of
`IsTypeNodeKind` can come out of a keyword lookup, so the type-node range and
JSDoc kinds are unreachable here) and `Binder::resolve_name(…, TYPE)` for the
resolution half. Construct signatures and constructor types are excluded
because upstream's kind test names only the three kinds.

**Measured.** `noImplicitAnyNamelessParameter`, `strictModeReservedWord2`
converted; corpus TS7051 missing 10 → 0, no verdict lost.

## 2. No `file_has_parse_errors` gate on implicit-any reporting

Every rule in `implicit_any.rs` opened with `|| self.file_has_parse_errors`.
Upstream has no such gate: `reportImplicitAny` runs from the widening of a
declaration's type whatever the file's syntactic diagnostics, and tsgo's
baselines carry TS7006/TS7031/TS7010 beside TS1005 (`destructuringParameterDeclaration2`
reports 35 binding-element lines after a missing `]` on line 8).

**Measured** (whole corpus, line-level): missing 7,151 → 7,045, extra
1,794 → 1,813; eleven cases converted, no RIGHT/EMPTY_RIGHT verdict lost, no
types line lost.

**The 19 new extra lines are parser divergences, not this rule.** Each sits in
a file where TSR's recovery tree differs from tsgo's, so the declaration this
rule sees is not the declaration upstream sees:

| case | TSR tree | tsgo tree |
|---|---|---|
| `parseInvalidNullableTypes`, `parseInvalidNonNullableTypes` | `a: string?` / `a: ?string` loses the annotation (TS7006) | `JSDocNullableType` / `JSDocNonNullableType` annotation |
| `derivedClassSuperCallsInNonConstructorMembers`, `reservedWords2/3`, `MemberFunctionDeclaration5_es6`, `parserErrorRecovery_ParameterList6`, `parserEqualsGreaterThanAfterFunction1` | a recovered, name-bearing bodiless method/function (TS7010) | a different recovery shape |
| `parametersSyntaxErrorNoCrash1/2/3`, `importCallExpressionIncorrect2` | a binding pattern recovered as a parameter (TS7031) | a different recovery shape |
| `ArrowFunction3`, `parserX_ArrowFunction3` | `(a): =>` recovered as an arrow with an unannotated `a` | `a` is not reported |

These are owned by the parser crates and reported to the integrator rather
than suppressed here; a gate that hides them also hid 106 correct lines.

**Falsifier.** If a parser-recovery fix lands and one of these cases still
reports the extra line, the cause is in this rule, not the tree.
