// Generated domain module 33 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model032Service, summarizeModel032 } from "./model032";
import type { Model032 } from "./model032";
import { Model026Service, summarizeModel026 } from "./model026";
import type { Model026 } from "./model026";

export type Model033Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model033Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model033 extends Entity<"model033"> {
  updatedAt?: number;
  name: string;
  status: Model033Status;
  tags: string[];
  lines: Model033Line[];
  owner?: { name: string; email?: string };
  parent?: Model032;
  related: Model026[];
}

export type Model033Event =
  | { kind: "created"; item: Model033 }
  | { kind: "renamed"; id: Id<"model033">; from: string; to: string }
  | { kind: "moved"; id: Id<"model033">; status: Model033Status }
  | { kind: "deleted"; id: Id<"model033">; reason?: string };

export type Model033Events = {
  change: Model033Event;
  error: { message: string; code: number };
};

export type Model033Numbers = KeysOfType<Model033Line, number>;
export type FrozenModel033 = DeepReadonly<Model033>;

export function describeModel033Event(event: Model033Event): string {
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

export function totalModel033(item: Model033): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel033(item: Model033): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel033(item), 3), (item.parent ? summarizeModel032(item.parent) : "-"), item.related.map(summarizeModel026).join(",")].join(" | ");
}

export class Model033Service extends Service<Model033, "model033"> {
  readonly events = new EventBus<Model033Events>();
  private readonly parents?: Model032Service;

  constructor(repository = new MemoryRepository<Model033, "model033">()) {
    super(repository);
  }

  validate(item: Model033): string[] {
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

  rename(id: Id<"model033">, to: string): Result<Model033> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model033 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model033">, status: Model033Status): Result<Model033Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model033">, patch: Patch<Pick<Model033, "name" | "tags">>): Model033 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model033Status, Model033[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model033Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model033">[]): Promise<Model033[]> {
    const found: Model033[] = [];
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

  linkedService(): Model032Service {
    return this.parents ?? new Model032Service();
  }
}

export function makeModel033(id: string, name: string): Model033 {
  return {
    id: id as Id<"model033">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 34, unit: "item" }],
    related: [],
  };
}

export const model033Defaults: FrozenModel033 = makeModel033("default-33", "Default 33");
export const model033Label = summarizeModel033(makeModel033("label", "Label"));
