// Generated domain module 120 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model119Service, summarizeModel119 } from "./model119";
import type { Model119 } from "./model119";
import { Model113Service, summarizeModel113 } from "./model113";
import type { Model113 } from "./model113";

export type Model120Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model120Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model120 extends Entity<"model120"> {
  updatedAt?: number;
  name: string;
  status: Model120Status;
  tags: string[];
  lines: Model120Line[];
  owner?: { name: string; email?: string };
  parent?: Model119;
  related: Model113[];
}

export type Model120Event =
  | { kind: "created"; item: Model120 }
  | { kind: "renamed"; id: Id<"model120">; from: string; to: string }
  | { kind: "moved"; id: Id<"model120">; status: Model120Status }
  | { kind: "deleted"; id: Id<"model120">; reason?: string };

export type Model120Events = {
  change: Model120Event;
  error: { message: string; code: number };
};

export type Model120Numbers = KeysOfType<Model120Line, number>;
export type FrozenModel120 = DeepReadonly<Model120>;

export function describeModel120Event(event: Model120Event): string {
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

export function totalModel120(item: Model120): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel120(item: Model120): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel120(item), 3), (item.parent ? summarizeModel119(item.parent) : "-"), item.related.map(summarizeModel113).join(",")].join(" | ");
}

export class Model120Service extends Service<Model120, "model120"> {
  readonly events = new EventBus<Model120Events>();
  private readonly parents?: Model119Service;

  constructor(repository = new MemoryRepository<Model120, "model120">()) {
    super(repository);
  }

  validate(item: Model120): string[] {
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

  rename(id: Id<"model120">, to: string): Result<Model120> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model120 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model120">, status: Model120Status): Result<Model120Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model120">, patch: Patch<Pick<Model120, "name" | "tags">>): Model120 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model120Status, Model120[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model120Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model120">[]): Promise<Model120[]> {
    const found: Model120[] = [];
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

  linkedService(): Model119Service {
    return this.parents ?? new Model119Service();
  }
}

export function makeModel120(id: string, name: string): Model120 {
  return {
    id: id as Id<"model120">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 121, unit: "kg" }],
    related: [],
  };
}

export const model120Defaults: FrozenModel120 = makeModel120("default-120", "Default 120");
export const model120Label = summarizeModel120(makeModel120("label", "Label"));
