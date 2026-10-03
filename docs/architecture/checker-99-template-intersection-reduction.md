# Template and string-mapping intersection reduction

Pinned tsgo: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Baseline: `8f8f4e1a`, 458,022/478,855 matching assertions.
Unit: tsr-6.11.

## The forcing constraint

Semantic template literal and intrinsic string-mapping types already exist, but
the intersection factory still followed the older assumption that only an
ordinary string literal could make `string` redundant. Consequently
`` string & `${number}` `` retained the alias `T1`, and matching/disjoint mapping
intersections such as `"prop" & Lowercase<string>` could not normalize. The
pinned implementation handles these in `getIntersectionTypeEx`
(`checker.go:26056`), `removeRedundantSupertypes` (`:26295`) and
`extractRedundantTemplateLiterals` (`:26317`).

## Native order and retained structure

Extraction runs after disjoint-domain detection and before `any` recovery. For
each template or mapping constituent, it tests every string-literal constituent
with the subtype relation. A matching literal removes the broader template or
mapping; a failed match against a pattern proves the intersection empty. A
non-pattern mapping that cannot be decided remains intact. This ordering is
observable: replacing it with printed-string matching would confuse
`` "prop" & `p${Lowercase<string>}p` `` (the literal survives) with
`` "setX" & `get${string}` `` (the answer is `never`).

`removeRedundantSupertypes` also treats template and mapping flags as string-like,
so `` string & `${number}` `` becomes `` `${number}` ``. Native then returns a singleton
regardless of the written intersection alias. This port limits that newly
observable alias elision to intersections whose original flags include a
template or string mapping; general alias identity and empty-object reduction
remain unchanged.

The rejected alternative was to normalize template-hole intersections in the
template factory itself. That loses the intersection's literal constituents,
which are exactly the evidence extraction needs, and would affect templates
outside intersection construction. It would become preferable only if upstream
moves this reduction into `getTemplateLiteralType`.

## Verification and limits

The asymmetric controls in
`crates/tsr-checker/tests/template_intersection_reduction.rs` cover primitive
supertype removal, matching template/mapping literals, disjoint patterns, and a
generic hole that must stay structured. They distinguish every branch using
the same shapes as pinned native controls `numericStringLiteralTypes.ts` and
`stringMappingReduction.ts`.

Full scorepair against the same-checkout `8f8f4e1a` baseline moves from 458,022
to 458,030 RIGHT: eight WRONG→RIGHT (six `stringMappingReduction`, two
`numericStringLiteralTypes`), zero RIGHT losses, zero GAP transitions, and zero
changed-WRONG payloads. Aligned GAP stays 2,445; WRONG falls from 13,777 to
13,769. The exact local commit is reported with the worker bundle.

This slice does not change general empty-object identity, callback inference,
template alias display, or serialization. Generic alias retention such as
`` string & `${T}` `` printing `T3<T>` remains a separate naming/identity question.
