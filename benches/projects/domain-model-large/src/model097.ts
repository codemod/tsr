// Generated domain module 97 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model096Service, summarizeModel096 } from "./model096";
import type { Model096 } from "./model096";
import { Model090Service, summarizeModel090 } from "./model090";
import type { Model090 } from "./model090";

export type Model097Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model097Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model097 extends Entity<"model097"> {
  updatedAt?: number;
  name: string;
  status: Model097Status;
  tags: string[];
  lines: Model097Line[];
  owner?: { name: string; email?: string };
  parent?: Model096;
  related: Model090[];
}

export type Model097Event =
  | { kind: "created"; item: Model097 }
  | { kind: "renamed"; id: Id<"model097">; from: string; to: string }
  | { kind: "moved"; id: Id<"model097">; status: Model097Status }
  | { kind: "deleted"; id: Id<"model097">; reason?: string };

export type Model097Events = {
  change: Model097Event;
  error: { message: string; code: number };
};

export type Model097Numbers = KeysOfType<Model097Line, number>;
export type FrozenModel097 = DeepReadonly<Model097>;

export function describeModel097Event(event: Model097Event): string {
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

export function totalModel097(item: Model097): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel097(item: Model097): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel097(item), 3), (item.parent ? summarizeModel096(item.parent) : "-"), item.related.map(summarizeModel090).join(",")].join(" | ");
}

export class Model097Service extends Service<Model097, "model097"> {
  readonly events = new EventBus<Model097Events>();
  private readonly parents?: Model096Service;

  constructor(repository = new MemoryRepository<Model097, "model097">()) {
    super(repository);
  }

  validate(item: Model097): string[] {
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

  rename(id: Id<"model097">, to: string): Result<Model097> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model097 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model097">, status: Model097Status): Result<Model097Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model097">, patch: Patch<Pick<Model097, "name" | "tags">>): Model097 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model097Status, Model097[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model097Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model097">[]): Promise<Model097[]> {
    const found: Model097[] = [];
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

  linkedService(): Model096Service {
    return this.parents ?? new Model096Service();
  }
}

export function makeModel097(id: string, name: string): Model097 {
  return {
    id: id as Id<"model097">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 98, unit: "s" }],
    related: [],
  };
}

export const model097Defaults: FrozenModel097 = makeModel097("default-97", "Default 97");
export const model097Label = summarizeModel097(makeModel097("label", "Label"));
