// Generated domain module 124 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model123Service, summarizeModel123 } from "./model123";
import type { Model123 } from "./model123";
import { Model117Service, summarizeModel117 } from "./model117";
import type { Model117 } from "./model117";

export type Model124Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model124Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model124 extends Entity<"model124"> {
  updatedAt?: number;
  name: string;
  status: Model124Status;
  tags: string[];
  lines: Model124Line[];
  owner?: { name: string; email?: string };
  parent?: Model123;
  related: Model117[];
}

export type Model124Event =
  | { kind: "created"; item: Model124 }
  | { kind: "renamed"; id: Id<"model124">; from: string; to: string }
  | { kind: "moved"; id: Id<"model124">; status: Model124Status }
  | { kind: "deleted"; id: Id<"model124">; reason?: string };

export type Model124Events = {
  change: Model124Event;
  error: { message: string; code: number };
};

export type Model124Numbers = KeysOfType<Model124Line, number>;
export type FrozenModel124 = DeepReadonly<Model124>;

export function describeModel124Event(event: Model124Event): string {
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

export function totalModel124(item: Model124): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel124(item: Model124): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel124(item), 3), (item.parent ? summarizeModel123(item.parent) : "-"), item.related.map(summarizeModel117).join(",")].join(" | ");
}

export class Model124Service extends Service<Model124, "model124"> {
  readonly events = new EventBus<Model124Events>();
  private readonly parents?: Model123Service;

  constructor(repository = new MemoryRepository<Model124, "model124">()) {
    super(repository);
  }

  validate(item: Model124): string[] {
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

  rename(id: Id<"model124">, to: string): Result<Model124> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model124 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model124">, status: Model124Status): Result<Model124Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model124">, patch: Patch<Pick<Model124, "name" | "tags">>): Model124 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model124Status, Model124[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model124Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model124">[]): Promise<Model124[]> {
    const found: Model124[] = [];
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

  linkedService(): Model123Service {
    return this.parents ?? new Model123Service();
  }
}

export function makeModel124(id: string, name: string): Model124 {
  return {
    id: id as Id<"model124">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 125, unit: "hour" }],
    related: [],
  };
}

export const model124Defaults: FrozenModel124 = makeModel124("default-124", "Default 124");
export const model124Label = summarizeModel124(makeModel124("label", "Label"));
