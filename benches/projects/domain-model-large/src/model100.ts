// Generated domain module 100 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model099Service, summarizeModel099 } from "./model099";
import type { Model099 } from "./model099";
import { Model093Service, summarizeModel093 } from "./model093";
import type { Model093 } from "./model093";

export type Model100Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model100Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model100 extends Entity<"model100"> {
  updatedAt?: number;
  name: string;
  status: Model100Status;
  tags: string[];
  lines: Model100Line[];
  owner?: { name: string; email?: string };
  parent?: Model099;
  related: Model093[];
}

export type Model100Event =
  | { kind: "created"; item: Model100 }
  | { kind: "renamed"; id: Id<"model100">; from: string; to: string }
  | { kind: "moved"; id: Id<"model100">; status: Model100Status }
  | { kind: "deleted"; id: Id<"model100">; reason?: string };

export type Model100Events = {
  change: Model100Event;
  error: { message: string; code: number };
};

export type Model100Numbers = KeysOfType<Model100Line, number>;
export type FrozenModel100 = DeepReadonly<Model100>;

export function describeModel100Event(event: Model100Event): string {
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

export function totalModel100(item: Model100): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel100(item: Model100): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel100(item), 3), (item.parent ? summarizeModel099(item.parent) : "-"), item.related.map(summarizeModel093).join(",")].join(" | ");
}

export class Model100Service extends Service<Model100, "model100"> {
  readonly events = new EventBus<Model100Events>();
  private readonly parents?: Model099Service;

  constructor(repository = new MemoryRepository<Model100, "model100">()) {
    super(repository);
  }

  validate(item: Model100): string[] {
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

  rename(id: Id<"model100">, to: string): Result<Model100> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model100 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model100">, status: Model100Status): Result<Model100Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model100">, patch: Patch<Pick<Model100, "name" | "tags">>): Model100 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model100Status, Model100[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model100Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model100">[]): Promise<Model100[]> {
    const found: Model100[] = [];
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

  linkedService(): Model099Service {
    return this.parents ?? new Model099Service();
  }
}

export function makeModel100(id: string, name: string): Model100 {
  return {
    id: id as Id<"model100">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 101, unit: "kg" }],
    related: [],
  };
}

export const model100Defaults: FrozenModel100 = makeModel100("default-100", "Default 100");
export const model100Label = summarizeModel100(makeModel100("label", "Label"));
