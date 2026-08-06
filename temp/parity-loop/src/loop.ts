import { appendFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';

import { ClaudeSDKAgent } from '@mastra/claude';

import { measure, repoState, type Reading } from './measure.ts';
import { buildPrompt } from './prompt.ts';
import {
  newStats,
  observerHooks,
  reportStream,
  statsForLog,
  summarise,
  toolBreakdown,
  type IterationStats,
} from './report.ts';

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
  /** Bound on a single iteration's turns. */
  maxTurns: number;
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
    maxTurns: Number(process.env.PARITY_MAX_TURNS ?? 400),
    dryRun: process.env.PARITY_DRY_RUN === '1',
    liveMeasure: process.env.PARITY_LIVE_MEASURE !== '0',
  };
}

const LOG = 'temp/parity-loop/runs';

const DIM = '\u001b[2m';
const BOLD = '\u001b[1m';
const RED = '\u001b[31m';
const GREEN = '\u001b[32m';
const RESET = '\u001b[0m';

/** `1h 04m 12s`, so a watcher can see how long an iteration actually took. */
function fmtDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`;
  if (m > 0) return `${m}m ${String(s).padStart(2, '0')}s`;
  return `${s}s`;
}

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
function makeAgent(cfg: Config, stats: IterationStats): ClaudeSDKAgent {
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
      maxTurns: cfg.maxTurns,
      // The only route to per-tool visibility: the adapter's chunk stream
      // carries no tool events, measured by probing a real run. The hooks
      // close over this iteration's `stats`, which is why the agent is
      // constructed per iteration rather than once for the whole loop.
      hooks: observerHooks(stats),
    },
  });
}

async function main(): Promise<void> {
  const cfg = config();
  const runId = new Date().toISOString().replace(/[:.]/g, '-');
  const history: Reading[] = [];

  console.log(`${BOLD}parity-loop${RESET} → ${cfg.repo}`);
  console.log(
    `${DIM}run ${runId} · target ${cfg.target}% · max ${cfg.maxIterations} iterations · ` +
      `stall after ${cfg.stallLimit} under +${cfg.minGain} lines · model ${cfg.model}${RESET}`,
  );
  console.log(`${DIM}log temp/parity-loop/runs/${runId}.jsonl${RESET}`);
  if (!cfg.liveMeasure) {
    console.log(`${DIM}⚠ measuring from the committed snapshot, not a live run${RESET}`);
  }

  let stalled = 0;

  for (let iteration = 1; iteration <= cfg.maxIterations; iteration += 1) {
    console.log(`\n${BOLD}── iteration ${iteration}/${cfg.maxIterations}${RESET} ${DIM}measuring…${RESET}`);
    const before = await measure(cfg.repo, cfg.liveMeasure);
    history.push(before);
    console.log(
      `  ${BOLD}${before.gradient.toFixed(2)}%${RESET} ` +
        `${before.matched.toLocaleString()}/${before.total.toLocaleString()} lines · ` +
        `${before.cases.toLocaleString()}/${before.caseTotal.toLocaleString()} cases · ` +
        `${before.commit} · ${DIM}${before.source}${RESET}`,
    );

    if (before.gradient >= cfg.target) {
      console.log(`\n${GREEN}✓ target reached: ${before.gradient.toFixed(2)}% ≥ ${cfg.target}%${RESET}`);
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
    console.log(`  ${DIM}prompt ${prompt.length.toLocaleString()} chars · model ${cfg.model} · max ${cfg.maxTurns} turns${RESET}`);
    const started = Date.now();

    let failure: string | undefined;
    const stats = newStats();
    const agent = makeAgent(cfg, stats);
    try {
      // **`stream`, not `generate`.** The vendor SDK owns the inner agent loop
      // and one iteration is minutes to hours; `generate` resolves only at the
      // end, which is total silence that cannot be told apart from a hang.
      const output = await agent.stream(prompt);
      const text = await reportStream(output.fullStream as AsyncIterable<never>, stats, (entry) =>
        record(cfg, runId, { iteration, ...(entry as object) }),
      );
      await record(cfg, runId, {
        iteration,
        event: 'agent-finished',
        stats: statsForLog(stats),
        text: text.slice(0, 4000),
      });
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
      console.error(`  ${RED}agent failed:${RESET} ${failure}`);
      await record(cfg, runId, { iteration, event: 'agent-failed', error: failure, stats: statsForLog(stats) });
    }

    const elapsed = Math.round((Date.now() - started) / 1000);
    console.log(`  ${DIM}${summarise(stats)} · ${fmtDuration(elapsed)}${RESET}`);
    const tools = toolBreakdown(stats);
    if (tools) console.log(`  ${DIM}${tools}${RESET}`);

    const after = await measure(cfg.repo, cfg.liveMeasure);
    const state = await repoState(cfg.repo);
    const gained = after.matched - before.matched;

    const move = gained > 0 ? GREEN : gained < 0 ? RED : DIM;
    console.log(
      `  ${move}→ ${after.gradient.toFixed(2)}%  ${gained >= 0 ? '+' : ''}${gained.toLocaleString()} lines${RESET} · ` +
        `${after.commit}${state.clean ? '' : `  ${RED}⚠ tree dirty${RESET}`}` +
        `${state.pushed ? '' : `  ${RED}⚠ unpushed${RESET}`}`,
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
