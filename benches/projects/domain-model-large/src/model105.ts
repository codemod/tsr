// Generated domain module 105 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model104Service, summarizeModel104 } from "./model104";
import type { Model104 } from "./model104";
import { Model098Service, summarizeModel098 } from "./model098";
import type { Model098 } from "./model098";

export type Model105Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model105Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model105 extends Entity<"model105"> {
  updatedAt?: number;
  name: string;
  status: Model105Status;
  tags: string[];
  lines: Model105Line[];
  owner?: { name: string; email?: string };
  parent?: Model104;
  related: Model098[];
}

export type Model105Event =
  | { kind: "created"; item: Model105 }
  | { kind: "renamed"; id: Id<"model105">; from: string; to: string }
  | { kind: "moved"; id: Id<"model105">; status: Model105Status }
  | { kind: "deleted"; id: Id<"model105">; reason?: string };

export type Model105Events = {
  change: Model105Event;
  error: { message: string; code: number };
};

export type Model105Numbers = KeysOfType<Model105Line, number>;
export type FrozenModel105 = DeepReadonly<Model105>;

export function describeModel105Event(event: Model105Event): string {
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

export function totalModel105(item: Model105): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel105(item: Model105): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel105(item), 3), (item.parent ? summarizeModel104(item.parent) : "-"), item.related.map(summarizeModel098).join(",")].join(" | ");
}

export class Model105Service extends Service<Model105, "model105"> {
  readonly events = new EventBus<Model105Events>();
  private readonly parents?: Model104Service;

  constructor(repository = new MemoryRepository<Model105, "model105">()) {
    super(repository);
  }

  validate(item: Model105): string[] {
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

  rename(id: Id<"model105">, to: string): Result<Model105> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model105 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model105">, status: Model105Status): Result<Model105Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model105">, patch: Patch<Pick<Model105, "name" | "tags">>): Model105 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model105Status, Model105[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model105Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model105">[]): Promise<Model105[]> {
    const found: Model105[] = [];
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

  linkedService(): Model104Service {
    return this.parents ?? new Model104Service();
  }
}

export function makeModel105(id: string, name: string): Model105 {
  return {
    id: id as Id<"model105">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 106, unit: "kg" }],
    related: [],
  };
}

export const model105Defaults: FrozenModel105 = makeModel105("default-105", "Default 105");
export const model105Label = summarizeModel105(makeModel105("label", "Label"));
