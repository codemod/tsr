// Generated domain module 10 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model009Service, summarizeModel009 } from "./model009";
import type { Model009 } from "./model009";
import { Model003Service, summarizeModel003 } from "./model003";
import type { Model003 } from "./model003";

export type Model010Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model010Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model010 extends Entity<"model010"> {
  updatedAt?: number;
  name: string;
  status: Model010Status;
  tags: string[];
  lines: Model010Line[];
  owner?: { name: string; email?: string };
  parent?: Model009;
  related: Model003[];
}

export type Model010Event =
  | { kind: "created"; item: Model010 }
  | { kind: "renamed"; id: Id<"model010">; from: string; to: string }
  | { kind: "moved"; id: Id<"model010">; status: Model010Status }
  | { kind: "deleted"; id: Id<"model010">; reason?: string };

export type Model010Events = {
  change: Model010Event;
  error: { message: string; code: number };
};

export type Model010Numbers = KeysOfType<Model010Line, number>;
export type FrozenModel010 = DeepReadonly<Model010>;

export function describeModel010Event(event: Model010Event): string {
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

export function totalModel010(item: Model010): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel010(item: Model010): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel010(item), 3), (item.parent ? summarizeModel009(item.parent) : "-"), item.related.map(summarizeModel003).join(",")].join(" | ");
}

export class Model010Service extends Service<Model010, "model010"> {
  readonly events = new EventBus<Model010Events>();
  private readonly parents?: Model009Service;

  constructor(repository = new MemoryRepository<Model010, "model010">()) {
    super(repository);
  }

  validate(item: Model010): string[] {
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

  rename(id: Id<"model010">, to: string): Result<Model010> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model010 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model010">, status: Model010Status): Result<Model010Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model010">, patch: Patch<Pick<Model010, "name" | "tags">>): Model010 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model010Status, Model010[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model010Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model010">[]): Promise<Model010[]> {
    const found: Model010[] = [];
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

  linkedService(): Model009Service {
    return this.parents ?? new Model009Service();
  }
}

export function makeModel010(id: string, name: string): Model010 {
  return {
    id: id as Id<"model010">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 11, unit: "kg" }],
    related: [],
  };
}

export const model010Defaults: FrozenModel010 = makeModel010("default-10", "Default 10");
export const model010Label = summarizeModel010(makeModel010("label", "Label"));
