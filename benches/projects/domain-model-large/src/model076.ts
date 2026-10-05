// Generated domain module 76 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model075Service, summarizeModel075 } from "./model075";
import type { Model075 } from "./model075";
import { Model069Service, summarizeModel069 } from "./model069";
import type { Model069 } from "./model069";

export type Model076Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model076Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model076 extends Entity<"model076"> {
  updatedAt?: number;
  name: string;
  status: Model076Status;
  tags: string[];
  lines: Model076Line[];
  owner?: { name: string; email?: string };
  parent?: Model075;
  related: Model069[];
}

export type Model076Event =
  | { kind: "created"; item: Model076 }
  | { kind: "renamed"; id: Id<"model076">; from: string; to: string }
  | { kind: "moved"; id: Id<"model076">; status: Model076Status }
  | { kind: "deleted"; id: Id<"model076">; reason?: string };

export type Model076Events = {
  change: Model076Event;
  error: { message: string; code: number };
};

export type Model076Numbers = KeysOfType<Model076Line, number>;
export type FrozenModel076 = DeepReadonly<Model076>;

export function describeModel076Event(event: Model076Event): string {
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

export function totalModel076(item: Model076): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel076(item: Model076): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel076(item), 3), (item.parent ? summarizeModel075(item.parent) : "-"), item.related.map(summarizeModel069).join(",")].join(" | ");
}

export class Model076Service extends Service<Model076, "model076"> {
  readonly events = new EventBus<Model076Events>();
  private readonly parents?: Model075Service;

  constructor(repository = new MemoryRepository<Model076, "model076">()) {
    super(repository);
  }

  validate(item: Model076): string[] {
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

  rename(id: Id<"model076">, to: string): Result<Model076> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model076 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model076">, status: Model076Status): Result<Model076Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model076">, patch: Patch<Pick<Model076, "name" | "tags">>): Model076 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model076Status, Model076[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model076Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model076">[]): Promise<Model076[]> {
    const found: Model076[] = [];
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

  linkedService(): Model075Service {
    return this.parents ?? new Model075Service();
  }
}

export function makeModel076(id: string, name: string): Model076 {
  return {
    id: id as Id<"model076">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 77, unit: "m" }],
    related: [],
  };
}

export const model076Defaults: FrozenModel076 = makeModel076("default-76", "Default 76");
export const model076Label = summarizeModel076(makeModel076("label", "Label"));
