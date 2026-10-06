// Generated domain module 175 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model174Service, summarizeModel174 } from "./model174";
import type { Model174 } from "./model174";
import { Model168Service, summarizeModel168 } from "./model168";
import type { Model168 } from "./model168";

export type Model175Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model175Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model175 extends Entity<"model175"> {
  updatedAt?: number;
  name: string;
  status: Model175Status;
  tags: string[];
  lines: Model175Line[];
  owner?: { name: string; email?: string };
  parent?: Model174;
  related: Model168[];
}

export type Model175Event =
  | { kind: "created"; item: Model175 }
  | { kind: "renamed"; id: Id<"model175">; from: string; to: string }
  | { kind: "moved"; id: Id<"model175">; status: Model175Status }
  | { kind: "deleted"; id: Id<"model175">; reason?: string };

export type Model175Events = {
  change: Model175Event;
  error: { message: string; code: number };
};

export type Model175Numbers = KeysOfType<Model175Line, number>;
export type FrozenModel175 = DeepReadonly<Model175>;

export function describeModel175Event(event: Model175Event): string {
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

export function totalModel175(item: Model175): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel175(item: Model175): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel175(item), 3), (item.parent ? summarizeModel174(item.parent) : "-"), item.related.map(summarizeModel168).join(",")].join(" | ");
}

export class Model175Service extends Service<Model175, "model175"> {
  readonly events = new EventBus<Model175Events>();
  private readonly parents?: Model174Service;

  constructor(repository = new MemoryRepository<Model175, "model175">()) {
    super(repository);
  }

  validate(item: Model175): string[] {
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

  rename(id: Id<"model175">, to: string): Result<Model175> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model175 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model175">, status: Model175Status): Result<Model175Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model175">, patch: Patch<Pick<Model175, "name" | "tags">>): Model175 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model175Status, Model175[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model175Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model175">[]): Promise<Model175[]> {
    const found: Model175[] = [];
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

  linkedService(): Model174Service {
    return this.parents ?? new Model174Service();
  }
}

export function makeModel175(id: string, name: string): Model175 {
  return {
    id: id as Id<"model175">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 176, unit: "kg" }],
    related: [],
  };
}

export const model175Defaults: FrozenModel175 = makeModel175("default-175", "Default 175");
export const model175Label = summarizeModel175(makeModel175("label", "Label"));
