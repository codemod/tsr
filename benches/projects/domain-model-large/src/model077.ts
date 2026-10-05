// Generated domain module 77 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model076Service, summarizeModel076 } from "./model076";
import type { Model076 } from "./model076";
import { Model070Service, summarizeModel070 } from "./model070";
import type { Model070 } from "./model070";

export type Model077Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model077Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model077 extends Entity<"model077"> {
  updatedAt?: number;
  name: string;
  status: Model077Status;
  tags: string[];
  lines: Model077Line[];
  owner?: { name: string; email?: string };
  parent?: Model076;
  related: Model070[];
}

export type Model077Event =
  | { kind: "created"; item: Model077 }
  | { kind: "renamed"; id: Id<"model077">; from: string; to: string }
  | { kind: "moved"; id: Id<"model077">; status: Model077Status }
  | { kind: "deleted"; id: Id<"model077">; reason?: string };

export type Model077Events = {
  change: Model077Event;
  error: { message: string; code: number };
};

export type Model077Numbers = KeysOfType<Model077Line, number>;
export type FrozenModel077 = DeepReadonly<Model077>;

export function describeModel077Event(event: Model077Event): string {
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

export function totalModel077(item: Model077): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel077(item: Model077): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel077(item), 3), (item.parent ? summarizeModel076(item.parent) : "-"), item.related.map(summarizeModel070).join(",")].join(" | ");
}

export class Model077Service extends Service<Model077, "model077"> {
  readonly events = new EventBus<Model077Events>();
  private readonly parents?: Model076Service;

  constructor(repository = new MemoryRepository<Model077, "model077">()) {
    super(repository);
  }

  validate(item: Model077): string[] {
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

  rename(id: Id<"model077">, to: string): Result<Model077> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model077 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model077">, status: Model077Status): Result<Model077Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model077">, patch: Patch<Pick<Model077, "name" | "tags">>): Model077 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model077Status, Model077[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model077Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model077">[]): Promise<Model077[]> {
    const found: Model077[] = [];
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

  linkedService(): Model076Service {
    return this.parents ?? new Model076Service();
  }
}

export function makeModel077(id: string, name: string): Model077 {
  return {
    id: id as Id<"model077">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 78, unit: "s" }],
    related: [],
  };
}

export const model077Defaults: FrozenModel077 = makeModel077("default-77", "Default 77");
export const model077Label = summarizeModel077(makeModel077("label", "Label"));
