# Rust substitution lifetimes before broader reuse

`tsr-1yb.4.1.2.2` audits frozen Rust
`b466d0304d3ef3e02f5b97f8ea187d3bd75e560a` against native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. The
[receipt](rust-mapper-lifetimes.json) contains current source hashes, lexical
consumers, ordinary CLI observations, private controls and restoration.
Canonical checker code and vendor are unchanged.

## Decision

Do not extend the existing object/signature vector-key caches or replace native
mapper identity with structural equality. A private control proves that print
mode can populate an object result that changes a later semantic request's answer.
`tsr-1yb.4.1.2.3` owns qualification and partition/invalidation of that context.
The counterexample is an internal API observation; a public native-fidelity
reproducer and intended supported answer still need qualification.

The measured consumer for a later bounded handoff is object instantiation in
`inference.rs::instantiate_anonymous_properties` and
`declared.rs::instantiate_type_literal`, coordinated with mapper cost owner
`tsr-1yb.16.1`. Both currently copy ordered pairs for a persistent key. A repeated
same-order request reuses a handle; reversing distinct pairs with the same answers
creates another key and handle. This fixture demonstrates an identity/cost
distinction, not a representative saved-wall ceiling. No representation change
or runtime speedup is retained by this audit.

Historical `ecacd9d0` allocation attribution gives the instantiation-worker origin
282,974,620/205,837,437 requested bytes in default/single mode, including nested
semantic work. It does not locate current key copying, and none of that total is
an avoidable-byte or CPU bound. Consume the existing `.16.1` key/worker attribution
before selecting a representation. The 2× whole-project acceptance remains unmet.

## Current key, owner and publication inventory

All IDs below belong to the current private `Checker`, its TypeStore and binder.
Equal local numbers from different checkers cannot identify equal types. No raw
TypeId or SymbolId can become a process-global semantic key.

| Boundary on this source | Current representation and lifetime | Required native distinction and handoff |
| --- | --- | --- |
| `inference.rs:5132`, `instantiate_type` | Borrowed ordered `(TypeId, TypeId)` pairs plus separate parameter identities/name labels. First exact source wins; direct hits precede lazy signature completion, mentioned-parameter checks and depth/count guards. No active mapper object/stack is represented. | Native active reuse requires the same mapper object and active frame, then a returned completed answer. Structural equality of pairs cannot create that identity. Native depth/refusal ordering also differs from this direct-hit road. |
| `inference.rs:1996/2074`, live contextual/return mapper | Active node-owned inference snapshots resolve vectors from current candidates; consumed parameters become fixed. Call nesting saves/restores or removes active contexts at `:244`. `outer_return_map` is a separately retained first snapshot. | Preserve context identity, fixing/non-fixing state, completed intra-expression sites and invalidation. A borrowed vector is one answer snapshot, not a native callback whose next read may change. |
| `checker.rs:239`, `instantiations`; `declared.rs:5162` reference creation | Private `(SymbolId, Vec<TypeId>)`, with ordered effective arguments. Deferred reference handles can be installed before members complete; reference/display metadata stays in separate tables. | Target-owned persistent identity differs from mapper-pointer active reuse. Handle publication does not prove completed members. Preserve alias/deferred/display and receiver context; do not merge this table with object or alias-body caches. |
| `checker.rs:774`, alias-body evaluations | Private alias symbol and ordered argument vector; error records refusal. Additional active alias bindings and recursion roads affect evaluation. | Alias identity and operation-specific context must survive. A remembered refusal is not a universal completed member image. |
| `checker.rs:727`, instantiated objects; `declared.rs:1747`, type literals | Private `(TypeId, Vec<(TypeId, TypeId)>)`. Type-literal branch reserves a handle before rebuilding members and replaces it with error on refusal. Anonymous-properties branch publishes after rebuilding; mapped branch (`mapped.rs:766`) first publishes error, then replaces it. | Distinguish reserved recursive handles, active error sentinels, completed objects and failure. One tuple shape is not one publication protocol. Parameters/names and `identity_unmapped_type_parameters` are not in this key. |
| `checker.rs:1068`, instantiated signatures; `inference.rs:5439` | Same tuple shape in a different private table. Completes pending original returns before entry, substitutes parts, renders a print clone and publishes after rebuilding. Stored mapper composition maps previous images recursively (`:5533`); later union array-member fallback reads that composition (`union_signatures.rs:232`). | Fresh own parameter identities precede outer pairs; merged direct lookup differs from recursive image composition. Preserve lazy original returns, predicates/this/constraints/defaults and retained composition. Do not treat printed signature text as semantic identity. |
| `inference.rs:2634`, signature-context cache | Explicit source/target declaration, owned parameter IDs, inputs/return/this/predicate/constraint/default types and original inputs. Separate in-progress set declines re-entry; only successful results publish. Changes to strictFunctionTypes clear this cache (`checker.rs:1543`). | Keep operation/context and option epochs explicit. This is not the native active mapper cache and cannot supply its object identity. |
| `inference.rs:5675`, print-clone mode | Temporarily sets `identity_unmapped_type_parameters`, then restores the flag. Existing object/signature keys can persist longer than the operation. | Restoration of a flag does not restore cache contents. Qualify mode partition/bypass/invalidation before broader reuse (`.4.1.2.3`). |

The current instantiation count resets on expression entry. Its increment precedes
the recursive worker's persistent cache lookups; it is neither native per-mapper
execution count nor proof that an expensive object builder ran. The earlier frozen
native inventory described an older count lifetime and must not be relabeled current.

