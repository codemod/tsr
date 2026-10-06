// Generated domain module 129 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model128Service, summarizeModel128 } from "./model128";
import type { Model128 } from "./model128";
import { Model122Service, summarizeModel122 } from "./model122";
import type { Model122 } from "./model122";

export type Model129Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model129Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model129 extends Entity<"model129"> {
  updatedAt?: number;
  name: string;
  status: Model129Status;
  tags: string[];
  lines: Model129Line[];
  owner?: { name: string; email?: string };
  parent?: Model128;
  related: Model122[];
}

export type Model129Event =
  | { kind: "created"; item: Model129 }
  | { kind: "renamed"; id: Id<"model129">; from: string; to: string }
  | { kind: "moved"; id: Id<"model129">; status: Model129Status }
  | { kind: "deleted"; id: Id<"model129">; reason?: string };

export type Model129Events = {
  change: Model129Event;
  error: { message: string; code: number };
};

export type Model129Numbers = KeysOfType<Model129Line, number>;
export type FrozenModel129 = DeepReadonly<Model129>;

export function describeModel129Event(event: Model129Event): string {
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

export function totalModel129(item: Model129): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel129(item: Model129): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel129(item), 3), (item.parent ? summarizeModel128(item.parent) : "-"), item.related.map(summarizeModel122).join(",")].join(" | ");
}

export class Model129Service extends Service<Model129, "model129"> {
  readonly events = new EventBus<Model129Events>();
  private readonly parents?: Model128Service;

  constructor(repository = new MemoryRepository<Model129, "model129">()) {
    super(repository);
  }

  validate(item: Model129): string[] {
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

  rename(id: Id<"model129">, to: string): Result<Model129> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model129 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model129">, status: Model129Status): Result<Model129Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model129">, patch: Patch<Pick<Model129, "name" | "tags">>): Model129 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model129Status, Model129[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model129Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model129">[]): Promise<Model129[]> {
    const found: Model129[] = [];
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

  linkedService(): Model128Service {
    return this.parents ?? new Model128Service();
  }
}

export function makeModel129(id: string, name: string): Model129 {
  return {
    id: id as Id<"model129">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 130, unit: "hour" }],
    related: [],
  };
}

export const model129Defaults: FrozenModel129 = makeModel129("default-129", "Default 129");
export const model129Label = summarizeModel129(makeModel129("label", "Label"));
