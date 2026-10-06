// Generated domain module 90 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model089Service, summarizeModel089 } from "./model089";
import type { Model089 } from "./model089";
import { Model083Service, summarizeModel083 } from "./model083";
import type { Model083 } from "./model083";

export type Model090Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model090Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model090 extends Entity<"model090"> {
  updatedAt?: number;
  name: string;
  status: Model090Status;
  tags: string[];
  lines: Model090Line[];
  owner?: { name: string; email?: string };
  parent?: Model089;
  related: Model083[];
}

export type Model090Event =
  | { kind: "created"; item: Model090 }
  | { kind: "renamed"; id: Id<"model090">; from: string; to: string }
  | { kind: "moved"; id: Id<"model090">; status: Model090Status }
  | { kind: "deleted"; id: Id<"model090">; reason?: string };

export type Model090Events = {
  change: Model090Event;
  error: { message: string; code: number };
};

export type Model090Numbers = KeysOfType<Model090Line, number>;
export type FrozenModel090 = DeepReadonly<Model090>;

export function describeModel090Event(event: Model090Event): string {
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

export function totalModel090(item: Model090): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel090(item: Model090): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel090(item), 3), (item.parent ? summarizeModel089(item.parent) : "-"), item.related.map(summarizeModel083).join(",")].join(" | ");
}

export class Model090Service extends Service<Model090, "model090"> {
  readonly events = new EventBus<Model090Events>();
  private readonly parents?: Model089Service;

  constructor(repository = new MemoryRepository<Model090, "model090">()) {
    super(repository);
  }

  validate(item: Model090): string[] {
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

  rename(id: Id<"model090">, to: string): Result<Model090> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model090 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model090">, status: Model090Status): Result<Model090Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model090">, patch: Patch<Pick<Model090, "name" | "tags">>): Model090 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model090Status, Model090[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model090Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model090">[]): Promise<Model090[]> {
    const found: Model090[] = [];
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

  linkedService(): Model089Service {
    return this.parents ?? new Model089Service();
  }
}

export function makeModel090(id: string, name: string): Model090 {
  return {
    id: id as Id<"model090">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 91, unit: "kg" }],
    related: [],
  };
}

export const model090Defaults: FrozenModel090 = makeModel090("default-90", "Default 90");
export const model090Label = summarizeModel090(makeModel090("label", "Label"));
