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
