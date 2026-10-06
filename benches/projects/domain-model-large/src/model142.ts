// Generated domain module 142 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model141Service, summarizeModel141 } from "./model141";
import type { Model141 } from "./model141";
import { Model135Service, summarizeModel135 } from "./model135";
import type { Model135 } from "./model135";

export type Model142Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model142Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model142 extends Entity<"model142"> {
  updatedAt?: number;
  name: string;
  status: Model142Status;
  tags: string[];
  lines: Model142Line[];
  owner?: { name: string; email?: string };
  parent?: Model141;
  related: Model135[];
}

export type Model142Event =
  | { kind: "created"; item: Model142 }
  | { kind: "renamed"; id: Id<"model142">; from: string; to: string }
  | { kind: "moved"; id: Id<"model142">; status: Model142Status }
  | { kind: "deleted"; id: Id<"model142">; reason?: string };

export type Model142Events = {
  change: Model142Event;
  error: { message: string; code: number };
};

export type Model142Numbers = KeysOfType<Model142Line, number>;
export type FrozenModel142 = DeepReadonly<Model142>;

export function describeModel142Event(event: Model142Event): string {
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

export function totalModel142(item: Model142): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel142(item: Model142): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel142(item), 3), (item.parent ? summarizeModel141(item.parent) : "-"), item.related.map(summarizeModel135).join(",")].join(" | ");
}

export class Model142Service extends Service<Model142, "model142"> {
  readonly events = new EventBus<Model142Events>();
  private readonly parents?: Model141Service;

  constructor(repository = new MemoryRepository<Model142, "model142">()) {
    super(repository);
  }

  validate(item: Model142): string[] {
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

  rename(id: Id<"model142">, to: string): Result<Model142> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model142 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model142">, status: Model142Status): Result<Model142Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model142">, patch: Patch<Pick<Model142, "name" | "tags">>): Model142 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model142Status, Model142[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model142Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model142">[]): Promise<Model142[]> {
    const found: Model142[] = [];
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

  linkedService(): Model141Service {
    return this.parents ?? new Model141Service();
  }
}

export function makeModel142(id: string, name: string): Model142 {
  return {
    id: id as Id<"model142">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 143, unit: "s" }],
    related: [],
  };
}

export const model142Defaults: FrozenModel142 = makeModel142("default-142", "Default 142");
export const model142Label = summarizeModel142(makeModel142("label", "Label"));
