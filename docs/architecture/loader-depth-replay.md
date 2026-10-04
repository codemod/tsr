# Native loader depth and replay controls

Source `30d5e218707cad0bc071ec143248bc0078452f9a`; native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. This audit advances
`tsr-1yb.19.1` and records prerequisites for production loader concurrency.
It selects no production change and makes no timing claim. Dynamic read
completion, casing, package redirects, augmentations and global merging still
need the broader controls described by that task. The whole-project 0.50
wall-ratio target remains unverified.

## Confirmed missing files and diagnostic

The public fixture is a chain:

```text
main.ts -> outer/index.js -> inner/index.js -> inner/leaf.js -> inner/typed.ts
```

Both packages live in `node_modules`; `typed.ts` assigns a string to a number.
With `maxNodeModuleJsDepth: 1` and roots `[main.ts, inner/index.js]`, TSR
loads only `outer/index.js` and `main.ts`. Both native worker modes load the
explicit inner root and its descendants, reporting TS2322 in `typed.ts`.
TSR reports zero diagnostics. Repeating the inner root preserves this gap.
An explicit root must remain reachable after an earlier depth elision.

The following table excludes the independent `globals.d.ts` fixture root.
`outer`, `inner`, `leaf` and `typed` denote the corresponding files above.
Native results repeat ten times per mode in fresh processes.

| Roots, depth | TSR loaded files in collection order | Native single-threaded | Native default | Diagnostics |
| --- | --- | --- | --- | --- |
| main, inner; 1 | outer, main | typed, leaf, inner, outer, main | same as single | TSR 0; native TS2322 |
| inner, main; 1 | typed, leaf, inner, outer, main | same as TSR | same as TSR | all TS2322 |
| main, outer; 1 | outer, main | inner, outer, main | same as single | all 0 |
| outer, main; 1 | inner, outer, main | outer, main | inner, outer, main | all 0 |
| main; 1 | outer, main | same as TSR | same as TSR | all 0 |
| main; 2 | inner, outer, main | same as TSR | same as TSR | all 0 |
| main; 3 | typed, leaf, inner, outer, main | same as TSR | same as TSR | all TS2322 |

The later-parent-root case has equal empty diagnostics while TSR omits a
file native includes. Diagnostics alone therefore cannot prove equal work.
The reverse-parent-root case exposes a second problem: native's two modes
load different files. Neither of these cases justifies weakening the
whole-project work-equivalence gate.

`tsr-1yb.19.1.1` owns the Rust depth/identity defect.
`tsr-1yb.19.1.2` owns the mode-specific native replay oracle. Both directly
block production `tsr-1yb.5`; the parent replay proof stays unfinished.

## Why the task lifecycle matters

Pinned `internal/compiler/filesparser.go:245` registers each path before
queueing work. Canonical `parseTaskData` retains tasks by original file-name
casing, a minimum observed depth, a first nonempty package ID and expansion
state. A duplicate task with the same spelling receives a `loadedTask`
alias. Collection resolves that alias before testing whether the file was
loaded (`filesparser.go:369`). The first collection encounter determines
the retained package instance before descendants are collected.

Rust `process_task` (`crates/tsr-compiler/src/loader.rs:638`) claims paths
during its depth-first walk, before the depth-elision check. It immediately
returns on every later claim. In the first control, the deep inner import
claims the path without loading it, and the later explicit root is discarded.
Root tasks exist before the walk, but their identity has not been registered
as native does. Moving the claim after the elision check might repair this
one example; it would not establish the native alias, casing, minimum-depth,
package-identity or already-expanded-parent contract.

Native `internal/core/workgroup.go:78` pops its single-threaded work queue
from the **end**. With roots `[main, outer]`, it executes the outer root at
depth zero before main. Its inner dependency is allowed at depth one.
With roots `[outer, main]`, main executes first and reaches outer at depth
one. Inner is elided at depth two. When the outer root subsequently arrives
at depth zero, the existing per-task `startedSubTasks` guard prevents
starting that already-expanded subtree again. The comment about lower-depth
reprocessing does not mean unconditional recursive retry. The default
goroutine work group produced the other work set in all ten observed runs;
this is an observation, not a guarantee for every schedule.

