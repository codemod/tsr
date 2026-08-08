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

Measured at the CLI-scaffold landing, 2026-08-08, by
`cargo run --release -p tsr-execute --bin cli_baselines`.

| | |
|---|---:|
| **`cli_baselines`** | **33/43 judged = 76.74%** |
| behind the emitter (excluded) | 151 |
| total `tsc` baselines | 194 |

`tsr` builds, runs, and type-checks a real repository. Phases 0–6 are done or
partly done; 7 and 8 are blocked on the emitter. The prose below was written
before any of it existed and is corrected in place; the phase board in §3 now
carries a state per phase.

> **Superseded: "There is no binary."** The workspace's only `[[bin]]` is
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
| a `ResolutionHost` over the OS | **done** | `tsr_execute::compile::DriverHost` |
| a command-line parser | **done** | `tsr_tsoptions::command_line`, 403 LOC ported, 27 tests |
| a `System` (writer, cwd, TTY, width, env, clock, version) | **done** | `tsr_execute::system::System`; `OsSystem` and `BaselineSystem` implement it |
| pretty diagnostic rendering | **done** | `tsr_diagnostics::format`, the refusal retired — see below |
| the baseline runner | **done** | `tsr_execute::baseline` + the `cli_baselines` bin |
| `--showConfig` | **done** | struct-order options, implied options, config-relative paths |
| `--help` | **done (narrow layout)** | the wide two-column form and `--all` are not, §3 phase 4 |
| `--init` | **refused** | writes a template built from per-option descriptions this port lacks |
| **emit** | **missing** | no `tsr-transformers`; `tsr-dts` is declaration-shape only |
| watch, `--build`, incremental | **refused, loudly** | report `NotImplemented` (upstream's status 5) rather than silently compiling once |

### The option surface, which is the number that matters most

| | upstream | here | share |
|---|---:|---:|---:|
| `CompilerOptions` fields | 130 | 125 | 96% |
| declared options (`declscompiler.go` / `declarations.rs`) | 121 | 134 | complete |

Counted by `awk '/^type CompilerOptions struct/,/^}/' | grep -cE '^\t[A-Z]'` and
`grep -c 'name: "'`. Was 50/48 before the CLI; the 27 added are the command-line
surface — `--help`, `--project`, `--showConfig`, `--pretty` and the rest — which
had nowhere to land before there was a driver to read them.

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

### Phase 0 — the runner, first · **DONE**

Port enough of `internal/execute/tsctests` to execute a baseline file: parse the
`currentDirectory` / `useCaseSensitiveFileNames` / `Input` / `ExitStatus` /
`Output` sections, build an `InMemoryFileSystem` from the `//// [path]` blocks,
run the driver, and diff. Add a `cli_baselines` suite to `tsr-conformance`
alongside the existing sixteen.

**Landed. The suite reads 28/43 judged (65.12%), 151 excluded as behind the
emitter.** It has read 13 → 19 → 20 → 25 → 26 → 28 → 30 → 32 → 33 as each phase landed.

**The runner also reads the Go test source.** A `tsc` baseline does not record
the environment its scenario runs under — that lives in `tsc_test.go` as a
`map[string]string` on the `tscInput` literal — so three baselines
(`NO_COLOR`, `FORCE_COLOR`, `TS_TEST_TERMINAL_WIDTH`) were being replayed with
inputs they never had. `ScenarioEnvironments` parses those declarations. The
alternative was to infer the environment from the file *name*, which those
three happen to state; that is a guess dressed as a rule and would mis-run
silently the moment upstream renamed a scenario. The gate as written expected a number near zero; the real first
reading was 13/43, because the phases below were built alongside rather than
after. Both halves of the original argument held: the runner found four separate
defects within minutes of first running — the version string, the exit status for
a config error, `-p .` resolving to the empty string, and the `--showConfig`
indent — none of which any unit test would have caught.

**Gate (as written):** the suite runs all 194 cases and reports a number. It will be near zero
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

### Phase 1 — the command-line parser · **DONE**

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

**Landed**, with 27 tests covering each behaviour above. Upstream's own test
table is *not* ported — the behaviours are tested directly instead, which is a
gap: `commandlineparser_test.go` is 572 lines and would be a stronger oracle.

**One deliberate divergence, and it is the only invented behaviour in the CLI.**
Upstream recurses without bound on `@response` files, so a file naming itself
overflows the stack. This port caps nesting at 32 and reports the same
`Cannot read file` diagnostic. A compiler a build system invokes should not be
crashable by a file it was pointed at, and the cap is far above any real usage.
Recorded here because "no improvisation" was the session's constraint and this is
the one place it was knowingly bent.

**Not ported:** `ParseBuildCommandLine` (build mode is phase 8), and the **watch
option table** — upstream consults it when a name misses the compiler table
(`commandlineparser.go:150`), so `--watchFile` reports as unknown here where
upstream accepts it.

**`clap` is refused** — see §6.

### Phase 2 — the host and the `System` · **DONE**

`tsc.System` (`internal/execute/tsc/compile.go:17`) is eight methods: `Writer`,
`FS`, `DefaultLibraryPath`, `GetCurrentDirectory`, `WriteOutputIsTTY`,
`GetWidthOfTerminal`, `GetEnvironmentVariable`, `Now`, `SinceStart`. Plus the
`ResolutionHost` impl pairing `OsFileSystem` with a current directory, which is
four lines and the last missing piece of the seam.

`DefaultLibraryPath` needed a decision upstream does not, because Go embeds its
libs. **Decided: on disk**, with the reasoning and the search order in
`crates/tsr-execute/src/os_system.rs`. `TSR_LIB_PATH`, then `<exe dir>/lib`, then
the vendored submodule path recorded at build time so `cargo run` works in this
checkout with no setup. Embedding was rejected for ~3.9 MB of binary and for
making the libraries un-swappable; the accepted cost is a binary that is not
self-contained. **§7.1 is answered and now records the answer.**

**Gate:** a `tsr` binary exists and `tsr --version` prints upstream's banner
byte-for-byte.

### Phase 3 — `tscCompilation`'s control flow · **DONE**

Port `internal/execute/tsc.go:119-262` — the function that decides what a `tsc`
invocation *means* before any compiling happens. It is 143 lines and almost all
of it is error paths:

`--init` → write a config; `--version`; `--help`/`--all`; `--watch` with
`--listFilesOnly` is an error; `--project` with files on the command line is an
error; `--project` naming a directory looks for `tsconfig.json` inside it;
otherwise walk ancestors for a `tsconfig.json` (`findConfigFile`, `:265`), and
error if one is found *and* files were named unless `--ignoreConfig`; with no
config and no files, print version + help and exit **1**.

**Landed**, including the two orderings that look like bugs until you read
upstream: a config that fails to parse exits **2** (`OutputsGenerated`), not 1,
and `tsc` with no config and no files prints its banner and help then exits
**1**, not 0.

### Phase 4 — help, version, `--showConfig` · **DONE**

`tsc/help.go` (15.9 KB), the version banner, and `tsoptions/showconfig.go` (389
LOC). Dull, large, and worth **17/17 of `showConfig`** plus a large share of
`commandLine` — the best ratio of any phase, because the output is a pure
function of the option table.

**All three landed.** `--help` is `printEasyHelp` in the narrow layout: the
header, the seven `COMMON COMMANDS` examples, and 27 options across
`COMMAND LINE FLAGS` and `COMMON COMPILER OPTIONS`, each with its description
and, where it has one, its `type:`/`one of:` and `default:` lines.

**The 27 entries live in `tsr-execute/src/help.rs`, not on the declarations.**
Upstream hangs `Description`, `Category`, `DefaultValueDescription` and
`ShowInSimplifiedHelpView` off every option; carrying four more fields on all 71
declarations to populate 27 would put help presentation into the table the
*parser* reads. The duplication is guarded by three tests that fail the build if
a help entry names an option that does not exist, documents a short name the
parser does not have, or carries a description that is not **verbatim** from the
generated diagnostic catalogue. That last one is what proves no help text here
was written by hand.

**Two regimes, one ported.** `generateOptionOutput` branches on
`terminalWidth >= 80`; above it each option is a wrapped two-column layout with a
blue-background "TS" icon in the header. Exactly one baseline sets
`TS_TEST_TERMINAL_WIDTH` to reach it. Not approximated.

**`--help --all` is not ported** and falls back to the simplified view: it lists
all ~135 options plus watch and build sections, and this port declares 71 with no
watch or build table, so the output could only be a subset pretending to be
the whole.

`--init` is **refused** rather than approximated: it writes a commented
`tsconfig.json` built from the same per-option descriptions, and a *different*
template is an artifact that would end up committed in users' repositories.

**`--showConfig`'s ordering rule was found rather than guessed** — see §7.5,
which is now answered.

### Phase 5 — the checking driver · **DONE**

`performCompilation` without emit: build the program, run the checker, report
diagnostics, compute the exit status. This is the phase that produces the thing
originally asked for — `tsr` pointed at a real repository — and it is fifth, not
first, because everything above it is what makes the result trustworthy.

**Landed, and it works.** On a three-error fixture:

```text
src/index.ts(4,7): error TS2322: Type 'string' is not assignable to type 'number'.
src/index.ts(5,13): error TS2304: Cannot find name 'nope'.

Found 3 errors in the same file, starting at: src/index.ts:4
```

Exit code 1, config read from `tsconfig.json`, libraries loaded, diagnostics
positioned. Expect wrong ones: at 41% of `checker_types` this is a development
instrument, and `--help` says so.

### Phase 6 — pretty output · **DONE, and it moved to the front**

`FormatDiagnosticWithColorAndContext` and `writeCodeSnippet`
(`diagnosticwriter.go:134-252`), **ported**.

**This phase was ninth on the plan and turned out to be a prerequisite.** §2
recorded that the baselines assert pretty output; what the plan did not draw the
conclusion from is that *almost every baseline that prints anything* is pretty,
so nothing could be compared until the renderer existed. Written last in the plan
and needed fourth in practice.

The `tsr-diagnostics::format` refusal is retired rather than overruled, and the
distinction matters: the refusal was against *approximating* a frame nobody could
check. With a byte-comparing oracle, approximation is no longer possible — which
is the exact condition the refusal named as its own falsifier.

### Phase 7 — emit · **BLOCKED**

Blocked on `tsr-transformers`, which does not exist. **144 of 194 `tsc` baselines
are behind this**, and it is a compiler workstream rather than a CLI one. Named
here so the ceiling is visible, not scheduled here.

### Phase 8 — `--build`, watch, incremental · **BLOCKED**

`execute/build` 2,039 LOC, `execute/incremental` 3,404, `execute/watcher.go` 602,
`watchmanager` 627. Behind phase 7 and behind the `notify`-versus-hand-rolled
decision PLAN.md Phase 8 already owns.

---

## 4. The ranked board

Re-ranked after the scaffold. Ceilings are measured; conversions now mostly are
too, because phase 0 exists.

| # | item | worth | state |
|---|---|---|---|
| 1 | `--help --all` | 1 baseline | **structure done**, and the table is *generated* from upstream rather than transcribed — 106 options, categories and descriptions as `Message` references (`help_all.rs`). Still short: the `--build` pseudo-option, 11 options whose description constant does not resolve, and the `WATCH OPTIONS` / `BUILD OPTIONS` sections |
| 2 | ~~`--showConfig` implied options~~ | **done**, +2 | §7.10 answered: `GetEmitScriptTarget` returns **latest-standard** for an unset target, not ES5 |
| 3 | `extends` diagnostics | 2 baselines | a non-string `files`/`include` element must report TS5024 **positioned in the base file**, which needs per-element spans carried across the `extends` hop |
| 4 | `--locale` | 2 baselines | one locale shipped; upstream has a message catalogue per language. **Refused** |
| 5 | `commandlineparser_test.go`'s table (572 LOC) | — | a stronger oracle for phase 1 than the 33 hand-written tests |
| 6 | the wide `--help` header | 1 baseline | §7.8: four columns unexplained |
| 7 | ~~`non-object-config-root`~~ | **done**, +1 | §7.9 answered: the summary *is* printed, and the pretty reporter writes one newline after the last frame |
| 8 | `references` in the program | 1 baseline | `Config-with-references-…` |
| 9 | emit (`tsr-transformers`) | **151 baselines** | **not a CLI item**; the ceiling above everything |

**Refused, with the reason:** `--locale` (`commandLine/locale.js` wants
`Verze FakeTSVersion`). This port ships one locale, upstream ships a message
catalogue per language, and translating diagnostics is not a compiler task.

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

~~**Approximating pretty output.**~~ **Retired**, exactly as the refusal said it
would be: it was against *guessing* a frame nobody could check, and phase 0 made
guessing impossible. The renderer is a transliteration of `writeCodeSnippet` and
is compared byte-for-byte. Kept on the page because the reasoning generalises —
a refusal that names its own falsifier is one that can be lifted honestly.

~~**A minimal `tsr check` stopgap.**~~ **Moot, and the decision was right.** The
faithful parser took one session rather than the afternoon a stopgap would have,
and the real `tsc` surface — `-p`, `--showConfig`, `--pretty`, response files —
came with it. A stopgap would have had to be deleted.

**The response-file recursion cap** is the one invented behaviour in the CLI, and
it is recorded in §3 phase 1 rather than hidden: upstream overflows its stack on
a self-referential `@file` and this port reports a diagnostic instead.

**`--init`.** Writes a commented `tsconfig.json` built from per-option
descriptions this port does not have. A different template would end up committed
in users' repositories, so it reports `NotImplemented` (upstream's own status 5).

