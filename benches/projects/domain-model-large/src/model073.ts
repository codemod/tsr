// Generated domain module 73 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model072Service, summarizeModel072 } from "./model072";
import type { Model072 } from "./model072";
import { Model066Service, summarizeModel066 } from "./model066";
import type { Model066 } from "./model066";

export type Model073Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model073Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model073 extends Entity<"model073"> {
  updatedAt?: number;
  name: string;
  status: Model073Status;
  tags: string[];
  lines: Model073Line[];
  owner?: { name: string; email?: string };
  parent?: Model072;
  related: Model066[];
}

export type Model073Event =
  | { kind: "created"; item: Model073 }
  | { kind: "renamed"; id: Id<"model073">; from: string; to: string }
  | { kind: "moved"; id: Id<"model073">; status: Model073Status }
  | { kind: "deleted"; id: Id<"model073">; reason?: string };

export type Model073Events = {
  change: Model073Event;
  error: { message: string; code: number };
};

export type Model073Numbers = KeysOfType<Model073Line, number>;
export type FrozenModel073 = DeepReadonly<Model073>;

export function describeModel073Event(event: Model073Event): string {
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

export function totalModel073(item: Model073): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel073(item: Model073): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel073(item), 3), (item.parent ? summarizeModel072(item.parent) : "-"), item.related.map(summarizeModel066).join(",")].join(" | ");
}

export class Model073Service extends Service<Model073, "model073"> {
  readonly events = new EventBus<Model073Events>();
  private readonly parents?: Model072Service;

  constructor(repository = new MemoryRepository<Model073, "model073">()) {
    super(repository);
  }

  validate(item: Model073): string[] {
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

  rename(id: Id<"model073">, to: string): Result<Model073> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model073 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model073">, status: Model073Status): Result<Model073Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model073">, patch: Patch<Pick<Model073, "name" | "tags">>): Model073 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model073Status, Model073[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model073Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model073">[]): Promise<Model073[]> {
    const found: Model073[] = [];
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

  linkedService(): Model072Service {
    return this.parents ?? new Model072Service();
  }
}

export function makeModel073(id: string, name: string): Model073 {
  return {
    id: id as Id<"model073">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 74, unit: "item" }],
    related: [],
  };
}

export const model073Defaults: FrozenModel073 = makeModel073("default-73", "Default 73");
export const model073Label = summarizeModel073(makeModel073("label", "Label"));
