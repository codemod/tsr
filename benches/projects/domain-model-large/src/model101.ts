// Generated domain module 101 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model100Service, summarizeModel100 } from "./model100";
import type { Model100 } from "./model100";
import { Model094Service, summarizeModel094 } from "./model094";
import type { Model094 } from "./model094";

export type Model101Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model101Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model101 extends Entity<"model101"> {
  updatedAt?: number;
  name: string;
  status: Model101Status;
  tags: string[];
  lines: Model101Line[];
  owner?: { name: string; email?: string };
  parent?: Model100;
  related: Model094[];
}

export type Model101Event =
  | { kind: "created"; item: Model101 }
  | { kind: "renamed"; id: Id<"model101">; from: string; to: string }
  | { kind: "moved"; id: Id<"model101">; status: Model101Status }
  | { kind: "deleted"; id: Id<"model101">; reason?: string };

export type Model101Events = {
  change: Model101Event;
  error: { message: string; code: number };
};

export type Model101Numbers = KeysOfType<Model101Line, number>;
export type FrozenModel101 = DeepReadonly<Model101>;

export function describeModel101Event(event: Model101Event): string {
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

export function totalModel101(item: Model101): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel101(item: Model101): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel101(item), 3), (item.parent ? summarizeModel100(item.parent) : "-"), item.related.map(summarizeModel094).join(",")].join(" | ");
}

export class Model101Service extends Service<Model101, "model101"> {
  readonly events = new EventBus<Model101Events>();
  private readonly parents?: Model100Service;

  constructor(repository = new MemoryRepository<Model101, "model101">()) {
    super(repository);
  }

  validate(item: Model101): string[] {
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

  rename(id: Id<"model101">, to: string): Result<Model101> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model101 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model101">, status: Model101Status): Result<Model101Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model101">, patch: Patch<Pick<Model101, "name" | "tags">>): Model101 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model101Status, Model101[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model101Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model101">[]): Promise<Model101[]> {
    const found: Model101[] = [];
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

  linkedService(): Model100Service {
    return this.parents ?? new Model100Service();
  }
}

export function makeModel101(id: string, name: string): Model101 {
  return {
    id: id as Id<"model101">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 102, unit: "m" }],
    related: [],
  };
}

export const model101Defaults: FrozenModel101 = makeModel101("default-101", "Default 101");
export const model101Label = summarizeModel101(makeModel101("label", "Label"));
