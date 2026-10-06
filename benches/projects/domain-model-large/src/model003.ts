// Generated domain module 3 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model002Service, summarizeModel002 } from "./model002";
import type { Model002 } from "./model002";

export type Model003Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model003Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model003 extends Entity<"model003"> {
  updatedAt?: number;
  name: string;
  status: Model003Status;
  tags: string[];
  lines: Model003Line[];
  owner?: { name: string; email?: string };
  parent?: Model002;
}

export type Model003Event =
  | { kind: "created"; item: Model003 }
  | { kind: "renamed"; id: Id<"model003">; from: string; to: string }
  | { kind: "moved"; id: Id<"model003">; status: Model003Status }
  | { kind: "deleted"; id: Id<"model003">; reason?: string };

export type Model003Events = {
  change: Model003Event;
  error: { message: string; code: number };
};

export type Model003Numbers = KeysOfType<Model003Line, number>;
export type FrozenModel003 = DeepReadonly<Model003>;

export function describeModel003Event(event: Model003Event): string {
  switch (event.kind) {
    case "created":
      return `created ${event.item.name}`;
    case "renamed":
      return `renamed ${event.from} -> ${event.to}`;
    case "moved":
      return `moved to ${event.status}`;
    case "deleted":
      return event.reason ? `deleted: ${event.reason}` : "deleted";
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export function totalModel003(item: Model003): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel003(item: Model003): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel003(item), 3), (item.parent ? summarizeModel002(item.parent) : "-")].join(" | ");
}

export class Model003Service extends Service<Model003, "model003"> {
  readonly events = new EventBus<Model003Events>();
  private readonly parents?: Model002Service;

  constructor(repository = new MemoryRepository<Model003, "model003">()) {
    super(repository);
  }

  validate(item: Model003): string[] {
    const problems: string[] = [];
    if (item.name.trim().length === 0) {
      problems.push("name is required");
    }
    for (const [index, line] of item.lines.entries()) {
      if (line.quantity <= 0) {
        problems.push(`line ${index} has no quantity`);
      }
    }
    return problems;
  }

  rename(id: Id<"model003">, to: string): Result<Model003> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model003 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model003">, status: Model003Status): Result<Model003Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model003">, patch: Patch<Pick<Model003, "name" | "tags">>): Model003 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model003Status, Model003[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model003Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model003">[]): Promise<Model003[]> {
    const found: Model003[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }

  parentNames(): string[] {
    return this.repository
      .list((item) => item.parent !== undefined)
      .map((item) => unwrapOr(ok(item.parent?.name ?? ""), ""));
  }

  linkedService(): Model002Service {
    return this.parents ?? new Model002Service();
  }
}

export function makeModel003(id: string, name: string): Model003 {
  return {
    id: id as Id<"model003">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 4, unit: "item" }],
  };
}

export const model003Defaults: FrozenModel003 = makeModel003("default-3", "Default 3");
export const model003Label = summarizeModel003(makeModel003("label", "Label"));
