/**
 * Live console reporting for one iteration.
 *
 * # Why this exists
 *
 * The first version called `agent.generate()`, which resolves only when the
 * whole iteration is done — minutes to hours of silence after the header,
 * indistinguishable from a hang. An unattended loop you cannot watch is one
 * you have to trust blindly, and nothing else in this project runs that way.
 *
 * # Where each kind of detail comes from, which was measured and not assumed
 *
 * Two sources, because **the Mastra adapter's chunk stream carries no tool
 * events at all.** That was established by probing a real run and printing
 * every distinct chunk type: `start`, `step-start`, `response-metadata`,
 * `text-start`, `text-delta`, `text-end`, `step-finish`, `finish` — and
 * nothing else. The vendor SDK runs the tool loop internally and the adapter
 * surfaces only the assistant's prose and lifecycle.
 *
 * So:
 *
 * - **Tool calls, session id, and file edits** come from the Claude Agent
 *   SDK's own `hooks` (`PreToolUse` above all), passed through `sdkOptions`.
 *   This is the only route to per-tool visibility.
 * - **Prose, token usage and cost** come from the chunk stream, where
 *   `providerMetadata.claude` carries `totalCostUsd` and `usage`.
 *
 * A reporter built only on the chunk stream would print a wall of text and
 * claim zero tool calls for an iteration that ran hundreds — which is exactly
 * what the first version did, and why this comment names its evidence.
 *
 * # Defensive by construction
 *
 * Field access goes through guarded helpers and unknown chunks fall through
 * silently. **A reporter that throws takes the iteration's work with it**,
 * which would be a terrible trade for prettier output.
 */

/** Anything the stream hands us. Deliberately not typed to the vendor union. */
type Chunk = { type?: string; payload?: Record<string, unknown> } & Record<string, unknown>;

const DIM = '[2m';
const BOLD = '[1m';
const CYAN = '[36m';
const YELLOW = '[33m';
const RED = '[31m';
const RESET = '[0m';

function pick(source: unknown, ...keys: string[]): unknown {
  if (!source || typeof source !== 'object') return undefined;
  const record = source as Record<string, unknown>;
  for (const key of keys) {
    if (record[key] !== undefined) return record[key];
  }
  return undefined;
}

function asString(value: unknown, max = 140): string {
  if (value === undefined || value === null) return '';
  const text = typeof value === 'string' ? value : JSON.stringify(value);
  const oneLine = text.replace(/\s+/g, ' ').trim();
  return oneLine.length > max ? `${oneLine.slice(0, max)}…` : oneLine;
}

/**
 * The most useful one-line summary of a tool call.
 *
 * Tool inputs are heterogeneous — a `Bash` call is a command, an `Edit` is a
 * path plus two blobs — so the interesting field is picked per tool. Dumping
 * whole inputs would bury the run in the very diff text the agent is writing.
 */
function toolSummary(name: string, input: unknown): string {
  const first = (...keys: string[]) => asString(pick(input, ...keys));
  switch (name) {
    case 'Bash':
      return first('command');
    case 'Read':
    case 'Write':
    case 'Edit':
    case 'NotebookEdit':
      return first('file_path');
    case 'Glob':
    case 'Grep':
      return [first('pattern'), first('path')].filter(Boolean).join(' ');
    case 'Task':
    case 'Agent':
      return first('description', 'prompt');
    case 'TodoWrite':
      return 'todos';
    case 'WebFetch':
      return first('url');
    default:
      return first('description', 'prompt', 'query', 'command', 'file_path', 'pattern');
  }
}

/**
 * The human-readable part of an error.
 *
 * The adapter hands back a stringified `Error` complete with its stack, which
 * is forty lines of node internals around one useful sentence such as
 * "Reached maximum number of turns (8)". The stack is still written to the
 * run log; only the console gets the sentence.
 */
function errorMessage(value: unknown): string {
  if (value && typeof value === 'object') {
    const message = pick(value, 'message');
    if (typeof message === 'string') return message;
  }
  if (typeof value === 'string') {
    try {
      const parsed: unknown = JSON.parse(value);
      const message = pick(parsed, 'message');
      if (typeof message === 'string') return message;
    } catch {
      // Not JSON; fall through to the plain string.
    }
  }
  return asString(value, 300);
}

/** What one iteration did, as counted by the reporter. */
export type IterationStats = {
  toolCalls: number;
  byTool: Record<string, number>;
  filesTouched: Set<string>;
  steps: number;
  sessionId?: string;
  transcript?: string;
  inputTokens?: number;
  outputTokens?: number;
  cachedInputTokens?: number;
  costUsd?: number;
  errors: string[];
};

export function newStats(): IterationStats {
  return { toolCalls: 0, byTool: {}, filesTouched: new Set(), steps: 0, errors: [] };
}

/**
 * The `hooks` object to hand to `sdkOptions`, which is what makes tool calls
 * visible at all.
 *
 * Every callback returns `{ continue: true }` — these observe and never
 * decide. A hook that can block is a hook that can wedge an unattended run,
 * and permission policy already lives in `permissionMode`.
 */
