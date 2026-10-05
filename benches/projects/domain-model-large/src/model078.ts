// Generated domain module 78 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model077Service, summarizeModel077 } from "./model077";
import type { Model077 } from "./model077";
import { Model071Service, summarizeModel071 } from "./model071";
import type { Model071 } from "./model071";

export type Model078Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model078Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model078 extends Entity<"model078"> {
  updatedAt?: number;
  name: string;
  status: Model078Status;
  tags: string[];
  lines: Model078Line[];
  owner?: { name: string; email?: string };
  parent?: Model077;
  related: Model071[];
}

export type Model078Event =
  | { kind: "created"; item: Model078 }
  | { kind: "renamed"; id: Id<"model078">; from: string; to: string }
  | { kind: "moved"; id: Id<"model078">; status: Model078Status }
  | { kind: "deleted"; id: Id<"model078">; reason?: string };

export type Model078Events = {
  change: Model078Event;
  error: { message: string; code: number };
};

export type Model078Numbers = KeysOfType<Model078Line, number>;
export type FrozenModel078 = DeepReadonly<Model078>;

export function describeModel078Event(event: Model078Event): string {
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

export function totalModel078(item: Model078): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel078(item: Model078): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel078(item), 3), (item.parent ? summarizeModel077(item.parent) : "-"), item.related.map(summarizeModel071).join(",")].join(" | ");
}

export class Model078Service extends Service<Model078, "model078"> {
  readonly events = new EventBus<Model078Events>();
  private readonly parents?: Model077Service;

  constructor(repository = new MemoryRepository<Model078, "model078">()) {
    super(repository);
  }

  validate(item: Model078): string[] {
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

  rename(id: Id<"model078">, to: string): Result<Model078> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model078 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model078">, status: Model078Status): Result<Model078Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model078">, patch: Patch<Pick<Model078, "name" | "tags">>): Model078 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model078Status, Model078[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model078Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model078">[]): Promise<Model078[]> {
    const found: Model078[] = [];
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

  linkedService(): Model077Service {
    return this.parents ?? new Model077Service();
  }
}

export function makeModel078(id: string, name: string): Model078 {
  return {
    id: id as Id<"model078">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 79, unit: "item" }],
    related: [],
  };
}

export const model078Defaults: FrozenModel078 = makeModel078("default-78", "Default 78");
export const model078Label = summarizeModel078(makeModel078("label", "Label"));
