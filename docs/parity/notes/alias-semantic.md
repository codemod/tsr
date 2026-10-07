# Alias semantic recovery — tsr-2zk.16.2

Native pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Recovery checkpoint: `0e7824ddb2f06af1dd4cbbe9779e0a17b720a0f3`.

## Implemented boundary

`getDeclaredTypeOfTypeAlias` resolves a generic alias's body, not an opaque
nominal object. `getTypeFromLiteralTypeNode` returns the existing regular literal;
`instantiateTypeWithAlias` leaves it unchanged. `declared.rs` now preserves that
identity for literal bodies, including parenthesized bodies. Parameter-bodied
aliases use the same parenthesis-transparent body projection at declaration and
reference sites. Explicit unions retain their existing alias construction.

- **Identity/owner:** existing Checker-owned `declared_types`, keyed by binder
  `SymbolId`; existing `instantiations`, keyed by symbol and ordered `TypeId`
  arguments. No printed-name keys, new cache, shared fields, or symbol-domain
  conversion.
- **Publication:** literal declarations use the existing DeclaredType resolution
  push/pop and circularity reporting. References force the existing declared
  owner before publishing the unchanged literal into the instantiation map.
  No provisional literal or recursive assumption becomes a completed result.
- **Receiver/alias context:** literal and parameter types are pre-existing native
  types; these operations do not attach the enclosing alias. No member receiver
  or written-reference presentation path changes.
- **Work boundary:** the existing body-node resolver runs under declared-type
  completion; cache reuse is unchanged. No new traversal or broad body evaluation
  is introduced. Expensive-work attribution remains `tsr-1yb.11`.

## Observed verification

Receipts are in ignored `target/recovery/alias/`, not `/tmp`.

- `alias_semantic_literals`: two failures before, three passes after. Covers
  string, signed numeric, boolean, null, bigint literal bodies, parenthesized
  parameter identity, and the distinct explicit-union presentation case.
- Direct pinned-native CLI control: literal assignments accepted, incompatible
  literal rejected with TS2322. TSR previously emitted two extra assignment
  errors; after, `native-focused.txt` and `after-focused.txt` compare byte-for-byte.
- 40 existing tests passed across `aliases`, `alias_naming`,
  `conditional_alias_members`, `interned_type_identity`, `variadic_tuple_alias`.
- Existing unfiltered checker-types case oracle: 8,051 passed, 1,487 failed,
  2,906 skipped, unchanged. Before/after raw verdict dumps compare byte-for-byte:
  summary 477,970 assertions, 469,785 RIGHT, 995 GAP, 7,190 WRONG. Thus no RIGHT
  losses or vanished keys in this oracle. These are not full-configuration totals.
- Anchor gate: 4,501 upstream references checked, zero unresolved.
- Fresh-process synthetic 3,000-variable CLI smoke, seven samples: TSR median
  before 0.263191 s, after 0.241101 s, native 0.291136 s. After/before 0.9161;
  after/native 0.8281. The baseline emits false positives and does different
  diagnostic work: **not equivalent complete work, not a speed claim**.

## Unmet campaign prerequisites

This Box clone contains only `box/recover-alias`; `box/parity-alias` and unreachable
recovery commits are absent. No saved-branch change was recovered or claimed
verified. Supply that source to resume history-based recovery.

The broader alias target/alias presentation infrastructure belongs to the
integration and symbols owners. An unchanged control still rejects assignment
from `Choice<string>` to `"a" | "b"` although native accepts it; declaration and
reference presentation alone are not semantic target publication. No shared API
was introduced in this slice. Generic recursive bodies, enclosing aliases,
imported written arguments, and general instantiation publication remain open
under the existing issue.

Full-configuration >=99.9% parity, preservation against disappeared historical
RIGHT-key receipts, and verified equivalent-complete-work median <=0.50 remain
uncertified. Existing oracle skips cannot certify those gates.
