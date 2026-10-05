// Generated domain module 113 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model112Service, summarizeModel112 } from "./model112";
import type { Model112 } from "./model112";
import { Model106Service, summarizeModel106 } from "./model106";
import type { Model106 } from "./model106";

export type Model113Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model113Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model113 extends Entity<"model113"> {
  updatedAt?: number;
  name: string;
  status: Model113Status;
  tags: string[];
  lines: Model113Line[];
  owner?: { name: string; email?: string };
  parent?: Model112;
  related: Model106[];
}

export type Model113Event =
  | { kind: "created"; item: Model113 }
  | { kind: "renamed"; id: Id<"model113">; from: string; to: string }
  | { kind: "moved"; id: Id<"model113">; status: Model113Status }
  | { kind: "deleted"; id: Id<"model113">; reason?: string };

export type Model113Events = {
  change: Model113Event;
  error: { message: string; code: number };
};

export type Model113Numbers = KeysOfType<Model113Line, number>;
export type FrozenModel113 = DeepReadonly<Model113>;

export function describeModel113Event(event: Model113Event): string {
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

export function totalModel113(item: Model113): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel113(item: Model113): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel113(item), 3), (item.parent ? summarizeModel112(item.parent) : "-"), item.related.map(summarizeModel106).join(",")].join(" | ");
}

export class Model113Service extends Service<Model113, "model113"> {
  readonly events = new EventBus<Model113Events>();
  private readonly parents?: Model112Service;

  constructor(repository = new MemoryRepository<Model113, "model113">()) {
    super(repository);
  }

  validate(item: Model113): string[] {
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

  rename(id: Id<"model113">, to: string): Result<Model113> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model113 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model113">, status: Model113Status): Result<Model113Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model113">, patch: Patch<Pick<Model113, "name" | "tags">>): Model113 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model113Status, Model113[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model113Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model113">[]): Promise<Model113[]> {
    const found: Model113[] = [];
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

  linkedService(): Model112Service {
    return this.parents ?? new Model112Service();
  }
}

export function makeModel113(id: string, name: string): Model113 {
  return {
    id: id as Id<"model113">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 114, unit: "item" }],
    related: [],
  };
}

export const model113Defaults: FrozenModel113 = makeModel113("default-113", "Default 113");
export const model113Label = summarizeModel113(makeModel113("label", "Label"));
