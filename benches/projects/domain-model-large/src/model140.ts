// Generated domain module 140 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model139Service, summarizeModel139 } from "./model139";
import type { Model139 } from "./model139";
import { Model133Service, summarizeModel133 } from "./model133";
import type { Model133 } from "./model133";

export type Model140Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model140Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model140 extends Entity<"model140"> {
  updatedAt?: number;
  name: string;
  status: Model140Status;
  tags: string[];
  lines: Model140Line[];
  owner?: { name: string; email?: string };
  parent?: Model139;
  related: Model133[];
}

export type Model140Event =
  | { kind: "created"; item: Model140 }
  | { kind: "renamed"; id: Id<"model140">; from: string; to: string }
  | { kind: "moved"; id: Id<"model140">; status: Model140Status }
  | { kind: "deleted"; id: Id<"model140">; reason?: string };

export type Model140Events = {
  change: Model140Event;
  error: { message: string; code: number };
};

export type Model140Numbers = KeysOfType<Model140Line, number>;
export type FrozenModel140 = DeepReadonly<Model140>;

export function describeModel140Event(event: Model140Event): string {
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

export function totalModel140(item: Model140): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel140(item: Model140): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel140(item), 3), (item.parent ? summarizeModel139(item.parent) : "-"), item.related.map(summarizeModel133).join(",")].join(" | ");
}

export class Model140Service extends Service<Model140, "model140"> {
  readonly events = new EventBus<Model140Events>();
  private readonly parents?: Model139Service;

  constructor(repository = new MemoryRepository<Model140, "model140">()) {
    super(repository);
  }

  validate(item: Model140): string[] {
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

  rename(id: Id<"model140">, to: string): Result<Model140> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model140 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model140">, status: Model140Status): Result<Model140Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model140">, patch: Patch<Pick<Model140, "name" | "tags">>): Model140 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model140Status, Model140[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model140Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model140">[]): Promise<Model140[]> {
    const found: Model140[] = [];
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

  linkedService(): Model139Service {
    return this.parents ?? new Model139Service();
  }
}

export function makeModel140(id: string, name: string): Model140 {
  return {
    id: id as Id<"model140">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 141, unit: "kg" }],
    related: [],
  };
}

export const model140Defaults: FrozenModel140 = makeModel140("default-140", "Default 140");
export const model140Label = summarizeModel140(makeModel140("label", "Label"));
