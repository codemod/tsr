# CLI status and plan

**The live dashboard for the `tsr` binary and everything behind it.**

[STATUS.md](STATUS.md) is the project's state and wins on any disagreement about
compiler numbers. This file is the same discipline applied to one workstream:
what the CLI *is*, what it can be measured against, what is built, and what has
been refused with the number that refused it. [PLAN.md](PLAN.md) §Phase 6 is the
one-paragraph version; this is the detail under it.

The rules are STATUS.md's rules and are not restated: every number carries the
commit it was measured at, a refused item stays on the page, corrections are made
in place and said to have been made.

**Read §2 before planning anything.** The CLI has a mechanical oracle of exactly
the kind the rest of the project is steered by, and it changes what "build the
CLI" means.

---

## 1. Where the CLI stands

Measured at `fe1efe7`, 2026-08-08.

**There is no binary.** The workspace's only `[[bin]]` is
`tsr-conformance`'s `coverage`. Nothing can be pointed at a repository, and
`cargo run -p tsr-execute` does not resolve because the crate does not exist.

What the fourteenth session landed is the *seam*: the pieces a driver calls, each
with an upstream anchor and a test, and no caller sequencing them.

| the driver needs | state | where |
|---|---|---|
| a real filesystem | **done** | `tsr_vfs::OsFileSystem` (`internal/vfs/osvfs`) |
| `tsconfig.json` → options | **done** | `tsr_tsoptions::parse_config_file` |
| `include`/`exclude`/`files` expansion | **done** | `tsr_tsoptions::file_names::expand` |
| program assembly from roots | **done** | `Program::from_root_files` |
| the bundled `lib.*.d.ts` | **on disk** | `vendor/typescript-go/internal/bundled/libs`, reachable through `LoadOptions::default_library_path` |
| checker configuration | **done** | `Checker::apply_compiler_options` ([ADR-0042](docs/adr/0042-checker-options-come-from-compiler-options.md)) |
| plain diagnostic rendering | **done** | `tsr_diagnostics::format`, byte-exact |
| **a `ResolutionHost` over the OS** | **missing** | every impl in the tree is in a test, an example, or the harness |
| **a command-line parser** | **missing** | §3 phase 1 |
| **a `System`** (writer, cwd, TTY, terminal width, env, clock) | **missing** | upstream's `tsc.System`, `internal/execute/tsc/compile.go:17` |
| **pretty diagnostic rendering** | **missing, deliberately** | §6 |
| **emit** | **missing** | no `tsr-transformers`; `tsr-dts` is declaration-shape only |
| watch, `--build`, incremental | **missing** | §3 phases 6–8 |

### The option surface, which is the number that matters most

| | upstream | here | share |
|---|---:|---:|---:|
| `CompilerOptions` fields | 130 | 50 | 38% |
| declared options (`declscompiler.go` / `declarations.rs`) | 135 | 48 | 36% |

Counted at `fe1efe7` by `awk '/^type CompilerOptions struct/,/^}/' | grep -cE '^\t[A-Z]'`
and `grep -c 'name: "'`. Ten of the 48 arrived this session with ADR-0042.

Everything not in the table reports as an unknown option. That is survivable for
a checking-only driver run by its own authors and is a **hard blocker for the
oracle in §2**, because a baseline whose input writes `--declaration` gets an
unknown-option error instead of a compilation.

---

## 2. The oracle — the CLI is measurable, and that is the whole plan

This is the finding that should shape the work. `tsc`'s command-line behaviour is
not something to be judged by trying it and looking at the output. Upstream ships
a baseline suite of the same mechanical kind as `conformance` and
`printer_round_trip`, and it asserts **exact stdout bytes and exit status**.

| suite | files | what it is |
|---|---:|---|
| `testdata/baselines/reference/tsc` | **194** | one compilation each: virtual FS, argv, exit status, output, resulting files |
| `testdata/baselines/reference/tscWatch` | **41** | the same, driven through edit cycles |
| `testdata/baselines/reference/config` | **80** | `tsconfig.json` parsing in isolation |
| `internal/tsoptions/commandlineparser_test.go` | 572 LOC | the parser's own unit table |
| `internal/tsoptions/tsconfigparsing_test.go` | 1,581 LOC | config parsing's unit table |
| `internal/execute/tsctests` | 12,504 LOC | the runner producing all of the above |

A `tsc` baseline looks like this — `tsc/commandLine/Initialized-TSConfig-with---help.js`:

