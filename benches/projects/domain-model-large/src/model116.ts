// Generated domain module 116 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model115Service, summarizeModel115 } from "./model115";
import type { Model115 } from "./model115";
import { Model109Service, summarizeModel109 } from "./model109";
import type { Model109 } from "./model109";

export type Model116Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model116Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model116 extends Entity<"model116"> {
  updatedAt?: number;
  name: string;
  status: Model116Status;
  tags: string[];
  lines: Model116Line[];
  owner?: { name: string; email?: string };
  parent?: Model115;
  related: Model109[];
}

export type Model116Event =
  | { kind: "created"; item: Model116 }
  | { kind: "renamed"; id: Id<"model116">; from: string; to: string }
  | { kind: "moved"; id: Id<"model116">; status: Model116Status }
  | { kind: "deleted"; id: Id<"model116">; reason?: string };

export type Model116Events = {
  change: Model116Event;
  error: { message: string; code: number };
};

export type Model116Numbers = KeysOfType<Model116Line, number>;
export type FrozenModel116 = DeepReadonly<Model116>;

export function describeModel116Event(event: Model116Event): string {
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

export function totalModel116(item: Model116): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel116(item: Model116): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel116(item), 3), (item.parent ? summarizeModel115(item.parent) : "-"), item.related.map(summarizeModel109).join(",")].join(" | ");
}

export class Model116Service extends Service<Model116, "model116"> {
  readonly events = new EventBus<Model116Events>();
  private readonly parents?: Model115Service;

  constructor(repository = new MemoryRepository<Model116, "model116">()) {
    super(repository);
  }

  validate(item: Model116): string[] {
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

  rename(id: Id<"model116">, to: string): Result<Model116> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model116 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model116">, status: Model116Status): Result<Model116Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model116">, patch: Patch<Pick<Model116, "name" | "tags">>): Model116 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model116Status, Model116[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model116Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model116">[]): Promise<Model116[]> {
    const found: Model116[] = [];
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

  linkedService(): Model115Service {
    return this.parents ?? new Model115Service();
  }
}

export function makeModel116(id: string, name: string): Model116 {
  return {
    id: id as Id<"model116">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 117, unit: "m" }],
    related: [],
  };
}

export const model116Defaults: FrozenModel116 = makeModel116("default-116", "Default 116");
export const model116Label = summarizeModel116(makeModel116("label", "Label"));
