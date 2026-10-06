// Generated domain module 154 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model153Service, summarizeModel153 } from "./model153";
import type { Model153 } from "./model153";
import { Model147Service, summarizeModel147 } from "./model147";
import type { Model147 } from "./model147";

export type Model154Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model154Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model154 extends Entity<"model154"> {
  updatedAt?: number;
  name: string;
  status: Model154Status;
  tags: string[];
  lines: Model154Line[];
  owner?: { name: string; email?: string };
  parent?: Model153;
  related: Model147[];
}

export type Model154Event =
  | { kind: "created"; item: Model154 }
  | { kind: "renamed"; id: Id<"model154">; from: string; to: string }
  | { kind: "moved"; id: Id<"model154">; status: Model154Status }
  | { kind: "deleted"; id: Id<"model154">; reason?: string };

export type Model154Events = {
  change: Model154Event;
  error: { message: string; code: number };
};

export type Model154Numbers = KeysOfType<Model154Line, number>;
export type FrozenModel154 = DeepReadonly<Model154>;

export function describeModel154Event(event: Model154Event): string {
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

export function totalModel154(item: Model154): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel154(item: Model154): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel154(item), 3), (item.parent ? summarizeModel153(item.parent) : "-"), item.related.map(summarizeModel147).join(",")].join(" | ");
}

export class Model154Service extends Service<Model154, "model154"> {
  readonly events = new EventBus<Model154Events>();
  private readonly parents?: Model153Service;

  constructor(repository = new MemoryRepository<Model154, "model154">()) {
    super(repository);
  }

  validate(item: Model154): string[] {
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

  rename(id: Id<"model154">, to: string): Result<Model154> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model154 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model154">, status: Model154Status): Result<Model154Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model154">, patch: Patch<Pick<Model154, "name" | "tags">>): Model154 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model154Status, Model154[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model154Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model154">[]): Promise<Model154[]> {
    const found: Model154[] = [];
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

  linkedService(): Model153Service {
    return this.parents ?? new Model153Service();
  }
}

export function makeModel154(id: string, name: string): Model154 {
  return {
    id: id as Id<"model154">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 155, unit: "hour" }],
    related: [],
  };
}

export const model154Defaults: FrozenModel154 = makeModel154("default-154", "Default 154");
export const model154Label = summarizeModel154(makeModel154("label", "Label"));