```
currentDirectory::/home/src/workspaces/project
useCaseSensitiveFileNames::true
Input::

tsgo
ExitStatus:: DiagnosticsPresent_OutputsSkipped
Output::
Version FakeTSVersion
tsc: The TypeScript Compiler - Version FakeTSVersion

<ESC>[1mCOMMON COMMANDS<ESC>[22m
...
```

Four consequences follow, and each of them is a decision the plan has to respect:

1. **The help text is asserted, with its ANSI codes.** `internal/execute/tsc/help.go`
   is 15.9 KB of layout. It is not chrome to be added at the end; it is a
   baselined artifact that several dozen cases print.
2. **Pretty output is asserted.** The escape sequences above are in the expected
   bytes. §6's refusal to approximate the pretty path is therefore a refusal to
   *guess* it, not permission to skip it forever.
3. **Exit status is asserted by name**, from a six-value enum
   (`internal/execute/tsc/compile.go:30-39`): `Success` 0,
   `DiagnosticsPresent_OutputsSkipped` 1, `DiagnosticsPresent_OutputsGenerated`
   2, `InvalidProject_OutputsSkipped` 3, `ProjectReferenceCycle_OutputsSkipped`
   4, `NotImplemented` 5. Getting 1 vs 2 right requires knowing whether outputs
   were written, which means the emit boundary is visible in the *exit code* even
   under `--noEmit`.
4. **The compiler's own correctness is mostly not what is being tested.** Most
   cases compile a handful of trivial files. A CLI suite can therefore ratchet
   while `checker_types` sits at 41%, which is the opposite of the assumption in
   PLAN.md's Phase 6 gate ("upstream `tsc` CLI baselines pass") being placed
   after the checker.

### The ceiling: how much of it is reachable without emit

**Measured at `fe1efe7`**: 144 of the 194 `tsc` baselines contain an emitted
artifact marker, and **50 do not**.

```
grep -rlE '^//// \[.*\.(js|d\.ts|tsbuildinfo)\]' testdata/baselines/reference/tsc | wc -l
```

| directory | emit-free | total |
|---|---:|---:|
| `showConfig` | **17** | 17 |
| `commandLine` | **21** | 33 |
| `ignoreConfig` | **9** | 16 |
| `extends` | **3** | 8 |
| everything else | 0 | 120 |

Read this the way §4's board is read: **50 is a ceiling, not a forecast.** Those
cases still need the help printer, the version banner, the `--showConfig`
serialiser, config `extends` resolution, and the parser's error messages. The
conversion is a different and smaller number, and it is not yet known. The
instrument that would find it is the runner in phase 0 — which is the argument
for building the runner before building anything it measures.

Note also that `noEmit`'s 20 cases all carry a marker, because `--noEmit` still
writes `.tsbuildinfo`. "No emit" in the option's name is not "no output files".

---

## 3. The phases

Each phase names its gate. A phase is done when the gate reads a number, not when
the code exists.

### Phase 0 — the runner, first

Port enough of `internal/execute/tsctests` to execute a baseline file: parse the
`currentDirectory` / `useCaseSensitiveFileNames` / `Input` / `ExitStatus` /
`Output` sections, build an `InMemoryFileSystem` from the `//// [path]` blocks,
run the driver, and diff. Add a `cli_baselines` suite to `tsr-conformance`
alongside the existing sixteen.

**Gate:** the suite runs all 194 cases and reports a number. It will be near zero
and that is correct — a suite that reads 0/194 honestly is the instrument every
later phase is steered by, and the project has already learned this lesson twice
(the checker's oracles existed before the checker; `diaggap.rs` moved
`diagnostics` 8.75× by measuring rather than building).

**Why first, against the obvious objection.** Building the runner before there is
anything to run feels backwards, and the eighth session's experience is the
answer: `diagnostics` was called "structurally blocked" by two handoffs, and what
unblocked it was an instrument, not a build. The alternative — build the parser,
eyeball it, then discover which of its forty error messages are wrong — is how
the option table came to have 38 entries and a comment explaining that the rest
report as unknown.

### Phase 1 — the command-line parser

Port `internal/tsoptions/commandlineparser.go` (403 LOC) against the existing
declaration table. Includes `ParseCommandLine`, `parseOptionValue`,
`parseResponseFile`, and the did-you-mean machinery in `errors.go` (131 LOC).

The semantics that make this not a generic arg parser, each verified in the
source:

