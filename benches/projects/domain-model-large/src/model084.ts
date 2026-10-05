// Generated domain module 84 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model083Service, summarizeModel083 } from "./model083";
import type { Model083 } from "./model083";
import { Model077Service, summarizeModel077 } from "./model077";
import type { Model077 } from "./model077";

export type Model084Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model084Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model084 extends Entity<"model084"> {
  updatedAt?: number;
  name: string;
  status: Model084Status;
  tags: string[];
  lines: Model084Line[];
  owner?: { name: string; email?: string };
  parent?: Model083;
  related: Model077[];
}

export type Model084Event =
  | { kind: "created"; item: Model084 }
  | { kind: "renamed"; id: Id<"model084">; from: string; to: string }
  | { kind: "moved"; id: Id<"model084">; status: Model084Status }
  | { kind: "deleted"; id: Id<"model084">; reason?: string };

export type Model084Events = {
  change: Model084Event;
  error: { message: string; code: number };
};

export type Model084Numbers = KeysOfType<Model084Line, number>;
export type FrozenModel084 = DeepReadonly<Model084>;

export function describeModel084Event(event: Model084Event): string {
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

export function totalModel084(item: Model084): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel084(item: Model084): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel084(item), 3), (item.parent ? summarizeModel083(item.parent) : "-"), item.related.map(summarizeModel077).join(",")].join(" | ");
}

export class Model084Service extends Service<Model084, "model084"> {
  readonly events = new EventBus<Model084Events>();
  private readonly parents?: Model083Service;

  constructor(repository = new MemoryRepository<Model084, "model084">()) {
    super(repository);
  }

  validate(item: Model084): string[] {
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

  rename(id: Id<"model084">, to: string): Result<Model084> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model084 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model084">, status: Model084Status): Result<Model084Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model084">, patch: Patch<Pick<Model084, "name" | "tags">>): Model084 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model084Status, Model084[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model084Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model084">[]): Promise<Model084[]> {
    const found: Model084[] = [];
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

  linkedService(): Model083Service {
    return this.parents ?? new Model083Service();
  }
}

export function makeModel084(id: string, name: string): Model084 {
  return {
    id: id as Id<"model084">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 85, unit: "hour" }],
    related: [],
  };
}

export const model084Defaults: FrozenModel084 = makeModel084("default-84", "Default 84");
export const model084Label = summarizeModel084(makeModel084("label", "Label"));
