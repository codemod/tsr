import { readFile } from 'node:fs/promises';

import type { Reading } from './measure.ts';

/**
 * Build the prompt for one iteration.
 *
 * # Why this is generated rather than stored
 *
 * The repo used to carry a hand-written loop prompt at
 * `.claude/ralph-loop.local.md`, deleted when this loop replaced it. It opened
 * with *"State at HEAD 78cfcba"* and a table reading
 * `checker_types 596/9,538, gradient 36.17%` — true when written and wrong by
 * more than thirty points by the time it went. A stored prompt goes stale
 * silently, and a loop that feeds a stale prompt to a fresh agent spends its
 * first turns re-deriving numbers that the prompt asserted.
 *
 * So the loop states **only what it measured itself**, this iteration, and for
 * everything else points at `STATUS.md` — which the project's own conventions
 * already require to be current, and which is the handoff every session in
 * this repo is written to consume.
 *
 * # Why a fresh session per iteration
 *
 * Not `resume`. The project's method is "read `STATUS.md`, it is the
 * dashboard"; a fresh context each iteration is how a human session starts and
 * is what keeps the loop from accumulating context rot over a long run. The
 * continuity lives in the repo — `STATUS.md`, the findings pages, the `bd`
 * issues — which is exactly where this project has spent three sessions
 * putting it.
 */
export async function buildPrompt(
  repo: string,
  reading: Reading,
  target: number,
  history: Reading[],
): Promise<string> {
  const status = await readFile(`${repo}/STATUS.md`, 'utf8');
  const board = extractSection(status, '## 4. What is next');

  const remaining = Math.max(0, Math.ceil((target / 100) * reading.total - reading.matched));
  const trend =
    history.length > 1
      ? history
          .slice(-5)
          .map((r) => `${r.gradient.toFixed(2)}%`)
          .join(' → ')
      : 'first measured iteration of this run';

  return `Continue the tsr port. The standing goal is **${target}% on the \`checker_types\` gradient**.

## Measured just now by the loop, not reported by anyone

    gradient   ${reading.gradient.toFixed(2)}%   (${reading.matched.toLocaleString()} / ${reading.total.toLocaleString()} assertion lines)
    cases      ${reading.cases.toLocaleString()} / ${reading.caseTotal.toLocaleString()}
    commit     ${reading.commit}
    to target  +${remaining.toLocaleString()} lines

Recent gradient across this loop's iterations: ${trend}

These figures come from \`cargo run --release -p tsr-conformance --bin coverage\`,
run by the loop itself before this prompt was written. Trust them over any
number in a document, and if a document disagrees, correct the document — that
is one of this project's standing rules and the loop cannot do it for you.

## Read first

\`STATUS.md\` is the live dashboard and is the handoff. Read it before anything
else, especially §4 (the ranked board) and §5 (what is already refused, with
the number that refused it). Then read whichever findings page in
\`docs/architecture/\` the board points at for the item you pick.

Its board currently reads:

${board}

## What one iteration is

Pick **one** item, and prefer the one the board ranks highest unless you can
say in a sentence why it is wrong. Then:

1. **Size it before building it**, through the mechanism that would actually
   convert the lines — not through the population's size. This project has
   several recorded cases of a number that was true of one set and quoted about
   another, and the most recent cost a registered bar.
2. **Register a keep/revert bar in \`docs/\` in its own commit, before the
   build.** The last two builds skipped this and both had to be judged after
   the fact. A bar written afterwards is not a bar.
3. Build it, with the five gates green: \`cargo fmt --all\`,
   \`cargo clippy --workspace --all-targets -- -D warnings\`,
   \`cargo test --workspace\` (counted with \`grep -c\`, never sampled with
   \`head\`), \`cargo run -p xtask -- anchors\`, \`cargo run -p xtask -- issue-ids\`.
4. **Score it against the bar you registered** using \`examples/casedelta.rs\`
   for per-case gain/loss, \`examples/depend.rs\` for the gap, and
   \`examples/wrongdelta.rs\` for gap→wrong, which \`casedelta\` cannot see.
5. If a leg fires: the first hypothesis is the build is wrong, the second is
   the bar's premise is wrong, and there is no third. Either is a result worth
   recording. **A refusal with a number is a successful iteration** — it stops
   the next one paying for the same negative.
6. Update \`STATUS.md\` (§1 numbers with the commit, §4 what moved, §5 anything
   newly refused with its number, one appended row in §7), close or file the
   \`bd\` issues, then **commit and push**. Work is not complete until
   \`git push\` succeeds.

Do not attempt several items at once, and do not leave the tree dirty — the
loop starts the next iteration from a fresh context and the repo is the only
thing that carries forward.`;
}

/** The named section of `STATUS.md`, up to the next `## ` heading. */
function extractSection(markdown: string, heading: string): string {
  const start = markdown.indexOf(heading);
  if (start === -1) return '(section not found — read STATUS.md directly)';
  const rest = markdown.slice(start);
  const end = rest.indexOf('\n## ', heading.length);
  const section = end === -1 ? rest : rest.slice(0, end);
  // Trimmed hard: the board is the part that steers, and pasting the whole
  // file would crowd out the instructions above it.
  return section.length > 4000 ? `${section.slice(0, 4000)}\n\n…(truncated — read STATUS.md)` : section;
}