**`--locale`.** One locale shipped; see §4.

---

## 7. Open questions

1. ~~**Where do the `lib.*.d.ts` files live for a shipped binary?**~~
   **Answered: on disk**, with a three-step search order. See §3 phase 2 and
   `crates/tsr-execute/src/os_system.rs`. Kept rather than deleted because the
   rejected option — embedding — is the one to revisit if a self-contained binary
   ever matters more than swappable libraries.
2. ~~**What is the binary called in baseline output?**~~ Answered by the shape of
   the problem: the *version* is what varies, not the name, so `System::version()`
   is the seam and `BaselineSystem` returns `FakeTSVersion`. The banner text
   itself is `tsc: The TypeScript Compiler`, which is what upstream prints and
   what the baselines expect.
3. ~~**Does `tsr-execute` depend on `tsr-conformance`?**~~ Neither: the baseline
   runner lives in `tsr-execute` and uses `tsr_vfs::InMemoryFileSystem` directly,
   so the CLI's oracle carries no dependency on the compiler's.
4. **`FakeTSVersion`** — resolved as (2). The real version is a constant in
   `tsr_execute::system::VERSION` and still has no build-time source.
5. ~~**`--showConfig` option ordering.**~~ **Answered by reading
   `showconfig.go` instead of guessing a third time.** `serializeCompilerOptions`
   (`:172`) reflects over `core.CompilerOptions` and emits every non-zero
   exported field in **Go struct declaration order**, skipping the command-line
   and output-formatting categories. Then `addImpliedOptions` (`:300`) appends
   derived options under three conditions: not written explicitly, at least one
   dependency written, and the computed value differing from what wholly default
   options would compute. All three are now implemented.

   Worth keeping as a record of method: the first version guessed our
   declaration-table order, the second guessed the config's written order and
   matched two baselines by luck. Both were guesses at a rule stated outright in
   389 lines of Go. **Two wrong guesses cost more than reading the source
   would have.**

