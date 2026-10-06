// Generated domain module 161 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model160Service, summarizeModel160 } from "./model160";
import type { Model160 } from "./model160";
import { Model154Service, summarizeModel154 } from "./model154";
import type { Model154 } from "./model154";

export type Model161Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model161Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model161 extends Entity<"model161"> {
  updatedAt?: number;
  name: string;
  status: Model161Status;
  tags: string[];
  lines: Model161Line[];
  owner?: { name: string; email?: string };
  parent?: Model160;
  related: Model154[];
}

export type Model161Event =
  | { kind: "created"; item: Model161 }
  | { kind: "renamed"; id: Id<"model161">; from: string; to: string }
  | { kind: "moved"; id: Id<"model161">; status: Model161Status }
  | { kind: "deleted"; id: Id<"model161">; reason?: string };

export type Model161Events = {
  change: Model161Event;
  error: { message: string; code: number };
};

export type Model161Numbers = KeysOfType<Model161Line, number>;
export type FrozenModel161 = DeepReadonly<Model161>;

export function describeModel161Event(event: Model161Event): string {
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

export function totalModel161(item: Model161): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel161(item: Model161): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel161(item), 3), (item.parent ? summarizeModel160(item.parent) : "-"), item.related.map(summarizeModel154).join(",")].join(" | ");
}

export class Model161Service extends Service<Model161, "model161"> {
  readonly events = new EventBus<Model161Events>();
  private readonly parents?: Model160Service;

  constructor(repository = new MemoryRepository<Model161, "model161">()) {
    super(repository);
  }

  validate(item: Model161): string[] {
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

  rename(id: Id<"model161">, to: string): Result<Model161> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model161 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model161">, status: Model161Status): Result<Model161Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model161">, patch: Patch<Pick<Model161, "name" | "tags">>): Model161 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model161Status, Model161[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model161Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model161">[]): Promise<Model161[]> {
    const found: Model161[] = [];
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

  linkedService(): Model160Service {
    return this.parents ?? new Model160Service();
  }
}

export function makeModel161(id: string, name: string): Model161 {
  return {
    id: id as Id<"model161">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 162, unit: "m" }],
    related: [],
  };
}

export const model161Defaults: FrozenModel161 = makeModel161("default-161", "Default 161");
export const model161Label = summarizeModel161(makeModel161("label", "Label"));
