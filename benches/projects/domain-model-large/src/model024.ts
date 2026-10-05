// Generated domain module 24 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model023Service, summarizeModel023 } from "./model023";
import type { Model023 } from "./model023";
import { Model017Service, summarizeModel017 } from "./model017";
import type { Model017 } from "./model017";

export type Model024Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model024Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model024 extends Entity<"model024"> {
  updatedAt?: number;
  name: string;
  status: Model024Status;
  tags: string[];
  lines: Model024Line[];
  owner?: { name: string; email?: string };
  parent?: Model023;
  related: Model017[];
}

export type Model024Event =
  | { kind: "created"; item: Model024 }
  | { kind: "renamed"; id: Id<"model024">; from: string; to: string }
  | { kind: "moved"; id: Id<"model024">; status: Model024Status }
  | { kind: "deleted"; id: Id<"model024">; reason?: string };

export type Model024Events = {
  change: Model024Event;
  error: { message: string; code: number };
};

export type Model024Numbers = KeysOfType<Model024Line, number>;
export type FrozenModel024 = DeepReadonly<Model024>;

export function describeModel024Event(event: Model024Event): string {
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

export function totalModel024(item: Model024): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel024(item: Model024): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel024(item), 3), (item.parent ? summarizeModel023(item.parent) : "-"), item.related.map(summarizeModel017).join(",")].join(" | ");
}

export class Model024Service extends Service<Model024, "model024"> {
  readonly events = new EventBus<Model024Events>();
  private readonly parents?: Model023Service;

  constructor(repository = new MemoryRepository<Model024, "model024">()) {
    super(repository);
  }

  validate(item: Model024): string[] {
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

  rename(id: Id<"model024">, to: string): Result<Model024> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model024 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model024">, status: Model024Status): Result<Model024Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model024">, patch: Patch<Pick<Model024, "name" | "tags">>): Model024 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model024Status, Model024[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model024Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model024">[]): Promise<Model024[]> {
    const found: Model024[] = [];
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

  linkedService(): Model023Service {
    return this.parents ?? new Model023Service();
  }
}

export function makeModel024(id: string, name: string): Model024 {
  return {
    id: id as Id<"model024">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 25, unit: "hour" }],
    related: [],
  };
}

export const model024Defaults: FrozenModel024 = makeModel024("default-24", "Default 24");
export const model024Label = summarizeModel024(makeModel024("label", "Label"));
