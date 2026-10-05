// Generated domain module 36 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model035Service, summarizeModel035 } from "./model035";
import type { Model035 } from "./model035";
import { Model029Service, summarizeModel029 } from "./model029";
import type { Model029 } from "./model029";

export type Model036Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model036Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model036 extends Entity<"model036"> {
  updatedAt?: number;
  name: string;
  status: Model036Status;
  tags: string[];
  lines: Model036Line[];
  owner?: { name: string; email?: string };
  parent?: Model035;
  related: Model029[];
}

export type Model036Event =
  | { kind: "created"; item: Model036 }
  | { kind: "renamed"; id: Id<"model036">; from: string; to: string }
  | { kind: "moved"; id: Id<"model036">; status: Model036Status }
  | { kind: "deleted"; id: Id<"model036">; reason?: string };

export type Model036Events = {
  change: Model036Event;
  error: { message: string; code: number };
};

export type Model036Numbers = KeysOfType<Model036Line, number>;
export type FrozenModel036 = DeepReadonly<Model036>;

export function describeModel036Event(event: Model036Event): string {
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

export function totalModel036(item: Model036): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel036(item: Model036): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel036(item), 3), (item.parent ? summarizeModel035(item.parent) : "-"), item.related.map(summarizeModel029).join(",")].join(" | ");
}

export class Model036Service extends Service<Model036, "model036"> {
  readonly events = new EventBus<Model036Events>();
  private readonly parents?: Model035Service;

  constructor(repository = new MemoryRepository<Model036, "model036">()) {
    super(repository);
  }

  validate(item: Model036): string[] {
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

  rename(id: Id<"model036">, to: string): Result<Model036> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model036 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model036">, status: Model036Status): Result<Model036Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model036">, patch: Patch<Pick<Model036, "name" | "tags">>): Model036 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model036Status, Model036[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model036Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model036">[]): Promise<Model036[]> {
    const found: Model036[] = [];
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

  linkedService(): Model035Service {
    return this.parents ?? new Model035Service();
  }
}

export function makeModel036(id: string, name: string): Model036 {
  return {
    id: id as Id<"model036">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 37, unit: "m" }],
    related: [],
  };
}

export const model036Defaults: FrozenModel036 = makeModel036("default-36", "Default 36");
export const model036Label = summarizeModel036(makeModel036("label", "Label"));
