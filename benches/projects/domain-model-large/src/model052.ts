// Generated domain module 52 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model051Service, summarizeModel051 } from "./model051";
import type { Model051 } from "./model051";
import { Model045Service, summarizeModel045 } from "./model045";
import type { Model045 } from "./model045";

export type Model052Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model052Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model052 extends Entity<"model052"> {
  updatedAt?: number;
  name: string;
  status: Model052Status;
  tags: string[];
  lines: Model052Line[];
  owner?: { name: string; email?: string };
  parent?: Model051;
  related: Model045[];
}

export type Model052Event =
  | { kind: "created"; item: Model052 }
  | { kind: "renamed"; id: Id<"model052">; from: string; to: string }
  | { kind: "moved"; id: Id<"model052">; status: Model052Status }
  | { kind: "deleted"; id: Id<"model052">; reason?: string };

export type Model052Events = {
  change: Model052Event;
  error: { message: string; code: number };
};

export type Model052Numbers = KeysOfType<Model052Line, number>;
export type FrozenModel052 = DeepReadonly<Model052>;

export function describeModel052Event(event: Model052Event): string {
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

export function totalModel052(item: Model052): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel052(item: Model052): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel052(item), 3), (item.parent ? summarizeModel051(item.parent) : "-"), item.related.map(summarizeModel045).join(",")].join(" | ");
}

export class Model052Service extends Service<Model052, "model052"> {
  readonly events = new EventBus<Model052Events>();
  private readonly parents?: Model051Service;

  constructor(repository = new MemoryRepository<Model052, "model052">()) {
    super(repository);
  }

  validate(item: Model052): string[] {
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

  rename(id: Id<"model052">, to: string): Result<Model052> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model052 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model052">, status: Model052Status): Result<Model052Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model052">, patch: Patch<Pick<Model052, "name" | "tags">>): Model052 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model052Status, Model052[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model052Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model052">[]): Promise<Model052[]> {
    const found: Model052[] = [];
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

  linkedService(): Model051Service {
    return this.parents ?? new Model051Service();
  }
}

export function makeModel052(id: string, name: string): Model052 {
  return {
    id: id as Id<"model052">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 53, unit: "s" }],
    related: [],
  };
}

export const model052Defaults: FrozenModel052 = makeModel052("default-52", "Default 52");
export const model052Label = summarizeModel052(makeModel052("label", "Label"));
