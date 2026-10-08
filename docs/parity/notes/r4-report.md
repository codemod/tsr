# r4-report: reportRelationError's missing-property head (tsr-2zk.956)

Lane `tsr-2zk.956`, which covers `tsr-2zk.1.2` and `tsr-2zk.918`. Round 4 closed
before a code commit could pass the gates, so **no code from this lane is
built**. This note records the census and the root causes found, so the next
box can start from them.

Baseline: integration branch `3f34e0da`, vendor `5b1047d`. The diagnostics
dump gave 4372 RIGHT, 1130 WRONG, 4982 EMPTY_RIGHT and 86 EMPTY_WRONG.

## Native mechanism (pinned anchors)

- `reportRelationError` (`relater.go:4751`) handles the missing-property chain
  heads at `relater.go:4816`:
  - It suppresses its own head (TS2322, TS2345 or TS2344) only when the chain's
    next message is TS2741 and `chainArgsMatch(nil, generalizedSourceType,
    targetType)` holds.
  - It does the same for TS2739/TS2740 when `chainArgsMatch(generalizedSourceType,
    targetType)` holds.
  - So the swap needs the missing-property message to name the **same displayed
    source and target strings** as the outer pair.
- `reportErrorResults` (`relater.go:4705`):
  - It displays `originalSource`/`originalTarget` when the type has an alias or
    a single non-augmenting base (`getSingleBaseForNonAugmentingSubtype`,
    `checker.go:28087`).
  - The inner missing-property message names the **normalized** pair
    (`getNormalizedType`, `checker.go:27865`).
  - When the two differ, the TS2322 head stays.
- `reportErrorResults` also returns **without any outer head** for a JSX
  attributes source against an intersection target that contains
  `IntrinsicAttributes` or `IntrinsicClassAttributes` (`relater.go:4722`).
  Only the constituent's own chain is reported, which is TS2741 against
  `IntrinsicAttributes`. This is `tsr-2zk.918`
  (`tsxIntrinsicAttributeErrors` 29:2).

## Census

The census script keeps the WRONG/EMPTY_WRONG rows and takes the multiset
difference of (line, col, code).

- **Only 2739/2740/2741 differ: 37 cases.**
  - 33 of them are *missing* diagnostics, where TSR is silent.
  - In these cases TSR declines before the head is chosen: an incomplete
    table, an unported relation verdict, or a declined caller (constraint,
    iterator, `using`, typeof-module and index-signature subtyping cases).
  - These are **not head-selection bugs**. They belong to the relater and
    member-table lanes.
  - The four *extra* cases:
    - `expandoFunctionExpressionsWithDynamicNames2` ×2
    - `expandoFunctionSymbolProperty`
    - `privateNamesAndStaticFields` (TS2739)
- **Same-position head swaps (the lane proper):**
  - TSR prints TS2741 where native keeps TS2322:
    - `inheritance1` 40:1 and 46:1
    - `classImplementsClass4` 16:1
    - `fuzzy` 21:34
    - `privateNamesUnique-4` 6:7
  - TSR prints TS2322 where native prints TS2741/TS2739:
    - `generic`
    - `chained2` ×2
    - `mappedTypeNotMistakenlyHomomorphic` ×2
    - `consistentAliasVsNonAliasRecordBehavior`
    - `importClause_namespaceImport` ×2
    - `checkJsxChildrenProperty2`
    - `tsxIntrinsicAttributeErrors`
    - `tsxSpreadAttributesResolution16`
    - `tsxSpreadAttributesResolution2` ×2
    - `tsxReactComponentWithDefaultTypeParameter3`
    - `tsxUnionElementType3`
    - `tsxUnionElementType6`
  - TSR prints TS2345 where native prints TS2741/TS2740:
    - `exportDefaultStripsFreshness`
    - `templateStringsArrayTypeRedefinedInES6Mode`

## Root causes found

1. **The complete-table path swaps without `chainArgsMatch`.** This explains
   the 5 extra-TS2741 cases. `report_relation_failure`
   (`crates/tsr-checker/src/assignreport.rs`) first tries
   `missing_required_property`, which only enumerates the two property
   tables.
   - The native chain names the normalized source:
     - `Control` for `ImageBase`/`Image1`;
     - `A` for `C2`'s base;
     - `C` for `this`;
     - `A1` for `A2`.
     Native's chain is e.g. "Property 'select' is missing in type 'Control'
     but required in type 'SelectableControl'". The outer display is
     `ImageBase`, so native keeps TS2322 with that line as an elaboration.
   - `unmatched_property_report` (`relater.rs`) already declines a
     single-base class or interface reference and an alias image for this
     reason. `missing_required_property` has no such gate.
   - Faithful fix: the swap fires only when the source and target that
     `reportErrorResults` displays equal the normalized pair the missing
     property is found on. Otherwise, emit TS2322 with the TS2741 line as a
     chain child.
   - That fix needs `getNormalizedType`'s source for the pair. The
     reporter cannot see it today (bd tsr-2zk.956).
2. **The JSX intersection target never drops the outer head.**
   - `jsx_component.rs:339` calls `report_relation_failure` with the whole
     `IntrinsicAttributes & Props` intersection. TSR's types carry no
     `ObjectFlagsJsxAttributes` bit (see the comment at
     `jsx_component.rs:283`), so the reporter cannot apply
     `relater.go:4722`.
   - Faithful fix: relate each intersection constituent in order
     (`eachTypeRelatedToType`) and report the first failing constituent's
     own head, with TS2741 against `IntrinsicAttributes` and TS6500/TS2728
     related info.
   - The JSX caller must say that the source is JSX attributes. This needs
     a parameter or flag that `jsx_component.rs` would pass, and that file
     is not owned here. Integrator routing needed. The fix would unlock 7
     tsx cases plus `checkJsxChildrenProperty2`.
3. **Alias and incomplete-table declines keep TS2322.**
   - Affected cases:
     - `generic`
     - `chained2`
     - `mappedTypeNotMistakenlyHomomorphic`
     - `consistentAliasVsNonAliasRecordBehavior`
     - `importClause_namespaceImport`
   - Here native's normalized pair *does* match the display, but both TSR
     paths return `None`:
     - `relation_property_table` is incomplete for mapped and alias images;
     - `unmatched_property_report` declines aliases outright.
   - This was not investigated further this round.
4. **The TS2345 call-site cases** (`exportDefaultStripsFreshness`,
   `templateStringsArrayTypeRedefinedInES6Mode`) were not investigated.

## Not done

No measurement of a fix. No perf run. The census script is the one described
in the lane brief.
