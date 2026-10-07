# Current CLI launch and member-builder qualification

At frozen TSR `bb982558a98d8a3ee7e2ddfa4876ee22b5186251`, this Mac reproduces a
20–30x CLI delay **before Rust `main`**. Fresh executable copies take seconds to
start; repeats enter in milliseconds. Scoped macOS logs show XProtect scan
results finishing just before entry. This diagnoses the local reproduction,
including outside the agent sandbox. It does not establish PR #5 source
causality or the user's exact failing invocation; `tsr-1yb.34` remains open.

The [receipt](checker-member-builder-current.json) retains complete outputs,
commands, failed attempts, source/build/input hashes and private driver sources.
The [13-file member replay](checker-member-builder-current.patch) is a rejected
private experiment, not a production fix. No canonical runtime changes,
security changes or speed improvement are retained.

## Launch delay

Two byte-identical fresh ordinary TSR copies check the committed
`domain-model-large` project in 10.865/11.179s, then 0.530/0.542s on repeats.
The private entry probe timestamps the first statement of `main`; the parent
records spawn start and return. Spawn returns in approximately 1ms, while entry
waits 6.66–15.55s in fresh-copy trials across sandboxed and unsandboxed runs.
Repeats enter in approximately 3ms. The slowest outside run is 16.141s versus
0.556s for its repeat; internal compilation is 0.583s versus 0.549s. Both
perform the public check and report the same intentional TS2322.

Version-only outside controls remove checking altogether:

| Executable | Fresh copy | Repeat |
| --- | ---: | ---: |
| TSR `--version` |7.272s|0.00365s|
| Pinned tsgo `--version` |8.429s|0.00713s|
| Already warmed TSR renamed, same inode |0.0624s|0.00282s|

The two own-path `syspolicyd` events report `GK Xprotect results` at
11:08:57.799596 and 11:09:08.650565 on 2026-10-06, immediately before the
corresponding delayed entry probes. Apple describes first-launch and changed-app
scans in [Platform Security](https://support.apple.com/en-gb/guide/security/sec469d47bd8/web).
Together these observations support an OS launch-scan explanation; no scan
duration guarantee or PR-specific mechanism is inferred. Valid linker ad-hoc
signatures and provenance metadata are observed, without changing either.
The tested machine is macOS 15.7.9 /24G830.

For the user's real project, build once and time the same executable twice:

```sh
time ~/dev/codemod/tsr/target/release/tsr --version
time ~/dev/codemod/tsr/target/release/tsr --version
~/dev/codemod/tsr/target/release/tsr --extendedDiagnostics
```

Run the last command inside the project directory. A slow first `--version`
followed by a fast repeat isolates startup; slow repeat project checks require
separate phase attribution. The manifest checkout `~/dev/codemod/tsr` is distinct
from this workspace. At read time it is `0b18d357`, behind 15 commits, with an
older release binary and unrelated local edits. It is neither updated nor
rebuilt by this investigation, and its HEAD does not certify that binary's
source.

## Current member replay fails the first public gate

The prior 12-file native member/signature replay applies cleanly to `bb982558`
and builds with pinned Rust 1.96.0. It adds **799 false TS2445 errors** to the
large public project. The complete failed output is preserved and timing
comparison stops before measured pairs. Its first 7.708s wall includes a fresh
launch delay and is not a performance comparison against warmed main.

A fixed imported-base control is accepted by native and frozen main in both
worker modes, but rejected by the replay. Outside-class access still produces
the required TS2445. The visibility walk resolves an imported base identifier
to an ALIAS symbol, then requires CLASS without following its target. Publishing
the inherited original property exposes this older visibility gap.

The narrow private repair uses existing `resolve_alias_fully` and merged-symbol
resolution before the class-declaration check. Native protected access at pinned
`5b1047d` uses `isClassDerivedFromDeclaringClasses` / `hasBaseType`
(`checker.go:11978` / `checker.go:19551`); the repair retains the port's existing
bounded identifier-only walk rather than claiming the whole native base-type
algorithm. No accessibility error is suppressed.

Twelve original native/main/replay CLI children qualify the red; 30 repaired
children cover direct and re-exported imported bases, local inheritance,
outside-class TS2445 and base-typed receiver TS2446 in default/single modes.
All repaired complete diagnostic/exit pairs agree with native. The receiver
control corrects main's TS2445 to native TS2446. These bounded checks do not
certify the broader member state machine or current full corpora.

## Warm checking still rejects retention

The original eight-child baseline mixes fresh first launch and warmed repeat;
it is preserved and excluded from checker performance comparisons. A separate
prospective protocol runs one warmup then two alternating measured pairs per
mode. Ordinary TSR/native medians are 0.553/0.169s default and 1.259/0.269s
single. They are locating observations, not a verified complete-work ratio.

The repaired member replay uses another same-source alternating batch:

| Warm public median | Baseline | Repaired replay |
| --- | ---: | ---: |
| Default wall |1.196s|1.384s|
| Default user CPU |2.007s|2.291s|
| Default peak RSS |136.10MB|172.49MB|
| Single wall |3.015s|3.289s|
| Single user CPU |2.522s|2.656s|
| Single peak RSS |134.85MB|171.35MB|

All 12 children preserve the one intentional diagnostic, ordered loaded files
and observed physical inputs. Both compilers report 265 loaded /202 checked
files in the native baseline batch; actual checked identities and complete
filesystem query inputs are not verified. Other agents' host activity is
uncontrolled, and baseline times shifted substantially between batches. Do not
compare the repaired replay against the earlier 0.553s or add percentages across
batches. The observed default wall change is +15.76%; the decision helper rejects
retention. No winning change needs confirmation, and no current full package,
full type/diagnostic corpus or strict lint run is claimed for this rejected
experiment. The unused `MemberLinks.containing` warning also remains.

Builds bind 632 Rust files under the archived compiled crates/test/xtask paths,
not a whole-repository Rust census, and 108 embedded libraries. An initial
missing toolchain pin produced a 1.89.0 build; it was excluded before any
measurement and corrected to 1.96.0. Failed library-path, sandboxed log-read and
unsupported xattr API attempts remain separately recorded. The entry probe is
fully restored before member replay builds. Fresh 13-file patch replay applies
and reproduces all final hashes exactly. The receipt reader recomputes medians,
checks all ordered loaded lists and complete native control outputs. Its first
attempt excluded embedded library lines and failed the count assertion; the
corrected reader consumes the original outputs without rerunning compilers.
Documentation checks resolve 4,495 upstream references with zero unresolved and
zero dangling section citations. The issue-ID gate fails on 191 historical IDs;
their containing files are byte-identical to frozen main, and all new task
references resolve. No global issue-ID pass is claimed.

`tsr-1yb.33.1` and builder `tsr-1yb.4.2.1` remain unfinished. The next builder
choice needs actual field-population/consumer cost and retained-storage evidence
before another implementation; the existing reference-cache rejection still
stands. The complete equivalent-work TSR/pinned-tsgo median wall target of at
most 0.50 remains unmet. The launch diagnosis is progress on the user's
regression report, not completion of that speed target.
