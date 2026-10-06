// Generated domain module 85 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model084Service, summarizeModel084 } from "./model084";
import type { Model084 } from "./model084";
import { Model078Service, summarizeModel078 } from "./model078";
import type { Model078 } from "./model078";

export type Model085Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model085Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model085 extends Entity<"model085"> {
  updatedAt?: number;
  name: string;
  status: Model085Status;
  tags: string[];
  lines: Model085Line[];
  owner?: { name: string; email?: string };
  parent?: Model084;
  related: Model078[];
}

export type Model085Event =
  | { kind: "created"; item: Model085 }
  | { kind: "renamed"; id: Id<"model085">; from: string; to: string }
  | { kind: "moved"; id: Id<"model085">; status: Model085Status }
  | { kind: "deleted"; id: Id<"model085">; reason?: string };

export type Model085Events = {
  change: Model085Event;
  error: { message: string; code: number };
};

export type Model085Numbers = KeysOfType<Model085Line, number>;
export type FrozenModel085 = DeepReadonly<Model085>;

export function describeModel085Event(event: Model085Event): string {
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

export function totalModel085(item: Model085): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel085(item: Model085): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel085(item), 3), (item.parent ? summarizeModel084(item.parent) : "-"), item.related.map(summarizeModel078).join(",")].join(" | ");
}

export class Model085Service extends Service<Model085, "model085"> {
  readonly events = new EventBus<Model085Events>();
  private readonly parents?: Model084Service;

  constructor(repository = new MemoryRepository<Model085, "model085">()) {
    super(repository);
  }

  validate(item: Model085): string[] {
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

  rename(id: Id<"model085">, to: string): Result<Model085> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model085 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model085">, status: Model085Status): Result<Model085Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model085">, patch: Patch<Pick<Model085, "name" | "tags">>): Model085 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model085Status, Model085[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model085Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model085">[]): Promise<Model085[]> {
    const found: Model085[] = [];
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

  linkedService(): Model084Service {
    return this.parents ?? new Model084Service();
  }
}

export function makeModel085(id: string, name: string): Model085 {
  return {
    id: id as Id<"model085">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 86, unit: "kg" }],
    related: [],
  };
}

export const model085Defaults: FrozenModel085 = makeModel085("default-85", "Default 85");
export const model085Label = summarizeModel085(makeModel085("label", "Label"));
