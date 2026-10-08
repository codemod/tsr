# ADR-0047: Configuration-varied cases are judged per configuration, as their own rows

- **Status:** Accepted — built (`crates/tsr-conformance/src/configuration.rs`,
  `CaseEntry::configured`, the `*_configured` suites, both verdict dumps).
- **Date:** 2026-10-08
- **Issue:** `bd tsr-2zk.940` (continuing `bd tsr-bb4.1`)
- **Related:** [ADR-0006](0006-conformance-oracle.md) (the conformance
  oracle), [`docs/conventions.md`](../conventions.md) (*the corpus directory
  is not the population*), [`docs/parity/notes/r4-variants.md`](../parity/notes/r4-variants.md)
  (the measurements).

## The forcing constraint

A case whose directive lists several values of a *vary-by* option —
`// @target: es2015, esnext`, `// @jsx: react-jsx,react-jsxdev`,
`// @strict: *` — is never compiled as itself by typescript-go's runner. It is
compiled once per combination, and each compilation writes its own baselines
under a suffixed name: `case(target=es2015).errors.txt`,
`case(jsx=react-jsx,target=esnext).types`. At the pinned `5b1047d`:

- **1,647 corpus cases expand into 4,168 named configurations**; 8 more are
  rejected by the runner before compiling (`@module: none` is an unknown
  value, a `t.Fatalf`).
- 2,496 of those configurations have baselines; the other 1,672 are skipped
  by `SkipUnsupportedCompilerOptions` (`target=es5` alone is 1,108 of them).
- Every suite here skipped these cases — 793 in `diagnostics`, 2,032 in
  `checker_types` — so **every parity number excluded them**.

Worse than the exclusion: the case parser stores the raw `es2015, esnext` and
`apply_test_directives` parses it as *unset*, so wherever such a case was
judged as itself it was judged under a configuration upstream never ran. The
r4-jsx2 lane found 22 `@jsx: a,b` cases in the diagnostics dump that way:
they have no varied `.errors.txt` (each configuration is clean), so
`has_varied_errors` did not skip them, and the dump scored them as
`EMPTY_*` against the default `jsx`.

## The decision

1. **Port the expansion, not the baseline names.**
   `configuration::file_based_test_configurations` is
   `GetFileBasedTestConfigurations` (`harnessutil.go:1007`) over
   `compilerVaryBy` (`compiler_runner.go:161`): which options vary, value
   deduplication by parsed meaning (`es6, es2015` is one compilation, named by
   the first spelling), `*` and `-v`/`!v`, the 25-variation cap, the
   unknown-value rejection, the name (sorted keys, lowercased values, comma
   joined), and the single-value normalisation (`@declaration: true;` and
   `true,` compile as `true`).
2. **A configuration is a `CaseEntry`** named `suite/case(<configuration>)`
   with the configuration attached. Its `stem()` is then the suffixed stem, so
   `baseline_path`, `has_any_baseline`, `has_known_divergence` and
   `had_error_baseline` all answer for that compilation without a second code
   path, and `load()` applies the configuration's values to the parsed
   options before anything compiles.
3. **Separate rows, separate keys.** `Corpus::discover()` is unchanged; the
   per-configuration population comes from `Corpus::configured`, and only the
   suites that ask for it (`Suite::per_configuration`) see it:
   `diagnostics_configured`, `checker_types_configured`,
   `binder_symbols_configured`. The plain rows keep their denominators, and
   the dumps add `case(target=es2015)` keys without moving any existing key —
   the integrator's zero-loss join keeps working.
4. **A varied case is never judged as itself.** Its plain key is skipped by
   the suites (as before, with a reason naming the configured row) and
   dropped from `diagverdictdump`, which removes the 22 `EMPTY_*` rows above.

## Alternatives, taken seriously

- **Enumerate variants from the baseline directory** (`has_variant` already
  indexes `case(…)` names). Rejected: it cannot see a configuration upstream
  ran and wrote nothing for, it cannot tell `target=es5` (skipped upstream)
  from a configuration this port names wrongly, and it would never surface
  drift in the vary-by set. It is, however, the cross-check: every one of the
  2,496 suffixed baseline sets is claimed by exactly one enumerated
  configuration (measured, `r4-variants.md`).
