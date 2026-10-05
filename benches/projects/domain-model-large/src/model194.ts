// Generated domain module 194 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model193Service, summarizeModel193 } from "./model193";
import type { Model193 } from "./model193";
import { Model187Service, summarizeModel187 } from "./model187";
import type { Model187 } from "./model187";

export type Model194Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model194Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model194 extends Entity<"model194"> {
  updatedAt?: number;
  name: string;
  status: Model194Status;
  tags: string[];
  lines: Model194Line[];
  owner?: { name: string; email?: string };
  parent?: Model193;
  related: Model187[];
}

export type Model194Event =
  | { kind: "created"; item: Model194 }
  | { kind: "renamed"; id: Id<"model194">; from: string; to: string }
  | { kind: "moved"; id: Id<"model194">; status: Model194Status }
  | { kind: "deleted"; id: Id<"model194">; reason?: string };

export type Model194Events = {
  change: Model194Event;
  error: { message: string; code: number };
};

export type Model194Numbers = KeysOfType<Model194Line, number>;
export type FrozenModel194 = DeepReadonly<Model194>;

export function describeModel194Event(event: Model194Event): string {
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

export function totalModel194(item: Model194): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel194(item: Model194): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel194(item), 3), (item.parent ? summarizeModel193(item.parent) : "-"), item.related.map(summarizeModel187).join(",")].join(" | ");
}

export class Model194Service extends Service<Model194, "model194"> {
  readonly events = new EventBus<Model194Events>();
  private readonly parents?: Model193Service;

  constructor(repository = new MemoryRepository<Model194, "model194">()) {
    super(repository);
  }

  validate(item: Model194): string[] {
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

  rename(id: Id<"model194">, to: string): Result<Model194> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model194 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model194">, status: Model194Status): Result<Model194Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model194">, patch: Patch<Pick<Model194, "name" | "tags">>): Model194 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model194Status, Model194[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model194Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model194">[]): Promise<Model194[]> {
    const found: Model194[] = [];
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

  linkedService(): Model193Service {
    return this.parents ?? new Model193Service();
  }
}

export function makeModel194(id: string, name: string): Model194 {
  return {
    id: id as Id<"model194">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 195, unit: "hour" }],
    related: [],
  };
}

export const model194Defaults: FrozenModel194 = makeModel194("default-194", "Default 194");
export const model194Label = summarizeModel194(makeModel194("label", "Label"));
