// Generated domain module 192 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model191Service, summarizeModel191 } from "./model191";
import type { Model191 } from "./model191";
import { Model185Service, summarizeModel185 } from "./model185";
import type { Model185 } from "./model185";

export type Model192Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model192Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model192 extends Entity<"model192"> {
  updatedAt?: number;
  name: string;
  status: Model192Status;
  tags: string[];
  lines: Model192Line[];
  owner?: { name: string; email?: string };
  parent?: Model191;
  related: Model185[];
}

export type Model192Event =
  | { kind: "created"; item: Model192 }
  | { kind: "renamed"; id: Id<"model192">; from: string; to: string }
  | { kind: "moved"; id: Id<"model192">; status: Model192Status }
  | { kind: "deleted"; id: Id<"model192">; reason?: string };

export type Model192Events = {
  change: Model192Event;
  error: { message: string; code: number };
};

export type Model192Numbers = KeysOfType<Model192Line, number>;
export type FrozenModel192 = DeepReadonly<Model192>;

export function describeModel192Event(event: Model192Event): string {
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

export function totalModel192(item: Model192): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel192(item: Model192): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel192(item), 3), (item.parent ? summarizeModel191(item.parent) : "-"), item.related.map(summarizeModel185).join(",")].join(" | ");
}

export class Model192Service extends Service<Model192, "model192"> {
  readonly events = new EventBus<Model192Events>();
  private readonly parents?: Model191Service;

  constructor(repository = new MemoryRepository<Model192, "model192">()) {
    super(repository);
  }

  validate(item: Model192): string[] {
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

  rename(id: Id<"model192">, to: string): Result<Model192> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model192 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model192">, status: Model192Status): Result<Model192Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model192">, patch: Patch<Pick<Model192, "name" | "tags">>): Model192 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model192Status, Model192[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model192Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model192">[]): Promise<Model192[]> {
    const found: Model192[] = [];
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

  linkedService(): Model191Service {
    return this.parents ?? new Model191Service();
  }
}

export function makeModel192(id: string, name: string): Model192 {
  return {
    id: id as Id<"model192">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 193, unit: "s" }],
    related: [],
  };
}

export const model192Defaults: FrozenModel192 = makeModel192("default-192", "Default 192");
export const model192Label = summarizeModel192(makeModel192("label", "Label"));
