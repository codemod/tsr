// Generated domain module 147 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model146Service, summarizeModel146 } from "./model146";
import type { Model146 } from "./model146";
import { Model140Service, summarizeModel140 } from "./model140";
import type { Model140 } from "./model140";

export type Model147Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model147Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model147 extends Entity<"model147"> {
  updatedAt?: number;
  name: string;
  status: Model147Status;
  tags: string[];
  lines: Model147Line[];
  owner?: { name: string; email?: string };
  parent?: Model146;
  related: Model140[];
}

export type Model147Event =
  | { kind: "created"; item: Model147 }
  | { kind: "renamed"; id: Id<"model147">; from: string; to: string }
  | { kind: "moved"; id: Id<"model147">; status: Model147Status }
  | { kind: "deleted"; id: Id<"model147">; reason?: string };

export type Model147Events = {
  change: Model147Event;
  error: { message: string; code: number };
};

export type Model147Numbers = KeysOfType<Model147Line, number>;
export type FrozenModel147 = DeepReadonly<Model147>;

export function describeModel147Event(event: Model147Event): string {
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

export function totalModel147(item: Model147): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel147(item: Model147): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel147(item), 3), (item.parent ? summarizeModel146(item.parent) : "-"), item.related.map(summarizeModel140).join(",")].join(" | ");
}

export class Model147Service extends Service<Model147, "model147"> {
  readonly events = new EventBus<Model147Events>();
  private readonly parents?: Model146Service;

  constructor(repository = new MemoryRepository<Model147, "model147">()) {
    super(repository);
  }

  validate(item: Model147): string[] {
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

  rename(id: Id<"model147">, to: string): Result<Model147> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model147 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model147">, status: Model147Status): Result<Model147Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model147">, patch: Patch<Pick<Model147, "name" | "tags">>): Model147 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model147Status, Model147[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model147Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model147">[]): Promise<Model147[]> {
    const found: Model147[] = [];
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

  linkedService(): Model146Service {
    return this.parents ?? new Model146Service();
  }
}

export function makeModel147(id: string, name: string): Model147 {
  return {
    id: id as Id<"model147">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 148, unit: "s" }],
    related: [],
  };
}

export const model147Defaults: FrozenModel147 = makeModel147("default-147", "Default 147");
export const model147Label = summarizeModel147(makeModel147("label", "Label"));
