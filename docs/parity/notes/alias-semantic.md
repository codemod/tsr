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

The saved source was initially absent. After the parent published it, explicit
fetch supplied `origin/box/parity-alias`. Its two commits after the recovery
checkpoint (`16f9a7aa`, `6e099017`) are uncommitted snapshots, not verified commits;
neither was cherry-picked. The owned hunks were reviewed against native and
re-derived, with current verification below.

The broader alias target/alias presentation infrastructure belongs to the
integration and symbols owners. An unchanged control still rejects assignment
from `Choice<string>` to `"a" | "b"` although native accepts it; declaration and
reference presentation alone are not semantic target publication. No shared API
was introduced in this slice. Generic recursive bodies, enclosing aliases,
imported written arguments, and general instantiation publication remain open
under the existing issue.

## Recovered alias-free instantiation

The recovered owned hunks complete `keyof` and argument-free `typeof` alias bodies
through the existing `instantiate_type` API. Native `getTypeAliasInstantiation`
completes the declared owner before cache lookup and constructs a mapper from
ordered local parameters to arguments; `instantiateTypeWorker` maps an index type
through `getIndexType`. Successful declared resolution seeds the own-parameter
instantiation. Templates retain their existing worker, and queries with written
type arguments remain outside this route. Written alias annotations reuse the
existing `qualified_written_text` channel, not semantic cache keys.

Identity/owner and publication follow the existing maps above. `instantiate_type`
owns the expensive walk on misses; no duplicate cache or active-mapper result is
introduced. Concrete value identity is retained for `typeof`; there is no new
receiver/alias-bearing image. Shared hunk/API requirement: **none**. General
alias-bearing mapper/presentation publication requires integration/symbol owners;
`typeof f<T>` requires the calls/expression owner's instantiation-expression
worker before this route can extend to it.

Current recovery evidence, replacing historical saved-branch claims:

- New `alias_semantic_declared_body` fails before and passes after: cold reverse
  argument order, distinct object-key arguments, warm reuse, actual value identity.
  All 44 targeted tests pass (including the preceding literal controls).
- Direct native-supported assignments remove all five false positives. A
  single-literal rejection control compares byte-for-byte with native. The
  union rejection control still omits native's nested TS2322 detail; this
  diagnostics-owner prerequisite is explicitly not exact parity.
- Unfiltered verdict oracle against checkpoint: 11 WRONG→RIGHT, zero RIGHT
  losses, zero vanished recognized keys. Summary: 477,970 assertions, 469,796
  RIGHT, 995 GAP, 7,179 WRONG. Case totals remain 8,051 passed / 1,487 failed /
  2,906 skipped. Gains: two `recursiveTypeRelations`, one `keyofIntersection`,
  eight `mappedTypeAsClauses`. Other already-WRONG rows change without becoming
  exact; no broad success is claimed.
- Anchor gate: 4,501 checked, zero unresolved.
- Seven-sample interleaved fresh-process 3,000-reference `keyof` smoke: baseline
  median 0.133485 s, recovered 0.137989 s, native 0.128580 s. After/before **1.0337**;
  after/native **1.0732**. All exit zero with empty output, but the baseline skips
  semantic mapping: equivalent complete work is unverified. The measured smoke
  is 3.37% slower; **no hotpath/no-slowdown gate or speed target is certified**.
  Receipts: `recovered-perf.txt`, `recovered-transitions.txt`,
  `recovered-target-tests.txt`, `semantic-focused-*.txt`.

## Callable declaration publication — tsr-2zk.16.2.1

The parent reported the pre-existing user test failure: F6 expected, `error`
actual for `type F6 = ({ a: string }) => typeof string`. That reported failure was
not rerun; the user test was neither changed nor committed.

Native `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode` creates the
binder-owned anonymous object with its enclosing alias before signature member
resolution. The existing function-types worker instead discarded its reservation
when signature preparation returned unsupported. Owned `declared.rs` now publishes
non-generic alias-bearing call/construct objects first, through the existing
`type_literal_key` and `type_literal_types` owner. The helper represents the native
object-publication boundary, not a function-name or failed-body heuristic.
Supported signature preparation populates the existing signature vector for
current eager consumers, but failure does not retract the completed object.
Absence of a vector is not published as an empty signature set; demand consumers
can still resolve the actual binder owner.

- Identity: contextual node key, binder `__type` owner, private Checker lifetime.
- Publication: object completes before optional signature metadata; unsupported
  signature preparation is not object failure or completed empty signatures.
- Context: existing non-generic alias naming eligibility and contextual key;
  no printed-name key, receiver conversion, or new semantic table.
- Work: existing signature worker, no duplicate body traversal/cache. General lazy
  signature consumer cutover remains integration-owned; no new shared API required.
- Independent regression covers malformed/uncomputable F6, constructor alias, and
  supported callable publication. Existing targeted function/alias tests pass.
- Direct native CLI control matches complete diagnostics byte-for-byte: TS7031
  binding element error and TS2322 call-result error. No suppression.
- Unfiltered oracle versus `215d1b31`: 16 WRONG→RIGHT, 6 GAP→RIGHT, zero RIGHT losses
  and vanished recognized keys. Summary 477,970 assertions: 469,818 RIGHT,
  989 GAP, 7,163 WRONG. An initial fully lazy experiment caused 85 RIGHT losses;
  it was rejected, not committed. Supported signature metadata remains prepared
  until direct-vector consumers are migrated together.
- Receipts: `lazy-tests.txt`, `lazy-native.txt`, `lazy-after.txt`,
  `lazy-transitions.txt`, `verdict-lazy.tsv`. No full-workspace gate was rerun.

Full-configuration >=99.9% parity, preservation against disappeared historical
RIGHT-key receipts, and verified equivalent-complete-work median <=0.50 remain
uncertified. Existing oracle skips cannot certify those gates.
