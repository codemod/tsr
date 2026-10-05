// Generated domain module 158 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model157Service, summarizeModel157 } from "./model157";
import type { Model157 } from "./model157";
import { Model151Service, summarizeModel151 } from "./model151";
import type { Model151 } from "./model151";

export type Model158Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model158Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model158 extends Entity<"model158"> {
  updatedAt?: number;
  name: string;
  status: Model158Status;
  tags: string[];
  lines: Model158Line[];
  owner?: { name: string; email?: string };
  parent?: Model157;
  related: Model151[];
}

export type Model158Event =
  | { kind: "created"; item: Model158 }
  | { kind: "renamed"; id: Id<"model158">; from: string; to: string }
  | { kind: "moved"; id: Id<"model158">; status: Model158Status }
  | { kind: "deleted"; id: Id<"model158">; reason?: string };

export type Model158Events = {
  change: Model158Event;
  error: { message: string; code: number };
};

export type Model158Numbers = KeysOfType<Model158Line, number>;
export type FrozenModel158 = DeepReadonly<Model158>;

export function describeModel158Event(event: Model158Event): string {
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

export function totalModel158(item: Model158): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel158(item: Model158): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel158(item), 3), (item.parent ? summarizeModel157(item.parent) : "-"), item.related.map(summarizeModel151).join(",")].join(" | ");
}

export class Model158Service extends Service<Model158, "model158"> {
  readonly events = new EventBus<Model158Events>();
  private readonly parents?: Model157Service;

  constructor(repository = new MemoryRepository<Model158, "model158">()) {
    super(repository);
  }

  validate(item: Model158): string[] {
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

  rename(id: Id<"model158">, to: string): Result<Model158> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model158 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model158">, status: Model158Status): Result<Model158Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model158">, patch: Patch<Pick<Model158, "name" | "tags">>): Model158 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model158Status, Model158[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model158Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model158">[]): Promise<Model158[]> {
    const found: Model158[] = [];
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

  linkedService(): Model157Service {
    return this.parents ?? new Model157Service();
  }
}

export function makeModel158(id: string, name: string): Model158 {
  return {
    id: id as Id<"model158">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 159, unit: "item" }],
    related: [],
  };
}

export const model158Defaults: FrozenModel158 = makeModel158("default-158", "Default 158");
export const model158Label = summarizeModel158(makeModel158("label", "Label"));
