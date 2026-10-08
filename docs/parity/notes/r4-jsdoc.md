# r4-jsdoc — JSDoc type hosting (round 4)

Lane brief: [round4.md](../round4.md), section `r4-jsdoc`. Issues
`tsr-2zk.16.98`, `.16.105`, `.16.106`, `.16.107`, `.16.38`. Native source is
`vendor/typescript-go` @ `5b1047d`.

## Ownership as worked

The owned files (`jsdoc_params.rs`, `jsdoc_annotations.rs`,
`jsdoc_modifiers.rs`, `jsdoc_full_signature.rs`, `js_case_data.rs`) hold the
replay of `reparseHosted` for function hosts, but none of the read sites the
five issues name. `js_case_data.rs` is generated Unicode case-mapping data
(`internal/stringutil/js_case_generated.go`) and has nothing to do with JSDoc.
On 2026-10-08 the integrator assigned three JSDoc-only helpers in
`symbols.rs` to this lane — `jsdoc_type_annotation`,
`jsdoc_parameter_annotation`, `jsdoc_cast_annotation` — editable inside their
bodies only. Every other site is delivered as a measured patch under
`docs/parity/notes/r4-jsdoc-*.diff`.

## 1. A parameter's own `@type` (`.16.98`, `jsdocTypeTagOnParameter1`)

**Native.** `reparseHosted`'s `KindJSDocTypeTag` arm has a `KindParameter`
case (`parser/reparser.go:363`): a comment written on the parameter itself,
`function f(/** @type {string} */ message)`, sets the parameter's `Type` from
the first `@type` tag of its last comment. From then on every consumer —
`getTypeForVariableLikeDeclaration` for the symbol, `getSignatureFromDeclaration`
for the signature, the reparse of the function's own comment (`param.Type ==
nil` test of the `@param` arm, `noTypedParams` of the full-signature arm) —
sees a typed parameter.

**Port.** `Checker::jsdoc_parameter_hosted_type` (`jsdoc_params.rs`) answers
that `Type` from the parameter's `jsdoc_entries` entry. Two consumers read it:

- `jsdoc_parameter_annotation` (`symbols.rs`) asks it first, before the
  function hosts' `@param` tags, because the parameter is finished — and its
  comment reparsed — before the function's comment is.
- `jsdoc_reparsed_function`'s replay seeds its per-parameter `typed` state
  with it, so a later `@param` does not claim the slot and a function `@type`
  does not become the full signature (`noTypedParams` is false).

The signature line needs no third change: `getSignatureFromDeclaration`'s
port types an identifier parameter through the symbol.

**Measured.** `jsdocTypeTagOnParameter1` 7 WRONG -> RIGHT; no other type
line or diagnostic verdict moves.

**Convention record.** No cache, side table or traversal is added: one
`jsdoc_entries` lookup keyed by the parameter's `NodeId` (owner: the parser's
side table, immutable after parse), then one comment's tags. Nothing is
published.

**Falsifier.** A parameter with both its own `@type` and a matching `@param`
whose baseline shows the `@param` type would prove the order wrong.
