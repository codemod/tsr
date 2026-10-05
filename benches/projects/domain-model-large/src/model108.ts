// Generated domain module 108 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model107Service, summarizeModel107 } from "./model107";
import type { Model107 } from "./model107";
import { Model101Service, summarizeModel101 } from "./model101";
import type { Model101 } from "./model101";

export type Model108Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model108Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model108 extends Entity<"model108"> {
  updatedAt?: number;
  name: string;
  status: Model108Status;
  tags: string[];
  lines: Model108Line[];
  owner?: { name: string; email?: string };
  parent?: Model107;
  related: Model101[];
}

export type Model108Event =
  | { kind: "created"; item: Model108 }
  | { kind: "renamed"; id: Id<"model108">; from: string; to: string }
  | { kind: "moved"; id: Id<"model108">; status: Model108Status }
  | { kind: "deleted"; id: Id<"model108">; reason?: string };

export type Model108Events = {
  change: Model108Event;
  error: { message: string; code: number };
};

export type Model108Numbers = KeysOfType<Model108Line, number>;
export type FrozenModel108 = DeepReadonly<Model108>;

export function describeModel108Event(event: Model108Event): string {
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

export function totalModel108(item: Model108): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel108(item: Model108): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel108(item), 3), (item.parent ? summarizeModel107(item.parent) : "-"), item.related.map(summarizeModel101).join(",")].join(" | ");
}

export class Model108Service extends Service<Model108, "model108"> {
  readonly events = new EventBus<Model108Events>();
  private readonly parents?: Model107Service;

  constructor(repository = new MemoryRepository<Model108, "model108">()) {
    super(repository);
  }

  validate(item: Model108): string[] {
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

  rename(id: Id<"model108">, to: string): Result<Model108> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model108 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model108">, status: Model108Status): Result<Model108Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model108">, patch: Patch<Pick<Model108, "name" | "tags">>): Model108 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model108Status, Model108[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model108Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model108">[]): Promise<Model108[]> {
    const found: Model108[] = [];
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

  linkedService(): Model107Service {
    return this.parents ?? new Model107Service();
  }
}

export function makeModel108(id: string, name: string): Model108 {
  return {
    id: id as Id<"model108">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 109, unit: "item" }],
    related: [],
  };
}

export const model108Defaults: FrozenModel108 = makeModel108("default-108", "Default 108");
export const model108Label = summarizeModel108(makeModel108("label", "Label"));
