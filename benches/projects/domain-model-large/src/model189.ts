// Generated domain module 189 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model188Service, summarizeModel188 } from "./model188";
import type { Model188 } from "./model188";
import { Model182Service, summarizeModel182 } from "./model182";
import type { Model182 } from "./model182";

export type Model189Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model189Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model189 extends Entity<"model189"> {
  updatedAt?: number;
  name: string;
  status: Model189Status;
  tags: string[];
  lines: Model189Line[];
  owner?: { name: string; email?: string };
  parent?: Model188;
  related: Model182[];
}

export type Model189Event =
  | { kind: "created"; item: Model189 }
  | { kind: "renamed"; id: Id<"model189">; from: string; to: string }
  | { kind: "moved"; id: Id<"model189">; status: Model189Status }
  | { kind: "deleted"; id: Id<"model189">; reason?: string };

export type Model189Events = {
  change: Model189Event;
  error: { message: string; code: number };
};

export type Model189Numbers = KeysOfType<Model189Line, number>;
export type FrozenModel189 = DeepReadonly<Model189>;

export function describeModel189Event(event: Model189Event): string {
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

export function totalModel189(item: Model189): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel189(item: Model189): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel189(item), 3), (item.parent ? summarizeModel188(item.parent) : "-"), item.related.map(summarizeModel182).join(",")].join(" | ");
}

export class Model189Service extends Service<Model189, "model189"> {
  readonly events = new EventBus<Model189Events>();
  private readonly parents?: Model188Service;

  constructor(repository = new MemoryRepository<Model189, "model189">()) {
    super(repository);
  }

  validate(item: Model189): string[] {
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

  rename(id: Id<"model189">, to: string): Result<Model189> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model189 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model189">, status: Model189Status): Result<Model189Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model189">, patch: Patch<Pick<Model189, "name" | "tags">>): Model189 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model189Status, Model189[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model189Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model189">[]): Promise<Model189[]> {
    const found: Model189[] = [];
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

  linkedService(): Model188Service {
    return this.parents ?? new Model188Service();
  }
}

export function makeModel189(id: string, name: string): Model189 {
  return {
    id: id as Id<"model189">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 190, unit: "hour" }],
    related: [],
  };
}

export const model189Defaults: FrozenModel189 = makeModel189("default-189", "Default 189");
export const model189Label = summarizeModel189(makeModel189("label", "Label"));
