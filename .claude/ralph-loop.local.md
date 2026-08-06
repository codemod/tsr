---
active: true
iteration: 1
session_id: 8bd82f76-2312-4b3b-9969-b84bf0914387
max_iterations: 0
completion_promise: null
started_at: "2026-08-05T07:30:58Z"
---

Continue the tsr port. Target: `checker_types` gradient 90%. Work as an agent
team, in a Ralph loop, and re-rank from the histogram every cycle.

Run `bd prime`. Read, in this order:
  docs/architecture/checker.md — the whole file, but especially
    "The re-ranking, 2026-08-05" and the divergence notes under each slice
  docs/architecture/checker-oracle.md — the instrument, the mutation records,
    and "The instrumentation invented a finding, and was caught by reading it"
  docs/conventions.md, and the anchoring rule in CLAUDE.md

## State at HEAD 78cfcba, upstream pinned at 5b1047d10

    parser_typescript        5,000/5,031    99.38%
    binder_symbols           8,292/8,459    98.03%
    printer_round_trip      11,681/11,737   99.52%
    checker_types              596/9,538     6.25%   gradient 36.17%
    diagnostics                 80/5,488     1.46%   ceiling 90.9%

The gradient went 22.39% → 36.17% in one session across four slices, each
ranked by `cargo run -p tsr-conformance --example types_shapes --release`.
That example is the instrument for everything below. Run it first; the numbers
in the docs were true at 78cfcba and your first job is to re-take them.

## What "90%" means, exactly

The **gradient** — matched assertion lines over the 468,921 the walker aligns.
Not the case rate, which is the gate and will trail far behind it; not
"percent of checker done", which nothing here measures. Report both, always,
and never quote the gradient as a pass rate. If you find yourself explaining
why a number went up, you have the wrong number.

90% is a long way from 36%. Everything currently ranked, if it all landed
perfectly, is worth roughly 70–80%. Expect to run out of ranked work and have
to re-derive more from the histogram. That is the loops job, not a surprise.