- `@responsefile` expands inline (`:168`).
- A boolean takes an **optional** following value: `--strict false` consumes the
  token, `--strict --noEmit` does not (`:262-272`).
- At most two leading dashes are stripped, so `-target` and `--target` are one
  option (`getInputOptionName`, `:163`).
- On a miss against the compiler table, the **watch** table is tried before
  erroring (`:150-152`).
- `IsTSConfigOnly` options accept `null` and `false` on the command line and
  error otherwise, with two different messages (`:236-256`).
- Errors are TS-coded diagnostics with spelling suggestions, not a parser's own
  rendering.
- `--build` is a separate entry point over a separate option set
  (`ParseBuildCommandLine`, `:63`).

**Gate:** `commandlineparser_test.go`'s table ported and passing.

**`clap` is refused** — see §6.

### Phase 2 — the host and the `System`

`tsc.System` (`internal/execute/tsc/compile.go:17`) is eight methods: `Writer`,
`FS`, `DefaultLibraryPath`, `GetCurrentDirectory`, `WriteOutputIsTTY`,
`GetWidthOfTerminal`, `GetEnvironmentVariable`, `Now`, `SinceStart`. Plus the
`ResolutionHost` impl pairing `OsFileSystem` with a current directory, which is
four lines and the last missing piece of the seam.

`DefaultLibraryPath` needs a decision that upstream does not have to make,
because Go embeds its libs and a Rust binary must either embed them
(`include_str!`, ~3.9 MB, and `lib.dom.d.ts` alone is 2.3 MB) or find them on
disk relative to the executable. **Not decided.** It is the first thing in §7.

**Gate:** a `tsr` binary exists and `tsr --version` prints upstream's banner
byte-for-byte.

### Phase 3 — `tscCompilation`'s control flow

Port `internal/execute/tsc.go:119-262` — the function that decides what a `tsc`
invocation *means* before any compiling happens. It is 143 lines and almost all
of it is error paths:

`--init` → write a config; `--version`; `--help`/`--all`; `--watch` with
`--listFilesOnly` is an error; `--project` with files on the command line is an
error; `--project` naming a directory looks for `tsconfig.json` inside it;
otherwise walk ancestors for a `tsconfig.json` (`findConfigFile`, `:265`), and
error if one is found *and* files were named unless `--ignoreConfig`; with no
config and no files, print version + help and exit **1**.

**Gate:** the emit-free slice of `commandLine` and `ignoreConfig` starts
converting. A number, not "it works".

### Phase 4 — help, version, `--showConfig`

`tsc/help.go` (15.9 KB), the version banner, and `tsoptions/showconfig.go` (389
LOC). Dull, large, and worth **17/17 of `showConfig`** plus a large share of
`commandLine` — the best ratio of any phase, because the output is a pure
function of the option table.

**Gate:** `showConfig` at or near 17/17.

### Phase 5 — the checking driver

`performCompilation` without emit: build the program, run the checker, report
diagnostics, compute the exit status. This is the phase that produces the thing
originally asked for — `tsr` pointed at a real repository — and it is fifth, not
first, because everything above it is what makes the result trustworthy.

**Gate:** `tsr --noEmit` on a real repository terminates, and the `noCheck` and
`listFilesOnly` families become reachable. Expect many false diagnostics: at 41%
of `checker_types` this is a development instrument and must be labelled one in
its own `--help`.

### Phase 6 — pretty output

`FormatDiagnosticWithColorAndContext` and `writeCodeSnippet`
(`diagnosticwriter.go:134-252`). Required by the baselines (§2.2), which is what
retires §6's "not yet" from `tsr-diagnostics::format`.

**Gate:** baselines that differ only in colour stop differing.

### Phase 7 — emit

Blocked on `tsr-transformers`, which does not exist. **144 of 194 `tsc` baselines
are behind this**, and it is a compiler workstream rather than a CLI one. Named
here so the ceiling is visible, not scheduled here.

### Phase 8 — `--build`, watch, incremental

`execute/build` 2,039 LOC, `execute/incremental` 3,404, `execute/watcher.go` 602,
`watchmanager` 627. Behind phase 7 and behind the `notify`-versus-hand-rolled
decision PLAN.md Phase 8 already owns.

---

## 4. The ranked board

Rank by what unblocks the most, then by what is cheapest to measure. As in
STATUS.md §4, a population is a ceiling and the conversion is unknown until an
instrument says otherwise.

