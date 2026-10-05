// Generated domain module 106 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model105Service, summarizeModel105 } from "./model105";
import type { Model105 } from "./model105";
import { Model099Service, summarizeModel099 } from "./model099";
import type { Model099 } from "./model099";

export type Model106Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model106Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model106 extends Entity<"model106"> {
  updatedAt?: number;
  name: string;
  status: Model106Status;
  tags: string[];
  lines: Model106Line[];
  owner?: { name: string; email?: string };
  parent?: Model105;
  related: Model099[];
}

export type Model106Event =
  | { kind: "created"; item: Model106 }
  | { kind: "renamed"; id: Id<"model106">; from: string; to: string }
  | { kind: "moved"; id: Id<"model106">; status: Model106Status }
  | { kind: "deleted"; id: Id<"model106">; reason?: string };

export type Model106Events = {
  change: Model106Event;
  error: { message: string; code: number };
};

export type Model106Numbers = KeysOfType<Model106Line, number>;
export type FrozenModel106 = DeepReadonly<Model106>;

export function describeModel106Event(event: Model106Event): string {
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

export function totalModel106(item: Model106): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel106(item: Model106): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel106(item), 3), (item.parent ? summarizeModel105(item.parent) : "-"), item.related.map(summarizeModel099).join(",")].join(" | ");
}

export class Model106Service extends Service<Model106, "model106"> {
  readonly events = new EventBus<Model106Events>();
  private readonly parents?: Model105Service;

  constructor(repository = new MemoryRepository<Model106, "model106">()) {
    super(repository);
  }

  validate(item: Model106): string[] {
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

  rename(id: Id<"model106">, to: string): Result<Model106> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model106 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model106">, status: Model106Status): Result<Model106Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model106">, patch: Patch<Pick<Model106, "name" | "tags">>): Model106 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model106Status, Model106[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model106Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model106">[]): Promise<Model106[]> {
    const found: Model106[] = [];
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

  linkedService(): Model105Service {
    return this.parents ?? new Model105Service();
  }
}

export function makeModel106(id: string, name: string): Model106 {
  return {
    id: id as Id<"model106">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 107, unit: "m" }],
    related: [],
  };
}

export const model106Defaults: FrozenModel106 = makeModel106("default-106", "Default 106");
export const model106Label = summarizeModel106(makeModel106("label", "Label"));
