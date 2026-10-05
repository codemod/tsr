// Generated domain module 132 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model131Service, summarizeModel131 } from "./model131";
import type { Model131 } from "./model131";
import { Model125Service, summarizeModel125 } from "./model125";
import type { Model125 } from "./model125";

export type Model132Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model132Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model132 extends Entity<"model132"> {
  updatedAt?: number;
  name: string;
  status: Model132Status;
  tags: string[];
  lines: Model132Line[];
  owner?: { name: string; email?: string };
  parent?: Model131;
  related: Model125[];
}

export type Model132Event =
  | { kind: "created"; item: Model132 }
  | { kind: "renamed"; id: Id<"model132">; from: string; to: string }
  | { kind: "moved"; id: Id<"model132">; status: Model132Status }
  | { kind: "deleted"; id: Id<"model132">; reason?: string };

export type Model132Events = {
  change: Model132Event;
  error: { message: string; code: number };
};

export type Model132Numbers = KeysOfType<Model132Line, number>;
export type FrozenModel132 = DeepReadonly<Model132>;

export function describeModel132Event(event: Model132Event): string {
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

export function totalModel132(item: Model132): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel132(item: Model132): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel132(item), 3), (item.parent ? summarizeModel131(item.parent) : "-"), item.related.map(summarizeModel125).join(",")].join(" | ");
}

export class Model132Service extends Service<Model132, "model132"> {
  readonly events = new EventBus<Model132Events>();
  private readonly parents?: Model131Service;

  constructor(repository = new MemoryRepository<Model132, "model132">()) {
    super(repository);
  }

  validate(item: Model132): string[] {
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

  rename(id: Id<"model132">, to: string): Result<Model132> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model132 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model132">, status: Model132Status): Result<Model132Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model132">, patch: Patch<Pick<Model132, "name" | "tags">>): Model132 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model132Status, Model132[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model132Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model132">[]): Promise<Model132[]> {
    const found: Model132[] = [];
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

  linkedService(): Model131Service {
    return this.parents ?? new Model131Service();
  }
}

export function makeModel132(id: string, name: string): Model132 {
  return {
    id: id as Id<"model132">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 133, unit: "s" }],
    related: [],
  };
}

export const model132Defaults: FrozenModel132 = makeModel132("default-132", "Default 132");
export const model132Label = summarizeModel132(makeModel132("label", "Label"));