| # | item | ceiling | conversion | why here |
|---|---|---:|---|---|
| 1 | phase 0 runner | — | — | every number below is unmeasurable without it |
| 2 | phase 1 parser | all 235 | unknown | nothing runs without argv |
| 3 | phase 4 help/version/showConfig | ~38 | unknown | pure function of the option table; best ratio |
| 4 | phase 2 host + `System` | — | — | four lines plus one undecided question (§7.1) |
| 5 | phase 3 control flow | ~30 | unknown | almost entirely error paths, so cheap to be exact |
| 6 | option table 48 → 135 | all | — | a missing option is an unknown-option error, not a default |
| 7 | phase 5 checking driver | ~8 | unknown | what a user asks for; not what the oracle rewards |
| 8 | phase 6 pretty | large | unknown | colour differences across the whole suite |

Item 6 is deliberately not item 1. Adding 87 option declarations is mechanical
and tempting to do first; doing it first would mean 87 entries whose behaviour
nothing checks. After phase 0 each one is measurable the day it lands.

---

## 5. Decisions already taken

| decision | where recorded |
|---|---|
| checker options come from `CompilerOptions`, resolved once | [ADR-0042](docs/adr/0042-checker-options-come-from-compiler-options.md) |
| the plain diagnostic format is byte-exact or it is a bug | `crates/tsr-diagnostics/src/format.rs` module docs |
| pretty output is refused rather than approximated | same |
| the OS filesystem panics rather than guessing case-sensitivity | `crates/tsr-vfs/src/os.rs` |
| crate is `tsr-execute`, binary is `tsr` | PLAN.md §crate layout |

---

## 6. Refused, with the reason that refused it

**`clap`, for the compiler-option surface.** Refused on four counts, each checked
against `commandlineparser.go` rather than recalled:

1. It reintroduces the declaration/assignment split that
   `crates/tsr-tsoptions/src/declarations.rs` exists to avoid — upstream keeps
   two lists in step by hand and has a test whose only job is to check them.
2. It cannot express the semantics listed in phase 1: optional boolean values,
   one-or-two dashes, response files, a second table on a miss.
3. Errors must be TS-coded diagnostics with did-you-mean suggestions, compared
   against baselines. `clap`'s renderer is a different shape and would have to be
   suppressed.
4. The faithful port is under 400 lines and smaller here than upstream, because
   our declarations carry their own setters.

**`clap` for outer subcommand dispatch is also declined**, more weakly: upstream
dispatches `--lsp` / `--api` by hand in a 32-line `main.go`, a `match` on
`argv[0]` costs nothing, and having the dependency present invites someone to
route compiler options through it later.

**Approximating pretty output.** A frame that is nearly upstream's — right idea,
different padding, colours chosen by eye — is worse than none, because nobody
replaces a format that already renders; they layer around it. Retired by phase 6,
not by taste.

**A minimal `tsr check` stopgap accepting only file paths and `--project`.**
Considered and declined on 2026-08-08: ~150 lines against phase 1's ~400, but it
is an invented surface at exactly the point where a stopgap grows callers
fastest. **This is the cheapest refusal on the page to reverse** — if being able
to run against a repository this week is worth more than the surface being right,
it is one afternoon. The condition for reversing it should be written down at the
time.

---

## 7. Open questions, none of them blocking today

1. **Where do the `lib.*.d.ts` files live for a shipped binary?** Embedded via
   `include_str!` (~3.9 MB in the binary, `lib.dom.d.ts` is 2.3 MB of it) or
   found on disk relative to the executable, as `DefaultLibraryPath` implies.
   Upstream does not face the choice; Go embeds them. Affects binary size,
   startup, and whether the binary is relocatable.
2. **What is the binary called in baseline output?** The baselines print `tsgo`.
   Substitution belongs in the phase 0 runner, but which direction — normalise
   ours to `tsgo`, or record ours and normalise the expectation — decides whether
   a diff is readable.
3. **Does `tsr-execute` depend on `tsr-conformance`'s in-memory FS for tests, or
   the reverse?** The baselines need `InMemoryFileSystem`; the layering should be
   decided before the runner, not after.
4. **`FakeTSVersion`.** Baselines pin a fake version string. Ours needs the same
   seam, and the real version needs somewhere to come from.

---

## 8. Updating this file

Same protocol as STATUS.md §8. In particular: when phase 0 lands, **replace every
"unknown" in §4's conversion column with a measured number**, and say in §2 that
the ceiling of 50 has been converted into a forecast — or that it has not, and by
how much it was wrong.
