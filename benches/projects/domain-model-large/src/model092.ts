// Generated domain module 92 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model091Service, summarizeModel091 } from "./model091";
import type { Model091 } from "./model091";
import { Model085Service, summarizeModel085 } from "./model085";
import type { Model085 } from "./model085";

export type Model092Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model092Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model092 extends Entity<"model092"> {
  updatedAt?: number;
  name: string;
  status: Model092Status;
  tags: string[];
  lines: Model092Line[];
  owner?: { name: string; email?: string };
  parent?: Model091;
  related: Model085[];
}

export type Model092Event =
  | { kind: "created"; item: Model092 }
  | { kind: "renamed"; id: Id<"model092">; from: string; to: string }
  | { kind: "moved"; id: Id<"model092">; status: Model092Status }
  | { kind: "deleted"; id: Id<"model092">; reason?: string };

export type Model092Events = {
  change: Model092Event;
  error: { message: string; code: number };
};

export type Model092Numbers = KeysOfType<Model092Line, number>;
export type FrozenModel092 = DeepReadonly<Model092>;

export function describeModel092Event(event: Model092Event): string {
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

export function totalModel092(item: Model092): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel092(item: Model092): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel092(item), 3), (item.parent ? summarizeModel091(item.parent) : "-"), item.related.map(summarizeModel085).join(",")].join(" | ");
}

export class Model092Service extends Service<Model092, "model092"> {
  readonly events = new EventBus<Model092Events>();
  private readonly parents?: Model091Service;

  constructor(repository = new MemoryRepository<Model092, "model092">()) {
    super(repository);
  }

  validate(item: Model092): string[] {
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

  rename(id: Id<"model092">, to: string): Result<Model092> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model092 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model092">, status: Model092Status): Result<Model092Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model092">, patch: Patch<Pick<Model092, "name" | "tags">>): Model092 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model092Status, Model092[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model092Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model092">[]): Promise<Model092[]> {
    const found: Model092[] = [];
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

  linkedService(): Model091Service {
    return this.parents ?? new Model091Service();
  }
}

export function makeModel092(id: string, name: string): Model092 {
  return {
    id: id as Id<"model092">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 93, unit: "s" }],
    related: [],
  };
}

export const model092Defaults: FrozenModel092 = makeModel092("default-92", "Default 92");
export const model092Label = summarizeModel092(makeModel092("label", "Label"));
