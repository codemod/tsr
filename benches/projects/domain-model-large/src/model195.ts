// Generated domain module 195 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model194Service, summarizeModel194 } from "./model194";
import type { Model194 } from "./model194";
import { Model188Service, summarizeModel188 } from "./model188";
import type { Model188 } from "./model188";

export type Model195Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model195Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model195 extends Entity<"model195"> {
  updatedAt?: number;
  name: string;
  status: Model195Status;
  tags: string[];
  lines: Model195Line[];
  owner?: { name: string; email?: string };
  parent?: Model194;
  related: Model188[];
}

export type Model195Event =
  | { kind: "created"; item: Model195 }
  | { kind: "renamed"; id: Id<"model195">; from: string; to: string }
  | { kind: "moved"; id: Id<"model195">; status: Model195Status }
  | { kind: "deleted"; id: Id<"model195">; reason?: string };

export type Model195Events = {
  change: Model195Event;
  error: { message: string; code: number };
};

export type Model195Numbers = KeysOfType<Model195Line, number>;
export type FrozenModel195 = DeepReadonly<Model195>;

export function describeModel195Event(event: Model195Event): string {
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

export function totalModel195(item: Model195): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel195(item: Model195): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel195(item), 3), (item.parent ? summarizeModel194(item.parent) : "-"), item.related.map(summarizeModel188).join(",")].join(" | ");
}

export class Model195Service extends Service<Model195, "model195"> {
  readonly events = new EventBus<Model195Events>();
  private readonly parents?: Model194Service;

  constructor(repository = new MemoryRepository<Model195, "model195">()) {
    super(repository);
  }

  validate(item: Model195): string[] {
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

  rename(id: Id<"model195">, to: string): Result<Model195> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model195 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model195">, status: Model195Status): Result<Model195Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model195">, patch: Patch<Pick<Model195, "name" | "tags">>): Model195 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model195Status, Model195[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model195Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model195">[]): Promise<Model195[]> {
    const found: Model195[] = [];
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

  linkedService(): Model194Service {
    return this.parents ?? new Model194Service();
  }
}

export function makeModel195(id: string, name: string): Model195 {
  return {
    id: id as Id<"model195">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 196, unit: "kg" }],
    related: [],
  };
}

export const model195Defaults: FrozenModel195 = makeModel195("default-195", "Default 195");
export const model195Label = summarizeModel195(makeModel195("label", "Label"));
