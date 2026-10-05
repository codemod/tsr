// Generated domain module 181 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model180Service, summarizeModel180 } from "./model180";
import type { Model180 } from "./model180";
import { Model174Service, summarizeModel174 } from "./model174";
import type { Model174 } from "./model174";

export type Model181Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model181Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model181 extends Entity<"model181"> {
  updatedAt?: number;
  name: string;
  status: Model181Status;
  tags: string[];
  lines: Model181Line[];
  owner?: { name: string; email?: string };
  parent?: Model180;
  related: Model174[];
}

export type Model181Event =
  | { kind: "created"; item: Model181 }
  | { kind: "renamed"; id: Id<"model181">; from: string; to: string }
  | { kind: "moved"; id: Id<"model181">; status: Model181Status }
  | { kind: "deleted"; id: Id<"model181">; reason?: string };

export type Model181Events = {
  change: Model181Event;
  error: { message: string; code: number };
};

export type Model181Numbers = KeysOfType<Model181Line, number>;
export type FrozenModel181 = DeepReadonly<Model181>;

export function describeModel181Event(event: Model181Event): string {
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

export function totalModel181(item: Model181): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel181(item: Model181): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel181(item), 3), (item.parent ? summarizeModel180(item.parent) : "-"), item.related.map(summarizeModel174).join(",")].join(" | ");
}

export class Model181Service extends Service<Model181, "model181"> {
  readonly events = new EventBus<Model181Events>();
  private readonly parents?: Model180Service;

  constructor(repository = new MemoryRepository<Model181, "model181">()) {
    super(repository);
  }

  validate(item: Model181): string[] {
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

  rename(id: Id<"model181">, to: string): Result<Model181> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model181 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model181">, status: Model181Status): Result<Model181Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model181">, patch: Patch<Pick<Model181, "name" | "tags">>): Model181 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model181Status, Model181[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model181Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model181">[]): Promise<Model181[]> {
    const found: Model181[] = [];
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

  linkedService(): Model180Service {
    return this.parents ?? new Model180Service();
  }
}

export function makeModel181(id: string, name: string): Model181 {
  return {
    id: id as Id<"model181">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 182, unit: "m" }],
    related: [],
  };
}

export const model181Defaults: FrozenModel181 = makeModel181("default-181", "Default 181");
export const model181Label = summarizeModel181(makeModel181("label", "Label"));
