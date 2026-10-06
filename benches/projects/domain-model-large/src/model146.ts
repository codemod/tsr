// Generated domain module 146 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model145Service, summarizeModel145 } from "./model145";
import type { Model145 } from "./model145";
import { Model139Service, summarizeModel139 } from "./model139";
import type { Model139 } from "./model139";

export type Model146Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model146Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model146 extends Entity<"model146"> {
  updatedAt?: number;
  name: string;
  status: Model146Status;
  tags: string[];
  lines: Model146Line[];
  owner?: { name: string; email?: string };
  parent?: Model145;
  related: Model139[];
}

export type Model146Event =
  | { kind: "created"; item: Model146 }
  | { kind: "renamed"; id: Id<"model146">; from: string; to: string }
  | { kind: "moved"; id: Id<"model146">; status: Model146Status }
  | { kind: "deleted"; id: Id<"model146">; reason?: string };

export type Model146Events = {
  change: Model146Event;
  error: { message: string; code: number };
};

export type Model146Numbers = KeysOfType<Model146Line, number>;
export type FrozenModel146 = DeepReadonly<Model146>;

export function describeModel146Event(event: Model146Event): string {
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

export function totalModel146(item: Model146): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel146(item: Model146): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel146(item), 3), (item.parent ? summarizeModel145(item.parent) : "-"), item.related.map(summarizeModel139).join(",")].join(" | ");
}

export class Model146Service extends Service<Model146, "model146"> {
  readonly events = new EventBus<Model146Events>();
  private readonly parents?: Model145Service;

  constructor(repository = new MemoryRepository<Model146, "model146">()) {
    super(repository);
  }

  validate(item: Model146): string[] {
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

  rename(id: Id<"model146">, to: string): Result<Model146> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model146 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model146">, status: Model146Status): Result<Model146Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model146">, patch: Patch<Pick<Model146, "name" | "tags">>): Model146 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model146Status, Model146[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model146Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model146">[]): Promise<Model146[]> {
    const found: Model146[] = [];
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

  linkedService(): Model145Service {
    return this.parents ?? new Model145Service();
  }
}

export function makeModel146(id: string, name: string): Model146 {
  return {
    id: id as Id<"model146">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 147, unit: "m" }],
    related: [],
  };
}

export const model146Defaults: FrozenModel146 = makeModel146("default-146", "Default 146");
export const model146Label = summarizeModel146(makeModel146("label", "Label"));
