# ADR-0020: the parser reads `tsconfig.json`, the option table carries its own setters, and the glob engine is ported rather than adopted

- **Status:** accepted
- **Date:** 2026-08-04
- **Relates to:** [ADR-0017](0017-program-before-tsconfig.md) (which deferred
  this deliberately), [ADR-0019](0019-the-loader-gate-discharges-the-mode-circularity.md)
  and [ADR-0018](0018-splitting-the-resolution-oracle.md) (the suites this moves),
  [ADR-0004](0004-oxc-inspiration-not-dependency.md) (dependencies vs. crates)
- **Scope:** how a config file is read, how compiler options are declared, where
  the `include`/`exclude` matcher lives, and what `tsr-tsoptions` deliberately
  omits

## The forcing constraint

Both resolution suites skipped every case that configures itself through a
`tsconfig.json` unit — 47 of them, of which **20 have a committed
`.trace.json`** sitting unjudged. It was the largest single skip bucket in each
suite, and it was the last thing between Phase 3 and its gate.

Worse than the 20: for those cases the harness could not evaluate *upstream's own
skip predicate*, because `SkipUnsupportedCompilerOptions` reads
`options.BaseUrl`, `options.OutFile` and `options.ModuleResolution`, and those
were in a file nothing parsed. They were bucketed as "tsconfig-configured", which
was true and uninformative.

## Decision 1: the parser reads the config file

`tsconfig.json` is not JSON. It permits comments and trailing commas, and the
corpus contains configs with both — `moduleResolutionWithSymlinks_referenceTypes`
opens its `compilerOptions` with a `//` comment explaining why `typeRoots` is
empty. A conforming JSON parser rejects real configs.

So `tsr-parser` gains a `ScriptKind::Json` entry point, ported from
`parseJSONText`, which reuses the object- and array-literal parsers already
there. It is about 70 lines. Comments become trivia and a trailing comma is
ordinary object-literal syntax, so neither needs a special case.

Two consequences follow that are worth having anyway: an option error can point
at the *value*'s span, and the language service will eventually want the same
tree for completions inside a config.

**Alternative: use `tsr_module::json`,** the ordered JSON reader ported in slice
2 for `package.json`. Rejected: it is strict JSON, so it would reject the configs
that matter, and teaching it comments would make it a second, worse jsonc parser
sitting beside a real one. It stays for `package.json`, which is machine-written
and read for values rather than spans.

**Alternative: a jsonc crate.** Rejected on ADR-0004's rule — we take oxc's
*dependencies*, not a parser for a language we already parse. It would also
produce a tree with no spans into our arena, so every diagnostic would need a
second position mapping.

## Decision 2: one option table, with setters, instead of upstream's two

Upstream declares an option in `optionsForCompiler` (`"target"` is an enum over
`ScriptTarget`) and assigns it in `ParseCompilerOptions`, a switch on the option
name. Two lists keyed by the same string, kept in step by hand — upstream has a
test whose only job is to check that they are.

Here each declaration carries its own setter, so name, type, path-ness and
destination are one entry. There is nothing to keep in step, and the test
upstream needs does not need writing.

**What we accepted:** a function pointer per option is slightly more machinery
than a match arm, and the table cannot be `const`-folded into a perfect hash the
way a generated switch could. Neither matters at one lookup per option per
config.

**How we would know this was wrong:** if the setters start needing context the
signature does not carry — `parseTypeAcquisition` and the watch options both
write into *different* structs — the single `fn(&mut CompilerOptions, …)` shape
breaks and the table has to gain a parent-option dimension, which is what
upstream's `parentOption` argument already is.

## Decision 3: the glob engine is ported into `tsr-vfs`, not adopted

`include`/`exclude` are not POSIX globs and not `.gitignore` patterns. Three
rules make them their own language, and each changes which files a program
contains:

- `**` in an *include* refuses to descend into `node_modules`,
  `bower_components`, `jspm_packages` or any dot-directory; the same `**` in an
  *exclude* descends. A pattern means different things in the two lists.
- A leading wildcard does not match a dotfile.
- `*.js` does not match `foo.min.js` unless the pattern mentions `.min.`.

No general-purpose matcher has these, and a project that compiled its own
`node_modules` would not fail loudly — it would just be wrong and slow. So it is
a direct port of `internal/vfs/vfsmatch`, ~450 lines of real logic, living in
`tsr-vfs` because that is where upstream puts it and because it is about the file
system rather than about options.

Upstream itself used to compile these to regular expressions and stopped; that
history is why "just build a regex" is not the cheap option it looks like.

**Kept from upstream, deliberately:** the matcher is iterative with a single
backtrack point, O(n·m) rather than exponential. A pattern like `*a*a*a*b` in a
real `tsconfig.json` would otherwise be a denial of service on the repository
containing it.

