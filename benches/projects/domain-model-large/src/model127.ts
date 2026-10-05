// Generated domain module 127 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model126Service, summarizeModel126 } from "./model126";
import type { Model126 } from "./model126";
import { Model120Service, summarizeModel120 } from "./model120";
import type { Model120 } from "./model120";

export type Model127Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model127Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model127 extends Entity<"model127"> {
  updatedAt?: number;
  name: string;
  status: Model127Status;
  tags: string[];
  lines: Model127Line[];
  owner?: { name: string; email?: string };
  parent?: Model126;
  related: Model120[];
}

export type Model127Event =
  | { kind: "created"; item: Model127 }
  | { kind: "renamed"; id: Id<"model127">; from: string; to: string }
  | { kind: "moved"; id: Id<"model127">; status: Model127Status }
  | { kind: "deleted"; id: Id<"model127">; reason?: string };

export type Model127Events = {
  change: Model127Event;
  error: { message: string; code: number };
};

export type Model127Numbers = KeysOfType<Model127Line, number>;
export type FrozenModel127 = DeepReadonly<Model127>;

export function describeModel127Event(event: Model127Event): string {
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

export function totalModel127(item: Model127): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel127(item: Model127): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel127(item), 3), (item.parent ? summarizeModel126(item.parent) : "-"), item.related.map(summarizeModel120).join(",")].join(" | ");
}

export class Model127Service extends Service<Model127, "model127"> {
  readonly events = new EventBus<Model127Events>();
  private readonly parents?: Model126Service;

  constructor(repository = new MemoryRepository<Model127, "model127">()) {
    super(repository);
  }

  validate(item: Model127): string[] {
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

  rename(id: Id<"model127">, to: string): Result<Model127> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model127 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model127">, status: Model127Status): Result<Model127Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model127">, patch: Patch<Pick<Model127, "name" | "tags">>): Model127 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model127Status, Model127[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model127Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model127">[]): Promise<Model127[]> {
    const found: Model127[] = [];
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

  linkedService(): Model126Service {
    return this.parents ?? new Model126Service();
  }
}

export function makeModel127(id: string, name: string): Model127 {
  return {
    id: id as Id<"model127">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 128, unit: "s" }],
    related: [],
  };
}

export const model127Defaults: FrozenModel127 = makeModel127("default-127", "Default 127");
export const model127Label = summarizeModel127(makeModel127("label", "Label"));
