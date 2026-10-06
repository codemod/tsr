// Generated domain module 157 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model156Service, summarizeModel156 } from "./model156";
import type { Model156 } from "./model156";
import { Model150Service, summarizeModel150 } from "./model150";
import type { Model150 } from "./model150";

export type Model157Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model157Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model157 extends Entity<"model157"> {
  updatedAt?: number;
  name: string;
  status: Model157Status;
  tags: string[];
  lines: Model157Line[];
  owner?: { name: string; email?: string };
  parent?: Model156;
  related: Model150[];
}

export type Model157Event =
  | { kind: "created"; item: Model157 }
  | { kind: "renamed"; id: Id<"model157">; from: string; to: string }
  | { kind: "moved"; id: Id<"model157">; status: Model157Status }
  | { kind: "deleted"; id: Id<"model157">; reason?: string };

export type Model157Events = {
  change: Model157Event;
  error: { message: string; code: number };
};

export type Model157Numbers = KeysOfType<Model157Line, number>;
export type FrozenModel157 = DeepReadonly<Model157>;

export function describeModel157Event(event: Model157Event): string {
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

export function totalModel157(item: Model157): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel157(item: Model157): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel157(item), 3), (item.parent ? summarizeModel156(item.parent) : "-"), item.related.map(summarizeModel150).join(",")].join(" | ");
}

export class Model157Service extends Service<Model157, "model157"> {
  readonly events = new EventBus<Model157Events>();
  private readonly parents?: Model156Service;

  constructor(repository = new MemoryRepository<Model157, "model157">()) {
    super(repository);
  }

  validate(item: Model157): string[] {
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

  rename(id: Id<"model157">, to: string): Result<Model157> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model157 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model157">, status: Model157Status): Result<Model157Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model157">, patch: Patch<Pick<Model157, "name" | "tags">>): Model157 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model157Status, Model157[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model157Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model157">[]): Promise<Model157[]> {
    const found: Model157[] = [];
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

  linkedService(): Model156Service {
    return this.parents ?? new Model156Service();
  }
}

export function makeModel157(id: string, name: string): Model157 {
  return {
    id: id as Id<"model157">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 158, unit: "s" }],
    related: [],
  };
}

export const model157Defaults: FrozenModel157 = makeModel157("default-157", "Default 157");
export const model157Label = summarizeModel157(makeModel157("label", "Label"));
