// Generated domain module 99 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model098Service, summarizeModel098 } from "./model098";
import type { Model098 } from "./model098";
import { Model092Service, summarizeModel092 } from "./model092";
import type { Model092 } from "./model092";

export type Model099Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model099Line {
  readonly sku: string;
  quantity: number;
  unit: "hour" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model099 extends Entity<"model099"> {
  updatedAt?: number;
  name: string;
  status: Model099Status;
  tags: string[];
  lines: Model099Line[];
  owner?: { name: string; email?: string };
  parent?: Model098;
  related: Model092[];
}

export type Model099Event =
  | { kind: "created"; item: Model099 }
  | { kind: "renamed"; id: Id<"model099">; from: string; to: string }
  | { kind: "moved"; id: Id<"model099">; status: Model099Status }
  | { kind: "deleted"; id: Id<"model099">; reason?: string };

export type Model099Events = {
  change: Model099Event;
  error: { message: string; code: number };
};

export type Model099Numbers = KeysOfType<Model099Line, number>;
export type FrozenModel099 = DeepReadonly<Model099>;

export function describeModel099Event(event: Model099Event): string {
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

export function totalModel099(item: Model099): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel099(item: Model099): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel099(item), 3), (item.parent ? summarizeModel098(item.parent) : "-"), item.related.map(summarizeModel092).join(",")].join(" | ");
}

export class Model099Service extends Service<Model099, "model099"> {
  readonly events = new EventBus<Model099Events>();
  private readonly parents?: Model098Service;

  constructor(repository = new MemoryRepository<Model099, "model099">()) {
    super(repository);
  }

  validate(item: Model099): string[] {
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

  rename(id: Id<"model099">, to: string): Result<Model099> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model099 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model099">, status: Model099Status): Result<Model099Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model099">, patch: Patch<Pick<Model099, "name" | "tags">>): Model099 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model099Status, Model099[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model099Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model099">[]): Promise<Model099[]> {
    const found: Model099[] = [];
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

  linkedService(): Model098Service {
    return this.parents ?? new Model098Service();
  }
}

export function makeModel099(id: string, name: string): Model099 {
  return {
    id: id as Id<"model099">,
    createdAt: 0,
    name,
    status: "deleted",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 100, unit: "hour" }],
    related: [],
  };
}

export const model099Defaults: FrozenModel099 = makeModel099("default-99", "Default 99");
export const model099Label = summarizeModel099(makeModel099("label", "Label"));
