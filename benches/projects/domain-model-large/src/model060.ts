// Generated domain module 60 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model059Service, summarizeModel059 } from "./model059";
import type { Model059 } from "./model059";
import { Model053Service, summarizeModel053 } from "./model053";
import type { Model053 } from "./model053";

export type Model060Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model060Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model060 extends Entity<"model060"> {
  updatedAt?: number;
  name: string;
  status: Model060Status;
  tags: string[];
  lines: Model060Line[];
  owner?: { name: string; email?: string };
  parent?: Model059;
  related: Model053[];
}

export type Model060Event =
  | { kind: "created"; item: Model060 }
  | { kind: "renamed"; id: Id<"model060">; from: string; to: string }
  | { kind: "moved"; id: Id<"model060">; status: Model060Status }
  | { kind: "deleted"; id: Id<"model060">; reason?: string };

export type Model060Events = {
  change: Model060Event;
  error: { message: string; code: number };
};

export type Model060Numbers = KeysOfType<Model060Line, number>;
export type FrozenModel060 = DeepReadonly<Model060>;

export function describeModel060Event(event: Model060Event): string {
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

export function totalModel060(item: Model060): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel060(item: Model060): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel060(item), 3), (item.parent ? summarizeModel059(item.parent) : "-"), item.related.map(summarizeModel053).join(",")].join(" | ");
}

export class Model060Service extends Service<Model060, "model060"> {
  readonly events = new EventBus<Model060Events>();
  private readonly parents?: Model059Service;

  constructor(repository = new MemoryRepository<Model060, "model060">()) {
    super(repository);
  }

  validate(item: Model060): string[] {
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

  rename(id: Id<"model060">, to: string): Result<Model060> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model060 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model060">, status: Model060Status): Result<Model060Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model060">, patch: Patch<Pick<Model060, "name" | "tags">>): Model060 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model060Status, Model060[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model060Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model060">[]): Promise<Model060[]> {
    const found: Model060[] = [];
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

  linkedService(): Model059Service {
    return this.parents ?? new Model059Service();
  }
}

export function makeModel060(id: string, name: string): Model060 {
  return {
    id: id as Id<"model060">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 61, unit: "kg" }],
    related: [],
  };
}

export const model060Defaults: FrozenModel060 = makeModel060("default-60", "Default 60");
export const model060Label = summarizeModel060(makeModel060("label", "Label"));
