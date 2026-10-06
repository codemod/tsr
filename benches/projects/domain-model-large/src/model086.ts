// Generated domain module 86 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model085Service, summarizeModel085 } from "./model085";
import type { Model085 } from "./model085";
import { Model079Service, summarizeModel079 } from "./model079";
import type { Model079 } from "./model079";

export type Model086Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model086Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model086 extends Entity<"model086"> {
  updatedAt?: number;
  name: string;
  status: Model086Status;
  tags: string[];
  lines: Model086Line[];
  owner?: { name: string; email?: string };
  parent?: Model085;
  related: Model079[];
}

export type Model086Event =
  | { kind: "created"; item: Model086 }
  | { kind: "renamed"; id: Id<"model086">; from: string; to: string }
  | { kind: "moved"; id: Id<"model086">; status: Model086Status }
  | { kind: "deleted"; id: Id<"model086">; reason?: string };

export type Model086Events = {
  change: Model086Event;
  error: { message: string; code: number };
};

export type Model086Numbers = KeysOfType<Model086Line, number>;
export type FrozenModel086 = DeepReadonly<Model086>;

export function describeModel086Event(event: Model086Event): string {
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

export function totalModel086(item: Model086): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel086(item: Model086): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel086(item), 3), (item.parent ? summarizeModel085(item.parent) : "-"), item.related.map(summarizeModel079).join(",")].join(" | ");
}

export class Model086Service extends Service<Model086, "model086"> {
  readonly events = new EventBus<Model086Events>();
  private readonly parents?: Model085Service;

  constructor(repository = new MemoryRepository<Model086, "model086">()) {
    super(repository);
  }

  validate(item: Model086): string[] {
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

  rename(id: Id<"model086">, to: string): Result<Model086> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model086 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model086">, status: Model086Status): Result<Model086Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model086">, patch: Patch<Pick<Model086, "name" | "tags">>): Model086 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model086Status, Model086[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model086Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model086">[]): Promise<Model086[]> {
    const found: Model086[] = [];
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

  linkedService(): Model085Service {
    return this.parents ?? new Model085Service();
  }
}

export function makeModel086(id: string, name: string): Model086 {
  return {
    id: id as Id<"model086">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 87, unit: "m" }],
    related: [],
  };
}

export const model086Defaults: FrozenModel086 = makeModel086("default-86", "Default 86");
export const model086Label = summarizeModel086(makeModel086("label", "Label"));
