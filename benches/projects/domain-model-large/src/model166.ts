// Generated domain module 166 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model165Service, summarizeModel165 } from "./model165";
import type { Model165 } from "./model165";
import { Model159Service, summarizeModel159 } from "./model159";
import type { Model159 } from "./model159";

export type Model166Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model166Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model166 extends Entity<"model166"> {
  updatedAt?: number;
  name: string;
  status: Model166Status;
  tags: string[];
  lines: Model166Line[];
  owner?: { name: string; email?: string };
  parent?: Model165;
  related: Model159[];
}

export type Model166Event =
  | { kind: "created"; item: Model166 }
  | { kind: "renamed"; id: Id<"model166">; from: string; to: string }
  | { kind: "moved"; id: Id<"model166">; status: Model166Status }
  | { kind: "deleted"; id: Id<"model166">; reason?: string };

export type Model166Events = {
  change: Model166Event;
  error: { message: string; code: number };
};

export type Model166Numbers = KeysOfType<Model166Line, number>;
export type FrozenModel166 = DeepReadonly<Model166>;

export function describeModel166Event(event: Model166Event): string {
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

export function totalModel166(item: Model166): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel166(item: Model166): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel166(item), 3), (item.parent ? summarizeModel165(item.parent) : "-"), item.related.map(summarizeModel159).join(",")].join(" | ");
}

export class Model166Service extends Service<Model166, "model166"> {
  readonly events = new EventBus<Model166Events>();
  private readonly parents?: Model165Service;

  constructor(repository = new MemoryRepository<Model166, "model166">()) {
    super(repository);
  }

  validate(item: Model166): string[] {
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

  rename(id: Id<"model166">, to: string): Result<Model166> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model166 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model166">, status: Model166Status): Result<Model166Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model166">, patch: Patch<Pick<Model166, "name" | "tags">>): Model166 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model166Status, Model166[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model166Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model166">[]): Promise<Model166[]> {
    const found: Model166[] = [];
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

  linkedService(): Model165Service {
    return this.parents ?? new Model165Service();
  }
}

export function makeModel166(id: string, name: string): Model166 {
  return {
    id: id as Id<"model166">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 167, unit: "m" }],
    related: [],
  };
}

export const model166Defaults: FrozenModel166 = makeModel166("default-166", "Default 166");
export const model166Label = summarizeModel166(makeModel166("label", "Label"));
