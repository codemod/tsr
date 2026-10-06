// Generated domain module 29 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model028Service, summarizeModel028 } from "./model028";
import type { Model028 } from "./model028";
import { Model022Service, summarizeModel022 } from "./model022";
import type { Model022 } from "./model022";

export type Model029Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model029Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model029 extends Entity<"model029"> {
  updatedAt?: number;
  name: string;
  status: Model029Status;
  tags: string[];
  lines: Model029Line[];
  owner?: { name: string; email?: string };
  parent?: Model028;
  related: Model022[];
}

export type Model029Event =
  | { kind: "created"; item: Model029 }
  | { kind: "renamed"; id: Id<"model029">; from: string; to: string }
  | { kind: "moved"; id: Id<"model029">; status: Model029Status }
  | { kind: "deleted"; id: Id<"model029">; reason?: string };

export type Model029Events = {
  change: Model029Event;
  error: { message: string; code: number };
};

export type Model029Numbers = KeysOfType<Model029Line, number>;
export type FrozenModel029 = DeepReadonly<Model029>;

export function describeModel029Event(event: Model029Event): string {
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

export function totalModel029(item: Model029): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel029(item: Model029): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel029(item), 3), (item.parent ? summarizeModel028(item.parent) : "-"), item.related.map(summarizeModel022).join(",")].join(" | ");
}

export class Model029Service extends Service<Model029, "model029"> {
  readonly events = new EventBus<Model029Events>();
  private readonly parents?: Model028Service;

  constructor(repository = new MemoryRepository<Model029, "model029">()) {
    super(repository);
  }

  validate(item: Model029): string[] {
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

  rename(id: Id<"model029">, to: string): Result<Model029> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model029 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model029">, status: Model029Status): Result<Model029Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model029">, patch: Patch<Pick<Model029, "name" | "tags">>): Model029 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model029Status, Model029[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model029Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model029">[]): Promise<Model029[]> {
    const found: Model029[] = [];
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

  linkedService(): Model028Service {
    return this.parents ?? new Model028Service();
  }
}

export function makeModel029(id: string, name: string): Model029 {
  return {
    id: id as Id<"model029">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 30, unit: "hour" }],
    related: [],
  };
}

export const model029Defaults: FrozenModel029 = makeModel029("default-29", "Default 29");
export const model029Label = summarizeModel029(makeModel029("label", "Label"));
