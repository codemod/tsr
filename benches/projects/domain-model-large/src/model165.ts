// Generated domain module 165 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model164Service, summarizeModel164 } from "./model164";
import type { Model164 } from "./model164";
import { Model158Service, summarizeModel158 } from "./model158";
import type { Model158 } from "./model158";

export type Model165Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model165Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model165 extends Entity<"model165"> {
  updatedAt?: number;
  name: string;
  status: Model165Status;
  tags: string[];
  lines: Model165Line[];
  owner?: { name: string; email?: string };
  parent?: Model164;
  related: Model158[];
}

export type Model165Event =
  | { kind: "created"; item: Model165 }
  | { kind: "renamed"; id: Id<"model165">; from: string; to: string }
  | { kind: "moved"; id: Id<"model165">; status: Model165Status }
  | { kind: "deleted"; id: Id<"model165">; reason?: string };

export type Model165Events = {
  change: Model165Event;
  error: { message: string; code: number };
};

export type Model165Numbers = KeysOfType<Model165Line, number>;
export type FrozenModel165 = DeepReadonly<Model165>;

export function describeModel165Event(event: Model165Event): string {
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

export function totalModel165(item: Model165): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel165(item: Model165): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel165(item), 3), (item.parent ? summarizeModel164(item.parent) : "-"), item.related.map(summarizeModel158).join(",")].join(" | ");
}

export class Model165Service extends Service<Model165, "model165"> {
  readonly events = new EventBus<Model165Events>();
  private readonly parents?: Model164Service;

  constructor(repository = new MemoryRepository<Model165, "model165">()) {
    super(repository);
  }

  validate(item: Model165): string[] {
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

  rename(id: Id<"model165">, to: string): Result<Model165> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model165 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model165">, status: Model165Status): Result<Model165Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model165">, patch: Patch<Pick<Model165, "name" | "tags">>): Model165 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model165Status, Model165[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model165Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model165">[]): Promise<Model165[]> {
    const found: Model165[] = [];
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

  linkedService(): Model164Service {
    return this.parents ?? new Model164Service();
  }
}

export function makeModel165(id: string, name: string): Model165 {
  return {
    id: id as Id<"model165">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 166, unit: "kg" }],
    related: [],
  };
}

export const model165Defaults: FrozenModel165 = makeModel165("default-165", "Default 165");
export const model165Label = summarizeModel165(makeModel165("label", "Label"));
