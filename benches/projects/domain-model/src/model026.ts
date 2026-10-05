// Generated domain module 26 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model025Service, summarizeModel025 } from "./model025";
import type { Model025 } from "./model025";
import { Model019Service, summarizeModel019 } from "./model019";
import type { Model019 } from "./model019";

export type Model026Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model026Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model026 extends Entity<"model026"> {
  updatedAt?: number;
  name: string;
  status: Model026Status;
  tags: string[];
  lines: Model026Line[];
  owner?: { name: string; email?: string };
  parent?: Model025;
  related: Model019[];
}

export type Model026Event =
  | { kind: "created"; item: Model026 }
  | { kind: "renamed"; id: Id<"model026">; from: string; to: string }
  | { kind: "moved"; id: Id<"model026">; status: Model026Status }
  | { kind: "deleted"; id: Id<"model026">; reason?: string };

export type Model026Events = {
  change: Model026Event;
  error: { message: string; code: number };
};

export type Model026Numbers = KeysOfType<Model026Line, number>;
export type FrozenModel026 = DeepReadonly<Model026>;

export function describeModel026Event(event: Model026Event): string {
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

export function totalModel026(item: Model026): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel026(item: Model026): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel026(item), 3), (item.parent ? summarizeModel025(item.parent) : "-"), item.related.map(summarizeModel019).join(",")].join(" | ");
}

export class Model026Service extends Service<Model026, "model026"> {
  readonly events = new EventBus<Model026Events>();
  private readonly parents?: Model025Service;

  constructor(repository = new MemoryRepository<Model026, "model026">()) {
    super(repository);
  }

  validate(item: Model026): string[] {
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

  rename(id: Id<"model026">, to: string): Result<Model026> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model026 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model026">, status: Model026Status): Result<Model026Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model026">, patch: Patch<Pick<Model026, "name" | "tags">>): Model026 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model026Status, Model026[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model026Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model026">[]): Promise<Model026[]> {
    const found: Model026[] = [];
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

  linkedService(): Model025Service {
    return this.parents ?? new Model025Service();
  }
}

export function makeModel026(id: string, name: string): Model026 {
  return {
    id: id as Id<"model026">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 27, unit: "m" }],
    related: [],
  };
}

export const model026Defaults: FrozenModel026 = makeModel026("default-26", "Default 26");
export const model026Label = summarizeModel026(makeModel026("label", "Label"));
