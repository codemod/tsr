# Parity lane: r4-variants (tsr-2zk.940, continuing tsr-bb4.1)

Configuration-varied cases judged per configuration. The decision and its
alternatives are [ADR-0047](../../adr/0047-configuration-varied-cases-are-judged-per-configuration.md);
this note holds the measurements, at the r4 integration head `59c76e7`
(vendor `5b1047d`).

## 1. The expansion matches native's, case for case

`tests/configurations.rs` compares `CaseEntry::configurations()` over every
`compiler/` and `conformance/` case with `fixtures/native_configurations.tsv`,
native's own `GetFileBasedTestConfigurations` output (ADR-0047 has the Go):

| | count |
|---|---:|
| cases that expand | 1,647 |
| named configurations | 4,168 |
| cases the runner rejects before compiling (`@module: none`) | 8 |
| configurations with any suffixed baseline | 2,496 |
| suffixed baseline sets no configuration claims | **0** |

The first comparison disagreed on one row, `conformance/templateStringMultiline3_ES6`,
the corpus's one bare-CR file: upstream's `optionRegex` value is `[^\r\n]*`,
and `case.rs` ran the `@target` value to the end of the file. Fixed in the
case parser (ADR-0047, *Consequences*); no other suite's verdict on that case
changed (the plain dumps are byte-identical, §3).

The 1,672 configurations with no baseline are the ones upstream's runner
skips (`SkipUnsupportedCompilerOptions`): `target=es5` (1,108),
`module=system`/`amd`/`umd`, `alwaysStrict=false`, `esModuleInterop=false`,
`moduleResolution=classic`/`node10`. The configured rows report them under
`prepare_compilation`'s skip reason; a configuration skipped as "upstream
recorded no output" instead would mean the enumeration or the skip predicate
is wrong (§4 gives that count).

## 2. New populations

From `diagverdictdump` / `verdictdump` (keys `suite/case(<configuration>)`):

| | plain | varied (new keys) |
|---|---:|---:|
| diagnostics RIGHT | 4,339 | 649 |
| diagnostics WRONG | 1,163 | 440 |
| diagnostics judged (expects ≥1 diagnostic) | 5,502 | 1,089 |
| diagnostics EMPTY_RIGHT / EMPTY_WRONG | 4,240 / 74 | 1,304 / 29 |
| types aligned lines RIGHT / GAP / WRONG | 470,211 / 915 / 6,853 | 65,779 / 299 / 2,357 |
| types configurations with aligned lines | — | 1,738 |

The varied diagnostics pass rate is 59.6% (649/1,089) against the plain
78.9% (4,339/5,502), and the varied types line rate 96.1% against the plain
98.4%: the configurations lean towards `target`/`module` permutations of
emit- and downlevel-oriented cases, where the port is thinner.

## 3. What happened to the existing keys

- **Both §5 loss checks are empty**, and no verdict changed on any key the
  two dumps share.
- `verdictdump`: the plain rows are **byte-identical** (477,979 rows).
- `diagverdictdump`: every surviving plain row is byte-identical; **754 plain
  rows are gone** — 739 `EMPTY_RIGHT` and 15 `EMPTY_WRONG`. Each is a case
  that upstream compiles only under named configurations, which no varied
  `.errors.txt` had excluded because every configuration is clean, so the
  dump had judged it as itself under the default options. That is the
  r4-jsx2 hazard (`r4-jsx2.md` §4, 22 `@jsx` cases) at its real size:
  most are `@target: es5, es2015` cases. Their configurations are now the
  `case(target=…)` rows. A join-based loss check cannot see a removed key,
  so they are listed here rather than left to be noticed.

## 4. Coverage rows and wall time

Full `coverage` run at the lane head (wall 1,141 s on 4 cores): every
existing row unchanged (`checker_types` 8114/9538, 98.19% lines;
`diagnostics` 4339/5502; `binder_symbols` 8508/8508). New rows:

| row | passed | rate | skipped |
|---|---:|---:|---:|
| `binder_symbols_configured` | 1664/1704 | 97.65% | 2464 |
| `checker_types_configured` | 1547/1928 | 80.24% (lines 91.01%) | 2240 |
| `diagnostics_configured` | 649/1089 | 59.60% | 3079 |

Skips in the configured rows: 1,600 by `SkipUnsupportedCompilerOptions`
(target ES5 1,244; module System/AMD/UMD 290; alwaysStrict=false 52;
esModuleInterop=false 20; outFile 20; moduleResolution Classic/Node10 14;
baseUrl 2), 68 known divergences, and **36 "upstream recorded no output"**:
all 18 cases are `@noTypesAndSymbols: true` + `@noEmit: true`, whose clean
configurations write no baseline at all — the plain 617's convention, not an
enumeration error.

Wall, uncontended, base binary against this head:

| run | base | after |
|---|---:|---:|
| `coverage checker_types diagnostics` (plain rows only) | 461 s | 472 s (+2.4%, noise) |
| `diagverdictdump` (+2,422 configuration rows) | 384 s | 350 s / 362 s |
| `verdictdump` (+74,276 aligned lines, +15.5%) | 269 s | 318 s (+18%) |

The plain-row overhead (`load()` computing the configuration, the dump's
`is_expanded` re-read) is within run-to-run noise; the rest is the extra
compilations. The `tsr` binary is byte-identical to the baseline's, so the
§5 perf gate does not apply.
