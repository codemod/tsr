// Generated domain module 30 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model029Service, summarizeModel029 } from "./model029";
import type { Model029 } from "./model029";
import { Model023Service, summarizeModel023 } from "./model023";
import type { Model023 } from "./model023";

export type Model030Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model030Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model030 extends Entity<"model030"> {
  updatedAt?: number;
  name: string;
  status: Model030Status;
  tags: string[];
  lines: Model030Line[];
  owner?: { name: string; email?: string };
  parent?: Model029;
  related: Model023[];
}

export type Model030Event =
  | { kind: "created"; item: Model030 }
  | { kind: "renamed"; id: Id<"model030">; from: string; to: string }
  | { kind: "moved"; id: Id<"model030">; status: Model030Status }
  | { kind: "deleted"; id: Id<"model030">; reason?: string };

export type Model030Events = {
  change: Model030Event;
  error: { message: string; code: number };
};

export type Model030Numbers = KeysOfType<Model030Line, number>;
export type FrozenModel030 = DeepReadonly<Model030>;

export function describeModel030Event(event: Model030Event): string {
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

export function totalModel030(item: Model030): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel030(item: Model030): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel030(item), 3), (item.parent ? summarizeModel029(item.parent) : "-"), item.related.map(summarizeModel023).join(",")].join(" | ");
}

export class Model030Service extends Service<Model030, "model030"> {
  readonly events = new EventBus<Model030Events>();
  private readonly parents?: Model029Service;

  constructor(repository = new MemoryRepository<Model030, "model030">()) {
    super(repository);
  }

  validate(item: Model030): string[] {
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

  rename(id: Id<"model030">, to: string): Result<Model030> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model030 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model030">, status: Model030Status): Result<Model030Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model030">, patch: Patch<Pick<Model030, "name" | "tags">>): Model030 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model030Status, Model030[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model030Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model030">[]): Promise<Model030[]> {
    const found: Model030[] = [];
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

  linkedService(): Model029Service {
    return this.parents ?? new Model029Service();
  }
}

export function makeModel030(id: string, name: string): Model030 {
  return {
    id: id as Id<"model030">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 31, unit: "kg" }],
    related: [],
  };
}

export const model030Defaults: FrozenModel030 = makeModel030("default-30", "Default 30");
export const model030Label = summarizeModel030(makeModel030("label", "Label"));
