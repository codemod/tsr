// Generated domain module 25 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model024Service, summarizeModel024 } from "./model024";
import type { Model024 } from "./model024";
import { Model018Service, summarizeModel018 } from "./model018";
import type { Model018 } from "./model018";

export type Model025Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model025Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model025 extends Entity<"model025"> {
  updatedAt?: number;
  name: string;
  status: Model025Status;
  tags: string[];
  lines: Model025Line[];
  owner?: { name: string; email?: string };
  parent?: Model024;
  related: Model018[];
}

export type Model025Event =
  | { kind: "created"; item: Model025 }
  | { kind: "renamed"; id: Id<"model025">; from: string; to: string }
  | { kind: "moved"; id: Id<"model025">; status: Model025Status }
  | { kind: "deleted"; id: Id<"model025">; reason?: string };

export type Model025Events = {
  change: Model025Event;
  error: { message: string; code: number };
};

export type Model025Numbers = KeysOfType<Model025Line, number>;
export type FrozenModel025 = DeepReadonly<Model025>;

export function describeModel025Event(event: Model025Event): string {
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

export function totalModel025(item: Model025): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel025(item: Model025): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel025(item), 3), (item.parent ? summarizeModel024(item.parent) : "-"), item.related.map(summarizeModel018).join(",")].join(" | ");
}

export class Model025Service extends Service<Model025, "model025"> {
  readonly events = new EventBus<Model025Events>();
  private readonly parents?: Model024Service;

  constructor(repository = new MemoryRepository<Model025, "model025">()) {
    super(repository);
  }

  validate(item: Model025): string[] {
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

  rename(id: Id<"model025">, to: string): Result<Model025> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model025 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model025">, status: Model025Status): Result<Model025Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model025">, patch: Patch<Pick<Model025, "name" | "tags">>): Model025 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model025Status, Model025[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model025Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model025">[]): Promise<Model025[]> {
    const found: Model025[] = [];
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

  linkedService(): Model024Service {
    return this.parents ?? new Model024Service();
  }
}

export function makeModel025(id: string, name: string): Model025 {
  return {
    id: id as Id<"model025">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 26, unit: "kg" }],
    related: [],
  };
}

export const model025Defaults: FrozenModel025 = makeModel025("default-25", "Default 25");
export const model025Label = summarizeModel025(makeModel025("label", "Label"));