- **Fold the variants into the existing rows.** One `diagnostics` row over
  plain and varied compilations is what upstream's own test count would be.
  Rejected *for now* because every recorded number in `STATUS.md` and the
  round docs is over the plain population, and a row whose denominator jumps
  by ~2,000 in a merge reads as a regression or a win that is neither. Revisit
  when the board is next re-based; summing the two rows gives the folded
  number exactly.
- **Derive the vary-by set from `tsr_tsoptions`.** That table declares only
  the options `CompilerOptions` has a field for and has no `Affects*` flags,
  so it would silently drop options upstream varies (`downlevelIteration`,
  `emitDecoratorMetadata`, …). The 72 names are checked in instead, as
  computed upstream, and the corpus test below is what catches drift.
- **Fix `apply_test_directives` to pick the first value of a list.** That is
  a configuration upstream does not run either; it would replace one wrong
  compilation with another.

## Consequences accepted

- The vary-by table and the six enum maps are a copy. An upstream bump that
  adds a vary-by option, or an enum key, changes the expansion here only when
  someone regenerates `tests/fixtures/native_configurations.tsv` — and then
  `tests/configurations.rs` fails until the table follows.
- The fixture is 249 KB. It is the oracle for the expansion, so it is
  committed rather than regenerated in CI (CI has no Go toolchain).
- The 22 plain `EMPTY_*` rows leave `diagverdictdump`; their configurations
  appear as `case(jsx=…)` rows instead. A join-based loss check does not see
  a removed key; it is listed in the lane report so nobody reads it as a
  silent shrink.
- `CaseEntry::is_expanded` re-reads the case file, once per eligible case in
  the dump. Measured in `r4-variants.md`.
- The case parser now ends a directive value at a bare `\r`, as upstream's
  `[^\r\n]*` does (`conformance/templateStringMultiline3_ES6`, the one
  bare-CR file, read `@target` as the rest of the file before).

## How the fixture was produced

A Go test dropped into `internal/testrunner` of the pinned checkout (and
removed afterwards), run with a locally built go1.26
(`scripts/offline-cargo/build-tsgo.sh` builds one offline):

```go
func TestZZDumpVariants(t *testing.T) {
	out, _ := os.Create(os.Getenv("ZZ_OUT"))
	defer out.Close()
	root := "../../_submodules/TypeScript/tests/cases"
	for _, suite := range []string{"compiler", "conformance"} {
		filepath.Walk(filepath.Join(root, suite), func(p string, info os.FileInfo, err error) error {
			if info.IsDir() || !(strings.HasSuffix(p, ".ts") || strings.HasSuffix(p, ".tsx")) {
				return nil
			}
			abs, _ := filepath.Abs(p)
			content, _ := osvfs.FS().ReadFile(abs) // decodes BOMs and UTF-16, as the runner does
			settings := extractCompilerSettings(content)
			base := filepath.Base(p)
			key := suite + "/" + base[:len(base)-len(filepath.Ext(base))]
			ok := t.Run(key, func(t *testing.T) {
				for _, c := range harnessutil.GetFileBasedTestConfigurations(t, settings, compilerVaryBy) {
					if c.Name != "" {
						fmt.Fprintf(out, "%s\t%s\n", key, c.Name)
					}
				}
			})
			if !ok {
				fmt.Fprintf(out, "%s\tFATAL\n", key)
			}
			return nil
		})
	}
}
```

then `sort`. Reading the files with `os.ReadFile` instead of `osvfs` misses
the UTF-16 cases (100 configurations, e.g. `compiler/instanceofOperator`) —
the first run made exactly that mistake.

## How we would know this is wrong

- `tests/configurations.rs` fails: the expansion disagrees with native's on
  any case.
- A configured row skips a configuration as "upstream recorded no output"
  where `prepare_compilation` finds no upstream skip reason *and* the case
  could have written something. Measured: 36 such configurations, all 18
  cases carrying `@noTypesAndSymbols: true` and `@noEmit: true`, whose
  clean configurations legitimately write nothing (`r4-variants.md` §4).
  Any other case there means the enumeration or the skip predicate is wrong.
- A configured case passes while its plain sibling (compiled under the
  default) would have failed for an option the configuration sets — that is
  the point; the reverse would mean the configuration's values are not
  reaching `CompilerOptions`.
