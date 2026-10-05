// Generated domain module 94 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model093Service, summarizeModel093 } from "./model093";
import type { Model093 } from "./model093";
import { Model087Service, summarizeModel087 } from "./model087";
import type { Model087 } from "./model087";

export type Model094Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model094Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model094 extends Entity<"model094"> {
  updatedAt?: number;
  name: string;
  status: Model094Status;
  tags: string[];
  lines: Model094Line[];
  owner?: { name: string; email?: string };
  parent?: Model093;
  related: Model087[];
}

export type Model094Event =
  | { kind: "created"; item: Model094 }
  | { kind: "renamed"; id: Id<"model094">; from: string; to: string }
  | { kind: "moved"; id: Id<"model094">; status: Model094Status }
  | { kind: "deleted"; id: Id<"model094">; reason?: string };

export type Model094Events = {
  change: Model094Event;
  error: { message: string; code: number };
};

export type Model094Numbers = KeysOfType<Model094Line, number>;
export type FrozenModel094 = DeepReadonly<Model094>;

export function describeModel094Event(event: Model094Event): string {
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

export function totalModel094(item: Model094): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel094(item: Model094): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel094(item), 3), (item.parent ? summarizeModel093(item.parent) : "-"), item.related.map(summarizeModel087).join(",")].join(" | ");
}

export class Model094Service extends Service<Model094, "model094"> {
  readonly events = new EventBus<Model094Events>();
  private readonly parents?: Model093Service;

  constructor(repository = new MemoryRepository<Model094, "model094">()) {
    super(repository);
  }

  validate(item: Model094): string[] {
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

  rename(id: Id<"model094">, to: string): Result<Model094> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model094 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model094">, status: Model094Status): Result<Model094Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model094">, patch: Patch<Pick<Model094, "name" | "tags">>): Model094 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model094Status, Model094[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model094Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model094">[]): Promise<Model094[]> {
    const found: Model094[] = [];
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

  linkedService(): Model093Service {
    return this.parents ?? new Model093Service();
  }
}

export function makeModel094(id: string, name: string): Model094 {
  return {
    id: id as Id<"model094">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 95, unit: "hour" }],
    related: [],
  };
}

export const model094Defaults: FrozenModel094 = makeModel094("default-94", "Default 94");
export const model094Label = summarizeModel094(makeModel094("label", "Label"));
