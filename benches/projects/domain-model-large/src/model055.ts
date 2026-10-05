// Generated domain module 55 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model054Service, summarizeModel054 } from "./model054";
import type { Model054 } from "./model054";
import { Model048Service, summarizeModel048 } from "./model048";
import type { Model048 } from "./model048";

export type Model055Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model055Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model055 extends Entity<"model055"> {
  updatedAt?: number;
  name: string;
  status: Model055Status;
  tags: string[];
  lines: Model055Line[];
  owner?: { name: string; email?: string };
  parent?: Model054;
  related: Model048[];
}

export type Model055Event =
  | { kind: "created"; item: Model055 }
  | { kind: "renamed"; id: Id<"model055">; from: string; to: string }
  | { kind: "moved"; id: Id<"model055">; status: Model055Status }
  | { kind: "deleted"; id: Id<"model055">; reason?: string };

export type Model055Events = {
  change: Model055Event;
  error: { message: string; code: number };
};

export type Model055Numbers = KeysOfType<Model055Line, number>;
export type FrozenModel055 = DeepReadonly<Model055>;

export function describeModel055Event(event: Model055Event): string {
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

export function totalModel055(item: Model055): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel055(item: Model055): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel055(item), 3), (item.parent ? summarizeModel054(item.parent) : "-"), item.related.map(summarizeModel048).join(",")].join(" | ");
}

export class Model055Service extends Service<Model055, "model055"> {
  readonly events = new EventBus<Model055Events>();
  private readonly parents?: Model054Service;

  constructor(repository = new MemoryRepository<Model055, "model055">()) {
    super(repository);
  }

  validate(item: Model055): string[] {
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

  rename(id: Id<"model055">, to: string): Result<Model055> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model055 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model055">, status: Model055Status): Result<Model055Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model055">, patch: Patch<Pick<Model055, "name" | "tags">>): Model055 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model055Status, Model055[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model055Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model055">[]): Promise<Model055[]> {
    const found: Model055[] = [];
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

  linkedService(): Model054Service {
    return this.parents ?? new Model054Service();
  }
}

export function makeModel055(id: string, name: string): Model055 {
  return {
    id: id as Id<"model055">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 56, unit: "kg" }],
    related: [],
  };
}

export const model055Defaults: FrozenModel055 = makeModel055("default-55", "Default 55");
export const model055Label = summarizeModel055(makeModel055("label", "Label"));
