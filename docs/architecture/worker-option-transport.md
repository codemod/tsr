# Worker option transport

`tsr-1yb.3.1.1.2` fixes the option carrier before production checker scheduling.
An extending config with `checkers: 2, singleThreaded: false` now replaces a
base config's `8, true`. Explicit JSON null clears either inherited field.
CLI `--checkers` replaces the project value, and a final CLI null clears an
earlier occurrence. Production checking remains serial.

Worker fields merge multiple bases left-to-right before applying actual own
values. Raw `compilerOptions` belongs to the declaring config. A base with an
own null clears earlier bases, while a derived base without an own null cannot
turn an ancestor's null into a new clearing operation. The broader existing
option/selection merge audit is tracked separately in `tsr-6.61`.

The pinned native source is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`:

- `internal/tsoptions/parsinghelpers.go:mergeCompilerOptions` copies set fields
  and clears fields explicitly null in raw `compilerOptions`.
- `internal/execute/tsc.go` wraps raw command-line options under
  `compilerOptions` before merging the project.
- `internal/tsoptions/commandlineparser.go:parseOptionValue` uses `strconv.Atoi`
  and the option's minimum; `checkers` has minimum one.
- `internal/compiler/checkerpool.go:newCheckerPool` selects from the complete
  Program file count, with `singleThreaded` first and a maximum of 256.

`CompilerOptions.checkers` carries a machine-width integer. CLI values bypass
JSON's floating representation, so `9007199254740993` stays exact on a 64-bit
host. JSON uses floating conversion and truncation; it accepts zero, negative
and fractional numeric controls that the CLI rejects. Native extreme JSON
float-to-integer conversion is implementation-dependent: the recorded extremes
and matching regression rows are characterized on darwin/arm64. They do not
certify those extremes on other architectures.

The standalone worker probe keeps its positive-i32 positional override contract.
It accepts wider parsed project counts and then applies the native pool clamp.
Compiler CLI parsing and this standalone argument parser are separate controls.

## Reproduce

Build the release `tsr` binary and `checker_workers` example, then run
`worker-option-transport-controls.py` with absolute `--normal`, `--probe`,
`--tsgo` binary paths and a fresh `--output` directory. It exercises inherited
and own config values, null clearing, a one-file noLib program, and complete
native/TSR CLI output for invalid and large valid counts. `POLICY` exposes parsed
count selection; `showConfig` omits worker controls and cannot prove their values.

`worker-option-transport-native_test.go` belongs to the pinned native
`internal/compiler` package. Add it through a Go overlay and run
`go test ./internal/compiler -run '^TestTSRTransport' -count=1` with
`TSR_TRANSPORT_ORACLE` and `TSR_TRANSPORT_MERGES` output paths. The recorded run
restored two previously instrumented source files through the overlay and
blanked two unrelated compiler tests to avoid their uncached dependency. It
checks isolated parser, real config/CLI merge and pool-slot behavior; it is not
the native compiler test suite. Pool controls supply full file-array lengths
without binding those synthetic files or constructing checker state.

## Validation limits

The accompanying JSON records exact paired source/binary hashes, native controls
and the completed validation gates. The source-450 corpus pair isolates this
transport change from concurrent checker fidelity work on main. Complete sorted
type assertion and diagnostic dumps preserve wrong/GAP rows, empty-error cases
and duplicate diagnostics. Matching aggregates alone are not the acceptance gate.
The diagnostic corpus records file/line/column/code tuples for every eligible
case; it does not record formatted messages or varied/known-divergence cases.
The public CLI controls separately compare complete formatted output.
Invalid numeric CLI outputs match native byte for byte. Valid counts retain an
existing extra TSR plain-output summary (`tsr-6.60`): the control pins that exact
footer difference and compares every diagnostic message with output enabled.
Using `--quiet true` would suppress diagnostics and is not a valid substitute.

This change is a correctness prerequisite. It does not establish a speed win,
a private-checker memory bound or the required comparable whole-project
TSR/pinned-tsgo median ratio of at most 0.50. Memory admission remains
`tsr-1yb.3.1.1.3`; affinity/query ownership and production scheduling remain
`tsr-1yb.3.2` and `tsr-1yb.6`.