**Dropped from upstream:** the visitor's incremental real-path computation, which
avoids a `Realpath` call for a directory already known not to be a symlink. It
needs the file system to report which entries are symlinks, which
`DirectoryEntries` does not, and it is an optimisation with the same answer.

## What this found

Wiring `paths` in for the first time turned up a latent bug in `tsr-module` that
no directive-configured case could reach, because a `paths` table can only be
written in a config file.

`core.Pattern.StarIndex` is a Go `int` where `-1` means "exact match", so its
**zero value is `0`**, and `IsValid()` rejects it (`0 < len("")` is false). The
port maps `-1` onto `Option::None` — correct for the meaning, wrong for the zero
value: `derive(Default)` produced `star_index: None`, which reads as a *valid*
exact pattern matching the empty string. `MatchPatternOrExact` returning "nothing
matched" therefore came back valid, and `MatchedText` asserted on a candidate
that does not match. The corpus run panicked on the first `paths` case.

`Default` is now hand-written to be the Go zero value, and the workaround
condition that had grown next to `is_valid()` at the call site is gone.

The general lesson, which is not specific to this bug: **mapping a Go sentinel
onto an `Option` changes the zero value**, and any `derive(Default)` on the
resulting struct is a claim that nobody checked.

## Consequences accepted

- **`extends` is not supported.** 6 of the corpus's 130 `tsconfig.json` units use
  it, and none of the 20 the resolution baselines judge. It needs a resolution
  stack, circularity detection, and — in its package form
  (`"extends": "@tsconfig/node16/tsconfig.json"`) — a module resolution of its
  own, which would make config parsing depend on `tsr-module` and give config
  parsing its own entries in the resolution trace. bd tsr-9or.6.
- **Project references, the command line, `--init` and `--showConfig` are
  absent**, with the rest of the machinery that needs them.
- **Config diagnostics are collected but not judged.** The messages are
  upstream's; the spans are the offending value's rather than upstream's exact
  `CreateDiagnosticForNodeInSourceFile` range. No suite reads them, so the
  difference is currently invisible — which is precisely why it is written down.
- **The option table is short.** It declares what `CompilerOptions` has a field
  for. Reading a config that sets `noImplicitOverride` reports an unknown option
  where upstream would accept it. That is visible only in diagnostics today, and
  the table grows when a field does.

## What this supersedes

ADR-0019 pinned an arithmetic tripwire — *"`file_loader`'s denominator drifts
from `module_resolution`'s by anything other than `+3 empty-trace, −2
libReplacement`"*. That relation still holds in form and has changed in value:
it is now **`+5 −4`**, because two more empty-trace cases and two more
`libReplacement` cases became visible once their configs could be read. The
tripwire is not weakened — it is asserted in
[module-resolution.md](../architecture/module-resolution.md) and by
`the_skip_rule_explains_every_missing_baseline`, which also gained the two new
empty-trace cases by name.

The mutation table in ADR-0019 was measured over 76 cases and is not re-run here;
the four mutations used for *this* slice are:

| Mutation | `module_resolution` | `file_loader` |
|---|---|---|
| `compilerOptions` never read | 91/95 | 94/114 |
| root files ignore the config's file list | 95/95 | 92/96 |
| path options not made absolute | 94/95 | 95/96 |
| include `**` descends into `node_modules` | 95/95 | 93/96 |

The second column of the first row is the point of the exercise: dropping config
options does not merely fail cases, it moves the **denominator**, because
upstream's skip predicate stops seeing the `baseUrl` and `outFile` those configs
declare. A rate that rises while its denominator grows is the shape of a
measurement bug, and it is why both numbers are reported together.

## How we would know this was wrong

- **A corpus case needs `extends`.** The moment one of the 146 trace baselines
  does, the "6 of 130, none of them judged" argument expires and the resolution
  stack has to be built.
- **A config diagnostic becomes an oracle.** `.errors.txt` baselines include
  config errors; when a suite starts judging them, the span approximation here
  becomes a real failure rather than a stated gap.
- **The two suites' denominators stop differing by the documented step.** They
  now share `crate::trace_case` for setup precisely so they cannot drift for any
  reason other than a suite's own stated limitation; if they do, the shared
  setup is the suspect.
- **`tsr-tsoptions` starts being imported by something that only needs
  `CompilerOptions`.** ADR-0017's separation — `program.go` imports `core`, not
  `tsoptions` — is what keeps the config parser optional. `tsr-compiler` importing
  it for `GetSupportedExtensions` is upstream's own arrangement
  (`fileloader.go` does exactly that); anything beyond that is drift.
