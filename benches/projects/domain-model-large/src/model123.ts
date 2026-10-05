// Generated domain module 123 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model122Service, summarizeModel122 } from "./model122";
import type { Model122 } from "./model122";
import { Model116Service, summarizeModel116 } from "./model116";
import type { Model116 } from "./model116";

export type Model123Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model123Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model123 extends Entity<"model123"> {
  updatedAt?: number;
  name: string;
  status: Model123Status;
  tags: string[];
  lines: Model123Line[];
  owner?: { name: string; email?: string };
  parent?: Model122;
  related: Model116[];
}

export type Model123Event =
  | { kind: "created"; item: Model123 }
  | { kind: "renamed"; id: Id<"model123">; from: string; to: string }
  | { kind: "moved"; id: Id<"model123">; status: Model123Status }
  | { kind: "deleted"; id: Id<"model123">; reason?: string };

export type Model123Events = {
  change: Model123Event;
  error: { message: string; code: number };
};

export type Model123Numbers = KeysOfType<Model123Line, number>;
export type FrozenModel123 = DeepReadonly<Model123>;

export function describeModel123Event(event: Model123Event): string {
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

export function totalModel123(item: Model123): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel123(item: Model123): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel123(item), 3), (item.parent ? summarizeModel122(item.parent) : "-"), item.related.map(summarizeModel116).join(",")].join(" | ");
}

export class Model123Service extends Service<Model123, "model123"> {
  readonly events = new EventBus<Model123Events>();
  private readonly parents?: Model122Service;

  constructor(repository = new MemoryRepository<Model123, "model123">()) {
    super(repository);
  }

  validate(item: Model123): string[] {
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

  rename(id: Id<"model123">, to: string): Result<Model123> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model123 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model123">, status: Model123Status): Result<Model123Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model123">, patch: Patch<Pick<Model123, "name" | "tags">>): Model123 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model123Status, Model123[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model123Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model123">[]): Promise<Model123[]> {
    const found: Model123[] = [];
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

  linkedService(): Model122Service {
    return this.parents ?? new Model122Service();
  }
}

export function makeModel123(id: string, name: string): Model123 {
  return {
    id: id as Id<"model123">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 124, unit: "item" }],
    related: [],
  };
}

export const model123Defaults: FrozenModel123 = makeModel123("default-123", "Default 123");
export const model123Label = summarizeModel123(makeModel123("label", "Label"));