6. **The wide `--help` layout** (`getPrettyOutput`, terminal width ≥ 80): the
   two-column wrap and the blue-background icon. One baseline.

7. **Config-file error paths.** `non-object-config-root` and `extends` with a
   non-string `files`/`include` expect diagnostics the config parser does not
   raise, and an exit status of 2.

8. **The wide `--help` header's padding, off by four columns.** The layout is
   ported from `getHeader` (`help.go:44`) and the option columns match, but the
   banner line does not. Measured on
   `show-help-with-ExitStatus.DiagnosticsPresent_OutputsSkipped.js` at
   `TS_TEST_TERMINAL_WIDTH=120`: the expected line 1 pads the banner to **119**
   columns before the icon, while line 2 indents the icon's second row to
   **115**. Upstream writes both from the same `leftAlign`
   (`fmt.Sprintf("%-*s", leftAlign, message)` then
   `strings.Repeat(" ", leftAlign)`), and `leftAlign` is `min(width,120) - 5` =
   115 — which reproduces line 2 exactly and line 1 four columns short.
   Something makes the first line wider that is not in the function as read.
   **Not fudged**: adding four to one branch would make this baseline pass and
   every other width wrong, and the discrepancy is worth more as a question than
   as a constant.

9. ~~**`non-object-config-root`'s trailing line.**~~ **Answered, and the earlier
   note here was wrong in a way worth keeping.** It recorded that config-parse
   errors print *no* error summary, reasoning from `tsc.go:225` building
   `reportErrorSummary` after that branch. The baseline says otherwise: it ends
   with `Found 2 errors in the same file, starting at: tsconfig.json:1`. The
   reporter built at `:225` is the *watch* one.

   With the summary restored, the remaining difference was a single blank line,
   and it belonged to the **pretty reporter**, not to the config path:
   `CreateDiagnosticReporter`'s pretty arm writes a newline after each
   diagnostic where the plain arm relies on the diagnostic's own trailing one.
   The two blank lines before the summary are one from there and one from the
   summary's own leading newline.

   **Reading the expected bytes settled in a minute what reading the call order
   had got backwards twice.** The line-diff the runner prints was actively
   misleading here — it reported the mismatch at the summary, which was correct
   output in the wrong place.

