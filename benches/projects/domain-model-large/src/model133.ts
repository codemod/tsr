// Generated domain module 133 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model132Service, summarizeModel132 } from "./model132";
import type { Model132 } from "./model132";
import { Model126Service, summarizeModel126 } from "./model126";
import type { Model126 } from "./model126";

export type Model133Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model133Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model133 extends Entity<"model133"> {
  updatedAt?: number;
  name: string;
  status: Model133Status;
  tags: string[];
  lines: Model133Line[];
  owner?: { name: string; email?: string };
  parent?: Model132;
  related: Model126[];
}

export type Model133Event =
  | { kind: "created"; item: Model133 }
  | { kind: "renamed"; id: Id<"model133">; from: string; to: string }
  | { kind: "moved"; id: Id<"model133">; status: Model133Status }
  | { kind: "deleted"; id: Id<"model133">; reason?: string };

export type Model133Events = {
  change: Model133Event;
  error: { message: string; code: number };
};

export type Model133Numbers = KeysOfType<Model133Line, number>;
export type FrozenModel133 = DeepReadonly<Model133>;

export function describeModel133Event(event: Model133Event): string {
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

export function totalModel133(item: Model133): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel133(item: Model133): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel133(item), 3), (item.parent ? summarizeModel132(item.parent) : "-"), item.related.map(summarizeModel126).join(",")].join(" | ");
}

export class Model133Service extends Service<Model133, "model133"> {
  readonly events = new EventBus<Model133Events>();
  private readonly parents?: Model132Service;

  constructor(repository = new MemoryRepository<Model133, "model133">()) {
    super(repository);
  }

  validate(item: Model133): string[] {
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

  rename(id: Id<"model133">, to: string): Result<Model133> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model133 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model133">, status: Model133Status): Result<Model133Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model133">, patch: Patch<Pick<Model133, "name" | "tags">>): Model133 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model133Status, Model133[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model133Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model133">[]): Promise<Model133[]> {
    const found: Model133[] = [];
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

  linkedService(): Model132Service {
    return this.parents ?? new Model132Service();
  }
}

export function makeModel133(id: string, name: string): Model133 {
  return {
    id: id as Id<"model133">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 134, unit: "item" }],
    related: [],
  };
}

export const model133Defaults: FrozenModel133 = makeModel133("default-133", "Default 133");
export const model133Label = summarizeModel133(makeModel133("label", "Label"));
