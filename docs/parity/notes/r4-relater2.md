# r4-relater2 — three relater arms from r4-realworld (`tsr-2zk.952`)

Lane under `tsr-2zk.952`, causes 7, 11 and 16 of
[`r4-realworld.md`](r4-realworld.md). The integrator closed round 4 before any
commit passed both loss checks, so **no checker code was changed here**. This
file records the root causes, the native anchors (`vendor/typescript-go` @
`5b1047d`), and the candidate patch. The patch converted all three repros, but
**its loss checks were not run to completion**.

Frozen base: `56cec1e`. Its `diagverdictdump` gives RIGHT 4343, EMPTY_RIGHT 4979,
WRONG 1159 and EMPTY_WRONG 89. Its `verdictdump` gives RIGHT 470235, WRONG 6851
and GAP 893. On jsTyping (`types: []`, scratch config in the project
directory), base TSR reports 505 diagnostics. Of those, 12 are
`SearchResult<undefined>` TS2322 and 5 are the `TracingNode`/`EmitNode` TS2352.

## tsr-2zk.934 (cause 16): already ported, no action needed

This was ported in `348cebf` ("r4-subtype: any source under the subtype relations",
`tsr-2zk.921`), at `relater.rs` `is_related_to_with_flags`, the arm that
starts "isSimpleTypeRelatedTo relates an `any` source to anything only under
the assignable and comparable relations" (relater.go:261). At base `56cec1e`,
`any_property_source_is_not_a_subtype_of_an_object_property` already prints
native's `(4,14) TS2322 'SourceFile[]'`. Its `#[ignore]` in
`crates/tsr-conformance/tests/realworld_repros.rs` can be removed. That file was
left untouched here.

## tsr-2zk.929 (cause 11): comparable optional-property handling

The `assertion_overlap.rs` callers are right. The divergence is in
`Relater::properties_related_to_with_optionals`, at two sites:

1. **A target property is optional and the source has no such member.**
   Native `propertiesRelatedTo` (relater.go:4232) sets
   `requireOptionalProperties` only for subtype and strictSubtype. TSR admits
   the missing optional member only under `Assignable`, so Comparable falls to
   §387's NotRelated. The fix is to replace `self.relation ==
   Relation::Assignable` with
   `!matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)`.
   This converts `n as { tracingPath?: string }` / `as TracingNode`.
2. **The source member is optional and the target member is required.**
   Native `propertyRelatedTo` passes `skipOptional = relation ==
   comparableRelation` (relater.go:4259 and :4318) for every source shape.
   TSR skips the check under Comparable only for an intersection source, through
   `source_parts.is_none() || relation != Comparable`. The fix is to gate the
   whole block on `self.relation != Relation::Comparable`. This converts
   `{ a: [1] } as { a?: number[]; c: number }` / `as EmitNode`.

The weak-type check itself is already faithful.
`fails_common_property_check` is skipped under Comparable for a source that is
not a unit type, as relater.go:2675 does.

## tsr-2zk.927 (cause 7): alias-variance gate

Native `structuredTypeRelatedToWorker` (relater.go:3389-3392) probes
`getAliasVariances` only when `source.flags & (Object|Conditional) != 0`.
TSR mints every generic alias instantiation that is not specially handled as a
`Named` OBJECT reference (`declared.rs` `create_type_reference_with_display`,
around :6168-6224). `SearchResult<undefined>` with `SearchResult<T> = { value: T |
undefined } | undefined` is therefore an opaque reference, and the merged
reference/alias arm in `structured_type_related_to_worker` (the
"relateVariances" block, `same_target_references`) decides it by measured
covariance. The answer is `undefined -> Resolved` = false, a false TS2322.

Candidate port, inside the owned arm: when the shared symbol is a TYPE_ALIAS,
evaluate the source body with `evaluate_alias_body`. This is the existing
`(symbol, arguments)` cache that `constraints.rs` `compute_base_constraint`
already reads, on the grounds that "native aliases already have their body's
semantic identity". If the body's flags do not intersect `OBJECT|CONDITIONAL`,
return `is_related_to(source_body, target_body)` instead of relating variances.
An unevaluable body keeps the variance road. The object-alias control
(`R<undefined> -> R<string>` still errors, as native does) is unaffected.

## Measured so far

The candidate patch is the two arms above plus a helper
`non_object_alias_bodies(symbol, source_args, target_args)`. On release TSR
with this patch:

- the `alias.ts`, `cmp.ts` and `anysub.ts` repros print exactly native's
  expected output;
- loss checks, the coverage run, perf and the workspace tests were **not run**,
  because the round closed.

The comparable-only dump was killed before it finished.

Risk to check first. The alias-body arm is on a hot path for same-alias
pairs. Its cost is one symbol-flag test, plus a cached `evaluate_alias_body`
for TYPE_ALIAS pairs only. Relating union bodies could recurse through
recursive union aliases; the relation stack should bound that, but this is
unverified.

## Candidate patch (unverified)

```diff
diff --git a/crates/tsr-checker/src/relater.rs b/crates/tsr-checker/src/relater.rs
index 03bbbfb..d8e445a 100644
--- a/crates/tsr-checker/src/relater.rs
+++ b/crates/tsr-checker/src/relater.rs
@@ -1555,6 +1555,36 @@ impl Relater<'_, '_, '_> {
         }
     }
 
