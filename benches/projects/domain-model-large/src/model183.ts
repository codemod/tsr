// Generated domain module 183 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model182Service, summarizeModel182 } from "./model182";
import type { Model182 } from "./model182";
import { Model176Service, summarizeModel176 } from "./model176";
import type { Model176 } from "./model176";

export type Model183Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model183Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model183 extends Entity<"model183"> {
  updatedAt?: number;
  name: string;
  status: Model183Status;
  tags: string[];
  lines: Model183Line[];
  owner?: { name: string; email?: string };
  parent?: Model182;
  related: Model176[];
}

export type Model183Event =
  | { kind: "created"; item: Model183 }
  | { kind: "renamed"; id: Id<"model183">; from: string; to: string }
  | { kind: "moved"; id: Id<"model183">; status: Model183Status }
  | { kind: "deleted"; id: Id<"model183">; reason?: string };

export type Model183Events = {
  change: Model183Event;
  error: { message: string; code: number };
};

export type Model183Numbers = KeysOfType<Model183Line, number>;
export type FrozenModel183 = DeepReadonly<Model183>;

export function describeModel183Event(event: Model183Event): string {
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

export function totalModel183(item: Model183): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel183(item: Model183): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel183(item), 3), (item.parent ? summarizeModel182(item.parent) : "-"), item.related.map(summarizeModel176).join(",")].join(" | ");
}

export class Model183Service extends Service<Model183, "model183"> {
  readonly events = new EventBus<Model183Events>();
  private readonly parents?: Model182Service;

  constructor(repository = new MemoryRepository<Model183, "model183">()) {
    super(repository);
  }

  validate(item: Model183): string[] {
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

  rename(id: Id<"model183">, to: string): Result<Model183> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model183 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model183">, status: Model183Status): Result<Model183Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model183">, patch: Patch<Pick<Model183, "name" | "tags">>): Model183 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model183Status, Model183[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model183Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model183">[]): Promise<Model183[]> {
    const found: Model183[] = [];
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

  linkedService(): Model182Service {
    return this.parents ?? new Model182Service();
  }
}

export function makeModel183(id: string, name: string): Model183 {
  return {
    id: id as Id<"model183">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 184, unit: "item" }],
    related: [],
  };
}

export const model183Defaults: FrozenModel183 = makeModel183("default-183", "Default 183");
export const model183Label = summarizeModel183(makeModel183("label", "Label"));
