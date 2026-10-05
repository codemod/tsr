// Generated domain module 64 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model063Service, summarizeModel063 } from "./model063";
import type { Model063 } from "./model063";
import { Model057Service, summarizeModel057 } from "./model057";
import type { Model057 } from "./model057";

export type Model064Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model064Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model064 extends Entity<"model064"> {
  updatedAt?: number;
  name: string;
  status: Model064Status;
  tags: string[];
  lines: Model064Line[];
  owner?: { name: string; email?: string };
  parent?: Model063;
  related: Model057[];
}

export type Model064Event =
  | { kind: "created"; item: Model064 }
  | { kind: "renamed"; id: Id<"model064">; from: string; to: string }
  | { kind: "moved"; id: Id<"model064">; status: Model064Status }
  | { kind: "deleted"; id: Id<"model064">; reason?: string };

export type Model064Events = {
  change: Model064Event;
  error: { message: string; code: number };
};

export type Model064Numbers = KeysOfType<Model064Line, number>;
export type FrozenModel064 = DeepReadonly<Model064>;

export function describeModel064Event(event: Model064Event): string {
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

export function totalModel064(item: Model064): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel064(item: Model064): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel064(item), 3), (item.parent ? summarizeModel063(item.parent) : "-"), item.related.map(summarizeModel057).join(",")].join(" | ");
}

export class Model064Service extends Service<Model064, "model064"> {
  readonly events = new EventBus<Model064Events>();
  private readonly parents?: Model063Service;

  constructor(repository = new MemoryRepository<Model064, "model064">()) {
    super(repository);
  }

  validate(item: Model064): string[] {
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

  rename(id: Id<"model064">, to: string): Result<Model064> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model064 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model064">, status: Model064Status): Result<Model064Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model064">, patch: Patch<Pick<Model064, "name" | "tags">>): Model064 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model064Status, Model064[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model064Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model064">[]): Promise<Model064[]> {
    const found: Model064[] = [];
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

  linkedService(): Model063Service {
    return this.parents ?? new Model063Service();
  }
}

export function makeModel064(id: string, name: string): Model064 {
  return {
    id: id as Id<"model064">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 65, unit: "hour" }],
    related: [],
  };
}

export const model064Defaults: FrozenModel064 = makeModel064("default-64", "Default 64");
export const model064Label = summarizeModel064(makeModel064("label", "Label"));
