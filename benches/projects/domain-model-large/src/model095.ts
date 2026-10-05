// Generated domain module 95 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model094Service, summarizeModel094 } from "./model094";
import type { Model094 } from "./model094";
import { Model088Service, summarizeModel088 } from "./model088";
import type { Model088 } from "./model088";

export type Model095Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model095Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model095 extends Entity<"model095"> {
  updatedAt?: number;
  name: string;
  status: Model095Status;
  tags: string[];
  lines: Model095Line[];
  owner?: { name: string; email?: string };
  parent?: Model094;
  related: Model088[];
}

export type Model095Event =
  | { kind: "created"; item: Model095 }
  | { kind: "renamed"; id: Id<"model095">; from: string; to: string }
  | { kind: "moved"; id: Id<"model095">; status: Model095Status }
  | { kind: "deleted"; id: Id<"model095">; reason?: string };

export type Model095Events = {
  change: Model095Event;
  error: { message: string; code: number };
};

export type Model095Numbers = KeysOfType<Model095Line, number>;
export type FrozenModel095 = DeepReadonly<Model095>;

export function describeModel095Event(event: Model095Event): string {
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

export function totalModel095(item: Model095): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel095(item: Model095): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel095(item), 3), (item.parent ? summarizeModel094(item.parent) : "-"), item.related.map(summarizeModel088).join(",")].join(" | ");
}

export class Model095Service extends Service<Model095, "model095"> {
  readonly events = new EventBus<Model095Events>();
  private readonly parents?: Model094Service;

  constructor(repository = new MemoryRepository<Model095, "model095">()) {
    super(repository);
  }

  validate(item: Model095): string[] {
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

  rename(id: Id<"model095">, to: string): Result<Model095> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model095 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model095">, status: Model095Status): Result<Model095Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model095">, patch: Patch<Pick<Model095, "name" | "tags">>): Model095 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model095Status, Model095[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model095Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model095">[]): Promise<Model095[]> {
    const found: Model095[] = [];
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

  linkedService(): Model094Service {
    return this.parents ?? new Model094Service();
  }
}

export function makeModel095(id: string, name: string): Model095 {
  return {
    id: id as Id<"model095">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 96, unit: "kg" }],
    related: [],
  };
}

export const model095Defaults: FrozenModel095 = makeModel095("default-95", "Default 95");
export const model095Label = summarizeModel095(makeModel095("label", "Label"));
