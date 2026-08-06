import { appendFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

import { ClaudeSDKAgent } from '@mastra/claude';

import { measure, repoState, type Reading } from './measure.ts';
import { buildPrompt } from './prompt.ts';

type Config = {
  /** Absolute path to the tsr checkout the loop drives. */
  repo: string;
  /** Stop once the gradient reaches this percentage. */
  target: number;
  /** Hard cap on iterations, whatever else happens. */
  maxIterations: number;
  /** Give up after this many consecutive iterations with no gradient gain. */
  stallLimit: number;
  /** Minimum gain, in lines, that counts as progress rather than noise. */
  minGain: number;
  /** Model to drive the agent with. */
  model: string;
  /** Build the prompt and print it, but never invoke the agent. */
  dryRun: boolean;
  /**
   * Measure by running the conformance binary rather than reading the
   * committed snapshot.
   *
   * Defaults to true and should stay that way for a real run: a snapshot is
   * only as fresh as the last time someone ran the gates, so an iteration that
   * skipped them would read as an iteration that made no progress — and the
   * loop would then count it toward the stall limit for the wrong reason.
   * Turning it off is for dry runs, where waiting minutes for a number nothing
   * acts on is pure cost.
   */
  liveMeasure: boolean;
};

function config(): Config {
  const repo = resolve(process.env.TSR_REPO ?? resolve(import.meta.dirname, '../../..'));
  return {
    repo,
    target: Number(process.env.PARITY_TARGET ?? 70),
    maxIterations: Number(process.env.PARITY_MAX_ITERATIONS ?? 10),
    stallLimit: Number(process.env.PARITY_STALL_LIMIT ?? 3),
    minGain: Number(process.env.PARITY_MIN_GAIN ?? 50),
    model: process.env.PARITY_MODEL ?? 'claude-fable-5',
    dryRun: process.env.PARITY_DRY_RUN === '1',
    liveMeasure: process.env.PARITY_LIVE_MEASURE !== '0',
  };
}

const LOG = 'temp/parity-loop/runs';

async function record(cfg: Config, runId: string, entry: unknown): Promise<void> {
  await mkdir(`${cfg.repo}/${LOG}`, { recursive: true });
  await appendFile(`${cfg.repo}/${LOG}/${runId}.jsonl`, `${JSON.stringify(entry)}\n`);
}

/**
 * The agent that does one iteration's work.
 *
 * `sdkOptions` matter more than they look:
 *
 * - **`cwd`** points at the tsr checkout, which is what makes the agent's
 *   relative paths and `cargo` invocations land in the right tree.
 * - **`settingSources: ['project']`** is required for `CLAUDE.md` to load at
 *   all. Without it the agent runs without this repo's instructions — the
 *   anchoring rule, the documentation rule, the session-completion protocol —
 *   which are most of what makes its output usable here.
 * - **`permissionMode: 'acceptEdits'`** lets it edit and run without a human
 *   at the keyboard, which is the entire point of the loop. It is deliberately
 *   not `bypassPermissions`: the loop is meant to run unattended on a repo
 *   whose remote it can push to, and that is enough capability already.
 * - **`maxTurns`** bounds a single iteration so one confused run cannot
 *   consume the whole budget before the loop regains control.
 */
function makeAgent(cfg: Config): ClaudeSDKAgent {
  return new ClaudeSDKAgent({
    id: 'tsr-parity',
    name: 'tsr parity loop',
    description:
      'Drives one iteration of the typescript-go → Rust port toward the checker_types gradient target.',
    sdkOptions: {
      cwd: cfg.repo,
      model: cfg.model,
      permissionMode: 'acceptEdits',
      settingSources: ['project', 'user'],
      maxTurns: Number(process.env.PARITY_MAX_TURNS ?? 400),
    },
  });
}

async function main(): Promise<void> {
  const cfg = config();
  const runId = new Date().toISOString().replace(/[:.]/g, '-');
  const agent = makeAgent(cfg);
  const history: Reading[] = [];

  console.log(`parity-loop → ${cfg.repo}`);
  console.log(`target ${cfg.target}%  max ${cfg.maxIterations} iterations  stall limit ${cfg.stallLimit}`);

  let stalled = 0;

  for (let iteration = 1; iteration <= cfg.maxIterations; iteration += 1) {
    const before = await measure(cfg.repo, cfg.liveMeasure);
    history.push(before);
    console.log(
      `\n── iteration ${iteration} — ${before.gradient.toFixed(2)}% ` +
        `(${before.matched.toLocaleString()}/${before.total.toLocaleString()}) at ${before.commit}`,
    );

    if (before.gradient >= cfg.target) {
      console.log(`\n✓ target reached: ${before.gradient.toFixed(2)}% ≥ ${cfg.target}%`);
      await record(cfg, runId, { iteration, event: 'target-reached', before });
      return;
    }

    const prompt = await buildPrompt(cfg.repo, before, cfg.target, history);

    if (cfg.dryRun) {
      console.log('\n--- DRY RUN, prompt follows ---\n');
      console.log(prompt);
      return;
    }

    await record(cfg, runId, { iteration, event: 'start', before });

    let failure: string | undefined;
    try {
      // The vendor SDK owns the inner agent loop; this await covers one whole
      // iteration of work, which is minutes to hours.
      const result = await agent.generate(prompt);
      await record(cfg, runId, { iteration, event: 'agent-finished', text: result.text?.slice(0, 4000) });
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
      console.error(`  agent failed: ${failure}`);
      await record(cfg, runId, { iteration, event: 'agent-failed', error: failure });
    }

    const after = await measure(cfg.repo, cfg.liveMeasure);
    const state = await repoState(cfg.repo);
    const gained = after.matched - before.matched;

    console.log(
      `  → ${after.gradient.toFixed(2)}% (${gained >= 0 ? '+' : ''}${gained.toLocaleString()} lines) ` +
        `at ${after.commit}${state.clean ? '' : '  ⚠ tree dirty'}${state.pushed ? '' : '  ⚠ unpushed'}`,
    );
    await record(cfg, runId, { iteration, event: 'measured', after, gained, ...state, failure });

    // Progress is judged on **lines the loop measured**, never on what the
    // agent said it did. A refusal-with-a-number is a good iteration and moves
    // nothing, which is exactly why the stall limit is a small count of
    // consecutive flat iterations rather than a single one.
    if (gained >= cfg.minGain) {
      stalled = 0;
    } else {
      stalled += 1;
      console.log(`  no material gain (${stalled}/${cfg.stallLimit} before the loop stops)`);
      if (stalled >= cfg.stallLimit) {
        console.log(
          `\n■ stopping: ${cfg.stallLimit} consecutive iterations under +${cfg.minGain} lines.\n` +
            `  This is the designed outcome when the ranked work runs out — read the last\n` +
            `  few entries in ${LOG}/${runId}.jsonl and STATUS.md §4 before restarting.`,
        );
        await record(cfg, runId, { iteration, event: 'stalled' });
        return;
      }
    }

    if (!state.clean || !state.pushed) {
      console.log(
        '  ⚠ the iteration left work uncommitted or unpushed; the next iteration starts\n' +
          '    from a fresh context and will not know about it.',
      );
    }
  }

  console.log(`\n■ stopping: reached the ${cfg.maxIterations}-iteration cap.`);
  await record(cfg, runId, { event: 'max-iterations' });
}

await main();
