// Generated domain module 37 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model036Service, summarizeModel036 } from "./model036";
import type { Model036 } from "./model036";
import { Model030Service, summarizeModel030 } from "./model030";
import type { Model030 } from "./model030";

export type Model037Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model037Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model037 extends Entity<"model037"> {
  updatedAt?: number;
  name: string;
  status: Model037Status;
  tags: string[];
  lines: Model037Line[];
  owner?: { name: string; email?: string };
  parent?: Model036;
  related: Model030[];
}

export type Model037Event =
  | { kind: "created"; item: Model037 }
  | { kind: "renamed"; id: Id<"model037">; from: string; to: string }
  | { kind: "moved"; id: Id<"model037">; status: Model037Status }
  | { kind: "deleted"; id: Id<"model037">; reason?: string };

export type Model037Events = {
  change: Model037Event;
  error: { message: string; code: number };
};

export type Model037Numbers = KeysOfType<Model037Line, number>;
export type FrozenModel037 = DeepReadonly<Model037>;

export function describeModel037Event(event: Model037Event): string {
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

export function totalModel037(item: Model037): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel037(item: Model037): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel037(item), 3), (item.parent ? summarizeModel036(item.parent) : "-"), item.related.map(summarizeModel030).join(",")].join(" | ");
}

export class Model037Service extends Service<Model037, "model037"> {
  readonly events = new EventBus<Model037Events>();
  private readonly parents?: Model036Service;

  constructor(repository = new MemoryRepository<Model037, "model037">()) {
    super(repository);
  }

  validate(item: Model037): string[] {
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

  rename(id: Id<"model037">, to: string): Result<Model037> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model037 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model037">, status: Model037Status): Result<Model037Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model037">, patch: Patch<Pick<Model037, "name" | "tags">>): Model037 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model037Status, Model037[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model037Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model037">[]): Promise<Model037[]> {
    const found: Model037[] = [];
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

  linkedService(): Model036Service {
    return this.parents ?? new Model036Service();
  }
}

export function makeModel037(id: string, name: string): Model037 {
  return {
    id: id as Id<"model037">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 38, unit: "s" }],
    related: [],
  };
}

export const model037Defaults: FrozenModel037 = makeModel037("default-37", "Default 37");
export const model037Label = summarizeModel037(makeModel037("label", "Label"));
