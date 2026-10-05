// Generated domain module 180 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model179Service, summarizeModel179 } from "./model179";
import type { Model179 } from "./model179";
import { Model173Service, summarizeModel173 } from "./model173";
import type { Model173 } from "./model173";

export type Model180Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model180Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model180 extends Entity<"model180"> {
  updatedAt?: number;
  name: string;
  status: Model180Status;
  tags: string[];
  lines: Model180Line[];
  owner?: { name: string; email?: string };
  parent?: Model179;
  related: Model173[];
}

export type Model180Event =
  | { kind: "created"; item: Model180 }
  | { kind: "renamed"; id: Id<"model180">; from: string; to: string }
  | { kind: "moved"; id: Id<"model180">; status: Model180Status }
  | { kind: "deleted"; id: Id<"model180">; reason?: string };

export type Model180Events = {
  change: Model180Event;
  error: { message: string; code: number };
};

export type Model180Numbers = KeysOfType<Model180Line, number>;
export type FrozenModel180 = DeepReadonly<Model180>;

export function describeModel180Event(event: Model180Event): string {
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

export function totalModel180(item: Model180): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel180(item: Model180): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel180(item), 3), (item.parent ? summarizeModel179(item.parent) : "-"), item.related.map(summarizeModel173).join(",")].join(" | ");
}

export class Model180Service extends Service<Model180, "model180"> {
  readonly events = new EventBus<Model180Events>();
  private readonly parents?: Model179Service;

  constructor(repository = new MemoryRepository<Model180, "model180">()) {
    super(repository);
  }

  validate(item: Model180): string[] {
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

  rename(id: Id<"model180">, to: string): Result<Model180> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model180 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model180">, status: Model180Status): Result<Model180Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model180">, patch: Patch<Pick<Model180, "name" | "tags">>): Model180 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model180Status, Model180[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model180Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model180">[]): Promise<Model180[]> {
    const found: Model180[] = [];
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

  linkedService(): Model179Service {
    return this.parents ?? new Model179Service();
  }
}

export function makeModel180(id: string, name: string): Model180 {
  return {
    id: id as Id<"model180">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 181, unit: "kg" }],
    related: [],
  };
}

export const model180Defaults: FrozenModel180 = makeModel180("default-180", "Default 180");
export const model180Label = summarizeModel180(makeModel180("label", "Label"));
