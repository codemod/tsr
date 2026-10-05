# Required written-node reuse and permitted preflight

`tsr-1yb.16.3.2.2` owns this bounded contract and archive mutations. Rust source
is `abe7eafe4439a2b679aeb0dd25df4ab4793e7111`, including upstream module-host
changes `9cb129a9`; native remains `5b1047d`. All 55 current-source oracle
children complete with exactly the same fixture inputs, displays and seven
known differences as the frozen `ecacd9d0` report. The canonical
[native protocol](reference-spelling.md) remains the expected display.

## Owner and consumers

`Checker::qualified_written_text` is a private `FxHashMap<NodeId, String>`.
`Checker::new` constructs it empty. Keys identify AST nodes in that checker's
Program; values are owned spellings for later annotation reuse. They are not
semantic TypeId keys or substitution-cache answers. No explicit clear/remove
site exists: the map lives with its private checker, including that worker's
queries and checks, then is dropped. The same numeric NodeId from another
Program cannot reuse this map. Worker maps are not merged or shared.

The current production inventory, excluding comments and test setup, is:

| Site | Operation | Role |
| --- | --- | --- |
| `declared.rs:674` | insert if absent | Written distributed `keyof` compound annotation |
| `declared.rs:4029` | insert if absent | Bare generic reference spelling before arity/default handling |
| `declared.rs:4092` | read/clone | Compose generic spelling from written argument nodes |
| `declared.rs:4104` | insert | Publish that composed reference spelling |
| `declared.rs:4745` | insert | Preserve a shortened bare qualified name |
| `declared.rs:4792` | read/clone | Compose qualified generic written arguments |
| `declared.rs:4807` | insert | Preserve written versus printed qualified reference |
| `signatures.rs:1654` | read/clone | Type-predicate annotation spelling |
| `signatures.rs:2355` | insert | Reuse an otherwise unresolvable return annotation |
| `signatures.rs:5073` | read/clone | General `written_annotation_text` consumer, for any annotation node |

`signatures.rs:5791` is test setup rather than another production producer.
Line anchors above refer to the pinned Rust source; function names and the
recorded file hashes identify the branch when later ports move them.

Both bare and composed spelling matter. Function parameters can retain `Box`
or `Pair<string>` while their computed return types are `Box<number>` or
`Pair<string, string>`. Nested `Outer<Box>` must compose from the argument's
written `Box`, not its computed default-filled display. Dropping a whole
written-text channel to remove copies changes these outputs.

## Exact safe decision boundary

The prospective change is confined to the spelling loop in
`get_instantiated_type_reference`. It may decide whether to allocate spelling
**after all explicit arguments are resolved successfully**, using the existing
condition: a partially written list, or at least one argument NodeId already
present in `qualified_written_text`.