Collection walks the original task tree, resolving aliases and redirects;
it emits each retained task's type-resolution traces followed by its module
traces, and appends source files after their descendants. Consequently,
`--listFiles` is a collection order, not parse order or a complete read plan.
A coordinator can own native-equivalent identity and replay state without
sharing the arena or copying native mutexes into a read worker. That state
must be proved before any worker completion can affect discovery or parsing.

## Trace evidence is currently incomplete in the Rust CLI

All thirteen native controls emit nonempty requested resolution traces.
The native modes have different ordered trace contents in three cases,
including two with equal loaded identities. Buffered delivery order cannot
by itself guarantee equal cache-aware messages: the resolver also observes
which package lookups other task work has warmed.

TSR emits **zero** requested trace lines in every control. Its loader stores
them, but `Program::from_root_files` does not retain `LoadedFiles.traces`,
and `run_compilation` does not forward them. Existing bug `tsr-1yb.1.1.1`
now has P1 priority and blocks the replay proof. This audit records empty
traces as a failure; it supplies no claim that Rust internal trace buffers
match native. Under `noResolve`, native still resolves and traces imports;
it suppresses adding their files. The explicit-root control asserts the
loaded-file behavior without assuming an empty native trace.

## Reproduction and validation

[loader-depth-controls.py](loader-depth-controls.py) contains all public
fixture bytes. It resolves the output directory to a physical path before
creating roots. On macOS, mixing `/tmp` roots with `/private/tmp` package
realpaths creates two different canonical keys and can hide the defect.
Loaded identities remain logical within that physical fixture; the script
does not collapse symlinks or remove unexpected output from comparisons.

```sh
python3 docs/architecture/loader-depth-controls.py \
  --tsr /absolute/path/to/tsr \
  --tsgo /absolute/path/to/pinned-tsgo \
  --tsr-source /absolute/path/to/tsr-checkout \
  --native-source /absolute/path/to/typescript-go-checkout \
  --output /tmp/new-loader-depth-controls \
  --native-repeats 10
```

The source paths must describe the binaries supplied. The audited TSR CLI
was built from an isolated exact-source checkout with
`CARGO_BUILD_JOBS=1 cargo build --release --locked --offline --bin tsr`.
The native source files match the original clean-oracle manifest. The driver
clears inherited `TSR_*` overrides and supplies an explicit pinned library
path, avoiding dependence on the lifetime of a build worktree.

The complete run covers thirteen cases: later root, earlier root, later
parent root, earlier parent root, no recovery root, outer root only, raised
depths two/three, zero depth, duplicate inner/main roots, relative JavaScript
outside `node_modules`, and `noResolve`. The main matrix uses `noLib` and a
minimal explicit global prelude; `types: []` excludes ambient package inputs.
Every supplied input/config hash is unchanged after its case. All 273
processes finish normally. The driver saves every raw stdout/stderr and
compares full multiline diagnostics, ordered loaded files, loaded sets and
ordered trace contents separately. It deliberately exits 1 for observed
mismatches; `results.json` still has `complete: true`. This matrix documents
the oracle differences to resolve, rather than serving as a passing CI gate.

Two additional physical fixtures were repeated using default libraries.
Each role loaded the same 51 pinned libraries, and both missing-work findings
persisted; the first retained the native TS2322/TSR-silence difference. These
six confirmations use the freshly built source binary and an explicit
library path. Preliminary scratch runs with missing prelude types or a
relocated binary's absent library fallback are excluded from the final
matrix and its acceptance evidence.

Driver syntax, multiline diagnostic preservation, an appended extra loaded
file, and unexpected extra output were checked. The latter two change the
comparison images instead of silently disappearing. No production code
changed; full checker and diagnostic corpora were not run for these
diagnostic-only artifacts. A retained loader fix still requires those gates.

[loader-depth-replay.json](loader-depth-replay.json) records exact source and
binary hashes, fixture/config hashes, distinct observed images, per-role raw
stdout hashes, trace counts/fingerprints, default-library confirmations and
validation. Full transcripts remain under
`/tmp/tsr-loader-depth-main-controls-final`; the script regenerates them.
Only the project path is normalized within traces, so ancestor-directory
spellings remain observable. These controls have no reordered read-worker
execution and do not close `tsr-1yb.19.1`.
