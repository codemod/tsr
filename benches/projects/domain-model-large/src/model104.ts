// Generated domain module 104 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model103Service, summarizeModel103 } from "./model103";
import type { Model103 } from "./model103";
import { Model097Service, summarizeModel097 } from "./model097";
import type { Model097 } from "./model097";

export type Model104Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model104Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model104 extends Entity<"model104"> {
  updatedAt?: number;
  name: string;
  status: Model104Status;
  tags: string[];
  lines: Model104Line[];
  owner?: { name: string; email?: string };
  parent?: Model103;
  related: Model097[];
}

export type Model104Event =
  | { kind: "created"; item: Model104 }
  | { kind: "renamed"; id: Id<"model104">; from: string; to: string }
  | { kind: "moved"; id: Id<"model104">; status: Model104Status }
  | { kind: "deleted"; id: Id<"model104">; reason?: string };

export type Model104Events = {
  change: Model104Event;
  error: { message: string; code: number };
};

export type Model104Numbers = KeysOfType<Model104Line, number>;
export type FrozenModel104 = DeepReadonly<Model104>;

export function describeModel104Event(event: Model104Event): string {
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

export function totalModel104(item: Model104): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel104(item: Model104): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel104(item), 3), (item.parent ? summarizeModel103(item.parent) : "-"), item.related.map(summarizeModel097).join(",")].join(" | ");
}

export class Model104Service extends Service<Model104, "model104"> {
  readonly events = new EventBus<Model104Events>();
  private readonly parents?: Model103Service;

  constructor(repository = new MemoryRepository<Model104, "model104">()) {
    super(repository);
  }

  validate(item: Model104): string[] {
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

  rename(id: Id<"model104">, to: string): Result<Model104> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model104 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model104">, status: Model104Status): Result<Model104Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model104">, patch: Patch<Pick<Model104, "name" | "tags">>): Model104 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model104Status, Model104[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model104Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model104">[]): Promise<Model104[]> {
    const found: Model104[] = [];
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

  linkedService(): Model103Service {
    return this.parents ?? new Model103Service();
  }
}

export function makeModel104(id: string, name: string): Model104 {
  return {
    id: id as Id<"model104">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 105, unit: "hour" }],
    related: [],
  };
}

export const model104Defaults: FrozenModel104 = makeModel104("default-104", "Default 104");
export const model104Label = summarizeModel104(makeModel104("label", "Label"));