That is the same predicate as the current loop's `any_written ||
partially_written`:

1. Each explicit argument produces exactly one resolved TypeId, or returns
   error before the spelling loop. The zip therefore covers the same arguments
   as a membership preflight.
2. `get_type_from_type_node` may publish an argument's written spelling. The
   preflight must follow those calls. It cannot be moved to the arity check or
   used to skip resolution.
3. During the spelling loop, the map is only read. `Some("")` still counts as
   written; checking membership preserves that behavior.
4. `Checker::type_to_string(&self)` delegates to
   `printing::type_to_string(&Type)`. That complete match formats intrinsic or
   literal data or clones precomputed `Named`, `Anonymous`, union/intersection
   text. It has no checker/store callback or member-resolution path. It is not
   contextual `type_to_string_at(&mut self)` or the native NodeBuilder.
5. `entity_name_text` reads immutable identifier/qualified-name AST text.
   Building its owned base string can follow the retention preflight too.
6. The retained vector, join and composed insertion must be unchanged. Bare
   reference registration, arity/error behavior and subsequent ordered
   dependent-default substitution remain in place.

This permits avoiding discarded formatting in **this branch only**. It does
not authorize skipping native rendering, changing another qualified-name
producer, reusing a mapper, caching name projections, or removing semantic
argument/default work. Native printing demonstrably instantiates types; its
zero counters would not prove the absence of other lazy work anyway.

## Mutation proof

[The runner](reference-spelling-mutations.py) rejects the canonical checkout,
wrong archive source/target, wrong source SHA, dirty tracked Rust source, a
different native substrate, stale helper/binary, and non-unique source anchors.
The archive's vendor gitlink is supplied by a symlink to the verified native
pin; it is the only permitted tracked setup difference.

It executes two source mutations, separately, without modifying canonical
checker files:

- `drop-reference-writes` discards the bare and composed reference spellings.
  The controls require new native mismatches for bare, nested, partial and
  dependent function parameters.
- `expand-written-bare` changes bare written text to a default-expanded form.
  The controls require new mismatches for bare and nested parameters.

Every comparison starts from a probe whose baseline already matches native.
Existing conditional/qualified gaps cannot make a mutation appear effective.
All fixture unit bytes and both Rust query orders are verified. Complete raw
children and changed outputs are retained. These deliberately wrong variants
are never production candidates; they demonstrate that the display controls
protect the retained predicate.

The [qualified receipt](reference-spelling-reuse.json) records seven new native
mismatches from dropped spelling and five from expanded bare spelling. Both
mutation builds and all 22 fixture children complete. The first locating batch
restored source but left the target at its last mutant; its raw receipt is
retained separately. The final runner fixes that cleanup boundary and verifies
restoration in the same run.

The runner restores the exact source bytes even on failure, rebuilds the
baseline, and requires its binary hash to match the original. It does not
overwrite Cargo's hardlinked artifacts with an unrelated executable. Any
timeout, build failure or ineffective control leaves the receipt incomplete.

## Public diagnostics and reproduction

[The public driver](reference-spelling-public.py) materializes the exact parser
unit bytes and retains complete ordinary TSR/native CLI output in default and
single-threaded modes. Across 11 families, ten diagnostic populations match;
qualified defaults retain two TSR TS2322 errors versus native acceptance
(`tsr-6.66`). Both worker modes preserve each compiler's own output and loaded
file listing. This does not establish equivalent cross-tool lazy work.

The native executable is the original unmodified `native-off` binary, built
**before** the lifecycle observation patch; its SHA is independently bound to
[that source-qualified receipt](native-worker-activity.json). It is not the
`native-probe` executable. Two attempted new native CLI builds failed for disk
space, with terminal logs retained. Only the task-owned disposable Go cache was
removed; sources, measured binaries and receipts were kept. Reusing the verified
original artifact avoids changing the oracle or compiler behavior.

Run these only with the fresh archive/source and build receipts described in
the [oracle reproduction](reference-spelling.md#reproduction):

```sh
python3 docs/architecture/reference-spelling-mutations.py \
  --source /absolute/archive/current-source \
  --target /absolute/archive/current-target \
  --oracle /absolute/archive/oracle-current/results.json \
  --directory /absolute/archive/new-mutation-run
python3 docs/architecture/reference-spelling-public.py \
  --normal /absolute/archive/current-target/release/tsr \
  --native /absolute/archive/native-off \
  --oracle /absolute/archive/oracle-current/results.json \
  --directory /absolute/archive/new-public-run
```

The handoff is `tsr-1yb.16.3.3`: make only the measured retention preflight,
preserve these controls and full previously RIGHT type/diagnostic rows, then
compare ordinary full-CLI default/single runs in two independent paired rounds.
The old allocation byte counts are locating evidence, not current saved wall.
`tsr-6.23.1`, `tsr-6.67` and `tsr-6.66` remain explicit fidelity gaps. Neither
this contract nor a local improvement proves the full comparable wall ratio
<=0.50.
