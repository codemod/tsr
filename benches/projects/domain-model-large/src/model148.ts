// Generated domain module 148 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model147Service, summarizeModel147 } from "./model147";
import type { Model147 } from "./model147";
import { Model141Service, summarizeModel141 } from "./model141";
import type { Model141 } from "./model141";

export type Model148Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model148Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model148 extends Entity<"model148"> {
  updatedAt?: number;
  name: string;
  status: Model148Status;
  tags: string[];
  lines: Model148Line[];
  owner?: { name: string; email?: string };
  parent?: Model147;
  related: Model141[];
}

export type Model148Event =
  | { kind: "created"; item: Model148 }
  | { kind: "renamed"; id: Id<"model148">; from: string; to: string }
  | { kind: "moved"; id: Id<"model148">; status: Model148Status }
  | { kind: "deleted"; id: Id<"model148">; reason?: string };

export type Model148Events = {
  change: Model148Event;
  error: { message: string; code: number };
};

export type Model148Numbers = KeysOfType<Model148Line, number>;
export type FrozenModel148 = DeepReadonly<Model148>;

export function describeModel148Event(event: Model148Event): string {
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

export function totalModel148(item: Model148): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel148(item: Model148): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel148(item), 3), (item.parent ? summarizeModel147(item.parent) : "-"), item.related.map(summarizeModel141).join(",")].join(" | ");
}

export class Model148Service extends Service<Model148, "model148"> {
  readonly events = new EventBus<Model148Events>();
  private readonly parents?: Model147Service;

  constructor(repository = new MemoryRepository<Model148, "model148">()) {
    super(repository);
  }

  validate(item: Model148): string[] {
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

  rename(id: Id<"model148">, to: string): Result<Model148> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model148 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model148">, status: Model148Status): Result<Model148Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model148">, patch: Patch<Pick<Model148, "name" | "tags">>): Model148 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model148Status, Model148[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model148Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model148">[]): Promise<Model148[]> {
    const found: Model148[] = [];
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

  linkedService(): Model147Service {
    return this.parents ?? new Model147Service();
  }
}

export function makeModel148(id: string, name: string): Model148 {
  return {
    id: id as Id<"model148">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 149, unit: "item" }],
    related: [],
  };
}

export const model148Defaults: FrozenModel148 = makeModel148("default-148", "Default 148");
export const model148Label = summarizeModel148(makeModel148("label", "Label"));
