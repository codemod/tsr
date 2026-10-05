// Generated domain module 179 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model178Service, summarizeModel178 } from "./model178";
import type { Model178 } from "./model178";
import { Model172Service, summarizeModel172 } from "./model172";
import type { Model172 } from "./model172";

export type Model179Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model179Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model179 extends Entity<"model179"> {
  updatedAt?: number;
  name: string;
  status: Model179Status;
  tags: string[];
  lines: Model179Line[];
  owner?: { name: string; email?: string };
  parent?: Model178;
  related: Model172[];
}

export type Model179Event =
  | { kind: "created"; item: Model179 }
  | { kind: "renamed"; id: Id<"model179">; from: string; to: string }
  | { kind: "moved"; id: Id<"model179">; status: Model179Status }
  | { kind: "deleted"; id: Id<"model179">; reason?: string };

export type Model179Events = {
  change: Model179Event;
  error: { message: string; code: number };
};

export type Model179Numbers = KeysOfType<Model179Line, number>;
export type FrozenModel179 = DeepReadonly<Model179>;

export function describeModel179Event(event: Model179Event): string {
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

export function totalModel179(item: Model179): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel179(item: Model179): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel179(item), 3), (item.parent ? summarizeModel178(item.parent) : "-"), item.related.map(summarizeModel172).join(",")].join(" | ");
}

export class Model179Service extends Service<Model179, "model179"> {
  readonly events = new EventBus<Model179Events>();
  private readonly parents?: Model178Service;

  constructor(repository = new MemoryRepository<Model179, "model179">()) {
    super(repository);
  }

  validate(item: Model179): string[] {
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

  rename(id: Id<"model179">, to: string): Result<Model179> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model179 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model179">, status: Model179Status): Result<Model179Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model179">, patch: Patch<Pick<Model179, "name" | "tags">>): Model179 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model179Status, Model179[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model179Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model179">[]): Promise<Model179[]> {
    const found: Model179[] = [];
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

  linkedService(): Model178Service {
    return this.parents ?? new Model178Service();
  }
}

export function makeModel179(id: string, name: string): Model179 {
  return {
    id: id as Id<"model179">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 180, unit: "hour" }],
    related: [],
  };
}

export const model179Defaults: FrozenModel179 = makeModel179("default-179", "Default 179");
export const model179Label = summarizeModel179(makeModel179("label", "Label"));
