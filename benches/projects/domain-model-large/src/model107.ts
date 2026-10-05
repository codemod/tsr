// Generated domain module 107 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model106Service, summarizeModel106 } from "./model106";
import type { Model106 } from "./model106";
import { Model100Service, summarizeModel100 } from "./model100";
import type { Model100 } from "./model100";

export type Model107Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model107Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model107 extends Entity<"model107"> {
  updatedAt?: number;
  name: string;
  status: Model107Status;
  tags: string[];
  lines: Model107Line[];
  owner?: { name: string; email?: string };
  parent?: Model106;
  related: Model100[];
}

export type Model107Event =
  | { kind: "created"; item: Model107 }
  | { kind: "renamed"; id: Id<"model107">; from: string; to: string }
  | { kind: "moved"; id: Id<"model107">; status: Model107Status }
  | { kind: "deleted"; id: Id<"model107">; reason?: string };

export type Model107Events = {
  change: Model107Event;
  error: { message: string; code: number };
};

export type Model107Numbers = KeysOfType<Model107Line, number>;
export type FrozenModel107 = DeepReadonly<Model107>;

export function describeModel107Event(event: Model107Event): string {
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

export function totalModel107(item: Model107): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel107(item: Model107): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel107(item), 3), (item.parent ? summarizeModel106(item.parent) : "-"), item.related.map(summarizeModel100).join(",")].join(" | ");
}

export class Model107Service extends Service<Model107, "model107"> {
  readonly events = new EventBus<Model107Events>();
  private readonly parents?: Model106Service;

  constructor(repository = new MemoryRepository<Model107, "model107">()) {
    super(repository);
  }

  validate(item: Model107): string[] {
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

  rename(id: Id<"model107">, to: string): Result<Model107> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model107 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model107">, status: Model107Status): Result<Model107Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model107">, patch: Patch<Pick<Model107, "name" | "tags">>): Model107 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model107Status, Model107[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model107Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model107">[]): Promise<Model107[]> {
    const found: Model107[] = [];
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

  linkedService(): Model106Service {
    return this.parents ?? new Model106Service();
  }
}

export function makeModel107(id: string, name: string): Model107 {
  return {
    id: id as Id<"model107">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 108, unit: "s" }],
    related: [],
  };
}

export const model107Defaults: FrozenModel107 = makeModel107("default-107", "Default 107");
export const model107Label = summarizeModel107(makeModel107("label", "Label"));
