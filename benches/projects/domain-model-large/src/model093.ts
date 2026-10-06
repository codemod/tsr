// Generated domain module 93 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model092Service, summarizeModel092 } from "./model092";
import type { Model092 } from "./model092";
import { Model086Service, summarizeModel086 } from "./model086";
import type { Model086 } from "./model086";

export type Model093Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model093Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model093 extends Entity<"model093"> {
  updatedAt?: number;
  name: string;
  status: Model093Status;
  tags: string[];
  lines: Model093Line[];
  owner?: { name: string; email?: string };
  parent?: Model092;
  related: Model086[];
}

export type Model093Event =
  | { kind: "created"; item: Model093 }
  | { kind: "renamed"; id: Id<"model093">; from: string; to: string }
  | { kind: "moved"; id: Id<"model093">; status: Model093Status }
  | { kind: "deleted"; id: Id<"model093">; reason?: string };

export type Model093Events = {
  change: Model093Event;
  error: { message: string; code: number };
};

export type Model093Numbers = KeysOfType<Model093Line, number>;
export type FrozenModel093 = DeepReadonly<Model093>;

export function describeModel093Event(event: Model093Event): string {
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

export function totalModel093(item: Model093): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel093(item: Model093): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel093(item), 3), (item.parent ? summarizeModel092(item.parent) : "-"), item.related.map(summarizeModel086).join(",")].join(" | ");
}

export class Model093Service extends Service<Model093, "model093"> {
  readonly events = new EventBus<Model093Events>();
  private readonly parents?: Model092Service;

  constructor(repository = new MemoryRepository<Model093, "model093">()) {
    super(repository);
  }

  validate(item: Model093): string[] {
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

  rename(id: Id<"model093">, to: string): Result<Model093> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model093 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model093">, status: Model093Status): Result<Model093Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model093">, patch: Patch<Pick<Model093, "name" | "tags">>): Model093 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model093Status, Model093[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model093Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model093">[]): Promise<Model093[]> {
    const found: Model093[] = [];
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

  linkedService(): Model092Service {
    return this.parents ?? new Model092Service();
  }
}

export function makeModel093(id: string, name: string): Model093 {
  return {
    id: id as Id<"model093">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 94, unit: "item" }],
    related: [],
  };
}

export const model093Defaults: FrozenModel093 = makeModel093("default-93", "Default 93");
export const model093Label = summarizeModel093(makeModel093("label", "Label"));
