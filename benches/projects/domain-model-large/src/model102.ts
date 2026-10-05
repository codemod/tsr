// Generated domain module 102 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model101Service, summarizeModel101 } from "./model101";
import type { Model101 } from "./model101";
import { Model095Service, summarizeModel095 } from "./model095";
import type { Model095 } from "./model095";

export type Model102Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model102Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model102 extends Entity<"model102"> {
  updatedAt?: number;
  name: string;
  status: Model102Status;
  tags: string[];
  lines: Model102Line[];
  owner?: { name: string; email?: string };
  parent?: Model101;
  related: Model095[];
}

export type Model102Event =
  | { kind: "created"; item: Model102 }
  | { kind: "renamed"; id: Id<"model102">; from: string; to: string }
  | { kind: "moved"; id: Id<"model102">; status: Model102Status }
  | { kind: "deleted"; id: Id<"model102">; reason?: string };

export type Model102Events = {
  change: Model102Event;
  error: { message: string; code: number };
};

export type Model102Numbers = KeysOfType<Model102Line, number>;
export type FrozenModel102 = DeepReadonly<Model102>;

export function describeModel102Event(event: Model102Event): string {
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

export function totalModel102(item: Model102): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel102(item: Model102): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel102(item), 3), (item.parent ? summarizeModel101(item.parent) : "-"), item.related.map(summarizeModel095).join(",")].join(" | ");
}

export class Model102Service extends Service<Model102, "model102"> {
  readonly events = new EventBus<Model102Events>();
  private readonly parents?: Model101Service;

  constructor(repository = new MemoryRepository<Model102, "model102">()) {
    super(repository);
  }

  validate(item: Model102): string[] {
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

  rename(id: Id<"model102">, to: string): Result<Model102> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model102 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model102">, status: Model102Status): Result<Model102Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model102">, patch: Patch<Pick<Model102, "name" | "tags">>): Model102 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model102Status, Model102[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model102Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model102">[]): Promise<Model102[]> {
    const found: Model102[] = [];
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

  linkedService(): Model101Service {
    return this.parents ?? new Model101Service();
  }
}

export function makeModel102(id: string, name: string): Model102 {
  return {
    id: id as Id<"model102">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 103, unit: "s" }],
    related: [],
  };
}

export const model102Defaults: FrozenModel102 = makeModel102("default-102", "Default 102");
export const model102Label = summarizeModel102(makeModel102("label", "Label"));
