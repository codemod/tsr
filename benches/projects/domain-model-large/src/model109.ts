// Generated domain module 109 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model108Service, summarizeModel108 } from "./model108";
import type { Model108 } from "./model108";
import { Model102Service, summarizeModel102 } from "./model102";
import type { Model102 } from "./model102";

export type Model109Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model109Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model109 extends Entity<"model109"> {
  updatedAt?: number;
  name: string;
  status: Model109Status;
  tags: string[];
  lines: Model109Line[];
  owner?: { name: string; email?: string };
  parent?: Model108;
  related: Model102[];
}

export type Model109Event =
  | { kind: "created"; item: Model109 }
  | { kind: "renamed"; id: Id<"model109">; from: string; to: string }
  | { kind: "moved"; id: Id<"model109">; status: Model109Status }
  | { kind: "deleted"; id: Id<"model109">; reason?: string };

export type Model109Events = {
  change: Model109Event;
  error: { message: string; code: number };
};

export type Model109Numbers = KeysOfType<Model109Line, number>;
export type FrozenModel109 = DeepReadonly<Model109>;

export function describeModel109Event(event: Model109Event): string {
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

export function totalModel109(item: Model109): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel109(item: Model109): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel109(item), 3), (item.parent ? summarizeModel108(item.parent) : "-"), item.related.map(summarizeModel102).join(",")].join(" | ");
}

export class Model109Service extends Service<Model109, "model109"> {
  readonly events = new EventBus<Model109Events>();
  private readonly parents?: Model108Service;

  constructor(repository = new MemoryRepository<Model109, "model109">()) {
    super(repository);
  }

  validate(item: Model109): string[] {
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

  rename(id: Id<"model109">, to: string): Result<Model109> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model109 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model109">, status: Model109Status): Result<Model109Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model109">, patch: Patch<Pick<Model109, "name" | "tags">>): Model109 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model109Status, Model109[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model109Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model109">[]): Promise<Model109[]> {
    const found: Model109[] = [];
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

  linkedService(): Model108Service {
    return this.parents ?? new Model108Service();
  }
}

export function makeModel109(id: string, name: string): Model109 {
  return {
    id: id as Id<"model109">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 110, unit: "hour" }],
    related: [],
  };
}

export const model109Defaults: FrozenModel109 = makeModel109("default-109", "Default 109");
export const model109Label = summarizeModel109(makeModel109("label", "Label"));
