// Generated domain module 75 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model074Service, summarizeModel074 } from "./model074";
import type { Model074 } from "./model074";
import { Model068Service, summarizeModel068 } from "./model068";
import type { Model068 } from "./model068";

export type Model075Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model075Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model075 extends Entity<"model075"> {
  updatedAt?: number;
  name: string;
  status: Model075Status;
  tags: string[];
  lines: Model075Line[];
  owner?: { name: string; email?: string };
  parent?: Model074;
  related: Model068[];
}

export type Model075Event =
  | { kind: "created"; item: Model075 }
  | { kind: "renamed"; id: Id<"model075">; from: string; to: string }
  | { kind: "moved"; id: Id<"model075">; status: Model075Status }
  | { kind: "deleted"; id: Id<"model075">; reason?: string };

export type Model075Events = {
  change: Model075Event;
  error: { message: string; code: number };
};

export type Model075Numbers = KeysOfType<Model075Line, number>;
export type FrozenModel075 = DeepReadonly<Model075>;

export function describeModel075Event(event: Model075Event): string {
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

export function totalModel075(item: Model075): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel075(item: Model075): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel075(item), 3), (item.parent ? summarizeModel074(item.parent) : "-"), item.related.map(summarizeModel068).join(",")].join(" | ");
}

export class Model075Service extends Service<Model075, "model075"> {
  readonly events = new EventBus<Model075Events>();
  private readonly parents?: Model074Service;

  constructor(repository = new MemoryRepository<Model075, "model075">()) {
    super(repository);
  }

  validate(item: Model075): string[] {
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

  rename(id: Id<"model075">, to: string): Result<Model075> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model075 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model075">, status: Model075Status): Result<Model075Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model075">, patch: Patch<Pick<Model075, "name" | "tags">>): Model075 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model075Status, Model075[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model075Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model075">[]): Promise<Model075[]> {
    const found: Model075[] = [];
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

  linkedService(): Model074Service {
    return this.parents ?? new Model074Service();
  }
}

export function makeModel075(id: string, name: string): Model075 {
  return {
    id: id as Id<"model075">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 76, unit: "kg" }],
    related: [],
  };
}

export const model075Defaults: FrozenModel075 = makeModel075("default-75", "Default 75");
export const model075Label = summarizeModel075(makeModel075("label", "Label"));
