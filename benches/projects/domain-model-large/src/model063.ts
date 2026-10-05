// Generated domain module 63 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model062Service, summarizeModel062 } from "./model062";
import type { Model062 } from "./model062";
import { Model056Service, summarizeModel056 } from "./model056";
import type { Model056 } from "./model056";

export type Model063Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model063Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model063 extends Entity<"model063"> {
  updatedAt?: number;
  name: string;
  status: Model063Status;
  tags: string[];
  lines: Model063Line[];
  owner?: { name: string; email?: string };
  parent?: Model062;
  related: Model056[];
}

export type Model063Event =
  | { kind: "created"; item: Model063 }
  | { kind: "renamed"; id: Id<"model063">; from: string; to: string }
  | { kind: "moved"; id: Id<"model063">; status: Model063Status }
  | { kind: "deleted"; id: Id<"model063">; reason?: string };

export type Model063Events = {
  change: Model063Event;
  error: { message: string; code: number };
};

export type Model063Numbers = KeysOfType<Model063Line, number>;
export type FrozenModel063 = DeepReadonly<Model063>;

export function describeModel063Event(event: Model063Event): string {
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

export function totalModel063(item: Model063): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel063(item: Model063): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel063(item), 3), (item.parent ? summarizeModel062(item.parent) : "-"), item.related.map(summarizeModel056).join(",")].join(" | ");
}

export class Model063Service extends Service<Model063, "model063"> {
  readonly events = new EventBus<Model063Events>();
  private readonly parents?: Model062Service;

  constructor(repository = new MemoryRepository<Model063, "model063">()) {
    super(repository);
  }

  validate(item: Model063): string[] {
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

  rename(id: Id<"model063">, to: string): Result<Model063> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model063 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model063">, status: Model063Status): Result<Model063Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model063">, patch: Patch<Pick<Model063, "name" | "tags">>): Model063 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model063Status, Model063[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model063Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model063">[]): Promise<Model063[]> {
    const found: Model063[] = [];
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

  linkedService(): Model062Service {
    return this.parents ?? new Model062Service();
  }
}

export function makeModel063(id: string, name: string): Model063 {
  return {
    id: id as Id<"model063">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 64, unit: "item" }],
    related: [],
  };
}

export const model063Defaults: FrozenModel063 = makeModel063("default-63", "Default 63");
export const model063Label = summarizeModel063(makeModel063("label", "Label"));
