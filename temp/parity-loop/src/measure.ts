import { execFile } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { promisify } from 'node:util';

const run = promisify(execFile);

/** One reading of where the port stands. */
export type Reading = {
  /** Matched assertion lines. */
  matched: number;
  /** The denominator the suite counts. */
  total: number;
  /** matched/total as a percentage — the number the project steers by. */
  gradient: number;
  /** Whole cases passing the positional gate. */
  cases: number;
  /** The gate's denominator. */
  caseTotal: number;
  /** Commit the reading was taken at. */
  commit: string;
  /** Whether it came from a live run or from the committed snapshot. */
  source: 'coverage' | 'snapshot';
};

const SNAPSHOT = 'crates/tsr-conformance/snapshots/checker_types.snap';

/**
 * Parse the two figures out of a `checker_types` summary.
 *
 * The same text is produced by the `coverage` binary on stdout and written to
 * the committed snapshot, so one parser serves both and they cannot drift.
 */
function parse(text: string): Omit<Reading, 'commit' | 'source'> {
  const lines = /Assertion lines:\s*(\d+)\/(\d+)/.exec(text);
  const cases = /Passed\s*:\s*(\d+)\/(\d+)/.exec(text);
  if (!lines || !cases) {
    throw new Error(
      'could not find `Assertion lines:` and `Passed :` in the checker_types summary — ' +
        'the report format changed and this parser needs updating',
    );
  }
  const matched = Number(lines[1]);
  const total = Number(lines[2]);
  return {
    matched,
    total,
    gradient: (matched / total) * 100,
    cases: Number(cases[1]),
    caseTotal: Number(cases[2]),
  };
}

async function commitOf(repo: string): Promise<string> {
  const { stdout } = await run('git', ['rev-parse', '--short', 'HEAD'], { cwd: repo });
  return stdout.trim();
}

/**
 * Measure the gradient **independently of anything the agent said**.
 *
 * This is the load-bearing part of the loop. An agent asked to report its own
 * progress can be wrong in good faith — this project's own history has several
 * numbers that were true of a different population than the one they were
 * quoted about — so the loop never reads a figure out of the agent's prose. It
 * runs the conformance binary and parses the summary, or, when `live` is
 * false, reads the committed snapshot the agent regenerates as part of its
 * gate discipline.
 *
 * `live` defaults to true and should stay that way: a snapshot is only as
 * fresh as the last time someone ran the gates, and an iteration that skipped
 * them would otherwise look like an iteration that made no progress.
 */
export async function measure(repo: string, live = true): Promise<Reading> {
  const commit = await commitOf(repo);
  if (!live) {
    const text = await readFile(`${repo}/${SNAPSHOT}`, 'utf8');
    return { ...parse(text), commit, source: 'snapshot' };
  }
  const { stdout } = await run(
    'cargo',
    ['run', '--release', '-p', 'tsr-conformance', '--bin', 'coverage'],
    { cwd: repo, maxBuffer: 64 * 1024 * 1024 },
  );
  return { ...parse(stdout), commit, source: 'coverage' };
}

/** Whether the tree is clean and every commit is on the remote. */
export async function repoState(repo: string): Promise<{ clean: boolean; pushed: boolean }> {
  const { stdout: status } = await run('git', ['status', '--porcelain'], { cwd: repo });
  let pushed = true;
  try {
    const { stdout: ahead } = await run(
      'git',
      ['rev-list', '--count', '@{upstream}..HEAD'],
      { cwd: repo },
    );
    pushed = ahead.trim() === '0';
  } catch {
    // No upstream configured: not something the loop should fail on, but not
    // something it should silently claim either.
    pushed = false;
  }
  return { clean: status.trim() === '', pushed };
}
