// Generated domain module 57 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model056Service, summarizeModel056 } from "./model056";
import type { Model056 } from "./model056";
import { Model050Service, summarizeModel050 } from "./model050";
import type { Model050 } from "./model050";

export type Model057Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model057Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model057 extends Entity<"model057"> {
  updatedAt?: number;
  name: string;
  status: Model057Status;
  tags: string[];
  lines: Model057Line[];
  owner?: { name: string; email?: string };
  parent?: Model056;
  related: Model050[];
}

export type Model057Event =
  | { kind: "created"; item: Model057 }
  | { kind: "renamed"; id: Id<"model057">; from: string; to: string }
  | { kind: "moved"; id: Id<"model057">; status: Model057Status }
  | { kind: "deleted"; id: Id<"model057">; reason?: string };

export type Model057Events = {
  change: Model057Event;
  error: { message: string; code: number };
};

export type Model057Numbers = KeysOfType<Model057Line, number>;
export type FrozenModel057 = DeepReadonly<Model057>;

export function describeModel057Event(event: Model057Event): string {
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

export function totalModel057(item: Model057): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel057(item: Model057): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel057(item), 3), (item.parent ? summarizeModel056(item.parent) : "-"), item.related.map(summarizeModel050).join(",")].join(" | ");
}

export class Model057Service extends Service<Model057, "model057"> {
  readonly events = new EventBus<Model057Events>();
  private readonly parents?: Model056Service;

  constructor(repository = new MemoryRepository<Model057, "model057">()) {
    super(repository);
  }

  validate(item: Model057): string[] {
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

  rename(id: Id<"model057">, to: string): Result<Model057> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model057 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model057">, status: Model057Status): Result<Model057Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model057">, patch: Patch<Pick<Model057, "name" | "tags">>): Model057 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model057Status, Model057[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model057Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model057">[]): Promise<Model057[]> {
    const found: Model057[] = [];
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

  linkedService(): Model056Service {
    return this.parents ?? new Model056Service();
  }
}

export function makeModel057(id: string, name: string): Model057 {
  return {
    id: id as Id<"model057">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 58, unit: "s" }],
    related: [],
  };
}

export const model057Defaults: FrozenModel057 = makeModel057("default-57", "Default 57");
export const model057Label = summarizeModel057(makeModel057("label", "Label"));
