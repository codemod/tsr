# parity-loop

An unattended loop that keeps driving the tsr port toward a `checker_types`
gradient target, so the work does not need a human re-prompting it each cycle.

Built on [`@mastra/claude`](https://mastra.ai/docs/agents/sdk-agents.md), which
wraps the Claude Agent SDK. Mastra does **not** own the agent loop here — the
vendor SDK does — so this package is deliberately thin: it measures, it builds
a prompt, it runs one iteration, it measures again, and it decides whether to
go round.

## Run it

```bash
cd temp/parity-loop
npm install

npm run dry-run     # print the prompt it would send, invoke nothing
npm run measure     # one independent reading of the gradient
npm start           # the loop
```

Configuration is by environment variable, all optional:

| variable | default | meaning |
|---|---|---|
| `PARITY_TARGET` | `70` | stop once the gradient reaches this |
| `PARITY_MAX_ITERATIONS` | `10` | hard cap whatever else happens |
| `PARITY_STALL_LIMIT` | `3` | stop after this many flat iterations |
| `PARITY_MIN_GAIN` | `50` | lines that count as progress rather than noise |
| `PARITY_MAX_TURNS` | `400` | bound on a single iteration |
| `PARITY_MODEL` | `claude-fable-5` | |
| `PARITY_LIVE_MEASURE` | `1` | `0` reads the committed snapshot instead of running the suite |
| `TSR_REPO` | the repo above this directory | |

## The three decisions that matter

Everything else is plumbing; these are the parts worth disagreeing with.

### 1. The loop measures the gradient itself and never reads it from the agent

Each iteration runs `cargo run --release -p tsr-conformance --bin coverage` and
parses the summary. It never extracts a number from the agent's prose, and the
stall detector is driven entirely by lines the loop measured.

This is not distrust of the model, it is this project's own recorded history.
`STATUS.md` carries several corrections where a figure was true of one
population and quoted about another — a ceiling estimate wrong by 4×, then
wrong again by 12× in the other direction; a wrong-bucket count that made a
build look like it had failed its bar by 20 lines when it had passed. A loop
that let the agent self-report progress would compound exactly that failure,
and would do it unattended.

The cost is a few minutes per iteration against an iteration measured in hours.

### 2. The prompt is generated fresh every iteration, from `STATUS.md`

This repo already contains a hand-written loop prompt at
`.claude/ralph-loop.local.md`. It opens with *"State at HEAD 78cfcba"* and a
table reading `gradient 36.17%`. Both were true when it was written and the
gradient is now above 69%. A stored prompt rots silently, and a stale prompt
fed to a fresh agent burns its first turns re-deriving numbers the prompt
asserted.

So the loop asserts **only what it measured itself this iteration** and points
at `STATUS.md` for everything else — which the project's conventions already
require to be current, and which every session here is written to consume as
the handoff. The board section is spliced into the prompt so the agent starts
from the ranking rather than rediscovering it.

### 3. A fresh session per iteration, not `resume`

The SDK offers `resume`/`continue`, and this does not use them. Continuity in
this project lives in the repo — `STATUS.md`, the findings pages under
`docs/architecture/`, the `bd` issues — because three sessions have been spent
deliberately putting it there. Starting fresh is how a human session starts
here, and it avoids accumulating context rot across a run that may be dozens of
iterations long.

## What "no progress" means, and why the loop stops

The stall detector needs a subtlety that a naive loop gets wrong: **an
iteration that converts nothing can be a good iteration.** This project treats
a refusal-with-a-number as a real result, because it stops the next session
paying for the same negative — several entries in `STATUS.md` §5 cost a full
cycle each to establish.

So the loop does not stop on the first flat iteration. It stops after
`PARITY_STALL_LIMIT` *consecutive* ones, on the reasoning that a run producing
nothing but refusals for three iterations has exhausted its ranked work and
needs a human to re-derive more from the histogram. That is a designed outcome,
not a failure — read the tail of `runs/<id>.jsonl` and `STATUS.md` §4 before
restarting it.

## What it does not do

- **It does not judge whether a build was good.** The bar-registration
  discipline lives in the prompt and in `docs/conventions.md`; the loop only
  checks that lines moved. It cannot tell a well-scored keep from a lucky one.
- **It does not enforce the gates.** It reports a dirty or unpushed tree after
  each iteration, because the next iteration starts from a fresh context and
  will not know about uncommitted work, but it does not block on it.
- **It has no budget accounting.** `PARITY_MAX_ITERATIONS` and
  `PARITY_MAX_TURNS` are the only cost bounds, and they are blunt. Set them
  deliberately before an unattended run.
- **It pushes to the remote**, because the agent does, under this repo's
  session-completion rule. Point `TSR_REPO` at a checkout whose remote you are
  happy to have written to.