export function observerHooks(stats: IterationStats) {
  const observe = async (input: unknown) => {
    try {
      const event = pick(input, 'hook_event_name');

      // `session_id` and `transcript_path` are on `BaseHookInput`, so they
      // arrive with *every* event. Reading them here rather than from
      // `SessionStart` is deliberate: that event was observed not to fire
      // through the Mastra adapter, and an id printed only sometimes is worse
      // than one printed from whatever event happens to come first.
      if (!stats.sessionId) {
        const id = pick(input, 'session_id');
        const transcript = pick(input, 'transcript_path');
        if (typeof id === 'string') {
          stats.sessionId = id;
          console.log(`  ${DIM}session ${id}${RESET}`);
        }
        if (typeof transcript === 'string') {
          stats.transcript = transcript;
          console.log(`  ${DIM}transcript ${transcript}${RESET}`);
        }
      }

      if (event === 'PreToolUse') {
        const name = String(pick(input, 'tool_name') ?? 'tool');
        const toolInput = pick(input, 'tool_input');
        stats.toolCalls += 1;
        stats.byTool[name] = (stats.byTool[name] ?? 0) + 1;
        const path = pick(toolInput, 'file_path');
        if (typeof path === 'string') stats.filesTouched.add(path);
        // Subagent calls are tagged, because a `Task` fan-out otherwise looks
        // like the main thread doing the work.
        const sub = pick(input, 'agent_id') ? ` ${DIM}(subagent)${RESET}` : '';
        console.log(
          `  ${CYAN}▸${RESET} ${BOLD}${name}${RESET}${sub} ${DIM}${toolSummary(name, toolInput)}${RESET}`,
        );
      }

      if (event === 'PostToolUseFailure') {
        const name = String(pick(input, 'tool_name') ?? 'tool');
        console.log(`  ${YELLOW}!${RESET} ${DIM}${name} failed${RESET}`);
      }
    } catch {
      // Never let reporting break the run.
    }
    return { continue: true } as const;
  };

  return {
    SessionStart: [{ hooks: [observe] }],
    PreToolUse: [{ hooks: [observe] }],
    PostToolUseFailure: [{ hooks: [observe] }],
  };
}

/**
 * Consume the chunk stream, printing prose and recording usage.
 *
 * Returns the accumulated assistant text, so the caller can log it without
 * awaiting a second promise off a stream this already consumed.
 */
export async function reportStream(
  stream: AsyncIterable<Chunk>,
  stats: IterationStats,
  log: (entry: unknown) => Promise<void>,
): Promise<string> {
  let text = '';
  let midText = false;

  const endText = () => {
    if (midText) {
      process.stdout.write('\n');
      midText = false;
    }
  };

  const readClaudeMeta = (payload: Record<string, unknown>) => {
    const claude = pick(pick(payload, 'providerMetadata'), 'claude');
    const cost = pick(claude, 'totalCostUsd');
    if (typeof cost === 'number') stats.costUsd = cost;
  };

  for await (const chunk of stream) {
    const type = chunk.type ?? 'unknown';
    const payload = (chunk.payload ?? {}) as Record<string, unknown>;

    switch (type) {
      case 'step-start':
        stats.steps += 1;
        break;

      case 'text-delta': {
        const delta = pick(payload, 'text', 'textDelta', 'delta');
        if (typeof delta === 'string' && delta.length > 0) {
          if (!midText) {
            process.stdout.write(`  ${DIM}│${RESET} `);
            midText = true;
          }
          // The gutter is kept on wrapped prose so agent text stays visually
          // distinct from the tool lines interleaved with it.
          process.stdout.write(delta.replace(/\n/g, `\n  ${DIM}│${RESET} `));
          text += delta;
        }
        break;
      }

      case 'text-end':
        readClaudeMeta(payload);
        endText();
        break;

      case 'error': {
        endText();
        const detail = errorMessage(pick(payload, 'error', 'message'));
        stats.errors.push(detail);
        console.log(`  ${RED}✗${RESET} ${detail}`);
        await log({ event: 'stream-error', detail });
        break;
      }

      case 'step-finish':
        readClaudeMeta(payload);
        break;

      case 'finish': {
        endText();
        readClaudeMeta(payload);
        const usage = pick(pick(payload, 'output'), 'usage') ?? pick(payload, 'usage');
        const input = pick(usage, 'inputTokens');
        const output = pick(usage, 'outputTokens');
        const cached = pick(usage, 'cachedInputTokens');
        if (typeof input === 'number') stats.inputTokens = input;
        if (typeof output === 'number') stats.outputTokens = output;
        if (typeof cached === 'number') stats.cachedInputTokens = cached;
        break;
      }

      default:
        break;
    }
  }

  endText();
  return text;
}

/** The end-of-iteration summary line. */
export function summarise(stats: IterationStats): string {
  const parts = [`${stats.toolCalls} tool calls`, `${stats.steps} steps`];
  if (stats.filesTouched.size) parts.push(`${stats.filesTouched.size} files`);
  if (stats.outputTokens !== undefined) {
    const cached = stats.cachedInputTokens ? `, ${stats.cachedInputTokens.toLocaleString()} cached` : '';
    parts.push(
      `${(stats.inputTokens ?? 0).toLocaleString()} in / ${stats.outputTokens.toLocaleString()} out${cached}`,
    );
  }
  if (stats.costUsd !== undefined) parts.push(`$${stats.costUsd.toFixed(2)}`);
  if (stats.errors.length) parts.push(`${stats.errors.length} errors`);

  return parts.join(' · ');
}

/** `Bash×120 Edit×40 …`, the six busiest tools. Empty when nothing ran. */
export function toolBreakdown(stats: IterationStats): string {
  return Object.entries(stats.byTool)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 6)
    .map(([name, n]) => `${name}×${n}`)
    .join(' ');
}

/** `stats` as something `JSON.stringify` can take (a `Set` is not). */
export function statsForLog(stats: IterationStats): Record<string, unknown> {
  return { ...stats, filesTouched: [...stats.filesTouched] };
}
