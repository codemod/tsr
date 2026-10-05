// Generated domain module 150 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model149Service, summarizeModel149 } from "./model149";
import type { Model149 } from "./model149";
import { Model143Service, summarizeModel143 } from "./model143";
import type { Model143 } from "./model143";

export type Model150Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model150Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model150 extends Entity<"model150"> {
  updatedAt?: number;
  name: string;
  status: Model150Status;
  tags: string[];
  lines: Model150Line[];
  owner?: { name: string; email?: string };
  parent?: Model149;
  related: Model143[];
}

export type Model150Event =
  | { kind: "created"; item: Model150 }
  | { kind: "renamed"; id: Id<"model150">; from: string; to: string }
  | { kind: "moved"; id: Id<"model150">; status: Model150Status }
  | { kind: "deleted"; id: Id<"model150">; reason?: string };

export type Model150Events = {
  change: Model150Event;
  error: { message: string; code: number };
};

export type Model150Numbers = KeysOfType<Model150Line, number>;
export type FrozenModel150 = DeepReadonly<Model150>;

export function describeModel150Event(event: Model150Event): string {
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

export function totalModel150(item: Model150): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel150(item: Model150): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel150(item), 3), (item.parent ? summarizeModel149(item.parent) : "-"), item.related.map(summarizeModel143).join(",")].join(" | ");
}

export class Model150Service extends Service<Model150, "model150"> {
  readonly events = new EventBus<Model150Events>();
  private readonly parents?: Model149Service;

  constructor(repository = new MemoryRepository<Model150, "model150">()) {
    super(repository);
  }

  validate(item: Model150): string[] {
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

  rename(id: Id<"model150">, to: string): Result<Model150> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model150 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model150">, status: Model150Status): Result<Model150Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model150">, patch: Patch<Pick<Model150, "name" | "tags">>): Model150 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model150Status, Model150[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model150Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model150">[]): Promise<Model150[]> {
    const found: Model150[] = [];
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

  linkedService(): Model149Service {
    return this.parents ?? new Model149Service();
  }
}

export function makeModel150(id: string, name: string): Model150 {
  return {
    id: id as Id<"model150">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 151, unit: "kg" }],
    related: [],
  };
}

export const model150Defaults: FrozenModel150 = makeModel150("default-150", "Default 150");
export const model150Label = summarizeModel150(makeModel150("label", "Label"));
