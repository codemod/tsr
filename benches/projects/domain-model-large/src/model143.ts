// Generated domain module 143 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model142Service, summarizeModel142 } from "./model142";
import type { Model142 } from "./model142";
import { Model136Service, summarizeModel136 } from "./model136";
import type { Model136 } from "./model136";

export type Model143Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model143Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model143 extends Entity<"model143"> {
  updatedAt?: number;
  name: string;
  status: Model143Status;
  tags: string[];
  lines: Model143Line[];
  owner?: { name: string; email?: string };
  parent?: Model142;
  related: Model136[];
}

export type Model143Event =
  | { kind: "created"; item: Model143 }
  | { kind: "renamed"; id: Id<"model143">; from: string; to: string }
  | { kind: "moved"; id: Id<"model143">; status: Model143Status }
  | { kind: "deleted"; id: Id<"model143">; reason?: string };

export type Model143Events = {
  change: Model143Event;
  error: { message: string; code: number };
};

export type Model143Numbers = KeysOfType<Model143Line, number>;
export type FrozenModel143 = DeepReadonly<Model143>;

export function describeModel143Event(event: Model143Event): string {
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

export function totalModel143(item: Model143): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel143(item: Model143): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel143(item), 3), (item.parent ? summarizeModel142(item.parent) : "-"), item.related.map(summarizeModel136).join(",")].join(" | ");
}

export class Model143Service extends Service<Model143, "model143"> {
  readonly events = new EventBus<Model143Events>();
  private readonly parents?: Model142Service;

  constructor(repository = new MemoryRepository<Model143, "model143">()) {
    super(repository);
  }

  validate(item: Model143): string[] {
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

  rename(id: Id<"model143">, to: string): Result<Model143> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model143 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model143">, status: Model143Status): Result<Model143Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model143">, patch: Patch<Pick<Model143, "name" | "tags">>): Model143 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model143Status, Model143[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model143Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model143">[]): Promise<Model143[]> {
    const found: Model143[] = [];
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

  linkedService(): Model142Service {
    return this.parents ?? new Model142Service();
  }
}

export function makeModel143(id: string, name: string): Model143 {
  return {
    id: id as Id<"model143">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 144, unit: "item" }],
    related: [],
  };
}

export const model143Defaults: FrozenModel143 = makeModel143("default-143", "Default 143");
export const model143Label = summarizeModel143(makeModel143("label", "Label"));
