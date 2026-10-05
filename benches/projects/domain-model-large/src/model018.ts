// Generated domain module 18 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model017Service, summarizeModel017 } from "./model017";
import type { Model017 } from "./model017";
import { Model011Service, summarizeModel011 } from "./model011";
import type { Model011 } from "./model011";

export type Model018Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model018Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model018 extends Entity<"model018"> {
  updatedAt?: number;
  name: string;
  status: Model018Status;
  tags: string[];
  lines: Model018Line[];
  owner?: { name: string; email?: string };
  parent?: Model017;
  related: Model011[];
}

export type Model018Event =
  | { kind: "created"; item: Model018 }
  | { kind: "renamed"; id: Id<"model018">; from: string; to: string }
  | { kind: "moved"; id: Id<"model018">; status: Model018Status }
  | { kind: "deleted"; id: Id<"model018">; reason?: string };

export type Model018Events = {
  change: Model018Event;
  error: { message: string; code: number };
};

export type Model018Numbers = KeysOfType<Model018Line, number>;
export type FrozenModel018 = DeepReadonly<Model018>;

export function describeModel018Event(event: Model018Event): string {
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

export function totalModel018(item: Model018): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel018(item: Model018): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel018(item), 3), (item.parent ? summarizeModel017(item.parent) : "-"), item.related.map(summarizeModel011).join(",")].join(" | ");
}

export class Model018Service extends Service<Model018, "model018"> {
  readonly events = new EventBus<Model018Events>();
  private readonly parents?: Model017Service;

  constructor(repository = new MemoryRepository<Model018, "model018">()) {
    super(repository);
  }

  validate(item: Model018): string[] {
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

  rename(id: Id<"model018">, to: string): Result<Model018> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model018 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model018">, status: Model018Status): Result<Model018Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model018">, patch: Patch<Pick<Model018, "name" | "tags">>): Model018 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model018Status, Model018[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model018Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model018">[]): Promise<Model018[]> {
    const found: Model018[] = [];
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

  linkedService(): Model017Service {
    return this.parents ?? new Model017Service();
  }
}

export function makeModel018(id: string, name: string): Model018 {
  return {
    id: id as Id<"model018">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 19, unit: "item" }],
    related: [],
  };
}

export const model018Defaults: FrozenModel018 = makeModel018("default-18", "Default 18");
export const model018Label = summarizeModel018(makeModel018("label", "Label"));
