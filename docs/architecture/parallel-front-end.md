# Parallel front end

Source baseline: TSR `6d55ae54c2ca1062efdb16ceb2181b3c6a785591`;
native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Work is tracked by `tsr-1yb.5`, `tsr-1yb.5.1.1`, `tsr-1yb.5.1.2`,
`tsr-1yb.22.1` and `tsr-1yb.22.2`.

## Experiment contract (CP0)

Target the actual CLI's parsing and binding, keeping loaded files, checked files,
source order, diagnostics and checker results unchanged. Compare serial, two
workers and default workers on the same source and input. Measure fresh-process
wall time, CPU, RSS and extended diagnostic phase times. Keep first-launch
security delays separate, retain their rows, and run warmed comparisons in
alternating order. Accept a production default only after whole-command benefit;
the independent release target remains TSR/native median wall ratio <=0.50 on
equivalent complete work, without previously RIGHT losses.

## Ownership inventory

Native `compiler/filesparser.go` queues independent file parse work; native
`Program.BindSourceFiles` queues independent file binds. Rust currently parses
into a caller-owned arena and binds into a program-wide symbol/flow store.
`Arena` is Send and not Sync; worker allocation cannot use that shared arena.

AST node and token `node_id` fields are immutable `Option<NodeId>` after
registration. Generated alias `node_id()` and `HasNodeId` expose those fields;
direct reads remain legitimate. IDs also live in NodeTable parent rows, NodeMap
indices, JSDoc host entries, binder declarations/locals/facts/computed-name links
and diagnostic consumers. Speculation truncates registration tables, so only
finished reachable registration rows may be published. JSDoc nodes are included
even when outside the SourceFile syntax walk. Spans and diagnostic offsets are
file-relative and must not be shifted.

The bounded parser candidate retains the existing field representation. Workers
own ParsedFile cells, private arenas and zero-based tables. Publication copies
syntax into the caller arena with an ordered node base, remaps every child and
JSDoc edge, and appends all table rows. Source slices borrow the canonical copy;
decoded strings are copied separately. States are private, finalizing, then
published immutable. No worker node or borrowed worker string escapes. Compared
with an accessor/atomic slot migration, this leaves all direct readers unchanged
and requires no new unsafe lifetime or allocator sharing. Its extra allocation,
copy time and transient double AST storage must be measured before adoption.

Binding must defer global merging until ordered publication. Symbol records
carry declaration/value NodeIds and parent/export/member SymbolIds. BindResult
also carries node-symbol columns, locals, globals/UMD exports, redirects,
conflicts, module and pattern augmentations, undefined, computed names, node-flow
columns, facts and end/return/fallthrough flow maps. FlowStore carries record
antecedents, label-list cells, switch-clause nodes and reduced label targets/list
heads; its overloaded auxiliary field must be relocated by flow kind. The
unreachable FlowId zero remains canonical. Symbol-store identity stays owned by
the coordinator; private stores cannot manufacture published checker handles.
Nil versus allocated-empty member/export tables must survive publication.

File-local binding does not read earlier globals. The coordinator must append
symbols in file order, replay script/UMD/global-augmentation merges, and synthesize
undefined at the same first-file boundary. Non-global module augmentations remain
the existing final program pass. Receiver and alias context remain the original
AST and published symbol links; no checker-owned type/member cache is shared.

Controls must exercise reversed completion, empty/skewed/malformed files, JSDoc,
TS/TSX/JS/JSON, duplicate globals, augmentations, aliases/cycles and branching flow.
Wrong-base and wrong-merge-order mutations must be rejected. Counts, identity
edges and complete diagnostics are correctness prerequisites, not speed wins.