The companion inventory contains 102 lexical `.instantiate_type(...)` sites in
17 Rust files, including tests and one example. Fifteen production modules have
sites; they are not 102 dynamically measured operations. The scanner excludes the
definition but is not a Rust parser, and nearest function labels do not establish
an indirect call graph.

| Consumer modules | Context that must remain visible |
| --- | --- |
| `assignment_declarations`, `members` | Heritage/reference arguments and polymorphic this; member lookup has a distinct declared type and concrete receiver. Its mapper appends this identity separately and omits unchanged ordinary pairs. |
| `calls`, `expressions`, `contextual`, `inference` | Explicit/inferred arguments, fixing state, original lazy returns, contextual callback snapshots and fresh own signature parameters. |
| `constraints` | Newly ported current-source type-argument constraint checks use the effective argument map; this module was absent from the earlier 16-file inventory. |
| `declared`, `mapped` | Ordered dependent defaults, alias/conditional roots, mapped constraints/templates/modifier sources and reserved/error publication. Recursive mapped/conditional roots are not generalized by the public controls. |
| `indexed`, `spreads`, `flow` | Deferred index/operand and generic/narrowing context; immutable printed names cannot replace type identity. |
| `signatures`, `union_signatures` | Signature own/outer parameter distinction, constructor arguments, carried composed maps, callback context and lazy returns. |
| `variances` | Private variance marker identities are an analysis operation, not ordinary user instantiation results. |

## Executable distinctions

The existing [native mapper controls](native-mapper-identity.md) remain the oracle
for exact pointer identity, active push/pop clearing, ordered/duplicate lookup,
merged/composite operations, changing deferred/inference answers and target/root
keys. Their seven private Go tests, mutation/race/package results and qualified
public worker observations are reused with their original hashes and limitations;
they are not claimed as newly executed in this audit.

The archive-only [Rust controls](rust-mapper-lifetimes.rs) exercise eight boundaries:
ordered/distinct pairs and duplicate precedence; same-spelled distinct source IDs
and changed snapshots; direct merge versus recursive image composition; fresh
signature parameters; changing non-fixing versus fixed inference answers; ordered
persistent object keys; print/semantic operation lifetime; and actual retained
signature mapper composition consumed by later instantiation.

For the operation-lifetime counterexample, instantiate `{value:T;other:U}` with
only T mapped. Cold semantic mode refuses. Remove its key, enter print mode and
instantiate successfully; restore the mode flag. The same semantic request now
returns the printed handle. Removing that key restores the original refusal.
This proves missing context on this supported internal road. It does not decide
which native-supported outer U answer the production API should return.

Three isolated mutations are rejected by their intended assertions: compare
printed source names, let the last duplicate source win, and accept candidates
after fixing. The mutation batch used the first seven-control append and restored
that exact source. An eighth stored-composition control was subsequently added;
all eight pass on the final formatted append. Both append versions and all raw
build/test receipts remain archived. Strict release checker library/test Clippy
passes. The initial signature-owner type error and helper-name lint failure are
preserved as setup failures, not semantic outcomes.

The [ordinary public runner](rust-mapper-lifetimes-controls.py) splits the existing
native fixture into eight families, each positive/negative in default/single mode.
All 64 fresh compiler children finish. All eight positive variants match native;
seven negative variants match complete native diagnostics. Nominal private-brand
rejection has the same position and TS2322 header but omits native's nested
private-property detail (`tsr-6.68`). Each tool's full diagnostics, status and
loaded order remain equal across worker modes. The deliberate negatives produce
nine native TS2322 assignments across the eight families, in each mode.

These ordinary children measure observable semantics, not mapper worker counts
or timing improvements. Input snapshots cover fixture/config/compiler bytes;
physical library bytes, all performed work and cross-tool scope equivalence remain
unqualified. Three wrong-native, wrong-source and wrong-Rust-binary guards reject
before compiler dispatch. The original unbound run is preserved separately; its
source/binary identity was checked by the enclosing archive, and the bound rerun
adds explicit guard inputs to the driver without changing fixture semantics.
The build identity binds these recorded artifacts; it is not universal build
attestation for an arbitrary supplied source label.

## Implementation handoff and reproduction

Before `.16.1` or `.4` extends object reuse, require one private immutable
substitution snapshot with ordered identity, duplicate precedence, own/receiver
context, alias/operation mode and completed publication stated explicitly. Keep
live callback/inference contexts outside this snapshot unless their epoch and
invalidation are separately proven. Borrowing a caller's slice is allowed only
within that call; retained keys/composed maps need owned checker-local lifetime.
Do not sort/deduplicate pairs, intern mapper objects by equal answers, share raw
IDs across checkers, or cache provisional/error answers as completed images.
The new context defect must be resolved or explicitly excluded from eligibility.

This audit supplies no current-source avoided-work or saved-wall bound.
The handoff is therefore no-change pending that measurement
and context qualification. Later code still requires full type/diagnostic corpus
with zero prior RIGHT losses, and two independently confirmed ordinary whole-CLI
rounds. The prior rejected spelling optimization remains rejected.

Freeze the named Rust revision in a private archive, attach clean pinned vendor
and build an ordinary release CLI before appending tests. Save its source/binary
build identity. Append the Rust control file to `inference.rs` only there, then run
`cargo test --release --offline -p tsr-checker --lib mapper_lifetime_audit` and
strict library/test Clippy. Run the public driver with that ordinary binary, the
qualified native binary, source SHA, build-identity JSON and a fresh output folder.
Never mutate canonical vendor. Restore the original Rust file after the control
batch and verify it against the frozen Git bytes and ordinary CLI hash.

Production Rust was not edited, so no new full-corpus score is reported. No
full-project native ratio, new cache implementation or 2× completion is claimed.
