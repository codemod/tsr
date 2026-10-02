# Enum and module value members

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline: fd9b42be, evidence dbf52fe8, 455,694/478,855 matching assertions.

`resolveAnonymousTypeMembers` in checker.go reads exports for enum and module
values. The port's property enumeration admitted only callable anonymous types;
index inference used the narrower spelling-suggestion enumeration. Consequently
`strings(TextEnum)` inferred string from a spurious reverse index, and spreading
an enum or namespace failed. The shared enumeration now admits enum/module
exports while retaining callable capture handling. Index inference uses that
semantic enumeration and retains its existing fallback for unrepresented shapes.
Classes continue through their separate static-member path.

Native enum reverse indexes exist when the declared type carries Enum or an
exported property has a number-like type. String-only enums have none. Const
enums also have these semantic indexes, even when a runtime use reports TS2475.
The previous regular-enum-only gate conflated runtime legality and type shape.
`checkElementAccessExpression` separately rejects const-enum access whose index
is not string-literal-like syntax; those accesses recover as any. Both the index
shape and this early error path are required together.

`formatUnionTypes` in printer.go collapses a complete contiguous enum-member run
using regular literal identities. The port now applies that display rule without
changing semantic constituents or freshness. This handles both fresh unions and
complete enum runs inside larger unions. Partial runs remain explicit members.

The first candidate gained 28 matches but lost 323 RIGHT rows: it replaced the
callable representation gate with symbol flags, dropping callable type literals,
and exposed const reverse indexes before porting the access rejection. Restoring
the callable gate and adding the native syntactic rejection gives 28 gains with
zero RIGHT losses. Four GAP→WRONG and 19 changed WRONG remain, chiefly namespace
qualification/method serialization and empty index-inference candidates. This is
not a claim that those rows are correct. No expectations or denominator changed.

Native controls: /tmp/tsr-99-enum-static.ts and emitted declarations in
/tmp/tsr-99-enum-static-native, explicitly --strict true --target esnext.
The new pipeline test pins 14 native outcomes across numeric/string/mixed/empty/
const enums, enum and module spreads, allowed const accesses and error recovery.
Negative probes retain text-only enum numeric inference (native unknown), and
namespace mixed-literal inference (native string | number, port string | 1).
Nonlocal enum-key interface spreads and qualified display remain for tsr-8.
A regression in any previously RIGHT corpus row falsifies this implementation.

Primary-thread simplification reviewed reuse, clarity and efficiency; no further
refactor was justified. Review is sequential under the user's no-delegation
instructions; no independent or cross-model review is claimed. Final committed
verification and mutation results follow below when measured.

Frozen source hash (checker sources and trace_case):
03c719d33f99f64b0b2272b43e0c56963583b0172431560169dc2d32693e3cdd.
Candidate verdict: /tmp/tsr-99-enum-static-candidate2.tsv; transition report:
/tmp/tsr-99-enum-static-delta2.txt. All 207 release workspace result blocks,
clippy, formatting and 3,331 anchors pass. Review receipt:
/tmp/compound-engineering-501/ce-code-review/enum-static/review.json.

Committed verification at b51228ae in /tmp/tsr-99-enum-static-verify is
byte-identical to the candidate verdict and has the same source hash. Coverage:
455,722/478,855 RIGHT (95.17%), 6,896/9,538 complete cases (72.30%). This is one
more complete case; 18,345 matches remain to 99%. All 207 workspace result blocks,
clippy, formatting and 3,331 anchors pass in the isolated checkout. Fresh depend:
505 non-gapping roots, 210 cycles, zero depth caps and 3,612 walked gaps; C3
balances while C1/C4 remain stale (tsr-6.29). The checker snapshot is copied
from this committed checkout. Logs: /tmp/tsr-99-enum-static-verified-*.log;
verdict: /tmp/tsr-99-enum-static-verified.tsv.

Four isolated mutations compile and fail their intended assertions: omitting
enum/module enumeration fails numericString; giving string enums a reverse index
fails textString; disabling enum-run printing fails numericString; bypassing the
const access syntax guard fails reverse. Each mutation restores its source in a
finally block; the final source hash equals the committed hash. Script:
/tmp/tsr-99-enum-static-mutations.py, logs:
/tmp/tsr-99-enum-static-mutation-{enumeration,reverse,printing,const_access}.log.

All seven focused tests pass after restoration:
/tmp/tsr-99-enum-static-restored-tests.log.
