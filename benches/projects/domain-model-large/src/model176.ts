// Generated domain module 176 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model175Service, summarizeModel175 } from "./model175";
import type { Model175 } from "./model175";
import { Model169Service, summarizeModel169 } from "./model169";
import type { Model169 } from "./model169";

export type Model176Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model176Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model176 extends Entity<"model176"> {
  updatedAt?: number;
  name: string;
  status: Model176Status;
  tags: string[];
  lines: Model176Line[];
  owner?: { name: string; email?: string };
  parent?: Model175;
  related: Model169[];
}

export type Model176Event =
  | { kind: "created"; item: Model176 }
  | { kind: "renamed"; id: Id<"model176">; from: string; to: string }
  | { kind: "moved"; id: Id<"model176">; status: Model176Status }
  | { kind: "deleted"; id: Id<"model176">; reason?: string };

export type Model176Events = {
  change: Model176Event;
  error: { message: string; code: number };
};

export type Model176Numbers = KeysOfType<Model176Line, number>;
export type FrozenModel176 = DeepReadonly<Model176>;

export function describeModel176Event(event: Model176Event): string {
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

export function totalModel176(item: Model176): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel176(item: Model176): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel176(item), 3), (item.parent ? summarizeModel175(item.parent) : "-"), item.related.map(summarizeModel169).join(",")].join(" | ");
}

export class Model176Service extends Service<Model176, "model176"> {
  readonly events = new EventBus<Model176Events>();
  private readonly parents?: Model175Service;

  constructor(repository = new MemoryRepository<Model176, "model176">()) {
    super(repository);
  }

  validate(item: Model176): string[] {
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

  rename(id: Id<"model176">, to: string): Result<Model176> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model176 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model176">, status: Model176Status): Result<Model176Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model176">, patch: Patch<Pick<Model176, "name" | "tags">>): Model176 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model176Status, Model176[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model176Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model176">[]): Promise<Model176[]> {
    const found: Model176[] = [];
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

  linkedService(): Model175Service {
    return this.parents ?? new Model175Service();
  }
}

export function makeModel176(id: string, name: string): Model176 {
  return {
    id: id as Id<"model176">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 177, unit: "m" }],
    related: [],
  };
}

export const model176Defaults: FrozenModel176 = makeModel176("default-176", "Default 176");
export const model176Label = summarizeModel176(makeModel176("label", "Label"));
