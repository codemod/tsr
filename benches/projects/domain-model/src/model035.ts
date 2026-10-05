// Generated domain module 35 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model034Service, summarizeModel034 } from "./model034";
import type { Model034 } from "./model034";
import { Model028Service, summarizeModel028 } from "./model028";
import type { Model028 } from "./model028";

export type Model035Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model035Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model035 extends Entity<"model035"> {
  updatedAt?: number;
  name: string;
  status: Model035Status;
  tags: string[];
  lines: Model035Line[];
  owner?: { name: string; email?: string };
  parent?: Model034;
  related: Model028[];
}

export type Model035Event =
  | { kind: "created"; item: Model035 }
  | { kind: "renamed"; id: Id<"model035">; from: string; to: string }
  | { kind: "moved"; id: Id<"model035">; status: Model035Status }
  | { kind: "deleted"; id: Id<"model035">; reason?: string };

export type Model035Events = {
  change: Model035Event;
  error: { message: string; code: number };
};

export type Model035Numbers = KeysOfType<Model035Line, number>;
export type FrozenModel035 = DeepReadonly<Model035>;

export function describeModel035Event(event: Model035Event): string {
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

export function totalModel035(item: Model035): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel035(item: Model035): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel035(item), 3), (item.parent ? summarizeModel034(item.parent) : "-"), item.related.map(summarizeModel028).join(",")].join(" | ");
}

export class Model035Service extends Service<Model035, "model035"> {
  readonly events = new EventBus<Model035Events>();
  private readonly parents?: Model034Service;

  constructor(repository = new MemoryRepository<Model035, "model035">()) {
    super(repository);
  }

  validate(item: Model035): string[] {
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

  rename(id: Id<"model035">, to: string): Result<Model035> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model035 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model035">, status: Model035Status): Result<Model035Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model035">, patch: Patch<Pick<Model035, "name" | "tags">>): Model035 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model035Status, Model035[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model035Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model035">[]): Promise<Model035[]> {
    const found: Model035[] = [];
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

  linkedService(): Model034Service {
    return this.parents ?? new Model034Service();
  }
}

export function makeModel035(id: string, name: string): Model035 {
  return {
    id: id as Id<"model035">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 36, unit: "kg" }],
    related: [],
  };
}

export const model035Defaults: FrozenModel035 = makeModel035("default-35", "Default 35");
export const model035Label = summarizeModel035(makeModel035("label", "Label"));