10. ~~**Which implied options `--showConfig` actually emits.**~~ **Answered, and
    the answer is a divergence in this port's core rather than anything about
    `--showConfig`.** `GetEmitScriptTarget` (`core/compileroptions.go:195`) is
    two lines: the written target, or `ScriptTargetLatestStandard`. This port's
    `CompilerOptions::emit_script_target` instead derives ES5 from the module
    kind, which is what older TypeScript did.

    One rule, two baselines, **opposite directions** — which is what makes it a
    fact about upstream rather than a fit to one case:

    - `Show-TSConfig-with-transitively-implied-options` writes `module: nodenext`
      and expects **no** `useDefineForClassFields`, because under upstream's rule
      the default options also compute `true` and an implied value equal to the
      default is dropped.
    - `Show-TSConfig-with-compileOnSave-and-more` writes `target: es5` and
      expects `useDefineForClassFields: false` — which appears *only* because the
      default is latest-standard `true` and es5 differs from it.

    Applied locally in `show_config.rs` (`show_config_emit_target`) and
    **deliberately not fixed in `tsr-core`**: `emit_script_target` is read by the
    checker, the loader and module resolution, so correcting it is a compiler
    change with its own conformance measurement, and this session is scoped to
    the CLI. **This is the highest-value item this session found for whoever
    touches the core next** — it is two lines, and the evidence is above.

## 8. Updating this file

Same protocol as STATUS.md §8. In particular: when phase 0 lands, **replace every
"unknown" in §4's conversion column with a measured number**, and say in §2 that
the ceiling of 50 has been converted into a forecast — or that it has not, and by
how much it was wrong.