+    /// The evaluated bodies of two instantiations of one type alias whose
+    /// source body is neither an Object nor a Conditional type
+    /// (`structuredTypeRelatedToWorker`'s alias-variance gate,
+    /// `relater.go:3392`). `None` for a class or interface reference, an
+    /// Object/Conditional body, or a body that does not evaluate; those keep
+    /// the variance road. The bodies come from `evaluate_alias_body`'s
+    /// existing `(symbol, arguments)` cache; nothing is published here.
+    fn non_object_alias_bodies(
+        &mut self,
+        symbol: SymbolId,
+        source_arguments: &[TypeId],
+        target_arguments: &[TypeId],
+    ) -> Option<(TypeId, TypeId)> {
+        if !self.checker.binder.symbols().get(symbol).flags.contains(SymbolFlags::TYPE_ALIAS)
+        {
+            return None;
+        }
+        let source_body = self.checker.evaluate_alias_body(symbol, source_arguments)?;
+        if self
+            .checker
+            .type_of(source_body)
+            .flags
+            .intersects(TypeFlags::OBJECT | TypeFlags::CONDITIONAL)
+        {
+            return None;
+        }
+        let target_body = self.checker.evaluate_alias_body(symbol, target_arguments)?;
+        Some((source_body, target_body))
+    }
+
     /// `structuredTypeRelatedToWorker`'s generic-mapped-target arm
     /// (`relater.go:3593`): is `source` related to `{ [P in Q]: T }` or
     /// `{ [P in Q as R]: T }`?
@@ -3348,6 +3378,22 @@ impl Relater<'_, '_, '_> {
             && source_symbol == target_symbol
             && source_arguments.len() == target_arguments.len()
         {
+            // structuredTypeRelatedToWorker's alias arm (relater.go:3389-3392)
+            // probes alias variance only when the SOURCE is an Object or
+            // Conditional type: other aliased types are interned and may or
+            // may not carry their alias. This port mints every generic alias
+            // instantiation as a named reference, so the native flags are the
+            // evaluated body's (`compute_base_constraint` reads the same
+            // body). A union, intersection or primitive body relates
+            // structurally, body to body: `SearchResult<undefined>` to
+            // `SearchResult<string>` with `SearchResult<T> = { value: T |
+            // undefined } | undefined` is decided member by member
+            // (tsr-2zk.927). An unevaluable body keeps the variance road.
+            if let Some((source_body, target_body)) =
+                self.non_object_alias_bodies(source_symbol, &source_arguments, &target_arguments)
+            {
+                return self.is_related_to(source_body, target_body);
+            }
             let measured = self.checker.inference_variances(source_symbol);
             let variances = match measured {
                 Some(variances)
@@ -3998,17 +4044,18 @@ impl Relater<'_, '_, '_> {
             let (Some(target_type), Some(source_type)) = (target_type, source_type) else {
                 // Row 2 of `checker-notes-assign.md` §2, half-answered by §15:
                 // a target property with no source counterpart is fine when
-                // the target property is OPTIONAL — under assignability
-                // always, under the subtype relations only for an
-                // object-literal source (`requireOptionalProperties`,
-                // upstream `propertiesRelatedTo`; interface-backed sources
+                // the target property is OPTIONAL — under the assignable and
+                // comparable relations always, under the subtype relations
+                // only for an object-literal source
+                // (`requireOptionalProperties`, relater.go:4232, which only
+                // the two subtype relations set; interface-backed sources
                 // must still match optionals or subtype reduction loses its
                 // order). Captured mapped modifiers override declaration
                 // optionality; this binder does not write SymbolFlags::OPTIONAL.
                 // Everything else stays row 2's Unknown.
                 if self.checker.get_type_of_property_of_type(source, &name).is_none()
                     && target_metadata.is_some_and(|flags| flags.0)
-                    && (self.relation == Relation::Assignable
+                    && (!matches!(self.relation, Relation::Subtype | Relation::StrictSubtype)
                         || self.checker.is_object_literal_type(source)
                         || self.is_structural_tuple_source(source))
                 {
@@ -4118,8 +4165,11 @@ impl Relater<'_, '_, '_> {
             // `{ p?: number }` is not related to `{ p: any }`, which is what
             // keeps `Contextual | Ellement` un-reduced
             // (`nonContextuallyTypedLogicalOr`, §15.1's two wrong lines).
+            // Comparability passes `skipOptional` (relater.go:4259, tested at
+            // :4318) for every source shape, so `x as { a: T[] }` from
+            // `{ a?: T[] }` overlaps (tsr-2zk.929).
             if target_metadata.is_some_and(|flags| !flags.0)
-                && (source_parts.is_none() || self.relation != Relation::Comparable)
+                && self.relation != Relation::Comparable
             {
                 if source_metadata.is_some_and(|flags| flags.0) {
                     parts.push(RelationResult::NotRelated);
```
